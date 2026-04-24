//! AST symbol replace primitive.
//!
//! `replace_symbol` splices `new_body_source` over the byte span of the named
//! symbol in `current_source`, then re-parses the result.
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

    let start = target.span.start_byte as usize;
    let end = target.span.end_byte as usize;
    if start > end || end > current_source.len() {
        return Err(Error::ReplaceFailed(format!(
            "span out of bounds for symbol '{}': start={start} end={end} source_len={}",
            qualified_name,
            current_source.len()
        )));
    }

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
