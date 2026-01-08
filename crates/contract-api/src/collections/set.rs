use core::marker::PhantomData;

use crate::{
    collections::{dictionary_key::DictionaryKey, mapping::Mapping},
    named_key::NamedKey,
};
use casper_types::ApiError;

/// A set collection that stores unique keys of type `K`.
#[derive(Clone)]
pub struct Set<K> {
    mapping: Mapping<K, ()>,
    marker: PhantomData<K>,
}

impl<K> Set<K> {
    pub const fn from_named_key(named_key: NamedKey) -> Self {
        Self {
            mapping: Mapping::from_named_key(named_key),
            marker: PhantomData,
        }
    }

    pub fn insert<'a>(&self, key: &'a K) -> Result<(), ApiError>
    where
        K: DictionaryKey<'a>,
    {
        self.mapping.insert(key, ())?;
        Ok(())
    }

    pub fn contains<'a>(&self, key: &'a K) -> Result<bool, ApiError>
    where
        K: DictionaryKey<'a>,
    {
        let value: Option<()> = self.mapping.get(key)?;
        Ok(value.is_some())
    }
}
