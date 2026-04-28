//! Wall-clock benchmark: parallel writes via `tokio::join!` should beat
//! sequential awaits when each "write" carries a realistic LLM-thinking
//! delay.
//!
//! This is the empirical evidence behind dkod-swarm's parallel-N-agents
//! value proposition. Without the simulated delay, file I/O is too fast
//! to show meaningful parallelism.
//!
//! ## Idealised vs observed
//!
//! With the default 100 ms per-write delay an **idealised** model
//! predicts `serial ≈ 300 ms`, `parallel ≈ 100 ms` (a `3×` ratio).
//! The **observed** measurement on this dev box lands closer to
//! `serial ≈ 500 ms`, `parallel ≈ 290 ms` — about `1.7×` — because
//! orchestrator overhead (`dkod_write_symbol`'s async tokio mutex,
//! tree-sitter re-parse, the per-file lock acquire) adds non-trivial
//! synchronization on top of the synthetic sleep on **both** arms.
//! The asymmetry that matters — three sleeps overlapping vs three in
//! sequence — is preserved, and the test asserts the **median ratio
//! over 5 trials > 1.5×** with comfortable headroom on local hardware.
//! The median absorbs single-trial CI scheduling outliers.

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
/// Floor on the env-tunable delay. Below ~50 ms the synthetic sleep
/// stops dominating orchestrator overhead and the ratio collapses
/// into noise — clamping prevents a misconfigured `DKOD_BENCH_LLM_DELAY_MS`
/// from turning the assertion into a flake.
const MIN_LLM_DELAY_MS: u64 = 50;
/// Ceiling on the env-tunable delay. Mostly a defensive cap so a
/// fat-fingered value doesn't make the test wall for minutes.
const MAX_LLM_DELAY_MS: u64 = 5_000;
/// Number of timing trials per arm; we assert on the median ratio so
/// one outlier on a shared CI runner can't fail the build.
const TIMING_TRIALS: usize = 5;

/// One simulated LLM-thinking delay per write. Tweak via env var
/// `DKOD_BENCH_LLM_DELAY_MS` for local exploration; CI uses the default.
/// Values are clamped to `[MIN_LLM_DELAY_MS, MAX_LLM_DELAY_MS]` so a
/// misconfigured env var cannot invalidate the benchmark assumptions.
fn llm_delay() -> Duration {
    let ms: u64 = std::env::var("DKOD_BENCH_LLM_DELAY_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LLM_DELAY_MS)
        .clamp(MIN_LLM_DELAY_MS, MAX_LLM_DELAY_MS);
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

/// Median of a non-empty slice of `f64`. NaNs are not expected here —
/// every input is a finite ratio of two `Duration::as_secs_f64` values
/// — so an unsorted ordering panic via `partial_cmp().unwrap()` is the
/// right loud failure mode if that assumption ever breaks.
fn median(xs: &[f64]) -> f64 {
    let mut sorted = xs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("ratio comparator: no NaN expected"));
    sorted[sorted.len() / 2]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_writes_beat_serial_writes_under_simulated_llm_delay() {
    // Run the parallel/serial pair `TIMING_TRIALS` times and assert on
    // the MEDIAN ratio. A single-shot wall-clock test is flaky on
    // shared CI runners — one scheduling outlier can drag a ~1.7×
    // observation under the 1.5× bar even though the parallelism is
    // working correctly. The median absorbs outliers without lowering
    // the assertion.
    let mut ratios = Vec::with_capacity(TIMING_TRIALS);
    let mut last_parallel = Duration::ZERO;
    let mut last_serial = Duration::ZERO;

    for trial in 0..TIMING_TRIALS {
        // Parallel run — fresh tempdir so previous trials don't perturb
        // this trial's timing.
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

        // Serial run — also fresh.
        let (ctx_s, writes_s, _tmp_s) = make_three_writes().await;
        let start_serial = Instant::now();
        for req in writes_s {
            synthetic_write(Arc::clone(&ctx_s), req).await;
        }
        let serial = start_serial.elapsed();

        let ratio = serial.as_secs_f64() / parallel.as_secs_f64();
        eprintln!(
            "  trial {trial}: parallel: {parallel:?}  serial: {serial:?}  ratio: {ratio:.2}x"
        );
        ratios.push(ratio);
        last_parallel = parallel;
        last_serial = serial;
    }

    let median_ratio = median(&ratios);
    eprintln!(
        "  median ratio over {TIMING_TRIALS} trials: {median_ratio:.2}x  (samples: {ratios:?})"
    );

    // The idealised model under a 100 ms delay predicts
    // `serial ≈ 300 ms`, `parallel ≈ 100 ms` (ratio ≈ 3×). Observed in
    // practice — see the module-level docs — both arms inflate via
    // orchestrator overhead, landing the median near ~1.7×. Assert
    // strictly above 1.5× with comfortable headroom.
    assert!(
        median_ratio > 1.5,
        "expected median parallel speedup > 1.5×, got {median_ratio:.2}× over {TIMING_TRIALS} trials \
         (last sample: parallel: {last_parallel:?}, serial: {last_serial:?}; all ratios: {ratios:?})"
    );
}
