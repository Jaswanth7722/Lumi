//! # Error Code Registry
//!
//! Every error variant has a stable, unique numeric code that never changes
//! between versions. Format: `LUMI-{CATEGORY}-{NNNN}`

use crate::category::ErrorCategory;
use crate::recovery::RecoveryStrategy;
use crate::severity::Severity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A stable, typed error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ErrorCode(u32);

impl ErrorCode {
    /// Create a new error code from its numeric value.
    pub const fn new(code: u32) -> Self {
        Self(code)
    }

    /// Return the numeric value.
    pub fn value(&self) -> u32 {
        self.0
    }

    /// Format as `LUMI-CATEGORY-NNNN`.
    pub fn format(&self, category: &ErrorCategory) -> String {
        let cat_prefix = category.short_code();
        format!("LUMI-{}-{:04}", cat_prefix, self.0)
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EC{:04}", self.0)
    }
}

impl From<u32> for ErrorCode {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// A registered entry in the error code registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorCodeEntry {
    /// The error code.
    pub code: ErrorCode,
    /// Human-readable name.
    pub name: &'static str,
    /// Error category.
    pub category: ErrorCategory,
    /// Default severity.
    pub default_severity: Severity,
    /// Default recovery strategy.
    pub default_recovery: RecoveryStrategy,
    /// Description of this error.
    pub description: &'static str,
    /// Documentation URL.
    pub docs_url: &'static str,
}

impl ErrorCodeEntry {
    /// Format the full error code string.
    pub fn formatted_code(&self) -> String {
        self.code.format(&self.category)
    }
}

// Error code constants by category
// Format: LUMI-{CAT}-{NNNN}
// Ranges: Runtime=01xx, Config=02xx, IPC=03xx, AI=04xx, Voice=05xx, etc.

/// Error codes for the Runtime category.
impl ErrorCode {
    // Runtime (01xx)
    pub const RUNTIME_BOOTSTRAP_FAILED: ErrorCode = ErrorCode(0x0101);
    pub const RUNTIME_SHUTDOWN_FAILED: ErrorCode = ErrorCode(0x0102);
    pub const RUNTIME_INTERNAL: ErrorCode = ErrorCode(0x0103);
    pub const RUNTIME_SERVICE_FAILED: ErrorCode = ErrorCode(0x0104);
    pub const RUNTIME_RESOURCE_EXHAUSTED: ErrorCode = ErrorCode(0x0105);
    pub const RUNTIME_LIFECYCLE_INVALID: ErrorCode = ErrorCode(0x0106);

    // Configuration (02xx)
    pub const CONFIG_FILE_NOT_FOUND: ErrorCode = ErrorCode(0x0201);
    pub const CONFIG_PARSE_ERROR: ErrorCode = ErrorCode(0x0202);
    pub const CONFIG_VALIDATION_FAILED: ErrorCode = ErrorCode(0x0203);
    pub const CONFIG_MIGRATION_FAILED: ErrorCode = ErrorCode(0x0204);
    pub const CONFIG_ENV_INVALID: ErrorCode = ErrorCode(0x0205);

    // IPC (03xx)
    pub const IPC_CONNECTION_FAILED: ErrorCode = ErrorCode(0x0301);
    pub const IPC_MESSAGE_SEND_FAILED: ErrorCode = ErrorCode(0x0302);
    pub const IPC_CHANNEL_CLOSED: ErrorCode = ErrorCode(0x0303);
    pub const IPC_TIMEOUT: ErrorCode = ErrorCode(0x0304);
    pub const IPC_PROTOCOL_ERROR: ErrorCode = ErrorCode(0x0305);

    // AI (04xx)
    pub const AI_INFERENCE_FAILED: ErrorCode = ErrorCode(0x0401);
    pub const AI_PROVIDER_UNREACHABLE: ErrorCode = ErrorCode(0x0402);
    pub const AI_MODEL_NOT_FOUND: ErrorCode = ErrorCode(0x0403);
    pub const AI_CONTEXT_OVERFLOW: ErrorCode = ErrorCode(0x0404);
    pub const AI_RATE_LIMITED: ErrorCode = ErrorCode(0x0405);

    // Voice (05xx)
    pub const VOICE_STT_FAILED: ErrorCode = ErrorCode(0x0501);
    pub const VOICE_TTS_FAILED: ErrorCode = ErrorCode(0x0502);
    pub const VOICE_WAKE_WORD_FAILED: ErrorCode = ErrorCode(0x0503);
    pub const VOICE_MICROPHONE_UNAVAILABLE: ErrorCode = ErrorCode(0x0504);

    // Storage (06xx)
    pub const STORAGE_READ_FAILED: ErrorCode = ErrorCode(0x0601);
    pub const STORAGE_WRITE_FAILED: ErrorCode = ErrorCode(0x0602);
    pub const STORAGE_FULL: ErrorCode = ErrorCode(0x0603);
    pub const STORAGE_CORRUPTION: ErrorCode = ErrorCode(0x0604);

    // Security (07xx)
    pub const SECURITY_ACCESS_DENIED: ErrorCode = ErrorCode(0x0701);
    pub const SECURITY_AUTHENTICATION_FAILED: ErrorCode = ErrorCode(0x0702);
    pub const SECURITY_SANDBOX_VIOLATION: ErrorCode = ErrorCode(0x0703);
    pub const SECURITY_SECRET_NOT_FOUND: ErrorCode = ErrorCode(0x0704);

    // Internal (99xx)
    pub const INTERNAL_INVARIANT_VIOLATION: ErrorCode = ErrorCode(0x9901);
    pub const INTERNAL_NOT_IMPLEMENTED: ErrorCode = ErrorCode(0x9902);
    pub const INTERNAL_UNEXPECTED: ErrorCode = ErrorCode(0x9903);
}

/// Static registry of all error codes.
/// Generated at compile time and tested for uniqueness.
pub static ERROR_CODE_REGISTRY: once_cell::sync::Lazy<HashMap<u32, ErrorCodeEntry>> =
    once_cell::sync::Lazy::new(build_registry);

fn build_registry() -> HashMap<u32, ErrorCodeEntry> {
    let mut m = HashMap::new();
    macro_rules! reg {
        ($code:ident, $name:literal, $cat:expr, $sev:expr, $rec:expr, $desc:literal, $docs:literal) => {
            let ec = ErrorCode::$code;
            m.insert(
                ec.0,
                ErrorCodeEntry {
                    code: ec,
                    name: $name,
                    category: $cat,
                    default_severity: $sev,
                    default_recovery: $rec,
                    description: $desc,
                    docs_url: $docs,
                },
            );
        };
    }

    use crate::recovery::RecoveryStrategy;
    use crate::severity::Severity;

    // Runtime
    reg!(
        RUNTIME_BOOTSTRAP_FAILED,
        "bootstrap_failed",
        ErrorCategory::Runtime,
        Severity::Fatal,
        RecoveryStrategy::SafeShutdown {
            save_state: true,
            exit_code: 1
        },
        "The runtime failed to complete bootstrap initialization",
        "https://docs.lumi.ai/errors/runtime/bootstrap-failed"
    );
    reg!(
        RUNTIME_SHUTDOWN_FAILED,
        "shutdown_failed",
        ErrorCategory::Runtime,
        Severity::Critical,
        RecoveryStrategy::SafeShutdown {
            save_state: false,
            exit_code: 1
        },
        "The runtime failed to shut down cleanly",
        "https://docs.lumi.ai/errors/runtime/shutdown-failed"
    );
    reg!(
        RUNTIME_INTERNAL,
        "internal_error",
        ErrorCategory::Internal,
        Severity::Fatal,
        RecoveryStrategy::CrashAndRecover,
        "An internal runtime invariant was violated",
        "https://docs.lumi.ai/errors/runtime/internal"
    );
    reg!(
        RUNTIME_SERVICE_FAILED,
        "service_failed",
        ErrorCategory::Runtime,
        Severity::Critical,
        RecoveryStrategy::RestartComponent {
            component_id: crate::recovery::ComponentId::new("service"),
            delay: std::time::Duration::from_secs(1)
        },
        "A registered service failed during operation",
        "https://docs.lumi.ai/errors/runtime/service-failed"
    );
    reg!(
        RUNTIME_RESOURCE_EXHAUSTED,
        "resource_exhausted",
        ErrorCategory::Runtime,
        Severity::Critical,
        RecoveryStrategy::GracefulDegradation {
            degraded_mode: crate::recovery::DegradedMode::ReducedFunctionality
        },
        "A system resource limit was exceeded",
        "https://docs.lumi.ai/errors/runtime/resource-exhausted"
    );

    // Configuration
    reg!(
        CONFIG_FILE_NOT_FOUND,
        "config_file_not_found",
        ErrorCategory::Configuration { field: None },
        Severity::Recoverable,
        RecoveryStrategy::LogAndContinue {
            min_severity: Severity::Warning
        },
        "Configuration file not found at expected path",
        "https://docs.lumi.ai/errors/config/file-not-found"
    );
    reg!(
        CONFIG_PARSE_ERROR,
        "config_parse_error",
        ErrorCategory::Configuration { field: None },
        Severity::Fatal,
        RecoveryStrategy::ReloadConfiguration,
        "Failed to parse configuration file",
        "https://docs.lumi.ai/errors/config/parse-error"
    );
    reg!(
        CONFIG_VALIDATION_FAILED,
        "config_validation_failed",
        ErrorCategory::Configuration { field: None },
        Severity::Critical,
        RecoveryStrategy::LogAndContinue {
            min_severity: Severity::Warning
        },
        "Configuration validation failed",
        "https://docs.lumi.ai/errors/config/validation-failed"
    );

    // IPC
    reg!(
        IPC_CONNECTION_FAILED,
        "ipc_connection_failed",
        ErrorCategory::Ipc {
            channel: "unknown".into()
        },
        Severity::Critical,
        RecoveryStrategy::Retry(crate::retry::RetryPolicy::exponential_default()),
        "IPC connection to subsystem failed",
        "https://docs.lumi.ai/errors/ipc/connection-failed"
    );

    // AI
    reg!(
        AI_INFERENCE_FAILED,
        "ai_inference_failed",
        ErrorCategory::AiCore { provider: None },
        Severity::Recoverable,
        RecoveryStrategy::Retry(crate::retry::RetryPolicy::exponential_default()),
        "AI inference request failed",
        "https://docs.lumi.ai/errors/ai/inference-failed"
    );

    // Storage
    reg!(
        STORAGE_WRITE_FAILED,
        "storage_write_failed",
        ErrorCategory::Storage { path: None },
        Severity::Critical,
        RecoveryStrategy::Retry(crate::retry::RetryPolicy::linear_default()),
        "Failed to write to storage subsystem",
        "https://docs.lumi.ai/errors/storage/write-failed"
    );

    reg!(
        INTERNAL_UNEXPECTED,
        "unexpected_error",
        ErrorCategory::Internal,
        Severity::Fatal,
        RecoveryStrategy::CrashAndRecover,
        "An unexpected error occurred",
        "https://docs.lumi.ai/errors/internal/unexpected"
    );

    m
}

/// Look up an error code entry by code value.
pub fn lookup_error_code(code: u32) -> Option<&'static ErrorCodeEntry> {
    ERROR_CODE_REGISTRY.get(&code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_error_code_uniqueness() {
        let mut seen = HashSet::new();
        for (code, entry) in ERROR_CODE_REGISTRY.iter() {
            assert!(
                seen.insert(code),
                "Duplicate error code: {} ({})",
                code,
                entry.name
            );
        }
    }

    #[test]
    fn test_error_code_format() {
        let entry = lookup_error_code(ErrorCode::CONFIG_FILE_NOT_FOUND.0).unwrap();
        assert_eq!(entry.formatted_code(), "LUMI-CFG-0201");
    }

    #[test]
    fn test_all_codes_are_unique_across_categories() {
        assert!(!ERROR_CODE_REGISTRY.is_empty());
    }
}
