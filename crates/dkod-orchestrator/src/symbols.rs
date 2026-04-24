use crate::{Error, Result};
use dk_core::{RawCallEdge, Symbol};
use dk_engine::parser::{LanguageParser, engine::QueryDrivenParser, langs::rust::RustConfig};
use std::path::Path;

/// Parse a single Rust file and return its symbols + raw call edges.
/// Stateless — the caller batches across files.
///
/// Returns an empty result for empty source without error.
/// Returns `Error::Engine` for parse failures (including non-UTF8 content
/// that tree-sitter cannot process — tree-sitter itself tolerates most
/// byte sequences, but query compilation failures are surfaced here).
pub fn extract_rust_file(source: &[u8], file_path: &Path) -> Result<(Vec<Symbol>, Vec<RawCallEdge>)> {
    // Honour the empty-source contract up-front — skip parser construction
    // entirely so the zero-symbol outcome is cheap and observably deterministic.
    if source.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    // FIXME(M1-6): parser construction compiles tree-sitter queries on every
    // call. M1-6's callgraph builder will parse N files in a loop; hoist the
    // `QueryDrivenParser` construction to a batch-level helper before then.
    let parser = QueryDrivenParser::new(Box::new(RustConfig))
        .map_err(|e| Error::Engine(format!("build parser: {e}")))?;
    let symbols = parser
        .extract_symbols(source, file_path)
        .map_err(|e| Error::Engine(format!("extract_symbols({}): {e}", file_path.display())))?;
    let calls = parser
        .extract_calls(source, file_path)
        .map_err(|e| Error::Engine(format!("extract_calls({}): {e}", file_path.display())))?;
    Ok((symbols, calls))
}
