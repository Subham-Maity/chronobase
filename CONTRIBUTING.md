# Contributing to ChronoBase

Thanks for taking the time to contribute!

## Getting started

```bash
git clone https://github.com/Subham-Maity/chronobase.git
cd chronobase
cargo test --workspace
cargo run -p chronobase
```

On Linux, install the Slint system dependencies first (see `README.md`).

## Project structure

- `crates/core/` — UI-free domain logic, atomic JSON store, backup/restore/wipe engine, cron scheduler, and tool discovery
- `crates/app/` — Slint UI (`.slint` files under `ui/`), CLI/headless entrypoint, and background runtime bridge
- `scripts/` — Cross-platform release packaging scripts
- `v1/` — Read-only archive of the original Node.js/Express version

## Guidelines

- Follow `RULE.md`: typed errors with `thiserror`, no `unwrap()`/`expect()`/`panic!()` outside `main.rs`/`build.rs`/tests, and `///` / `//!` doc comments
- Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` before opening a PR
- Every bug fix must include a regression test

## Reporting bugs

Open an issue with:
1. What you expected to happen
2. What actually happened
3. Your OS, architecture, and `chronobase --version`
4. Relevant logs (`RUST_LOG=debug chronobase`)

