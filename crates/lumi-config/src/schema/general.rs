use serde::{Deserialize, Serialize};

/// General application settings.
///
/// Controls language, startup behavior, update checking, and theme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GeneralConfig {
    /// IETF BCP 47 language tag (e.g., "en", "ja", "zh-CN").
    /// Default: "en"
    #[serde(default = "default_language")]
    pub language: String,

    /// Whether to start Lumi on OS login.
    /// Default: true
    #[serde(default = "default_true")]
    pub start_on_login: bool,

    /// Whether to check for updates automatically.
    /// Default: true
    #[serde(default = "default_true")]
    pub check_updates: bool,

    /// Update channel: "stable", "beta", or "nightly".
    /// Default: "stable"
    #[serde(default = "default_update_channel")]
    pub update_channel: String,

    /// UI theme: "auto", "light", "dark", or "system".
    /// Default: "auto" (follows system preference)
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_language() -> String {
    "en".into()
}
fn default_update_channel() -> String {
    "stable".into()
}
fn default_theme() -> String {
    "auto".into()
}
fn default_true() -> bool {
    true
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            language: default_language(),
            start_on_login: true,
            check_updates: true,
            update_channel: default_update_channel(),
            theme: default_theme(),
        }
    }
}
