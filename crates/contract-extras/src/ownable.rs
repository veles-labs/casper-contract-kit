use veles_casper_contract_api::{
    casper_types::{ApiError, Key, account::AccountHash},
    named_key::NamedKey,
    utils,
    veles_casper_contract_macros::casper,
};

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnableError {
    Unauthorized = 62000,
    OwnerMissing = 62001,
    AlreadyPaused = 62002,
    NotPaused = 62003,
    ContractPaused = 62004,
}

impl From<OwnableError> for ApiError {
    fn from(value: OwnableError) -> Self {
        ApiError::User(value as u16)
    }
}

pub static OWNER_KEY_NAME: NamedKey = NamedKey::from_name("owner");

#[casper(contract)]
pub mod ownable {
    use veles_casper_contract_api::casper_types::Key;

    use super::*;

    #[casper(export)]
    pub fn transfer_ownership(new_owner: AccountHash) -> Result<(), ApiError> {
        ownable::ensure_owner()?;
        OWNER_KEY_NAME.set(Key::Account(new_owner))?;
        Ok(())
    }

    #[casper(export)]
    pub fn renounce_ownership() -> Result<(), ApiError> {
        ownable::ensure_owner()?;
        OWNER_KEY_NAME.clear();
        Ok(())
    }

    #[casper(export)]
    pub fn current_owner() -> Result<Option<AccountHash>, ApiError> {
        get_current_owner()
    }
}

fn get_current_owner() -> Result<Option<AccountHash>, ApiError> {
    match OWNER_KEY_NAME.get() {
        Ok(Some(Key::Account(account))) => Ok(Some(account)),
        Ok(Some(_)) => Err(ApiError::UnexpectedKeyVariant),
        Ok(None) => Ok(None),
        Err(ApiError::MissingKey) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn ensure_owner() -> Result<AccountHash, ApiError> {
    let caller = utils::get_immediate_account()?;
    match get_current_owner()? {
        Some(owner) if owner == caller => Ok(owner),
        Some(_) => Err(OwnableError::Unauthorized.into()),
        None => Err(OwnableError::OwnerMissing.into()),
    }
}
