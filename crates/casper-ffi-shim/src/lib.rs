//! FFI shim for Casper host functions.
//!
//! This module provides a Foreign Function Interface (FFI) layer that allows
//! smart contract code to run under any host target architecture. It defines
//! the necessary set of functions to avoid `undefined symbol` when (for example)
//! trying to run unit tests within your smart contract code.
//!
//! There is _some_ support for stubbing the host i.e. put_key/get_key/remove_key/has_key
//! does implement a simple in-memory key-value store. However, most functions
//! are just stubs that log their invocation and return default values.
//!
//! Importing this makes rust-analyzer happy.
#![allow(unused_variables)]
#![allow(clippy::missing_safety_doc)]

/// Macro to handle unimplemented FFI functions without panicking
macro_rules! unimplemented_ffi {
    ($fn_name:expr) => {{
        eprintln!("FFI function {} called but not implemented", $fn_name);
        let error: u32 = ApiError::Unhandled.into();
        error as i32
    }};
    ($fn_name:expr, void) => {{
        eprintln!("FFI function {} called but not implemented", $fn_name);
    }};
}

use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    mem,
    ptr::NonNull,
    sync::{Arc, RwLock},
};

use casper_types::{
    AccessRights, ApiError, CLTyped, CLValue, Key, StoredValue, U256, U512, URef, URefAddr,
    api_error,
    bytesrepr::{self, ToBytes},
};

// Custom error type for revert that can be handled without unwinding
#[derive(Debug, Clone)]
pub struct RevertError {
    pub status: u32,
    pub api_error: ApiError,
}

impl core::fmt::Display for RevertError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Casper revert: {:?} ({})", self.api_error, self.status)
    }
}

impl core::error::Error for RevertError {}

// Make RevertError compatible with panic_any
unsafe impl Send for RevertError {}
unsafe impl Sync for RevertError {}

// Thread-local storage for revert errors
thread_local! {
    static REVERT_ERROR: RefCell<Option<RevertError>> = const { RefCell::new(None) };
}

/// Check if a revert occurred and return the error if it did
pub fn check_revert() -> Option<RevertError> {
    REVERT_ERROR.with(|r| r.borrow().clone())
}

/// Clear any pending revert error
pub fn clear_revert() {
    REVERT_ERROR.with(|r| *r.borrow_mut() = None);
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostFunction {
    CasperReadValue,
    CasperWrite,
    CasperAdd,
    CasperNewUref,
    CasperLoadAuthorizationKeys,
    CasperLoadNamedKeys,
    CasperRet,
    CasperGetKey(String),
    CasperHasKey(String),
    CasperPutKey(String, Key),
    CasperRemoveKey(String),
    CasperRevert,
    CasperIsValidUref,
    CasperAddAssociatedKey,
    CasperRemoveAssociatedKey,
    CasperUpdateAssociatedKey,
    CasperSetActionThreshold,
    CasperGetCaller,
    CasperGetBlocktime,
    CasperCreatePurse,
    CasperTransferToAccount,
    CasperTransferFromPurseToAccount,
    CasperTransferFromPurseToPurse,
    CasperGetBalance,
    CasperGetPhase,
    CasperGetSystemContract,
    CasperGetMainPurse,
    CasperReadHostBuffer,
    CasperCreateContractPackageAtHash,
    CasperCreateContractUserGroup,
    CasperAddContractVersion,
    CasperAddContractVersionWithMessageTopics,
    CasperAddPackageVersionWithMessageTopics,
    CasperDisableContractVersion,
    CasperCallContract,
    CasperCallVersionedContract,
    CasperGetNamedArgSize,
    CasperGetNamedArg,
    CasperRemoveContractUserGroup,
    CasperProvisionContractUserGroupUref,
    CasperRemoveContractUserGroupUrefs,
    CasperBlake2b,
    CasperLoadCallStack,
    CasperPrint,
    CasperNewDictionary,
    CasperDictionaryGet,
    CasperDictionaryRead,
    CasperDictionaryPut,
    CasperRandomBytes,
    CasperEnableContractVersion,
    CasperManageMessageTopic,
    CasperEmitMessage,
    CasperLoadCallerInformation,
    CasperGetBlockInfo,
    CasperGenericHash,
    CasperRecoverSecp256k1,
    CasperVerifySignature,
    CasperCallPackageVersion,
}

#[derive(Debug, Default)]
pub struct EnvImpl {
    /// Simplified, always creates deterministic addresses by counting up.
    address_generator: U256,
    database: BTreeMap<Key, StoredValue>,
    args: BTreeMap<String, CLValue>,
    named_keys: BTreeMap<String, Key>,
    host_buffer: Option<CLValue>,
    dictionaries: BTreeMap<URefAddr, BTreeMap<String, CLValue>>,
    /// Very simple host function call trace for testing purposes.
    trace: Vec<HostFunction>,
}

#[derive(Debug, Clone)]
pub struct Env {
    env_impl: Arc<RwLock<EnvImpl>>,
}

impl EnvImpl {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn next_address(&mut self) -> [u8; 32] {
        self.address_generator += U256::one();
        let mut output = [0; 32];
        self.address_generator.to_little_endian(&mut output);
        output
    }
}

impl Env {
    pub fn named_keys(&self) -> BTreeMap<String, Key> {
        self.env_impl.read().unwrap().named_keys.clone()
    }

    /// Returns and clears the current trace of host function calls.
    ///
    /// This is primarily intended for testing purposes.
    pub fn trace(&self) -> Vec<HostFunction> {
        mem::take(&mut self.env_impl.write().unwrap().trace)
    }
}

#[derive(Debug)]
pub struct EnvBuilder {
    address_generator: U256,
    database: BTreeMap<Key, StoredValue>,
    args: BTreeMap<String, CLValue>,
    named_keys: BTreeMap<String, Key>,
    dictionaries: BTreeMap<URefAddr, BTreeMap<String, CLValue>>,
}

impl EnvBuilder {
    pub fn new() -> Self {
        Self {
            address_generator: U256::zero(),
            database: BTreeMap::new(),
            args: BTreeMap::new(),
            named_keys: BTreeMap::new(),
            dictionaries: BTreeMap::new(),
        }
    }

    pub fn with_address_generator(mut self, address_generator: U256) -> Self {
        self.address_generator = address_generator;
        self
    }

    pub fn with_database(mut self, database: BTreeMap<Key, StoredValue>) -> Self {
        self.database = database;
        self
    }

    pub fn with_args(mut self, args: BTreeMap<String, CLValue>) -> Self {
        self.args = args;
        self
    }

    pub fn with_arg<T: ToBytes + CLTyped>(mut self, name: impl Into<String>, value: T) -> Self {
        let value = CLValue::from_t(value).expect("Failed to convert value to CLValue");
        self.args.insert(name.into(), value);
        self
    }

    pub fn with_storage(mut self, key: Key, value: StoredValue) -> Self {
        self.database.insert(key, value);
        self
    }

    pub fn with_named_keys(mut self, named_keys: BTreeMap<String, Key>) -> Self {
        self.named_keys = named_keys;
        self
    }

    pub fn with_named_key(mut self, name: impl Into<String>, key: Key) -> Self {
        self.named_keys.insert(name.into(), key);
        self
    }

    pub fn build(self) -> Env {
        Env {
            env_impl: Arc::new(RwLock::new(EnvImpl {
                address_generator: self.address_generator,
                database: self.database,
                args: self.args,
                named_keys: self.named_keys,
                host_buffer: None,
                dictionaries: self.dictionaries,
                trace: Vec::new(),
            })),
        }
    }
}

impl Default for EnvBuilder {
    fn default() -> Self {
        Self::new()
    }
}

thread_local! {
    static ENV: RefCell<RwLock<VecDeque<Env>>> = const { RefCell::new(RwLock::new(VecDeque::new())) };

}

pub fn dispatch_with<F>(new_env: Env, func: F)
where
    F: FnOnce(&Env),
{
    ENV.with(|stack| {
        let env = stack.borrow();
        env.write().unwrap().push_back(new_env.clone());

        // Clear any previous revert error
        clear_revert();

        // Execute the function

        func(&new_env);

        env.write().unwrap().pop_back();
    })
}

fn with_current_env<F, R>(func: F) -> R
where
    F: FnOnce(&mut EnvImpl) -> R,
{
    ENV.with(|stack| {
        let env = stack.borrow();
        let binding = env.read().unwrap();
        let back_mut = binding.back().expect("Env should not be empty");
        func(&mut back_mut.env_impl.write().unwrap())
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_read_value(
    key_ptr: *const u8,
    key_size: usize,
    output_size: *mut usize,
) -> i32 {
    let key = unsafe { core::slice::from_raw_parts(key_ptr, key_size) };
    let key: Key = bytesrepr::deserialize_from_slice(key).expect("Failed to deserialize key");
    let mut output_size = NonNull::new(output_size).expect("output_size pointer must not be null");

    with_current_env(|env| {
        env.trace.push(HostFunction::CasperReadValue);
        match env.database.get(&key) {
            Some(value) => {
                let cl_value: CLValue = value
                    .clone()
                    .try_into()
                    .expect("Failed to convert to CLValue");

                unsafe {
                    *output_size.as_mut() = cl_value.inner_bytes().len();
                }

                let old_host_buffer = env.host_buffer.replace(cl_value);
                if let Some(old_host_buffer) = &old_host_buffer {
                    panic!("Host buffer should be empty before writing to it: {old_host_buffer:?}");
                }

                0 // Success
            }
            None => -1, // Not found
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_write(
    key_ptr: *const u8,
    key_size: usize,
    value_ptr: *const u8,
    value_size: usize,
) {
    let key = unsafe { core::slice::from_raw_parts(key_ptr, key_size) };
    let key: Key = bytesrepr::deserialize_from_slice(key).expect("Failed to deserialize key");
    let value = unsafe { core::slice::from_raw_parts(value_ptr, value_size) };
    let value: CLValue =
        bytesrepr::deserialize_from_slice(value).expect("Failed to deserialize value");

    with_current_env(|env| {
        env.trace.push(HostFunction::CasperWrite);
        env.database.insert(key, StoredValue::CLValue(value));
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_add(
    key_ptr: *const u8,
    key_size: usize,
    value_ptr: *const u8,
    value_size: usize,
) {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_new_uref(
    uref_ptr: *mut u8,
    value_ptr: *const u8,
    value_size: usize,
) {
    let value = unsafe { core::slice::from_raw_parts(value_ptr, value_size) };
    let value: CLValue =
        bytesrepr::deserialize_from_slice(value).expect("Failed to deserialize value");

    with_current_env(|env| {
        env.trace.push(HostFunction::CasperNewUref);
        let uref = URef::new(env.next_address(), AccessRights::READ_ADD_WRITE);
        let key = Key::URef(uref);
        env.database.insert(key, StoredValue::CLValue(value));

        let key_bytes = uref.to_bytes().expect("Failed to serialize URef");
        unsafe {
            core::ptr::copy_nonoverlapping(key_bytes.as_ptr(), uref_ptr, key_bytes.len());
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_load_authorization_keys(
    total_keys: *mut usize,
    result_size: *mut usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_load_named_keys(
    total_keys: *mut usize,
    result_size: *mut usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_ret(value_ptr: *const u8, value_size: usize) -> ! {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_get_key(
    name_ptr: *const u8,
    name_size: usize,
    output_ptr: *mut u8,
    output_size: usize,
    bytes_written_ptr: *mut usize,
) -> i32 {
    let result = with_current_env(|env| {
        let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_size) };
        let name: String =
            bytesrepr::deserialize_from_slice(name_bytes).expect("Failed to deserialize name");
        env.trace.push(HostFunction::CasperGetKey(name.clone()));

        match env.named_keys.get(&name) {
            Some(key) => {
                let key_bytes = key.to_bytes().expect("Failed to serialize key");
                if key_bytes.len() > output_size {
                    return Err(ApiError::BufferTooSmall);
                }
                unsafe {
                    core::ptr::copy_nonoverlapping(key_bytes.as_ptr(), output_ptr, key_bytes.len());
                    *bytes_written_ptr = key_bytes.len();
                }
                Ok(()) // Success
            }
            None => Err(ApiError::MissingKey), // Key not found
        }
    });
    api_error::i32_from(result)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_has_key(name_ptr: *const u8, name_size: usize) -> i32 {
    with_current_env(|env| {
        let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_size) };
        let name: String =
            bytesrepr::deserialize_from_slice(name_bytes).expect("Failed to deserialize name");
        env.trace.push(HostFunction::CasperHasKey(name.clone()));
        if env.named_keys.contains_key(&name) {
            0 // Key exists
        } else {
            1 // Key does not exist
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_put_key(
    name_ptr: *const u8,
    name_size: usize,
    key_ptr: *const u8,
    key_size: usize,
) {
    with_current_env(|env| {
        let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_size) };
        let name: String =
            bytesrepr::deserialize_from_slice(name_bytes).expect("Failed to deserialize name");
        let key_bytes = unsafe { core::slice::from_raw_parts(key_ptr, key_size) };
        let key: Key =
            bytesrepr::deserialize_from_slice(key_bytes).expect("Failed to deserialize key");
        env.trace
            .push(HostFunction::CasperPutKey(name.clone(), key));
        env.named_keys.insert(name, key);
    });
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_remove_key(name_ptr: *const u8, name_size: usize) {
    with_current_env(|env| {
        let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_size) };
        let name: String =
            bytesrepr::deserialize_from_slice(name_bytes).expect("Failed to deserialize name");
        env.trace.push(HostFunction::CasperRemoveKey(name.clone()));
        env.named_keys.remove(&name);
    });
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_revert(status: u32) -> ! {
    let api_error = ApiError::from(status);

    // Store the revert error in thread-local storage for potential inspection
    REVERT_ERROR.with(|r| *r.borrow_mut() = Some(RevertError { status, api_error }));

    // Print comprehensive error information for debugging
    eprintln!("=== CASPER REVERT ===");
    eprintln!("Status: {}", status);
    eprintln!("API Error: {:?}", api_error);
    eprintln!("This indicates the smart contract execution was reverted.");
    eprintln!("====================");

    // Use abort for a clean termination without unwinding issues
    std::process::abort();
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_is_valid_uref(uref_ptr: *const u8, uref_size: usize) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_add_associated_key(
    account_hash_ptr: *const u8,
    account_hash_size: usize,
    weight: i32,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_remove_associated_key(
    account_hash_ptr: *const u8,
    account_hash_size: usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_update_associated_key(
    account_hash_ptr: *const u8,
    account_hash_size: usize,
    weight: i32,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_set_action_threshold(permission_level: u32, threshold: u32) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_get_caller(output_size_ptr: *mut usize) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_get_blocktime(dest_ptr: *const u8) {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_create_purse(purse_ptr: *mut u8, purse_size: usize) -> i32 {
    with_current_env(|env| {
        env.trace.push(HostFunction::CasperCreatePurse);
        let uref = URef::new(env.next_address(), AccessRights::READ_ADD_WRITE);
        let key_1 = Key::URef(uref);
        let value_1 = StoredValue::CLValue(CLValue::unit());
        env.database.insert(key_1, value_1);

        let key_2 = Key::Balance(uref.addr());
        let value_2 = StoredValue::CLValue(
            CLValue::from_t(U512::zero()).expect("Failed to create CLValue for balance"),
        );
        env.database.insert(key_2, value_2);

        let key_bytes = uref.to_bytes().expect("Failed to serialize URef");
        unsafe {
            core::ptr::copy_nonoverlapping(key_bytes.as_ptr(), purse_ptr, purse_size);
        }
    });

    0 // Success
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_transfer_to_account(
    target_ptr: *const u8,
    target_size: usize,
    amount_ptr: *const u8,
    amount_size: usize,
    id_ptr: *const u8,
    id_size: usize,
    result_ptr: *const i32,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_transfer_from_purse_to_account(
    source_ptr: *const u8,
    source_size: usize,
    target_ptr: *const u8,
    target_size: usize,
    amount_ptr: *const u8,
    amount_size: usize,
    id_ptr: *const u8,
    id_size: usize,
    result_ptr: *const i32,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_transfer_from_purse_to_purse(
    source_ptr: *const u8,
    source_size: usize,
    target_ptr: *const u8,
    target_size: usize,
    amount_ptr: *const u8,
    amount_size: usize,
    id_ptr: *const u8,
    id_size: usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_get_balance(
    purse_ptr: *const u8,
    purse_size: usize,
    result_size: *mut usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_get_phase(dest_ptr: *mut u8) {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_get_system_contract(
    system_contract_index: u32,
    dest_ptr: *mut u8,
    dest_size: usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_get_main_purse(dest_ptr: *mut u8) {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_read_host_buffer(
    dest_ptr: *mut u8,
    dest_size: usize,
    bytes_written: *mut usize,
) -> i32 {
    let result = with_current_env(|env| match env.host_buffer.take() {
        Some(host_buffer) => {
            let bytes = host_buffer.inner_bytes();

            unsafe {
                *bytes_written = bytes.len();
                assert_eq!(bytes.len(), dest_size, "Host buffer size mismatch");
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), dest_ptr, dest_size);
            }
            Ok(())
        }
        None => Err(ApiError::HostBufferEmpty),
    });
    api_error::i32_from(result)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_create_contract_package_at_hash(
    hash_addr_ptr: *mut u8,
    access_addr_ptr: *mut u8,
    is_locked: bool,
) {
    todo!();
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_create_contract_user_group(
    contract_package_hash_ptr: *const u8,
    contract_package_hash_size: usize,
    label_ptr: *const u8,
    label_size: usize,
    num_new_urefs: u8,
    existing_urefs_ptr: *const u8,
    existing_urefs_size: usize,
    output_size_ptr: *mut usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_add_contract_version(
    contract_package_hash_ptr: *const u8,
    contract_package_hash_size: usize,
    version_ptr: *const u32,
    entry_points_ptr: *const u8,
    entry_points_size: usize,
    named_keys_ptr: *const u8,
    named_keys_size: usize,
    output_ptr: *mut u8,
    output_size: usize,
    bytes_written_ptr: *mut usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_add_contract_version_with_message_topics(
    contract_package_hash_ptr: *const u8,
    contract_package_hash_size: usize,
    version_ptr: *const u32,
    entry_points_ptr: *const u8,
    entry_points_size: usize,
    named_keys_ptr: *const u8,
    named_keys_size: usize,
    message_topics_ptr: *const u8,
    message_topics_size: usize,
    output_ptr: *mut u8,
    output_size: usize,
) -> i32 {
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_add_package_version_with_message_topics(
    package_hash_ptr: *const u8,
    package_hash_size: usize,
    version_ptr: *const u32,
    entry_points_ptr: *const u8,
    entry_points_size: usize,
    named_keys_ptr: *const u8,
    named_keys_size: usize,
    message_topics_ptr: *const u8,
    message_topics_size: usize,
    output_ptr: *mut u8,
    output_size: usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_disable_contract_version(
    contract_package_hash_ptr: *const u8,
    contract_package_hash_size: usize,
    contract_hash_ptr: *const u8,
    contract_hash_size: usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_call_contract(
    contract_hash_ptr: *const u8,
    contract_hash_size: usize,
    entry_point_name_ptr: *const u8,
    entry_point_name_size: usize,
    runtime_args_ptr: *const u8,
    runtime_args_size: usize,
    result_size: *mut usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_call_versioned_contract(
    contract_package_hash_ptr: *const u8,
    contract_package_hash_size: usize,
    contract_version_ptr: *const u8,
    contract_version_size: usize,
    entry_point_name_ptr: *const u8,
    entry_point_name_size: usize,
    runtime_args_ptr: *const u8,
    runtime_args_size: usize,
    result_size: *mut usize,
) -> i32 {
    todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_get_named_arg_size(
    name_ptr: *const u8,
    name_size: usize,
    dest_size: *mut usize,
) -> i32 {
    let name: &[u8] = unsafe { core::slice::from_raw_parts(name_ptr, name_size) };
    let name: &str = core::str::from_utf8(name).expect("Failed to convert bytes to str");
    with_current_env(|env| {
        env.trace.push(HostFunction::CasperGetNamedArgSize);
        match env.args.get(name) {
            Some(value) => {
                let size = value.inner_bytes().len();
                unsafe {
                    *dest_size = size;
                }
                0 // Success
            }
            None => {
                unsafe {
                    *dest_size = 0;
                }
                let i: u32 = ApiError::MissingArgument.into();
                i as i32 // Missing argument
            }
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_get_named_arg(
    name_ptr: *const u8,
    name_size: usize,
    dest_ptr: *mut u8,
    dest_size: usize,
) -> i32 {
    let name: &[u8] = unsafe { core::slice::from_raw_parts(name_ptr, name_size) };
    let name: &str = core::str::from_utf8(name).expect("Failed to convert bytes to str");
    let result = with_current_env(|env| {
        env.trace.push(HostFunction::CasperGetNamedArg);
        match env.args.get(name) {
            Some(value) => {
                let bytes = value.inner_bytes();
                if bytes.len() > dest_size {
                    return Err(ApiError::BufferTooSmall);
                }
                unsafe {
                    core::ptr::copy_nonoverlapping(bytes.as_ptr(), dest_ptr, bytes.len());
                }
                Ok(()) // Success
            }
            None => Err(ApiError::MissingArgument),
        }
    });

    api_error::i32_from(result)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_remove_contract_user_group(
    contract_package_hash_ptr: *const u8,
    contract_package_hash_size: usize,
    label_ptr: *const u8,
    label_size: usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_provision_contract_user_group_uref(
    contract_package_hash_ptr: *const u8,
    contract_package_hash_size: usize,
    label_ptr: *const u8,
    label_size: usize,
    value_size_ptr: *const usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_remove_contract_user_group_urefs(
    contract_package_hash_ptr: *const u8,
    contract_package_hash_size: usize,
    label_ptr: *const u8,
    label_size: usize,
    urefs_ptr: *const u8,
    urefs_size: usize,
) -> i32 {
    todo!()
}
#[deprecated(note = "Superseded by ext_ffi::casper_generic_hash")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_blake2b(
    in_ptr: *const u8,
    in_size: usize,
    out_ptr: *mut u8,
    out_size: usize,
) -> i32 {
    todo!()
}
#[deprecated]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_load_call_stack(
    call_stack_len_ptr: *mut usize,
    result_size_ptr: *mut usize,
) -> i32 {
    todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_print(text_ptr: *const u8, text_size: usize) {
    let text: &[u8] = unsafe { core::slice::from_raw_parts(text_ptr, text_size) };
    let text: String =
        bytesrepr::deserialize_from_slice(text).expect("Failed to deserialize text for printing");

    eprintln!("Print: {text}");
}

/// Creates a new dictionary and returns its URef in the host buffer.
///
/// # Safety
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_new_dictionary(output_size_ptr: *mut usize) -> i32 {
    with_current_env(|env| {
        let uref = URef::new(env.next_address(), AccessRights::READ_ADD_WRITE);
        let key = Key::URef(uref);

        let cl_value = CLValue::unit();

        env.database
            .insert(key, StoredValue::CLValue(cl_value.clone()));

        env.dictionaries.entry(uref.addr()).or_default();

        let old_host_buffer = env
            .host_buffer
            .replace(CLValue::from_t(uref).expect("Failed to create CLValue from URef"));
        if let Some(old_value) = old_host_buffer {
            panic!("Host buffer already contains a value, cannot overwrite it: {old_value:?}");
        }
        let key_bytes = uref.to_bytes().expect("Failed to serialize URef");
        unsafe {
            *output_size_ptr = key_bytes.len();
        }
        0 // Success
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_dictionary_get(
    uref_ptr: *const u8,
    uref_size: usize,
    key_bytes_ptr: *const u8,
    key_bytes_size: usize,
    output_size: *mut usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_dictionary_read(
    key_ptr: *const u8,
    key_size: usize,
    output_size: *mut usize,
) -> i32 {
    todo!()
}
/// Inserts a key-value pair into the specified dictionary.
///
/// # Safety
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_dictionary_put(
    uref_ptr: *const u8,
    uref_size: usize,
    key_ptr: *const u8,
    key_size: usize,
    value_ptr: *const u8,
    value_size: usize,
) -> i32 {
    with_current_env(|env| {
        let uref_bytes = unsafe { core::slice::from_raw_parts(uref_ptr, uref_size) };
        let uref: URef =
            bytesrepr::deserialize_from_slice(uref_bytes).expect("Failed to deserialize URef");

        let key_bytes = unsafe { core::slice::from_raw_parts(key_ptr, key_size) };
        let key =
            String::from_utf8(key_bytes.to_vec()).expect("Failed to convert key bytes to String");

        let value_bytes = unsafe { core::slice::from_raw_parts(value_ptr, value_size) };
        let value: CLValue =
            bytesrepr::deserialize_from_slice(value_bytes).expect("Failed to deserialize value");

        if let Some(dict) = env.dictionaries.get_mut(&uref.addr()) {
            dict.insert(key, value);
            0 // Success
        } else {
            -1 // Dictionary not found
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_random_bytes(out_ptr: *mut u8, out_size: usize) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_enable_contract_version(
    contract_package_hash_ptr: *const u8,
    contract_package_hash_size: usize,
    contract_hash_ptr: *const u8,
    contract_hash_size: usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_manage_message_topic(
    topic_name_ptr: *const u8,
    topic_name_size: usize,
    operation_ptr: *const u8,
    operation_size: usize,
) -> i32 {
    todo!()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_emit_message(
    topic_name_ptr: *const u8,
    topic_name_size: usize,
    message_ptr: *const u8,
    message_size: usize,
) -> i32 {
    todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_load_caller_information(
    action: u8,
    call_stack_len_ptr: *mut usize,
    result_size_ptr: *mut usize,
) -> i32 {
    unimplemented_ffi!("casper_load_caller_information")
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_get_block_info(field_idx: u8, dest_ptr: *const u8) {
    todo!("casper_get_block_info")
}

/// The 32-byte digest keccak256 hash function
#[allow(dead_code)]
fn keccak256<T: AsRef<[u8]>>(data: T) -> [u8; 32] {
    use keccak_asm::Digest as KeccakDigest;
    use keccak_asm::Keccak256;

    let mut h = Keccak256::new();
    KeccakDigest::update(&mut h, &data);
    let mut out = [0u8; 32];
    let result = KeccakDigest::finalize(h);
    out.copy_from_slice(&result);
    out
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_generic_hash(
    in_ptr: *const u8,
    in_size: usize,
    hash_algo_type: u8,
    out_ptr: *const u8,
    out_size: usize,
) -> i32 {
    let result = {
        // For allowing fallback in the code that uses this FFI function we'll report InvalidArgument as if given algorithm is not supported instead of failing.
        // This allows production code to fallback gracefully instead of panicking.
        Err(ApiError::InvalidArgument)
    };

    api_error::i32_from(result)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_recover_secp256k1(
    message_ptr: *const u8,
    message_size: usize,
    signature_ptr: *const u8,
    signature_size: usize,
    out_ptr: *const u8,
    recovery_id: u8,
) -> i32 {
    todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_verify_signature(
    message_ptr: *const u8,
    message_size: usize,
    signature_ptr: *const u8,
    signature_size: usize,
    public_key_ptr: *const u8,
    public_key_size: usize,
) -> i32 {
    todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn casper_call_package_version(
    contract_package_hash_ptr: *const u8,
    contract_package_hash_size: usize,
    major_version_ptr: *const u8,
    major_version_size: usize,
    contract_version_ptr: *const u8,
    contract_version_size: usize,
    entry_point_name_ptr: *const u8,
    entry_point_name_size: usize,
    runtime_args_ptr: *const u8,
    runtime_args_size: usize,
    result_size: *mut usize,
) -> i32 {
    todo!()
}
