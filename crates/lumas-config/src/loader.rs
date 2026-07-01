//! # Multi-Stage Configuration Loader
//!
//! Implements a 7-stage loading pipeline as a state machine.

use crate::cache::ConfigCache;
use crate::env::ENV_REGISTRY;
use crate::error::ConfigError;
use crate::events::{ConfigEventPublisher, ConfigLoaded, ConfigMigrated};
use crate::migration::{MigrationEngine, MigrationV0ToV1};
use crate::platform;
use crate::resolver::{merge_configs, ResolvedConfig};
use crate::schema::LumiConfig;
use crate::validator::validate_config;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Stages of the config loading pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoaderStage {
    Unstarted,
    LoadingDefaults,
    LoadingFile {
        path: PathBuf,
    },
    LoadingEnvironment,
    ApplyingOverrides,
    RunningMigration {
        from: u32,
        to: u32,
    },
    Validating,
    Complete,
    Failed {
        stage: Box<LoaderStage>,
        error: String,
    },
}

/// Multi-stage configuration loader.
pub struct ConfigLoader {
    /// Current stage in the loading pipeline.
    stage: LoaderStage,
    /// Path to the config file (None to use default platform path).
    file_path: Option<PathBuf>,
    /// CLI argument overrides as (key, value) pairs.
    args: Vec<(String, String)>,
    /// Event publisher for emitting config lifecycle events.
    event_publisher: Option<Arc<dyn ConfigEventPublisher>>,
}

impl ConfigLoader {
    /// Create a new config loader.
    pub fn new() -> Self {
        Self {
            stage: LoaderStage::Unstarted,
            file_path: None,
            args: Vec::new(),
            event_publisher: None,
        }
    }

    /// Set the file path to load from (instead of the default platform path).
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.file_path = Some(path);
        self
    }

    /// Set CLI argument overrides.
    pub fn with_args(mut self, args: Vec<(String, String)>) -> Self {
        self.args = args;
        self
    }

    /// Set the event publisher for emitting config lifecycle events.
    pub fn with_event_publisher(mut self, publisher: Arc<dyn ConfigEventPublisher>) -> Self {
        self.event_publisher = Some(publisher);
        self
    }

    /// Get the current stage.
    pub fn stage(&self) -> &LoaderStage {
        &self.stage
    }

    /// Execute all 7 stages and return a validated LumiConfig + cache.
    pub async fn load(mut self) -> Result<(ConfigCache, Arc<LumiConfig>), ConfigError> {
        // Stage 1: Defaults
        self.stage = LoaderStage::LoadingDefaults;
        debug!("Stage 1: Loading defaults");
        let mut config = LumiConfig::default();

        // Stage 2: File Load
        let config_path = self.file_path.clone().unwrap_or_else(|| {
            platform::config_file_path().unwrap_or_else(|_| PathBuf::from("config.toml"))
        });

        self.stage = LoaderStage::LoadingFile {
            path: config_path.clone(),
        };
        let mut file_loaded = false;

        if config_path.exists() {
            debug!("Stage 2: Loading config file: {:?}", config_path);
            match std::fs::read_to_string(&config_path) {
                Ok(content) => {
                    match toml::from_str::<toml::Value>(&content) {
                        Ok(toml_value) => {
                            // Check schema version for migration
                            let schema_version = toml_value
                                .get("schema_version")
                                .and_then(|v| v.as_integer())
                                .unwrap_or(0)
                                as u32;

                            if schema_version > LumiConfig::CURRENT_SCHEMA_VERSION {
                                return Err(ConfigError::SchemaTooNew {
                                    found: schema_version,
                                    max: LumiConfig::CURRENT_SCHEMA_VERSION,
                                });
                            }

                            // Stage 5 (interleaved): Migration
                            if schema_version < LumiConfig::CURRENT_SCHEMA_VERSION {
                                self.stage = LoaderStage::RunningMigration {
                                    from: schema_version,
                                    to: LumiConfig::CURRENT_SCHEMA_VERSION,
                                };
                                debug!(
                                    "Stage 5: Running migration v{schema_version} → v{}",
                                    LumiConfig::CURRENT_SCHEMA_VERSION
                                );

                                let mut engine = MigrationEngine::new();
                                engine.register(Box::new(MigrationV0ToV1));
                                let (migrated, log) = engine.migrate(toml_value, schema_version)?;

                                // Deserialize migrated TOML
                                let migrated_str = toml::to_string(&migrated).map_err(|e| {
                                    ConfigError::MigrationFailed {
                                        from: schema_version,
                                        to: LumiConfig::CURRENT_SCHEMA_VERSION,
                                        reason: format!("Failed to serialize migrated config: {e}"),
                                    }
                                })?;
                                let file_config: LumiConfig = toml::from_str(&migrated_str)
                                    .map_err(|e| ConfigError::ParseError {
                                        path: config_path.clone(),
                                        line: 0,
                                        col: 0,
                                        message: format!(
                                            "Failed to deserialize migrated config: {e}"
                                        ),
                                    })?;

                                config = merge_configs(config, file_config);

                                if let Some(ref publisher) = self.event_publisher {
                                    publisher
                                        .on_config_migrated(ConfigMigrated::new(
                                            schema_version,
                                            LumiConfig::CURRENT_SCHEMA_VERSION,
                                            log,
                                        ))
                                        .await;
                                }
                            } else {
                                let file_config: LumiConfig =
                                    toml::from_str(&content).map_err(|e| {
                                        ConfigError::ParseError {
                                            path: config_path.clone(),
                                            line: 0,
                                            col: 0,
                                            message: format!("TOML parse error: {e}"),
                                        }
                                    })?;
                                config = merge_configs(config, file_config);
                            }

                            file_loaded = true;
                        }
                        Err(e) => {
                            return Err(ConfigError::ParseError {
                                path: config_path.clone(),
                                line: 0,
                                col: 0,
                                message: format!("TOML parse error: {e}"),
                            });
                        }
                    }
                }
                Err(e) => {
                    return Err(ConfigError::FileNotFound {
                        path: config_path.clone(),
                        source: e,
                    });
                }
            }
        } else {
            info!("No config file found at {:?}, using defaults", config_path);
        }

        // Stage 3: Environment Variables
        self.stage = LoaderStage::LoadingEnvironment;
        debug!("Stage 3: Loading environment variables");
        match ENV_REGISTRY.load() {
            Ok(env_overrides) => {
                for (key, value) in &env_overrides {
                    debug!("  Env override: {key}");
                    config = apply_toml_override(config, key, value.clone());
                }
            }
            Err(errors) => {
                for err in &errors {
                    warn!("Environment variable error: {err}");
                }
            }
        }

        // Stage 4: CLI Overrides
        self.stage = LoaderStage::ApplyingOverrides;
        debug!("Stage 4: Applying CLI overrides");
        for (key, value) in &self.args {
            debug!("  CLI override: {key} = {value}");
            config = apply_cli_override(config, key, value);
        }

        // Stage 6: Validation
        self.stage = LoaderStage::Validating;
        debug!("Stage 6: Validating configuration");
        validate_config(&config)?;

        // Stage 7: Complete
        self.stage = LoaderStage::Complete;
        debug!("Stage 7: Complete");

        // Create cache
        let resolved =
            ResolvedConfig::new(Arc::new(config.clone()), std::collections::HashMap::new());
        let cache = ConfigCache::new(config.clone());
        cache.store(config.clone(), resolved);

        // Emit ConfigLoaded event
        if let Some(ref publisher) = self.event_publisher {
            publisher
                .on_config_loaded(ConfigLoaded::new(
                    if file_loaded { Some(config_path) } else { None },
                    LumiConfig::CURRENT_SCHEMA_VERSION,
                ))
                .await;
        }

        Ok((cache, Arc::new(config)))
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply a single TOML value override by dotted key path.
/// Serializes the config to TOML, sets the value at the path, and deserializes back.
fn apply_toml_override(config: LumiConfig, key: &str, value: toml::Value) -> LumiConfig {
    let toml_str = match toml::to_string(&config) {
        Ok(s) => s,
        Err(_) => return config,
    };
    let mut root: toml::Value = match toml::from_str(&toml_str) {
        Ok(v) => v,
        Err(_) => return config,
    };

    set_toml_value(&mut root, key, value);

    match toml::from_str(&toml::to_string(&root).unwrap_or_default()) {
        Ok(c) => c,
        Err(_) => config,
    }
}

/// Apply a CLI string override by dotted key path.
/// Parses the string value as a TOML value before applying.
fn apply_cli_override(config: LumiConfig, key: &str, value: &str) -> LumiConfig {
    // Try to parse as TOML value (number, bool, string)
    let toml_val = if let Ok(n) = value.parse::<i64>() {
        toml::Value::Integer(n)
    } else if let Ok(n) = value.parse::<f64>() {
        toml::Value::Float(n)
    } else if let Ok(b) = value.parse::<bool>() {
        toml::Value::Boolean(b)
    } else {
        toml::Value::String(value.to_string())
    };

    apply_toml_override(config, key, toml_val)
}

/// Walk a dotted key path into a TOML value tree and set the leaf value.
fn set_toml_value(root: &mut toml::Value, key: &str, value: toml::Value) {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return;
    }

    let mut current = root;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // Last part: set the value
            if let Some(table) = current.as_table_mut() {
                table.insert(part.to_string(), value.clone());
            }
        } else {
            // Intermediate part: navigate deeper
            if let Some(table) = current.as_table_mut() {
                if !table.contains_key(*part) {
                    table.insert(
                        (*part).to_string(),
                        toml::Value::Table(toml::value::Table::new()),
                    );
                }
                // Move to the nested value
                if let Some(next) = table.get_mut(*part) {
                    current = next;
                } else {
                    return;
                }
            } else {
                return;
            }
        }
    }
}
