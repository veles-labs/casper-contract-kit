use veles_casper_contract_api::{
    casper_types::ApiError, named_key::NamedKey, typed_uref::TypedURef,
    veles_casper_contract_macros::casper,
};

static PAUSED_NAMED_KEY: NamedKey = NamedKey::from_name("paused");
pub static PAUSED_TUREF: TypedURef<bool> = TypedURef::from_named_key(&PAUSED_NAMED_KEY);

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PausableError {
    NotPaused = 41000,
    AlreadyPaused = 41001,
    ContractPaused = 41002,
}

impl From<PausableError> for ApiError {
    fn from(value: PausableError) -> Self {
        ApiError::User(value as u16)
    }
}

#[casper(contract)]
pub mod pausable {
    use super::*;

    use crate::{ownable, pausable::PausableError};

    use super::PAUSED_TUREF;

    #[casper(export)]
    pub fn pause() -> Result<(), ApiError> {
        ownable::ensure_owner()?;
        if PAUSED_TUREF.read()?.unwrap_or(false) {
            return Err(PausableError::AlreadyPaused.into());
        }
        PAUSED_TUREF.write(true)
    }

    #[casper(export)]
    pub fn unpause() -> Result<(), ApiError> {
        ownable::ensure_owner()?;
        if !PAUSED_TUREF.read()?.unwrap_or(false) {
            return Err(PausableError::NotPaused.into());
        }
        PAUSED_TUREF.write(false)
    }

    #[casper(export)]
    pub fn is_paused() -> Result<bool, ApiError> {
        Ok(PAUSED_TUREF.read()?.unwrap_or(false))
    }
}

pub fn require_unpaused() -> Result<(), ApiError> {
    if PAUSED_TUREF.read()?.unwrap_or(false) {
        Err(PausableError::ContractPaused.into())
    } else {
        Ok(())
    }
}
