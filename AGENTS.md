# Repository Guidelines

## Build, Test, and Development Commands

- `cargo xtask build-example <package>` builds a single example contract to `wasm32v1-none` in release mode.
- `cargo xtask build-examples` builds all example contracts under `./examples` to `wasm32v1-none` in release mode.
- `cargo check --examples` to ensure examples compile when modifying or adding them.
- `cargo clippy --all --all-targets --all-features` for workspace linting.
- `cargo test -p veles-casper-contract-api --tests` runs contract-api unit tests.

## Coding Style & Naming Conventions
- Format with `cargo fmt --all` (4-space indentation) before reviews.
- Run `cargo clippy --all --all-targets --all-features` before handing off work to keep lints clean across the entire workspace.
- Use `snake_case` for modules/files (e.g., `foo_bar.rs`) and UpperCamelCase for types and events; keep entrypoint names descriptive.
- When adding new modules, prefer the single-file layout (`module_name.rs`) instead of legacy `module_name/mod.rs` folders.

## Repo Layout Notes
- `crates/` contains core libraries (e.g., `contract-api`, `contract-extras`, `contract-macros`, `casper-ffi-shim`).
- When adding/removing crates, update the `Crates` section in `README.md` to keep the list and descriptions in sync.
- `examples/` contains deployable contract examples (built via `cargo xtask`).
- `xtask/` provides the task runner used by `cargo xtask`.
