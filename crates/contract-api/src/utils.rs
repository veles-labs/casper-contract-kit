use alloc::boxed::Box;
use alloc::vec::Vec;
use casper_types::bytesrepr::{Bytes, FromBytes, U8_SERIALIZED_LENGTH};
use casper_types::contracts::ContractHash;
use casper_types::global_state::TrieMerkleProofStep;
use casper_types::system::CallerInfo;
use casper_types::{BLAKE2B_DIGEST_LENGTH, CLTyped, Digest, Key, Pointer};
use core::mem::MaybeUninit;
use core::num::NonZeroU64;

use crate::error::UniversalError;

use crate::macro_support::CasperMessage;
use crate::{
    casper_contract::{
        contract_api::{self, runtime},
        ext_ffi,
        unwrap_or_revert::UnwrapOrRevert,
    },
    casper_types::{
        ApiError, CLValue, DICTIONARY_ITEM_KEY_MAX_LENGTH, EntityAddr, URef,
        account::AccountHash,
        api_error,
        bytesrepr::{self, ToBytes},
    },
};

#[repr(u8)]
pub enum CallerAction {
    Initiator = 0,
    Immediate = 1,
    FullStack = 2,
}

pub fn get_initiator_or_immediate(action: CallerAction) -> Result<CallerInfo, UniversalError> {
    let (call_stack_len, result_size) = {
        let mut call_stack_len: usize = 0;
        let mut result_size: usize = 0;
        let ret = unsafe {
            ext_ffi::casper_load_caller_information(
                action as u8,
                &mut call_stack_len as *mut usize,
                &mut result_size as *mut usize,
            )
        };
        let res = api_error::result_from(ret);
        crate::log!(
            "casper_load_caller_information returned: {:?}, call_stack_len: {}, result_size: {}",
            res,
            call_stack_len,
            result_size
        );
        res?;
        (call_stack_len, result_size)
    };
    if call_stack_len == 0 {
        crate::log!("No caller information found");
        return Err(ApiError::InvalidCallerInfoRequest.into());
    }
    crate::log!("Call stack length: {call_stack_len}, result size: {result_size}");
    let bytes = read_host_buffer(result_size).unwrap_or_revert();
    let caller: Vec<CallerInfo> = bytesrepr::deserialize(bytes).unwrap_or_revert();

    if caller.len() != 1 {
        crate::log!("Unexpected caller information: {caller:?}");
        return Err(ApiError::Unhandled.into());
    };
    crate::log!("Caller information obtained: {caller:?}");
    let first = caller.first().unwrap_or_revert().clone();
    Ok(first)
}

pub fn get_immediate_entity_addr() -> Result<Option<EntityAddr>, UniversalError> {
    const ACCOUNT: u8 = 0;
    const CONTRACT_PACKAGE: u8 = 2;
    const ENTITY: u8 = 3;
    const CONTRACT: u8 = 4;

    crate::log!("Getting immediate caller...");

    let caller_info = match get_initiator_or_immediate(CallerAction::Immediate) {
        Ok(info) => info,
        Err(UniversalError::Other(ApiError::Unhandled)) => {
            // IMMEDIATE fails with "Unhandled" when there's no immediate call frame.
            return Ok(None);
        }
        Err(e) => {
            return Err(e);
        }
    };

    crate::log!("Immediate caller obtained: {caller_info:?}");

    match caller_info.kind() {
        ACCOUNT => {
            let account_hash = caller_info
                .get_field_by_index(ACCOUNT)
                .unwrap()
                .to_t::<Option<AccountHash>>()?
                .ok_or(UniversalError::InvalidContext)?;

            Ok(Some(EntityAddr::Account(account_hash.value())))
        }

        ENTITY => {
            let entity_addr: EntityAddr = caller_info
                .get_field_by_index(ENTITY)
                .unwrap()
                .to_t::<Option<EntityAddr>>()?
                .ok_or(UniversalError::InvalidContext)?;
            Ok(Some(entity_addr))
        }
        CONTRACT_PACKAGE => Err(UniversalError::InvalidContext),
        CONTRACT => {
            let entity_addr: ContractHash = caller_info
                .get_field_by_index(CONTRACT)
                .unwrap()
                .to_t::<Option<ContractHash>>()?
                .ok_or(UniversalError::InvalidContext)?;
            Ok(Some(EntityAddr::SmartContract(entity_addr.value())))
        }
        _ => Err(UniversalError::InvalidContext),
    }
}

pub fn get_immediate_account() -> Result<AccountHash, ApiError> {
    crate::log!("Getting immediate entity address...");
    let entity_addr = get_immediate_entity_addr()?;
    crate::log!("Immediate entity address obtained: {entity_addr:?}");

    let Some(EntityAddr::Account(account_hash)) = entity_addr else {
        return Err(UniversalError::InvalidContext.into());
    };

    Ok(AccountHash::new(account_hash))
}

pub(crate) fn to_ptr<T: ToBytes>(t: &T) -> (*const u8, usize, Vec<u8>) {
    let bytes = t.into_bytes().unwrap_or_revert();
    let ptr = bytes.as_ptr();
    let size = bytes.len();
    (ptr, size, bytes)
}

pub fn dictionary_put_clvalue(
    dictionary_seed_uref: &URef,
    dictionary_item_key: &str,
    cl_value: CLValue,
) -> Result<(), ApiError> {
    let (uref_ptr, uref_size, _bytes1) = to_ptr(dictionary_seed_uref);
    let dictionary_item_key_ptr = dictionary_item_key.as_ptr();

    let dictionary_item_key_size = dictionary_item_key.len();

    if dictionary_item_key_size > DICTIONARY_ITEM_KEY_MAX_LENGTH {
        runtime::revert(ApiError::DictionaryItemKeyExceedsLength);
    }

    let (cl_value_ptr, cl_value_size, _bytes) = to_ptr(&cl_value);

    unsafe {
        let ret = ext_ffi::casper_dictionary_put(
            uref_ptr,
            uref_size,
            dictionary_item_key_ptr,
            dictionary_item_key_size,
            cl_value_ptr,
            cl_value_size,
        );
        api_error::result_from(ret)
    }
}

fn read_host_buffer_into(dest: &mut [u8]) -> Result<usize, ApiError> {
    let mut bytes_written = MaybeUninit::uninit();
    let ret = unsafe {
        ext_ffi::casper_read_host_buffer(dest.as_mut_ptr(), dest.len(), bytes_written.as_mut_ptr())
    };
    // NOTE: When rewriting below expression as `result_from(ret).map(|_| unsafe { ... })`, and the
    // caller ignores the return value, execution of the contract becomes unstable and ultimately
    // leads to `Unreachable` error.
    api_error::result_from(ret)?;
    Ok(unsafe { bytes_written.assume_init() })
}

pub fn dictionary_get_bytes(
    dictionary_seed_uref: &URef,
    dictionary_item_key: &[u8],
) -> Result<Option<Vec<u8>>, bytesrepr::Error> {
    let (uref_ptr, uref_size, _bytes1) = to_ptr(dictionary_seed_uref);
    let dictionary_item_key_ptr = dictionary_item_key.as_ptr();
    let dictionary_item_key_size = dictionary_item_key.len();

    if dictionary_item_key_size > DICTIONARY_ITEM_KEY_MAX_LENGTH {
        runtime::revert(ApiError::DictionaryItemKeyExceedsLength)
    }

    let value_size = {
        let mut value_size = MaybeUninit::uninit();
        let ret = unsafe {
            ext_ffi::casper_dictionary_get(
                uref_ptr,
                uref_size,
                dictionary_item_key_ptr,
                dictionary_item_key_size,
                value_size.as_mut_ptr(),
            )
        };
        match api_error::result_from(ret) {
            Ok(_) => unsafe { value_size.assume_init() },
            Err(ApiError::ValueNotFound) => return Ok(None),
            Err(e) => runtime::revert(e),
        }
    };

    let value_bytes = read_host_buffer(value_size).unwrap_or_revert();
    Ok(Some(value_bytes))
}

pub fn new_dictionary_uref() -> Result<URef, ApiError> {
    let value_size = {
        let mut value_size = MaybeUninit::uninit();
        let ret = unsafe { ext_ffi::casper_new_dictionary(value_size.as_mut_ptr()) };
        api_error::result_from(ret)?;
        unsafe { value_size.assume_init() }
    };
    let value_bytes = read_host_buffer(value_size)?;
    let uref: URef = bytesrepr::deserialize(value_bytes)?;
    Ok(uref)
}

/// Creates a new dictionary and returns its URef wrapped in a Key.
///
/// A convenience function to be used with `NamedKey::get_or_init`.
pub fn new_dictionary_key() -> Result<Key, ApiError> {
    let uref = new_dictionary_uref()?;
    Ok(Key::URef(uref))
}

/// Creates a new URef with the given value and returns it wrapped in a Key.
///
/// A convenience function to be used with `NamedKey::get_or_init`.
pub fn new_uref_key<T: ToBytes + CLTyped>(value: T) -> Result<Key, ApiError> {
    let uref = contract_api::storage::new_uref(&value);
    Ok(Key::URef(uref))
}

pub(crate) fn read_host_buffer(size: usize) -> Result<Vec<u8>, ApiError> {
    let mut dest: Vec<u8> = if size == 0 {
        Vec::new()
    } else {
        let bytes_non_null_ptr = contract_api::alloc_bytes(size);
        unsafe { Vec::from_raw_parts(bytes_non_null_ptr.as_ptr(), size, size) }
    };
    read_host_buffer_into(&mut dest)?;
    Ok(dest)
}

pub fn get_key(name: &'static str) -> Result<Option<casper_types::Key>, ApiError> {
    let name = length_prefixed_string(name);
    let mut key_bytes = [0u8; 64];
    let mut total_bytes: usize = 0;
    let ret = unsafe {
        ext_ffi::casper_get_key(
            name.as_ptr(),
            name.len(),
            key_bytes.as_mut_ptr(),
            key_bytes.len(),
            &mut total_bytes as *mut usize,
        )
    };
    match api_error::result_from(ret) {
        Ok(_) => {
            let key: Key =
                bytesrepr::deserialize_from_slice(&key_bytes[..total_bytes]).unwrap_or_revert();
            Ok(Some(key))
        }
        Err(ApiError::MissingKey) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn put_key(name: &'static str, key: Key) -> Result<(), ApiError> {
    let name = length_prefixed_string(name);
    let key_bytes = key.into_bytes()?;
    unsafe {
        ext_ffi::casper_put_key(
            name.as_ptr(),
            name.len(),
            key_bytes.as_ptr(),
            key_bytes.len(),
        )
    };
    Ok(())
}

/// Reads value under `key` in the global state.
pub fn read_key<T: FromBytes>(key: &Key) -> Result<Option<T>, ApiError> {
    let key_bytes = key.into_bytes()?;

    let value_size = {
        let mut value_size = MaybeUninit::uninit();
        let ret = unsafe {
            ext_ffi::casper_read_value(key_bytes.as_ptr(), key_bytes.len(), value_size.as_mut_ptr())
        };
        match api_error::result_from(ret) {
            Ok(_) => unsafe { value_size.assume_init() },
            Err(ApiError::ValueNotFound) => return Ok(None),
            Err(e) => return Err(e),
        }
    };

    let value_bytes = read_host_buffer(value_size)?;
    let value: T = bytesrepr::deserialize(value_bytes)?;
    Ok(Some(value))
}

/// Writes `value` under `key` in the global state.
pub fn write_key<T: ToBytes + CLTyped>(value: &T, key: Key) -> Result<(), ApiError> {
    let (key_ptr, key_size, _bytes1) = to_ptr(&key);

    let cl_value = CLValue::from_t(value)?;
    let (cl_value_ptr, cl_value_size, _bytes2) = to_ptr(&cl_value);

    unsafe {
        ext_ffi::casper_write(key_ptr, key_size, cl_value_ptr, cl_value_size);
    }

    Ok(())
}

fn length_prefixed_string(name: &'static str) -> Vec<u8> {
    let mut len_prefixed = Vec::with_capacity(U8_SERIALIZED_LENGTH + name.len());
    len_prefixed.extend_from_slice(&(name.len() as u32).to_le_bytes());
    len_prefixed.extend_from_slice(name.as_bytes());
    len_prefixed
}

pub fn has_key(name: &'static str) -> bool {
    let len_prefixed = length_prefixed_string(name);
    let ret = unsafe { ext_ffi::casper_has_key(len_prefixed.as_ptr(), len_prefixed.len()) };
    ret == 0
}

/// Removes the key from the global state.
pub fn remove_key(name: &'static str) {
    let len_prefixed = length_prefixed_string(name);
    unsafe { ext_ffi::casper_remove_key(len_prefixed.as_ptr(), len_prefixed.len()) };
}

/// Retrieves the URef associated with the given name from the global state.
pub fn get_uref(name: &'static str) -> Result<Option<URef>, ApiError> {
    let uref = get_key(name)?
        .and_then(|key| key.into_uref())
        .ok_or(ApiError::UnexpectedKeyVariant)?;
    Ok(Some(uref))
}

pub fn emit_message<E: CasperMessage>(event: E) -> Result<(), ApiError> {
    let payload = event.into_message_payload()?;
    {
        let topic_name = E::TOPIC_NAME.as_bytes();
        let message_bytes = payload.into_bytes()?;

        let result = unsafe {
            ext_ffi::casper_emit_message(
                topic_name.as_ptr(),
                topic_name.len(),
                message_bytes.as_ptr(),
                message_bytes.len(),
            )
        };

        api_error::result_from(result)
    }?;
    Ok(())
}

pub fn get_block_time() -> NonZeroU64 {
    let block_time: MaybeUninit<[u8; 8]> = MaybeUninit::uninit();
    unsafe {
        ext_ffi::casper_get_blocktime(block_time.as_ptr().cast());
    }
    let block_time = unsafe { block_time.assume_init() };
    let block_time = u64::from_le_bytes(block_time);
    crate::log_assert_ne!(block_time, 0, "Block time should never be zero");
    unsafe { NonZeroU64::new_unchecked(block_time) }
}

pub fn get_block_height() -> u64 {
    let block_height: MaybeUninit<[u8; 8]> = MaybeUninit::uninit();
    unsafe {
        ext_ffi::casper_get_block_info(
            runtime::BLOCK_HEIGHT_FIELD_IDX,
            block_height.as_ptr().cast(),
        )
    }
    let block_height_bytes = unsafe { block_height.assume_init() };
    u64::from_le_bytes(block_height_bytes)
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum HashAlgorithm {
    /// Blake2b
    Blake2b = 0,
    /// Blake3
    Blake3 = 1,
    /// Sha256,
    Sha256 = 2,
    /// Keccak256
    Keccak256 = 3,
}

pub fn generic_hash<T: AsRef<[u8]>>(algo: HashAlgorithm, data: T) -> Result<[u8; 32], ApiError> {
    let mut ret: MaybeUninit<[u8; BLAKE2B_DIGEST_LENGTH]> = MaybeUninit::uninit();
    let asref = data.as_ref();
    let result = unsafe {
        ext_ffi::casper_generic_hash(
            asref.as_ptr(),
            asref.len(),
            algo as u8,
            ret.as_mut_ptr().cast(),
            BLAKE2B_DIGEST_LENGTH,
        )
    };
    api_error::result_from(result)?;
    Ok(unsafe { ret.assume_init() })
}

pub(crate) const RADIX: usize = 256;

/// Type alias for values under pointer blocks.
pub type PointerBlockValue = Option<Pointer>;

/// Type alias for arrays of pointer block values.
pub type PointerBlockArray = [PointerBlockValue; RADIX];

/// Represents the underlying structure of a node in a Merkle Trie
#[derive(Copy, Clone)]
pub struct PointerBlock(PointerBlockArray);

impl PointerBlock {
    /// Constructs a `PointerBlock` from a slice of indexed `Pointer`s.
    pub fn from_indexed_pointers(indexed_pointers: &[(u8, Pointer)]) -> Self {
        let mut ret = PointerBlock([None; RADIX]);
        for (idx, ptr) in indexed_pointers.iter() {
            ret.0[*idx as usize] = Some(*ptr);
        }
        ret
    }
}

impl ToBytes for PointerBlock {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut result = bytesrepr::allocate_buffer(self)?;
        for pointer in self.0.iter() {
            result.append(&mut pointer.to_bytes()?);
        }
        Ok(result)
    }

    fn serialized_length(&self) -> usize {
        self.0.iter().map(ToBytes::serialized_length).sum()
    }

    fn write_bytes(&self, writer: &mut Vec<u8>) -> Result<(), bytesrepr::Error> {
        for pointer in self.0.iter() {
            pointer.write_bytes(writer)?;
        }
        Ok(())
    }
}

pub enum Trie {
    /// Trie node.
    Node {
        /// Node pointer block.
        pointer_block: Box<PointerBlock>,
    },
    /// Trie extension node.
    Extension {
        /// Extension node affix bytes.
        affix: Bytes,
        /// Extension node pointer.
        pointer: Pointer,
    },
}

impl ToBytes for Trie {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut ret = bytesrepr::allocate_buffer(self)?;
        self.write_bytes(&mut ret)?;
        Ok(ret)
    }

    fn serialized_length(&self) -> usize {
        U8_SERIALIZED_LENGTH
            + match self {
                Trie::Node { pointer_block } => pointer_block.serialized_length(),
                Trie::Extension { affix, pointer } => {
                    affix.serialized_length() + pointer.serialized_length()
                }
            }
    }

    fn write_bytes(&self, writer: &mut Vec<u8>) -> Result<(), bytesrepr::Error> {
        // NOTE: When changing this make sure all partial deserializers that are referencing
        // `LazyTrieLeaf` are also updated.
        let tag = match self {
            Trie::Node { .. } => 1u8,
            Trie::Extension { .. } => 2u8,
        };

        writer.push(tag);
        match self {
            Trie::Node { pointer_block } => pointer_block.write_bytes(writer)?,
            Trie::Extension { affix, pointer } => {
                affix.write_bytes(writer)?;
                pointer.write_bytes(writer)?;
            }
        }
        Ok(())
    }
}

/// Computes the state hash from the given trie leaf hash and an iterator over the proof steps.
///
/// # Arguments
///
/// * `trie_leaf_hash` - The hash of the trie leaf node to start from.
/// * `proof_steps` - An iterator over the proof steps to apply.
///
/// # Returns
/// A `Result` containing the computed state hash as a `Digest` or an error if serialization fails
pub fn compute_state_hash<I>(trie_leaf_hash: [u8; 32], proof_steps: I) -> Result<[u8; 32], ApiError>
where
    I: Iterator<Item = TrieMerkleProofStep>,
{
    let mut hash = trie_leaf_hash;

    for (proof_step_index, proof_step) in proof_steps.enumerate() {
        let pointer: Pointer = if proof_step_index == 0 {
            Pointer::LeafPointer(Digest::from_raw(hash))
        } else {
            Pointer::NodePointer(Digest::from_raw(hash))
        };
        let proof_step_bytes = match proof_step {
            TrieMerkleProofStep::Node {
                hole_index,
                indexed_pointers_with_hole: mut indexed_pointers,
            } => {
                debug_assert!(hole_index as usize <= 256, "hole_index exceeded RADIX");
                debug_assert_eq!(
                    indexed_pointers.iter().find(|(i, _ptr)| *i == hole_index),
                    None,
                );
                indexed_pointers.push((hole_index, pointer));
                Trie::Node {
                    pointer_block: Box::new(PointerBlock::from_indexed_pointers(&indexed_pointers)),
                }
                .to_bytes()?
            }
            TrieMerkleProofStep::Extension { affix } => {
                Trie::Extension { affix, pointer }.to_bytes()?
            }
        };
        hash = generic_hash(HashAlgorithm::Blake2b, &proof_step_bytes)?;
    }
    Ok(hash)
}

#[cfg(enable_casper_log)]
unsafe extern "C" {
    fn casper_print(text_ptr: *const u8, text_size: usize);
}

#[cfg(enable_casper_log)]
pub fn print(text: &str) {
    let value = text.to_bytes().unwrap();
    print_raw(value.as_slice());
}

#[cfg(enable_casper_log)]
pub fn print_raw(bytes: &[u8]) {
    debug_assert!(
        {
            let length: u32 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            length as usize + 4 == bytes.len()
        },
        "Invalid length prefix in print_raw"
    );
    unsafe {
        casper_print(bytes.as_ptr(), bytes.len());
    }
}

#[cfg(enable_casper_log)]
#[macro_export]
macro_rules! log {
    ($($args:tt)*) => ({
        let formatted = alloc::format!($($args)*);
        $crate::utils::print(&formatted);
    })
}

#[cfg(not(enable_casper_log))]
#[macro_export]
macro_rules! log {
    ($fmt:expr) => {};
    ($fmt:expr, $($args:tt)*) => { let _ = ($($args)*); };
}

/// Asserts that two expressions are equal, logging the assertion if `enable_casper_log` is enabled.
/// When `enable_casper_log` is disabled, this macro is a no-op.
#[cfg(enable_casper_log)]
#[macro_export]
macro_rules! log_assert_eq {
    ($left:expr, $right:expr) => {
        if $left != $right {
            $crate::log!("[{}:{}] assertion failed: `(left == right)`\n  left: `{:?}`,\n right: `{:?}`", file!(), line!(), $left, $right);
            panic!("assertion failed: left == right");
        }
    };
    ($left:expr, $right:expr, $($args:tt)*) => {
        if $left != $right {
            $crate::log!("[{}:{}] assertion failed: `(left == right)`\n  left: `{:?}`,\n right: `{:?}`\n{}", file!(), line!(), $left, $right, alloc::format!($($args)*));
            panic!("assertion failed: left == right");
        }
    };
}

#[cfg(not(enable_casper_log))]
#[macro_export]
macro_rules! log_assert_eq {
    ($left:expr, $right:expr) => {};
    ($left:expr, $right:expr, $($args:tt)*) => {};
}

/// Asserts that two expressions are not equal, logging the assertion if `enable_casper_log` is enabled.
/// When `enable_casper_log` is disabled, this macro is a no-op.
#[cfg(enable_casper_log)]
#[macro_export]
macro_rules! log_assert_ne {
    ($left:expr, $right:expr) => {
        if $left == $right {
            $crate::log!("[{}:{}] assertion failed: `(left != right)`\n  left: `{:?}`,\n right: `{:?}`", file!(), line!(), $left, $right);
            panic!("assertion failed: left != right");
        }
    };
    ($left:expr, $right:expr, $($args:tt)*) => {
        if $left == $right {
            $crate::log!("[{}:{}] assertion failed: `(left != right)`\n  left: `{:?}`,\n right: `{:?}`\n{}", file!(), line!(), $left, $right, alloc::format!($($args)*));
            panic!("assertion failed: left != right");
        }
    };
}

#[cfg(not(enable_casper_log))]
#[macro_export]
macro_rules! log_assert_ne {
    ($left:expr, $right:expr) => {};
    ($left:expr, $right:expr, $($args:tt)*) => {};
}
