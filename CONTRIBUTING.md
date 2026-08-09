# Contributing to chunklog

Thanks for your interest in contributing to chunklog, a version-control
library for voxel worlds.

## Getting started

Prerequisites: Rust 1.86+ (see `rust-version` in `Cargo.toml`).

```sh
cargo build          # build the library and CLI
cargo test           # run unit/integration tests and doctests
cargo clippy --all-targets -- -D warnings   # lint (must be clean)
cargo fmt --all -- --check                   # formatting (must be clean)
cargo doc --no-deps  # API documentation
cargo bench          # benchmarks (criterion)
cargo run --example simple_game_integration # end-to-end demo
```

CI runs all of the above on every push, plus an MSRV (1.86) check and
coverage. Make sure everything is green locally before opening a PR.

## Reporting issues

- **Bugs**: use the bug report template. Include a minimal reproduction,
  the expected vs. actual behavior, and your environment (OS, Rust
  version).
- **Feature requests**: use the feature request template. Explain the
  problem you are solving, not just the solution you want.
- **Security vulnerabilities**: do not open a public issue; report them
  privately via GitHub's security advisory feature (see `SECURITY.md`).

## Submitting changes

1. Open an issue describing the change, or comment on an existing one.
2. Keep pull requests small and focused. One logical change per PR.
3. Follow the existing code style and module layout; mimic surrounding
   code.
4. Add or update tests for every change. New public API must have
   rustdoc documentation (`#![warn(missing_docs)]` enforces this).
5. Run the checks listed above and make sure they pass.
6. Write a concise commit message in the imperative mood (e.g.
   "Add checkout of detached commits").

## Design principles

- Choose the simplest implementation that fully meets the requirement.
  Avoid speculative abstraction and configuration.
- Reuse existing components before adding new ones.
- Keep concerns separated: object model (`object`), storage backends
  (`store`), high-level repository operations (`repo`).
- Fail loudly when assumptions are violated. Do not silently ignore
  errors.
- Do not add compatibility layers for obsolete paths; remove them.

## License

By contributing you agree that your contributions are licensed under
[MIT OR Apache-2.0](LICENSE-MIT).
