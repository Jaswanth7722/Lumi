use serde::{Deserialize, Serialize};

/// IPC (inter-process communication) configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IPCConfig {
    #[serde(default = "default_socket_path")]
    pub socket_path: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_timeout_ms")]
    pub connection_timeout_ms: u64,
    #[serde(default = "default_max_retries")]
    pub max_connection_retries: u32,
}

fn default_socket_path() -> String {
    "/tmp/lumi.sock".into()
}
fn default_port() -> u16 {
    0
}
fn default_timeout_ms() -> u64 {
    5000
}
fn default_max_retries() -> u32 {
    3
}

impl Default for IPCConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            port: 0,
            connection_timeout_ms: 5000,
            max_connection_retries: 3,
        }
    }
}
