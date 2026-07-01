//! # Error Report
//!
//! Provides the serializable, exportable representation of a `LumasError`.
//! Supports three serialization targets:
//! - User-facing JSON (safe for UI, no diagnostics)
//! - Log line (structured JSON line for log files, full detail)
//! - Crash report section (embedded in CrashReport, maximum detail)
//!
//! # Security
//! User-facing output is guaranteed to never contain stack traces,
//! source code locations, or any diagnostic detail.

use crate::error::LumasError;
use crate::error_code::ErrorCode;
use crate::severity::Severity;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Serialization mode for error reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    /// User-facing: sanitized, no diagnostics, safe for UI display.
    UserFacing,
    /// Log line: structured JSON for log files, full diagnostic detail.
    LogLine,
    /// Crash report: maximum detail, embedded in CrashReport.
    CrashReport,
}

/// A serializable error report.
///
/// The `diagnostic` field is automatically omitted from user-facing output
/// via `#[serde(skip)]` logic in the serialization methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorReport {
    /// Formatted error code (e.g., "LUMI-AI-0103").
    pub error_code: String,
    /// Severity level.
    pub severity: Severity,
    /// Error category name.
    pub category: String,
    /// User-facing message (safe for display).
    pub user_message: String,
    /// Diagnostic message (full detail).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Session ID.
    pub session_id: String,
    /// Correlation ID.
    pub correlation_id: String,
    /// Error context details (file, line, subsystem).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Stack trace (omitted from user-facing output).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_trace: Option<String>,
    /// Recovery hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_hint: Option<String>,
    /// Duration in ms if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    /// Additional context fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<HashMap<String, String>>,
}

impl ErrorReport {
    /// Create a new error report from a LumasError.
    pub fn from_error(error: &LumasError, format: ReportFormat) -> Self {
        let session_id = String::new(); // Would come from runtime context
        let correlation_id = String::new(); // Would come from runtime context

        let (diagnostic, location, stack_trace, recovery_hint) = match format {
            ReportFormat::UserFacing => (None, None, None, None),
            ReportFormat::LogLine => (
                Some(error.diagnostic_message().to_string()),
                Some(error.context().location.to_string()),
                None,
                Some(format!("{:?}", error.recovery())),
            ),
            ReportFormat::CrashReport => (
                Some(error.diagnostic_message().to_string()),
                Some(error.context().location.to_string()),
                Some(error.context().cause_chain.to_string()),
                Some(format!("{:?}", error.recovery())),
            ),
        };

        Self {
            error_code: error.code().format(error.category()),
            severity: error.severity(),
            category: error.category().display_name().to_string(),
            user_message: error.user_message().to_string(),
            diagnostic,
            timestamp: Utc::now().to_rfc3339(),
            session_id,
            correlation_id,
            location,
            stack_trace,
            recovery_hint,
            duration_ms: None,
            fields: None,
        }
    }

    /// Serialize to user-safe JSON (no diagnostics, no stack traces).
    ///
    /// # Errors
    /// Returns a serialization error if JSON serialization fails.
    pub fn to_user_json(&self) -> Result<String, serde_json::Error> {
        // Create a stripped-down version for user output
        let user = serde_json::json!({
            "error_code": self.error_code,
            "severity": self.severity.to_string(),
            "category": self.category,
            "user_message": self.user_message,
            "timestamp": self.timestamp,
        });
        serde_json::to_string(&user)
    }

    /// Serialize to a structured JSON log line (full detail).
    ///
    /// # Errors
    /// Returns a serialization error if JSON serialization fails.
    pub fn to_log_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to a crash report section (maximum detail).
    ///
    /// # Errors
    /// Returns a serialization error if JSON serialization fails.
    pub fn to_crash_report_section(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

/// A collection of error reports for batch export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorReportBatch {
    /// The reports.
    pub reports: Vec<ErrorReport>,
    /// Timestamp of the export.
    pub exported_at: String,
    /// Total count.
    pub count: usize,
}

/// Determine whether diagnostic info should be omitted in user-facing output.
#[doc(hidden)]
pub fn should_omit_diagnostic(diagnostic: &Option<String>) -> bool {
    diagnostic.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::ErrorCategory;
    use crate::error::LumasError;
    use crate::error_code::ErrorCode;

    fn make_test_error() -> LumasError {
        LumasError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "Inference request failed",
        )
    }

    #[test]
    fn test_user_facing_no_diagnostics() {
        let error = make_test_error();
        let report = ErrorReport::from_error(&error, ReportFormat::UserFacing);

        assert!(report.diagnostic.is_none());
        assert!(report.location.is_none());
        assert!(report.stack_trace.is_none());
        assert_eq!(report.user_message, "Inference request failed");
    }

    #[test]
    fn test_log_line_has_diagnostics() {
        let error = make_test_error();
        let report = ErrorReport::from_error(&error, ReportFormat::LogLine);

        assert!(report.diagnostic.is_some());
        assert!(report.location.is_some());
    }

    #[test]
    fn test_user_json_no_secrets() {
        let error = make_test_error();
        let report = ErrorReport::from_error(&error, ReportFormat::UserFacing);
        let json = report.to_user_json().unwrap();

        // Should not contain diagnostic fields
        assert!(!json.contains("diagnostic"));
        assert!(!json.contains("stack_trace"));
        assert!(!json.contains("location"));
    }

    #[test]
    fn test_log_line_valid_json() {
        let error = make_test_error();
        let report = ErrorReport::from_error(&error, ReportFormat::LogLine);
        let json = report.to_log_line().unwrap();

        // Verify it's parseable JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["user_message"], "Inference request failed");
    }

    #[test]
    fn test_crash_report_section() {
        let error = make_test_error();
        let report = ErrorReport::from_error(&error, ReportFormat::CrashReport);
        let section = report.to_crash_report_section().unwrap();

        assert_eq!(section["user_message"], "Inference request failed");
        assert!(section.get("diagnostic").is_some());
    }
}
