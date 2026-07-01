//! # Log Record
//!
//! The canonical data structure for a single log record.
//! Created by the tracing Layer, enriched with context, redacted,
//! then dispatched through the pipeline to all active sinks.

use crate::context::LogContext;
use crate::level::LogLevel;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::Serialize;
use uuid::Uuid;

/// The canonical data structure for a single log record.
#[derive(Debug, Clone, Serialize)]
pub struct LogRecord {
    /// Unique record identifier.
    pub id: Uuid,
    /// Timestamp of when the record was created.
    pub timestamp: DateTime<Utc>,
    /// Severity level.
    pub level: LogLevel,

    /// Source location (populated from tracing metadata).
    pub target: String,
    /// Module path.
    pub module_path: Option<String>,
    /// Source file.
    pub file: Option<String>,
    /// Source line number.
    pub line: Option<u32>,

    /// Process ID.
    pub process_id: u32,
    /// Thread ID.
    pub thread_id: u64,
    /// Thread name.
    pub thread_name: Option<String>,

    /// Correlation context.
    pub context: LogContext,

    /// Log message.
    pub message: String,
    /// Structured fields with deterministic key order.
    pub fields: IndexMap<String, FieldValue>,

    /// Span name if inside a tracing span.
    pub span_name: Option<String>,
    /// Span ID.
    pub span_id: Option<u64>,
    /// Duration in milliseconds (for span close events).
    pub duration_ms: Option<f64>,
    /// Error information if the event carries an error.
    pub error: Option<LogErrorInfo>,
}

impl LogRecord {
    /// Create a new log record with the given metadata.
    pub fn new(level: LogLevel, target: String, message: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            level,
            target,
            module_path: None,
            file: None,
            line: None,
            process_id: std::process::id(),
            thread_id: {
                // SAFETY: thread ID is stable for the lifetime of the thread
                #[allow(unsafe_code)]
                unsafe {
                    std::mem::transmute::<std::thread::ThreadId, u64>(std::thread::current().id())
                }
            },
            thread_name: std::thread::current().name().map(String::from),
            context: LogContext::default(),
            message,
            fields: IndexMap::new(),
            span_name: None,
            span_id: None,
            duration_ms: None,
            error: None,
        }
    }

    /// Set the context on this record.
    pub fn with_context(mut self, ctx: LogContext) -> Self {
        self.context = ctx;
        self
    }

    /// Add a field to the record.
    pub fn with_field(mut self, key: &str, value: FieldValue) -> Self {
        self.fields.insert(key.to_string(), value);
        self
    }

    /// Set the source location.
    pub fn with_source(
        mut self,
        module_path: Option<String>,
        file: Option<String>,
        line: Option<u32>,
    ) -> Self {
        self.module_path = module_path;
        self.file = file;
        self.line = line;
        self
    }
}

/// A log field value. Supports all JSON-representable types plus special
/// variants for redacted secrets and structured sub-objects.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum FieldValue {
    /// Boolean value.
    Bool(bool),
    /// Signed integer.
    Int(i64),
    /// Unsigned integer.
    Uint(u64),
    /// Floating-point value.
    Float(f64),
    /// String value.
    String(String),
    /// Redacted marker — serializes as "***REDACTED***".
    Redacted,
    /// Structured sub-object with deterministic key order.
    Object(IndexMap<String, FieldValue>),
    /// Array of values.
    Array(Vec<FieldValue>),
    /// Null value.
    Null,
}

impl FieldValue {
    /// Convert a serde_json::Value into a FieldValue.
    pub fn from_json(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => FieldValue::Null,
            serde_json::Value::Bool(b) => FieldValue::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    FieldValue::Int(i)
                } else if let Some(u) = n.as_u64() {
                    FieldValue::Uint(u)
                } else if let Some(f) = n.as_f64() {
                    FieldValue::Float(f)
                } else {
                    FieldValue::String(n.to_string())
                }
            }
            serde_json::Value::String(s) => FieldValue::String(s),
            serde_json::Value::Array(arr) => {
                FieldValue::Array(arr.into_iter().map(FieldValue::from_json).collect())
            }
            serde_json::Value::Object(obj) => {
                let mut map = IndexMap::new();
                for (k, v) in obj {
                    map.insert(k, FieldValue::from_json(v));
                }
                FieldValue::Object(map)
            }
        }
    }

    /// Try to get the value as a string slice.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            FieldValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

impl From<String> for FieldValue {
    fn from(s: String) -> Self {
        FieldValue::String(s)
    }
}

impl From<&str> for FieldValue {
    fn from(s: &str) -> Self {
        FieldValue::String(s.to_string())
    }
}

impl From<i64> for FieldValue {
    fn from(v: i64) -> Self {
        FieldValue::Int(v)
    }
}

impl From<u64> for FieldValue {
    fn from(v: u64) -> Self {
        FieldValue::Uint(v)
    }
}

impl From<f64> for FieldValue {
    fn from(v: f64) -> Self {
        FieldValue::Float(v)
    }
}

impl From<bool> for FieldValue {
    fn from(v: bool) -> Self {
        FieldValue::Bool(v)
    }
}

/// Error information carried in a log record.
#[derive(Debug, Clone, Serialize)]
pub struct LogErrorInfo {
    /// Error message.
    pub message: String,
    /// Error type name.
    pub kind: String,
    /// Source error chain.
    pub source_chain: Vec<String>,
}

impl LogErrorInfo {
    /// Create a new LogErrorInfo from a std::error::Error reference.
    pub fn from_error(error: &(dyn std::error::Error + 'static)) -> Self {
        let kind = std::any::type_name_of_val(error).to_string();
        let mut source_chain = Vec::new();
        let mut source = error.source();
        while let Some(s) = source {
            source_chain.push(s.to_string());
            source = s.source();
        }

        Self {
            message: error.to_string(),
            kind,
            source_chain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_record_creation() {
        let record = LogRecord::new(LogLevel::Info, "test_module".into(), "test message".into());
        assert_eq!(record.level, LogLevel::Info);
        assert_eq!(record.target, "test_module");
        assert_eq!(record.message, "test message");
        assert!(record.fields.is_empty());
    }

    #[test]
    fn test_field_value_from_json() {
        let json = serde_json::json!({
            "name": "test",
            "count": 42,
            "active": true
        });
        let fv = FieldValue::from_json(json);
        match fv {
            FieldValue::Object(map) => {
                assert_eq!(map.len(), 3);
                assert_eq!(map["name"].as_str(), Some("test"));
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_field_value_from_types() {
        let _: FieldValue = "hello".into();
        let _: FieldValue = 42i64.into();
        let _: FieldValue = 3.14f64.into();
        let _: FieldValue = true.into();
    }
}
