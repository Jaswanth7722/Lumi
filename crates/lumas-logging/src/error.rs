//! # Log Error Hierarchy
//!
//! Complete, structured error types for the logging system.
//! Every variant carries rich context for diagnostics.

use std::path::PathBuf;

/// Top-level logging error enum.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// Logger already installed; install() may only be called once.
    #[error("Logger already installed; install() may only be called once")]
    AlreadyInstalled,

    /// Logger not yet installed.
    #[error("Logger not yet installed; call LogManager::install() during bootstrap")]
    NotInstalled,

    /// Sink is already registered.
    #[error("Sink '{name}' is already registered")]
    SinkAlreadyRegistered { name: String },

    /// Sink not found.
    #[error("Sink '{name}' not found")]
    SinkNotFound { name: String },

    /// Pipeline channel full; record dropped.
    #[error("Log pipeline channel full; record dropped (total dropped: {total_dropped})")]
    ChannelFull { total_dropped: u64 },

    /// Sink write failed.
    #[error("Sink write failed for '{sink}': {source}")]
    SinkWriteFailed {
        sink: String,
        source: std::io::Error,
    },

    /// Log rotation failed.
    #[error("Log rotation failed for '{path}': {reason}")]
    RotationFailed { path: PathBuf, reason: String },

    /// Log compression failed.
    #[error("Log file compression failed for '{path}': {source}")]
    CompressionFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Formatter error.
    #[error("Log formatter error in '{formatter}': {reason}")]
    FormatError { formatter: String, reason: String },

    /// Redaction rule invalid.
    #[error("Redaction rule '{rule}' failed to compile: {reason}")]
    RedactionRuleInvalid { rule: String, reason: String },

    /// Export failed.
    #[error("Log export failed: {reason}")]
    ExportFailed { reason: String },

    /// Flush timed out.
    #[error("Log flush timed out after {timeout_ms}ms")]
    FlushTimeout { timeout_ms: u64 },

    /// Shutdown timed out.
    #[error("Log shutdown timed out after {timeout_ms}ms; unflushed records may be lost")]
    ShutdownTimeout { timeout_ms: u64 },
}

impl LogError {
    /// True if the application can continue safely after this error.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            LogError::ChannelFull { .. }
                | LogError::SinkWriteFailed { .. }
                | LogError::RotationFailed { .. }
                | LogError::CompressionFailed { .. }
                | LogError::FormatError { .. }
                | LogError::RedactionRuleInvalid { .. }
                | LogError::ExportFailed { .. }
                | LogError::FlushTimeout { .. }
                | LogError::ShutdownTimeout { .. }
        )
    }

    /// What the system or operator should do to resolve this condition.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            LogError::AlreadyInstalled => {
                "This is a programming error. LogManager::install() must only be called once during bootstrap."
            }
            LogError::NotInstalled => {
                "Call LogManager::install() during bootstrap before any subsystem starts."
            }
            LogError::SinkAlreadyRegistered { .. } => {
                "Remove the duplicate sink registration or use a different sink name."
            }
            LogError::SinkNotFound { .. } => {
                "Verify the sink name and register it before referencing it."
            }
            LogError::ChannelFull { .. } => {
                "Increase pipeline_channel_capacity in LoggingConfig or reduce log volume."
            }
            LogError::SinkWriteFailed { .. } => {
                "Check disk space, permissions, and that the log path is valid."
            }
            LogError::RotationFailed { .. } => {
                "Ensure the log directory is writable and has sufficient space."
            }
            LogError::CompressionFailed { .. } => {
                "Check disk space; compressed files require temporary storage."
            }
            LogError::FormatError { .. } => {
                "Check the formatter configuration; this is typically a programming error."
            }
            LogError::RedactionRuleInvalid { .. } => {
                "Fix the regex pattern in the redaction rule configuration."
            }
            LogError::ExportFailed { .. } => {
                "Check the export destination is writable and has sufficient space."
            }
            LogError::FlushTimeout { .. } => {
                "This is a transient condition; retry the flush operation."
            }
            LogError::ShutdownTimeout { .. } => {
                "Increase the shutdown timeout or check for stuck sink operations."
            }
        }
    }
}
