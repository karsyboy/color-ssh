//! Sensitive in-memory string/buffer helpers.
//!
//! These wrappers reduce accidental secret exposure in logs and ensure buffers
//! are zeroized when dropped.

pub use secrecy::ExposeSecret;
use secrecy::SecretString;
use std::fmt;
use std::str;
use zeroize::Zeroize;

#[derive(Default, Clone)]
/// Redacted wrapper around a secret string.
pub struct SensitiveString(SecretString);

impl SensitiveString {
    /// Create a new sensitive string from owned or borrowed input.
    pub fn new(value: impl Into<String>) -> Self {
        Self(SecretString::new(value.into().into_boxed_str()))
    }

    /// Build directly from an owned `String`.
    pub fn from_owned_string(value: String) -> Self {
        Self(SecretString::new(value.into_boxed_str()))
    }

    /// Decode UTF-8 bytes into a sensitive string.
    pub fn from_utf8_bytes(value: Vec<u8>) -> Result<Self, std::string::FromUtf8Error> {
        String::from_utf8(value).map(Self::from_owned_string)
    }
}

impl ExposeSecret<str> for SensitiveString {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl PartialEq for SensitiveString {
    fn eq(&self, other: &Self) -> bool {
        self.expose_secret() == other.expose_secret()
    }
}

impl Eq for SensitiveString {}

impl fmt::Debug for SensitiveString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for SensitiveString {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for SensitiveString {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

pub fn sensitive_string(value: impl Into<String>) -> SensitiveString {
    SensitiveString::new(value)
}

#[derive(Default)]
/// Editable secret buffer used by interactive prompts.
pub struct SensitiveBuffer {
    bytes: Vec<u8>,
    len: usize,
}

impl SensitiveBuffer {
    /// Create an empty secret buffer.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn char_len(&self) -> usize {
        self.as_str().map(|value| value.chars().count()).unwrap_or(0)
    }

    pub fn clear(&mut self) {
        self.bytes.zeroize();
        self.len = 0;
    }

    pub fn insert_char(&mut self, cursor_chars: usize, ch: char) {
        let mut encoded = [0u8; 4];
        let encoded = ch.encode_utf8(&mut encoded).as_bytes();
        let insert_at = self.byte_index_for_char(cursor_chars);
        self.secure_reserve(encoded.len());
        self.bytes.copy_within(insert_at..self.len, insert_at + encoded.len());
        self.bytes[insert_at..insert_at + encoded.len()].copy_from_slice(encoded);
        self.len += encoded.len();
    }

    pub fn backspace_char(&mut self, cursor_chars: usize) -> usize {
        if cursor_chars == 0 {
            return 0;
        }
        let end = self.byte_index_for_char(cursor_chars);
        let start = self.byte_index_for_char(cursor_chars - 1);
        self.remove_range(start, end);
        cursor_chars - 1
    }

    pub fn delete_char(&mut self, cursor_chars: usize) -> usize {
        let len = self.char_len();
        if cursor_chars >= len {
            return len;
        }
        let start = self.byte_index_for_char(cursor_chars);
        let end = self.byte_index_for_char(cursor_chars + 1);
        self.remove_range(start, end);
        cursor_chars
    }

    pub fn masked(&self) -> String {
        "*".repeat(self.char_len())
    }

    pub fn as_str(&self) -> Result<&str, str::Utf8Error> {
        str::from_utf8(&self.bytes[..self.len])
    }

    pub fn into_sensitive_string(mut self) -> Result<SensitiveString, std::string::FromUtf8Error> {
        let len = self.len;
        let bytes = std::mem::take(&mut self.bytes);
        self.len = 0;
        let mut active = bytes;
        active.truncate(len);
        SensitiveString::from_utf8_bytes(active)
    }

    fn byte_index_for_char(&self, char_index: usize) -> usize {
        let Ok(text) = self.as_str() else {
            return self.len;
        };
        if char_index == 0 {
            return 0;
        }

        let max = text.chars().count();
        let clamped = char_index.min(max);
        if clamped == max {
            return self.len;
        }

        text.char_indices().nth(clamped).map_or(self.len, |(byte_index, _)| byte_index)
    }

    fn secure_reserve(&mut self, additional: usize) {
        let required = self.len.saturating_add(additional);
        if required <= self.bytes.len() {
            return;
        }

        let doubled = self.bytes.len().saturating_mul(2).max(8);
        let new_capacity = doubled.max(required);
        let mut new_bytes = vec![0u8; new_capacity];
        new_bytes[..self.len].copy_from_slice(&self.bytes[..self.len]);
        self.bytes.zeroize();
        self.bytes = new_bytes;
    }

    fn remove_range(&mut self, start: usize, end: usize) {
        if start >= end || end > self.len {
            return;
        }
        let removed = end - start;
        self.bytes.copy_within(end..self.len, start);
        let tail_start = self.len - removed;
        self.bytes[tail_start..self.len].zeroize();
        self.len -= removed;
    }
}

impl fmt::Debug for SensitiveBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl Drop for SensitiveBuffer {
    fn drop(&mut self) {
        self.bytes.zeroize();
        self.len = 0;
    }
}

pub mod serde_sensitive_string {
    use super::{SensitiveString, sensitive_string};
    use secrecy::ExposeSecret;
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serializer};
    use std::fmt;

    /// Serialize a sensitive string as plain text for protocol payloads.
    pub fn serialize<S>(value: &SensitiveString, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(value.expose_secret())
    }

    /// Deserialize a sensitive string from plain text.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<SensitiveString, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SensitiveStringVisitor;

        impl<'de> Visitor<'de> for SensitiveStringVisitor {
            type Value = SensitiveString;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a secret string")
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(sensitive_string(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(sensitive_string(value))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(SensitiveString::from_owned_string(value))
            }
        }

        deserializer.deserialize_string(SensitiveStringVisitor)
    }
}

#[cfg(test)]
#[path = "../test/auth/secret.rs"]
mod tests;
