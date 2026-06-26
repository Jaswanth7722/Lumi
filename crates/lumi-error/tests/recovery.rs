//! Integration tests for recovery strategies, thrash detection, and escalation.

use lumi_error::category::ErrorCategory;
use lumi_error::error::LumiError;
use lumi_error::error_code::ErrorCode;
use lumi_error::prelude::*;
use lumi_error::recovery::*;
use std::time::Duration;

#[tokio::test]
async fn test_recovery_engine_creation() {
    let engine = RecoveryEngine::new();
    // Default engine should handle any error
    engine.add_rule(RecoveryRule {
        error_code: None,
        category_filter: None,
        strategy: RecoveryStrategy::LogAndContinue {
            min_severity: Severity::Warning,
        },
        priority: 0,
    });
}

#[tokio::test]
async fn test_recovery_rule_matching() {
    let error = LumiError::new(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "test",
    );

    let mut rule_set = RecoveryRuleSet::new();
    rule_set.add(RecoveryRule {
        error_code: None,
        category_filter: None,
        strategy: RecoveryStrategy::Retry(RetryPolicy::exponential_default()),
        priority: 10,
    });

    let strategy = rule_set.match_strategy(&error);
    assert!(strategy.is_some());
    assert!(matches!(strategy.unwrap(), RecoveryStrategy::Retry(_)));
}

#[tokio::test]
async fn test_recovery_with_priority() {
    let error = LumiError::new(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "test",
    );

    let mut rule_set = RecoveryRuleSet::new();
    rule_set.add(RecoveryRule {
        error_code: None,
        category_filter: None,
        strategy: RecoveryStrategy::Ignore,
        priority: 0,
    });
    rule_set.add(RecoveryRule {
        error_code: None,
        category_filter: None,
        strategy: RecoveryStrategy::Retry(RetryPolicy::exponential_default()),
        priority: 100,
    });

    // Higher priority should win
    let strategy = rule_set.match_strategy(&error);
    assert!(matches!(strategy.unwrap(), RecoveryStrategy::Retry(_)));
}

#[tokio::test]
async fn test_thrash_escalates_to_safe_shutdown() {
    let engine = RecoveryEngine::new();
    engine.add_rule(RecoveryRule {
        error_code: None,
        category_filter: None,
        strategy: RecoveryStrategy::RestartComponent {
            component_id: ComponentId::new("ai-core"),
            delay: Duration::from_millis(1),
        },
        priority: 100,
    });

    let error = LumiError::new(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "test",
    );

    // Multiple rapid recovery attempts should trigger thrash detection
    for i in 0..6 {
        let outcome = engine.recover(&error);
        if i >= 4 {
            match outcome {
                RecoveryOutcome::Escalated { .. } => return, // Thrash detected
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    panic!("Thrash detection should have triggered safe shutdown");
}

#[tokio::test]
async fn test_recovery_outcome_types() {
    // Recovered
    let outcome = RecoveryOutcome::Recovered;
    assert!(matches!(outcome, RecoveryOutcome::Recovered));

    // Degraded
    let outcome = RecoveryOutcome::Degraded {
        capabilities_lost: vec![Capability("voice".into())],
    };
    assert!(matches!(outcome, RecoveryOutcome::Degraded { .. }));

    // Escalated
    let outcome = RecoveryOutcome::Escalated {
        escalated_to: Box::new(RecoveryStrategy::SafeShutdown {
            save_state: true,
            exit_code: 1,
        }),
    };
    assert!(matches!(outcome, RecoveryOutcome::Escalated { .. }));

    // Failed
    let err = LumiError::new(
        ErrorCode::INTERNAL_UNEXPECTED,
        ErrorCategory::Internal,
        "recovery failed",
    );
    let outcome = RecoveryOutcome::Failed {
        reason: Box::new(err),
    };
    assert!(matches!(outcome, RecoveryOutcome::Failed { .. }));
}
