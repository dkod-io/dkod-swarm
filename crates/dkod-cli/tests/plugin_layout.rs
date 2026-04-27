//! Structural validation for the dkod-swarm plugin layout.
//!
//! These tests don't exercise the plugin's *behaviour* — that is M5's job
//! (e2e smoke against a real Rust sandbox). What they assert is that the
//! plugin directory under `plugin/` is *shaped correctly*: manifests parse,
//! markdown files have the expected frontmatter delimiter, and the
//! plugin.json's `name` field matches what the marketplace.json advertises.
//!
//! The test only runs from the workspace root; it locates files relative
//! to `CARGO_MANIFEST_DIR`.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `crates/dkod-cli`; workspace root is two up.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn marketplace_manifest_is_valid_json_and_names_dkod_swarm() {
    let path = workspace_root().join(".claude-plugin/marketplace.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("parse JSON");
    assert_eq!(parsed["name"], "dkod-swarm", "marketplace name mismatch");
    let plugins = parsed["plugins"].as_array().expect("plugins array");
    assert!(
        plugins.iter().any(|p| p["name"] == "dkod-swarm"),
        "marketplace.plugins must contain a `dkod-swarm` entry"
    );
}

#[test]
fn plugin_manifest_is_valid_json_and_names_dkod_swarm() {
    let path = workspace_root().join("plugin/.claude-plugin/plugin.json");
    let text = std::fs::read_to_string(&path).expect("read plugin.json");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("parse JSON");
    assert_eq!(parsed["name"], "dkod-swarm");
    assert!(parsed["version"].is_string());
    assert!(parsed["description"].is_string());
}

#[test]
fn mcp_config_is_valid_json_and_declares_dkod_swarm_server() {
    let path = workspace_root().join("plugin/.mcp.json");
    let text = std::fs::read_to_string(&path).expect("read .mcp.json");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("parse JSON");
    let servers = parsed["mcpServers"].as_object().expect("mcpServers map");
    assert!(
        servers.contains_key("dkod-swarm"),
        "mcpServers must declare a `dkod-swarm` entry"
    );
}
