//! # Logging and Telemetry — Structured Logging and Audit (Chapter 28)
//!
//! Defines structured log entries, audit log for security-sensitive operations,
//! and opt-in telemetry collection.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Structured Logging
// ---------------------------------------------------------------------------

/// Log severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// A structured log entry written by any Lumi process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub process: String,
    pub module: String,
    pub message: String,
    pub fields: HashMap<String, String>,
    pub trace_id: Option<String>,
}

impl LogEntry {
    /// Create a new log entry with ISO 8601 timestamp.
    pub fn new(level: LogLevel, process: &str, module: &str, message: &str) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level,
            process: process.to_string(),
            module: module.to_string(),
            message: message.to_string(),
            fields: HashMap::new(),
            trace_id: None,
        }
    }

    /// Add a structured field to the log entry.
    pub fn with_field(mut self, key: &str, value: &str) -> Self {
        self.fields.insert(key.to_string(), value.to_string());
        self
    }

    /// Set the trace ID for distributed tracing.
    pub fn with_trace(mut self, trace_id: &str) -> Self {
        self.trace_id = Some(trace_id.to_string());
        self
    }
}

// ---------------------------------------------------------------------------
// Audit Log
// ---------------------------------------------------------------------------

/// Types of security-sensitive events logged to the audit log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    ToolExecuted,
    ToolDenied,
    MemoryWritten,
    MemoryDeleted,
    ScreenCaptured,
    ClipboardRead,
    PluginInstalled,
    PluginUninstalled,
    APIKeyAccessed,
}

/// Outcome of an audited operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditOutcome {
    Success,
    Denied,
    Failed,
}

/// A single entry in the audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub event_type: AuditEventType,
    pub tool_name: Option<String>,
    pub tool_input_summary: Option<String>,
    pub outcome: AuditOutcome,
    pub user_approved: Option<bool>,
}

impl AuditEntry {
    /// Create a new audit entry for a tool execution.
    pub fn tool_executed(tool_name: &str, outcome: AuditOutcome, approved: Option<bool>) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event_type: AuditEventType::ToolExecuted,
            tool_name: Some(tool_name.to_string()),
            tool_input_summary: None,
            outcome,
            user_approved: approved,
        }
    }

    /// Create a new audit entry for a memory deletion.
    pub fn memory_deleted(count: u64) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event_type: AuditEventType::MemoryDeleted,
            tool_name: None,
            tool_input_summary: Some(format!("{count} memories deleted")),
            outcome: AuditOutcome::Success,
            user_approved: Some(true),
        }
    }
}

// ---------------------------------------------------------------------------
// Telemetry
// ---------------------------------------------------------------------------

/// Opt-in telemetry event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TelemetryEvent {
    SessionStarted { duration_est: u64 },
    SessionEnded { duration_secs: u64 },
    FeatureUsed { feature: String },
    ToolResult { tool: String, success: bool },
    Performance { metric: String, value: f64 },
    Crash { stack_hash: String },
}

/// Telemetry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub session_id: String,
    pub install_id: String,
}

impl TelemetryConfig {
    pub fn new() -> Self {
        Self {
            enabled: false,
            session_id: uuid::Uuid::new_v4().to_string(),
            install_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry::new(LogLevel::Info, "core", "state_machine", "State initialized");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.process, "core");
        assert!(entry.timestamp.contains('T')); // ISO 8601
    }

    #[test]
    fn test_log_entry_with_fields() {
        let entry = LogEntry::new(LogLevel::Warn, "render", "animation", "Clip not found")
            .with_field("clip_id", "idle_breathe")
            .with_trace("trace-123");
        assert_eq!(entry.fields.get("clip_id").unwrap(), "idle_breathe");
        assert_eq!(entry.trace_id.unwrap(), "trace-123");
    }

    #[test]
    fn test_audit_entry_tool() {
        let entry = AuditEntry::tool_executed("fs.delete_file", AuditOutcome::Denied, Some(false));
        assert_eq!(entry.event_type, AuditEventType::ToolExecuted);
        assert_eq!(entry.outcome, AuditOutcome::Denied);
        assert_eq!(entry.user_approved, Some(false));
    }

    #[test]
    fn test_audit_entry_memory() {
        let entry = AuditEntry::memory_deleted(42);
        assert_eq!(entry.event_type, AuditEventType::MemoryDeleted);
        assert_eq!(entry.outcome, AuditOutcome::Success);
    }

    #[test]
    fn test_telemetry_config() {
        let config = TelemetryConfig::new();
        assert!(!config.enabled);
        assert!(!config.session_id.is_empty());
        assert!(!config.install_id.is_empty());
    }
}
