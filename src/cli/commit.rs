use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::repo::Repository;

const STAGING_DIR: &str = ".chunklog/staging";

/// Arguments for `chunklog commit`.
#[derive(Args)]
pub struct CommitArgs {
    /// Commit message
    #[arg(short, long)]
    message: String,
}

/// Runs the `commit` subcommand.
pub fn run(args: CommitArgs) -> Result<()> {
    let mut repo = Repository::open(Path::new("."))?;
    let staging = repo.root().join(STAGING_DIR);
    let chunks = read_staged_chunks(&staging)?;
    let hash = repo.commit(&chunks, &args.message)?;
    clear_staging(&staging)?;
    println!("{hash}  {}", args.message);
    Ok(())
}

/// Reads chunk files from the staging directory. Each file is named
/// `<x>,<z>` and its contents become the chunk blob.
fn read_staged_chunks(staging: &Path) -> Result<HashMap<(i32, i32), Vec<u8>>> {
    let mut chunks = HashMap::new();
    if !staging.is_dir() {
        return Ok(chunks);
    }
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let (x, z) = parse_coords(&name)
            .with_context(|| format!("invalid chunk file name in staging: {name}"))?;
        chunks.insert((x, z), fs::read(entry.path())?);
    }
    Ok(chunks)
}

fn parse_coords(name: &str) -> Result<(i32, i32)> {
    let Some((x, z)) = name.split_once(',') else {
        bail!("expected <x>,<z>");
    };
    Ok((x.parse()?, z.parse()?))
}

fn clear_staging(staging: &Path) -> Result<()> {
    if !staging.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && !entry.file_name().to_string_lossy().starts_with('.') {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
