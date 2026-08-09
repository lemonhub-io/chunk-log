use std::path::Path;

use anyhow::Result;

use crate::repo::Repository;

pub fn run() -> Result<()> {
    let repo = Repository::open(Path::new("."))?;
    for (hash, message) in repo.log()? {
        println!("{hash}  {message}");
    }
    Ok(())
}
