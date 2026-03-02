pub use secrecy::ExposeSecret;
use secrecy::SecretString;
use std::fmt;

#[derive(Default, Clone)]
pub struct SensitiveString(SecretString);

impl SensitiveString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(SecretString::new(value.into().into_boxed_str()))
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

pub mod serde_sensitive_string {
    use super::{SensitiveString, sensitive_string};
    use secrecy::ExposeSecret;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &SensitiveString, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(value.expose_secret())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SensitiveString, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(sensitive_string(value))
    }
}
