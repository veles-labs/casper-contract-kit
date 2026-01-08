use casper_types::{ApiError, RuntimeArgs, contract_messages::MessagePayload};

/// A trait for types that can be converted into runtime arguments.
pub trait IntoRuntimeArgs {
    fn into_runtime_args(self) -> RuntimeArgs;
}

/// A trait for types that can be converted into Casper messages.
pub trait CasperMessage: Sized {
    const TOPIC_NAME: &'static str;
    const TOPIC_NAME_HASH: [u8; 32];

    fn into_message_payload(self) -> Result<MessagePayload, ApiError>;
}

pub fn set_panic_hook() {
    #[cfg(feature = "std")]
    std::panic::set_hook(alloc::boxed::Box::new(|_info| {
        crate::casper_contract::contract_api::runtime::revert(crate::error::UniversalError::Panic);
    }));
}
