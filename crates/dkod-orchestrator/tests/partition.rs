use dkod_orchestrator::callgraph::CallGraph;
use dkod_orchestrator::partition::{partition, Warning};
use dkod_orchestrator::symbols::extract_rust_file;

fn fixture(path: &str) -> (Vec<dk_core::Symbol>, Vec<dk_core::RawCallEdge>) {
    let p = std::path::Path::new(path);
    let src = std::fs::read(p).unwrap();
    extract_rust_file(&src, p).unwrap()
}

#[test]
fn basic_four_functions_split_into_four_singleton_groups() {
    let (syms, calls) = fixture("tests/fixtures/basic/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    let in_scope: Vec<String> = ["alpha", "beta", "gamma", "delta"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let p = partition(&in_scope, &g, 4).unwrap();
    assert_eq!(
        p.groups.len(),
        4,
        "expected 4 disjoint singleton groups, got {}",
        p.groups.len()
    );

    let all: Vec<_> = p
        .groups
        .iter()
        .flat_map(|g| g.symbols.iter().map(|s| s.qualified_name.clone()))
        .collect();
    for name in &in_scope {
        assert!(
            all.iter().any(|n| n.contains(name)),
            "{name} missing from partition"
        );
    }
}

#[test]
fn trait_coupling_puts_coupled_symbols_into_one_group() {
    let (syms, calls) = fixture("tests/fixtures/trait_coupling/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    // say_english → greet, say_french → greet (dk-engine resolves method calls
    // by short name).  The three call-connected symbols must land in the same
    // group regardless of how many isolated struct/trait/impl symbols exist.
    let in_scope: Vec<String> = g.all_symbols().map(|s| s.qualified_name.clone()).collect();

    let p = partition(&in_scope, &g, 4).unwrap();

    // Find the group containing say_english and say_french.
    let coupled_group = p.groups.iter().find(|grp| {
        grp.symbols
            .iter()
            .any(|s| s.qualified_name.contains("say_english"))
    });
    assert!(
        coupled_group.is_some(),
        "say_english must appear in some group"
    );
    let coupled_group = coupled_group.unwrap();
    assert!(
        coupled_group
            .symbols
            .iter()
            .any(|s| s.qualified_name.contains("say_french")),
        "say_english and say_french must share a group (both call greet); groups: {:?}",
        p.groups
            .iter()
            .map(|g| (g.id.as_str(), g.symbols.iter().map(|s| s.qualified_name.as_str()).collect::<Vec<_>>()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn fewer_ccs_than_target_emits_warning() {
    let (syms, calls) = fixture("tests/fixtures/basic/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    let in_scope: Vec<String> = ["alpha", "beta"].iter().map(|s| s.to_string()).collect();

    let p = partition(&in_scope, &g, 4).unwrap();
    assert_eq!(p.groups.len(), 2);
    assert!(
        p.warnings
            .iter()
            .any(|w| matches!(w, Warning::FewerGroupsThanTarget { target: 4, got: 2 }))
    );
}

// --- Edge-case tests (hardening defaults) ---

#[test]
fn empty_in_scope_returns_zero_groups() {
    let (syms, calls) = fixture("tests/fixtures/basic/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    let p = partition(&[], &g, 4).unwrap();
    assert_eq!(p.groups.len(), 0, "empty in_scope must yield zero groups");
}

#[test]
fn unknown_scope_name_emits_warning() {
    let (syms, calls) = fixture("tests/fixtures/basic/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    let in_scope = vec!["alpha".to_string(), "doesnt_exist".to_string()];
    let p = partition(&in_scope, &g, 1).unwrap();
    assert!(
        p.warnings
            .iter()
            .any(|w| matches!(w, Warning::ScopeSymbolUnknown { name } if name == "doesnt_exist")),
        "expected ScopeSymbolUnknown warning"
    );
}

#[test]
fn singleton_isolated_symbols_each_get_own_group() {
    let (syms, calls) = fixture("tests/fixtures/basic/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    // alpha, beta, gamma are not connected; each must be its own group
    let in_scope: Vec<String> = ["alpha", "beta", "gamma"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let p = partition(&in_scope, &g, 3).unwrap();
    assert_eq!(p.groups.len(), 3, "three singletons = three groups");
}
