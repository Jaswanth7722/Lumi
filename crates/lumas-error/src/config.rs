//! # Error System Configuration
//!
//! Configuration for the error handling system.
//! Mirrors the `[error_handling]` section of the Lumas config.

use std::path::PathBuf;

/// Configuration for the error handling system.
#[derive(Debug, Clone)]
pub struct ErrorConfig {
    /// Directory to write crash reports to.
    pub crash_report_dir: PathBuf,
    /// Maximum number of crash reports to retain (oldest are rotated).
    pub max_crash_reports: usize,
    /// Capacity of the error history buffer.
    pub error_history_capacity: usize,
    /// Whether stack traces are enabled (requires "backtrace" feature).
    pub stack_traces_enabled: bool,
    /// Whether to sanitize secrets in error output.
    pub sanitize_secrets: bool,
    /// Window in seconds for pattern escalation detection.
    pub pattern_escalation_window_secs: u64,
    /// Number of occurrences within the window to escalate severity.
    pub pattern_escalation_threshold: u32,
    /// Window in seconds for recovery thrash detection.
    pub recovery_thrash_window_secs: u64,
    /// Number of recovery attempts within window to trigger safe shutdown.
    pub recovery_thrash_threshold: u32,
}

impl Default for ErrorConfig {
    fn default() -> Self {
        Self {
            crash_report_dir: PathBuf::from("crashes"),
            max_crash_reports: 50,
            error_history_capacity: 10_000,
            stack_traces_enabled: false,
            sanitize_secrets: true,
            pattern_escalation_window_secs: 60,
            pattern_escalation_threshold: 5,
            recovery_thrash_window_secs: 60,
            recovery_thrash_threshold: 5,
        }
    }
}

impl ErrorConfig {
    /// Create a new error config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the crash report directory.
    pub fn with_crash_report_dir(mut self, dir: PathBuf) -> Self {
        self.crash_report_dir = dir;
        self
    }

    /// Enable stack traces.
    pub fn with_stack_traces(mut self, enabled: bool) -> Self {
        self.stack_traces_enabled = enabled;
        self
    }

    /// Validate the configuration.
    ///
    /// # Returns
    /// `Ok(())` if the config is valid, `Err(String)` with a description of the issue.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_crash_reports == 0 {
            return Err("max_crash_reports must be > 0".to_string());
        }
        if self.error_history_capacity == 0 {
            return Err("error_history_capacity must be > 0".to_string());
        }
        if self.pattern_escalation_threshold == 0 {
            return Err("pattern_escalation_threshold must be > 0".to_string());
        }
        if self.recovery_thrash_threshold == 0 {
            return Err("recovery_thrash_threshold must be > 0".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ErrorConfig::default();
        assert!(config.sanitize_secrets);
        assert_eq!(config.max_crash_reports, 50);
        assert_eq!(config.error_history_capacity, 10_000);
    }

    #[test]
    fn test_config_validation() {
        let valid = ErrorConfig::default();
        assert!(valid.validate().is_ok());

        let invalid = ErrorConfig {
            max_crash_reports: 0,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_builder() {
        let config = ErrorConfig::new()
            .with_crash_report_dir(PathBuf::from("/tmp/crashes"))
            .with_stack_traces(true);

        assert_eq!(config.crash_report_dir, PathBuf::from("/tmp/crashes"));
        assert!(config.stack_traces_enabled);
    }
}
