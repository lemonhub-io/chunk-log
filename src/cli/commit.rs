use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::repo::{ChangeSet, Repository};

const STAGING_DIR: &str = ".chunklog/staging";
const REMOVALS_FILE: &str = ".remove";

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
    let changes = read_staged_changes(&staging)?;
    if changes.is_empty() {
        bail!("nothing staged; add <x>,<z> files or coordinates to .remove");
    }
    let hash = repo.commit_changes(&changes, &args.message)?;
    clear_staging(&staging)?;
    println!("{hash}  {}", args.message);
    Ok(())
}

/// Reads an incremental patch from staging.
///
/// Files named `<x>,<z>` are upserts. The optional `.remove` file contains
/// one `<x>,<z>` coordinate per non-empty line.
fn read_staged_changes(staging: &Path) -> Result<ChangeSet> {
    let mut changes = ChangeSet::new();
    if !staging.is_dir() {
        return Ok(changes);
    }
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.as_ref() == REMOVALS_FILE {
            let contents = fs::read_to_string(entry.path())?;
            for (index, line) in contents.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let coords = parse_coords(line).with_context(|| {
                    format!("invalid coordinate on {} line {}", REMOVALS_FILE, index + 1)
                })?;
                if changes.upserts.contains_key(&coords) {
                    bail!("coordinate {coords:?} is both staged and removed");
                }
                changes.removals.insert(coords);
            }
            continue;
        }
        if name.starts_with('.') {
            continue;
        }
        let coords = parse_coords(&name)
            .with_context(|| format!("invalid chunk file name in staging: {name}"))?;
        if changes.removals.contains(&coords) {
            bail!("coordinate {coords:?} is both staged and removed");
        }
        changes.upserts.insert(coords, fs::read(entry.path())?);
    }
    Ok(changes)
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
        if entry.file_type()?.is_file() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with('.') || name.as_ref() == REMOVALS_FILE {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn staging_is_an_incremental_patch() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("1,-2"), [7, 8]).unwrap();
        fs::write(dir.path().join(REMOVALS_FILE), "3,4\n# comment\n").unwrap();
        let changes = read_staged_changes(dir.path()).unwrap();
        assert_eq!(changes.upserts.get(&(1, -2)).unwrap(), &[7, 8]);
        assert!(changes.removals.contains(&(3, 4)));
    }

    #[test]
    fn conflicting_staging_entries_fail() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("1,2"), [7]).unwrap();
        fs::write(dir.path().join(REMOVALS_FILE), "1,2\n").unwrap();
        assert!(read_staged_changes(dir.path()).is_err());
    }
}
