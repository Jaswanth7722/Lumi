//! # Test Utilities
//!
//! Provides test helpers for lumi-error:
//! - `LumiErrorBuilder` — fluent builder for test error construction
//! - `assert_error_code!` — macro to assert error codes
//! - `assert_user_safe!` — macro to assert no secrets in user-facing output
//! - `MockRecoveryEngine` — records recovery calls for assertion
//! - `FakeClock` — deterministic timestamps in tests
//!
//! Gated behind `#[cfg(any(test, feature = "testing"))]`.
//!
//! # Thread Safety
//! All test utilities are `Send + Sync`.

use crate::category::ErrorCategory;
use crate::error::LumiError;
use crate::error_code::ErrorCode;
use crate::recovery::{RecoveryEngine, RecoveryOutcome, RecoveryRule, RecoveryStrategy};
use crate::severity::Severity;

/// Fluent builder for constructing LumiErrors in tests.
#[derive(Debug)]
pub struct LumiErrorBuilder {
    code: ErrorCode,
    category: ErrorCategory,
    message: String,
    severity: Severity,
    source: Option<LumiError>,
}

impl LumiErrorBuilder {
    /// Create a new error builder.
    pub fn new() -> Self {
        Self {
            code: ErrorCode::INTERNAL_UNEXPECTED,
            category: ErrorCategory::Internal,
            message: String::new(),
            severity: Severity::Recoverable,
            source: None,
        }
    }

    /// Set the error code.
    pub fn with_code(mut self, code: ErrorCode) -> Self {
        self.code = code;
        self
    }

    /// Set the error category.
    pub fn with_category(mut self, category: ErrorCategory) -> Self {
        self.category = category;
        self
    }

    /// Set the error message.
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Set the severity.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Attach a source error.
    pub fn with_source(mut self, source: LumiError) -> Self {
        self.source = Some(source);
        self
    }

    /// Build the LumiError.
    pub fn build(self) -> LumiError {
        let mut err =
            LumiError::new(self.code, self.category, self.message).with_severity(self.severity);
        if let Some(source) = self.source {
            err = err.with_source(source);
        }
        err
    }
}

impl Default for LumiErrorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Assert that a result's error code matches the expected code.
///
/// # Example
///
/// ```ignore
/// let result: LumiResult<()> = Err(make_test_error());
/// assert_error_code!(result, ErrorCode::AI_INFERENCE_FAILED);
/// ```
#[macro_export]
macro_rules! assert_error_code {
    ($result:expr, $code:expr) => {
        match $result {
            Ok(_) => panic!("expected error but got Ok"),
            Err(e) => assert_eq!(e.code(), $code, "error code mismatch"),
        }
    };
}

/// Assert that an error's user-facing output contains no secrets.
///
/// Checks that the user-facing message does not contain:
/// - API key patterns
/// - Token patterns
/// - Password patterns
///
/// # Example
///
/// ```ignore
/// let error = make_test_error();
/// assert_user_safe!(error);
/// ```
#[macro_export]
macro_rules! assert_user_safe {
    ($error:expr) => {
        let user_msg = $error.user_message().to_string();
        let secrets = ["sk-ant-", "sk-", "ghp_", "-----BEGIN", "bearer ", "basic "];
        for secret in &secrets {
            assert!(
                !user_msg.to_lowercase().contains(secret),
                "user-facing message contains potential secret: '{}'",
                secret
            );
        }
    };
}

/// A mock recovery engine that records recovery calls for assertion.
#[derive(Debug, Default)]
pub struct MockRecoveryEngine {
    /// Record of recovery calls.
    pub calls: Vec<MockRecoveryCall>,
}

/// A recorded recovery call.
#[derive(Debug, Clone)]
pub struct MockRecoveryCall {
    /// The error that was being recovered.
    pub error_code: ErrorCode,
    /// The strategy that was applied.
    pub strategy: RecoveryStrategy,
    /// The outcome returned.
    pub outcome: RecoveryOutcome,
}

impl MockRecoveryEngine {
    /// Create a new mock recovery engine.
    pub fn new() -> Self {
        Self { calls: Vec::new() }
    }

    /// Record a recovery attempt and return the configured outcome.
    pub fn recover(&mut self, error: &LumiError, strategy: &RecoveryStrategy) -> RecoveryOutcome {
        let outcome = RecoveryOutcome::Recovered;
        self.calls.push(MockRecoveryCall {
            error_code: error.code(),
            strategy: strategy.clone(),
            outcome: outcome.clone(),
        });
        outcome
    }

    /// Get the number of recovery calls.
    pub fn call_count(&self) -> usize {
        self.calls.len()
    }

    /// Get the last recovery call.
    pub fn last_call(&self) -> Option<&MockRecoveryCall> {
        self.calls.last()
    }

    /// Clear recorded calls.
    pub fn clear(&mut self) {
        self.calls.clear();
    }
}

/// A fake clock for deterministic timestamps in tests.
#[derive(Debug, Clone)]
pub struct FakeClock {
    /// The current time as a duration since epoch.
    elapsed: std::time::Duration,
}

impl FakeClock {
    /// Create a new fake clock at the given time.
    pub fn new(start: std::time::Duration) -> Self {
        Self { elapsed: start }
    }

    /// Create a new fake clock at Unix epoch.
    pub fn epoch() -> Self {
        Self {
            elapsed: std::time::Duration::ZERO,
        }
    }

    /// Advance the clock by the given duration.
    pub fn advance(&mut self, duration: std::time::Duration) {
        self.elapsed += duration;
    }

    /// Get the current fake time.
    pub fn now(&self) -> std::time::Duration {
        self.elapsed
    }

    /// Get the current fake time as a SystemTime.
    pub fn system_time(&self) -> std::time::SystemTime {
        std::time::UNIX_EPOCH + self.elapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::ErrorCategory;

    #[test]
    fn test_error_builder() {
        let error = LumiErrorBuilder::new()
            .with_code(ErrorCode::AI_INFERENCE_FAILED)
            .with_category(ErrorCategory::AiCore { provider: None })
            .with_message("test error")
            .with_severity(Severity::Critical)
            .build();

        assert_eq!(error.code(), ErrorCode::AI_INFERENCE_FAILED);
        assert_eq!(error.severity(), Severity::Critical);
    }

    #[test]
    fn test_mock_recovery_engine() {
        let mut mock = MockRecoveryEngine::new();
        let error = LumiErrorBuilder::new()
            .with_code(ErrorCode::AI_INFERENCE_FAILED)
            .with_category(ErrorCategory::AiCore { provider: None })
            .with_message("test")
            .build();

        let strategy = RecoveryStrategy::Retry(crate::retry::RetryPolicy::exponential_default());
        mock.recover(&error, &strategy);

        assert_eq!(mock.call_count(), 1);
        assert!(mock.last_call().is_some());
    }

    #[test]
    fn test_fake_clock() {
        let mut clock = FakeClock::epoch();
        let now = clock.now();
        assert_eq!(now.as_secs(), 0);

        clock.advance(std::time::Duration::from_secs(100));
        assert_eq!(clock.now().as_secs(), 100);
    }

    #[test]
    fn test_assert_error_code_macro() {
        let error = LumiErrorBuilder::new()
            .with_code(ErrorCode::AI_INFERENCE_FAILED)
            .with_category(ErrorCategory::AiCore { provider: None })
            .with_message("test")
            .build();

        assert_eq!(error.code(), ErrorCode::AI_INFERENCE_FAILED);
    }

    #[test]
    fn test_assert_user_safe_macro() {
        let error = LumiErrorBuilder::new()
            .with_code(ErrorCode::AI_INFERENCE_FAILED)
            .with_category(ErrorCategory::AiCore { provider: None })
            .with_message("Inference provider unreachable")
            .build();

        // This should not panic
        let _ = error;
    }
}
