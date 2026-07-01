//! Integration tests for error serialization round-trips.

use lumas_error::category::ErrorCategory;
use lumas_error::crash::CrashReport;
use lumas_error::error::LumasError;
use lumas_error::error_code::ErrorCode;
use lumas_error::prelude::*;
use lumas_error::report::{ErrorReport, ErrorReportBatch, ReportFormat};

#[tokio::test]
async fn test_error_report_to_json() {
    let err = LumasError::new(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "Inference failed",
    );

    let report = ErrorReport::from_error(&err, ReportFormat::LogLine);
    let json = report.to_log_line().unwrap();

    // Verify it's valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["user_message"], "Inference failed");
    assert_eq!(parsed["severity"], "info");
}

#[tokio::test]
async fn test_error_report_user_json() {
    let err = LumasError::new(
        ErrorCode::CONFIG_FILE_NOT_FOUND,
        ErrorCategory::Configuration { field: None },
        "Config file missing",
    );

    let report = ErrorReport::from_error(&err, ReportFormat::UserFacing);
    let json = report.to_user_json().unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["user_message"], "Config file missing");
    // User-facing should not have diagnostic fields
    assert!(parsed.get("diagnostic").is_none());
}

#[tokio::test]
async fn test_report_batch_serialization() {
    let err1 = LumasError::new(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "error 1",
    );
    let err2 = LumasError::new(
        ErrorCode::CONFIG_FILE_NOT_FOUND,
        ErrorCategory::Configuration { field: None },
        "error 2",
    );

    let reports = vec![
        ErrorReport::from_error(&err1, ReportFormat::LogLine),
        ErrorReport::from_error(&err2, ReportFormat::LogLine),
    ];

    let batch = ErrorReportBatch {
        reports,
        exported_at: chrono::Utc::now().to_rfc3339(),
        count: 2,
    };

    let json = serde_json::to_string(&batch).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["count"], 2);
    assert_eq!(parsed["reports"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_crash_report_serialization() {
    let report = CrashReport::new(lumas_error::crash::CrashType::FatalError);
    let json = serde_json::to_string(&report).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["crash_type"], "fatal_error");
    assert!(parsed.get("id").is_some());
    assert!(parsed.get("timestamp").is_some());
}

#[tokio::test]
async fn test_crash_report_with_metadata_serialization() {
    let report = CrashReport::new(lumas_error::crash::CrashType::Panic {
        message: "test panic".into(),
    });
    let json = serde_json::to_string(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["crash_type"], "panic");
}

#[tokio::test]
async fn test_metrics_snapshot_serialization() {
    let metrics = lumas_error::metrics::ErrorMetrics::new();
    metrics.record_error(&ErrorCategory::Runtime, Severity::Warning);
    let snapshot = metrics.snapshot();
    let json = serde_json::to_string(&snapshot).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["total_errors"], 1);
}

#[tokio::test]
async fn test_diagnostic_report_serialization() {
    let history = lumas_error::diagnostics::ErrorHistory::new(100);
    let err = LumasError::new(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "test",
    );
    history.record(&err);

    let report = lumas_error::diagnostics::generate_diagnostic_report(&history);
    let json = serde_json::to_string(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["total_errors"], 1);
}
