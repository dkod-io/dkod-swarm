//! Pure helper for the `dkod_write_symbol` MCP tool.
//!
//! The `#[tool]` wrapper that exposes this lives in `tools/mod.rs`.
//!
//! # Concurrency
//!
//! This is the only tool in M2 that takes a per-file lock. The locking
//! sequence is:
//!
//! 1. Resolve the caller-supplied path under the canonical repo root.
//!    This rejects absolute paths, `..` traversal, and symlinks pointing
//!    outside the repo (delegated to `tools::path::resolve_under_repo`,
//!    shared with `dkod_plan`).
//! 2. Take the per-file lock keyed by the canonical absolute path. Two
//!    concurrent writes to the same file serialise here; two concurrent
//!    writes to *different* files do not.
//! 3. Inside the guard: read → `replace_symbol` → write → append to
//!    `writes.jsonl`. Holding the guard across the full read-modify-write
//!    closes the TOCTOU window — a concurrent writer cannot slip a write
//!    in between our read and our write.
//!
//! # Why not `spawn_blocking` here?
//!
//! `replace_symbol` runs a tree-sitter parse, which is CPU-bound, and the
//! plan flagged `spawn_blocking` as a future optimisation. We deliberately
//! keep it synchronous on the runtime: the `_guard` returned by
//! `tokio::sync::Mutex::lock().await` cannot move across thread boundaries,
//! so wrapping the parse in `spawn_blocking` would require routing data
//! over a channel while still holding the guard — substantially uglier
//! code without a real perf win at M2 fixture sizes. If/when this becomes
//! hot we can introduce a held-guard channel pattern; until then, simpler
//! is better.

use crate::schema::{WriteSymbolRequest, WriteSymbolResponse};
use crate::tools::path::resolve_under_repo;
use crate::{Error, Result, ServerCtx};
use dkod_orchestrator::replace::{ReplaceOutcome, replace_symbol};
use dkod_worktree::{WriteLog, WriteRecord};

pub async fn write_symbol(ctx: &ServerCtx, req: WriteSymbolRequest) -> Result<WriteSymbolResponse> {
    // Snapshot the active session id and release the lock immediately —
    // the file-lock acquisition below is a separate critical section, and
    // holding `active_session` across `.lock().await` on a per-file mutex
    // would needlessly serialise unrelated tools (e.g. concurrent `dkod_status`
    // calls when M2-5 lands).
    let sid = ctx
        .active_session
        .lock()
        .await
        .clone()
        .ok_or(Error::NoActiveSession)?;

    // Path-escape guard: reject absolute paths, `..` traversal, and symlinks
    // that resolve outside the repo. `canonical_repo` here is canonicalised
    // once per call — every per-file canonicalize inside `resolve_under_repo`
    // joins onto this already-real path.
    let canonical_repo = std::fs::canonicalize(&ctx.repo_root).map_err(Error::Io)?;
    let canonical = resolve_under_repo(&canonical_repo, &req.file)?;

    // Per-file lock. Two writes to the *same* canonical path serialise on
    // `_guard`; two writes to *different* paths run concurrently because
    // each gets its own `Arc<Mutex<()>>` from the lock table.
    let lock = ctx.file_lock(&canonical).await;
    let _guard = lock.lock().await;

    // Read → replace → write — all inside the guard scope. If we read
    // before the lock, a concurrent writer could overwrite our basis and
    // our write would silently undo their change.
    let bytes = std::fs::read(&canonical).map_err(Error::Io)?;
    let outcome = replace_symbol(&bytes, &req.qualified_name, &req.new_body)?;
    let (new_source, outcome_label, fallback_reason) = match outcome {
        ReplaceOutcome::ParsedOk { new_source } => (new_source, "parsed_ok", None),
        ReplaceOutcome::Fallback { new_source, reason } => (new_source, "fallback", Some(reason)),
    };
    let bytes_written = new_source.len();
    std::fs::write(&canonical, &new_source).map_err(Error::Io)?;

    // Append to `writes.jsonl`. `WriteLog::append` opens with O_APPEND, but
    // we do not lean on POSIX append atomicity — concurrent appenders to
    // the same group log are already serialised by the per-file lock above
    // (they are writing the same file, so they share a lock entry only if
    // they target the same source path; for distinct source paths within
    // the same group, the JSONL append happens to be safe under O_APPEND
    // for line-sized writes on Linux/macOS, which is what we rely on here).
    let log = WriteLog::open(&ctx.paths, &sid, &req.group_id)?;
    log.append(&WriteRecord {
        symbol: req.qualified_name,
        file_path: req.file,
        timestamp: crate::time::iso8601_now(),
    })?;

    Ok(WriteSymbolResponse {
        outcome: outcome_label.into(),
        fallback_reason,
        bytes_written,
    })
}
