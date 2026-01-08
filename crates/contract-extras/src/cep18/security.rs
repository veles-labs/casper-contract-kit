use alloc::{collections::BTreeMap, vec, vec::Vec};

use veles_casper_contract_api::{
    casper_contract::unwrap_or_revert::UnwrapOrRevert,
    casper_types::CLType,
    casper_types::{
        CLTyped, Key,
        bytesrepr::{self, FromBytes, ToBytes},
    },
};

use super::{
    SECURITY_BADGES_DICT,
    error::Cep18Error,
    utils::{base64_encode, get_immediate_caller},
};
use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SecurityBadge {
    Admin = 0,
    Minter = 1,
    None = 2,
}

impl CLTyped for SecurityBadge {
    fn cl_type() -> CLType {
        CLType::U8
    }
}

impl ToBytes for SecurityBadge {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        Ok(vec![*self as u8])
    }

    fn serialized_length(&self) -> usize {
        1
    }
}

impl FromBytes for SecurityBadge {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        Ok((
            match bytes[0] {
                0 => SecurityBadge::Admin,
                1 => SecurityBadge::Minter,
                2 => SecurityBadge::None,
                _ => return Err(bytesrepr::Error::LeftOverBytes),
            },
            &[],
        ))
    }
}

pub fn sec_check(allowed_badge_list: Vec<SecurityBadge>) -> Result<(), Cep18Error> {
    let caller = get_immediate_caller();
    let caller_bytes = caller
        .to_bytes()
        .unwrap_or_revert_with(Cep18Error::FailedToConvertBytes);

    let badge = SECURITY_BADGES_DICT
        .get_dict::<_, SecurityBadge>(&base64_encode(caller_bytes))
        .map_err(|_| Cep18Error::FailedToReadFromStorage)?
        .ok_or(Cep18Error::InsufficientRights)?;

    if !allowed_badge_list.contains(&badge) {
        return Err(Cep18Error::InsufficientRights);
    }

    Ok(())
}

pub fn change_sec_badge(badge_map: &BTreeMap<Key, SecurityBadge>) -> Result<(), Cep18Error> {
    for (user, badge) in badge_map {
        SECURITY_BADGES_DICT
            .put_dict(
                base64_encode(
                    user.to_bytes()
                        .unwrap_or_revert_with(Cep18Error::FailedToConvertBytes),
                ),
                *badge,
            )
            .map_err(|_| Cep18Error::FailedToInsertToSecurityList)?;
    }
    Ok(())
}
