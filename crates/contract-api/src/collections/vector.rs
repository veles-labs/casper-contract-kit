use core::marker::PhantomData;

use super::base128;
use crate::{
    casper_types::{
        ApiError, CLTyped,
        bytesrepr::{FromBytes, ToBytes},
    },
    named_key::NamedKey,
};

const VEC_LENGTH_KEY: &str = "length";

/// A vector collection that stores elements of type `T` in a sequential manner.
pub struct Vector<T> {
    named_key: NamedKey,
    marker: PhantomData<T>,
}

impl<T> Vector<T> {
    pub const fn from_named_key(named_key: NamedKey) -> Self {
        Self {
            named_key,
            marker: PhantomData,
        }
    }

    pub fn push(&self, value: T) -> Result<(), ApiError>
    where
        T: ToBytes + CLTyped,
    {
        let length: u64 = self.len()?;

        let key = base128::encode_bytes(&length.to_le_bytes());
        self.named_key.put_dict(&key, value)?;

        // Update length
        self.set_len(length + 1)?;
        Ok(())
    }

    pub fn len(&self) -> Result<u64, ApiError> {
        let length: u64 = self.named_key.get_dict(VEC_LENGTH_KEY)?.unwrap_or(0);
        Ok(length)
    }

    pub fn is_empty(&self) -> Result<bool, ApiError> {
        Ok(self.len()? == 0)
    }

    pub fn set_len(&self, new_length: u64) -> Result<(), ApiError> {
        self.named_key.put_dict(VEC_LENGTH_KEY, new_length)?;
        Ok(())
    }

    pub fn get(&self, index: u64) -> Result<Option<T>, ApiError>
    where
        T: FromBytes + CLTyped,
    {
        let key = base128::encode_bytes(&index.to_le_bytes());
        let value: Option<T> = self.named_key.get_dict(&key)?;
        Ok(value)
    }

    pub fn set(&self, index: u64, value: T) -> Result<(), ApiError>
    where
        T: ToBytes + CLTyped,
    {
        let key = base128::encode_bytes(&index.to_le_bytes());
        self.named_key.put_dict(&key, value)?;
        Ok(())
    }
}

unsafe impl<T: Sync> Sync for Vector<T> {}
