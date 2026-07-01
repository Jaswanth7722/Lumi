use serde::{Deserialize, Serialize};

/// Runtime behavior configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    /// Whether to show the runtime console/debug window.
    #[serde(default)]
    pub show_console: bool,

    /// Log level: "trace", "debug", "info", "warn", "error".
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Maximum number of concurrent background tasks.
    #[serde(default = "default_max_tasks")]
    pub max_concurrent_tasks: u32,

    /// Graceful shutdown timeout in seconds.
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout_secs: u64,
}

fn default_log_level() -> String {
    "info".into()
}
fn default_max_tasks() -> u32 {
    128
}
fn default_shutdown_timeout() -> u64 {
    30
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            show_console: false,
            log_level: default_log_level(),
            max_concurrent_tasks: default_max_tasks(),
            shutdown_timeout_secs: default_shutdown_timeout(),
        }
    }
}
