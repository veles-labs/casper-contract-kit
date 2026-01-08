#![cfg_attr(target_arch = "wasm32", no_std)]

use veles_casper_contract_api::prelude::*;

use do_nothing_stored::HASH_KEY;

#[casper(export)]
pub fn call() -> Result<(), ApiError> {
    // Retrieve the contract hash of the deployed do-nothing-stored contract
    let contract_hash = HASH_KEY
        .get()?
        .ok_or(ApiError::MissingKey)?
        .into_hash_addr()
        .ok_or(ApiError::MissingKey)?;

    let client = do_nothing_stored::contract::Client::new(contract_hash.into());

    // Type-safe call to the `delegate` entry point. Any modification on the other side will cause compiler to report issues at all call sites.
    let _result_1: () = client.delegate(U512::from(42u32));

    // This will revert
    // let _result_2: () = client.delegate(U512::one());

    Ok(())
}
