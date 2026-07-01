//! # JSON Formatter
//!
//! Produces one JSON object per line (NDJSON format).
//! Field ordering is deterministic (not alphabetical — follows insertion order).

use crate::error::LogError;
use crate::formatter::Formatter;
use crate::record::LogRecord;
use serde::Serialize;

/// JSON formatter producing NDJSON output.
pub struct JsonFormatter;

impl JsonFormatter {
    /// Create a new JSON formatter.
    pub fn new() -> Self {
        Self
    }
}

impl Formatter for JsonFormatter {
    fn name(&self) -> &'static str {
        "json"
    }

    fn format(&self, record: &LogRecord, buf: &mut Vec<u8>) -> Result<(), LogError> {
        buf.clear();

        serde_json::to_writer(buf as &mut dyn std::io::Write, record).map_err(|e| {
            LogError::FormatError {
                formatter: "json".into(),
                reason: e.to_string(),
            }
        })?;

        // Add newline for NDJSON format
        buf.push(b'\n');

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::LogLevel;
    use crate::record::LogRecord;

    #[test]
    fn test_json_format_is_valid() {
        let formatter = JsonFormatter::new();
        let record = LogRecord::new(LogLevel::Info, "test".into(), "hello".into());

        let mut buf = Vec::new();
        formatter.format(&record, &mut buf).unwrap();

        let output = String::from_utf8_lossy(&buf);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["level"], "info");
        assert_eq!(parsed["message"], "hello");
        assert_eq!(parsed["target"], "test");
    }

    #[test]
    fn test_json_format_ends_with_newline() {
        let formatter = JsonFormatter::new();
        let record = LogRecord::new(LogLevel::Info, "test".into(), "test".into());

        let mut buf = Vec::new();
        formatter.format(&record, &mut buf).unwrap();

        assert_eq!(buf.last(), Some(&b'\n'));
    }
}
