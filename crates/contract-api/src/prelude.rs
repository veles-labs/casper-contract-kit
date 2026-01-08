//! A prelude for the contract-api crate.
//!
//! It re-exports commonly used items for easy import.
pub use crate::{
    casper_contract::contract_api::{runtime, storage},
    casper_types::{ApiError, Key, U512, contract_messages::MessageTopicOperation},
    macro_support::CasperMessage,
    named_key::NamedKey,
    typed_uref::TypedURef,
    utils,
    veles_casper_contract_macros::{CasperMessage, casper},
};
