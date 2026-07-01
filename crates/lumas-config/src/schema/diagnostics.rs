use serde::{Deserialize, Serialize};

/// Diagnostics and debugging configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsConfig {
    #[serde(default)]
    pub enable_metrics: bool,
    #[serde(default)]
    pub enable_profiling: bool,
    #[serde(default)]
    pub enable_tracing: bool,
    #[serde(default)]
    pub metrics_port: Option<u16>,
    #[serde(default = "default_true")]
    pub crash_reporting: bool,
    #[serde(default)]
    pub debug_mode: bool,
}

fn default_true() -> bool {
    true
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            enable_metrics: false,
            enable_profiling: false,
            enable_tracing: false,
            metrics_port: None,
            crash_reporting: true,
            debug_mode: false,
        }
    }
}
