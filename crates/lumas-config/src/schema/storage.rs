use serde::{Deserialize, Serialize};

/// Storage/data persistence configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    #[serde(default)]
    pub data_dir: Option<String>,
    #[serde(default)]
    pub models_dir: Option<String>,
    #[serde(default = "default_cache_size")]
    pub max_cache_size_mb: u32,
    #[serde(default = "default_true")]
    pub compress_cache: bool,
    #[serde(default = "default_backup_count")]
    pub backup_retention_count: u32,
}

fn default_cache_size() -> u32 {
    1024
}
fn default_true() -> bool {
    true
}
fn default_backup_count() -> u32 {
    5
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            models_dir: None,
            max_cache_size_mb: 1024,
            compress_cache: true,
            backup_retention_count: 5,
        }
    }
}
