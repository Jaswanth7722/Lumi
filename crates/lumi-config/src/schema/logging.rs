use serde::{Deserialize, Serialize};

/// Logging and telemetry configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    #[serde(default = "default_level")]
    pub level: String,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_true")]
    pub file_logging: bool,
    #[serde(default)]
    pub log_dir: Option<String>,
    #[serde(default = "default_max_logs")]
    pub max_log_files: u32,
    #[serde(default = "default_max_size")]
    pub max_log_file_size_mb: u32,
}

fn default_level() -> String {
    "info".into()
}
fn default_format() -> String {
    "json".into()
}
fn default_true() -> bool {
    true
}
fn default_max_logs() -> u32 {
    10
}
fn default_max_size() -> u32 {
    50
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            format: default_format(),
            file_logging: true,
            log_dir: None,
            max_log_files: 10,
            max_log_file_size_mb: 50,
        }
    }
}
