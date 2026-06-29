//! # Privacy Model — Data Minimization and User Control (Chapter 24)
//!
//! Defines privacy tiers, PII detection, data inventory structures,
//! and privacy-aware memory filtering.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Privacy Tiers
// ---------------------------------------------------------------------------

/// Privacy sensitivity tiers for data observed by Lumi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyTier {
    /// Process names, window titles — always observed.
    System = 0,
    /// Application metadata, active app — observed, ephemeral.
    Low = 1,
    /// Clipboard content, notification text — off by default.
    Medium = 2,
    /// Screen content, file contents — explicit request only.
    High = 3,
    /// Passwords, credit cards — never stored.
    Sensitive = 4,
}

impl PrivacyTier {
    pub fn description(&self) -> &'static str {
        match self {
            PrivacyTier::System => "always observed, cannot disable",
            PrivacyTier::Low => "Observed but ephemeral, can disable",
            PrivacyTier::Medium => "Off by default, opt-in per feature",
            PrivacyTier::High => "Explicit request only, always requires approval",
            PrivacyTier::Sensitive => "never stored, not configurable",
        }
    }
}

/// Configuration for a specific privacy tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyTierConfig {
    pub tier: PrivacyTier,
    pub enabled: bool,
    pub requires_approval: bool,
    pub can_user_disable: bool,
}

/// Returns default privacy tier configurations.
pub fn default_privacy_config() -> Vec<PrivacyTierConfig> {
    vec![
        PrivacyTierConfig {
            tier: PrivacyTier::System,
            enabled: true,
            requires_approval: false,
            can_user_disable: false,
        },
        PrivacyTierConfig {
            tier: PrivacyTier::Low,
            enabled: true,
            requires_approval: false,
            can_user_disable: true,
        },
        PrivacyTierConfig {
            tier: PrivacyTier::Medium,
            enabled: false,
            requires_approval: true,
            can_user_disable: true,
        },
        PrivacyTierConfig {
            tier: PrivacyTier::High,
            enabled: false,
            requires_approval: true,
            can_user_disable: true,
        },
        PrivacyTierConfig {
            tier: PrivacyTier::Sensitive,
            enabled: false,
            requires_approval: true,
            can_user_disable: false,
        },
    ]
}

// ---------------------------------------------------------------------------
// PII Detection
// ---------------------------------------------------------------------------

/// Categories of personally identifiable information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PIICategory {
    CreditCard,
    SSN,
    Password,
    Email,
    PhoneNumber,
    APIKey,
    PrivateKey,
    AccessToken,
}

/// Action to take when PII is detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PIIAction {
    /// Never store this memory.
    Block,
    /// Store with PII replaced by placeholder.
    Redact { placeholder: String },
    /// Store but flag to user.
    Warn { message: String },
}

/// A PII detection pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PIIPattern {
    pub category: PIICategory,
    pub description: String,
    pub action: PIIAction,
}

/// PII detector that scans content before memory storage.
#[derive(Debug)]
pub struct PIIDetector {
    pub patterns: Vec<PIIPattern>,
}

impl PIIDetector {
    pub fn new() -> Self {
        Self {
            patterns: default_pii_patterns(),
        }
    }

    /// Scan content for PII and return the first detected action.
    pub fn scan(&self, content: &str) -> Option<PIIAction> {
        for pattern in &self.patterns {
            // Pattern matching with keywords (in production, use regex)
            let lower = content.to_lowercase();
            let matched = match pattern.category {
                PIICategory::Email => lower.contains('@') && lower.contains('.'),
                PIICategory::CreditCard => {
                    content.chars().filter(|c| c.is_ascii_digit()).count() >= 13
                }
                PIICategory::APIKey => {
                    let alphanumeric = content.chars().filter(|c| c.is_alphanumeric()).count();
                    content.len() >= 20
                        && content.contains(|c: char| !c.is_alphanumeric() && !c.is_whitespace())
                        && alphanumeric > 0
                }
                _ => false,
            };
            if matched {
                return Some(pattern.action.clone());
            }
        }
        None
    }
}

impl Default for PIIDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the default PII detection patterns.
pub fn default_pii_patterns() -> Vec<PIIPattern> {
    vec![
        PIIPattern {
            category: PIICategory::CreditCard,
            description: "Credit card number".into(),
            action: PIIAction::Block,
        },
        PIIPattern {
            category: PIICategory::Email,
            description: "Email address".into(),
            action: PIIAction::Redact {
                placeholder: "[email]".into(),
            },
        },
        PIIPattern {
            category: PIICategory::APIKey,
            description: "API key or token".into(),
            action: PIIAction::Warn {
                message: "Content may contain an API key".into(),
            },
        },
        PIIPattern {
            category: PIICategory::Password,
            description: "Password or secret".into(),
            action: PIIAction::Block,
        },
    ]
}

// ---------------------------------------------------------------------------
// Data Inventory
// ---------------------------------------------------------------------------

/// Plugin data contribution to the data inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDataEntry {
    pub keys_stored: Vec<String>,
    pub size_bytes: u64,
}

/// A complete snapshot of all data Lumi has stored about the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataInventory {
    pub memories: MemoryInventory,
    pub conversation_cache: CacheInventory,
    pub voice_data: VoiceDataStatus,
    pub screen_data: ScreenDataStatus,
    pub plugin_data: HashMap<String, PluginDataEntry>,
}

/// Memory storage inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInventory {
    pub count: u64,
    pub types: HashMap<String, u64>,
    pub oldest: i64,
    pub newest: i64,
    pub size_bytes: u64,
}

/// Conversation cache inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheInventory {
    pub message_count: u64,
    pub session_count: u64,
    pub size_bytes: u64,
}

/// Voice data status — always false by architecture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceDataStatus {
    pub stored: bool,
}

/// Screen data status — always false by architecture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenDataStatus {
    pub stored: bool,
}

impl VoiceDataStatus {
    pub fn new() -> Self {
        Self { stored: false }
    }
}

impl Default for VoiceDataStatus {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenDataStatus {
    pub fn new() -> Self {
        Self { stored: false }
    }
}

impl Default for ScreenDataStatus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_privacy_tier_descriptions() {
        assert!(PrivacyTier::System.description().contains("always"));
        assert!(PrivacyTier::Sensitive.description().contains("never"));
    }

    #[test]
    fn test_default_privacy_config() {
        let config = default_privacy_config();
        assert_eq!(config.len(), 5);
        assert!(config[0].tier == PrivacyTier::System && config[0].enabled);
        assert!(config[2].tier == PrivacyTier::Medium && !config[2].enabled);
    }

    #[test]
    fn test_pii_detector_email() {
        let detector = PIIDetector::new();
        let result = detector.scan("Contact me at user@example.com");
        assert!(result.is_some());
        match result.unwrap() {
            PIIAction::Redact { .. } => {}
            other => panic!("Expected Redact, got {:?}", other),
        }
    }

    #[test]
    fn test_pii_detector_credit_card() {
        let detector = PIIDetector::new();
        let result = detector.scan("My card is 4111 1111 1111 1111");
        assert!(result.is_some());
        match result.unwrap() {
            PIIAction::Block => {}
            other => panic!("Expected Block, got {:?}", other),
        }
    }

    #[test]
    fn test_pii_detector_clean_text() {
        let detector = PIIDetector::new();
        let result = detector.scan("I prefer dark mode in editors");
        assert!(result.is_none());
    }

    #[test]
    fn test_data_inventory_defaults() {
        let voice = VoiceDataStatus::new();
        assert!(!voice.stored);
        let screen = ScreenDataStatus::new();
        assert!(!screen.stored);
    }
}
