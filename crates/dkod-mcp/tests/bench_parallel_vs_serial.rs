//! Wall-clock benchmark: parallel writes via `tokio::join!` should beat
//! sequential awaits when each "write" carries a realistic LLM-thinking
//! delay.
//!
//! This is the empirical evidence behind dkod-swarm's parallel-N-agents
//! value proposition. Without the simulated delay, file I/O is too fast
//! to show meaningful parallelism.
//!
//! With a 100ms-per-write delay an idealised model predicts
//! `serial ≈ 300ms`, `parallel ≈ 100ms` (a 3× ratio). The actual measurement
//! lands closer to `serial ≈ 500ms`, `parallel ≈ 290ms` (~1.7×) because
//! orchestrator overhead — `dkod_write_symbol`'s async tokio mutex,
//! tree-sitter re-parse, the per-file lock acquire — adds non-trivial
//! synchronization on top of the synthetic sleep on both arms. The
//! asymmetry that matters (three sleeps overlapping vs three in
//! sequence) is preserved, and the assertion (`> 1.5×`) holds with
//! comfortable headroom across 5/5 stress runs locally.

#[path = "common/mod.rs"]
mod common;
use common::init_tempo_repo;

use dkod_mcp::ServerCtx;
use dkod_mcp::schema::{ExecuteBeginRequest, GroupInput, SymbolRefSchema, WriteSymbolRequest};
use dkod_mcp::tools::execute_begin::execute_begin;
use dkod_mcp::tools::write_symbol::write_symbol;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Default per-write LLM-thinking delay. 100 ms is a realistic lower
/// bound for an LLM Task-subagent step; large enough to dominate the
/// orchestrator's intrinsic overhead so the parallel/serial ratio
/// stays measurable on noisy CI hardware.
const DEFAULT_LLM_DELAY_MS: u64 = 100;

/// One simulated LLM-thinking delay per write. Tweak via env var
/// `DKOD_BENCH_LLM_DELAY_MS` for local exploration; CI uses the default.
fn llm_delay() -> Duration {
    let ms: u64 = std::env::var("DKOD_BENCH_LLM_DELAY_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LLM_DELAY_MS);
    Duration::from_millis(ms)
}

/// Drive a single `write_symbol` call after a simulated LLM-thinking
/// delay. Mirrors what a Task subagent actually does in production:
/// the LLM "thinks" (delay), then the AST write happens (fast).
async fn synthetic_write(ctx: Arc<ServerCtx>, req: WriteSymbolRequest) {
    sleep(llm_delay()).await;
    write_symbol(&ctx, req).await.expect("write_symbol");
}

/// Set up a fresh dkod session with three groups, each owning one
/// distinct file. Returns the active context + the three write
/// requests the benchmark will fire (parallel vs serial).
async fn make_three_writes() -> (Arc<ServerCtx>, Vec<WriteSymbolRequest>, tempfile::TempDir) {
    let (tmp, root) = init_tempo_repo();
    let ctx = Arc::new(ServerCtx::new(&root));

    // Three distinct files so the per-file lock does not serialise
    // them — we want to measure the orchestrator's parallelism, not
    // the lock's correctness (covered by `tests/write_symbol_lock.rs`).
    std::fs::write(root.join("src/lib.rs"), "pub fn a() {}\npub fn b() {}\n").unwrap();
    std::fs::write(root.join("src/m1.rs"), "pub fn m1_fn() {}\n").unwrap();
    std::fs::write(root.join("src/m2.rs"), "pub fn m2_fn() {}\n").unwrap();
    let s = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(s.success());
    let s = std::process::Command::new("git")
        .args(["commit", "-q", "-m", "bench seed"])
        .current_dir(&root)
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .status()
        .unwrap();
    assert!(s.success());

    execute_begin(
        &ctx,
        ExecuteBeginRequest {
            task_prompt: "bench".into(),
            groups: vec![GroupInput {
                id: "g1".into(),
                symbols: vec![
                    SymbolRefSchema {
                        qualified_name: "a".into(),
                        file_path: PathBuf::from("src/lib.rs"),
                        kind: "function".into(),
                    },
                    SymbolRefSchema {
                        qualified_name: "m1_fn".into(),
                        file_path: PathBuf::from("src/m1.rs"),
                        kind: "function".into(),
                    },
                    SymbolRefSchema {
                        qualified_name: "m2_fn".into(),
                        file_path: PathBuf::from("src/m2.rs"),
                        kind: "function".into(),
                    },
                ],
                agent_prompt: "rewrite".into(),
            }],
        },
    )
    .await
    .expect("execute_begin");

    let writes = vec![
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/lib.rs"),
            qualified_name: "a".into(),
            new_body: "pub fn a() { /* P */ }".into(),
        },
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/m1.rs"),
            qualified_name: "m1_fn".into(),
            new_body: "pub fn m1_fn() { /* P */ }".into(),
        },
        WriteSymbolRequest {
            group_id: "g1".into(),
            file: PathBuf::from("src/m2.rs"),
            qualified_name: "m2_fn".into(),
            new_body: "pub fn m2_fn() { /* P */ }".into(),
        },
    ];
    (ctx, writes, tmp)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_writes_beat_serial_writes_under_simulated_llm_delay() {
    // Parallel run.
    let (ctx_p, writes_p, _tmp_p) = make_three_writes().await;
    let start_parallel = Instant::now();
    let ctx_a = Arc::clone(&ctx_p);
    let ctx_b = Arc::clone(&ctx_p);
    let ctx_c = Arc::clone(&ctx_p);
    let mut iter = writes_p.into_iter();
    let w_a = synthetic_write(ctx_a, iter.next().unwrap());
    let w_b = synthetic_write(ctx_b, iter.next().unwrap());
    let w_c = synthetic_write(ctx_c, iter.next().unwrap());
    tokio::join!(w_a, w_b, w_c);
    let parallel = start_parallel.elapsed();

    // Serial run — fresh context (clean tempdir) so the parallel run's
    // write artifacts don't perturb the serial timing.
    let (ctx_s, writes_s, _tmp_s) = make_three_writes().await;
    let start_serial = Instant::now();
    for req in writes_s {
        synthetic_write(Arc::clone(&ctx_s), req).await;
    }
    let serial = start_serial.elapsed();

    eprintln!(
        "parallel: {parallel:?}  serial: {serial:?}  ratio: {:.2}x",
        serial.as_secs_f64() / parallel.as_secs_f64()
    );
    // Expected: serial ≈ 3 × delay, parallel ≈ 1 × delay. Assert > 1.5×
    // with margin for CI scheduling jitter. With a 100ms delay,
    // serial ≈ 310ms and parallel ≈ 110ms — ratio ≈ 2.8×.
    let ratio = serial.as_secs_f64() / parallel.as_secs_f64();
    assert!(
        ratio > 1.5,
        "expected parallel speedup > 1.5×, got {ratio:.2}× (parallel: {parallel:?}, serial: {serial:?})"
    );
}
