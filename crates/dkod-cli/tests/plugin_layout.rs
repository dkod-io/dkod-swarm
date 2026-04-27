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
    let server = &parsed["mcpServers"]["dkod-swarm"];
    assert!(
        server.is_object(),
        "mcpServers must declare a `dkod-swarm` entry"
    );
    assert_eq!(
        server["command"], "cargo",
        "dkod-swarm MCP command must be `cargo` so CLAUDE_PLUGIN_ROOT/../Cargo.toml resolves"
    );
    let args = server["args"].as_array().expect("dkod-swarm args array");
    assert!(
        args.iter().any(|v| v == "--mcp"),
        "dkod-swarm args must pass `--mcp` so dkod-cli enters stdio MCP mode"
    );
}

#[test]
fn skill_md_has_frontmatter_and_name_field() {
    let path = workspace_root().join("plugin/skills/dkod-swarm/SKILL.md");
    let text = std::fs::read_to_string(&path).expect("read SKILL.md");
    assert!(
        text.starts_with("---\n"),
        "SKILL.md must start with a YAML frontmatter delimiter `---`"
    );
    // Find the closing `---` and pull the frontmatter slice.
    let after_open = &text[4..];
    let close_idx = after_open
        .find("\n---")
        .expect("SKILL.md frontmatter has no closing delimiter");
    let frontmatter = &after_open[..close_idx];
    assert!(
        frontmatter.contains("name: dkod-swarm"),
        "SKILL.md frontmatter must declare `name: dkod-swarm`; got:\n{frontmatter}"
    );
    assert!(
        frontmatter.contains("description:"),
        "SKILL.md frontmatter must include a description"
    );
}

fn assert_md_frontmatter_has(path: &std::path::Path, required_keys: &[&str]) {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    assert!(
        text.starts_with("---\n"),
        "{} must start with `---` frontmatter delimiter",
        path.display()
    );
    let after_open = &text[4..];
    let close_idx = after_open
        .find("\n---")
        .unwrap_or_else(|| panic!("{} has no closing `---`", path.display()));
    let frontmatter = &after_open[..close_idx];
    for key in required_keys {
        // Match against the start of a frontmatter line (after any leading
        // indentation) so a key buried inside a value or comment cannot
        // false-positive. `key` is expected to look like `name:` or
        // `name: parallel-executor` — both shapes work because the check
        // is "line starts with key".
        let has_key = frontmatter
            .lines()
            .any(|line| line.trim_start().starts_with(key));
        assert!(
            has_key,
            "{} frontmatter missing `{key}` as a line; got:\n{frontmatter}",
            path.display()
        );
    }
}

#[test]
fn slash_command_files_have_description_frontmatter() {
    let dir = workspace_root().join("plugin/commands");
    for name in ["plan.md", "execute.md", "pr.md"] {
        assert_md_frontmatter_has(&dir.join(name), &["description:"]);
    }
}

#[test]
fn parallel_executor_agent_has_required_frontmatter() {
    let path = workspace_root().join("plugin/agents/parallel-executor.md");
    assert_md_frontmatter_has(
        &path,
        &["name: parallel-executor", "description:", "model:"],
    );
}
