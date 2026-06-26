//! # Severity Level
//!
//! Typed severity enum with ordering, recovery guidance, and tracing interop.

/// Error severity in ascending order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Trace-level diagnostic (most verbose).
    Trace = 0,
    /// Debug-level diagnostic.
    Debug = 1,
    /// Informational message.
    Info = 2,
    /// Warning — non-critical issue.
    Warning = 3,
    /// Recoverable error — system can continue with degraded functionality.
    Recoverable = 4,
    /// Critical error — requires immediate attention but system may survive.
    Critical = 5,
    /// Fatal error — system cannot continue. Requires process restart.
    Fatal = 6,
}

impl Severity {
    /// True if this severity represents an actionable condition (Warning and above).
    pub fn is_actionable(&self) -> bool {
        *self >= Severity::Warning
    }

    /// True if this severity is recoverable (Recoverable and below).
    /// Critical and Fatal are NOT recoverable.
    pub fn is_recoverable(&self) -> bool {
        *self <= Severity::Recoverable
    }

    /// True if this severity requires generating a crash report.
    pub fn requires_crash_report(&self) -> bool {
        *self >= Severity::Critical
    }

    /// Convert to a tracing::Level.
    pub fn to_tracing_level(&self) -> tracing::Level {
        match self {
            Severity::Trace => tracing::Level::TRACE,
            Severity::Debug => tracing::Level::DEBUG,
            Severity::Info => tracing::Level::INFO,
            Severity::Warning => tracing::Level::WARN,
            Severity::Recoverable => tracing::Level::ERROR,
            Severity::Critical => tracing::Level::ERROR,
            Severity::Fatal => tracing::Level::ERROR,
        }
    }
}

impl Default for Severity {
    fn default() -> Self {
        Severity::Info
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Trace => write!(f, "trace"),
            Severity::Debug => write!(f, "debug"),
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Recoverable => write!(f, "recoverable"),
            Severity::Critical => write!(f, "critical"),
            Severity::Fatal => write!(f, "fatal"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Trace < Severity::Debug);
        assert!(Severity::Debug < Severity::Info);
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Recoverable);
        assert!(Severity::Recoverable < Severity::Critical);
        assert!(Severity::Critical < Severity::Fatal);
    }

    #[test]
    fn test_actionable() {
        assert!(!Severity::Debug.is_actionable());
        assert!(Severity::Warning.is_actionable());
        assert!(Severity::Fatal.is_actionable());
    }

    #[test]
    fn test_recoverable() {
        assert!(Severity::Trace.is_recoverable());
        assert!(Severity::Warning.is_recoverable());
        assert!(Severity::Recoverable.is_recoverable());
        assert!(!Severity::Critical.is_recoverable());
        assert!(!Severity::Fatal.is_recoverable());
    }

    #[test]
    fn test_crash_report() {
        assert!(!Severity::Info.requires_crash_report());
        assert!(!Severity::Recoverable.requires_crash_report());
        assert!(Severity::Critical.requires_crash_report());
        assert!(Severity::Fatal.requires_crash_report());
    }
}
