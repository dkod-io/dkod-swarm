use dkod_orchestrator::symbols::extract_rust_file;

#[test]
fn extracts_function_symbols_from_inline_source() {
    let src = b"pub fn login() {}\npub fn logout() {}\n";
    let (syms, _calls) = extract_rust_file(src, std::path::Path::new("auth.rs")).unwrap();

    let names: Vec<_> = syms.iter().map(|s| s.qualified_name.clone()).collect();
    assert!(names.iter().any(|n| n.contains("login")), "missing login: {names:?}");
    assert!(names.iter().any(|n| n.contains("logout")), "missing logout: {names:?}");
}

#[test]
fn extracts_calls_between_functions() {
    let src = br#"
pub fn login() { validate(); }
pub fn validate() -> bool { true }
"#;
    let (_syms, calls) = extract_rust_file(src, std::path::Path::new("auth.rs")).unwrap();

    let found = calls.iter().any(|c| c.caller_name.contains("login") && c.callee_name.contains("validate"));
    assert!(found, "expected login -> validate edge; got {calls:?}");
}
