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
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let ctx = Arc::new(ServerCtx::new(repo_root));
    let server = McpServer::new(ctx);
    tokio::spawn(async move {
        let svc = server.serve(server_io).await.expect("server serve");
        svc.waiting().await.ok();
    });
    ().into_dyn()
        .serve(client_io)
        .await
        .expect("client handshake")
}

fn run(dir: &Path, args: &[&str]) {
    let status = Command::new(args[0])
        .args(&args[1..])
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "command failed: {args:?}");
}

fn run_with_identity(dir: &Path, args: &[&str]) {
    let status = Command::new(args[0])
        .args(&args[1..])
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Haim Ari")
        .env("GIT_AUTHOR_EMAIL", "haimari1@gmail.com")
        .env("GIT_COMMITTER_NAME", "Haim Ari")
        .env("GIT_COMMITTER_EMAIL", "haimari1@gmail.com")
        .status()
        .unwrap();
    assert!(status.success(), "command failed: {args:?}");
}
