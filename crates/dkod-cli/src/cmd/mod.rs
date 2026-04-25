//! Per-subcommand dispatch. Each module exposes a single `run(...)`
//! function — synchronous (`pub fn run`) when the work is plain
//! filesystem / library calls (e.g. `init`), or asynchronous
//! (`pub async fn run`) when the body needs `.await` (the M3-2 status /
//! abort wrappers acquire the active-session tokio mutex; the M3-3 mcp
//! wrapper drives `serve(stdio()).await`). The dispatch in `main.rs`
//! handles both shapes by `await`ing async fns and calling sync fns
//! directly.

pub mod init;
pub mod status;
