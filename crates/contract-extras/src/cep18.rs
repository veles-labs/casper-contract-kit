pub mod constants;
#[cfg(test)]
pub mod entry_points;
pub mod error;
pub mod events;
pub mod modalities;
pub mod security;

pub mod allowances;

pub mod balances;

pub mod utils;

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::convert::TryFrom;
use veles_casper_contract_api::{
    casper_contract::{
        contract_api::{
            runtime::{self, put_key, revert},
            storage::{self, read},
        },
        unwrap_or_revert::UnwrapOrRevert,
    },
    casper_types::{
        AddressableEntityHash, EntityAddr, Key, NamedKeys, U256, bytesrepr::ToBytes,
        contract_messages::MessageTopicOperation, contracts::ContractPackageHash, runtime_args,
    },
    named_key::NamedKey,
    typed_uref::TypedURef,
    veles_casper_contract_macros::casper,
};
use {
    allowances::{read_allowance_from, write_allowance_to},
    balances::{read_balance_from, transfer_balance, write_balance_to},
    constants::{
        ADMIN_LIST, ARG_CONTRACT_HASH, ARG_DECIMALS, ARG_ENABLE_MINT_BURN, ARG_EVENTS,
        ARG_EVENTS_MODE, ARG_NAME, ARG_PACKAGE_HASH, ARG_SYMBOL, ARG_TOTAL_SUPPLY, DICT_ALLOWANCES,
        DICT_BALANCES, DICT_SECURITY_BADGES, ENTRY_POINT_INIT, MINTER_LIST, NONE_LIST,
        PREFIX_ACCESS_KEY_NAME, PREFIX_CEP18, PREFIX_CONTRACT_NAME, PREFIX_CONTRACT_PACKAGE_NAME,
        PREFIX_CONTRACT_VERSION,
    },
    error::Cep18Error,
    events::{
        Burn, ChangeEventsMode, ChangeSecurity, DecreaseAllowance, Event, IncreaseAllowance, Mint,
        SetAllowance, Transfer, TransferFrom, init_events,
    },
    modalities::EventsMode,
    security::{SecurityBadge, change_sec_badge, sec_check},
    utils::{
        base64_encode, get_contract_version_key, get_immediate_caller,
        get_optional_named_arg_with_user_errors, get_uref_with_user_errors,
    },
};

static NAME: NamedKey = NamedKey::from_name(ARG_NAME);
pub static NAME_KEY: TypedURef<String> = TypedURef::from_named_key(&NAME);
static SYMBOL: NamedKey = NamedKey::from_name(ARG_SYMBOL);
pub static SYMBOL_KEY: TypedURef<String> = TypedURef::from_named_key(&SYMBOL);
static DECIMALS: NamedKey = NamedKey::from_name(ARG_DECIMALS);
pub static DECIMALS_KEY: TypedURef<u8> = TypedURef::from_named_key(&DECIMALS);
static TOTAL_SUPPLY: NamedKey = NamedKey::from_name(ARG_TOTAL_SUPPLY);
pub static TOTAL_SUPPLY_KEY: TypedURef<U256> = TypedURef::from_named_key(&TOTAL_SUPPLY);
static EVENTS_MODE: NamedKey = NamedKey::from_name(ARG_EVENTS_MODE);
pub static EVENTS_MODE_KEY: TypedURef<u8> = TypedURef::from_named_key(&EVENTS_MODE);
static ENABLE_MINT_BURN: NamedKey = NamedKey::from_name(ARG_ENABLE_MINT_BURN);
pub static ENABLE_MINT_BURN_KEY: TypedURef<u8> = TypedURef::from_named_key(&ENABLE_MINT_BURN);

pub static ALLOWANCES_DICT: NamedKey = NamedKey::from_name(DICT_ALLOWANCES);
pub static BALANCES_DICT: NamedKey = NamedKey::from_name(DICT_BALANCES);
pub static SECURITY_BADGES_DICT: NamedKey = NamedKey::from_name(DICT_SECURITY_BADGES);

#[casper(contract)]
pub mod cep18 {
    use alloc::collections::BTreeMap;
    use veles_casper_contract_api::veles_casper_contract_macros::casper;

    use super::*;

    #[casper(export)]
    pub fn name() -> Result<String, Cep18Error> {
        Ok(NAME_KEY
            .read()
            .map_err(|_| Cep18Error::FailedToReturnEntryPointResult)?
            .expect("Name should be initialized"))
    }

    #[casper(export)]
    pub fn symbol() -> Result<String, Cep18Error> {
        Ok(SYMBOL_KEY
            .read()
            .map_err(|_| Cep18Error::FailedToReturnEntryPointResult)?
            .expect("Symbol should be initialized"))
    }

    #[casper(export)]
    pub fn decimals() -> Result<u8, Cep18Error> {
        Ok(DECIMALS_KEY
            .read()
            .map_err(|_| Cep18Error::FailedToReturnEntryPointResult)?
            .expect("Decimals should be initialized"))
    }

    #[casper(export)]
    pub fn total_supply() -> Result<U256, Cep18Error> {
        Ok(TOTAL_SUPPLY_KEY
            .read()
            .map_err(|_| Cep18Error::FailedToReturnEntryPointResult)?
            .expect("Total supply should be initialized"))
    }

    #[casper(export)]
    pub fn balance_of(address: Key) -> Result<U256, Cep18Error> {
        read_balance_from(address)
    }

    #[casper(export)]
    pub fn allowance(owner: Key, spender: Key) -> Result<U256, Cep18Error> {
        read_allowance_from(owner, spender)
    }

    #[casper(export)]
    pub fn approve(spender: Key, amount: U256) -> Result<(), Cep18Error> {
        let caller = get_immediate_caller();
        if spender == caller {
            return Err(Cep18Error::CannotTargetSelfUser);
        }

        write_allowance_to(caller, spender, amount)?;

        events::record_event_dictionary(Event::SetAllowance(SetAllowance {
            owner: caller,
            spender,
            allowance: amount,
        }));
        Ok(())
    }

    #[casper(export)]
    pub fn decrease_allowance(spender: Key, amount: U256) -> Result<(), Cep18Error> {
        let caller = get_immediate_caller();
        if spender == caller {
            return Err(Cep18Error::CannotTargetSelfUser);
        }

        let current_allowance = read_allowance_from(caller, spender)?;
        let new_allowance = current_allowance.saturating_sub(amount);
        write_allowance_to(caller, spender, new_allowance)?;

        events::record_event_dictionary(Event::DecreaseAllowance(DecreaseAllowance {
            owner: caller,
            spender,
            decr_by: amount,
            allowance: new_allowance,
        }));
        Ok(())
    }

    #[casper(export)]
    pub fn increase_allowance(spender: Key, amount: U256) -> Result<(), Cep18Error> {
        let caller = get_immediate_caller();
        if spender == caller {
            return Err(Cep18Error::CannotTargetSelfUser);
        }

        let current_allowance = read_allowance_from(caller, spender)?;
        let new_allowance = current_allowance.saturating_add(amount);
        write_allowance_to(caller, spender, new_allowance)?;

        events::record_event_dictionary(Event::IncreaseAllowance(IncreaseAllowance {
            owner: caller,
            spender,
            allowance: new_allowance,
            inc_by: amount,
        }));
        Ok(())
    }

    #[casper(export)]
    pub fn transfer(recipient: Key, amount: U256) -> Result<(), Cep18Error> {
        let caller = get_immediate_caller();
        if caller == recipient {
            return Err(Cep18Error::CannotTargetSelfUser);
        }

        transfer_balance(caller, recipient, amount)?;

        events::record_event_dictionary(Event::Transfer(Transfer {
            sender: caller,
            recipient,
            amount,
        }));
        Ok(())
    }

    #[casper(export)]
    pub fn transfer_from(owner: Key, recipient: Key, amount: U256) -> Result<(), Cep18Error> {
        let caller = get_immediate_caller();
        if owner == recipient {
            return Err(Cep18Error::CannotTargetSelfUser);
        }

        if amount.is_zero() {
            return Ok(());
        }

        let spender_allowance = read_allowance_from(owner, caller)?;
        let new_spender_allowance = spender_allowance
            .checked_sub(amount)
            .ok_or(Cep18Error::InsufficientAllowance)?;

        transfer_balance(owner, recipient, amount)?;
        write_allowance_to(owner, caller, new_spender_allowance)?;

        events::record_event_dictionary(Event::TransferFrom(TransferFrom {
            spender: caller,
            owner,
            recipient,
            amount,
        }));
        Ok(())
    }

    #[casper(export)]
    pub fn mint(owner: Key, amount: U256) -> Result<(), Cep18Error> {
        ensure_mint_burn_enabled()?;

        sec_check(vec![SecurityBadge::Admin, SecurityBadge::Minter])?;

        let new_balance = {
            let balance = read_balance_from(owner)?;
            balance.checked_add(amount).ok_or(Cep18Error::Overflow)?
        };

        let new_total_supply = {
            let total_supply = TOTAL_SUPPLY_KEY
                .read()
                .map_err(|_| Cep18Error::FailedToReadFromStorage)?
                .expect("Total supply should be initialized");

            total_supply
                .checked_add(amount)
                .ok_or(Cep18Error::Overflow)?
        };

        write_balance_to(owner, new_balance)?;
        TOTAL_SUPPLY_KEY
            .write(new_total_supply)
            .map_err(|_| Cep18Error::FailedToReadFromStorage)?;

        events::record_event_dictionary(Event::Mint(Mint {
            recipient: owner,
            amount,
        }));
        Ok(())
    }

    #[casper(export)]
    pub fn burn(owner: Key, amount: U256) -> Result<(), Cep18Error> {
        ensure_mint_burn_enabled()?;

        sec_check(vec![SecurityBadge::Admin, SecurityBadge::Minter])?;

        let new_balance = {
            let balance = read_balance_from(owner)?;
            balance
                .checked_sub(amount)
                .ok_or(Cep18Error::InsufficientBalance)?
        };

        let new_total_supply = {
            let total_supply = TOTAL_SUPPLY_KEY
                .read()
                .map_err(|_| Cep18Error::FailedToReadFromStorage)?
                .expect("Total supply should be initialized");
            total_supply
                .checked_sub(amount)
                .ok_or(Cep18Error::FailedToChangeTotalSupply)?
        };

        write_balance_to(owner, new_balance)?;
        TOTAL_SUPPLY_KEY
            .write(new_total_supply)
            .map_err(|_| Cep18Error::FailedToReadFromStorage)?;

        events::record_event_dictionary(Event::Burn(Burn { owner, amount }));
        Ok(())
    }

    #[casper(export)]
    pub fn init() -> Result<(), Cep18Error> {
        if veles_casper_contract_api::utils::get_key(DICT_ALLOWANCES).is_ok() {
            return Err(Cep18Error::AlreadyInitialized);
        }

        let package_hash: Key = runtime::get_named_arg(ARG_PACKAGE_HASH);
        veles_casper_contract_api::utils::put_key(ARG_PACKAGE_HASH, package_hash)
            .unwrap_or_revert();

        let contract_hash: Key = runtime::get_named_arg(ARG_CONTRACT_HASH);
        put_key(ARG_CONTRACT_HASH, contract_hash);

        ALLOWANCES_DICT
            .get_or_init(veles_casper_contract_api::utils::new_dictionary_key)
            .and_then(|named_key| named_key.put_to_named_keys())
            .map_err(|_| Cep18Error::FailedToCreateDictionary)?;

        BALANCES_DICT
            .clone()
            .get_or_init(veles_casper_contract_api::utils::new_dictionary_key)
            .and_then(|named_key| named_key.put_to_named_keys())
            .map_err(|_| Cep18Error::FailedToCreateDictionary)?;
        let initial_supply: U256 = runtime::get_named_arg(ARG_TOTAL_SUPPLY);

        let caller = get_immediate_caller();

        write_balance_to(caller, initial_supply)?;
        TOTAL_SUPPLY_KEY
            .write(initial_supply)
            .map_err(|_| Cep18Error::FailedToReadFromStorage)?;

        let security_badges_dict = SECURITY_BADGES_DICT
            .get_or_init(veles_casper_contract_api::utils::new_dictionary_key)
            .and_then(|named_key| named_key.put_to_named_keys())
            .map_err(|_| Cep18Error::FailedToCreateDictionary)?;

        security_badges_dict
            .put_dict(
                base64_encode(
                    caller
                        .to_bytes()
                        .map_err(|_| Cep18Error::FailedToConvertBytes)?,
                ),
                SecurityBadge::Admin,
            )
            .map_err(|_| Cep18Error::FailedToInsertToSecurityList)?;

        let admin_list: Option<Vec<Key>> =
            get_optional_named_arg_with_user_errors(ADMIN_LIST, Cep18Error::InvalidAdminList);
        let minter_list: Option<Vec<Key>> =
            get_optional_named_arg_with_user_errors(MINTER_LIST, Cep18Error::InvalidMinterList);

        init_events()?;

        if let Some(minter_list) = minter_list {
            for minter in minter_list {
                security_badges_dict
                    .put_dict(
                        base64_encode(
                            minter
                                .to_bytes()
                                .map_err(|_| Cep18Error::FailedToConvertBytes)?,
                        ),
                        SecurityBadge::Minter,
                    )
                    .map_err(|_| Cep18Error::FailedToInsertToSecurityList)?;
            }
        }

        if let Some(admin_list) = admin_list {
            for admin in admin_list {
                security_badges_dict
                    .put_dict(
                        base64_encode(
                            admin
                                .to_bytes()
                                .map_err(|_| Cep18Error::FailedToConvertBytes)?,
                        ),
                        SecurityBadge::Admin,
                    )
                    .map_err(|_| Cep18Error::FailedToInsertToSecurityList)?;
            }
        }

        events::record_event_dictionary(Event::Mint(Mint {
            recipient: caller,
            amount: initial_supply,
        }));
        Ok(())
    }

    #[casper(export)]
    pub fn change_security() -> Result<(), Cep18Error> {
        ensure_mint_burn_enabled()?;
        sec_check(vec![SecurityBadge::Admin])?;

        let admin_list: Option<Vec<Key>> =
            get_optional_named_arg_with_user_errors(ADMIN_LIST, Cep18Error::InvalidAdminList);
        let minter_list: Option<Vec<Key>> =
            get_optional_named_arg_with_user_errors(MINTER_LIST, Cep18Error::InvalidMinterList);
        let none_list: Option<Vec<Key>> =
            get_optional_named_arg_with_user_errors(NONE_LIST, Cep18Error::InvalidNoneList);

        let mut badge_map: BTreeMap<Key, SecurityBadge> = BTreeMap::new();
        if let Some(minter_list) = minter_list {
            for account_key in minter_list {
                badge_map.insert(account_key, SecurityBadge::Minter);
            }
        }
        if let Some(admin_list) = admin_list {
            for account_key in admin_list {
                badge_map.insert(account_key, SecurityBadge::Admin);
            }
        }
        if let Some(none_list) = none_list {
            for account_key in none_list {
                badge_map.insert(account_key, SecurityBadge::None);
            }
        }

        let caller = get_immediate_caller();
        badge_map.remove(&caller);

        change_sec_badge(&badge_map)?;

        events::record_event_dictionary(Event::ChangeSecurity(ChangeSecurity {
            admin: caller,
            sec_change_map: badge_map,
        }));
        Ok(())
    }

    #[casper(export)]
    pub fn change_events_mode(events_mode: u8) -> Result<(), Cep18Error> {
        sec_check(vec![SecurityBadge::Admin])?;

        let desired_events_mode = EventsMode::try_from(events_mode)?;
        let current_events_mode_raw = EVENTS_MODE_KEY
            .read()
            .map_err(|_| Cep18Error::InvalidEventsMode)?
            .unwrap_or(0);
        let current_events_mode = EventsMode::try_from(current_events_mode_raw)?;

        if desired_events_mode == current_events_mode {
            return Ok(());
        }

        EVENTS_MODE_KEY
            .write(events_mode)
            .map_err(|_| Cep18Error::FailedToReadFromStorage)?;
        init_events()?;

        events::record_event_dictionary(Event::ChangeEventsMode(ChangeEventsMode { events_mode }));
        Ok(())
    }
}

pub(crate) fn ensure_mint_burn_enabled() -> Result<(), Cep18Error> {
    let flag = ENABLE_MINT_BURN_KEY
        .read()
        .map_err(|_| Cep18Error::FailedToReadFromStorage)?
        .unwrap_or(0);
    if flag == 0 {
        return Err(Cep18Error::MintBurnDisabled);
    }
    Ok(())
}

pub fn upgrade(name: &str) {
    let entry_points = cep18::entry_points();

    let package_key_name = &format!("{PREFIX_CEP18}_{PREFIX_CONTRACT_PACKAGE_NAME}_{name}");
    let contract_key_name = &format!("{PREFIX_CEP18}_{PREFIX_CONTRACT_NAME}_{name}");

    let old_contract_package_hash = match runtime::get_key(package_key_name)
        .unwrap_or_revert_with(Cep18Error::FailedToGetOldPackageKey)
    {
        Key::Hash(contract_hash) => contract_hash,
        Key::AddressableEntity(EntityAddr::SmartContract(contract_hash)) => contract_hash,
        Key::SmartContract(package_hash) => package_hash,
        _ => revert(Cep18Error::MissingPackageHashForUpgrade),
    };
    let contract_package_hash = ContractPackageHash::new(old_contract_package_hash);

    let previous_contract_hash = match runtime::get_key(contract_key_name)
        .unwrap_or_revert_with(Cep18Error::FailedToGetOldContractHashKey)
    {
        Key::Hash(contract_hash) => contract_hash,
        Key::AddressableEntity(EntityAddr::SmartContract(contract_hash)) => contract_hash,
        _ => revert(Cep18Error::MissingContractHashForUpgrade),
    };
    let converted_previous_contract_hash = AddressableEntityHash::new(previous_contract_hash);

    let events_mode = get_optional_named_arg_with_user_errors::<u8>(
        ARG_EVENTS_MODE,
        Cep18Error::InvalidEventsMode,
    );

    let version_value_uref = get_uref_with_user_errors(
        &format!("{PREFIX_CEP18}_{PREFIX_CONTRACT_VERSION}_{name}"),
        Cep18Error::MissingVersionContractKey,
        Cep18Error::InvalidVersionContractKey,
    );

    let version_value: String = read(version_value_uref)
        .unwrap_or_default()
        .unwrap_or_default();

    let message_topics: BTreeMap<String, MessageTopicOperation> = if !version_value.is_empty() {
        BTreeMap::new()
    } else {
        BTreeMap::from([(ARG_EVENTS.to_string(), MessageTopicOperation::Add)])
    };

    let named_keys = NamedKeys::new();

    let (contract_hash, contract_version) = storage::add_contract_version(
        contract_package_hash,
        entry_points,
        named_keys,
        message_topics,
    );

    storage::disable_contract_version(
        contract_package_hash,
        converted_previous_contract_hash.into(),
    )
    .unwrap_or_revert_with(Cep18Error::FailedToDisableContractVersion);

    runtime::put_key(
        &format!("{PREFIX_CEP18}_{PREFIX_CONTRACT_NAME}_{name}"),
        Key::Hash(contract_hash.value()),
    );

    runtime::put_key(
        &format!("{PREFIX_CEP18}_{PREFIX_CONTRACT_VERSION}_{name}"),
        storage::new_uref(get_contract_version_key(contract_version).to_string()).into(),
    );

    if let Some(events_mode_u8) = events_mode {
        let wrapped_testnet_token = cep18::Client::new(contract_hash);
        wrapped_testnet_token.change_events_mode(events_mode_u8);
    }
}

pub fn install_contract(name: &str) {
    let symbol: String = runtime::get_named_arg(ARG_SYMBOL);
    let decimals: u8 = runtime::get_named_arg(ARG_DECIMALS);
    let total_supply: U256 = runtime::get_named_arg(ARG_TOTAL_SUPPLY);
    let events_mode: u8 =
        get_optional_named_arg_with_user_errors(ARG_EVENTS_MODE, Cep18Error::InvalidEventsMode)
            .unwrap_or(0u8);

    let admin_list: Option<Vec<Key>> =
        get_optional_named_arg_with_user_errors(ADMIN_LIST, Cep18Error::InvalidAdminList);
    let minter_list: Option<Vec<Key>> =
        get_optional_named_arg_with_user_errors(MINTER_LIST, Cep18Error::InvalidMinterList);

    let enable_mint_burn: u8 = get_optional_named_arg_with_user_errors(
        ARG_ENABLE_MINT_BURN,
        Cep18Error::InvalidEnableMBFlag,
    )
    .unwrap_or(0);

    let mut named_keys = NamedKeys::new();

    NAME.get_or_init(|| veles_casper_contract_api::utils::new_uref_key(name))
        .and_then(|named_key| named_key.append_to_named_keys(&mut named_keys))
        .unwrap_or_revert_with(Cep18Error::FailedToCreateDictionary);

    SYMBOL
        .get_or_init(|| veles_casper_contract_api::utils::new_uref_key(symbol))
        .and_then(|named_key| named_key.append_to_named_keys(&mut named_keys))
        .unwrap_or_revert_with(Cep18Error::FailedToCreateDictionary);

    DECIMALS
        .get_or_init(|| veles_casper_contract_api::utils::new_uref_key(decimals))
        .and_then(|named_key| named_key.append_to_named_keys(&mut named_keys))
        .unwrap_or_revert_with(Cep18Error::FailedToCreateDictionary);

    TOTAL_SUPPLY
        .get_or_init(|| veles_casper_contract_api::utils::new_uref_key(total_supply))
        .and_then(|named_key| named_key.append_to_named_keys(&mut named_keys))
        .unwrap_or_revert_with(Cep18Error::FailedToCreateDictionary);

    EVENTS_MODE
        .get_or_init(|| veles_casper_contract_api::utils::new_uref_key(events_mode))
        .and_then(|named_key| named_key.append_to_named_keys(&mut named_keys))
        .unwrap_or_revert_with(Cep18Error::FailedToCreateDictionary);

    ENABLE_MINT_BURN
        .get_or_init(|| veles_casper_contract_api::utils::new_uref_key(enable_mint_burn))
        .and_then(|named_key| named_key.append_to_named_keys(&mut named_keys))
        .unwrap_or_revert_with(Cep18Error::FailedToCreateDictionary);

    let entry_points = cep18::entry_points();

    let message_topics = BTreeMap::from([(ARG_EVENTS.to_string(), MessageTopicOperation::Add)]);

    let package_hash_name = format!("{PREFIX_CEP18}_{PREFIX_CONTRACT_PACKAGE_NAME}_{name}");

    let (contract_hash, contract_version) = storage::new_contract(
        entry_points,
        Some(named_keys),
        Some(package_hash_name.clone()),
        Some(format!("{PREFIX_CEP18}_{PREFIX_ACCESS_KEY_NAME}_{name}")),
        Some(message_topics),
    );

    let package_hash = runtime::get_key(&package_hash_name)
        .unwrap_or_revert_with(Cep18Error::FailedToGetPackageKey);

    let contract_hash_key = Key::Hash(contract_hash.value());

    runtime::put_key(
        &format!("{PREFIX_CEP18}_{PREFIX_CONTRACT_NAME}_{name}"),
        contract_hash_key,
    );

    runtime::put_key(
        &format!("{PREFIX_CEP18}_{PREFIX_CONTRACT_VERSION}_{name}"),
        storage::new_uref(get_contract_version_key(contract_version).to_string()).into(),
    );

    let mut init_args = runtime_args! {
        ARG_TOTAL_SUPPLY => total_supply,
        ARG_PACKAGE_HASH => package_hash,
        ARG_CONTRACT_HASH => contract_hash_key,
        ARG_EVENTS_MODE => events_mode
    };

    if let Some(admin_list) = admin_list {
        init_args
            .insert(ADMIN_LIST, admin_list)
            .unwrap_or_revert_with(Cep18Error::FailedToInsertToSecurityList);
    }
    if let Some(minter_list) = minter_list {
        init_args
            .insert(MINTER_LIST, minter_list)
            .unwrap_or_revert_with(Cep18Error::FailedToInsertToSecurityList);
    }

    runtime::call_contract::<()>(contract_hash, ENTRY_POINT_INIT, init_args);
}

#[cfg(test)]
mod tests {
    use super::{cep18, entry_points::generate_entry_points};
    use alloc::{
        collections::{BTreeMap, BTreeSet},
        string::{String, ToString},
        vec::Vec,
    };
    use veles_casper_contract_api::casper_types::{EntityEntryPoint, EntryPoints};

    fn as_map(entry_points: EntryPoints) -> BTreeMap<String, EntityEntryPoint> {
        entry_points
            .take_entry_points()
            .into_iter()
            .map(|entry_point| (entry_point.name().to_string(), entry_point))
            .collect()
    }

    #[test]
    fn generate_entry_points_match() {
        let macro_entry_points = as_map(cep18::entry_points());
        let manual_entry_points = as_map(generate_entry_points());

        let manual_keys: BTreeSet<_> = manual_entry_points.keys().cloned().collect();
        let macro_keys: BTreeSet<_> = macro_entry_points.keys().cloned().collect();

        let missing_in_macro: Vec<_> = manual_keys.difference(&macro_keys).cloned().collect();

        assert!(
            missing_in_macro.is_empty(),
            "manual entry points missing from macro-generated set: {:?}",
            missing_in_macro
        );

        for (name, manual_entry_point) in manual_entry_points {
            let macro_entry_point = macro_entry_points
                .get(&name)
                .unwrap_or_else(|| panic!("missing macro entry point: {name}"));

            assert_eq!(
                macro_entry_point.args(),
                manual_entry_point.args(),
                "argument mismatch for entry point {name}"
            );
            assert_eq!(
                macro_entry_point.ret(),
                manual_entry_point.ret(),
                "return type mismatch for entry point {name}"
            );
            assert_eq!(
                macro_entry_point.access(),
                manual_entry_point.access(),
                "access mismatch for entry point {name}"
            );
            assert_eq!(
                macro_entry_point.entry_point_type(),
                manual_entry_point.entry_point_type(),
                "entry point type mismatch for entry point {name}"
            );
            assert_eq!(
                macro_entry_point.entry_point_payment(),
                manual_entry_point.entry_point_payment(),
                "payment mismatch for entry point {name}"
            );
        }
    }
}
