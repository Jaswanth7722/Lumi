use crate::secret::Secret;
use serde::{Deserialize, Serialize};

/// Security and authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub validate_plugin_signatures: bool,
    #[serde(default, skip_serializing)]
    pub plugin_signing_key: Option<Secret<String>>,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_true")]
    pub sandbox_plugins: bool,
    #[serde(default)]
    pub network_access: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            validate_plugin_signatures: true,
            plugin_signing_key: None,
            allowed_origins: vec!["https://lumi.app".into()],
            sandbox_plugins: true,
            network_access: false,
        }
    }
}
