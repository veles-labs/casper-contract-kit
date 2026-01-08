use std::{
    fs,
    path::{Path, PathBuf},
};

use once_cell::sync::Lazy;
use veles_casper_contract_api::macro_support::IntoRuntimeArgs;
use veles_casper_contract_api::{
    casper_engine_test_support::{
        DEFAULT_ACCOUNT_ADDR, ExecuteRequestBuilder, LOCAL_GENESIS_REQUEST, LmdbWasmTestBuilder,
    },
    casper_types::{self, Key, contracts::ContractHash},
};

pub const PROFILE: &str = "release";
pub const WASM_TARGET: &str = "wasm32v1-none";

pub static RUST_WORKSPACE_PATH: Lazy<PathBuf> = Lazy::new(|| {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CARGO_MANIFEST_DIR should have parent")
        .parent()
        .expect("workspace root should have parent")
        .to_path_buf()
        .to_path_buf()
});
// The location of compiled Wasm files if compiled from the Rust sources within the casper-node
// repo, i.e. 'casper-node/target/wasm32v1-none/release/'.
pub static RUST_WORKSPACE_WASM_PATH: Lazy<PathBuf> = Lazy::new(|| {
    RUST_WORKSPACE_PATH
        .join("target")
        .join(WASM_TARGET)
        .join(PROFILE)
});

static DO_NOTHING_STORED_WASM: Lazy<Vec<u8>> = Lazy::new(|| {
    fs::read(RUST_WORKSPACE_WASM_PATH.join("do_nothing_stored.wasm")).unwrap_or_else(|err| {
        panic!(
            "should read {:?} from target dir: {err}",
            RUST_WORKSPACE_WASM_PATH.clone(),
        );
    })
});

#[test]
fn install_and_execute() {
    let args = do_nothing_stored::contract::delegate::Args {
        amount: casper_types::U512::from(42u64),
    };
    let mut builder = LmdbWasmTestBuilder::default();
    builder.run_genesis(LOCAL_GENESIS_REQUEST.clone());
    let contract_hash = install_do_nothing_stored_contract(&mut builder);

    call_delegate(&mut builder, contract_hash, args);
}

fn install_do_nothing_stored_contract(builder: &mut LmdbWasmTestBuilder) -> ContractHash {
    let do_nothing_stored_wasm = DO_NOTHING_STORED_WASM.clone();

    let install_request = ExecuteRequestBuilder::module_bytes(
        *DEFAULT_ACCOUNT_ADDR,
        do_nothing_stored_wasm,
        casper_types::RuntimeArgs::default(),
    )
    .build();

    builder.exec(install_request).expect_success().commit();

    let installer = builder
        .get_account(*DEFAULT_ACCOUNT_ADDR)
        .expect("installer should exist");
    let Key::Hash(contract_hash_bytes) = installer
        .named_keys()
        .get(do_nothing_stored::HASH_KEY_NAME)
        .expect("missing oracle contract key")
    else {
        panic!("do_nothing_stored contract hash key should exist");
    };

    ContractHash::from(*contract_hash_bytes)
}

fn call_delegate(
    builder: &mut LmdbWasmTestBuilder,
    contract_hash: ContractHash,
    args: do_nothing_stored::contract::delegate::Args,
) {
    let exec_request = ExecuteRequestBuilder::contract_call_by_hash(
        *DEFAULT_ACCOUNT_ADDR,
        contract_hash.into(),
        do_nothing_stored::contract::delegate::NAME,
        args.into_runtime_args(),
    );

    builder.exec(exec_request.build()).expect_success().commit();
}
