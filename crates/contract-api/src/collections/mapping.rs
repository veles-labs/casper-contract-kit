use core::marker::PhantomData;

use crate::{collections::dictionary_key::DictionaryKey, named_key::NamedKey};
use casper_types::{
    ApiError, CLTyped,
    bytesrepr::{FromBytes, ToBytes},
};

/// A mapping collection that associates keys of type `K` to values of type `V`.
#[derive(Clone)]
pub struct Mapping<K, V> {
    named_key: NamedKey,
    marker: PhantomData<(K, V)>,
}

impl<K, V> Mapping<K, V> {
    pub const fn from_named_key(named_key: NamedKey) -> Self {
        Self {
            named_key,
            marker: PhantomData,
        }
    }

    pub fn bind_to(&self, named_key: NamedKey) -> Self {
        Self {
            named_key,
            marker: PhantomData,
        }
    }

    pub fn named_uref(&self) -> &NamedKey {
        &self.named_key
    }

    pub fn insert<'a>(&self, key: &'a K, value: V) -> Result<(), ApiError>
    where
        K: DictionaryKey<'a>,
        V: ToBytes + CLTyped,
    {
        let key_preimage = key.dictionary_key();
        self.named_key.put_dict(&key_preimage, &value)?;
        Ok(())
    }

    pub fn get<'a>(&self, key: &'a K) -> Result<Option<V>, ApiError>
    where
        K: DictionaryKey<'a>,
        V: FromBytes + CLTyped,
    {
        let key_preimage = key.dictionary_key();
        let value: Option<V> = self.named_key.get_dict(&key_preimage)?;
        Ok(value)
    }
}

unsafe impl<K: Sync, V: Sync> Sync for Mapping<K, V> {}
