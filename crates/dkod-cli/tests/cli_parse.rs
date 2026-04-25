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
