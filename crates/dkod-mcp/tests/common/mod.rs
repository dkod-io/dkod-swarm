#![allow(dead_code)] // harness; each test file uses a subset

// NOTE — rmcp 1.5 client wiring delta vs plan:
//
// The plan sketch typed the handshake as:
//     `RunningService<RoleClient, rmcp::model::InitializeRequestParam>`.
//
// That second generic is wrong: `RunningService<R: ServiceRole, S: Service<R>>`
// takes the *handler* type, not the request params. With `().into_dyn()` the
// handler is `Box<dyn DynService<RoleClient>>`. Exposing that concrete type
// through every test signature is ugly, so helpers below return
// `RunningService<RoleClient, Box<dyn DynService<RoleClient>>>` — callers can
// alias it if they want.
//
// The rest of the plan's outline (duplex transport, `.serve(client_io)` on the
// dyn-erased unit handler) is correct per the rmcp-1.5 probe.

use dkod_mcp::{McpServer, ServerCtx};
use rmcp::{
    RoleClient, ServiceExt,
    service::{DynService, RunningService},
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

/// Buffer size for the in-memory duplex connecting the test client and
/// server. 64 KiB is large enough for a full MCP initialize handshake
/// without fragmentation.
const DUPLEX_BUF_BYTES: usize = 64 * 1024;

/// Initialise a fresh git repo in a tempdir, seed one commit on `main`
/// with `tests/fixtures/tiny_rust/src/lib.rs`, and return the repo path.
pub fn init_tempo_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();

    run(&root, &["git", "init", "-q", "-b", "main"]);
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tiny_rust/src/lib.rs");
    std::fs::copy(&fixture, src.join("lib.rs")).expect("copy fixture");
    // Minimal Cargo.toml so the fixture compiles if a caller chooses to build it.
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();

    run_with_identity(&root, &["git", "add", "-A"]);
    run_with_identity(&root, &["git", "commit", "-q", "-m", "seed"]);

    // init .dkod/
    dkod_worktree::init_repo(&root, None).expect("init_repo");
    (tmp, root)
}

/// Spawn an in-process rmcp server bound to `repo_root` and return a client
/// connected to it via an in-memory duplex transport.
pub async fn spawn_in_process_server(
    repo_root: &Path,
) -> RunningService<RoleClient, Box<dyn DynService<RoleClient>>> {
    // rmcp supports arbitrary `AsyncRead + AsyncWrite` transports; a pair of
    // `tokio::io::duplex` streams gives us an in-process client/server link
    // without touching real stdio. Confirmed by the rmcp-1.5 probe.
    let (client_io, server_io) = tokio::io::duplex(DUPLEX_BUF_BYTES);
    let ctx = Arc::new(ServerCtx::new(repo_root));
    let server = McpServer::new(ctx);
    tokio::spawn(async move {
        let svc = server.serve(server_io).await.expect("server serve");
        if let Err(e) = svc.waiting().await {
            eprintln!("dkod-mcp test server exited with error: {e:?}");
        }
    });
    ().into_dyn()
        .serve(client_io)
        .await
        .expect("client handshake")
}

fn run(dir: &Path, args: &[&str]) {
    assert!(!args.is_empty(), "command args cannot be empty");
    let output = Command::new(args[0])
        .args(&args[1..])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command failed: {args:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// Fixture identity used for the seed commit of the in-memory test repo.
// Production commits flow through `dkod_worktree::branch::commit_paths`,
// which forces the real Haim Ari identity; the fixture seed commit is
// never pushed anywhere and uses a reserved `.invalid` TLD per RFC 2606
// to keep PII out of committed test code.
const FIXTURE_GIT_NAME: &str = "dkod-swarm fixture";
const FIXTURE_GIT_EMAIL: &str = "fixture@example.invalid";

fn run_with_identity(dir: &Path, args: &[&str]) {
    assert!(!args.is_empty(), "command args cannot be empty");
    let output = Command::new(args[0])
        .args(&args[1..])
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", FIXTURE_GIT_NAME)
        .env("GIT_AUTHOR_EMAIL", FIXTURE_GIT_EMAIL)
        .env("GIT_COMMITTER_NAME", FIXTURE_GIT_NAME)
        .env("GIT_COMMITTER_EMAIL", FIXTURE_GIT_EMAIL)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command failed: {args:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
