//! # Logging Event Types
//!
//! Event types emitted by the logging system for integration with
//! lumi-runtime's typed event bus.

use chrono::{DateTime, Utc};
use lumi_runtime::event::Event;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Emitted when the logging system is fully initialized.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingInitialized {
    /// Names of registered sinks.
    pub sinks: Vec<String>,
    /// Global log level.
    pub level: crate::level::LogLevel,
    /// When the system was initialized.
    pub initialized_at: DateTime<Utc>,
}

impl Event for LoggingInitialized {
    fn event_type() -> &'static str {
        "LoggingInitialized"
    }
}

/// Emitted when a log file is rotated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotated {
    /// Name of the sink that rotated.
    pub sink_name: String,
    /// Previous file path.
    pub old_path: PathBuf,
    /// New file path.
    pub new_path: PathBuf,
    /// Size of the rotated file in bytes.
    pub size_bytes: u64,
    /// Whether the rotated file was compressed.
    pub compressed: bool,
    /// When the rotation occurred.
    pub rotated_at: DateTime<Utc>,
}

impl Event for LogRotated {
    fn event_type() -> &'static str {
        "LogRotated"
    }
}

/// Emitted when the pipeline drops records due to backpressure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecordsDropped {
    /// Number of records dropped.
    pub count: u64,
    /// Reason for dropping.
    pub reason: String,
    /// When the drop was detected.
    pub detected_at: DateTime<Utc>,
}

impl Event for LogRecordsDropped {
    fn event_type() -> &'static str {
        "LogRecordsDropped"
    }
}

/// Emitted when a sink fails to write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSinkError {
    /// Name of the failing sink.
    pub sink_name: String,
    /// Error description.
    pub error: String,
    /// When the error occurred.
    pub occurred_at: DateTime<Utc>,
}

impl Event for LogSinkError {
    fn event_type() -> &'static str {
        "LogSinkError"
    }
}

/// Emitted when the log level is changed at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLevelChanged {
    /// Previous log level.
    pub old_level: crate::level::LogLevel,
    /// New log level.
    pub new_level: crate::level::LogLevel,
    /// When the change occurred.
    pub changed_at: DateTime<Utc>,
}

impl Event for LogLevelChanged {
    fn event_type() -> &'static str {
        "LogLevelChanged"
    }
}

/// Emitted when logging is shut down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingShutdown {
    /// Total records written during this session.
    pub records_written: u64,
    /// Total records dropped during this session.
    pub records_dropped: u64,
    /// When the shutdown occurred.
    pub shutdown_at: DateTime<Utc>,
}

impl Event for LoggingShutdown {
    fn event_type() -> &'static str {
        "LoggingShutdown"
    }
}
