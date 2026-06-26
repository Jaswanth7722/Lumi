//! Integration tests for retry policies and strategies.

use lumi_error::category::ErrorCategory;
use lumi_error::error::LumiError;
use lumi_error::error_code::ErrorCode;
use lumi_error::prelude::*;
use lumi_error::retry::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

#[tokio::test]
async fn test_retry_immediate() {
    let policy = RetryPolicy::new(3).with_strategy(RetryStrategy::Immediate);

    let attempts = AtomicU32::new(0);
    let result = retry(&policy, || async {
        attempts.fetch_add(1, Ordering::SeqCst);
        Err::<(), LumiError>(LumiError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "transient",
        ))
    })
    .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_retry_success_on_second_attempt() {
    let policy = RetryPolicy::new(3).with_strategy(RetryStrategy::Immediate);

    let attempts = AtomicU32::new(0);
    let result = retry(&policy, || async {
        let current = attempts.fetch_add(1, Ordering::SeqCst);
        if current == 1 {
            Ok::<_, LumiError>("success")
        } else {
            Err(LumiError::new(
                ErrorCode::AI_INFERENCE_FAILED,
                ErrorCategory::AiCore { provider: None },
                "transient",
            ))
        }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
}

#[tokio::test]
async fn test_retry_fatal_does_not_retry() {
    let policy = RetryPolicy::new(3).with_strategy(RetryStrategy::Immediate);

    let attempts = AtomicU32::new(0);
    let result = retry(&policy, || async {
        attempts.fetch_add(1, Ordering::SeqCst);
        Err::<(), LumiError>(
            LumiError::new(ErrorCode::RUNTIME_INTERNAL, ErrorCategory::Runtime, "fatal")
                .with_severity(Severity::Fatal),
        )
    })
    .await;

    assert!(result.is_err());
    // Should not retry on fatal
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_exponential_backoff_delays() {
    let policy = RetryPolicy::new(5).with_strategy(RetryStrategy::Exponential {
        initial: Duration::from_millis(10),
        base: 2.0,
        max: Duration::from_secs(1),
    });

    let delay1 = policy.delay_for_attempt(1);
    let delay2 = policy.delay_for_attempt(2);
    let delay3 = policy.delay_for_attempt(3);

    assert_eq!(delay1, Duration::from_millis(10));
    assert_eq!(delay2, Duration::from_millis(20));
    assert_eq!(delay3, Duration::from_millis(40));
}

#[tokio::test]
async fn test_retry_condition_recoverable_only() {
    let condition = RetryCondition::recoverable_only();
    let fatal_err = LumiError::new(ErrorCode::RUNTIME_INTERNAL, ErrorCategory::Runtime, "fatal")
        .with_severity(Severity::Fatal);

    assert!(!condition.should_retry(&fatal_err));
}

#[tokio::test]
async fn test_retry_cancellation() {
    // Test that retry respects errors that shouldn't be retried
    let policy = RetryPolicy::new(5)
        .with_strategy(RetryStrategy::Immediate)
        .with_retry_condition(RetryCondition::new(|e| {
            e.code() != ErrorCode::AI_INFERENCE_FAILED
        }));

    let result = retry(&policy, || async {
        Err::<(), LumiError>(LumiError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "should not retry",
        ))
    })
    .await;

    assert!(result.is_err());
}
