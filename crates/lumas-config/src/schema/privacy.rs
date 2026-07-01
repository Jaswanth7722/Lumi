use serde::{Deserialize, Serialize};

/// Privacy and data collection configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PrivacyConfig {
    #[serde(default)]
    pub screen_capture_enabled: bool,
    #[serde(default = "default_clipboard")]
    pub clipboard_access: String,
    #[serde(default)]
    pub telemetry_enabled: bool,
    #[serde(default = "default_true")]
    pub crash_reports_enabled: bool,
    #[serde(default)]
    pub analytics_enabled: bool,
    #[serde(default = "default_true")]
    pub local_processing_only: bool,
}

fn default_clipboard() -> String {
    "on_request".into()
}
fn default_true() -> bool {
    true
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            screen_capture_enabled: false,
            clipboard_access: default_clipboard(),
            telemetry_enabled: false,
            crash_reports_enabled: true,
            analytics_enabled: false,
            local_processing_only: true,
        }
    }
}
