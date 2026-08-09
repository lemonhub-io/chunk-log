use std::path::Path;

use anyhow::Result;

use crate::repo::Repository;

/// Runs the `gc` subcommand.
pub fn run() -> Result<()> {
    let repo = Repository::open(Path::new("."))?;
    let stats = repo.collect_garbage()?;
    println!(
        "Removed {} objects, retained {}",
        stats.removed, stats.retained
    );
    Ok(())
}
