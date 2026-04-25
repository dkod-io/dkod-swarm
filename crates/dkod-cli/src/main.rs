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
        Command::Init { verify_cmd } => {
            let cwd = std::env::current_dir()?;
            dkod_cli::cmd::init::run(&cwd, verify_cmd)?;
            Ok(())
        }
        Command::Status => {
            let cwd = std::env::current_dir()?;
            dkod_cli::cmd::status::run(&cwd).await?;
            Ok(())
        }
        Command::Abort => {
            let cwd = std::env::current_dir()?;
            dkod_cli::cmd::abort::run(&cwd).await?;
            Ok(())
        }
        Command::Mcp => anyhow::bail!("`dkod --mcp` not yet implemented (PR M3-3)"),
    }
}
