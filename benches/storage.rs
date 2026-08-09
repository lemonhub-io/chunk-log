use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::Duration;

use chunklog::{ChangeSet, FilesystemStore, MemoryStore, Repository, SqliteStore};
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use tempfile::{tempdir, TempDir};

const CHUNK_BYTES: usize = 256;

fn world(size: usize) -> HashMap<(i32, i32), Vec<u8>> {
    let world: HashMap<_, _> = (0..size)
        .map(|index| {
            let index_u64 = index as u64;
            let mut payload = vec![0u8; CHUNK_BYTES];
            payload[..8].copy_from_slice(&index_u64.to_be_bytes());
            let mut state = index_u64 ^ 0x9e37_79b9_7f4a_7c15;
            for byte in &mut payload[8..] {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                *byte = (state >> 56) as u8;
            }
            ((index as i32, (index as i32).wrapping_mul(3)), payload)
        })
        .collect();
    assert_eq!(world.len(), size);
    assert_eq!(world.values().collect::<HashSet<_>>().len(), size);
    world
}

struct DurableRepo {
    repo: Repository<SqliteStore>,
    _dir: TempDir,
}

fn setup_durable(world: &HashMap<(i32, i32), Vec<u8>>) -> DurableRepo {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.commit_snapshot(world, "setup").unwrap();
    DurableRepo { repo, _dir: dir }
}

fn full_snapshot_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("algorithm/full_snapshot_memory");
    for size in [100, 1_000, 10_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("chunks", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let dir = tempdir().unwrap();
                    let repo = Repository::init_with(MemoryStore::new(), dir.path()).unwrap();
                    (repo, world(size), dir)
                },
                |(mut repo, world, _dir)| {
                    repo.commit_snapshot(&world, "save").unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn full_snapshot_sqlite(c: &mut Criterion) {
    let mut group = c.benchmark_group("durable_io/full_snapshot_sqlite");
    for size in [100, 1_000, 10_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("chunks", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let dir = tempdir().unwrap();
                    let repo = Repository::init(dir.path()).unwrap();
                    (repo, world(size), dir)
                },
                |(mut repo, world, _dir)| {
                    repo.commit_snapshot(&world, "save").unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn full_snapshot_loose(c: &mut Criterion) {
    let mut group = c.benchmark_group("loose_io/full_snapshot_filesystem");
    for size in [100, 1_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("chunks", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let dir = tempdir().unwrap();
                    let repo = Repository::<FilesystemStore>::init_loose(dir.path()).unwrap();
                    (repo, world(size), dir)
                },
                |(mut repo, world, _dir)| {
                    repo.commit_snapshot(&world, "save").unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn incremental_commit(c: &mut Criterion) {
    let mut group = c.benchmark_group("durable_io/incremental_commit_k1");
    for size in [100, 1_000] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("world_chunks", size), &size, |b, &size| {
            let mut setup = setup_durable(&world(size));
            let mut generation = 0u64;
            b.iter(|| {
                generation += 1;
                let mut changes = ChangeSet::new();
                changes.upsert((0, 0), generation.to_be_bytes().to_vec());
                setup.repo.commit_changes(&changes, "one change").unwrap();
            });
        });
    }
    group.finish();
}

fn load_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("durable_io/load");
    for size in [100, 1_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("chunks", size), &size, |b, &size| {
            let expected = world(size);
            let setup = setup_durable(&expected);
            let head = setup.repo.head().unwrap();
            b.iter(|| {
                let loaded = setup.repo.load(head).unwrap();
                assert_eq!(loaded, expected);
            });
        });
    }
    group.finish();
}

fn checkout_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("durable_io/logical_checkout");
    for size in [100, 1_000] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("world_chunks", size), &size, |b, &size| {
            let mut setup = setup_durable(&world(size));
            setup.repo.create_branch("feature").unwrap();
            let mut on_main = true;
            b.iter(|| {
                let target = if on_main { "feature" } else { "main" };
                setup.repo.checkout(target).unwrap();
                on_main = !on_main;
            });
        });
    }
    group.finish();
}

fn naive_copy_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("baseline/naive_full_snapshot");
    for size in [100, 1_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("chunks", size), &size, |b, &size| {
            b.iter_batched(
                || (world(size), tempdir().unwrap()),
                |(world, dir)| {
                    for ((x, z), data) in &world {
                        fs::write(dir.path().join(format!("{x},{z}")), data).unwrap();
                    }
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets =
        full_snapshot_memory,
        full_snapshot_sqlite,
        full_snapshot_loose,
        incremental_commit,
        load_bench,
        checkout_bench,
        naive_copy_bench
}
criterion_main!(benches);
