use std::path::Path;

use anyhow::Result;
use clap::Args;

use crate::repo::Repository;

/// Arguments for `chunklog checkout`.
#[derive(Args)]
pub struct CheckoutArgs {
    /// Branch name or commit hash to switch to
    target: String,
    /// Create a new branch at the current HEAD and switch to it
    #[arg(short = 'b')]
    create: bool,
}

/// Runs the `checkout` subcommand.
pub fn run(args: CheckoutArgs) -> Result<()> {
    let mut repo = Repository::open(Path::new("."))?;
    if args.create {
        repo.create_branch(&args.target)?;
    }
    let checkout = repo.checkout(&args.target)?;
    match &checkout.branch {
        Some(name) => println!("Switched to branch '{name}'"),
        None => println!("HEAD is now at {} (detached)", checkout.commit),
    }
    Ok(())
}
