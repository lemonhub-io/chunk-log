# Luanti-generated workload result

Generated: 2026-08-09  
Engine: official Luanti 5.16.1 Windows x64 release  
Game: `paper-workloads/luanti-game`  
World template: `paper-workloads/luanti-world`  
Database: Luanti SQLite map backend; database itself is generated and not committed  
Importer: `examples/luanti_workload.rs`  
chunklog backend: verified `MemoryStore`  

The minimal game registers one singlenode mapgen node, requests a deterministic area through Luanti's `emerge_area`, waits for all callbacks, and requests a clean server shutdown. Luanti generated 2,023 serialized mapblocks. The importer groups all vertical mapblocks with the same `(x,z)` mapblock coordinate into one opaque column payload, preserving y values, block lengths, and exact engine-emitted bytes.

```text
columns=289
payload_bytes=116477
unique_payloads=2
snapshot_ms=7.275
blobs=2
branches=270
leaves=289
commits=1
canonical_bytes=34758
```

## Interpretation

This is a real-engine serialization test but not a production-player trace. The singlenode generator intentionally creates highly repetitive columns, so only two of 289 aggregated payloads are unique. The result demonstrates that chunklog accepts Luanti-emitted binary data as opaque payloads and that identical serialized columns share Blob objects. It does not estimate edit density, production terrain entropy, multiplayer behavior, or end-user latency.

The 7.275 ms snapshot is one observation, not a Criterion distribution, and excludes Luanti generation time and SQLite extraction time. It is reported as an integration result rather than a performance comparison.
