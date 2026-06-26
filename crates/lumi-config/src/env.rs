//! # Environment Variable Loader
//!
//! Compile-time registry mapping LUMI_* env var names to config field paths and types.

use crate::error::ConfigError;

/// Type of value an environment variable should be parsed as.
#[derive(Debug, Clone)]
pub enum EnvValueType {
    Bool,
    U32,
    U64,
    F32,
    F64,
    String,
    Path,
    Url,
    /// Enum with known valid variants.
    Enum {
        variants: &'static [&'static str],
    },
}

/// A single entry in the environment variable registry.
#[derive(Debug, Clone)]
pub struct EnvEntry {
    /// The full environment variable name (e.g., "LUMI_AI_TEMPERATURE").
    pub env_var: &'static str,
    /// The dotted config field path (e.g., "ai.temperature").
    pub config_path: &'static str,
    /// The expected type of the value.
    pub value_type: EnvValueType,
    /// Human-readable description.
    pub description: &'static str,
}

/// Registry mapping LUMI_* env var names to config field paths.
/// This is the single source of truth for all supported env vars.
pub struct EnvRegistry {
    /// All registered env var entries.
    pub entries: &'static [EnvEntry],
}

impl EnvRegistry {
    /// Load all LUMI_* environment variables and return key-value pairs
    /// representing only the overridden fields.
    pub fn load(&self) -> Result<Vec<(String, toml::Value)>, Vec<ConfigError>> {
        let mut overrides = Vec::new();
        let mut errors = Vec::new();

        for entry in self.entries {
            if let Ok(value) = std::env::var(entry.env_var) {
                match self.parse_value(entry, &value) {
                    Ok(toml_value) => {
                        overrides.push((entry.config_path.to_string(), toml_value));
                    }
                    Err(e) => errors.push(e),
                }
            }
        }

        if errors.is_empty() {
            Ok(overrides)
        } else {
            Err(errors)
        }
    }

    /// Parse an env var string value into a toml::Value according to its registered type.
    fn parse_value(&self, entry: &EnvEntry, value: &str) -> Result<toml::Value, ConfigError> {
        match &entry.value_type {
            EnvValueType::Bool => match value.to_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Ok(toml::Value::Boolean(true)),
                "false" | "0" | "no" | "off" => Ok(toml::Value::Boolean(false)),
                _ => Err(ConfigError::EnvVarInvalid {
                    var: entry.env_var.to_string(),
                    value: value.to_string(),
                    reason: format!("Cannot parse '{value}' as boolean"),
                }),
            },
            EnvValueType::U32 => value
                .parse::<u32>()
                .map(|n| toml::Value::Integer(n as i64))
                .map_err(|e| ConfigError::EnvVarInvalid {
                    var: entry.env_var.to_string(),
                    value: value.to_string(),
                    reason: format!("Cannot parse as u32: {e}"),
                }),
            EnvValueType::U64 => value
                .parse::<u64>()
                .map(|n| toml::Value::Integer(n as i64))
                .map_err(|e| ConfigError::EnvVarInvalid {
                    var: entry.env_var.to_string(),
                    value: value.to_string(),
                    reason: format!("Cannot parse as u64: {e}"),
                }),
            EnvValueType::F32 | EnvValueType::F64 => value
                .parse::<f64>()
                .map(toml::Value::Float)
                .map_err(|e| ConfigError::EnvVarInvalid {
                    var: entry.env_var.to_string(),
                    value: value.to_string(),
                    reason: format!("Cannot parse as f64: {e}"),
                }),
            EnvValueType::String => Ok(toml::Value::String(value.to_string())),
            EnvValueType::Path => Ok(toml::Value::String(value.to_string())),
            EnvValueType::Url => {
                // Validate URL format
                url::Url::parse(value)
                    .map(|_| toml::Value::String(value.to_string()))
                    .map_err(|e| ConfigError::EnvVarInvalid {
                        var: entry.env_var.to_string(),
                        value: value.to_string(),
                        reason: format!("Invalid URL: {e}"),
                    })
            }
            EnvValueType::Enum { variants } => {
                if variants.contains(&value) {
                    Ok(toml::Value::String(value.to_string()))
                } else {
                    Err(ConfigError::EnvVarInvalid {
                        var: entry.env_var.to_string(),
                        value: value.to_string(),
                        reason: format!(
                            "Invalid value '{value}'. Expected one of: {}",
                            variants.join(", ")
                        ),
                    })
                }
            }
        }
    }
}

/// Full registry of all supported environment variable overrides.
/// Must be kept in sync with LumiConfig schema.
pub static ENV_REGISTRY: EnvRegistry = EnvRegistry {
    entries: &[
        EnvEntry {
            env_var: "LUMI_GENERAL_LANGUAGE",
            config_path: "general.language",
            value_type: EnvValueType::String,
            description: "Override the UI language",
        },
        EnvEntry {
            env_var: "LUMI_GENERAL_START_ON_LOGIN",
            config_path: "general.start_on_login",
            value_type: EnvValueType::Bool,
            description: "Start on OS login",
        },
        EnvEntry {
            env_var: "LUMI_GENERAL_UPDATE_CHANNEL",
            config_path: "general.update_channel",
            value_type: EnvValueType::Enum {
                variants: &["stable", "beta", "nightly"],
            },
            description: "Override the update channel",
        },
        EnvEntry {
            env_var: "LUMI_AI_INFERENCE_MODE",
            config_path: "ai.inference_mode",
            value_type: EnvValueType::Enum {
                variants: &[
                    "always_local",
                    "always_cloud",
                    "prefer_local",
                    "prefer_cloud",
                ],
            },
            description: "Override the AI inference mode",
        },
        EnvEntry {
            env_var: "LUMI_AI_CLOUD_PROVIDER",
            config_path: "ai.cloud_provider",
            value_type: EnvValueType::Enum {
                variants: &["anthropic", "openai_compatible"],
            },
            description: "Override the cloud AI provider",
        },
        EnvEntry {
            env_var: "LUMI_AI_LOCAL_MODEL",
            config_path: "ai.local_model",
            value_type: EnvValueType::String,
            description: "Override the local model filename",
        },
        EnvEntry {
            env_var: "LUMI_AI_TEMPERATURE",
            config_path: "ai.temperature",
            value_type: EnvValueType::F64,
            description: "Override the AI temperature",
        },
        EnvEntry {
            env_var: "LUMI_AI_MAX_RESPONSE_TOKENS",
            config_path: "ai.max_response_tokens",
            value_type: EnvValueType::U32,
            description: "Override max response tokens",
        },
        EnvEntry {
            env_var: "LUMI_AI_ANTHROPIC_API_KEY",
            config_path: "ai.anthropic_api_key",
            value_type: EnvValueType::String,
            description: "Anthropic API key (treated as secret, never logged)",
        },
        EnvEntry {
            env_var: "LUMI_PERFORMANCE_RENDER_FPS_CAP",
            config_path: "performance.render_fps_cap",
            value_type: EnvValueType::U32,
            description: "Override render FPS cap",
        },
        EnvEntry {
            env_var: "LUMI_PERFORMANCE_GPU_MEMORY_LIMIT_MB",
            config_path: "performance.gpu_memory_limit_mb",
            value_type: EnvValueType::U32,
            description: "Override GPU memory limit in MB",
        },
        EnvEntry {
            env_var: "LUMI_VOICE_ENABLED",
            config_path: "voice.enabled",
            value_type: EnvValueType::Bool,
            description: "Enable/disable voice features",
        },
        EnvEntry {
            env_var: "LUMI_PRIVACY_TELEMETRY_ENABLED",
            config_path: "privacy.telemetry_enabled",
            value_type: EnvValueType::Bool,
            description: "Enable/disable telemetry",
        },
        EnvEntry {
            env_var: "LUMI_LOGGING_LEVEL",
            config_path: "logging.level",
            value_type: EnvValueType::Enum {
                variants: &["trace", "debug", "info", "warn", "error"],
            },
            description: "Override the logging level",
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_registry_has_entries() {
        assert!(!ENV_REGISTRY.entries.is_empty());
    }

    #[test]
    fn test_parse_bool_true() {
        let entry = EnvEntry {
            env_var: "TEST_BOOL",
            config_path: "test.bool",
            value_type: EnvValueType::Bool,
            description: "",
        };
        let val = ENV_REGISTRY.parse_value(&entry, "true").unwrap();
        assert_eq!(val, toml::Value::Boolean(true));
    }

    #[test]
    fn test_parse_u32() {
        let entry = EnvEntry {
            env_var: "TEST_U32",
            config_path: "test.u32",
            value_type: EnvValueType::U32,
            description: "",
        };
        let val = ENV_REGISTRY.parse_value(&entry, "42").unwrap();
        assert_eq!(val, toml::Value::Integer(42));
    }

    #[test]
    fn test_parse_invalid_enum_returns_error() {
        let entry = EnvEntry {
            env_var: "TEST_ENUM",
            config_path: "test.enum",
            value_type: EnvValueType::Enum {
                variants: &["a", "b", "c"],
            },
            description: "",
        };
        let result = ENV_REGISTRY.parse_value(&entry, "invalid");
        assert!(result.is_err());
    }
}
