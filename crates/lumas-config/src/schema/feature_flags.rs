use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Runtime feature flags configuration.
///
/// Feature flags control optional and experimental functionality.
/// Flags can be enabled/disabled at compile time via Cargo features,
/// or at runtime via config file or environment variables.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FeatureFlags {
    /// Set of enabled feature flag names.
    #[serde(default)]
    pub enabled: Vec<String>,

    /// Set of disabled feature flag names.
    #[serde(default)]
    pub disabled: Vec<String>,

    /// Additional metadata for feature flags (name → description).
    #[serde(default, skip_serializing)]
    pub metadata: HashMap<String, String>,
}

impl FeatureFlags {
    /// Whether a specific feature flag is enabled.
    pub fn is_enabled(&self, flag: &str) -> bool {
        if self.disabled.contains(&flag.to_string()) {
            return false;
        }
        self.enabled.contains(&flag.to_string())
    }

    /// Get the set of all known feature flags.
    pub fn all_enabled(&self) -> HashSet<&str> {
        self.enabled.iter().map(|s| s.as_str()).collect()
    }

    /// Check if the feature flags are internally consistent.
    pub fn check_consistency(&self) -> Result<(), String> {
        for flag in &self.enabled {
            if self.disabled.contains(flag) {
                return Err(format!(
                    "Feature flag '{flag}' is both enabled and disabled"
                ));
            }
        }
        Ok(())
    }
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            enabled: vec![
                "voice".into(),
                "memory".into(),
                "plugins".into(),
                "desktop_awareness".into(),
            ],
            disabled: vec![],
            metadata: HashMap::new(),
        }
    }
}
