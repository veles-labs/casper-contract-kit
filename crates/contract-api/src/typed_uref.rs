use core::marker::PhantomData;

use crate::{
    casper_types::{
        ApiError, CLTyped, URef,
        bytesrepr::{FromBytes, ToBytes},
    },
    named_key::NamedKey,
};

/// A typed URef that associates a `NamedKey` with a specific type `T` for easy and safe storage access.
pub struct TypedURef<'a, T> {
    named_key: &'a NamedKey,
    _marker: PhantomData<T>,
}

impl<'a, T> TypedURef<'a, T> {
    /// Creates a new `TypedURef` from a given `NamedKey`.
    pub const fn from_named_key(named_key: &'a NamedKey) -> Self {
        Self {
            named_key,
            _marker: PhantomData,
        }
    }

    /// Retrieves the underlying `URef` if it exists.
    pub fn uref(&self) -> Result<Option<URef>, ApiError> {
        let key = self.named_key.get()?;
        Ok(key.and_then(|key| key.into_uref()))
    }

    /// Reads the value stored under this TypedURef.
    pub fn read(&self) -> Result<Option<T>, ApiError>
    where
        T: CLTyped + FromBytes,
    {
        self.named_key.read()
    }

    /// Writes the value under this TypedURef.
    pub fn write(&self, value: T) -> Result<(), ApiError>
    where
        T: CLTyped + ToBytes,
    {
        self.named_key.write(&value)
    }
}

unsafe impl<T: Sync> Sync for TypedURef<'_, T> {}
