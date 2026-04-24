use dkod_orchestrator::replace::{replace_symbol, ReplaceOutcome};

#[test]
fn replaces_existing_function_body_cleanly() {
    let src = b"pub fn hello() -> &'static str { \"hi\" }\n";
    let outcome =
        replace_symbol(src, "hello", "pub fn hello() -> &'static str { \"HELLO\" }").unwrap();
    match outcome {
        ReplaceOutcome::ParsedOk { new_source } => {
            let s = String::from_utf8(new_source).unwrap();
            assert!(s.contains("HELLO"));
            assert!(!s.contains("\"hi\""));
        }
        ReplaceOutcome::Fallback { .. } => panic!("expected ParsedOk"),
    }
}

#[test]
fn missing_symbol_errors() {
    let src = b"pub fn hello() {}\n";
    let err = replace_symbol(src, "nope", "pub fn nope() {}").unwrap_err();
    assert!(format!("{err}").contains("not found"), "got: {err}");
}

#[test]
fn replaces_one_of_many_preserving_others() {
    let src = br#"pub fn a() -> i32 { 1 }
pub fn b() -> i32 { 2 }
pub fn c() -> i32 { 3 }
"#;
    let outcome = replace_symbol(src, "b", "pub fn b() -> i32 { 20 }").unwrap();
    let s = match outcome {
        ReplaceOutcome::ParsedOk { new_source } => String::from_utf8(new_source).unwrap(),
        ReplaceOutcome::Fallback { .. } => panic!("expected ParsedOk"),
    };
    assert!(s.contains("pub fn a() -> i32 { 1 }"));
    assert!(s.contains("pub fn b() -> i32 { 20 }"));
    assert!(s.contains("pub fn c() -> i32 { 3 }"));
    assert!(!s.contains("pub fn b() -> i32 { 2 }"));
}

#[test]
fn empty_qualified_name_errors() {
    let src = b"pub fn hello() {}\n";
    let err = replace_symbol(src, "", "pub fn hello() {}").unwrap_err();
    assert!(format!("{err}").contains("not found"), "got: {err}");
}

#[test]
fn empty_source_errors_symbol_not_found() {
    let err = replace_symbol(b"", "hello", "pub fn hello() {}").unwrap_err();
    assert!(format!("{err}").contains("not found"), "got: {err}");
}

#[test]
fn syntactically_invalid_replacement_yields_fallback() {
    let src = b"pub fn hello() -> i32 { 1 }\n";
    // Intentionally broken — unmatched brace.
    let outcome = replace_symbol(src, "hello", "pub fn hello() -> i32 { ").unwrap();
    match outcome {
        ReplaceOutcome::Fallback { new_source, reason } => {
            let s = String::from_utf8(new_source).unwrap();
            assert!(s.contains("pub fn hello() -> i32 { "));
            assert!(!reason.is_empty());
        }
        ReplaceOutcome::ParsedOk { .. } => {
            panic!("broken replacement must not be reported as ParsedOk")
        }
    }
}
