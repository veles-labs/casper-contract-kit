#![cfg_attr(target_arch = "wasm32", no_std)]

pub(crate) mod event;

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::{format, string::String};

use veles_casper_contract_api::prelude::*;

pub const HASH_KEY_NAME: &str = "do_nothing_hash";
pub static HASH_KEY: NamedKey = NamedKey::from_name(HASH_KEY_NAME);
pub const PACKAGE_HASH_KEY_NAME: &str = "do_nothing_package_hash";
pub const ACCESS_KEY_NAME: &str = "do_nothing_access";
pub static CONTRACT_VERSION_KEY: NamedKey = NamedKey::from_name("contract_version");

#[casper(contract)]
pub mod contract {
    use super::*;

    #[casper(export)]
    pub fn delegate(amount: U512) -> Result<(), ApiError> {
        if amount == U512::one() {
            Err(ApiError::User(50000))
        } else {
            let did_nothing = event::DidNothing {
                caller: utils::get_immediate_entity_addr()?
                    .ok_or(ApiError::InvalidCallerInfoRequest)?,
                amount,
            };

            utils::emit_message(did_nothing)?;

            Ok(())
        }
    }

    #[casper(export)]
    pub fn hello(who: String) -> Result<String, ApiError> {
        if who.is_empty() {
            return Err(ApiError::User(50001));
        }

        Ok(format!("Hello, {who}!"))
    }

    #[casper(export)]
    pub fn add(lhs: u64, rhs: u64) -> Result<u64, ApiError> {
        Ok(lhs + rhs)
    }

    #[casper(export)]
    pub fn mapping() -> BTreeMap<String, u64> {
        let mut map = BTreeMap::new();
        map.insert("A".into(), 1);
        map.insert("B".into(), 2);
        map.insert("C".into(), 3);
        map
    }
}

#[casper(export)]
pub fn call() -> Result<(), ApiError> {
    let entry_points = contract::entry_points();

    let mut messages = BTreeMap::new();
    messages.insert(
        event::DidNothing::TOPIC_NAME.into(),
        MessageTopicOperation::Add,
    );

    let (contract_hash, contract_version) = storage::new_contract(
        entry_points,
        None,
        Some(PACKAGE_HASH_KEY_NAME.into()),
        Some(ACCESS_KEY_NAME.into()),
        Some(messages),
    );

    CONTRACT_VERSION_KEY
        .get_or_init(|| utils::new_uref_key(contract_version))?
        .put_to_named_keys()?;
    HASH_KEY.set(Key::Hash(contract_hash.value()))?;
    Ok(())
}
