//! Integration tests for diagnostics: history queries, pattern detection, and reports.

use lumas_error::category::ErrorCategory;
use lumas_error::diagnostics::*;
use lumas_error::error::LumasError;
use lumas_error::error_code::ErrorCode;
use lumas_error::prelude::*;

fn make_error(code: ErrorCode, cat: ErrorCategory, msg: &str) -> LumasError {
    LumasError::new(code, cat, msg)
}

#[tokio::test]
async fn test_history_record_and_recent() {
    let history = ErrorHistory::new(100);
    let err = make_error(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "test error",
    );
    history.record(&err);

    let recent = history.recent(10);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].code, ErrorCode::AI_INFERENCE_FAILED);
}

#[tokio::test]
async fn test_history_query_by_level() {
    let history = ErrorHistory::new(100);
    history.record(&make_error(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "info-level",
    ));
    history.record(
        &make_error(
            ErrorCode::RUNTIME_INTERNAL,
            ErrorCategory::Runtime,
            "critical error",
        )
        .with_severity(Severity::Critical),
    );

    let query = ErrorQuery::new()
        .with_min_severity(Severity::Critical)
        .with_max_results(10);
    let results = history.query(&query);
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn test_history_query_by_correlation_id() {
    let history = ErrorHistory::new(100);
    history.record(&make_error(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "test",
    ));

    let query = ErrorQuery::new()
        .with_correlation_id("nonexistent")
        .with_max_results(10);
    let results = history.query(&query);
    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn test_history_bounded_capacity() {
    let history = ErrorHistory::new(5);
    for i in 0..10 {
        let err = make_error(
            ErrorCode::INTERNAL_UNEXPECTED,
            ErrorCategory::Internal,
            &format!("error {}", i),
        );
        history.record(&err);
    }
    assert_eq!(history.len(), 5);
}

#[tokio::test]
async fn test_pattern_detection() {
    let history = ErrorHistory::new(100);
    let err = make_error(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "repeated",
    );

    // Record the same error multiple times rapidly
    for _ in 0..6 {
        history.record(&err);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    let patterns = history.analyze_failure_patterns();
    assert!(!patterns.is_empty(), "Should detect at least one pattern");
}

#[tokio::test]
async fn test_diagnostic_report_generation() {
    let history = ErrorHistory::new(100);
    history.record(&make_error(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "AI error",
    ));
    history.record(&make_error(
        ErrorCode::CONFIG_FILE_NOT_FOUND,
        ErrorCategory::Configuration { field: None },
        "config error",
    ));

    let report = generate_diagnostic_report(&history);
    assert_eq!(report.total_errors, 2);
    assert!(report.errors_by_category.contains_key("AI Core"));
}

#[tokio::test]
async fn test_search_by_text() {
    let history = ErrorHistory::new(100);
    history.record(&make_error(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "Provider was unreachable",
    ));

    let query = ErrorQuery::new()
        .with_search_text("unreachable")
        .with_max_results(10);
    let results = history.query(&query);
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn test_clear_history() {
    let history = ErrorHistory::new(100);
    history.record(&make_error(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "test",
    ));
    assert_eq!(history.len(), 1);
    history.clear();
    assert_eq!(history.len(), 0);
}
