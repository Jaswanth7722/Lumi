//! # lumi-error — Centralized Error Handling for the Lumas Platform
//!
//! ## Architecture Overview
//!
//! Every subsystem in the Lumas workspace communicates failures exclusively
//! through `lumi-error`. This crate provides:
//!
//! - **[`LumasError`](error::LumasError)** — Root sum type over all subsystem error categories
//! - **[`ErrorCategory`](category::ErrorCategory)** — Typed, structured error categories with metadata
//! - **[`Severity`](severity::Severity)** — Typed severity with ordering and recovery guidance
//! - **[`ErrorCode`](error_code::ErrorCode)** — Stable numeric error codes (format: `LUMI-CAT-NNNN`)
//! - **[`RecoveryEngine`](recovery::RecoveryEngine)** — Rule-based error recovery with thrash detection
//! - **[`RetryPolicy`](retry::RetryPolicy)** — Builder-based retry with exponential backoff
//! - **[`CrashReport`](crash::CrashReport)** — Atomic crash report generation
//! - **[`ErrorMetrics`](metrics::ErrorMetrics)** — Lock-free error counters
//! - **[`ErrorHistory`](diagnostics::ErrorHistory)** — Queryable error history with pattern detection
//!
//! ## Quick Start
//!
//! ```rust
//! use lumas_error::prelude::*;
//!
//! fn do_work() -> LumiResult<String> {
//!     Err(LumasError::new(
//!         ErrorCode::AI_INFERENCE_FAILED,
//!         ErrorCategory::AiCore { provider: None },
//!         "Inference request failed",
//!     ))
//! }
//!
//! fn caller() -> LumiResult<String> {
//!     do_work().context("Failed to process AI request")?;
//!     Ok("done".to_string())
//! }
//! ```
//!
//! ## Security Model
//!
//! - `UserFacingMessage` (safe for UI display) vs `DiagnosticMessage` (full detail)
//! - Stack traces never reach user-facing output
//! - API keys, tokens, and passwords are not exposed in error messages
//!
//! ## Feature Flags
//!
//! - `backtrace` — Enables stack trace capture (off by default)
//! - `metrics` — Enables error metrics (on by default)
//! - `serde` — Enables serialization (on by default)
//! - `async` — Enables tokio-based async operations (on by default)

// WORKSPACE AUDIT — Required by spec
// This comment block documents the existing error infrastructure inventory
// conducted before implementing this crate.
//
// ## Inventory of Existing Error Infrastructure
//
// | Crate | File | Existing Error Types | Status |
// |-------|------|---------------------|--------|
// | lumas-runtime | src/error.rs | RuntimeError enum (8 variants), RuntimeResult<T> alias | REPLACE → LumasError |
// | lumas-runtime | src/event.rs | Event trait (Send+Sync+Clone+Debug+'static) | REUSE |
// | lumas-config | src/error.rs | ConfigError enum (12 variants) with is_recoverable/suggested_action | REPLACE → LumasError |
// | lumas-config | src/validator.rs | ValidationError, Validate trait | REPLACE → LumasError |
// | lumi-logging | src/error.rs | LogError enum (13 variants) | REPLACE → LumasError |
// | lumi-common | src/security.rs | SecretDescriptor, Error enum | REPLACE → LumasError |
// | lumi-common | src/ipc.rs | various error types | REPLACE → LumasError |
// | lumi-core | src/main.rs | uses tracing for errors | REUSE via ErrorFormatter |
//
// ## Migration Strategy
// 1. Phase 1: Define LumasError as the unified error type (this crate)
// 2. Phase 2: Add From<ExistingError> impls for each crate (using impl_from_error! macro)
// 3. Phase 3: Replace existing error returns with LumiResult in each crate
// 4. Phase 4: Remove legacy error types
// 5. Integration points: ErrorReport → lumi-logging sinks, ErrorEvent → event bus

#![cfg_attr(feature = "backtrace", feature(backtrace))]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

// serde derive macros are available through Cargo.toml dependency (features = ["derive"])

pub mod category;
pub mod config;
pub mod context;
pub mod crash;
pub mod diagnostics;
pub mod error;
pub mod error_code;
pub mod event;
pub mod formatter;
pub mod integration;
pub mod metrics;
pub mod panic;
pub mod recovery;
pub mod report;
pub mod retry;
pub mod severity;
pub mod stacktrace;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

// Re-export key types at the crate level for convenience
pub use category::ErrorCategory;
pub use config::ErrorConfig;
pub use crash::CrashReport;
pub use diagnostics::{
    DiagnosticReport, ErrorHistory, ErrorHistoryEntry, ErrorQuery, FailurePattern,
};
pub use error::{
    DiagnosticMessage, LumasError, LumiResult, LumiResultExt, RecoveryHint, UserFacingMessage,
};
pub use error_code::{ErrorCode, ErrorCodeEntry, lookup_error_code};
pub use event::{ErrorEvent, ErrorEventBus, ErrorEventEmitter};
pub use formatter::{ErrorFormatter, FormatKind, OutputMode};
pub use integration::ErrorBridge;
pub use metrics::{ErrorMetrics, MetricsSnapshot, global_error_metrics};
pub use recovery::{
    Capability, ComponentId, DegradedMode, FallbackHandler, RecoveryEngine, RecoveryOutcome,
    RecoveryRule, RecoveryRuleSet, RecoveryStrategy, ServiceId,
};
pub use report::{ErrorReport, ErrorReportBatch, ReportFormat};
pub use retry::{JitterConfig, RetryAttempt, RetryCondition, RetryPolicy, RetryStrategy};
pub use severity::Severity;

/// Prelude module — import this to get all commonly used types.
pub mod prelude {
    pub use crate::category::ErrorCategory;
    pub use crate::config::ErrorConfig;
    pub use crate::error::{LumasError, LumiResult, LumiResultExt};
    pub use crate::error_code::ErrorCode;
    pub use crate::formatter::{ErrorFormatter, FormatKind, OutputMode};
    pub use crate::metrics::{ErrorMetrics, MetricsSnapshot, global_error_metrics};
    pub use crate::recovery::{ComponentId, RecoveryEngine, RecoveryOutcome, RecoveryStrategy};
    pub use crate::report::ErrorReport;
    pub use crate::retry::{RetryPolicy, RetryStrategy};
    pub use crate::severity::Severity;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prelude_imports() {
        use prelude::*;
        let _: LumiResult<()> = Ok(());
    }

    #[test]
    fn test_error_round_trip() {
        let err = LumasError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "test",
        );
        let _ = err.to_string();
    }

    #[test]
    fn test_result_context() {
        fn inner() -> LumiResult<()> {
            Err(LumasError::new(
                ErrorCode::RUNTIME_INTERNAL,
                ErrorCategory::Runtime,
                "inner error",
            ))
        }

        let result = inner().context("outer context");
        assert!(result.is_err());
    }

    #[test]
    fn test_severity_in_errors() {
        let err = LumasError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "test",
        )
        .with_severity(Severity::Critical);

        assert_eq!(err.severity(), Severity::Critical);
        assert!(!err.severity().is_recoverable());
    }
}
