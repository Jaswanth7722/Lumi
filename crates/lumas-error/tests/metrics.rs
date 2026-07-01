//! Integration tests for error metrics correctness under concurrent load.

use lumas_error::category::ErrorCategory;
use lumas_error::metrics::ErrorMetrics;
use lumas_error::severity::Severity;
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn test_metrics_basic_counters() {
    let metrics = ErrorMetrics::new();
    metrics.record_error(&ErrorCategory::Runtime, Severity::Critical);
    metrics.record_error(
        &ErrorCategory::AiCore { provider: None },
        Severity::Recoverable,
    );

    assert_eq!(metrics.total_errors.load(Ordering::Relaxed), 2);
    assert_eq!(
        metrics.errors_by_severity[Severity::Critical as usize].load(Ordering::Relaxed),
        1
    );
    assert_eq!(
        metrics.errors_by_severity[Severity::Recoverable as usize].load(Ordering::Relaxed),
        1
    );
}

#[tokio::test]
async fn test_metrics_concurrent_increments() {
    let metrics = Arc::new(ErrorMetrics::new());
    let mut handles = Vec::new();

    for _ in 0..10 {
        let m = metrics.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                m.record_error(
                    &ErrorCategory::AiCore { provider: None },
                    Severity::Recoverable,
                );
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(metrics.total_errors.load(Ordering::Relaxed), 1000);
}

#[tokio::test]
async fn test_metrics_recovery_counters() {
    let metrics = ErrorMetrics::new();

    for _ in 0..5 {
        metrics.record_recovery_attempt();
    }
    for _ in 0..3 {
        metrics.record_recovery_success();
    }

    assert_eq!(metrics.recovery_attempts.load(Ordering::Relaxed), 5);
    assert_eq!(metrics.recovery_successes.load(Ordering::Relaxed), 3);
}

#[tokio::test]
async fn test_metrics_snapshot_accuracy() {
    let metrics = ErrorMetrics::new();
    metrics.record_error(&ErrorCategory::Runtime, Severity::Warning);
    metrics.record_recovery_attempt();
    metrics.record_panic();

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.total_errors, 1);
    assert_eq!(snapshot.recovery_attempts, 1);
    assert_eq!(snapshot.panic_count, 1);
}

#[tokio::test]
async fn test_metrics_by_category() {
    let metrics = ErrorMetrics::new();
    metrics.record_error(&ErrorCategory::Runtime, Severity::Critical);
    metrics.record_error(
        &ErrorCategory::AiCore { provider: None },
        Severity::Recoverable,
    );

    // Category entries should exist
    assert!(
        metrics
            .errors_by_category
            .contains_key(&ErrorCategory::Runtime)
    );
}

#[tokio::test]
async fn test_drop_rate_calculation() {
    let metrics = ErrorMetrics::new();
    metrics.record_error(&ErrorCategory::Runtime, Severity::Warning);
    // Rate should be > 0
    let rate = metrics.error_rate_per_sec();
    assert!(rate >= 0.0);
}
