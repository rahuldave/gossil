//! The 128-bit identifier of every entity, written as 32 letters from `k` to `z`.
//!
//! Ported from `src/store/model/primitives/id.rs` at the Gest pin. The random
//! bytes come from `getrandom` rather than `rand`. Serde decoding goes through
//! [`FromStr`], so invalid text is a decoding error and not a panic.

use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use libsql::Value;
use serde::{Deserialize, Serialize};

/// A 128-bit identifier encoded as a 32-character string using the `k`-`z` alphabet.
///
/// Each byte is split into two nibbles and mapped through a reverse-hex alphabet,
/// producing a lowercase, URL-safe string that sorts in reverse byte order.
/// The encoding is bijective. Every valid 32-character `[k-z]` string decodes
/// to exactly one `[u8; 16]`, and each `[u8; 16]` encodes to exactly one such string.
///
/// # Examples
///
/// ```
/// # use gossil::gest_model::primitives::Id;
/// let id = Id::new();
/// assert_eq!(id.to_string().len(), 32);
///
/// let parsed: Id = id.to_string().parse().unwrap();
/// assert_eq!(id, parsed);
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(into = "String")]
pub struct Primitive([u8; 16]);

impl<'de> Deserialize<'de> for Primitive {
    /// Decode an identifier string through [`FromStr`].
    ///
    /// Upstream decodes through the panicking `From<String>`. Here a wrong
    /// length or a letter outside `k` to `z` is a decoding error.
    ///
    /// # Arguments
    ///
    /// * `deserializer` - the input, which must hold a string
    ///
    /// # Returns
    ///
    /// The identifier that the string encodes.
    ///
    /// # Errors
    ///
    /// Returns the error of the deserializer for a value that is not a string,
    /// or for a string that is not a valid identifier.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        encoded.parse().map_err(serde::de::Error::custom)
    }
}

impl Primitive {
    /// Alphabet used to encode bytes into the `k`-`z` character range.
    const REVERSE_HEX_CHARS: &[u8; 16] = b"zyxwvutsrqponmlk";

    /// Generate a new random `Id` from the random source of the operating system.
    ///
    /// # Returns
    ///
    /// An id of 16 random bytes.
    ///
    /// # Panics
    ///
    /// Panics when the operating system gives no random bytes. Upstream `rand`
    /// panics in the same case.
    pub fn new() -> Self {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("the operating system gave no random bytes");
        Self(bytes)
    }

    /// Validate that `s` is a valid ID prefix of 1 to 32 characters, all in `[k-z]`.
    ///
    /// # Arguments
    ///
    /// * `s` - the prefix to check
    ///
    /// # Returns
    ///
    /// Returns `Ok(s)` on success so callers can chain the result.
    ///
    /// # Errors
    ///
    /// Rejects empty strings, strings longer than 32 bytes, and any character
    /// outside the ID alphabet. The error message says what is wrong.
    pub fn validate_prefix(s: &str) -> Result<&str, String> {
        if s.is_empty() {
            return Err("Id prefix must not be empty".to_string());
        }
        if s.len() > 32 {
            return Err(format!(
                "Id prefix must be at most 32 characters, got {}",
                s.len()
            ));
        }
        if let Some(c) = s.chars().find(|c| !('k'..='z').contains(c)) {
            return Err(format!(
                "Id prefix must contain only characters k-z, found '{c}'"
            ));
        }
        Ok(s)
    }

    /// Returns the first 8 characters of the encoded ID as a short prefix.
    ///
    /// # Returns
    ///
    /// The characters that encode the first 4 bytes.
    pub fn short(&self) -> String {
        Self::encode_prefix(&self.0)
    }

    /// Decode a 32-character `[k-z]` string back into 16 raw bytes.
    ///
    /// # Arguments
    ///
    /// * `s` - the encoded id
    ///
    /// # Returns
    ///
    /// The 16 bytes that `s` encodes.
    ///
    /// # Errors
    ///
    /// Returns a message when `s` is not 32 bytes long or holds a character
    /// outside `k`-`z`.
    fn decode(s: &str) -> Result<[u8; 16], String> {
        let chars = s.as_bytes();
        if chars.len() != 32 {
            return Err(format!(
                "Id must be exactly 32 characters, got {}",
                chars.len()
            ));
        }

        let mut bytes = [0u8; 16];
        for (byte, pair) in bytes.iter_mut().zip(chars.chunks_exact(2)) {
            let high = Self::nibble_from_char(pair[0])?;
            let low = Self::nibble_from_char(pair[1])?;
            *byte = (high << 4) | low;
        }
        Ok(bytes)
    }

    /// Encode 16 raw bytes into a 32-character `[k-z]` string.
    ///
    /// # Arguments
    ///
    /// * `bytes` - the raw id
    ///
    /// # Returns
    ///
    /// The encoded id, two characters per byte, high nibble first.
    fn encode(bytes: &[u8; 16]) -> String {
        Self::encode_bytes(bytes)
    }

    /// Encode only the first 4 bytes into an 8-character string for [`Primitive::short`].
    ///
    /// # Arguments
    ///
    /// * `bytes` - the raw id
    ///
    /// # Returns
    ///
    /// The 8-character prefix.
    fn encode_prefix(bytes: &[u8; 16]) -> String {
        Self::encode_bytes(&bytes[..4])
    }

    /// Encode each byte as two characters, high nibble first.
    ///
    /// Upstream repeats this loop in `encode` and `encode_prefix`. Both call it here.
    ///
    /// # Arguments
    ///
    /// * `bytes` - the bytes to encode
    ///
    /// # Returns
    ///
    /// A string of twice as many characters as `bytes` has bytes.
    fn encode_bytes(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            s.push(Self::REVERSE_HEX_CHARS[usize::from(b >> 4)] as char);
            s.push(Self::REVERSE_HEX_CHARS[usize::from(b & 0x0F)] as char);
        }
        s
    }

    /// Convert a single `[k-z]` character byte to its nibble value (`0..=15`).
    ///
    /// # Arguments
    ///
    /// * `c` - the character, as a byte
    ///
    /// # Returns
    ///
    /// The nibble, where `z` is 0 and `k` is 15.
    ///
    /// # Errors
    ///
    /// Returns a message for any byte outside `k`-`z`.
    fn nibble_from_char(c: u8) -> Result<u8, String> {
        match c {
            b'k'..=b'z' => Ok(b'z' - c),
            _ => Err(format!(
                "Id must contain only characters k-z, found '{}'",
                c as char
            )),
        }
    }
}

/// Delegates to [`Primitive::new`], generating a fresh random identifier.
impl Default for Primitive {
    /// Generate a fresh random identifier.
    ///
    /// # Panics
    ///
    /// Panics when the operating system gives no random bytes, as
    /// [`Primitive::new`] does.
    fn default() -> Self {
        Self::new()
    }
}

/// Formats the ID as a 32-character `[k-z]` encoded string.
impl Display for Primitive {
    /// Write the 32 encoded characters.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&Self::encode(&self.0))
    }
}

/// Converts to the 32-character encoded string representation.
impl From<Primitive> for String {
    /// Return the 32 encoded characters.
    fn from(id: Primitive) -> Self {
        id.to_string()
    }
}

/// Converts to a [`Value::Text`] for storage in libsql.
impl From<Primitive> for Value {
    /// Return the 32 encoded characters as SQL text.
    fn from(id: Primitive) -> Self {
        Value::from(id.to_string())
    }
}

/// Parses a 32-character `[k-z]` string into an `Id`.
///
/// Returns an error if the string is not exactly 32 characters or contains
/// characters outside the `[k-z]` range.
impl FromStr for Primitive {
    /// The message that says why the text is not an identifier.
    type Err = String;

    /// Parse 32 characters from `k` to `z`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 32 {
            return Err(format!("Id must be exactly 32 characters, got {}", s.len()));
        }
        let bytes = Self::decode(s)?;
        Ok(Self(bytes))
    }
}

/// Parses the string into an `Id`, panicking on invalid input.
///
/// Prefer [`FromStr`] when handling untrusted input.
impl From<String> for Primitive {
    /// Parse text that the caller already knows is a valid identifier.
    ///
    /// # Panics
    ///
    /// Panics when the text is not 32 characters from `k` to `z`, as upstream does.
    fn from(s: String) -> Self {
        s.parse().unwrap_or_else(|e| panic!("invalid Id: {e}"))
    }
}

/// Parses the string into an `Id`, panicking on invalid input.
///
/// Prefer [`FromStr`] when handling untrusted input.
impl From<&str> for Primitive {
    /// Parse text that the caller already knows is a valid identifier.
    ///
    /// # Panics
    ///
    /// Panics when the text is not 32 characters from `k` to `z`, as upstream does.
    fn from(s: &str) -> Self {
        s.parse().unwrap_or_else(|e| panic!("invalid Id: {e}"))
    }
}

/// The inherited tests that need the private encoder or the private byte array.
///
/// The other inherited `id.rs` tests use only the public API, so they live in
/// `tests/gest_model.rs`.
#[cfg(test)]
mod tests {
    use super::*;

    /// Inherited from the `encode_decode` module of upstream `id.rs`.
    mod encode_decode {
        use super::*;

        #[test]
        fn it_encodes_ff_bytes_as_k() {
            let bytes = [0xFF; 16];
            assert_eq!(
                Primitive::encode(&bytes),
                "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"
            );
        }

        #[test]
        fn it_encodes_zero_bytes_as_z() {
            let bytes = [0u8; 16];
            assert_eq!(
                Primitive::encode(&bytes),
                "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
            );
        }

        #[test]
        fn it_roundtrips_encode_decode() {
            let bytes = [
                0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB,
                0xCD, 0xEF,
            ];
            let encoded = Primitive::encode(&bytes);
            let decoded = Primitive::decode(&encoded).unwrap();
            assert_eq!(bytes, decoded);
        }
    }

    /// Inherited from the `display` module of upstream `id.rs`.
    mod display {
        use super::*;

        #[test]
        fn it_encodes_all_ff_bytes_as_all_k() {
            let id = Primitive([0xFF; 16]);
            assert_eq!(id.to_string(), "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk");
        }
    }

    /// The private decoder returns an error for a wrong length before it reads a character.
    #[test]
    fn decode_refuses_a_wrong_length() {
        assert_eq!(
            Primitive::decode("zzz").unwrap_err(),
            "Id must be exactly 32 characters, got 3"
        );
    }
}
