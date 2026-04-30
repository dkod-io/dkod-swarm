//! Tests for outer-attribute and doc-comment span handling in `replace_symbol`.
//!
//! Without prefix expansion, agents that emit a function with its leading
//! `///` docs and `#[...]` attributes get those duplicated, because the raw
//! engine span starts at the `fn` keyword. These tests pin the contract:
//! the splice covers the whole outer-prefix region (stacked outer
//! doc-comments + outer attributes immediately above the symbol), so the
//! caller's `new_body` may safely include them and end up appearing exactly
//! once in the resulting source.

use dkod_orchestrator::replace::{ReplaceOutcome, replace_symbol};

fn parsed_ok(outcome: ReplaceOutcome) -> String {
    match outcome {
        ReplaceOutcome::ParsedOk { new_source } => String::from_utf8(new_source).unwrap(),
        ReplaceOutcome::Fallback { reason, .. } => panic!("expected ParsedOk; fallback: {reason}"),
    }
}

#[test]
fn doc_comment_above_function_is_not_duplicated() {
    let src = b"\
/// old docs
pub fn hello() -> i32 { 1 }
";
    let new_body = "\
/// new docs
pub fn hello() -> i32 { 2 }";
    let s = parsed_ok(replace_symbol(src, "hello", new_body).unwrap());
    assert_eq!(
        s.matches("/// old docs").count(),
        0,
        "old doc not removed: {s}"
    );
    assert_eq!(
        s.matches("/// new docs").count(),
        1,
        "new doc duplicated or missing: {s}"
    );
    assert!(s.contains("{ 2 }"), "new body missing: {s}");
}

#[test]
fn outer_attribute_above_function_is_not_duplicated() {
    let src = b"\
#[test]
fn t() { assert_eq!(1, 1); }
";
    let new_body = "\
#[test]
fn t() { assert_eq!(2, 2); }";
    let s = parsed_ok(replace_symbol(src, "t", new_body).unwrap());
    assert_eq!(
        s.matches("#[test]").count(),
        1,
        "#[test] duplicated or missing: {s}"
    );
    assert!(s.contains("assert_eq!(2, 2)"), "new body missing: {s}");
    assert!(!s.contains("assert_eq!(1, 1)"), "old body not removed: {s}");
}

#[test]
fn doc_and_attr_stacked_each_appear_once() {
    let src = b"\
/// docs
#[test]
fn t() { assert!(false); }
";
    let new_body = "\
/// docs
#[test]
fn t() { assert!(true); }";
    let s = parsed_ok(replace_symbol(src, "t", new_body).unwrap());
    assert_eq!(s.matches("/// docs").count(), 1, "doc duplicated: {s}");
    assert_eq!(s.matches("#[test]").count(), 1, "attr duplicated: {s}");
    assert!(s.contains("assert!(true)"));
    assert!(!s.contains("assert!(false)"));
}

#[test]
fn multiple_attributes_each_appear_once() {
    let src = b"\
#[test]
#[ignore]
fn t() { assert!(false); }
";
    let new_body = "\
#[test]
#[ignore]
fn t() { assert!(true); }";
    let s = parsed_ok(replace_symbol(src, "t", new_body).unwrap());
    assert_eq!(
        s.matches("#[test]").count(),
        1,
        "#[test] duplicated: {s}"
    );
    assert_eq!(
        s.matches("#[ignore]").count(),
        1,
        "#[ignore] duplicated: {s}"
    );
}

#[test]
fn no_prefix_keeps_existing_behavior() {
    let src = b"pub fn hello() -> i32 { 1 }\n";
    let s = parsed_ok(replace_symbol(src, "hello", "pub fn hello() -> i32 { 2 }").unwrap());
    assert_eq!(s.matches("pub fn hello").count(), 1);
    assert!(s.contains("{ 2 }"));
    assert!(!s.contains("{ 1 }"));
}

#[test]
fn inner_doc_comment_is_not_swallowed() {
    // `//!` is an INNER doc-comment for the enclosing module; it must NOT
    // be consumed when expanding the prefix of a function below it.
    let src = b"\
//! crate-level doc
pub fn hello() -> i32 { 1 }
";
    let new_body = "pub fn hello() -> i32 { 2 }";
    let s = parsed_ok(replace_symbol(src, "hello", new_body).unwrap());
    assert!(
        s.contains("//! crate-level doc"),
        "inner doc was wrongly swallowed: {s}"
    );
    assert!(s.contains("{ 2 }"));
}

#[test]
fn regular_line_comment_is_not_swallowed() {
    // `//` (single-slash) is a regular comment, not a doc-comment, and
    // must not be consumed as part of the symbol's outer prefix.
    let src = b"\
// random note, not a doc-comment
pub fn hello() -> i32 { 1 }
";
    let new_body = "pub fn hello() -> i32 { 2 }";
    let s = parsed_ok(replace_symbol(src, "hello", new_body).unwrap());
    assert!(
        s.contains("// random note"),
        "regular comment was wrongly swallowed: {s}"
    );
    assert!(s.contains("{ 2 }"));
}

#[test]
fn four_slash_comment_is_not_swallowed() {
    // Four (or more) slashes is a regular comment by Rust grammar — it is
    // NOT an outer doc-comment and must not be consumed as part of the
    // outer prefix. Pins the explicit `////`-exclusion rule in the helper.
    let src = b"\
//// banner comment, not a doc-comment
pub fn hello() -> i32 { 1 }
";
    let new_body = "pub fn hello() -> i32 { 2 }";
    let s = parsed_ok(replace_symbol(src, "hello", new_body).unwrap());
    assert!(
        s.contains("//// banner comment"),
        "four-slash comment was wrongly swallowed: {s}"
    );
    assert!(s.contains("{ 2 }"));
}

#[test]
fn indented_outer_attr_inside_mod_is_not_duplicated() {
    // The bench fixture surfaced this: test functions live inside
    // `mod tests` and are indented. The engine's `start_byte` for the
    // function points at `fn`, which is NOT column 0 — the bytes
    // immediately before are the indentation spaces. Earlier versions
    // of `expand_outer_prefix_span` required `source[start - 1] == '\n'`,
    // which is false for indented symbols, so the helper bailed and the
    // splice missed the `#[test]` line above. Pin the contract: leading
    // whitespace before `start_byte` is treated as the symbol's own
    // indentation, the line above (its `#[test]`) is correctly swallowed,
    // and the splice replaces both atomically.
    let src = b"\
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() { assert!(false); }
}
";
    // Note: avoid the `"\\\n    "` escape pattern — Rust eats the newline AND
    // the leading whitespace, which would defeat the test by stripping the
    // intended indentation from `new_body`. Use explicit `\n` instead.
    let new_body = "    #[test]\n    fn t() { assert!(true); }";
    let s = parsed_ok(replace_symbol(src, "t", new_body).unwrap());
    // Function's own #[test] must appear exactly once and at the expected
    // column 4. The mod-level `#[cfg(test)]` is unrelated and contains
    // `(test)`, not `[test]`, so it does not match the substring.
    assert_eq!(
        s.matches("    #[test]").count(),
        1,
        "indented #[test] duplicated or mis-indented: {s}"
    );
    assert_eq!(
        s.matches("#[test]").count(),
        1,
        "got more than one #[test] line in: {s}"
    );
    assert!(s.contains("assert!(true)"));
    assert!(!s.contains("assert!(false)"));
}

#[test]
fn indented_doc_and_attr_stacked_each_appear_once() {
    // Same as `doc_and_attr_stacked_each_appear_once` but the symbol is
    // nested inside a `mod` and therefore indented.
    let src = b"\
mod inner {
    /// docs
    #[test]
    fn t() { assert!(false); }
}
";
    let new_body = "    /// docs\n    #[test]\n    fn t() { assert!(true); }";
    let s = parsed_ok(replace_symbol(src, "t", new_body).unwrap());
    assert_eq!(
        s.matches("    /// docs").count(),
        1,
        "indented doc duplicated or mis-indented: {s}"
    );
    assert_eq!(
        s.matches("    #[test]").count(),
        1,
        "indented #[test] duplicated or mis-indented: {s}"
    );
    assert!(s.contains("assert!(true)"));
    assert!(!s.contains("assert!(false)"));
}

#[test]
fn previous_item_separator_blank_not_consumed() {
    // A blank line between the previous item and the target's own prefix
    // is separator whitespace — the algorithm must walk back through it
    // but forward-trim leading blanks so the previous item is not touched.
    let src = b"\
pub fn earlier() -> i32 { 0 }

/// docs for hello
pub fn hello() -> i32 { 1 }
";
    let new_body = "\
/// docs for hello
pub fn hello() -> i32 { 2 }";
    let s = parsed_ok(replace_symbol(src, "hello", new_body).unwrap());
    assert!(
        s.contains("pub fn earlier() -> i32 { 0 }"),
        "earlier item was wrongly modified: {s}"
    );
    assert_eq!(s.matches("/// docs for hello").count(), 1);
    assert!(s.contains("{ 2 }"));
    assert!(!s.contains("{ 1 }"));
}
