use dkod_orchestrator::callgraph::CallGraph;
use dkod_orchestrator::symbols::extract_rust_file;

#[test]
fn graph_resolves_intra_file_edges() {
    let src = br#"
pub fn login() { validate(); }
pub fn validate() -> bool { true }
"#;
    let (syms, calls) = extract_rust_file(src, std::path::Path::new("auth.rs")).unwrap();
    let g = CallGraph::build(&syms, &calls);

    let login_id = g.symbol_id_by_name("login").expect("login present");
    let validate_id = g.symbol_id_by_name("validate").expect("validate present");

    let succ = g.successors(login_id);
    assert!(succ.contains(&validate_id), "login should call validate");
}

#[test]
fn unresolved_edges_are_surfaced_not_panicking() {
    let src = b"pub fn boom() { external_thing(); }\n";
    let (syms, calls) = extract_rust_file(src, std::path::Path::new("x.rs")).unwrap();
    let g = CallGraph::build(&syms, &calls);
    // external_thing isn't in syms — it lands in `unresolved`, not a panic
    // and not in successors().
    assert!(g.unresolved_count() >= 1, "expected at least one unresolved edge");
    let id = g.symbol_id_by_name("boom").unwrap();
    assert!(g.successors(id).is_empty());
}

#[test]
fn self_call_does_not_create_edge() {
    let src = br#"pub fn recurse(n: i32) -> i32 { if n == 0 { 0 } else { recurse(n - 1) } }"#;
    let (syms, calls) = extract_rust_file(src, std::path::Path::new("r.rs")).unwrap();
    let g = CallGraph::build(&syms, &calls);
    let id = g.symbol_id_by_name("recurse").unwrap();
    // Self-loop must not appear in successors
    assert!(!g.successors(id).contains(&id), "self-loop should be filtered");
}

#[test]
fn empty_symbols_and_edges_returns_empty_graph() {
    let g = CallGraph::build(&[], &[]);
    assert_eq!(g.unresolved_count(), 0);
}
