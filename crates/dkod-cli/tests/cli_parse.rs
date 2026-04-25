use clap::Parser;
use dkod_cli::cli::{Cli, Command};

#[test]
fn parses_init() {
    let cli = Cli::parse_from(["dkod", "init"]);
    let cmd = cli.command_resolved().unwrap();
    assert!(matches!(cmd, Command::Init { verify_cmd: None }));
}

#[test]
fn parses_init_with_verify_cmd() {
    let cli = Cli::parse_from(["dkod", "init", "--verify-cmd", "cargo test"]);
    let cmd = cli.command_resolved().unwrap();
    let Command::Init { verify_cmd } = cmd else {
        panic!("expected Init")
    };
    assert_eq!(verify_cmd.as_deref(), Some("cargo test"));
}

#[test]
fn parses_status() {
    let cli = Cli::parse_from(["dkod", "status"]);
    let cmd = cli.command_resolved().unwrap();
    assert!(matches!(cmd, Command::Status));
}

#[test]
fn parses_abort() {
    let cli = Cli::parse_from(["dkod", "abort"]);
    let cmd = cli.command_resolved().unwrap();
    assert!(matches!(cmd, Command::Abort));
}

#[test]
fn parses_mcp_flag() {
    let cli = Cli::parse_from(["dkod", "--mcp"]);
    let cmd = cli.command_resolved().unwrap();
    assert!(matches!(cmd, Command::Mcp));
}

#[test]
fn rejects_mcp_with_subcommand() {
    // `--mcp` and a subcommand are mutually exclusive — `command_resolved`
    // surfaces this as an Err so `main.rs` can exit non-zero with a
    // useful message instead of dispatching to the wrong handler.
    let cli = Cli::parse_from(["dkod", "--mcp", "init"]);
    let err = cli.command_resolved().expect_err("expected Err for --mcp + init");
    assert!(
        err.contains("--mcp"),
        "error message should mention --mcp, got: {err}"
    );
}

#[test]
fn rejects_no_mode_selector() {
    // No `--mcp`, no subcommand — `Cli::parse_from(["dkod"])` is accepted by
    // clap because `subcommand` is `Option<RawCommand>`; the resolver is
    // what enforces "exactly one mode" and must error here.
    let cli = Cli::parse_from(["dkod"]);
    let err = cli
        .command_resolved()
        .expect_err("expected Err when neither --mcp nor a subcommand is given");
    assert!(
        err.contains("no subcommand"),
        "error message should hint at the missing subcommand, got: {err}"
    );
}
