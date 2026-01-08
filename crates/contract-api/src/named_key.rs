use alloc::vec::Vec;
use core::cell::Cell;

use crate::{
    casper_contract::unwrap_or_revert::UnwrapOrRevert,
    casper_types::{
        ApiError, CLTyped, CLValue, Key, NamedKeys, URef,
        bytesrepr::{self, FromBytes, ToBytes},
    },
    utils,
};

/// A named key that refers to a key in the global state by its name.
///
/// The `NamedKey` struct provides a high-level API for interacting with named keys
/// in the global state. It allows for lazy resolution and caching of the underlying key,
/// as well as convenient methods for reading and writing values associated with the named key.
///
/// See also it's companion [`TypedURef`] for working with URefs in a type-safe manner.
#[derive(Clone)]
pub struct NamedKey {
    name: &'static str,
    key: Cell<Option<Result<Option<Key>, ApiError>>>,
}

unsafe impl Sync for NamedKey {}

impl NamedKey {
    /// Creates a new `NamedKey` instance from the given name.
    pub const fn from_name(name: &'static str) -> NamedKey {
        NamedKey {
            name,
            key: Cell::new(None),
        }
    }

    /// Retrieves the key associated with this named key, initializing it with the provided
    /// function if it does not already exist.
    ///
    /// Returns a reference to the `NamedKey` itself for chaining.
    pub fn get_or_init<F>(&self, f: F) -> Result<&NamedKey, ApiError>
    where
        F: FnOnce() -> Result<Key, ApiError>,
    {
        // Tries to resolve the named key - this should perform `get_key` operation first.
        let key = self.resolve_key()?;

        match key {
            Some(_) => {
                // There is a named key under this name; return it
                Ok(self)
            }
            None => {
                // There is no named key entry under this name; we need to create it.
                let new_key = f()?;
                self.key.replace(Some(Ok(Some(new_key))));
                Ok(self)
            }
        }
    }

    fn resolve_key(&self) -> Result<Option<Key>, ApiError> {
        if let Some(cached) = self.key.take() {
            match cached {
                Ok(opt_key) => {
                    self.key.set(Some(Ok(opt_key)));
                    return Ok(opt_key);
                }
                Err(e) => {
                    self.key.set(Some(Err(e)));
                    return Err(e);
                }
            }
        }

        let result = utils::get_key(self.name);
        match result {
            Ok(opt_key) => {
                self.key.set(Some(Ok(opt_key)));
                Ok(opt_key)
            }
            Err(e) => {
                self.key.set(Some(Err(e)));
                Err(e)
            }
        }
    }

    /// Resolves the URef associated with this named key, if it exists.
    fn resolve_uref(&self) -> Result<Option<URef>, ApiError> {
        let key = self.resolve_key()?;
        match key {
            Some(key) => Ok(key.into_uref()),
            None => Ok(None),
        }
    }

    /// Returns the name of this named key.
    pub const fn name(&self) -> &str {
        self.name
    }

    /// Retrieves the key from the global state under this named key.
    pub fn get(&self) -> Result<Option<Key>, ApiError> {
        self.resolve_key()
    }

    /// Takes the key instance from this named key, removing it from the cache.
    ///
    /// This only removes the cached value; it does not remove the named key from the global state.
    ///
    /// Useful for situations where you want to [`get_or_init`](Self::get_or_init) a new value
    /// (i.e. a new `URef` or dictionary) then you want to take the cached value and put it into
    /// named keys of a contract at installation time.
    pub fn take(&self) -> Result<Option<Key>, ApiError> {
        let key = self.resolve_key()?;
        self.key.replace(None);
        Ok(key)
    }

    /// Puts the named key into the global state.
    ///
    /// This is useful when initializing the named keys of a contract or account (in session).
    pub fn put_to_named_keys(&self) -> Result<&NamedKey, ApiError> {
        let key = self.resolve_key()?.ok_or(ApiError::MissingKey)?;
        utils::put_key(self.name, key)?;
        Ok(self)
    }

    /// Appends the named key to the given named keys map.
    ///
    /// This is useful when constructing a contract's named keys map before
    /// deploying it.
    pub fn append_to_named_keys(&self, named_keys: &mut NamedKeys) -> Result<&NamedKey, ApiError> {
        let key = self.resolve_key()?.ok_or(ApiError::MissingKey)?;
        named_keys.insert(self.name.into(), key);
        Ok(self)
    }

    /// Sets the key in the global state under this named key.
    ///
    /// This is equivaelnt to `put_key`. Due to high-level nature of this API it is named `set` to
    /// better reflect its purpose.
    pub fn set(&self, key: Key) -> Result<(), ApiError> {
        let _old_value = self.key.replace(Some(Ok(Some(key))));
        utils::put_key(self.name, key)?;
        Ok(())
    }

    /// Removes the key from the global state under this named key.
    pub fn clear(&self) {
        utils::remove_key(self.name);
    }

    /// Reads the value stored under this named key.
    ///
    /// This makes sense only if the named key refers to a [`URef`], although the execution engine allows reading from any [`Key`].
    pub fn read<T>(&self) -> Result<Option<T>, ApiError>
    where
        T: FromBytes,
    {
        let key = self.get()?;
        match key {
            Some(key) => utils::read_key(&key),
            None => Ok(None),
        }
    }

    /// Writes the value under this named key.
    ///
    pub fn write<T>(&self, value: &T) -> Result<(), ApiError>
    where
        T: ToBytes + CLTyped,
    {
        let key = self.resolve_key()?.ok_or(ApiError::MissingKey)?;
        utils::write_key(value, key)
    }

    /// Writes the value under the given dictionary item key.
    pub fn put_dict<K, V>(&self, dictionary_item_key: K, value: V) -> Result<(), ApiError>
    where
        K: AsRef<str>,
        V: CLTyped + ToBytes,
    {
        let cl_value = CLValue::from_t(value).unwrap_or_revert();
        self.put_dict_clvalue(dictionary_item_key, cl_value)?;
        Ok(())
    }

    fn put_dict_clvalue<A>(&self, key: A, value: CLValue) -> Result<(), ApiError>
    where
        A: AsRef<str>,
    {
        let uref = self.resolve_uref()?.ok_or(ApiError::MissingKey)?;
        utils::dictionary_put_clvalue(&uref, key.as_ref(), value)?;
        Ok(())
    }

    fn get_bytes<K>(&self, key: K) -> Result<Option<Vec<u8>>, ApiError>
    where
        K: AsRef<[u8]>,
    {
        let uref = self.resolve_uref()?.ok_or(ApiError::MissingKey)?;
        let bytes = utils::dictionary_get_bytes(&uref, key.as_ref())?;
        Ok(bytes)
    }

    pub fn get_dict<K, V>(&self, key: K) -> Result<Option<V>, ApiError>
    where
        K: AsRef<str>,
        V: bytesrepr::FromBytes + CLTyped,
    {
        let key: &str = key.as_ref();
        match self.get_bytes(key.as_bytes())? {
            Some(bytes) => {
                let value: V = bytesrepr::deserialize(bytes)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use veles_casper_ffi_shim::{EnvBuilder, HostFunction, dispatch_with};

    use super::*;

    const NAME: &str = "test_key";
    const EXPECTED_KEY: Key = Key::Hash([42u8; 32]);

    thread_local! {
        static NAMED_KEY: NamedKey = const { NamedKey::from_name(NAME) };
    }

    fn reset_named_key_cache() {
        NAMED_KEY.with(|named_key| {
            named_key.key.replace(None);
        });
    }

    fn with_named_key<F, R>(f: F) -> R
    where
        F: FnOnce(&NamedKey) -> R,
    {
        NAMED_KEY.with(|named_key| f(named_key))
    }

    #[test]
    fn test_named_key_set_and_get() {
        reset_named_key_cache();
        dispatch_with(EnvBuilder::new().build(), |env| {
            with_named_key(|named_key| {
                assert_eq!(named_key.get().unwrap(), None);
                assert_eq!(env.trace(), vec![HostFunction::CasperGetKey(NAME.into()),]);

                named_key.set(EXPECTED_KEY).unwrap();
                assert_eq!(
                    env.trace(),
                    vec![HostFunction::CasperPutKey(NAME.into(), EXPECTED_KEY),]
                );

                let retrieved_key = named_key.get().unwrap().unwrap();
                assert_eq!(retrieved_key, EXPECTED_KEY);

                assert_eq!(
                    env.trace(),
                    vec![
                        // No `casper_get_key` calls here because of caching
                    ]
                );
            });
        });
    }

    #[test]
    fn resolve_pre_existing() {
        reset_named_key_cache();
        let env = EnvBuilder::new().with_named_key(NAME, EXPECTED_KEY).build();

        dispatch_with(env, |env| {
            with_named_key(|named_key| {
                assert_eq!(named_key.get().unwrap().unwrap(), EXPECTED_KEY);
                assert_eq!(env.trace(), vec![HostFunction::CasperGetKey(NAME.into()),]);
            });
        });
    }

    #[test]
    fn test_named_key_get_or_init_creates_new() {
        reset_named_key_cache();
        dispatch_with(EnvBuilder::new().build(), |env| {
            with_named_key(|named_key| {
                let result = named_key.get_or_init(|| Ok(EXPECTED_KEY));
                assert!(result.is_ok());

                assert_eq!(env.trace(), vec![HostFunction::CasperGetKey(NAME.into()),]);

                // Get or init should not create a named key (yet)
                assert!(!env.named_keys().contains_key(NAME));

                // Cached value exists now, but it's not in sync with the context's named keys (and that's fine for situation where you want to create new dictionary and put it to named keys of contract)
                let retrieved_key = named_key.get().unwrap().unwrap();
                assert_eq!(retrieved_key, EXPECTED_KEY);

                named_key.put_to_named_keys().unwrap();

                assert_eq!(
                    env.trace(),
                    vec![HostFunction::CasperPutKey(NAME.into(), EXPECTED_KEY),]
                );
            });
        });
    }

    #[test]
    fn test_named_key_get_or_init_returns_existing() {
        reset_named_key_cache();
        let env = EnvBuilder::new().with_named_key(NAME, EXPECTED_KEY).build();

        dispatch_with(env, |env| {
            with_named_key(|named_key| {
                let result = named_key.get_or_init(|| Ok(Key::Hash([99u8; 32])));
                assert!(result.is_ok());

                let retrieved_key = named_key.get().unwrap().unwrap();
                assert_eq!(retrieved_key, EXPECTED_KEY);

                assert_eq!(env.trace(), vec![HostFunction::CasperGetKey(NAME.into()),]);
            });
        });
    }

    #[test]
    fn test_named_key_clear() {
        reset_named_key_cache();
        let env = EnvBuilder::new().with_named_key(NAME, EXPECTED_KEY).build();

        dispatch_with(env, |env| {
            with_named_key(|named_key| {
                named_key.clear();

                assert_eq!(
                    env.trace(),
                    vec![HostFunction::CasperRemoveKey(NAME.into()),]
                );
            });
        });
    }

    #[test]
    fn test_named_key_caching() {
        reset_named_key_cache();
        let env = EnvBuilder::new().with_named_key(NAME, EXPECTED_KEY).build();

        dispatch_with(env, |env| {
            with_named_key(|named_key| {
                // First call should query the host
                assert_eq!(named_key.get().unwrap().unwrap(), EXPECTED_KEY);
                assert_eq!(env.trace(), vec![HostFunction::CasperGetKey(NAME.into())]);

                // Second call should use cached value
                assert_eq!(named_key.get().unwrap().unwrap(), EXPECTED_KEY);
                assert_eq!(env.trace(), vec![]);

                // Third call should also use cached value
                assert_eq!(named_key.get().unwrap().unwrap(), EXPECTED_KEY);
                assert_eq!(env.trace(), vec![]);
            });
        });
    }

    #[test]
    fn test_named_key_name() {
        reset_named_key_cache();
        with_named_key(|named_key| {
            assert_eq!(named_key.name(), NAME);
        });
    }

    #[test]
    fn test_named_key_put_to_named_keys() {
        reset_named_key_cache();
        let env = EnvBuilder::new().with_named_key(NAME, EXPECTED_KEY).build();

        dispatch_with(env, |env| {
            with_named_key(|named_key| {
                let result = named_key.put_to_named_keys();
                assert!(result.is_ok());

                assert_eq!(
                    env.trace(),
                    vec![
                        HostFunction::CasperGetKey(NAME.into()),
                        HostFunction::CasperPutKey(NAME.into(), EXPECTED_KEY),
                    ]
                );
            });
        });
    }

    #[test]
    fn test_named_key_append_to_named_keys() {
        reset_named_key_cache();
        let env = EnvBuilder::new().with_named_key(NAME, EXPECTED_KEY).build();

        dispatch_with(env, |_env| {
            with_named_key(|named_key| {
                let mut named_keys = NamedKeys::new();
                let result = named_key.append_to_named_keys(&mut named_keys);
                assert!(result.is_ok());

                assert_eq!(named_keys.get(NAME), Some(&EXPECTED_KEY));
            });
        });
    }

    #[test]
    fn test_named_key_take() {
        reset_named_key_cache();
        let env = EnvBuilder::new().build();

        dispatch_with(env, |env| {
            with_named_key(|named_key| {
                let _key = named_key.get_or_init(|| Ok(EXPECTED_KEY)).unwrap();
                assert!(!env.named_keys().contains_key(NAME));

                let taken_key = named_key.take().unwrap().unwrap();
                assert_eq!(taken_key, EXPECTED_KEY);

                assert_eq!(env.trace(), vec![HostFunction::CasperGetKey(NAME.into()),]);

                // Subsequent get should re-query the host
                let taken_key_2 = named_key.get().unwrap();
                assert_eq!(taken_key_2, None);

                assert_eq!(env.trace(), vec![HostFunction::CasperGetKey(NAME.into()),]);
            });
        });
    }
}
