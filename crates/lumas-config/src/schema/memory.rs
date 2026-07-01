use serde::{Deserialize, Serialize};

/// Memory system configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MemoryConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_retention_days")]
    pub retention_days_default: u32,
    #[serde(default = "default_true")]
    pub auto_extract: bool,
    #[serde(default = "default_true")]
    pub require_confirmation_for_observations: bool,
    #[serde(default = "default_max_memories")]
    pub max_active_memories: u32,
}

fn default_true() -> bool {
    true
}
fn default_retention_days() -> u32 {
    365
}
fn default_max_memories() -> u32 {
    100
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days_default: 365,
            auto_extract: true,
            require_confirmation_for_observations: true,
            max_active_memories: 100,
        }
    }
}
