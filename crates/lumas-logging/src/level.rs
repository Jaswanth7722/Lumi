//! # Log Level
//!
//! Log level in ascending severity order with atomically mutable global level
//! for lock-free runtime changes.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

/// Log level in ascending severity order.
/// Implements PartialOrd so level >= filter comparisons work naturally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Trace (most verbose).
    Trace = 0,
    /// Debug.
    Debug = 1,
    /// Info.
    Info = 2,
    /// Warn.
    Warn = 3,
    /// Error.
    Error = 4,
    /// Critical (least verbose, highest severity).
    Critical = 5,
}

impl LogLevel {
    /// Convert from tracing::Level.
    /// Maps tracing::Level::Error → Error; is_critical flag promotes to Critical.
    pub fn from_tracing(level: &tracing::Level, is_critical: bool) -> Self {
        if is_critical {
            return LogLevel::Critical;
        }
        match *level {
            tracing::Level::TRACE => LogLevel::Trace,
            tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::ERROR => LogLevel::Error,
        }
    }

    /// Convert to a tracing::Level for filter interop.
    pub fn to_tracing_level_filter(self) -> tracing::level_filters::LevelFilter {
        match self {
            LogLevel::Trace => tracing::level_filters::LevelFilter::TRACE,
            LogLevel::Debug => tracing::level_filters::LevelFilter::DEBUG,
            LogLevel::Info => tracing::level_filters::LevelFilter::INFO,
            LogLevel::Warn => tracing::level_filters::LevelFilter::WARN,
            LogLevel::Error => tracing::level_filters::LevelFilter::ERROR,
            LogLevel::Critical => tracing::level_filters::LevelFilter::ERROR,
        }
    }

    /// ANSI color code for console output.
    pub fn ansi_color(self) -> &'static str {
        match self {
            LogLevel::Trace => "\x1b[2m",       // dim
            LogLevel::Debug => "\x1b[34m",      // blue
            LogLevel::Info => "\x1b[32m",       // green
            LogLevel::Warn => "\x1b[33m",       // yellow
            LogLevel::Error => "\x1b[31m",      // red
            LogLevel::Critical => "\x1b[1;31m", // bold red
        }
    }

    /// ANSI reset code.
    pub fn ansi_reset() -> &'static str {
        "\x1b[0m"
    }

    /// Short 4-character label for columnar console output.
    pub fn short_label(self) -> &'static str {
        match self {
            LogLevel::Trace => "TRCE",
            LogLevel::Debug => "DBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERR ",
            LogLevel::Critical => "CRIT",
        }
    }

    /// Parse from string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "trace" | "trce" => Some(LogLevel::Trace),
            "debug" | "dbug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" => Some(LogLevel::Warn),
            "error" | "err" => Some(LogLevel::Error),
            "critical" | "crit" | "fatal" => Some(LogLevel::Critical),
            _ => None,
        }
    }
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Info
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "trace"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
            LogLevel::Critical => write!(f, "critical"),
        }
    }
}

/// Convenience alias for Arc-wrapped AtomicLogLevel.
pub type ArcLogLevel = AtomicLogLevel;

/// Atomically mutable global log level, changed at runtime without restart.
/// Stored as u8 via AtomicU8 for lock-free reads on the hot path.
pub struct AtomicLogLevel(AtomicU8);

impl AtomicLogLevel {
    /// Create a new atomic log level.
    pub const fn new(level: LogLevel) -> Self {
        Self(AtomicU8::new(level as u8))
    }

    /// Load the current level (lock-free).
    pub fn load(&self) -> LogLevel {
        match self.0.load(Ordering::Relaxed) {
            0 => LogLevel::Trace,
            1 => LogLevel::Debug,
            2 => LogLevel::Info,
            3 => LogLevel::Warn,
            4 => LogLevel::Error,
            5 => LogLevel::Critical,
            _ => LogLevel::Info,
        }
    }

    /// Store a new level (lock-free, immediate across all threads).
    pub fn store(&self, level: LogLevel) {
        self.0.store(level as u8, Ordering::Release);
    }

    /// Check if the given level is enabled.
    pub fn is_enabled(&self, level: LogLevel) -> bool {
        level >= self.load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_ordering() {
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
        assert!(LogLevel::Error < LogLevel::Critical);
    }

    #[test]
    fn test_atomic_log_level() {
        let level = AtomicLogLevel::new(LogLevel::Info);
        assert!(!level.is_enabled(LogLevel::Trace));
        assert!(!level.is_enabled(LogLevel::Debug));
        assert!(level.is_enabled(LogLevel::Info));
        assert!(level.is_enabled(LogLevel::Warn));
        assert!(level.is_enabled(LogLevel::Error));

        level.store(LogLevel::Debug);
        assert!(level.is_enabled(LogLevel::Trace)); // now trace is enabled
        assert!(level.is_enabled(LogLevel::Debug));
    }

    #[test]
    fn test_from_str() {
        assert_eq!(LogLevel::from_str("info"), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_str("ERROR"), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_str("critical"), Some(LogLevel::Critical));
        assert_eq!(LogLevel::from_str("invalid"), None);
    }

    #[test]
    fn test_short_labels() {
        assert_eq!(LogLevel::Trace.short_label(), "TRCE");
        assert_eq!(LogLevel::Info.short_label(), "INFO");
        assert_eq!(LogLevel::Critical.short_label(), "CRIT");
    }
}
