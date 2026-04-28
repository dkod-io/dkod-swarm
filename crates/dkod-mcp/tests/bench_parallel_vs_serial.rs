//! Wall-clock benchmark: parallel writes via `tokio::join!` should beat
//! sequential awaits when each "write" carries a realistic LLM-thinking
//! delay.
//!
//! This is the empirical evidence behind dkod-swarm's parallel-N-agents
//! value proposition. Without the simulated delay, file I/O is too fast
//! to show meaningful parallelism. With a 100ms-per-write delay,
//! sequential = ~300ms, parallel = ~100ms. The test asserts a > 1.5×
//! speedup with a safety margin for CI variance.

#[path = "common/mod.rs"]
mod common;

use dkod_mcp::ServerCtx;
use dkod_mcp::schema::WriteSymbolRequest;
use dkod_mcp::tools::write_symbol::write_symbol;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// One simulated LLM-thinking delay per write. Tweak via env var
/// `DKOD_BENCH_LLM_DELAY_MS` for local exploration; CI uses the default.
fn llm_delay() -> Duration {
    let ms: u64 = std::env::var("DKOD_BENCH_LLM_DELAY_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    Duration::from_millis(ms)
}

/// Drive a single `write_symbol` call after a simulated LLM-thinking
/// delay. Mirrors what a Task subagent actually does in production:
/// the LLM "thinks" (delay), then the AST write happens (fast).
async fn synthetic_write(ctx: Arc<ServerCtx>, req: WriteSymbolRequest) {
    sleep(llm_delay()).await;
    write_symbol(&ctx, req).await.expect("write_symbol");
}
