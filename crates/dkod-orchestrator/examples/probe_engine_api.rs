//! Probe-only binary. Confirms the public `dk_engine` parser API shape on
//! the version pinned in `Cargo.toml`. Run with:
//!   cargo run --example probe_engine_api -p dkod-orchestrator
//!
//! Not a test — pure smoke. Delete or keep as a diagnostics tool after M1.

use dk_engine::parser::{LanguageParser, engine::QueryDrivenParser, langs::rust::RustConfig};
use std::path::Path;

fn main() {
    let src = b"pub fn hello() -> &'static str { \"hi\" }\n";
    // CONFIRMED API on 0.3.x: QueryDrivenParser::new takes Box<dyn LanguageConfig>
    // and returns Result<Self>. The plan assumed bare RustConfig (infallible);
    // actual signature is: pub fn new(config: Box<dyn LanguageConfig>) -> Result<Self>.
    let parser = QueryDrivenParser::new(Box::new(RustConfig)).expect("build parser");
    let syms = parser
        .extract_symbols(src, Path::new("probe.rs"))
        .expect("extract_symbols");
    println!("found {} symbols", syms.len());
    for s in &syms {
        println!("  {} ({:?}) span={:?}", s.qualified_name, s.kind, s.span);
    }
}
