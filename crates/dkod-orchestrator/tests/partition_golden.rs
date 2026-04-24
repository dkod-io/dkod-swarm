use dkod_orchestrator::callgraph::CallGraph;
use dkod_orchestrator::partition::{Partition, partition};
use dkod_orchestrator::symbols::extract_rust_file;

fn load_fixture(path: &str) -> (Vec<dk_core::Symbol>, Vec<dk_core::RawCallEdge>) {
    let p = std::path::Path::new(path);
    extract_rust_file(&std::fs::read(p).unwrap(), p).unwrap()
}

fn canonical_json(p: &Partition) -> String {
    let mut p = p.clone();
    p.groups.sort_by(|a, b| a.id.cmp(&b.id));
    for g in &mut p.groups {
        g.symbols
            .sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
    }
    serde_json::to_string_pretty(&p).unwrap()
}

fn assert_golden(actual: &str, golden_path: &str) {
    let path = std::path::Path::new(golden_path);
    if std::env::var_os("UPDATE").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, format!("{}\n", actual.trim_end())).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path).unwrap_or_else(|_| {
        panic!(
            "golden not found: {}. Run with UPDATE=1 to create.",
            path.display()
        )
    });
    assert_eq!(
        actual.trim(),
        expected.trim(),
        "golden mismatch: {}",
        path.display()
    );
}

#[test]
fn golden_basic_all_four() {
    let (syms, calls) = load_fixture("tests/fixtures/basic/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    let in_scope: Vec<String> = ["alpha", "beta", "gamma", "delta"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let p = partition(&in_scope, &g, 4).unwrap();
    assert_golden(
        &canonical_json(&p),
        "tests/fixtures/golden/basic_all_four.json",
    );
}

#[test]
fn golden_trait_coupling_full() {
    let (syms, calls) = load_fixture("tests/fixtures/trait_coupling/src/lib.rs");
    let g = CallGraph::build(&syms, &calls);
    let in_scope: Vec<String> = g.all_symbols().map(|s| s.qualified_name.clone()).collect();
    let p = partition(&in_scope, &g, 4).unwrap();
    assert_golden(
        &canonical_json(&p),
        "tests/fixtures/golden/trait_coupling_full.json",
    );
}
