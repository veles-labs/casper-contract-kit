use alloc::collections::BTreeMap;
use alloc::string::String;
use core::convert::TryFrom;

use super::{
    EVENTS_MODE_KEY, constants::ARG_EVENTS, error::Cep18Error, modalities::EventsMode,
    security::SecurityBadge,
};

use veles_casper_contract_api::{
    casper_contract::{
        contract_api::runtime::{emit_message, get_key},
        unwrap_or_revert::UnwrapOrRevert,
    },
    casper_event_standard::{EVENTS_DICT, Event, Schemas, emit, init},
    casper_types::{Key, U256, bytesrepr::Bytes, contract_messages::MessagePayload},
};

use serde::{Deserialize, Serialize};

pub fn record_event_dictionary(event: Event) {
    let events_mode_raw = EVENTS_MODE_KEY
        .read()
        .unwrap_or_revert_with(Cep18Error::InvalidEventsMode)
        .unwrap_or_revert_with(Cep18Error::InvalidEventsMode);
    let events_mode =
        EventsMode::try_from(events_mode_raw).unwrap_or_revert_with(Cep18Error::InvalidEventsMode);

    match events_mode {
        EventsMode::NoEvents => {}
        EventsMode::CES => ces(event),
        EventsMode::Native => emit_message(ARG_EVENTS, &event.to_json().into()).unwrap_or_revert(),
        EventsMode::NativeBytes => {
            let payload = MessagePayload::Bytes(Bytes::from(event.to_json().as_bytes()));
            emit_message(ARG_EVENTS, &payload).unwrap_or_revert()
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Event {
    Mint(Mint),
    Burn(Burn),
    SetAllowance(SetAllowance),
    IncreaseAllowance(IncreaseAllowance),
    DecreaseAllowance(DecreaseAllowance),
    Transfer(Transfer),
    TransferFrom(TransferFrom),
    ChangeSecurity(ChangeSecurity),
    ChangeEventsMode(ChangeEventsMode),
}

impl Event {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .map_err(|_| Cep18Error::FailedToConvertToJson)
            .unwrap_or_revert()
    }
}

use veles_casper_contract_api::casper_event_standard; // to bring in the Event derive macro

#[derive(Serialize, Deserialize, Event, Debug, PartialEq, Eq)]
pub struct Mint {
    pub recipient: Key,
    pub amount: U256,
}

impl Mint {
    pub fn new(recipient: Key, amount: U256) -> Self {
        Self { recipient, amount }
    }
}

#[derive(Serialize, Deserialize, Event, Debug, PartialEq, Eq)]
pub struct Burn {
    pub owner: Key,
    pub amount: U256,
}

#[derive(Serialize, Deserialize, Event, Debug, PartialEq, Eq)]
pub struct SetAllowance {
    pub owner: Key,
    pub spender: Key,
    pub allowance: U256,
}

#[derive(Serialize, Deserialize, Event, Debug, PartialEq, Eq)]
pub struct IncreaseAllowance {
    pub owner: Key,
    pub spender: Key,
    pub allowance: U256,
    pub inc_by: U256,
}

#[derive(Serialize, Deserialize, Event, Debug, PartialEq, Eq)]
pub struct DecreaseAllowance {
    pub owner: Key,
    pub spender: Key,
    pub allowance: U256,
    pub decr_by: U256,
}

#[derive(Serialize, Deserialize, Event, Debug, PartialEq, Eq)]
pub struct Transfer {
    pub sender: Key,
    pub recipient: Key,
    pub amount: U256,
}

#[derive(Serialize, Deserialize, Event, Debug, PartialEq, Eq)]
pub struct TransferFrom {
    pub spender: Key,
    pub owner: Key,
    pub recipient: Key,
    pub amount: U256,
}

#[derive(Serialize, Deserialize, Event, Debug, PartialEq, Eq)]
pub struct ChangeSecurity {
    pub admin: Key,
    pub sec_change_map: BTreeMap<Key, SecurityBadge>,
}

#[derive(Serialize, Deserialize, Event, Debug, PartialEq, Eq)]
pub struct ChangeEventsMode {
    pub events_mode: u8,
}

fn ces(event: Event) {
    match event {
        Event::Mint(ev) => emit(ev),
        Event::Burn(ev) => emit(ev),
        Event::SetAllowance(ev) => emit(ev),
        Event::IncreaseAllowance(ev) => emit(ev),
        Event::DecreaseAllowance(ev) => emit(ev),
        Event::Transfer(ev) => emit(ev),
        Event::TransferFrom(ev) => emit(ev),
        Event::ChangeSecurity(ev) => emit(ev),
        Event::ChangeEventsMode(ev) => emit(ev),
    }
}

pub fn init_events() -> Result<(), Cep18Error> {
    let events_mode_raw = EVENTS_MODE_KEY
        .read()
        .map_err(|_| Cep18Error::InvalidEventsMode)?
        .ok_or(Cep18Error::InvalidEventsMode)?;

    let events_mode =
        EventsMode::try_from(events_mode_raw).unwrap_or_revert_with(Cep18Error::InvalidEventsMode);

    if EventsMode::CES == events_mode && get_key(EVENTS_DICT).is_none() {
        let schemas = Schemas::new()
            .with::<Mint>()
            .with::<Burn>()
            .with::<SetAllowance>()
            .with::<IncreaseAllowance>()
            .with::<DecreaseAllowance>()
            .with::<Transfer>()
            .with::<TransferFrom>()
            .with::<ChangeSecurity>()
            .with::<ChangeEventsMode>();
        init(schemas);
    }

    Ok(())
}
