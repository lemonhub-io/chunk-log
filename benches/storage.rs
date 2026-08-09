use std::collections::HashMap;
use std::fs;

use chunklog::{FilesystemStore, Repository};
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use tempfile::{tempdir, TempDir};

const CHUNK_BYTES: usize = 256;

/// Builds a world of `size` distinct chunks on a square grid, each a
/// deterministic 256-byte chunk payload.
fn world(size: usize) -> HashMap<(i32, i32), Vec<u8>> {
    let side = (size as f64).sqrt().ceil() as i32;
    let mut chunks = HashMap::new();
    let mut byte = 0u8;
    for x in 0..side {
        for z in 0..side {
            let mut data = Vec::with_capacity(CHUNK_BYTES);
            for _ in 0..CHUNK_BYTES {
                data.push(byte.wrapping_mul(31).wrapping_add(7));
                byte = byte.wrapping_add(1);
            }
            chunks.insert((x, z), data);
        }
    }
    chunks
}

struct BenchRepo {
    repo: Repository<FilesystemStore>,
    _dir: TempDir,
}

/// A repository with one commit of `world`.
fn setup(world: &HashMap<(i32, i32), Vec<u8>>) -> BenchRepo {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.commit(world, "setup").unwrap();
    BenchRepo { repo, _dir: dir }
}

fn commit_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("commit");
    for size in [100, 1000, 10000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("chunks", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let dir = tempdir().unwrap();
                    let repo = Repository::init(dir.path()).unwrap();
                    (repo, world(size), dir)
                },
                |(mut repo, world, _dir)| {
                    repo.commit(&world, "save").unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn load_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("load");
    for size in [100, 1000, 10000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("chunks", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let world = world(size);
                    let setup = setup(&world);
                    (setup, world)
                },
                |(setup, world)| {
                    let loaded = setup.repo.load(setup.repo.head().unwrap()).unwrap();
                    assert_eq!(loaded, world);
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn checkout_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("checkout");
    for size in [100, 1000, 10000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("chunks", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let world = world(size);
                    setup(&world)
                },
                |mut setup| {
                    setup.repo.checkout("main").unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// Baseline: a naive full copy of every chunk to its own file.
fn naive_copy_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("naive_copy");
    for size in [100, 1000, 10000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("chunks", size), &size, |b, &size| {
            let world = world(size);
            let dir = tempdir().unwrap();
            b.iter(|| {
                for ((x, z), data) in &world {
                    fs::write(dir.path().join(format!("{x},{z}")), data).unwrap();
                }
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    commit_bench,
    load_bench,
    checkout_bench,
    naive_copy_bench
);
criterion_main!(benches);
