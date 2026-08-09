//! Imports real Luanti mapblock payloads from a generated `map.sqlite`.
//!
//! Usage:
//! `cargo run --release --example luanti_workload -- <sqlite3> <map.sqlite>`

use std::collections::{BTreeMap, HashMap};
use std::env;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use chunklog::{MemoryStore, Object, ObjectStore, Repository, TreeNode};
use tempfile::tempdir;

type Coordinates = (i32, i32);
type VerticalBlocks = Vec<(i32, Vec<u8>)>;

fn decode_hex(text: &str) -> Result<Vec<u8>> {
    if text.len() % 2 != 0 {
        bail!("odd hex payload length");
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair)?, 16).map_err(Into::into))
        .collect()
}

fn read_columns(sqlite: &Path, database: &Path) -> Result<HashMap<Coordinates, Vec<u8>>> {
    let query = "SELECT x, y, z, hex(data) FROM blocks ORDER BY x, z, y;";
    let output = Command::new(sqlite)
        .arg("-separator")
        .arg("|")
        .arg(database)
        .arg(query)
        .output()
        .with_context(|| format!("failed to execute {}", sqlite.display()))?;
    if !output.status.success() {
        bail!(
            "sqlite3 failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8(output.stdout)?;
    let mut blocks: BTreeMap<Coordinates, VerticalBlocks> = BTreeMap::new();
    for (line_number, line) in text.lines().enumerate() {
        let fields: Vec<_> = line.split('|').collect();
        if fields.len() != 4 {
            bail!("invalid sqlite output on line {}", line_number + 1);
        }
        let x: i32 = fields[0].parse()?;
        let y: i32 = fields[1].parse()?;
        let z: i32 = fields[2].parse()?;
        blocks
            .entry((x, z))
            .or_default()
            .push((y, decode_hex(fields[3])?));
    }

    let mut columns = HashMap::new();
    for (coords, mut vertical) in blocks {
        vertical.sort_by_key(|(y, _)| *y);
        let mut payload = Vec::new();
        payload.extend_from_slice(&(vertical.len() as u32).to_be_bytes());
        for (y, block) in vertical {
            payload.extend_from_slice(&y.to_be_bytes());
            payload.extend_from_slice(&(block.len() as u64).to_be_bytes());
            payload.extend_from_slice(&block);
        }
        columns.insert(coords, payload);
    }
    Ok(columns)
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() != 3 {
        bail!("usage: luanti_workload <sqlite3 executable> <map.sqlite>");
    }
    let columns = read_columns(Path::new(&args[1]), Path::new(&args[2]))?;
    if columns.is_empty() {
        bail!("Luanti database contains no mapblocks");
    }
    let payload_bytes: usize = columns.values().map(Vec::len).sum();
    let unique_payloads = columns
        .values()
        .map(|payload| blake3::hash(payload))
        .collect::<std::collections::HashSet<_>>()
        .len();

    let dir = tempdir()?;
    let mut repo = Repository::init_with(MemoryStore::new(), dir.path())?;
    let start = Instant::now();
    repo.commit_snapshot(&columns, "Luanti-generated mapblocks")?;
    let elapsed = start.elapsed();

    let mut blobs = 0usize;
    let mut branches = 0usize;
    let mut leaves = 0usize;
    let mut commits = 0usize;
    let mut canonical_bytes = 0usize;
    for hash in repo.store().list()? {
        let bytes = repo.store().read(hash)?;
        canonical_bytes += bytes.len();
        match Object::from_bytes(&bytes)? {
            Object::Blob(_) => blobs += 1,
            Object::Tree(TreeNode::Branch(_)) => branches += 1,
            Object::Tree(TreeNode::Leaf { .. }) => leaves += 1,
            Object::Commit(_) => commits += 1,
        }
    }

    println!("# Luanti-generated workload");
    println!("columns={}", columns.len());
    println!("payload_bytes={payload_bytes}");
    println!("unique_payloads={unique_payloads}");
    println!("snapshot_ms={:.3}", elapsed.as_secs_f64() * 1_000.0);
    println!("blobs={blobs}");
    println!("branches={branches}");
    println!("leaves={leaves}");
    println!("commits={commits}");
    println!("canonical_bytes={canonical_bytes}");
    Ok(())
}
