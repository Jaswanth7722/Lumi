use serde::{Deserialize, Serialize};

/// Plugin system configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub allow_unsigned_plugins: bool,
    #[serde(default = "default_max_plugins")]
    pub max_active_plugins: u32,
    #[serde(default)]
    pub sandbox_level: String,
    #[serde(default)]
    pub plugin_dir: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_max_plugins() -> u32 {
    20
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_unsigned_plugins: false,
            max_active_plugins: 20,
            sandbox_level: "isolated".into(),
            plugin_dir: None,
        }
    }
}
