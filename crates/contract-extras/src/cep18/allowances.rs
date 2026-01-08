//! Implementation of allowances.
use super::{ALLOWANCES_DICT, error::Cep18Error, utils::make_dictionary_item_key};
use veles_casper_contract_api::casper_types::{Key, U256};

/// Writes an allowance for owner and spender for a specific amount.
pub fn write_allowance_to(owner: Key, spender: Key, amount: U256) -> Result<(), Cep18Error> {
    let dictionary_item_key = make_dictionary_item_key(&owner, &spender);
    ALLOWANCES_DICT
        .put_dict(dictionary_item_key, amount)
        .map_err(|_| Cep18Error::FailedToReadFromStorage)
}

/// Reads an allowance for a owner and spender
pub fn read_allowance_from(owner: Key, spender: Key) -> Result<U256, Cep18Error> {
    let dictionary_item_key = make_dictionary_item_key(&owner, &spender);
    let value = ALLOWANCES_DICT
        .get_dict(&dictionary_item_key)
        .map_err(|_| Cep18Error::FailedToReadFromStorage)?
        .unwrap_or_default();
    Ok(value)
}
