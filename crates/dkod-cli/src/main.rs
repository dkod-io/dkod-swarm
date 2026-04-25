use clap::Parser;
use dkod_cli::cli::{Cli, Command};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("dkod fatal: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cmd = cli.command_resolved().map_err(anyhow::Error::msg)?;
    match cmd {
        Command::Init { verify_cmd: _ } => {
            anyhow::bail!("`dkod init` not yet implemented (Task 3)")
        }
        Command::Status => anyhow::bail!("`dkod status` not yet implemented (PR M3-2)"),
        Command::Abort => anyhow::bail!("`dkod abort` not yet implemented (PR M3-2)"),
        Command::Mcp => anyhow::bail!("`dkod --mcp` not yet implemented (PR M3-3)"),
    }
}
