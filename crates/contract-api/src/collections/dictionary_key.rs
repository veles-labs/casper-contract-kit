use alloc::{borrow::Cow, string::String};
use casper_types::{U256, account::AccountHash, contracts::ContractHash};

use crate::collections::base128;

/// A trait for types that can be used as dictionary keys.
///
/// This implementation uses base128 encoding for binary data to ensure that the resulting
/// dictionary key is a valid UTF-8 string and can be safely with the casper's dictionary API.
///
/// The choice of base128 encoding is motivated by the need to efficiently represent binary data
/// in a text-friendly format, while minimizing the size of the encoded string. Compared to base64
/// encoding, base128 uses less overhead for the same amount of binary data, making it more suitable for
/// use cases where dictionary keys need to be compact.
///
/// This may not be backwards-compatible with pre-existing code using base64 or other encoding.
pub trait DictionaryKey<'a> {
    fn dictionary_key(&'a self) -> Cow<'a, str>;
}

impl<'a, T> DictionaryKey<'a> for &'a T
where
    T: DictionaryKey<'a>,
{
    fn dictionary_key(&'a self) -> Cow<'a, str> {
        (**self).dictionary_key()
    }
}

impl<'a> DictionaryKey<'a> for str {
    fn dictionary_key(&'a self) -> Cow<'a, str> {
        Cow::Borrowed(self)
    }
}

impl DictionaryKey<'_> for u32 {
    fn dictionary_key(&self) -> Cow<'_, str> {
        Cow::Owned(base128::encode_bytes(&self.to_le_bytes()))
    }
}

impl DictionaryKey<'_> for u64 {
    fn dictionary_key(&self) -> Cow<'_, str> {
        Cow::Owned(base128::encode_bytes(&self.to_le_bytes()))
    }
}

impl<const N: usize> DictionaryKey<'_> for [u8; N] {
    fn dictionary_key(&self) -> Cow<'_, str> {
        Cow::Owned(base128::encode_bytes(self))
    }
}

impl DictionaryKey<'_> for AccountHash {
    fn dictionary_key(&self) -> Cow<'_, str> {
        Cow::Owned(base128::encode_bytes(self.as_bytes()))
    }
}

impl DictionaryKey<'_> for ContractHash {
    fn dictionary_key(&self) -> Cow<'_, str> {
        Cow::Owned(base128::encode_bytes(self.as_bytes()))
    }
}

impl DictionaryKey<'_> for U256 {
    fn dictionary_key(&self) -> Cow<'_, str> {
        let mut bytes = [0u8; 32];
        self.to_little_endian(&mut bytes);
        Cow::Owned(base128::encode_bytes(&bytes))
    }
}
const TUPLE_DELIMITER: char = ':';

macro_rules! impl_dictionary_key_for_tuple {
    ( $( ($idx:tt, $T:ident) ),+ ) => {
        impl<'a, $($T),+> DictionaryKey<'a> for ($( $T, )+)
        where
            $($T: DictionaryKey<'a>,)+
        {
            fn dictionary_key(&'a self) -> Cow<'a, str> {
                let parts = [
                    $(self.$idx.dictionary_key(),)+
                ];
                let separators = parts.len().saturating_sub(1);
                let capacity = parts.iter().map(|part| part.len()).sum::<usize>() + separators;
                let mut combined = String::with_capacity(capacity);
                for (idx, part) in parts.iter().enumerate() {
                    if idx != 0 {
                        combined.push(TUPLE_DELIMITER);
                    }
                    combined.push_str(part);
                }
                Cow::Owned(combined)
            }
        }
    };
}

impl_dictionary_key_for_tuple!((0, T1));
impl_dictionary_key_for_tuple!((0, T1), (1, T2));
impl_dictionary_key_for_tuple!((0, T1), (1, T2), (2, T3));
impl_dictionary_key_for_tuple!((0, T1), (1, T2), (2, T3), (3, T4));
impl_dictionary_key_for_tuple!((0, T1), (1, T2), (2, T3), (3, T4), (4, T5));
impl_dictionary_key_for_tuple!((0, T1), (1, T2), (2, T3), (3, T4), (4, T5), (5, T6));
impl_dictionary_key_for_tuple!(
    (0, T1),
    (1, T2),
    (2, T3),
    (3, T4),
    (4, T5),
    (5, T6),
    (6, T7)
);
impl_dictionary_key_for_tuple!(
    (0, T1),
    (1, T2),
    (2, T3),
    (3, T4),
    (4, T5),
    (5, T6),
    (6, T7),
    (7, T8)
);
impl_dictionary_key_for_tuple!(
    (0, T1),
    (1, T2),
    (2, T3),
    (3, T4),
    (4, T5),
    (5, T6),
    (6, T7),
    (7, T8),
    (8, T9)
);
impl_dictionary_key_for_tuple!(
    (0, T1),
    (1, T2),
    (2, T3),
    (3, T4),
    (4, T5),
    (5, T6),
    (6, T7),
    (7, T8),
    (8, T9),
    (9, T10)
);

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    #[test]
    fn test_dictionary_key_u64() {
        let a = U256::MAX;
        let b = u64::MAX;
        let compound = (a, b);
        let key = compound.dictionary_key();
        let toks = key.split(TUPLE_DELIMITER).collect::<Vec<_>>();
        assert_eq!(toks.len(), 2);
        assert_eq!(base128::decode_bytes(toks[0]), Ok(vec![255u8; 32]));
        assert_eq!(base128::decode_bytes(toks[1]), Ok(b.to_le_bytes().to_vec()));
    }

    #[test]
    fn triple_key() {
        let a = U256::MAX;
        let b = u64::MAX;
        let c = 123u32;
        let compound = ((a, b), c);
        let key = compound.dictionary_key();
        let toks = key.split(TUPLE_DELIMITER).collect::<Vec<_>>();
        assert_eq!(toks.len(), 3);
        assert_eq!(base128::decode_bytes(toks[0]), Ok(vec![255u8; 32]));
        assert_eq!(base128::decode_bytes(toks[1]), Ok(b.to_le_bytes().to_vec()));
        assert_eq!(base128::decode_bytes(toks[2]), Ok(c.to_le_bytes().to_vec()));
    }
}
