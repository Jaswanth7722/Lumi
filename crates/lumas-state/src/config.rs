//! # State Machine Configuration
//!
//! Mirrors the `[state_machine]` section of the Lumas config.

use std::time::Duration;

/// Top-level state machine system configuration.
#[derive(Debug, Clone)]
pub struct StateMachineConfig {
    /// Per-machine event queue capacity.
    pub event_queue_capacity: usize,
    /// Maximum concurrent transitions across all machines.
    pub max_concurrent_transitions: usize,
    /// Default timeout for any single transition.
    pub transition_timeout: Duration,
    /// Maximum time for a guard evaluation.
    pub guard_timeout: Duration,
    /// Maximum time for a blocking action.
    pub action_timeout: Duration,
    /// Scheduler configuration.
    pub scheduler: SchedulerConfig,
    /// History configuration.
    pub history: HistoryConfig,
    /// Recovery configuration.
    pub recovery: RecoveryConfig,
    /// Visualization configuration (only used with feature = "visualization").
    pub visualization: VisualizationConfig,
}

impl Default for StateMachineConfig {
    fn default() -> Self {
        Self {
            event_queue_capacity: 256,
            max_concurrent_transitions: 8,
            transition_timeout: Duration::from_millis(5000),
            guard_timeout: Duration::from_millis(500),
            action_timeout: Duration::from_millis(2000),
            scheduler: SchedulerConfig::default(),
            history: HistoryConfig::default(),
            recovery: RecoveryConfig::default(),
            visualization: VisualizationConfig::default(),
        }
    }
}

/// Scheduler configuration.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Scheduler tick resolution in milliseconds.
    pub resolution_ms: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self { resolution_ms: 10 }
    }
}

/// History configuration.
#[derive(Debug, Clone)]
pub struct HistoryConfig {
    /// Maximum number of transition records per machine.
    pub max_records_per_machine: usize,
    /// Retention period in hours for transition history.
    pub retention_hours: u32,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_records_per_machine: 5000,
            retention_hours: 24,
        }
    }
}

/// Recovery configuration.
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Automatically recover on guard error.
    pub auto_recover_on_guard_error: bool,
    /// Fallback state name for recovery.
    pub recovery_state: String,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            auto_recover_on_guard_error: true,
            recovery_state: "idle".into(),
        }
    }
}

/// Visualization configuration (only used with feature = "visualization").
#[derive(Debug, Clone)]
pub struct VisualizationConfig {
    /// Output directory for generated state graphs.
    pub output_dir: String,
    /// Auto-generate graphs on startup.
    pub auto_generate_on_startup: bool,
}

impl Default for VisualizationConfig {
    fn default() -> Self {
        Self {
            output_dir: "{data_dir}/state-graphs".into(),
            auto_generate_on_startup: false,
        }
    }
}

impl StateMachineConfig {
    /// Validate the configuration, returning an error if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.event_queue_capacity == 0 {
            return Err("event_queue_capacity must be > 0".into());
        }
        if self.max_concurrent_transitions == 0 {
            return Err("max_concurrent_transitions must be > 0".into());
        }
        if self.transition_timeout.is_zero() {
            return Err("transition_timeout must be > 0".into());
        }
        if self.guard_timeout.is_zero() {
            return Err("guard_timeout must be > 0".into());
        }
        if self.action_timeout.is_zero() {
            return Err("action_timeout must be > 0".into());
        }
        if self.scheduler.resolution_ms == 0 {
            return Err("scheduler.resolution_ms must be > 0".into());
        }
        if self.history.max_records_per_machine == 0 {
            return Err("history.max_records_per_machine must be > 0".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = StateMachineConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.event_queue_capacity, 256);
        assert_eq!(config.scheduler.resolution_ms, 10);
    }

    #[test]
    fn test_validation_fails_on_zero_capacity() {
        let mut config = StateMachineConfig::default();
        config.event_queue_capacity = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_recovery_config_defaults() {
        let config = RecoveryConfig::default();
        assert!(config.auto_recover_on_guard_error);
        assert_eq!(config.recovery_state, "idle");
    }
}
