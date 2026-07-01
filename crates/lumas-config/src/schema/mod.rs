//! # Lumas Config Schema
//!
//! All subsystem configuration structs for the Lumas platform, matching SRS Chapter 27.
//! Each struct implements Debug, Clone, Serialize, Deserialize, and Default.
//!
//! Every struct uses `#[serde(deny_unknown_fields)]` to catch typos in user config
//! and `#[serde(default)]` on all fields so partial files merge with defaults.

mod accessibility;
mod ai;
mod animation;
mod character;
mod diagnostics;
mod feature_flags;
mod general;
mod ipc;
mod logging;
mod memory;
mod performance;
mod physics;
mod plugin;
mod privacy;
mod rendering;
mod runtime;
mod security;
mod storage;
mod update;
mod voice;
mod workspace;

pub use accessibility::AccessibilityConfig;
pub use ai::{AIConfig, CloudProvider, InferenceMode};
pub use animation::AnimationConfig;
pub use character::CharacterConfig;
pub use diagnostics::DiagnosticsConfig;
pub use feature_flags::FeatureFlags;
pub use general::GeneralConfig;
pub use ipc::IPCConfig;
pub use logging::LoggingConfig;
pub use memory::MemoryConfig;
pub use performance::PerformanceConfig;
pub use physics::PhysicsConfig;
pub use plugin::PluginConfig;
pub use privacy::PrivacyConfig;
pub use rendering::RenderingConfig;
pub use runtime::RuntimeConfig;
pub use security::SecurityConfig;
pub use storage::StorageConfig;
pub use update::UpdateConfig;
pub use voice::{STTModel, VoiceConfig};
pub use workspace::WorkspaceConfig;

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Root configuration object for the entire Lumas platform.
///
/// Constructed by `ConfigManager` during bootstrap. After construction,
/// it is stored in an `ArcSwap` and never mutated in place — hot reload
/// replaces the entire `Arc<LumiConfig>` atomically.
///
/// All fields are public to subsystems but only `ConfigManager` may produce
/// a `LumiConfig` instance (enforced by `pub(crate)` constructors).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LumiConfig {
    /// Schema version for migration support. Populated by ConfigManager,
    /// not by the user. Do not serialize back to file after migration.
    #[serde(default = "LumiConfig::current_schema_version")]
    pub schema_version: u32,

    #[serde(default)]
    pub general: GeneralConfig,

    #[serde(default)]
    pub runtime: RuntimeConfig,

    #[serde(default)]
    pub character: CharacterConfig,

    #[serde(default)]
    pub ai: AIConfig,

    #[serde(default)]
    pub voice: VoiceConfig,

    #[serde(default)]
    pub memory: MemoryConfig,

    #[serde(default)]
    pub rendering: RenderingConfig,

    #[serde(default)]
    pub physics: PhysicsConfig,

    #[serde(default)]
    pub animation: AnimationConfig,

    #[serde(default)]
    pub workspace: WorkspaceConfig,

    #[serde(default)]
    pub plugin: PluginConfig,

    #[serde(default)]
    pub ipc: IPCConfig,

    #[serde(default)]
    pub storage: StorageConfig,

    #[serde(default)]
    pub security: SecurityConfig,

    #[serde(default)]
    pub privacy: PrivacyConfig,

    #[serde(default)]
    pub performance: PerformanceConfig,

    #[serde(default)]
    pub accessibility: AccessibilityConfig,

    #[serde(default)]
    pub logging: LoggingConfig,

    #[serde(default)]
    pub update: UpdateConfig,

    #[serde(default)]
    pub diagnostics: DiagnosticsConfig,

    #[serde(default)]
    pub feature_flags: FeatureFlags,
}

impl LumiConfig {
    /// Current schema version for migration support.
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    /// Returns the current schema version (used for serde default).
    pub fn current_schema_version() -> u32 {
        Self::CURRENT_SCHEMA_VERSION
    }

    /// Load configuration from a TOML string, merging with defaults.
    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }

    /// Serialize configuration to TOML string.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Get the platform-appropriate config directory path.
    pub fn config_dir() -> std::path::PathBuf {
        crate::platform::config_dir().unwrap_or_else(|_| std::path::PathBuf::from("/tmp/lumi"))
    }

    /// Get the full path to the config file.
    pub fn config_path() -> std::path::PathBuf {
        crate::platform::config_file_path()
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/lumi/config.toml"))
    }

    /// Wrap in Arc for cache storage.
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

impl Default for LumiConfig {
    fn default() -> Self {
        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            general: GeneralConfig::default(),
            runtime: RuntimeConfig::default(),
            character: CharacterConfig::default(),
            ai: AIConfig::default(),
            voice: VoiceConfig::default(),
            memory: MemoryConfig::default(),
            rendering: RenderingConfig::default(),
            physics: PhysicsConfig::default(),
            animation: AnimationConfig::default(),
            workspace: WorkspaceConfig::default(),
            plugin: PluginConfig::default(),
            ipc: IPCConfig::default(),
            storage: StorageConfig::default(),
            security: SecurityConfig::default(),
            privacy: PrivacyConfig::default(),
            performance: PerformanceConfig::default(),
            accessibility: AccessibilityConfig::default(),
            logging: LoggingConfig::default(),
            update: UpdateConfig::default(),
            diagnostics: DiagnosticsConfig::default(),
            feature_flags: FeatureFlags::default(),
        }
    }
}
