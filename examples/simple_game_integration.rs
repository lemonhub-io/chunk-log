//! A minimal headless voxel game integrated with chunklog.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example simple_game_integration
//! ```
//!
//! Demonstrates the full workflow a game would use: save, incremental
//! save (deduplication), rollback, branching, and garbage collection.

use std::collections::HashMap;

use anyhow::Result;
use chunklog::{Hash, ObjectStore, Repository};

/// Chunk edge length in blocks.
const CHUNK_SIZE: i32 = 16;

/// A minimal voxel world held in memory.
///
/// Chunk data is simulated "compressed" bytes: a deterministic height
/// field. In a real game these bytes would be the compressed chunk
/// payload produced by the engine.
struct VoxelGame {
    chunks: HashMap<(i32, i32), Vec<u8>>,
    seed: u64,
}

impl VoxelGame {
    fn new(seed: u64) -> Self {
        Self {
            chunks: HashMap::new(),
            seed,
        }
    }

    /// Deterministic noise in `0..8`, used as terrain height.
    fn height(&mut self, x: i32, z: i32) -> u8 {
        self.seed = self
            .seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let coord = (x as u32).wrapping_mul(0x9E37_79B1) ^ (z as u32).wrapping_mul(0x85EB_CA77);
        let mixed = self
            .seed
            .wrapping_add(coord as u64)
            .wrapping_mul(0x2545_F491_4F6C_DD1D);
        (mixed >> 32) as u8 % 8
    }

    /// Builds the chunk at `(x, z)` if it does not exist yet.
    fn ensure_chunk(&mut self, x: i32, z: i32) {
        if !self.chunks.contains_key(&(x, z)) {
            let mut data = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);
            for bx in 0..CHUNK_SIZE {
                for bz in 0..CHUNK_SIZE {
                    data.push(self.height(x * CHUNK_SIZE + bx, z * CHUNK_SIZE + bz));
                }
            }
            self.chunks.insert((x, z), data);
        }
    }

    /// Ensures a square world of `radius` chunks around the origin.
    fn explore(&mut self, radius: i32) {
        for x in -radius..radius {
            for z in -radius..radius {
                self.ensure_chunk(x, z);
            }
        }
    }

    /// Modifies the chunk at `(x, z)` by raising all heights by one.
    fn build(&mut self, x: i32, z: i32) {
        let data = self.chunks.get_mut(&(x, z)).expect("chunk must exist");
        for byte in data {
            *byte = byte.saturating_add(1);
        }
    }

    /// The number of chunks currently held in memory.
    fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Saves the current world as a commit.
    fn save<S: ObjectStore>(&mut self, repo: &mut Repository<S>, message: &str) -> Result<Hash> {
        repo.commit(&self.chunks, message)
    }
}

fn main() -> Result<()> {
    let dir = std::env::temp_dir().join("chunklog-simple-game");
    let _ = std::fs::remove_dir_all(&dir);

    let mut repo = Repository::init(&dir)?;
    let mut game = VoxelGame::new(0xC0FFEE);

    println!("== chunklog x simple voxel game ==");

    // First save: a 4x4 world (16 chunks).
    game.explore(2);
    let first = game.save(&mut repo, "explored 4x4 world")?;
    println!("saved {} chunks: {first}", game.chunk_count());

    // Second save after editing two chunks. Only those two chunks
    // produce new blobs; the other 14 are deduplicated.
    game.build(0, 0);
    game.build(1, 1);
    let second = game.save(&mut repo, "raised two hills")?;
    println!(
        "second save: {} objects in store (naive full copy would be {} chunk files)",
        repo.store().list()?.len(),
        game.chunk_count() * 2
    );

    // Rollback: load the world as of the first save.
    let rolled_back = repo.load(first)?;
    println!(
        "rollback: chunk (0,0) terrain height {} (first save), {} (second save)",
        rolled_back[&(0, 0)][0],
        repo.load(second)?[&(0, 0)][0]
    );

    // Experiment on a side branch.
    repo.create_branch("experiment")?;
    repo.checkout("experiment")?;
    game.build(-1, -1);
    let experiment = game.save(&mut repo, "experiment: raised a hill")?;
    println!("experiment committed: {experiment}");

    // Back to main: the experiment is unreachable from here.
    repo.checkout("main")?;
    println!(
        "switched to 'main', world unchanged ({} chunks)",
        repo.load(second)?.len()
    );

    // Garbage collection reclaims the experiment's objects.
    repo.delete_branch("experiment")?;
    let stats = repo.collect_garbage()?;
    println!("gc: removed {}, retained {}", stats.removed, stats.retained);

    let _ = std::fs::remove_dir_all(&dir);
    println!("== done ==");
    Ok(())
}
