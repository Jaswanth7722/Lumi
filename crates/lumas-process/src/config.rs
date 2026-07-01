//! # Process Management Configuration
//!
//! Configuration for the process management system.
//!
//! Loaded once during bootstrap and stored in `RuntimeContext`.
//! Hot-reloadable via `ArcSwap` for runtime configuration changes.

use crate::heartbeat::HeartbeatConfig;
use crate::resource::ResourceLimits;
use crate::restart::RestartPolicy;
use crate::supervisor::SupervisionStrategy;
use serde::{Deserialize, Serialize};

/// Top-level configuration for the process management system.
///
/// # Examples
///
/// ```ignore
/// use lumas_process::ProcessManagementConfig;
/// let config = ProcessManagementConfig::default();
/// assert_eq!(config.startup_max_parallel, 4);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessManagementConfig {
    /// Default supervision strategy for the root supervisor.
    pub supervision_strategy: SupervisionStrategy,
    /// Maximum number of processes to start in parallel.
    pub startup_max_parallel: usize,
    /// Default heartbeat configuration for all processes.
    pub default_heartbeat: HeartbeatConfig,
    /// Default restart policy for all processes.
    pub default_restart_policy: RestartPolicy,
    /// Default resource limits for all processes.
    pub default_resource_limits: ResourceLimits,
    /// Whether to enable the background resource monitor.
    pub enable_resource_monitor: bool,
    /// Whether to enable the heartbeat checker.
    pub enable_heartbeat_checker: bool,
    /// Maximum number of worker restart attempts.
    pub worker_max_restarts: u32,
}

impl Default for ProcessManagementConfig {
    fn default() -> Self {
        Self {
            supervision_strategy: SupervisionStrategy::OneForOne,
            startup_max_parallel: 4,
            default_heartbeat: HeartbeatConfig::default(),
            default_restart_policy: RestartPolicy::Immediate {
                max_restarts: 3,
                window_secs: 60,
            },
            default_resource_limits: ResourceLimits::default(),
            enable_resource_monitor: true,
            enable_heartbeat_checker: true,
            worker_max_restarts: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ProcessManagementConfig::default();
        assert_eq!(config.startup_max_parallel, 4);
        assert!(config.enable_resource_monitor);
    }
}
