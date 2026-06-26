//! # Pretty Formatter
//!
//! Produces columnar, human-readable output for console and development.

use crate::error::LogError;
use crate::formatter::Formatter;
use crate::level::LogLevel;
use crate::record::{FieldValue, LogRecord};
use chrono::Timelike;

/// Human-readable console formatter with ANSI color support.
pub struct PrettyFormatter {
    /// Whether to use ANSI colors.
    use_colors: bool,
}

impl PrettyFormatter {
    /// Create a new pretty formatter.
    pub fn new(use_colors: bool) -> Self {
        Self { use_colors }
    }

    /// Create a formatter that auto-detects TTY.
    pub fn auto() -> Self {
        let use_colors =
            atty::is(atty::Stream::Stdout) && std::env::var("LUMI_LOG_NO_COLOR").is_err();
        Self { use_colors }
    }
}

impl Formatter for PrettyFormatter {
    fn name(&self) -> &'static str {
        "pretty"
    }

    fn format(&self, record: &LogRecord, buf: &mut Vec<u8>) -> Result<(), LogError> {
        buf.clear();
        use std::fmt::Write;

        // Timestamp: ISO 8601 with milliseconds (29 chars)
        let ts = record.timestamp.format("%Y-%m-%dT%H:%M:%S.%3fZ");
        write!(buf, "{}", ts).map_err(io_error)?;

        // Level: 5 chars, ANSI colored
        write!(buf, "  ").map_err(io_error)?;
        if self.use_colors {
            write!(buf, "{}", record.level.ansi_color()).map_err(io_error)?;
        }
        write!(buf, " {:5} ", record.level.short_label()).map_err(io_error)?;
        if self.use_colors {
            write!(buf, "{}", LogLevel::ansi_reset()).map_err(io_error)?;
        }

        // Subsystem: 12 chars, left-aligned
        let subsystem = record.context.subsystem.as_deref().unwrap_or("");
        write!(buf, " [{:<12}] ", subsystem).map_err(io_error)?;

        // Message: variable width
        write!(buf, "{}", record.message).map_err(io_error)?;

        // Fields: key=value pairs
        for (key, value) in &record.fields {
            write!(buf, "  {key}=").map_err(io_error)?;
            format_field_value(value, buf);
        }

        // Error info
        if let Some(ref error) = record.error {
            write!(buf, "  error=\"{}\"", error.message).map_err(io_error)?;
        }

        // Newline
        writeln!(buf).map_err(io_error)?;

        Ok(())
    }
}

fn format_field_value(value: &FieldValue, buf: &mut String) {
    match value {
        FieldValue::String(s) => {
            if s.contains(' ') || s.contains('=') {
                write!(buf, "\"{s}\"").unwrap();
            } else {
                write!(buf, "{s}").unwrap();
            }
        }
        FieldValue::Int(i) => write!(buf, "{i}").unwrap(),
        FieldValue::Uint(u) => write!(buf, "{u}").unwrap(),
        FieldValue::Float(f) => write!(buf, "{f}").unwrap(),
        FieldValue::Bool(b) => write!(buf, "{b}").unwrap(),
        FieldValue::Redacted => write!(buf, "***REDACTED***").unwrap(),
        FieldValue::Null => write!(buf, "null").unwrap(),
        FieldValue::Array(arr) => {
            write!(buf, "[").unwrap();
            for (i, v) in arr.iter().enumerate() {
                if i > 0 {
                    write!(buf, ",").unwrap();
                }
                format_field_value(v, buf);
            }
            write!(buf, "]").unwrap();
        }
        FieldValue::Object(map) => {
            write!(buf, "{{").unwrap();
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    write!(buf, ",").unwrap();
                }
                write!(buf, "{k}:").unwrap();
                format_field_value(v, buf);
            }
            write!(buf, "}}").unwrap();
        }
    }
}

fn io_error(_: std::fmt::Error) -> LogError {
    // fmt::Write on Vec<u8> is infallible
    LogError::FormatError {
        formatter: "pretty".into(),
        reason: "write error".into(),
    }
}

/// Helper to write formatted content to Vec<u8> using Write trait.
use std::fmt::Write;

impl Write for Vec<u8> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::LogContext;
    use crate::level::LogLevel;
    use crate::record::LogRecord;
    use uuid::Uuid;

    #[test]
    fn test_pretty_format_contains_level_and_message() {
        let formatter = PrettyFormatter::new(false);
        let mut record = LogRecord::new(LogLevel::Info, "test".into(), "hello world".into());
        record.context = LogContext::builder()
            .with_correlation_id(Uuid::new_v4())
            .build();

        let mut buf = Vec::new();
        formatter.format(&record, &mut buf).unwrap();

        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("INFO"));
        assert!(output.contains("hello world"));
    }

    #[test]
    fn test_pretty_format_includes_timestamp() {
        let formatter = PrettyFormatter::new(false);
        let record = LogRecord::new(LogLevel::Warn, "test".into(), "test".into());

        let mut buf = Vec::new();
        formatter.format(&record, &mut buf).unwrap();

        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains('T')); // ISO timestamp char
        assert!(output.contains('Z')); // UTC marker
    }
}
