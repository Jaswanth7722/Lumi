//! # Secret<T> Newtype
//!
//! Wraps sensitive configuration values so they can never appear in logs,
//! error messages, or serialized output.
//!
//! # Thread Safety
//!
//! `Secret<T>` is `Send + Sync` when `T: Send + Sync`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Wraps a sensitive value to prevent accidental exposure in logs or serialization.
///
/// Debug output is always `Secret(***REDACTED***)`.
/// Display output is always `***REDACTED***`.
/// Serialize always outputs the redacted marker string.
/// Deserialize works normally (loading from a file is OK).
///
/// The only way to access the inner value is via [`expose_secret()`](Secret::expose_secret).
///
/// # Examples
///
/// ```ignore
/// let key = Secret::new("sk-ant-abc123".to_string());
/// assert_eq!(format!("{key:?}"), "Secret(***REDACTED***)");
/// assert_eq!(key.expose_secret(), "sk-ant-abc123");
/// ```
pub struct Secret<T>(T);

impl<T> Secret<T> {
    /// Wrap a value in a Secret.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Access the inner value. Callers must handle the exposed value carefully
    /// and never log it or include it in error messages.
    pub fn expose_secret(&self) -> &T {
        &self.0
    }

    /// Consume the Secret and return the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Clone> Clone for Secret<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: PartialEq> PartialEq for Secret<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: fmt::Debug> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret(***REDACTED***)")
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "***REDACTED***")
    }
}

// Serde: deserialize normally, serialize as redacted
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Secret<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Secret::new)
    }
}

impl<T: Serialize> Serialize for Secret<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Always serialize as the redacted marker string
        serializer.serialize_str("***REDACTED***")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_debug_redacted() {
        let secret = Secret::new("super-secret-key");
        let debug_str = format!("{secret:?}");
        assert!(debug_str.contains("***REDACTED***"));
        assert!(!debug_str.contains("super-secret-key"));
    }

    #[test]
    fn test_secret_display_redacted() {
        let secret = Secret::new("super-secret-key");
        let display_str = format!("{secret}");
        assert_eq!(display_str, "***REDACTED***");
    }

    #[test]
    fn test_expose_secret() {
        let secret = Secret::new("api-key-123");
        assert_eq!(*secret.expose_secret(), "api-key-123");
    }

    #[test]
    fn test_secret_deserialize_from_str() {
        let json = "\"sk-ant-test-key\"";
        let secret: Secret<String> = serde_json::from_str(json).unwrap();
        assert_eq!(secret.expose_secret(), "sk-ant-test-key");
    }

    #[test]
    fn test_secret_serialize_redacted() {
        let secret = Secret::new("should-not-appear");
        let json = serde_json::to_string(&secret).unwrap();
        assert_eq!(json, "\"***REDACTED***\"");
    }

    #[test]
    fn test_secret_clone() {
        let secret = Secret::new(String::from("clone-me"));
        let cloned = secret.clone();
        assert_eq!(cloned.expose_secret().as_str(), "clone-me");
    }

    #[test]
    fn test_secret_into_inner() {
        let secret = Secret::new("consume-me".to_string());
        let inner: String = secret.into_inner();
        assert_eq!(inner, "consume-me");
    }
}
