//! # Error Formatting
//!
//! Provides three named formats for error output:
//! - `Compact` — single line: `[LUMI-AI-0103] Inference provider unreachable (recoverable)`
//! - `Detailed` — multi-line with context, cause chain, recovery hint
//! - `Json` — fully structured JSON (uses `ErrorReport`)
//!
//! Each format works for both UserFacing (default) and Diagnostic output modes.

use crate::error::LumiError;
use crate::report::{ErrorReport, ReportFormat};

/// Output mode determines which fields are included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// User-facing output (sanitized, safe for UI display).
    UserFacing,
    /// Diagnostic output (full detail, for logs and crash reports).
    Diagnostic,
}

/// Format variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatKind {
    /// Single-line compact format.
    Compact,
    /// Multi-line detailed format.
    Detailed,
    /// Structured JSON format.
    Json,
}

/// Error formatter with three named formats and two output modes.
#[derive(Debug, Clone)]
pub struct ErrorFormatter {
    /// The format kind.
    kind: FormatKind,
    /// The output mode.
    mode: OutputMode,
}

impl ErrorFormatter {
    /// Create a new formatter with the given kind and mode.
    pub fn new(kind: FormatKind, mode: OutputMode) -> Self {
        Self { kind, mode }
    }

    /// Create a compact user-facing formatter.
    pub fn compact_user() -> Self {
        Self {
            kind: FormatKind::Compact,
            mode: OutputMode::UserFacing,
        }
    }

    /// Create a detailed diagnostic formatter.
    pub fn detailed_diagnostic() -> Self {
        Self {
            kind: FormatKind::Detailed,
            mode: OutputMode::Diagnostic,
        }
    }

    /// Create a JSON formatter.
    pub fn json(format: ReportFormat) -> Self {
        let mode = match format {
            ReportFormat::UserFacing => OutputMode::UserFacing,
            _ => OutputMode::Diagnostic,
        };
        Self {
            kind: FormatKind::Json,
            mode,
        }
    }

    /// Format a LumiError into a string.
    ///
    /// # Arguments
    /// * `error` - The error to format.
    ///
    /// # Returns
    /// A formatted string.
    pub fn format(&self, error: &LumiError) -> String {
        match self.kind {
            FormatKind::Compact => self.format_compact(error),
            FormatKind::Detailed => self.format_detailed(error),
            FormatKind::Json => self.format_json(error),
        }
    }

    /// Compact format: single line.
    fn format_compact(&self, error: &LumiError) -> String {
        let code = error.code().format(error.category());
        match self.mode {
            OutputMode::UserFacing => {
                format!("[{}] {}", code, error.user_message())
            }
            OutputMode::Diagnostic => {
                format!(
                    "[{}] {} ({})",
                    code,
                    error.diagnostic_message(),
                    error.severity()
                )
            }
        }
    }

    /// Detailed format: multi-line with full context.
    fn format_detailed(&self, error: &LumiError) -> String {
        let code = error.code().format(error.category());
        let severity = error.severity();
        let category = error.category();
        let location = &error.context().location;
        let cause_chain = &error.context().cause_chain;

        match self.mode {
            OutputMode::UserFacing => {
                format!(
                    "[{code}] {category}\nLocation: {location}\n{msg}\nSeverity: {severity}",
                    code = code,
                    category = category,
                    location = location,
                    msg = error.user_message(),
                    severity = severity,
                )
            }
            OutputMode::Diagnostic => {
                let mut output = format!(
                    "[{code}] {category}\nLocation: {location}\nThread: {thread:?}\nSeverity: {severity}\nMessage: {msg}",
                    code = code,
                    category = category,
                    location = location,
                    thread = error.context().thread,
                    severity = severity,
                    msg = error.diagnostic_message(),
                );

                if !cause_chain.is_empty() {
                    output.push_str(&format!("\nCause chain:\n{}", cause_chain));
                }

                let recovery = error.recovery();
                match recovery {
                    crate::error::RecoveryHint::None => {}
                    crate::error::RecoveryHint::Suggestion(s) => {
                        output.push_str(&format!("\nRecovery suggestion: {}", s));
                    }
                    crate::error::RecoveryHint::Strategy(s) => {
                        output.push_str(&format!("\nRecovery strategy: {:?}", s));
                    }
                }

                // Include source chain
                if let Some(source) = error.source() {
                    output.push_str(&format!("\nSource: {}", source));
                }

                output
            }
        }
    }

    /// JSON format: structured JSON.
    fn format_json(&self, error: &LumiError) -> String {
        let report_format = match self.mode {
            OutputMode::UserFacing => ReportFormat::UserFacing,
            OutputMode::Diagnostic => ReportFormat::LogLine,
        };

        let report = ErrorReport::from_error(error, report_format);

        match self.mode {
            OutputMode::UserFacing => report
                .to_user_json()
                .unwrap_or_else(|_| format!("{{\"error\": \"serialization failed\"}}")),
            OutputMode::Diagnostic => report
                .to_log_line()
                .unwrap_or_else(|_| format!("{{\"error\": \"serialization failed\"}}")),
        }
    }

    /// Get the format kind.
    pub fn kind(&self) -> FormatKind {
        self.kind
    }

    /// Get the output mode.
    pub fn mode(&self) -> OutputMode {
        self.mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::ErrorCategory;
    use crate::error::LumiError;
    use crate::error_code::ErrorCode;

    fn make_test_error() -> LumiError {
        LumiError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore {
                provider: Some("anthropic".into()),
            },
            "Inference request failed",
        )
    }

    #[test]
    fn test_compact_format() {
        let error = make_test_error();
        let formatter = ErrorFormatter::compact_user();
        let output = formatter.format(&error);
        assert!(output.starts_with("[LUMI-AI-0401]"));
        assert!(output.contains("Inference request failed"));
    }

    #[test]
    fn test_compact_diagnostic() {
        let error = make_test_error();
        let formatter = ErrorFormatter::new(FormatKind::Compact, OutputMode::Diagnostic);
        let output = formatter.format(&error);
        assert!(output.contains("recoverable"));
    }

    #[test]
    fn test_detailed_format() {
        let error = make_test_error();
        let formatter = ErrorFormatter::detailed_diagnostic();
        let output = formatter.format(&error);
        assert!(output.contains("AI Core"));
        assert!(output.contains("Inference request failed"));
    }

    #[test]
    fn test_json_format() {
        let error = make_test_error();
        let formatter = ErrorFormatter::json(ReportFormat::LogLine);
        let output = formatter.format(&error);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["user_message"], "Inference request failed");
        assert!(parsed.get("diagnostic").is_some());
    }

    #[test]
    fn test_json_user_facing_no_diagnostics() {
        let error = make_test_error();
        let formatter = ErrorFormatter::json(ReportFormat::UserFacing);
        let output = formatter.format(&error);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["user_message"], "Inference request failed");
        assert!(parsed.get("diagnostic").is_none());
    }
}
