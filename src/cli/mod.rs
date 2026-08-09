//! Command-line interface for chunklog.

/// The `chunklog branch` command: create, list, or delete branches.
pub mod branch;

/// The `chunklog checkout` command: switch branches or commits.
pub mod checkout;

/// The `chunklog diff` command: show world changes between commits.
pub mod diff;

/// The `chunklog gc` command: delete unreachable objects.
pub mod gc;

/// The `chunklog init` command: initialize a repository.
pub mod init;

/// The `chunklog commit` command: commit staged chunks.
pub mod commit;

/// The `chunklog log` command: show commit history.
pub mod log;

use anyhow::Result;
use clap::{Parser, Subcommand};

use self::{
    branch::BranchArgs, checkout::CheckoutArgs, commit::CommitArgs, diff::DiffArgs, init::InitArgs,
};

#[derive(Parser)]
#[command(name = "chunklog", version, about = "Version control for voxel worlds")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create, list, or delete branches
    Branch(BranchArgs),
    /// Switch to a branch or commit
    Checkout(CheckoutArgs),
    /// Show world changes between commits
    Diff(DiffArgs),
    /// Delete unreachable objects
    Gc,
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
        Command::Branch(args) => branch::run(args),
        Command::Checkout(args) => checkout::run(args),
        Command::Diff(args) => diff::run(args),
        Command::Gc => gc::run(),
        Command::Init(args) => init::run(args),
        Command::Commit(args) => commit::run(args),
        Command::Log => log::run(),
    }
}
