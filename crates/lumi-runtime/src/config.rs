//! # Configuration System
//!
//! Configuration loading with defaults, file loading, environment variable
//! overrides, validation, and hot reload via file watching.
//!
//! # Stages
//!
//! 1. **Defaults** — compile-time defaults via `Default` trait
//! 2. **File Loading** — load from platform-appropriate path
//! 3. **Environment Overrides** — `LUMI_{SECTION}_{KEY}` env vars
//! 4. **Validation** — range checks, path existence, cross-field constraints
//! 5. **Hot Reload** — file watcher with debounced reload
//!
//! # Thread Safety
//!
//! `ConfigLoader` is `Send + Sync`. Configuration is delivered to subscribers
//! via `Arc<LumiConfig>` through `ArcSwap`, allowing lock-free reads.

use crate::error::ConfigError;
use crate::event::{ConfigLoaded, ConfigReloadFailed, ConfigReloaded, EventBus};
use arc_swap::ArcSwap;
use lumi_config::LumiConfig;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// A single configuration validation error.
#[derive(Debug, Clone)]
pub struct ConfigValidationError {
    /// The field path that failed validation.
    pub field: String,
    /// A human-readable error message.
    pub message: String,
    /// The severity of the error.
    pub severity: ValidationSeverity,
}

/// Severity of a validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    /// Hard error — configuration cannot be used.
    Error,
    /// Warning — configuration is usable but suboptimal.
    Warning,
}

/// Loads, validates, and watches configuration files.
pub struct ConfigLoader {
    /// The loaded and validated configuration.
    current: Arc<ArcSwap<LumiConfig>>,
    /// The path to the configuration file, if any.
    config_path: Option<PathBuf>,
    /// Whether hot reloading is active.
    hot_reload_active: std::sync::atomic::AtomicBool,
}

impl ConfigLoader {
    /// Create a new configuration loader with default values.
    pub fn new() -> Self {
        Self {
            current: Arc::new(ArcSwap::new(Arc::new(LumiConfig::default()))),
            config_path: None,
            hot_reload_active: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Get the current configuration (lock-free).
    pub fn current(&self) -> Arc<LumiConfig> {
        self.current.load_full()
    }

    /// Determine the platform-appropriate config file path.
    fn get_config_path() -> PathBuf {
        LumiConfig::config_path()
    }

    /// Load configuration from defaults, file, and environment.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if the file cannot be parsed or validated.
    pub fn load(&mut self) -> Result<Arc<LumiConfig>, Vec<ConfigError>> {
        let mut errors = Vec::new();

        // Stage 1: Start with defaults
        let mut config = LumiConfig::default();

        // Stage 2: Load from file if present
        let config_path = Self::get_config_path();
        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => match LumiConfig::from_toml(&content) {
                    Ok(file_config) => {
                        // Merge: file values override defaults
                        Self::merge_configs(&mut config, file_config);
                        self.config_path = Some(config_path.clone());
                        info!("Loaded configuration from: {:?}", config_path);
                    }
                    Err(e) => {
                        errors.push(ConfigError::ParseError {
                            path: config_path.clone(),
                            line: 0,
                            column: 0,
                            message: e.to_string(),
                        });
                    }
                },
                Err(e) => {
                    debug!("No config file at {:?}: {}", config_path, e);
                }
            }
        } else {
            debug!("No config file at {:?}, using defaults", config_path);
        }

        // Stage 3: Environment variable overrides
        if let Err(env_errors) = self.apply_env_overrides(&mut config) {
            errors.extend(env_errors);
        }

        // Stage 4: Validation
        let validation_errors = self.validate(&config);
        errors.extend(
            validation_errors
                .into_iter()
                .filter(|e| matches!(e.severity, ValidationSeverity::Error))
                .map(|e| ConfigError::ConstraintViolation {
                    message: e.message,
                    fields: vec![e.field.clone()],
                }),
        );

        if !errors.is_empty() {
            return Err(errors);
        }

        let config = Arc::new(config);
        self.current.store(config.clone());
        Ok(config)
    }

    /// Load configuration from a specific file path.
    pub fn load_from(&mut self, path: &Path) -> Result<Arc<LumiConfig>, Vec<ConfigError>> {
        self.config_path = Some(path.to_path_buf());
        self.load()
    }

    /// Apply environment variable overrides.
    ///
    /// Convention: `LUMI_{SECTION}_{KEY}` (e.g., `LUMI_AI_INFERENCE_MODE`).
    fn apply_env_overrides(&self, config: &mut LumiConfig) -> Result<(), Vec<ConfigError>> {
        let mut errors = Vec::new();

        for (key, value) in std::env::vars() {
            if !key.starts_with("LUMI_") {
                continue;
            }

            let config_key = &key[5..]; // Strip "LUMI_"

            match config_key {
                // General
                "GENERAL_LANGUAGE" => config.general.language = value,
                "GENERAL_START_ON_LOGIN" => {
                    config.general.start_on_login = Self::parse_bool(&key, &value, &mut errors)
                }
                "GENERAL_CHECK_UPDATES" => {
                    config.general.check_updates = Self::parse_bool(&key, &value, &mut errors)
                }
                "GENERAL_UPDATE_CHANNEL" => config.general.update_channel = value,

                // AI
                "AI_INFERENCE_MODE" => config.ai.inference_mode = value,
                "AI_CLOUD_PROVIDER" => config.ai.cloud_provider = value,
                "AI_LOCAL_MODEL" => config.ai.local_model = value,
                "AI_TEMPERATURE" => {
                    config.ai.temperature = Self::parse_f64(&key, &value, &mut errors)
                }
                "AI_MAX_RESPONSE_TOKENS" => {
                    config.ai.max_response_tokens = Self::parse_u32(&key, &value, &mut errors)
                }

                // Privacy
                "PRIVACY_SCREEN_CAPTURE_ENABLED" => {
                    config.privacy.screen_capture_enabled =
                        Self::parse_bool(&key, &value, &mut errors)
                }
                "PRIVACY_TELEMETRY_ENABLED" => {
                    config.privacy.telemetry_enabled = Self::parse_bool(&key, &value, &mut errors)
                }

                // Performance
                "PERFORMANCE_RENDER_FPS_CAP" => {
                    config.performance.render_fps_cap = Self::parse_u32(&key, &value, &mut errors)
                }
                "PERFORMANCE_GPU_MEMORY_LIMIT_MB" => {
                    config.performance.gpu_memory_limit_mb =
                        Self::parse_u32(&key, &value, &mut errors)
                }

                _ => {
                    debug!("Unknown env var override: {key} (key: {config_key})");
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Parse a boolean from an environment variable.
    fn parse_bool(key: &str, value: &str, errors: &mut Vec<ConfigError>) -> bool {
        match value.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => {
                errors.push(ConfigError::EnvVarError {
                    var: key.to_string(),
                    value: value.to_string(),
                    message: format!("Cannot parse '{value}' as boolean"),
                });
                false
            }
        }
    }

    /// Parse a f64 from an environment variable.
    fn parse_f64(key: &str, value: &str, errors: &mut Vec<ConfigError>) -> f64 {
        value.parse::<f64>().unwrap_or_else(|e| {
            errors.push(ConfigError::EnvVarError {
                var: key.to_string(),
                value: value.to_string(),
                message: format!("Cannot parse as f64: {e}"),
            });
            0.0
        })
    }

    /// Parse a u32 from an environment variable.
    fn parse_u32(key: &str, value: &str, errors: &mut Vec<ConfigError>) -> u32 {
        value.parse::<u32>().unwrap_or_else(|e| {
            errors.push(ConfigError::EnvVarError {
                var: key.to_string(),
                value: value.to_string(),
                message: format!("Cannot parse as u32: {e}"),
            });
            0
        })
    }

    /// Validate configuration constraints.
    fn validate(&self, config: &LumiConfig) -> Vec<ConfigValidationError> {
        let mut errors = Vec::new();

        // Range checks
        if config.ai.temperature < 0.0 || config.ai.temperature > 2.0 {
            errors.push(ConfigValidationError {
                field: "ai.temperature".into(),
                message: format!(
                    "Temperature must be between 0.0 and 2.0, got {}",
                    config.ai.temperature
                ),
                severity: ValidationSeverity::Error,
            });
        }

        if config.character.size_scale < 0.5 || config.character.size_scale > 2.0 {
            errors.push(ConfigValidationError {
                field: "character.size_scale".into(),
                message: format!(
                    "Size scale must be between 0.5 and 2.0, got {}",
                    config.character.size_scale
                ),
                severity: ValidationSeverity::Error,
            });
        }

        if !["stable", "beta", "nightly"].contains(&config.general.update_channel.as_str()) {
            errors.push(ConfigValidationError {
                field: "general.update_channel".into(),
                message: format!(
                    "Update channel must be 'stable', 'beta', or 'nightly', got '{}'",
                    config.general.update_channel
                ),
                severity: ValidationSeverity::Warning,
            });
        }

        // Cross-field constraints
        if config.ai.inference_mode == "always_local" && config.ai.local_model.is_empty() {
            errors.push(ConfigValidationError {
                field: "ai.local_model".into(),
                message: "local_model must be set when inference_mode is 'always_local'".into(),
                severity: ValidationSeverity::Error,
            });
        }

        if config.general.language.len() != 2 && !config.general.language.contains('-') {
            errors.push(ConfigValidationError {
                field: "general.language".into(),
                message: format!(
                    "Language should be a BCP 47 tag (e.g., 'en', 'ja'), got '{}'",
                    config.general.language
                ),
                severity: ValidationSeverity::Warning,
            });
        }

        errors
    }

    /// Merge a file-loaded config into the default config.
    fn merge_configs(base: &mut LumiConfig, overlay: LumiConfig) {
        // General
        if overlay.general.language != "en" {
            base.general.language = overlay.general.language;
        }
        if overlay.general.update_channel != "stable" {
            base.general.update_channel = overlay.general.update_channel;
        }
        base.general.start_on_login = overlay.general.start_on_login;
        base.general.check_updates = overlay.general.check_updates;

        // Character
        if overlay.character.name != "Lumi" {
            base.character.name = overlay.character.name;
        }
        base.character.size_scale = overlay.character.size_scale;
        base.character.position_x = overlay.character.position_x;
        base.character.position_y = overlay.character.position_y;

        // AI
        if overlay.ai.inference_mode != "prefer_local" {
            base.ai.inference_mode = overlay.ai.inference_mode;
        }
        base.ai.temperature = overlay.ai.temperature;
        base.ai.max_response_tokens = overlay.ai.max_response_tokens;

        // Voice
        base.voice.enabled = overlay.voice.enabled;
        base.voice.tts_enabled = overlay.voice.tts_enabled;

        // Privacy
        base.privacy.screen_capture_enabled = overlay.privacy.screen_capture_enabled;
        base.privacy.telemetry_enabled = overlay.privacy.telemetry_enabled;
        base.privacy.crash_reports_enabled = overlay.privacy.crash_reports_enabled;

        // Memory
        base.memory.enabled = overlay.memory.enabled;
        base.memory.retention_days_default = overlay.memory.retention_days_default;
        base.memory.auto_extract = overlay.memory.auto_extract;

        // Performance
        base.performance.render_fps_cap = overlay.performance.render_fps_cap;
        base.performance.gpu_memory_limit_mb = overlay.performance.gpu_memory_limit_mb;
        base.performance.animation_quality = overlay.performance.animation_quality;
    }

    /// Get the config file path.
    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_loaded() {
        let loader = ConfigLoader::new();
        let config = loader.current();
        assert_eq!(config.general.language, "en");
        assert_eq!(config.ai.temperature, 0.7);
    }

    #[test]
    fn test_parse_bool() {
        let mut errors = Vec::new();
        assert!(ConfigLoader::parse_bool("TEST", "true", &mut errors));
        assert!(!ConfigLoader::parse_bool("TEST", "false", &mut errors));
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validation_temperature_out_of_range() {
        let loader = ConfigLoader::new();
        let mut config = LumiConfig::default();
        config.ai.temperature = 3.0;
        let errors = loader.validate(&config);
        let temp_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.field == "ai.temperature")
            .collect();
        assert!(!temp_errors.is_empty());
    }

    #[test]
    fn test_validation_update_channel() {
        let loader = ConfigLoader::new();
        let mut config = LumiConfig::default();
        config.general.update_channel = "invalid".into();
        let errors = loader.validate(&config);
        let channel_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.field == "general.update_channel")
            .collect();
        assert!(!channel_errors.is_empty());
    }

    #[test]
    fn test_config_path() {
        let path = ConfigLoader::get_config_path();
        assert!(path.ends_with("config.toml"));
    }
}
