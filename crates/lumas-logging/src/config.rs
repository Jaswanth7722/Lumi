//! # Logging Config
//!
//! Mirrors the `logging` section of LumiConfig with additional
//! logging-system-specific fields.

use crate::level::LogLevel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Console output stream selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleStream {
    /// Write to stdout.
    Stdout,
    /// Write to stderr.
    Stderr,
    /// Auto: Info and below → stdout, Warn and above → stderr.
    Auto,
}

impl Default for ConsoleStream {
    fn default() -> Self {
        ConsoleStream::Auto
    }
}

/// Logging configuration for the lumi-logging system.
///
/// This parallels the `logging` section of LumiConfig from lumas-config
/// but adds logging-system-specific fields like pipeline capacity,
/// memory sink capacity, and subsystem-level overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    /// Minimum log level for the global filter. Default: Info
    #[serde(default = "LoggingConfig::default_level")]
    pub level: LogLevel,

    /// Enable ANSI color output for console sink. Default: true (auto-detected)
    #[serde(default = "LoggingConfig::default_console_colors")]
    pub console_colors: bool,

    /// Console output stream. Default: Auto
    #[serde(default)]
    pub console_stream: ConsoleStream,

    /// Enable file sink. Default: true
    #[serde(default = "LoggingConfig::default_file_enabled")]
    pub file_enabled: bool,

    /// Log file base path. Default: platform logs_dir()/lumi.log
    #[serde(default)]
    pub file_path: Option<PathBuf>,

    /// Maximum log file size in bytes before rotation. Default: 50MB
    #[serde(default = "LoggingConfig::default_max_file_size_bytes")]
    pub max_file_size_bytes: u64,

    /// Maximum number of rotated log files to retain. Default: 10
    #[serde(default = "LoggingConfig::default_max_rotated_files")]
    pub max_rotated_files: u32,

    /// Compress rotated log files with gzip. Default: true
    #[serde(default = "LoggingConfig::default_compress_rotated")]
    pub compress_rotated: bool,

    /// Log retention in days. Files older than this are deleted. Default: 30
    #[serde(default = "LoggingConfig::default_retention_days")]
    pub retention_days: u32,

    /// Internal pipeline channel capacity in records. Default: 65536
    #[serde(default = "LoggingConfig::default_pipeline_channel_capacity")]
    pub pipeline_channel_capacity: usize,

    /// Memory sink capacity (for diagnostics). Default: 10000
    #[serde(default = "LoggingConfig::default_memory_sink_capacity")]
    pub memory_sink_capacity: usize,

    /// Background flush interval in milliseconds. Default: 1000
    #[serde(default = "LoggingConfig::default_flush_interval_ms")]
    pub flush_interval_ms: u64,

    /// Enable JSON format for file sink. Default: true (console always pretty)
    #[serde(default = "LoggingConfig::default_json_file_format")]
    pub json_file_format: bool,

    /// Subsystem-level log level overrides. e.g. {"rendering": "warn"}
    #[serde(default)]
    pub level_overrides: HashMap<String, LogLevel>,
}

impl LoggingConfig {
    // Default value helpers
    fn default_level() -> LogLevel {
        LogLevel::Info
    }
    fn default_console_colors() -> bool {
        true
    }
    fn default_file_enabled() -> bool {
        true
    }
    fn default_max_file_size_bytes() -> u64 {
        50 * 1024 * 1024
    }
    fn default_max_rotated_files() -> u32 {
        10
    }
    fn default_compress_rotated() -> bool {
        true
    }
    fn default_retention_days() -> u32 {
        30
    }
    fn default_pipeline_channel_capacity() -> usize {
        65536
    }
    fn default_memory_sink_capacity() -> usize {
        10000
    }
    fn default_flush_interval_ms() -> u64 {
        1000
    }
    fn default_json_file_format() -> bool {
        true
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            console_colors: true,
            console_stream: ConsoleStream::Auto,
            file_enabled: true,
            file_path: None,
            max_file_size_bytes: 50 * 1024 * 1024,
            max_rotated_files: 10,
            compress_rotated: true,
            retention_days: 30,
            pipeline_channel_capacity: 65536,
            memory_sink_capacity: 10000,
            flush_interval_ms: 1000,
            json_file_format: true,
            level_overrides: HashMap::new(),
        }
    }
}
