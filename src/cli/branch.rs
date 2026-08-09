use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;

use crate::repo::Repository;

/// Arguments for `chunklog branch`.
#[derive(Args)]
pub struct BranchArgs {
    /// Name of the branch to create
    name: Option<String>,
    /// Delete the named branch
    #[arg(short = 'd')]
    delete: bool,
}

/// Runs the `branch` subcommand.
pub fn run(args: BranchArgs) -> Result<()> {
    let mut repo = Repository::open(Path::new("."))?;
    if args.delete {
        let name = args.name.context("branch name required for deletion")?;
        repo.delete_branch(&name)?;
        println!("Deleted branch '{name}'");
        return Ok(());
    }
    if let Some(name) = args.name {
        repo.create_branch(&name)?;
        println!("Created branch '{name}'");
        return Ok(());
    }
    let current = repo.current_branch().map(str::to_string);
    for branch in repo.branches()? {
        let marker = if current.as_deref() == Some(branch.name.as_str()) {
            "*"
        } else {
            " "
        };
        println!("{marker} {}", branch.name);
    }
    Ok(())
}
