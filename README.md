# Casper Universal API

Workspace providing `veles-casper-contract-api` and companion crates to build Casper smart contracts with a single import and a more ergonomic, safe-by-default API.

## Why this library
- One library to import; core Casper crates are re-exported and kept in sync (e.g., `casper-contract`, `casper-types`, `casper-event-standard`). Note: re-exports are a convenience and may diverge in the future.
- Works with Rust workspaces out of the box (single set of dependency versions for all crates and examples).
- Stable Rust only; no nightly required.
- Targets the MVP-only Wasm backend (`wasm32v1-none`) to avoid unsupported opcodes.
- Better debugging story: compile-time log enable/disable via `enable_casper_log` cfg or `ENABLE_CASPER_LOG` env var.
- Higher-level entrypoints: no more `extern "C"` + `#[no_mangle]` thanks to `#[casper(...)]`.
- Typed contract-to-contract calls so breaking changes surface at compile time.
- Automatic binding for named args and return values via generated `Args` and typed `Client` methods.
- Fewer footguns via typed helpers and safer defaults (named keys, typed URefs, base128 dictionary keys).

Some of these ergonomics are not available in the official crates alone without this workspace.

This workspace builds on top of the official Casper crates and aims to be a respectful, additive layer: ergonomic macros, safer helpers, and common utilities while preserving the core model and APIs.
Unlike some smart contract development tools, it does not force a particular coding style or try to be a full-fledged framework. It follows the established "program with functions" paradigm and makes it more convenient.

## What you get
- A `prelude` that centralizes common imports: runtime/storage, core types, macros, and helpers.
- `#[casper(contract)]` and `#[casper(export)]` macros that generate `entry_points()`, `Client`, `Args`, `NAME`, and `IntoRuntimeArgs` glue.
- State helpers: `NamedKey`, `TypedURef`, `len_prefixed!`, dictionary read/write helpers, base128 dictionary keys, immediate caller/entity access.
- High-level collections on dictionaries (`Mapping`, `Set`, `Vector`) plus dictionary-key helpers.
- Events/messages: `CasperMessage` derive + `emit_message` helper.
- Host-side support (non-Wasm): `casper-ffi-shim`, test support, and `veles-casper-rust-sdk` for JSON-RPC + SSE streams (std only).

## Crates
- `veles-casper-contract-api`: main API surface, re-exports, and utilities.
- `veles-casper-contract-macros`: procedural macros for entrypoints, args, and clients.
- `veles-casper-contract-extras`: common contract building blocks.
- `veles-casper-ffi-shim`: non-Wasm bindings for testing and tooling.
- `veles-casper-rust-sdk`: host-side Rust SDK utilities (JSON-RPC wrapper, SSE listener/stream, transaction helpers).

## Repository layout
- Crates live in `./crates`.
- Example smart contracts live in `./examples`.

## Getting started
Add the dependency (use `workspace = true` if you're already in this repo workspace):

```toml
[dependencies]
# Before crates.io release
veles-casper-contract-api = { path = "../casper-contract-kit/crates/contract-api" }
```

Create a contract module with exported entrypoints:

```rust
#![cfg_attr(target_arch = "wasm32", no_std)]
use veles_casper_contract_api::prelude::*;

#[casper(contract)]
pub mod hello {
    use super::*;

    #[casper(export)]
    pub fn who_am_i() -> Result<AccountHash, ApiError> {
        utils::get_immediate_account()
    }
}
```

Macro-generated API (see `examples/do-nothing-stored` and `examples/do-nothing-caller`):
- `#[casper(contract)]` generates `contract::Client` with type-safe methods; each method wraps a `call_contract` host call and returns the typed result to the caller.
- Every `#[casper(export)]` entrypoint gets a module like `contract::delegate` that exposes `NAME` and `Args { ... }` (used in tests with `ExecuteRequestBuilder::contract_call_by_hash` and `IntoRuntimeArgs`).

When a contract is imported by another contract, enable the `as_dependency` feature on the dependency (see `examples/do-nothing-caller/Cargo.toml`). This prevents exporting Wasm entrypoints from the dependency while still generating `Client`, `Args`, and `NAME` for type-safe calls and compile-time breakage on interface changes.

Build for Casper:

```sh
rustup target add wasm32v1-none
cargo build --target wasm32v1-none --release
```

## Debug logging
Enable logs at compile time:

```sh
RUSTFLAGS="--cfg enable_casper_log" cargo build --target wasm32v1-none
# or
ENABLE_CASPER_LOG=1 cargo build --target wasm32v1-none
```

## Examples

```sh
cargo build -p do-nothing-stored --target wasm32v1-none
```

- `do-nothing-stored`: minimal stored contract with messages and named keys.
- `do-nothing-caller`: contract that imports the stored contract and uses the generated `Client` (via `as_dependency`).
- The `do-nothing-stored` tests expect `target/wasm32v1-none/release/do_nothing_stored.wasm` to exist.

## Roadmap
- Build tool for smart contracts (all-in-one deploy/call/manage accounts with best-intention defaults).
- Standalone dev server similar to Hardhat/Ganache (no more cctl/nctl).
