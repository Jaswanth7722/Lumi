//! # LumiError — Core Error Type
//!
//! The root error type for the entire Lumi platform.
//! A sum type over all subsystem error categories with severity, recovery hints,
//! structured context, and a full causal chain.

use crate::category::ErrorCategory;
use crate::context::ErrorContext;
use crate::error_code::ErrorCode;
use crate::recovery::RecoveryHint;
use crate::severity::Severity;
use std::fmt;
use std::sync::Arc;

/// A user-facing message (sanitized, safe to display in UI).
#[derive(Debug, Clone)]
pub struct UserFacingMessage(pub String);

impl UserFacingMessage {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl fmt::Display for UserFacingMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A diagnostic message (full detail, for logs and crash reports only).
#[derive(Debug, Clone)]
pub struct DiagnosticMessage(pub String);

impl DiagnosticMessage {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl fmt::Display for DiagnosticMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A recovery hint attached to an error.
#[derive(Debug, Clone)]
pub enum RecoveryHint {
    /// No specific hint.
    None,
    /// A human-readable suggestion.
    Suggestion(String),
    /// A specific recovery strategy.
    Strategy(crate::recovery::RecoveryStrategy),
}

impl Default for RecoveryHint {
    fn default() -> Self {
        Self::None
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "serde")] {
        use serde::{Serialize, Deserialize};
    }
}

/// The root error type for the Lumi platform.
///
/// LumiError is a structured error type that:
/// - Carries a typed `ErrorCategory` for every subsystem
/// - Encodes severity in the type system
/// - Has a stable error code that never changes between versions
/// - Tracks the full causal chain (bounded to 16 entries)
/// - Provides separate user-facing and diagnostic messages
/// - Supports context attachment via the `?` operator
///
/// # Example
///
/// ```ignore
/// use lumi_error::prelude::*;
/// use lumi_error::ErrorCode;
///
/// fn do_something() -> LumiResult<()> {
///     Err(LumiError::new(ErrorCode::AI_INFERENCE_FAILED, ErrorCategory::AiCore { provider: None })
///         .with_message("Inference request failed")
///         .with_recovery(RecoveryStrategy::Retry(RetryPolicy::exponential_default())))
/// }
/// ```
#[derive(Debug, Clone)]
pub struct LumiError {
    /// Error category from the taxonomy.
    category: ErrorCategory,
    /// Severity level.
    severity: Severity,
    /// Stable error code.
    code: ErrorCode,
    /// Causal chain and environment context.
    context: ErrorContext,
    /// Recovery hint.
    recovery: RecoveryHint,
    /// User-facing message (safe for UI).
    user_message: UserFacingMessage,
    /// Diagnostic message (full detail, for logs).
    diagnostic_message: DiagnosticMessage,
    /// Source error in the causal chain.
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl LumiError {
    /// Create a new LumiError with the given code and category.
    pub fn new(code: ErrorCode, category: ErrorCategory, message: impl Into<String>) -> Self {
        let msg = message.into();
        Self {
            category: category.clone(),
            severity: Severity::default(),
            code,
            context: ErrorContext::capture(
                crate::context::SourceLocation {
                    file: file!(),
                    line: line!(),
                    column: column!(),
                    function: "",
                },
                category.display_name(),
            ),
            recovery: RecoveryHint::None,
            user_message: UserFacingMessage::new(&msg),
            diagnostic_message: DiagnosticMessage::new(&msg),
            source: None,
        }
    }

    /// Set the severity.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Set a user-facing message.
    pub fn with_user_message(mut self, msg: impl Into<String>) -> Self {
        self.user_message = UserFacingMessage::new(msg);
        self
    }

    /// Set a diagnostic message.
    pub fn with_diagnostic_message(mut self, msg: impl Into<String>) -> Self {
        self.diagnostic_message = DiagnosticMessage::new(msg);
        self
    }

    /// Set a recovery hint.
    pub fn with_recovery(mut self, hint: RecoveryHint) -> Self {
        self.recovery = hint;
        self
    }

    /// Attach a source error.
    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Get the error category.
    pub fn category(&self) -> &ErrorCategory {
        &self.category
    }

    /// Get the severity.
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// Get the error code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// Get a reference to the context.
    pub fn context(&self) -> &ErrorContext {
        &self.context
    }

    /// Get the user-facing message.
    pub fn user_message(&self) -> &UserFacingMessage {
        &self.user_message
    }

    /// Get the diagnostic message.
    pub fn diagnostic_message(&self) -> &DiagnosticMessage {
        &self.diagnostic_message
    }

    /// Get the recovery hint.
    pub fn recovery(&self) -> &RecoveryHint {
        &self.recovery
    }
}

impl fmt::Display for LumiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_message)
    }
}

impl std::error::Error for LumiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|s| s.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<LumiError> for Box<dyn std::error::Error + Send + Sync + 'static> {
    fn from(err: LumiError) -> Self {
        Box::new(err)
    }
}

/// Convenience result type for LumiError.
pub type LumiResult<T> = Result<T, LumiError>;

impl<T> LumiResultExt for Result<T, LumiError> {
    fn context(self, msg: impl Into<String>) -> Self {
        self.map_err(|e| {
            LumiError::new(e.code(), e.category().clone(), msg)
                .with_severity(e.severity())
                .with_source(e)
        })
    }
}

/// Extension trait for LumiResult convenience methods.
pub trait LumiResultExt {
    /// Attach a human-readable context message.
    fn context(self, msg: impl Into<String>) -> Self;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::ErrorCategory;
    use crate::error_code::ErrorCode;

    #[test]
    fn test_error_creation() {
        let err = LumiError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "test error",
        );
        assert_eq!(err.code(), ErrorCode::AI_INFERENCE_FAILED);
        assert!(matches!(err.category(), ErrorCategory::AiCore { .. }));
    }

    #[test]
    fn test_error_display() {
        let err = LumiError::new(
            ErrorCode::CONFIG_FILE_NOT_FOUND,
            ErrorCategory::Configuration { field: None },
            "config not found",
        );
        assert_eq!(err.to_string(), "config not found");
    }

    #[test]
    fn test_error_source_chain() {
        let inner = LumiError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "inner error",
        );
        let outer = LumiError::new(
            ErrorCode::RUNTIME_SERVICE_FAILED,
            ErrorCategory::Runtime,
            "outer error",
        )
        .with_source(inner);
        assert!(outer.source().is_some());
    }

    #[test]
    fn test_result_ext() {
        let result: LumiResult<()> = Err(LumiError::new(
            ErrorCode::CONFIG_FILE_NOT_FOUND,
            ErrorCategory::Configuration { field: None },
            "original",
        ));
        let mapped = result.context("wrapped context");
        assert!(mapped.is_err());
        let err = mapped.unwrap_err();
        assert_eq!(err.code(), ErrorCode::CONFIG_FILE_NOT_FOUND);
    }
}
