//! AST symbol replace primitive.
//!
//! `replace_symbol` splices `new_body_source` over the byte span of the named
//! symbol in `current_source`, then re-parses the result.
//!
//! # Outer-prefix span expansion
//!
//! `dk-core::Symbol.span` covers the symbol *body* — for a Rust function,
//! that is the span of `pub fn foo() { … }`. It does **not** include the
//! leading `///` doc-comments and `#[…]` outer attributes that attach to
//! the symbol. Callers (especially LLM-driven subagents) naturally emit
//! a `new_body_source` that *includes* those outer-prefix lines, since
//! that is what the symbol "looks like" in the file. To make the API
//! match that mental model, `replace_symbol` walks the bytes immediately
//! preceding `start_byte` line-by-line and swallows:
//!
//! 1. blank lines,
//! 2. outer doc-comment lines starting with `///` (but not `////`),
//! 3. single-line outer attributes whose line trimmed starts with `#[`
//!    and ends with `]`,
//!
//! stopping at any other content (regular code, `//`, inner-doc `//!`,
//! block doc-comments, multi-line `#[…]` attributes). The leading run
//! of blank lines that does *not* precede a real prefix line is then
//! forward-trimmed so a separator blank between the previous item and
//! the target's prefix is preserved. v1 limitation: multi-line `#[…]`
//! attributes and `/** … */` block doc-comments fall through into the
//! existing splice — the engine span is used unchanged in those cases
//! and the duplicate-prefix risk remains; both are tracked as
//! follow-ups.
//!
//! # ParsedOk vs Fallback decision
//!
//! After the splice we re-parse the new source.  We declare `ParsedOk` only
//! when **both** conditions hold:
//!
//! 1. The re-parse returns at least one symbol (syntactically valid Rust).
//! 2. The **same `qualified_name`** (or short `name`) that was replaced is
//!    still present in the re-parsed symbol list.
//!
//! Condition (2) guards against tree-sitter silently tolerating broken input
//! and returning some unrelated symbols: the caller receives `Fallback` with
//! the spliced bytes so it can record an `UnsupportedConstruct` warning
//! (design §edge case #5).  See Task 23 / hardening note in the plan.
use crate::symbols::extract_rust_file;
use crate::{Error, Result};
use std::path::PathBuf;

/// Outcome of [`replace_symbol`].
#[derive(Debug)]
pub enum ReplaceOutcome {
    /// The splice succeeded and a follow-up parse confirmed the symbol is
    /// still present in the new source.
    ParsedOk { new_source: Vec<u8> },
    /// The splice was applied but the follow-up parse did not verify (either
    /// re-parse failed, yielded no symbols, or the replaced symbol disappeared).
    /// Caller should record an `UnsupportedConstruct` warning (design §edge
    /// case #5).
    Fallback { new_source: Vec<u8>, reason: String },
}

/// Replace a symbol's source span with `new_body_source`.
///
/// Resolution is two-tiered:
///
/// 1. Try an exact match on `Symbol.qualified_name` (what most callers pass
///    after consulting the partitioner).
/// 2. Otherwise, try to match on the short `Symbol.name`. If **exactly one**
///    symbol has that short name, use it. If more than one does — e.g.
///    `greet` appears on both `impl Greeter for English` and
///    `impl Greeter for French` — return [`Error::InvalidPartition`] with
///    the candidate qualified_names so the caller can disambiguate.
///
/// # Errors
///
/// - [`Error::SymbolNotFound`] – `qualified_name` not found in `current_source`
///   (includes the empty-name and empty-source cases).
/// - [`Error::InvalidPartition`] – the short name is ambiguous; caller must
///   pass one of the listed qualified_names instead.
/// - [`Error::ReplaceFailed`] – the stored span is out of bounds (should not
///   happen with well-formed engine output, but checked defensively).
/// - [`Error::Engine`] – the initial parse of `current_source` fails.
pub fn replace_symbol(
    current_source: &[u8],
    qualified_name: &str,
    new_body_source: &str,
) -> Result<ReplaceOutcome> {
    let path = PathBuf::from("<in-memory>");

    // extract_rust_file returns ([], []) for empty source — we surface that
    // as SymbolNotFound below, which is the correct contract.
    let (syms, _calls) = extract_rust_file(current_source, &path)?;

    // Tier 1: exact qualified_name match.
    let target = if let Some(s) = syms.iter().find(|s| s.qualified_name == qualified_name) {
        s
    } else {
        // Tier 2: unique short-name match.
        let mut short_matches = syms.iter().filter(|s| s.name == qualified_name);
        let Some(first) = short_matches.next() else {
            return Err(Error::SymbolNotFound {
                name: qualified_name.to_owned(),
                file: path.clone(),
            });
        };
        if short_matches.next().is_some() {
            let candidates: Vec<String> = syms
                .iter()
                .filter(|s| s.name == qualified_name)
                .map(|s| s.qualified_name.clone())
                .collect();
            return Err(Error::InvalidPartition(format!(
                "ambiguous short name {:?}; pass one of {candidates:?} as qualified_name",
                qualified_name
            )));
        }
        first
    };

    let raw_start = target.span.start_byte as usize;
    let end = target.span.end_byte as usize;
    if raw_start > end || end > current_source.len() {
        return Err(Error::ReplaceFailed(format!(
            "span out of bounds for symbol '{}': start={raw_start} end={end} source_len={}",
            qualified_name,
            current_source.len()
        )));
    }
    // Walk backward over outer doc-comments and single-line outer attributes
    // so the splice covers the symbol's full outer prefix (see module docs).
    let start = expand_outer_prefix_span(current_source, raw_start);

    // Splice: prefix + new body + suffix.
    let mut new_source = Vec::with_capacity(current_source.len() + new_body_source.len());
    new_source.extend_from_slice(&current_source[..start]);
    new_source.extend_from_slice(new_body_source.as_bytes());
    new_source.extend_from_slice(&current_source[end..]);

    // Re-parse and verify: ParsedOk requires (1) non-empty symbols AND (2)
    // the same qualified_name is present.  This prevents a mis-fire when
    // tree-sitter tolerates broken input and returns unrelated symbols.
    match extract_rust_file(&new_source, &path) {
        Ok(ref parsed) if !parsed.0.is_empty() => {
            let name_present = parsed
                .0
                .iter()
                .any(|s| s.qualified_name == qualified_name || s.name == qualified_name);
            if name_present {
                Ok(ReplaceOutcome::ParsedOk { new_source })
            } else {
                Ok(ReplaceOutcome::Fallback {
                    new_source,
                    reason: format!(
                        "re-parse succeeded but symbol '{}' not found in result",
                        qualified_name
                    ),
                })
            }
        }
        Ok(_) => Ok(ReplaceOutcome::Fallback {
            new_source,
            reason: "re-parse yielded no symbols".to_owned(),
        }),
        Err(e) => Ok(ReplaceOutcome::Fallback {
            new_source,
            reason: format!("re-parse failed: {e}"),
        }),
    }
}

/// Expand `symbol_start` backward over the symbol's outer-prefix lines.
///
/// Returns the byte offset where the splice should begin so that the
/// caller's `new_body_source` (which typically includes the leading
/// `///` doc-comments and `#[…]` attributes) replaces those lines in
/// place rather than duplicating them.
///
/// See the module-level docs for the full set of rules and v1 limits.
/// Mid-line symbol starts (rare in idiomatic Rust source) are returned
/// unchanged — expansion would otherwise walk into a sibling symbol on
/// the same physical line.
fn expand_outer_prefix_span(source: &[u8], symbol_start: usize) -> usize {
    if symbol_start > source.len() {
        return symbol_start;
    }
    // Only expand when the symbol begins at a line boundary. Mid-line
    // starts are left alone (see fn-doc).
    let at_line_start = symbol_start == 0 || source[symbol_start - 1] == b'\n';
    if !at_line_start {
        return symbol_start;
    }

    let mut cursor = symbol_start;

    // Walk backward. After each iteration `cursor` is at the start of
    // either an outer-prefix line (which we have just absorbed) or the
    // first byte we refused to absorb.
    while cursor > 0 {
        // `cursor` is at a line-start, so `source[cursor - 1]` is `\n`.
        // The line above is `[prev_line_start, cursor - 1)`.
        let prev_newline = cursor - 1;
        let prev_line_start = source[..prev_newline]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);

        let line = &source[prev_line_start..prev_newline];
        let trimmed_start = line.trim_ascii_start();

        let is_blank = trimmed_start.is_empty();
        // `///` is an outer doc-comment; `////` (four or more slashes) is a
        // regular line comment by Rust grammar.
        let is_outer_doc = trimmed_start.starts_with(b"///")
            && (trimmed_start.len() < 4 || trimmed_start[3] != b'/');
        // Single-line outer attribute: line trimmed must start with `#[`
        // AND end with `]`. Multi-line `#[…]` falls through; the v1 limit
        // is documented in the module header.
        let is_outer_attr =
            trimmed_start.starts_with(b"#[") && line.trim_ascii_end().ends_with(b"]");

        if is_blank || is_outer_doc || is_outer_attr {
            cursor = prev_line_start;
        } else {
            break;
        }
    }

    // Forward-trim the leading blank lines we may have walked through.
    // These are separator whitespace between the previous item and our
    // own prefix; consuming them would either shift the splice's leading
    // newline into the previous item's territory or (worse) force the
    // caller's `new_body` to reproduce the gap. Stop at the first
    // non-blank line — that line is the real start of the outer prefix.
    while cursor < symbol_start {
        let line_end = source[cursor..symbol_start]
            .iter()
            .position(|&b| b == b'\n')
            .map(|offset| cursor + offset)
            .unwrap_or(symbol_start);
        let line = &source[cursor..line_end];
        if line.trim_ascii_start().is_empty() {
            // Advance past this blank line and its terminating newline.
            cursor = (line_end + 1).min(symbol_start);
        } else {
            break;
        }
    }

    cursor
}
