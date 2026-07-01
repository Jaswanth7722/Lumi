//! Integration tests for concurrency: race conditions under tokio::task::spawn.

use lumas_error::category::ErrorCategory;
use lumas_error::diagnostics::ErrorHistory;
use lumas_error::error::LumasError;
use lumas_error::error_code::ErrorCode;
use lumas_error::metrics::ErrorMetrics;
use lumas_error::prelude::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn test_concurrent_error_creation() {
    let mut handles = Vec::new();
    for i in 0..100 {
        handles.push(tokio::spawn(async move {
            let _err = LumasError::new(
                ErrorCode::AI_INFERENCE_FAILED,
                ErrorCategory::AiCore { provider: None },
                format!("error {}", i),
            );
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test]
async fn test_concurrent_metrics_updates() {
    let metrics = Arc::new(ErrorMetrics::new());
    let mut handles = Vec::new();

    for _ in 0..50 {
        let m = metrics.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..20 {
                m.record_error(&ErrorCategory::Runtime, Severity::Warning);
                m.record_recovery_attempt();
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(metrics.total_errors.load(Ordering::Relaxed), 1000);
    assert_eq!(metrics.recovery_attempts.load(Ordering::Relaxed), 1000);
}

#[tokio::test]
async fn test_concurrent_history_writes() {
    let history = Arc::new(ErrorHistory::new(1000));
    let mut handles = Vec::new();

    for i in 0..50 {
        let h = history.clone();
        handles.push(tokio::spawn(async move {
            let err = LumasError::new(
                ErrorCode::RUNTIME_INTERNAL,
                ErrorCategory::Runtime,
                format!("concurrent error {}", i),
            );
            h.record(&err);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(history.len(), 50);
}

#[tokio::test]
async fn test_concurrent_history_reads_during_writes() {
    let history = Arc::new(ErrorHistory::new(100));
    let history_clone = history.clone();

    // Writer task
    let writer = tokio::spawn(async move {
        for i in 0..20 {
            let err = LumasError::new(
                ErrorCode::INTERNAL_UNEXPECTED,
                ErrorCategory::Internal,
                format!("write {}", i),
            );
            history_clone.record(&err);
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    });

    // Reader tasks
    let mut readers = Vec::new();
    for _ in 0..5 {
        let h = history.clone();
        readers.push(tokio::spawn(async move {
            for _ in 0..10 {
                let _recent = h.recent(5);
                let _patterns = h.analyze_failure_patterns();
                tokio::time::sleep(std::time::Duration::from_milli(1)).await;
            }
        }));
    }

    writer.await.unwrap();
    for r in readers {
        r.await.unwrap();
    }
}

#[tokio::test]
async fn test_1000_spawns_no_data_loss() {
    let metrics = Arc::new(ErrorMetrics::new());
    let mut handles = Vec::new();

    for _ in 0..1000 {
        let m = metrics.clone();
        handles.push(tokio::spawn(async move {
            m.record_error(&ErrorCategory::Runtime, Severity::Trace);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(metrics.total_errors.load(Ordering::Relaxed), 1000);
}
