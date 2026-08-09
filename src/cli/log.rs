use std::path::Path;

use anyhow::Result;

use crate::repo::Repository;

/// Runs the `log` subcommand.
pub fn run() -> Result<()> {
    let repo = Repository::open(Path::new("."))?;
    for entry in repo.log()? {
        println!("{}  {}", entry.hash, entry.message);
    }
    Ok(())
}
