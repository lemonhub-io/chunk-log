use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;

use crate::repo::Repository;

/// Arguments for `chunklog diff`.
#[derive(Args)]
pub struct DiffArgs {
    /// Commit or branch to diff from (defaults to the empty world)
    from: Option<String>,
    /// Commit or branch to diff to (defaults to HEAD)
    to: Option<String>,
}

/// Runs the `diff` subcommand.
pub fn run(args: DiffArgs) -> Result<()> {
    let repo = Repository::open(Path::new("."))?;
    let (from, to) = match (&args.from, &args.to) {
        (Some(a), Some(b)) => (Some(repo.resolve(a)?), repo.resolve(b)?),
        (Some(a), None) => (
            Some(repo.resolve(a)?),
            repo.head().context("no commits yet")?,
        ),
        (None, Some(b)) => (None, repo.resolve(b)?),
        (None, None) => (None, repo.head().context("no commits yet")?),
    };
    let diff = repo.diff(from, to)?;
    for (coords, hash) in &diff.added {
        println!("+ ({},{}) {}", coords.0, coords.1, hash);
    }
    for (coords, (old, new)) in &diff.modified {
        println!("~ ({},{}) {} -> {}", coords.0, coords.1, old, new);
    }
    for (coords, hash) in &diff.removed {
        println!("- ({},{}) {}", coords.0, coords.1, hash);
    }
    Ok(())
}
