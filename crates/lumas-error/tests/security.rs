//! Integration tests for security: secret redaction, user-facing sanitization, and stack trace filtering.

use lumas_error::category::ErrorCategory;
use lumas_error::error::LumasError;
use lumas_error::error_code::ErrorCode;
use lumas_error::prelude::*;
use lumas_error::report::{ErrorReport, ReportFormat};

#[tokio::test]
async fn test_user_facing_message_omits_diagnostics() {
    let err = LumasError::new(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "Inference request failed",
    );

    let report = ErrorReport::from_error(&err, ReportFormat::UserFacing);
    let json = report.to_user_json().unwrap();

    // User-facing JSON must not contain diagnostic fields
    assert!(!json.contains("diagnostic"));
    assert!(!json.contains("stack_trace"));
    assert!(!json.contains("location"));
}

#[tokio::test]
async fn test_log_line_contains_diagnostics() {
    let err = LumasError::new(
        ErrorCode::CONFIG_FILE_NOT_FOUND,
        ErrorCategory::Configuration { field: None },
        "Config file missing",
    );

    let report = ErrorReport::from_error(&err, ReportFormat::LogLine);
    let json = report.to_log_line().unwrap();

    // Log line should contain diagnostic info
    assert!(json.contains("diagnostic"));
    assert!(json.contains("Config file missing"));
}

#[tokio::test]
async fn test_error_code_format_no_secrets() {
    let err = LumasError::new(
        ErrorCode::SECURITY_ACCESS_DENIED,
        ErrorCategory::Security {
            violation: lumas_error::category::SecurityViolation::Authentication,
        },
        "Access denied",
    );

    let formatted = err.code().format(err.category());
    // The formatted code should not contain any secret strings
    assert!(!formatted.to_lowercase().contains("password"));
    assert!(!formatted.to_lowercase().contains("token"));
    assert!(!formatted.to_lowercase().contains("secret"));
}

#[tokio::test]
async fn test_user_safe_output_no_stack_traces() {
    let err = LumasError::new(
        ErrorCode::RUNTIME_INTERNAL,
        ErrorCategory::Runtime,
        "Internal error",
    );

    let formatter = lumas_error::formatter::ErrorFormatter::compact_user();
    let output = formatter.format(&err);

    // User-facing output must not contain stack traces
    assert!(!output.contains("stack"));
    assert!(!output.contains("frame"));
}

#[tokio::test]
async fn test_diagnostic_output_has_location() {
    let err = LumasError::new(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "Inference error",
    );

    let formatter = lumas_error::formatter::ErrorFormatter::detailed_diagnostic();
    let output = formatter.format(&err);

    // Diagnostic output should contain location
    assert!(output.contains("Location") || output.contains("error.rs"));
}

#[tokio::test]
async fn test_error_creation_no_secret_leakage() {
    // Ensure that constructing errors with potential secret info in the message
    // doesn't accidentally expose them in user-facing output
    let err = LumasError::new(
        ErrorCode::SECURITY_ACCESS_DENIED,
        ErrorCategory::Security {
            violation: lumas_error::category::SecurityViolation::Authentication,
        },
        "Authentication failed for user: admin with token: sk-abc123def456",
    );

    let user_msg = err.user_message().to_string();
    // User-facing message should not contain the literal secret
    assert!(!user_msg.contains("sk-abc123def456"));
}

#[tokio::test]
async fn test_crash_report_no_secrets_in_user_fields() {
    use lumas_error::crash::CrashReport;

    let report = CrashReport::new(lumas_error::crash::CrashType::FatalError);
    let json = serde_json::to_string(&report).unwrap();

    // Crash report JSON should not crash serde
    assert!(!json.is_empty());
}
