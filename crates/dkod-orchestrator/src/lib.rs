//! `dk-engine 0.3.x` exposes: `QueryDrivenParser::new(Box<dyn LanguageConfig>)`,
//! returning `Result<QueryDrivenParser>`. Implements `LanguageParser` with
//! `extract_symbols` and `extract_calls` returning
//! `Result<Vec<dk_core::{Symbol, RawCallEdge}>>`. The plan assumed an infallible
//! bare-value constructor; actual signature wraps the config in a `Box` and is
//! fallible. Confirmed by `examples/probe_engine_api.rs`.

pub mod callgraph;
pub mod error;
pub mod symbols;
pub use error::{Error, Result};
