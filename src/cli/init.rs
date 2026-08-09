use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::repo::Repository;

#[derive(Args)]
pub struct InitArgs {
    /// Directory to initialize (defaults to the current directory)
    path: Option<PathBuf>,
}

pub fn run(args: InitArgs) -> Result<()> {
    let path = args.path.unwrap_or_else(|| PathBuf::from("."));
    Repository::init(&path)?;
    println!(
        "Initialized empty chunklog repository in {}",
        path.join(".chunklog").display()
    );
    Ok(())
}
