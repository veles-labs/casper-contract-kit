//! Base128 encoding and decoding for byte arrays.
//!
//! This module provides functions to encode byte arrays into a base128 string
//! and decode base128 strings back into byte arrays. The encoding uses 7 bits
//! per character, allowing for efficient storage and transmission of binary data
//! in a text-friendly format.
//!
//! The encoding scheme represents each byte as one or more base128 characters,
//! where each character is in the range 0x00 to 0x7F. The first character contains
//! the highest 7 bits, the second character the next 7 bits, and so on. If the total
//! number of bits is not a multiple of 7, the last character is padded with zeros in the least
//! significant bits.
use alloc::{string::String, vec::Vec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    EmptyInput,
    InvalidDigit(u8),
    Overflow,
    NonZeroPadding,
}

pub fn encode_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::from("\0");
    }

    let mut result = Vec::new();
    let mut bit_buffer: u32 = 0;
    let mut bits_in_buffer = 0usize;

    for &byte in bytes {
        bit_buffer = (bit_buffer << 8) | u32::from(byte);
        bits_in_buffer += 8;

        while bits_in_buffer >= 7 {
            bits_in_buffer -= 7;
            let digit = ((bit_buffer >> bits_in_buffer) & 0x7F) as u8;
            result.push(digit);
            if bits_in_buffer == 0 {
                bit_buffer = 0;
            } else {
                bit_buffer &= (1u32 << bits_in_buffer) - 1;
            }
        }
    }

    if bits_in_buffer > 0 {
        let digit = (bit_buffer << (7 - bits_in_buffer)) as u8 & 0x7F;
        result.push(digit);
    }

    debug_assert!(str::from_utf8(&result).is_ok());
    unsafe { String::from_utf8_unchecked(result) }
}

pub fn decode_bytes(digits: &str) -> Result<Vec<u8>, DecodeError> {
    let digits = digits.as_bytes();
    if digits.is_empty() {
        return Err(DecodeError::EmptyInput);
    }
    if digits.len() == 1 && digits[0] == 0 {
        return Ok(Vec::new());
    }

    let mut bytes = Vec::new();
    let mut bit_buffer: u32 = 0;
    let mut bits_in_buffer = 0usize;

    for &digit in digits {
        if digit > 0x7F {
            return Err(DecodeError::InvalidDigit(digit));
        }

        bit_buffer = (bit_buffer << 7) | u32::from(digit);
        bits_in_buffer += 7;

        while bits_in_buffer >= 8 {
            bits_in_buffer -= 8;
            let byte = ((bit_buffer >> bits_in_buffer) & 0xFF) as u8;
            bytes.push(byte);
            if bits_in_buffer == 0 {
                bit_buffer = 0;
            } else {
                bit_buffer &= (1u32 << bits_in_buffer) - 1;
            }
        }
    }

    if bits_in_buffer > 0 {
        let mask = (1u32 << bits_in_buffer) - 1;
        if bit_buffer & mask != 0 {
            return Err(DecodeError::NonZeroPadding);
        }
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use alloc::{
        string::{String, ToString},
        vec,
    };
    use proptest::prelude::*;

    #[test]
    fn encode_value_examples() {
        assert_eq!(
            super::encode_bytes(&0u64.to_le_bytes()),
            String::from("\0\0\0\0\0\0\0\0\0\0")
        );

        let result = super::encode_bytes(&u64::MAX.to_le_bytes());
        assert_eq!(result.len(), 10);
        assert_eq!(u64::MAX.to_string().len(), 20);

        let truth = "\u{7f}\u{7f}\u{7f}\u{7f}\u{7f}\u{7f}\u{7f}\u{7f}\u{7f}@";
        assert_eq!(&result, truth);
    }

    #[test]
    fn encode_bytes_examples() {
        let encoded = super::encode_bytes(&[255u8; 32]);
        assert_eq!(encoded.len(), 37);

        let bytes = u64::MAX.to_be_bytes();
        let encoded = super::encode_bytes(&bytes);
        assert_eq!(super::decode_bytes(&encoded).unwrap(), bytes);
    }

    #[test]
    fn roundtrip_known_values() {
        let encoded = super::encode_bytes(&42u64.to_le_bytes());
        assert_eq!(super::decode_bytes(&encoded).unwrap(), 42u64.to_le_bytes());

        let blob = vec![0u8, 1, 2, 200, 255];
        let encoded = super::encode_bytes(&blob);
        assert_eq!(super::decode_bytes(&encoded).unwrap(), blob);
    }

    proptest! {
        #[test]
        fn proptest_u64_roundtrip(value in any::<u64>()) {
            let encoded = super::encode_bytes(&value.to_le_bytes());
            let decoded = super::decode_bytes(&encoded).unwrap();
            let orig_value = u64::from_le_bytes(decoded.try_into().unwrap());
            prop_assert_eq!(orig_value, value);
        }

        #[test]
        fn proptest_bytes_roundtrip(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
            let encoded = super::encode_bytes(&bytes);
            let decoded = super::decode_bytes(&encoded).unwrap();
            prop_assert_eq!(decoded, bytes);
        }
    }
}
