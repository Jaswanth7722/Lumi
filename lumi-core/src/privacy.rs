//! # Privacy Model — Data Minimization and User Control (Chapter 24)

use lumi_common::privacy::{
    CacheInventory, DataInventory, MemoryInventory, PIIAction, PIIDetector, PluginDataEntry,
    PrivacyTier, ScreenDataStatus, VoiceDataStatus,
};
use std::collections::HashMap;
use tracing::debug;

/// Manages privacy policies and PII detection for user data.
pub struct PrivacyManager {
    pii_detector: PIIDetector,
    privacy_tier: PrivacyTier,
    feature_toggles: HashMap<String, bool>,
    audit_events: Vec<PrivacyAuditEvent>,
}

struct PrivacyAuditEvent {
    timestamp: i64,
    description: String,
}

impl PrivacyManager {
    pub fn new() -> Self {
        Self {
            pii_detector: PIIDetector::new(),
            privacy_tier: PrivacyTier::Medium,
            feature_toggles: Self::default_toggles(),
            audit_events: Vec::new(),
        }
    }

    fn default_toggles() -> HashMap<String, bool> {
        let mut toggles = HashMap::new();
        toggles.insert("clipboard_access".into(), false);
        toggles.insert("screen_capture".into(), false);
        toggles.insert("notification_reading".into(), true);
        toggles.insert("active_window_tracking".into(), true);
        toggles.insert("telemetry".into(), false);
        toggles
    }

    pub fn screen_content(&self, content: &str) -> Option<PIIAction> {
        self.pii_detector.scan(content)
    }

    pub fn is_feature_enabled(&self, feature: &str) -> bool {
        *self.feature_toggles.get(feature).unwrap_or(&false)
    }

    pub fn set_feature_enabled(&mut self, feature: &str, enabled: bool) {
        self.feature_toggles.insert(feature.to_string(), enabled);
        self.audit_events.push(PrivacyAuditEvent {
            timestamp: chrono::Utc::now().timestamp_millis(),
            description: format!(
                "Feature '{}' {}",
                feature,
                if enabled { "enabled" } else { "disabled" }
            ),
        });
    }

    pub fn set_privacy_tier(&mut self, tier: PrivacyTier) {
        self.privacy_tier = tier;
    }

    pub fn privacy_tier(&self) -> &PrivacyTier {
        &self.privacy_tier
    }

    pub fn data_inventory(&self) -> DataInventory {
        DataInventory {
            memories: MemoryInventory {
                count: 0,
                types: HashMap::new(),
                oldest: 0,
                newest: 0,
                size_bytes: 0,
            },
            conversation_cache: CacheInventory {
                message_count: 0,
                session_count: 0,
                size_bytes: 0,
            },
            voice_data: VoiceDataStatus::new(),
            screen_data: ScreenDataStatus::new(),
            plugin_data: HashMap::new(),
        }
    }

    pub fn feature_toggles(&self) -> &HashMap<String, bool> {
        &self.feature_toggles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_toggles() {
        let manager = PrivacyManager::new();
        assert!(!manager.is_feature_enabled("clipboard_access"));
        assert!(manager.is_feature_enabled("active_window_tracking"));
    }

    #[test]
    fn test_toggle_feature() {
        let mut manager = PrivacyManager::new();
        manager.set_feature_enabled("clipboard_access", true);
        assert!(manager.is_feature_enabled("clipboard_access"));
    }

    #[test]
    fn test_default_tier() {
        let manager = PrivacyManager::new();
        assert_eq!(*manager.privacy_tier(), PrivacyTier::Medium);
    }

    #[test]
    fn test_pii_detection() {
        let manager = PrivacyManager::new();
        let result = manager.screen_content("Contact me at user@example.com");
        assert!(result.is_some());
    }
}
