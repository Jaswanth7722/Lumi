//! # Redaction Engine
//!
//! Scans every log record for sensitive patterns before reaching any sink.
//! Must be fast (< 1µs per record with no matching rules) and extensible.

use crate::error::LogError;
use crate::record::{FieldValue, LogRecord};
use once_cell::sync::Lazy;
use regex::Regex;

/// Built-in secret patterns for automatic redaction.
pub const BUILTIN_PATTERNS: &[(&str, &str)] = &[
    ("anthropic_api_key", r"sk-ant-[A-Za-z0-9\-_]{20,}"),
    ("openai_api_key", r"sk-[A-Za-z0-9]{20,}"),
    ("github_token", r"ghp_[A-Za-z0-9]{36}"),
    ("bearer_token", r"(?i)bearer\s+[A-Za-z0-9\-._~+/]+=*"),
    ("basic_auth", r"(?i)basic\s+[A-Za-z0-9+/]+=*"),
    ("private_key_block", r"-----BEGIN [A-Z ]+ PRIVATE KEY-----"),
    (
        "credit_card",
        r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13})\b",
    ),
    ("ssn", r"\b\d{3}-\d{2}-\d{4}\b"),
    (
        "email_address",
        r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b",
    ),
];

/// Compiled built-in patterns.
static BUILTIN_REGEXES: Lazy<Vec<(String, Regex)>> = Lazy::new(|| {
    BUILTIN_PATTERNS
        .iter()
        .filter_map(|(name, pattern)| Regex::new(pattern).ok().map(|re| (name.to_string(), re)))
        .collect()
});

/// A redaction rule that scans log records for sensitive patterns.
pub trait RedactionRule: Send + Sync {
    /// Unique name for this rule (used in error reporting).
    fn name(&self) -> &'static str;

    /// Apply redaction to a single FieldValue in place.
    /// Return true if the value was redacted.
    fn apply(&self, key: &str, value: &mut FieldValue) -> bool;
}

/// Redacts any field whose key contains a sensitive substring (case-insensitive).
pub struct KeyNameRule;

impl KeyNameRule {
    /// Substrings that trigger redaction when found in a field key.
    const SENSITIVE_KEY_SUBSTRINGS: &'static [&'static str] = &[
        "api_key",
        "secret",
        "password",
        "token",
        "credential",
        "private_key",
        "access_key",
        "auth",
    ];
}

impl RedactionRule for KeyNameRule {
    fn name(&self) -> &'static str {
        "key_name_rule"
    }

    fn apply(&self, key: &str, value: &mut FieldValue) -> bool {
        let key_lower = key.to_lowercase();
        if Self::SENSITIVE_KEY_SUBSTRINGS
            .iter()
            .any(|&sub| key_lower.contains(sub))
        {
            *value = FieldValue::Redacted;
            true
        } else {
            false
        }
    }
}

/// Regex-based pattern matching for String FieldValues and messages.
pub struct PatternRule {
    name: &'static str,
    pattern: Regex,
}

impl PatternRule {
    /// Create a new PatternRule from a compiled regex.
    pub fn new(name: &'static str, pattern: Regex) -> Self {
        Self { name, pattern }
    }

    /// Create a PatternRule from a pattern string.
    pub fn compile(name: &'static str, pattern: &str) -> Result<Self, LogError> {
        Regex::new(pattern)
            .map(|re| Self { name, pattern: re })
            .map_err(|e| LogError::RedactionRuleInvalid {
                rule: name.to_string(),
                reason: e.to_string(),
            })
    }
}

impl RedactionRule for PatternRule {
    fn name(&self) -> &'static str {
        self.name
    }

    fn apply(&self, _key: &str, value: &mut FieldValue) -> bool {
        if let FieldValue::String(ref s) = value {
            if self.pattern.is_match(s) {
                *value = FieldValue::Redacted;
                return true;
            }
        }
        false
    }
}

/// Redact message strings by applying pattern rules to find and replace sensitive content.
pub fn redact_message(message: &str, rules: &[Box<dyn RedactionRule>]) -> String {
    let mut result = message.to_string();
    for (name, re) in BUILTIN_REGEXES.iter() {
        result = re
            .replace_all(&result, format!("[REDACTED:{name}]"))
            .to_string();
    }
    // Also try PatternRule instances that match strings
    for rule in rules {
        if let FieldValue::String(s) = &mut FieldValue::String(result.clone()) {
            if rule.apply("__message__", s) {
                if let FieldValue::String(redacted) = s {
                    result = redacted.clone();
                }
            }
        }
    }
    result
}

/// The redaction engine holds an ordered list of rules applied to every record.
pub struct RedactionEngine {
    rules: Vec<Box<dyn RedactionRule>>,
}

impl RedactionEngine {
    /// Create a new redaction engine with default built-in rules.
    pub fn new() -> Self {
        let mut engine = Self { rules: Vec::new() };
        engine.register(Box::new(KeyNameRule)).ok();
        // Register built-in pattern rules
        for (name, pattern) in BUILTIN_PATTERNS {
            if let Ok(rule) = PatternRule::compile(name, pattern) {
                engine.register(Box::new(rule)).ok();
            }
        }
        engine
    }

    /// Register a custom redaction rule.
    pub fn register(&mut self, rule: Box<dyn RedactionRule>) -> Result<(), LogError> {
        if self.rules.iter().any(|r| r.name() == rule.name()) {
            return Err(LogError::RedactionRuleInvalid {
                rule: rule.name().to_string(),
                reason: "A rule with this name is already registered".into(),
            });
        }
        self.rules.push(rule);
        Ok(())
    }

    /// Apply all rules to a record, mutating fields in place.
    /// The message string is also scanned for pattern matches.
    /// This method must never panic or return an error.
    pub fn redact(&self, record: &mut LogRecord) {
        // Redact each field
        for (key, value) in record.fields.iter_mut() {
            self.apply_rules_to_field(key, value);
        }

        // Redact the message string
        record.message = redact_message(&record.message, &self.rules);
    }

    /// Apply all rules recursively to a field value and its children.
    fn apply_rules_to_field(&self, key: &str, value: &mut FieldValue) {
        for rule in &self.rules {
            rule.apply(key, value);
        }

        // Recurse into nested structures
        match value {
            FieldValue::Object(map) => {
                for (k, v) in map.iter_mut() {
                    self.apply_rules_to_field(k, v);
                }
            }
            FieldValue::Array(arr) => {
                for v in arr.iter_mut() {
                    self.apply_rules_to_field(key, v);
                }
            }
            _ => {}
        }
    }
}

impl Default for RedactionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::LogLevel;
    use indexmap::IndexMap;

    #[test]
    fn test_key_name_rule_redacts_api_key() {
        let rule = KeyNameRule;
        let mut value = FieldValue::String("my-api-key-value".into());
        assert!(rule.apply("api_key", &mut value));
        assert!(matches!(value, FieldValue::Redacted));
    }

    #[test]
    fn test_key_name_rule_passes_normal_fields() {
        let rule = KeyNameRule;
        let mut value = FieldValue::String("hello".into());
        assert!(!rule.apply("username", &mut value));
        assert!(matches!(value, FieldValue::String(_)));
    }

    #[test]
    fn test_redaction_engine_redacts_record() {
        let engine = RedactionEngine::new();
        let mut record = LogRecord::new(
            LogLevel::Info,
            "test".into(),
            "User email: user@example.com".into(),
        );
        record.fields.insert(
            "api_key".into(),
            FieldValue::String("sk-ant-abc123def456ghi789".into()),
        );

        engine.redact(&mut record);

        // Field should be redacted
        match &record.fields["api_key"] {
            FieldValue::Redacted => {} // OK
            _ => panic!("api_key field should be redacted"),
        }

        // Message should have email redacted
        assert!(record.message.contains("[REDACTED:email_address]"));
    }

    #[test]
    fn test_builtin_patterns_match() {
        for (name, pattern) in BUILTIN_PATTERNS {
            let re = Regex::new(pattern).unwrap();
            match *name {
                "anthropic_api_key" => {
                    assert!(re.is_match("sk-ant-abc123def456ghi789jkl123"), "{name}")
                }
                "email_address" => assert!(re.is_match("user@example.com"), "{name}"),
                "ssn" => assert!(re.is_match("123-45-6789"), "{name}"),
                _ => {}
            }
        }
    }
}
