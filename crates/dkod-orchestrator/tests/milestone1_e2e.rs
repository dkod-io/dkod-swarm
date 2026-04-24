//! End-to-end happy path for milestone 1:
//! init → plan → simulate two agents landing symbol writes → commit_per_group →
//! assert git log has the expected shape.

use dkod_orchestrator::callgraph::CallGraph;
use dkod_orchestrator::commit::commit_per_group;
use dkod_orchestrator::partition::partition;
use dkod_orchestrator::replace::{replace_symbol, ReplaceOutcome};
use dkod_orchestrator::symbols::extract_rust_file;
use dkod_worktree::{
    branch, init_repo, GroupSpec, GroupStatus, Paths, SessionId, WriteLog, WriteRecord,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn run_git(args: &[&str], dir: &Path) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {} spawn failed: {e}", args.join(" ")));
    assert!(
        out.status.success(),
        "git {} failed (exit {:?}):\nstdout: {}\nstderr: {}",
        args.join(" "),
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

fn init_git(dir: &Path) {
    run_git(&["init", "-b", "main"], dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    // Two independent functions → partition produces two singleton groups.
    std::fs::write(
        dir.join("src/lib.rs"),
        "pub fn alpha() -> i32 { 1 }\npub fn beta() -> i32 { 2 }\n",
    )
    .unwrap();
    run_git(&["add", "."], dir);
    // Commit with enforced identity via env vars on the Command directly.
    let out = Command::new("git")
        .args(["commit", "-m", "init"])
        .env("GIT_AUTHOR_NAME", "Haim Ari")
        .env("GIT_AUTHOR_EMAIL", "haimari1@gmail.com")
        .env("GIT_COMMITTER_NAME", "Haim Ari")
        .env("GIT_COMMITTER_EMAIL", "haimari1@gmail.com")
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git commit -m init failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn m1_happy_path_produces_expected_git_log() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_git(repo);

    // Phase 0: init scaffold
    init_repo(repo, None).unwrap();
    let paths = Paths::new(repo);
    let sid = SessionId::generate();

    // Phase 2: plan — use a relative path so sym.file_path is relative,
    // which ensures `git add -- src/lib.rs` works from the repo root.
    let rel_path = PathBuf::from("src/lib.rs");
    let src = std::fs::read(repo.join(&rel_path)).unwrap();
    let (syms, calls) = extract_rust_file(&src, &rel_path).unwrap();
    let g = CallGraph::build(&syms, &calls);
    let in_scope: Vec<String> = syms.iter().map(|s| s.qualified_name.clone()).collect();
    let plan = partition(&in_scope, &g, 2).unwrap();
    assert_eq!(
        plan.groups.len(),
        2,
        "expected 2 singleton groups; got {}",
        plan.groups.len()
    );

    // Phase 3: execute_begin
    branch::create_dk_branch(repo, "main", sid.as_str()).unwrap();

    // Simulate two agents, one per group. Each agent:
    //   1. Applies its replacement to the current on-disk source.
    //   2. Appends a WriteRecord to the group's writes.jsonl.
    //   3. Calls commit_per_group for its group alone.
    //
    // Sequential application ensures each group commits a non-empty delta even
    // though both touch the same file.
    let mut group_ids = Vec::new();
    for (i, group) in plan.groups.iter().enumerate() {
        GroupSpec {
            id: group.id.clone(),
            symbols: group.symbols.clone(),
            agent_prompt: format!("bump values in group {}", group.id),
            status: GroupStatus::Done,
        }
        .save(&paths, &sid)
        .unwrap();

        let log = WriteLog::open(&paths, &sid, &group.id).unwrap();
        for sym in &group.symbols {
            // Re-read the current file state (updated by prior iterations).
            let current = std::fs::read(repo.join(&sym.file_path)).unwrap();
            let short = sym.qualified_name.rsplit("::").next().unwrap();
            let new_body = format!("pub fn {short}() -> i32 {{ {}0 }}", i + 1);
            let outcome = replace_symbol(&current, &sym.qualified_name, &new_body).unwrap();
            let new_src = match outcome {
                ReplaceOutcome::ParsedOk { new_source } => new_source,
                ReplaceOutcome::Fallback { new_source, .. } => new_source,
            };
            std::fs::write(repo.join(&sym.file_path), &new_src).unwrap();

            log.append(&WriteRecord {
                symbol: sym.qualified_name.clone(),
                file_path: sym.file_path.clone(),
                timestamp: "2026-04-24T12:00:00Z".into(),
            })
            .unwrap();
        }

        // Commit this group immediately so the next group has a clean base.
        commit_per_group(repo, &paths, &sid, std::slice::from_ref(&group.id)).unwrap();
        group_ids.push(group.id.clone());
    }

    // Phase 5: assertions
    let log_output = Command::new("git")
        .args(["log", "--format=%H %an <%ae> | %s"])
        .current_dir(repo)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&log_output.stdout);
    println!("git log output:\n{text}");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.len(),
        1 + group_ids.len(),
        "expected init + {} group commits; got {}\n{text}",
        group_ids.len(),
        lines.len()
    );
    // Most-recent commits first (group commits), then init commit last.
    for line in lines.iter().take(group_ids.len()) {
        assert!(
            line.contains("Haim Ari <haimari1@gmail.com>"),
            "identity not enforced: {line}"
        );
    }
    let init_line = lines.last().unwrap();
    assert!(
        init_line.contains("init"),
        "last commit should be init; got {init_line}"
    );

    // Source reflects the replacements.
    let final_src =
        String::from_utf8(std::fs::read(repo.join("src/lib.rs")).unwrap()).unwrap();
    // Both groups' replacements must be present; OR would let a silent
    // fallback on one replacement pass the test unnoticed.
    assert!(
        final_src.contains("10") && final_src.contains("20"),
        "both replacements must land: {final_src}"
    );
}
