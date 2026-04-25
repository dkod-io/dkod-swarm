//! Integration tests for `dkod_pr` (PR M2-7).
//!
//! Strategy: shim `gh` (and, for the happy-path test, `git`) via a temp
//! `bin/` directory passed to `pr_with_shim` as `path_prefix`. This avoids
//! mutating the global `PATH` (which would race with sibling tests) and
//! avoids hitting GitHub from CI.

#[path = "common/mod.rs"]
mod common;

use common::init_tempo_repo;
use dkod_mcp::ServerCtx;
use dkod_mcp::schema::{
    ExecuteBeginRequest, GroupInput, PrRequest, SymbolRefSchema, WriteSymbolRequest,
};
use dkod_mcp::tools::commit::commit;
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::tools::pr::{pr, pr_with_shim};
use dkod_mcp::tools::write_symbol::write_symbol;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Drop a `gh` shim under `<dir>/bin/` whose body is `body` (verbatim shell
/// script content). Returns the bin dir to pass as `path_prefix`.
fn make_gh_shim(dir: &Path, body: &str) -> PathBuf {
    let bin_dir = dir.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let shim = bin_dir.join("gh");
    std::fs::write(&shim, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&shim).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&shim, perm).unwrap();
    }
    bin_dir
}

/// Drop a `git` shim alongside the `gh` shim under the same `bin_dir`. Used
/// only by the happy-path test, which needs `git push` to succeed without a
/// real `origin` remote. The shim is *additive* — it does not replace the
/// real `git`; we only override the `push` subcommand and forward everything
/// else to the system `git` so existing in-test git commands still work
/// (e.g. `git rev-parse` in `commit`'s pre/post HEAD capture).
///
/// **Important:** the helper looks up the absolute path to the system `git`
/// at shim-creation time and bakes it into the script, so the shim's own
/// `PATH` (which has `bin_dir` in front) cannot accidentally re-invoke the
/// shim recursively.
fn install_git_shim(bin_dir: &Path) {
    let real_git = which_git();
    let shim = bin_dir.join("git");
    let body = format!(
        r#"#!/bin/sh
if [ "$1" = "push" ]; then
    # Pretend the push succeeded.
    exit 0
fi
exec {real} "$@"
"#,
        real = shell_quote(&real_git),
    );
    std::fs::write(&shim, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&shim).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&shim, perm).unwrap();
    }
}

/// Locate the real `git` binary on the test runner. We resolve it once at
/// shim-creation time so the shim never needs to consult `PATH` (which it
/// has shadowed). Falls back to `/usr/bin/git` only if `which` is unavailable.
fn which_git() -> String {
    let out = std::process::Command::new("which")
        .arg("git")
        .output()
        .expect("which git");
    if out.status.success() {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        "/usr/bin/git".to_string()
    }
}

/// Single-quote `s` for safe inclusion in a `/bin/sh` command. We never
/// expect spaces in a git path on dev boxes, but locking this down keeps the
/// shim resilient.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[tokio::test]
async fn pr_is_idempotent_when_already_open() {
    // Drive the tool through begin → write → commit → pr, and shim `gh` to
    // unconditionally claim the PR already exists. The first idempotency
    // check fires, so the helper must NOT call `gh pr create` (the shim
    // exits non-zero on `pr create` to make a regression loud).
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));

    execute_begin(
        &ctx,
        ExecuteBeginRequest {
            task_prompt: "demo".into(),
            groups: vec![GroupInput {
                id: "g1".into(),
                symbols: vec![SymbolRefSchema {
                    qualified_name: "a".into(),
                    file_path: PathBuf::from("src/lib.rs"),
                    kind: "function".into(),
                }],
                agent_prompt: "rewrite a".into(),
            }],
        },
    )
    .await
    .expect("execute_begin");

    write_symbol(
        &ctx,
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/lib.rs"),
            qualified_name: "a".into(),
            new_body: "pub fn a() { /* x */ }".into(),
        },
    )
    .await
    .expect("write_symbol");
    commit(&ctx).await.expect("commit");

    // gh shim: `pr list` → URL, `pr create` → loud failure.
    let bin_dir = make_gh_shim(
        &root,
        r#"if [ "$1" = "pr" ] && [ "$2" = "list" ]; then
    echo "https://github.com/x/y/pull/42"
    exit 0
elif [ "$1" = "pr" ] && [ "$2" = "create" ]; then
    echo "shim: pr create should not have been called" >&2
    exit 99
fi
echo "shim: unhandled gh args: $*" >&2
exit 99"#,
    );

    let resp = pr_with_shim(
        &ctx,
        PrRequest {
            title: "t".into(),
            body: "b".into(),
        },
        Some(&bin_dir),
    )
    .await
    .expect("pr_with_shim");
    assert!(resp.was_existing, "expected was_existing=true");
    assert_eq!(resp.url, "https://github.com/x/y/pull/42");
}

#[tokio::test]
async fn pr_errors_when_verify_cmd_fails() {
    // verify_cmd = `false` (always-fail UNIX command). The helper must
    // surface VerifyFailed *before* touching gh / git. To prove no gh / git
    // call escapes verify-fail, we install a shim that exits non-zero on
    // ANY invocation: if the helper ever reached past verify, it would
    // surface a different error variant (Gh / Git) and the matches! check
    // below would fail.
    let (_tmp, root) = init_tempo_repo();
    // Overwrite the config file written by `init_repo` to add verify_cmd.
    let cfg = dkod_worktree::Config {
        main_branch: "main".into(),
        verify_cmd: Some("false".into()),
    };
    cfg.save(&root.join(".dkod/config.toml")).unwrap();

    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(
        &ctx,
        ExecuteBeginRequest {
            task_prompt: "demo".into(),
            groups: vec![GroupInput {
                id: "g1".into(),
                symbols: vec![],
                agent_prompt: "x".into(),
            }],
        },
    )
    .await
    .expect("execute_begin");

    let bin_dir = make_gh_shim(
        &root,
        r#"echo "shim: verify_cmd path should not invoke gh" >&2
exit 77"#,
    );

    let err = pr_with_shim(
        &ctx,
        PrRequest {
            title: "t".into(),
            body: "b".into(),
        },
        Some(&bin_dir),
    )
    .await
    .expect_err("expected verify_cmd failure");
    assert!(
        matches!(err, dkod_mcp::Error::VerifyFailed { .. }),
        "expected VerifyFailed, got {err:?}"
    );
}

#[tokio::test]
async fn pr_creates_when_no_existing_pr() {
    // Happy path: `gh pr list` returns empty (no existing PR), `git push`
    // succeeds (shimmed), `gh pr list` returns empty again on the post-push
    // re-check, and `gh pr create` returns a canned URL. Asserts
    // was_existing=false and the URL flows through.
    //
    // The gh shim distinguishes `pr list` (echo nothing, exit 0) from
    // `pr create` (echo the canned URL). The git shim short-circuits `git
    // push` and forwards everything else to the real binary.
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));

    execute_begin(
        &ctx,
        ExecuteBeginRequest {
            task_prompt: "demo".into(),
            groups: vec![GroupInput {
                id: "g1".into(),
                symbols: vec![SymbolRefSchema {
                    qualified_name: "a".into(),
                    file_path: PathBuf::from("src/lib.rs"),
                    kind: "function".into(),
                }],
                agent_prompt: "rewrite a".into(),
            }],
        },
    )
    .await
    .expect("execute_begin");

    write_symbol(
        &ctx,
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/lib.rs"),
            qualified_name: "a".into(),
            new_body: "pub fn a() { /* y */ }".into(),
        },
    )
    .await
    .expect("write_symbol");
    commit(&ctx).await.expect("commit");

    let bin_dir = make_gh_shim(
        &root,
        r#"if [ "$1" = "pr" ] && [ "$2" = "list" ]; then
    # No existing PR.
    exit 0
elif [ "$1" = "pr" ] && [ "$2" = "create" ]; then
    echo "https://github.com/fake/repo/pull/123"
    exit 0
fi
echo "shim: unhandled gh args: $*" >&2
exit 99"#,
    );
    install_git_shim(&bin_dir);

    let resp = pr_with_shim(
        &ctx,
        PrRequest {
            title: "t".into(),
            body: "b".into(),
        },
        Some(&bin_dir),
    )
    .await
    .expect("pr_with_shim");
    assert!(!resp.was_existing, "expected was_existing=false");
    assert_eq!(resp.url, "https://github.com/fake/repo/pull/123");
}

/// Race-window coverage for the **second** idempotency check
/// (post-push, pre-create). Scenario: the first `gh pr list` returns
/// nothing (no existing PR), the push succeeds, then a concurrent
/// process opens a PR before our `gh pr create` runs — our second
/// `pr_exists` call must catch that and return `was_existing: true`
/// without invoking `gh pr create`.
///
/// Implementation: a stateful `gh` shim that tracks invocation count
/// across calls. The first `pr list` returns empty; the second returns
/// a URL. `pr create` exits non-zero so the test fails noisily if
/// reached.
#[tokio::test]
async fn pr_handles_race_when_pr_appears_during_push() {
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    execute_begin(
        &ctx,
        ExecuteBeginRequest {
            task_prompt: "demo".into(),
            groups: vec![GroupInput {
                id: "g1".into(),
                symbols: vec![SymbolRefSchema {
                    qualified_name: "a".into(),
                    file_path: PathBuf::from("src/lib.rs"),
                    kind: "function".into(),
                }],
                agent_prompt: "rewrite a".into(),
            }],
        },
    )
    .await
    .expect("execute_begin");
    write_symbol(
        &ctx,
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/lib.rs"),
            qualified_name: "a".into(),
            new_body: "pub fn a() { /* y */ }".into(),
        },
    )
    .await
    .expect("write_symbol");
    commit(&ctx).await.expect("commit");

    // Stateful shim: a counter file on disk distinguishes the first
    // `pr list` (return empty) from the second (return a URL). `pr
    // create` exits 99 to make a regression loud.
    let counter_file = root.join(".gh-list-count");
    std::fs::write(&counter_file, "0").unwrap();
    let counter_path = counter_file.display().to_string();
    let bin_dir = make_gh_shim(
        &root,
        &format!(
            r#"if [ "$1" = "pr" ] && [ "$2" = "list" ]; then
    n=$(cat "{counter_path}")
    n=$((n + 1))
    echo "$n" > "{counter_path}"
    if [ "$n" = "1" ]; then
        # First check (pre-push): no existing PR.
        exit 0
    else
        # Second check (post-push): a concurrent process opened one.
        echo "https://github.com/fake/repo/pull/777"
        exit 0
    fi
elif [ "$1" = "pr" ] && [ "$2" = "create" ]; then
    echo "shim: pr create must NOT be called when second idempotency check finds a PR" >&2
    exit 99
fi
echo "shim: unhandled gh args: $*" >&2
exit 99"#
        ),
    );
    install_git_shim(&bin_dir);

    let resp = pr_with_shim(
        &ctx,
        PrRequest {
            title: "t".into(),
            body: "b".into(),
        },
        Some(&bin_dir),
    )
    .await
    .expect("pr_with_shim");
    assert!(
        resp.was_existing,
        "expected was_existing=true (second idempotency check fired), got {resp:?}"
    );
    assert_eq!(resp.url, "https://github.com/fake/repo/pull/777");

    // Sanity: the shim was invoked exactly twice for `pr list` (once
    // before push, once after) — no third call since `pr create` is
    // skipped.
    let final_count = std::fs::read_to_string(&counter_file).unwrap();
    assert_eq!(final_count.trim(), "2", "expected 2 pr-list calls");
}

#[tokio::test]
async fn pr_without_active_session_errors() {
    // No `dkod_execute_begin` was called → `pr` must fail fast with
    // `NoActiveSession` before doing any subprocess work.
    let (_tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));
    let err = pr(
        &ctx,
        PrRequest {
            title: "t".into(),
            body: "b".into(),
        },
    )
    .await
    .expect_err("expected NoActiveSession");
    assert!(
        matches!(err, dkod_mcp::Error::NoActiveSession),
        "expected NoActiveSession, got {err:?}"
    );
}
