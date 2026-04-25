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

    // Re-check the active session under the per-file lock. There is a
    // narrow window between the initial `sid` snapshot above and the
    // acquisition of `_guard` during which `dkod_abort` can complete
    // (it acquires `active_session` + the lock-table outer mutex
    // atomically, destroys the dk-branch, and clears both). Without this
    // re-check, a write that was in-flight when abort fired would land
    // on the post-abort working tree (`main`), corrupting the wrong
    // branch. A complete fix would have `dkod_abort` drain every
    // per-file lock before destroying the branch — that is filed for a
    // follow-up PR; the re-check here closes the realistic window for
    // the current single-stdio-client threat model.
    {
        let active_now = ctx.active_session.lock().await;
        match active_now.as_ref() {
            Some(s) if s.as_str() == sid.as_str() => {}
            _ => return Err(Error::NoActiveSession),
        }
    }

    // Open the WriteLog BEFORE mutating the file. `WriteLog::open`
    // creates the group's `writes.jsonl` parent directory and validates
    // that `req.group_id` is a safe path component — failing here
    // ensures we never mutate the source file with a bad group id.
    let log = WriteLog::open(&ctx.paths, &sid, &req.group_id)?;

    // Read → replace → write — all inside the guard scope. If we read
    // before the lock, a concurrent writer could overwrite our basis and
    // our write would silently undo their change. We keep `bytes` (the
    // pre-write content) in scope so the post-append undo path below can
    // restore the file if the audit-log append fails.
    let bytes = std::fs::read(&canonical).map_err(Error::Io)?;
    let outcome = replace_symbol(&bytes, &req.qualified_name, &req.new_body)?;
    let (new_source, outcome_label, fallback_reason) = match outcome {
        ReplaceOutcome::ParsedOk { new_source } => (new_source, "parsed_ok", None),
        ReplaceOutcome::Fallback { new_source, reason } => (new_source, "fallback", Some(reason)),
    };
    let bytes_written = new_source.len();
    std::fs::write(&canonical, &new_source).map_err(Error::Io)?;

    // Append to the group's `writes.jsonl`.
    //
    // The per-file lock above is keyed by **source path** (e.g. `src/lib.rs`),
    // not by the JSONL log path — so it does NOT serialise concurrent
    // appenders to the same group log when those appenders are writing
    // *different* source files. Concurrent JSONL append safety relies on
    // `WriteLog::append` opening with `O_APPEND`, which on Linux/macOS gives
    // atomic line-sized writes (POSIX `write(2)` PIPE_BUF guarantee for
    // local files). The records produced here are well under that bound,
    // so this is the correct guarantee to lean on.
    //
    // What the per-file lock DOES serialise is the read → replace → write
    // → append sequence for the *same* source path, ensuring two writes to
    // the same file see consistent intermediate state.
    //
    // If the append fails AFTER the file was successfully written, undo
    // the file write so the on-disk source matches the audit trail. M2-6
    // (`dkod_commit`) drives commits from `writes.jsonl`; a write that
    // succeeded but never made it into the log would silently miss the
    // commit and ship out-of-sync source on the dk-branch. Restoring the
    // pre-write content is the only way to keep audit/source consistent
    // without a more elaborate two-phase-commit protocol.
    let record = WriteRecord {
        symbol: req.qualified_name,
        file_path: req.file,
        timestamp: crate::time::iso8601_now(),
    };
    if let Err(append_err) = log.append(&record) {
        if let Err(undo_err) = std::fs::write(&canonical, &bytes) {
            eprintln!(
                "dkod-mcp write_symbol: post-append undo of {canonical:?} also failed: {undo_err} \
                 (file is now in a partial-commit state — audit-log append failed: {append_err})"
            );
        }
        return Err(append_err.into());
    }

    Ok(WriteSymbolResponse {
        outcome: outcome_label.into(),
        fallback_reason,
        bytes_written,
    })
}
