use clap::{Parser, Subcommand};

/// `dkod` — user-facing CLI for dkod-swarm.
///
/// `dkod --mcp` launches the stdio MCP server (Claude Code is the
/// expected caller). The other subcommands are operator-facing wrappers
/// over the same `dkod-mcp` library helpers, so output matches what the
/// MCP tools return.
#[derive(Debug, Parser)]
#[command(name = "dkod", version, about = "dkod-swarm CLI")]
pub struct Cli {
    /// Stdio MCP-server mode. Mutually exclusive with the subcommands.
    /// We expose `--mcp` as a top-level flag (not a subcommand) so the
    /// invocation matches design §Topology: `dkod-cli --mcp`.
    #[arg(long)]
    pub mcp: bool,

    #[command(subcommand)]
    pub subcommand: Option<RawCommand>,
}

/// Subcommand as parsed from argv, before reconciling with the
/// top-level `--mcp` flag. Use [`Cli::command_resolved`] to collapse
/// this and the flag into the [`Command`] enum the dispatch matches on.
#[derive(Debug, Subcommand)]
pub enum RawCommand {
    /// Initialise `.dkod/` in the current directory.
    Init {
        /// Optional shell command to run before `dkod_pr` opens the PR.
        #[arg(long)]
        verify_cmd: Option<String>,
    },
    /// Print the current session state as JSON.
    Status,
    /// Destroy the active dk-branch and clear session state.
    Abort,
}

/// Resolved command after reconciling the `--mcp` flag with the
/// subcommand: exactly one of the variants below is selected.
#[derive(Debug)]
pub enum Command {
    /// `dkod init` — wrap `dkod_worktree::init_repo`.
    Init { verify_cmd: Option<String> },
    /// `dkod status` — JSON snapshot of the session.
    Status,
    /// `dkod abort` — destroy the active dk-branch.
    Abort,
    /// `dkod --mcp` — stdio MCP server.
    Mcp,
}

impl Cli {
    /// Reconciled view: collapses `--mcp` and the subcommand into a
    /// single enum. Errors if both are set or neither is set.
    pub fn command_resolved(&self) -> Result<Command, &'static str> {
        match (self.mcp, &self.subcommand) {
            (true, Some(_)) => Err("--mcp cannot be combined with a subcommand"),
            (true, None) => Ok(Command::Mcp),
            (false, Some(RawCommand::Init { verify_cmd })) => Ok(Command::Init {
                verify_cmd: verify_cmd.clone(),
            }),
            (false, Some(RawCommand::Status)) => Ok(Command::Status),
            (false, Some(RawCommand::Abort)) => Ok(Command::Abort),
            (false, None) => Err("no subcommand given (try `dkod --help`)"),
        }
    }
}
