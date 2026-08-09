//! Command-line interface for chunklog.

/// The `chunklog init` command: initialize a repository.
pub mod init;

/// The `chunklog commit` command: commit staged chunks.
pub mod commit;

/// The `chunklog log` command: show commit history.
pub mod log;

use anyhow::Result;
use clap::{Parser, Subcommand};

use self::{commit::CommitArgs, init::InitArgs};

#[derive(Parser)]
#[command(name = "chunklog", version, about = "Version control for voxel worlds")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a repository in a directory
    Init(InitArgs),
    /// Commit staged chunks with a message
    Commit(CommitArgs),
    /// Show commit history
    Log,
}

/// Runs the CLI, parsing arguments from the command line.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init(args) => init::run(args),
        Command::Commit(args) => commit::run(args),
        Command::Log => log::run(),
    }
}
