use serde::{Deserialize, Serialize};

/// Automatic update configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpdateConfig {
    #[serde(default = "default_channel")]
    pub channel: String,
    #[serde(default = "default_true")]
    pub auto_check: bool,
    #[serde(default = "default_true")]
    pub auto_download: bool,
    #[serde(default)]
    pub auto_install: bool,
    #[serde(default = "default_interval")]
    pub check_interval_hours: u32,
    #[serde(default)]
    pub proxy_url: Option<String>,
}

fn default_channel() -> String {
    "stable".into()
}
fn default_true() -> bool {
    true
}
fn default_interval() -> u32 {
    24
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            channel: default_channel(),
            auto_check: true,
            auto_download: true,
            auto_install: false,
            check_interval_hours: 24,
            proxy_url: None,
        }
    }
}
