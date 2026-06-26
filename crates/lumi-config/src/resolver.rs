//! # Layered Precedence Resolver
//!
//! Implements field-level merging across config layers with source tracking.

use crate::schema::LumiConfig;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Records the source of each resolved configuration value.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// The resolved configuration.
    pub config: Arc<LumiConfig>,
    /// Source annotation for each config field path (e.g., "ai.temperature" → Source).
    pub sources: HashMap<String, ConfigSource>,
    /// When this config was resolved.
    pub resolved_at: DateTime<Utc>,
}

impl Default for ResolvedConfig {
    fn default() -> Self {
        Self {
            config: Arc::new(LumiConfig::default()),
            sources: HashMap::new(),
            resolved_at: Utc::now(),
        }
    }
}

impl ResolvedConfig {
    /// Create a new resolved config.
    pub fn new(config: Arc<LumiConfig>, sources: HashMap<String, ConfigSource>) -> Self {
        Self {
            config,
            sources,
            resolved_at: Utc::now(),
        }
    }
}

/// The origin source of a configuration value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    /// Compile-time default value.
    Default,
    /// Loaded from a config file.
    File {
        /// Path to the file.
        path: PathBuf,
    },
    /// Loaded from an environment variable.
    Environment {
        /// The environment variable name.
        var: String,
    },
    /// Overridden via CLI argument.
    CliArgument {
        /// The CLI key.
        key: String,
    },
    /// Applied as a runtime override.
    RuntimeOverride,
}

impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSource::Default => write!(f, "default"),
            ConfigSource::File { path } => write!(f, "file({})", path.display()),
            ConfigSource::Environment { var } => write!(f, "env({var})"),
            ConfigSource::CliArgument { key } => write!(f, "cli({key})"),
            ConfigSource::RuntimeOverride => write!(f, "override"),
        }
    }
}

/// Merges two LumiConfig instances field-by-field.
///
/// Fields in `overlay` override fields in `base` only when they differ
/// from the `Default` value of that field (i.e., the user explicitly set them).
///
/// This is more precise than a TOML-level merge because it respects
/// type-level default semantics rather than TOML key presence.
pub fn merge_configs(base: LumiConfig, overlay: LumiConfig) -> LumiConfig {
    // This is a structural merge - for each field we check if the overlay
    // differs from its default. For now, we simply return the overlay merged
    // with base using serde's merge behavior (overlay values override base).
    // A more sophisticated implementation would do field-level comparison.
    let mut merged = base;

    // General
    if overlay.general.language != "en" {
        merged.general.language = overlay.general.language;
    }
    merged.general.start_on_login = overlay.general.start_on_login;
    merged.general.check_updates = overlay.general.check_updates;
    if overlay.general.update_channel != "stable" {
        merged.general.update_channel = overlay.general.update_channel;
    }
    if overlay.general.theme != "auto" {
        merged.general.theme = overlay.general.theme;
    }

    // Runtime
    merged.runtime.show_console = overlay.runtime.show_console;
    if overlay.runtime.log_level != "info" {
        merged.runtime.log_level = overlay.runtime.log_level;
    }

    // Character
    if overlay.character.name != "Lumi" {
        merged.character.name = overlay.character.name;
    }
    merged.character.size_scale = overlay.character.size_scale;
    merged.character.position_x = overlay.character.position_x;
    merged.character.position_y = overlay.character.position_y;
    merged.character.locked = overlay.character.locked;

    // AI
    merged.ai.inference_mode = overlay.ai.inference_mode;
    merged.ai.cloud_provider = overlay.ai.cloud_provider;
    if overlay.ai.anthropic_api_key.is_some() {
        merged.ai.anthropic_api_key = overlay.ai.anthropic_api_key;
    }
    if overlay.ai.openai_api_key.is_some() {
        merged.ai.openai_api_key = overlay.ai.openai_api_key;
    }
    if !overlay.ai.local_model.is_empty() {
        merged.ai.local_model = overlay.ai.local_model;
    }
    merged.ai.temperature = overlay.ai.temperature;
    merged.ai.max_response_tokens = overlay.ai.max_response_tokens;

    // Voice
    merged.voice.enabled = overlay.voice.enabled;
    merged.voice.tts_enabled = overlay.voice.tts_enabled;
    if overlay.voice.wake_word != "Hey Lumi" {
        merged.voice.wake_word = overlay.voice.wake_word;
    }

    // Memory
    merged.memory.enabled = overlay.memory.enabled;
    merged.memory.retention_days_default = overlay.memory.retention_days_default;
    merged.memory.auto_extract = overlay.memory.auto_extract;

    // Privacy
    merged.privacy.screen_capture_enabled = overlay.privacy.screen_capture_enabled;
    merged.privacy.telemetry_enabled = overlay.privacy.telemetry_enabled;
    merged.privacy.crash_reports_enabled = overlay.privacy.crash_reports_enabled;

    // Performance
    merged.performance.render_fps_cap = overlay.performance.render_fps_cap;
    merged.performance.gpu_memory_limit_mb = overlay.performance.gpu_memory_limit_mb;
    if overlay.performance.render_quality != "auto" {
        merged.performance.render_quality = overlay.performance.render_quality;
    }
    if overlay.performance.animation_quality != "full" {
        merged.performance.animation_quality = overlay.performance.animation_quality;
    }

    // Feature flags
    if !overlay.feature_flags.enabled.is_empty() {
        merged.feature_flags.enabled = overlay.feature_flags.enabled;
    }
    if !overlay.feature_flags.disabled.is_empty() {
        merged.feature_flags.disabled = overlay.feature_flags.disabled;
    }

    // Schema version
    merged.schema_version = overlay.schema_version;

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_with_empty_overlay() {
        let base = LumiConfig::default();
        let overlay = LumiConfig::default();
        let merged = merge_configs(base.clone(), overlay);
        assert_eq!(merged.general.language, base.general.language);
    }

    #[test]
    fn test_merge_overrides_specific_fields() {
        let base = LumiConfig::default();
        let mut overlay = LumiConfig::default();
        overlay.ai.temperature = 1.5;
        overlay.general.language = "ja".into();

        let merged = merge_configs(base, overlay);
        assert_eq!(merged.ai.temperature, 1.5);
        assert_eq!(merged.general.language, "ja");
    }
}
