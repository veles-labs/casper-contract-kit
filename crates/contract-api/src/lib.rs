#![cfg_attr(all(not(feature = "std"), not(test)), no_std)]

extern crate alloc;

#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
pub mod binary_port;
pub mod error;
pub mod wasm_support;
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
pub use casper_binary_port;
#[cfg(not(target_arch = "wasm32"))]
pub use casper_client;
pub use casper_contract;
#[cfg(not(target_arch = "wasm32"))]
pub use casper_engine_test_support;
pub use casper_event_standard;
#[cfg(not(target_arch = "wasm32"))]
pub use casper_execution_engine;
#[cfg(not(target_arch = "wasm32"))]
pub use casper_storage;
pub use casper_types;
pub use veles_casper_contract_macros;
#[cfg(not(target_arch = "wasm32"))]
pub use veles_casper_ffi_shim;

#[cfg(feature = "wasm_allocator")]
pub use lol_alloc;

pub mod collections;
pub mod macro_support;
pub mod named_key;
pub mod prelude;
#[cfg(all(not(target_arch = "wasm32"), feature = "std"))]
pub mod sdk;
pub mod typed_uref;
pub mod utils;
