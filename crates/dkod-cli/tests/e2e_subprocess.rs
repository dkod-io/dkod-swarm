//! End-to-end subprocess tests for the compiled `dkod` binary.
//!
//! These spawn the real binary via `assert_cmd::Command::cargo_bin` so
//! the full argv → clap → dispatch → library path is exercised, not
//! just the in-process `cmd::*::run` helpers covered by other tests.
//! Coverage focus: `dkod init` writes `.dkod/config.toml` and `dkod
//! status` produces parseable JSON for an empty session; `--help`
//! advertises every subcommand and the `--mcp` flag; combining
//! `--mcp` with a subcommand exits non-zero.
//!
//! `dkod --mcp` itself is NOT spawned here — it would block on stdin
//! waiting for an MCP client. The MCP surface is covered by
//! `dkod-mcp/tests/e2e_smoke.rs` (M2-8).

use assert_cmd::Command;
use std::path::PathBuf;

/// Minimal git repo bootstrap — `dkod init` calls
/// `dkod_worktree::branch::detect_main`, which requires a real git
/// repo with `main` as the initial branch.
fn init_git_repo(root: &std::path::Path) {
    let s = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(root)
        .status()
        .expect("spawn git init");
    assert!(s.success(), "git init exited non-zero");
}

#[test]
fn dkod_init_then_status_via_subprocess() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root: PathBuf = tmp.path().to_path_buf();
    init_git_repo(&root);

    // `dkod init --verify-cmd "cargo test"` should succeed and write
    // the config file.
    Command::cargo_bin("dkod")
        .expect("cargo_bin dkod")
        .arg("init")
        .arg("--verify-cmd")
        .arg("cargo test")
        .current_dir(&root)
        .assert()
        .success();
    assert!(
        root.join(".dkod/config.toml").is_file(),
        ".dkod/config.toml should exist after `dkod init`"
    );

    // `dkod status` against a freshly-initialised repo prints the
    // empty-session response. We capture stdout and parse it as JSON
    // to assert structure rather than string-matching.
    let out = Command::cargo_bin("dkod")
        .expect("cargo_bin dkod")
        .arg("status")
        .current_dir(&root)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).expect("status stdout is UTF-8");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("status stdout should be valid JSON");
    assert!(
        parsed["active_session_id"].is_null(),
        "expected no active session, got: {parsed}"
    );
    assert!(
        parsed["dk_branch"].is_null(),
        "expected no dk-branch, got: {parsed}"
    );
    assert!(
        parsed["groups"]
            .as_array()
            .expect("groups should be an array")
            .is_empty(),
        "expected empty groups, got: {parsed}"
    );
}

#[test]
fn dkod_help_lists_subcommands() {
    let out = Command::cargo_bin("dkod")
        .expect("cargo_bin dkod")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).expect("help stdout is UTF-8");
    for needle in ["init", "status", "abort", "--mcp"] {
        assert!(
            s.contains(needle),
            "`dkod --help` should advertise `{needle}`, got:\n{s}"
        );
    }
}

#[test]
fn dkod_rejects_mcp_with_subcommand() {
    // clap accepts `--mcp init` as a parse (the flag and the
    // subcommand are not declared mutually exclusive at the clap
    // layer); `Cli::command_resolved` is what enforces the rule and
    // surfaces an Err. The binary's `main` propagates that Err and
    // exits non-zero — that is what we assert here. The stderr check
    // pins the failure to the resolver path: a `.failure()`-only
    // assertion would also pass if clap rejected the args earlier for
    // some unrelated reason.
    let assert = Command::cargo_bin("dkod")
        .expect("cargo_bin dkod")
        .args(["--mcp", "init"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("--mcp cannot be combined"),
        "stderr should report the resolver error, got: {stderr}"
    );
}
