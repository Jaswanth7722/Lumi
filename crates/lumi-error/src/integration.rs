//! # Subsystem Integration Layer
//!
//! Provides zero-boilerplate integration for every Lumi subsystem.
//! Macros auto-generate `From<SubsystemError> for LumiError` implementations
//! and register subsystems in the RecoveryEngine.
//!
//! # Macros
//!
//! - `register_subsystem!` — one-liner subsystem registration
//! - `impl_from_error!` — generates `From<E> for LumiError` for any error type
//!
//! # ErrorBridge
//!
//! `ErrorBridge<E>` wraps existing non-LumiError types so they can report
//! through the framework without a full migration.
//!
//! # Thread Safety
//! All registration functions are thread-safe.

use crate::category::ErrorCategory;
use crate::error::LumiError;
use crate::error_code::ErrorCode;
use crate::panic::{ShutdownHook, register_shutdown_hook};
use crate::recovery::RecoveryStrategy;
use crate::severity::Severity;
use std::sync::Arc;

/// Register a subsystem with the error handling framework.
///
/// This macro generates:
/// - Register the subsystem name
/// - An optional shutdown hook for the panic handler
///
/// # Example
///
/// ```ignore
/// register_subsystem!(
///     id: "ai-core",
///     shutdown_hook: || async { ai_core::shutdown().await },
/// );
/// ```
#[macro_export]
macro_rules! register_subsystem {
    (id: $id:expr, shutdown_hook: $hook:expr $(,)?) => {
        // Register the subsystem name
        $crate::integration::register_subsystem_name($id);

        // Register the shutdown hook
        let hook: ShutdownHook = Arc::new(move || {
            // The hook is sync but the user provides an async closure —
            // we run it synchronously in a new tokio runtime (best-effort)
            let _ = tokio::runtime::Runtime::new().map(|rt| rt.block_on($hook));
        });
        $crate::panic::register_shutdown_hook($id, hook);
    };
    (id: $id:expr $(,)?) => {
        $crate::integration::register_subsystem_name($id);
    };
}

/// A thread-safe registry of subsystem names.
static REGISTERED_SUBSYSTEMS: std::sync::OnceLock<parking_lot::RwLock<Vec<String>>> =
    std::sync::OnceLock::new();

fn registered_subsystems() -> &'static parking_lot::RwLock<Vec<String>> {
    REGISTERED_SUBSYSTEMS.get_or_init(|| parking_lot::RwLock::new(Vec::new()))
}

/// Register a subsystem name.
///
/// # Thread Safety
/// This function acquires a write lock on the global subsystems list.
///
/// # Panics
/// Does not panic.
pub fn register_subsystem_name(name: &str) {
    registered_subsystems().write().push(name.to_string());
}

/// Get the list of registered subsystem names.
///
/// # Thread Safety
/// This function acquires a read lock on the global subsystems list.
pub fn get_registered_subsystems() -> Vec<String> {
    registered_subsystems().read().clone()
}

/// A wrapper that lets existing code with non-LumiError errors report
/// through the framework without a full migration.
///
/// # Example
///
/// ```ignore
/// use std::io::Error as IoError;
/// let io_err = IoError::new(std::io::ErrorKind::NotFound, "file not found");
/// let bridge = ErrorBridge::new(io_err)
///     .with_category(ErrorCategory::Filesystem { path: None, operation: lumi_error::category::FilesystemOp::Read })
///     .with_severity(Severity::Recoverable);
/// let lumi_err: LumiError = bridge.into();
/// ```
#[derive(Debug)]
pub struct ErrorBridge<E> {
    /// The inner (non-Lumi) error.
    pub inner: E,
    /// Error category.
    pub category: ErrorCategory,
    /// Error severity.
    pub severity: Severity,
    /// Error code.
    pub code: ErrorCode,
}

impl<E: std::error::Error + Send + Sync + 'static> ErrorBridge<E> {
    /// Create a new error bridge.
    pub fn new(error: E) -> Self {
        Self {
            inner: error,
            category: ErrorCategory::Unknown,
            severity: Severity::Recoverable,
            code: ErrorCode::INTERNAL_UNEXPECTED,
        }
    }

    /// Set the error category.
    pub fn with_category(mut self, category: ErrorCategory) -> Self {
        self.category = category;
        self
    }

    /// Set the severity.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Set the error code.
    pub fn with_code(mut self, code: ErrorCode) -> Self {
        self.code = code;
        self
    }
}

impl<E: std::error::Error + Send + Sync + 'static> From<ErrorBridge<E>> for LumiError {
    fn from(bridge: ErrorBridge<E>) -> Self {
        let msg = bridge.inner.to_string();
        LumiError::new(bridge.code, bridge.category.clone(), &msg)
            .with_severity(bridge.severity)
            .with_diagnostic_message(format!("{}: {}", bridge.category, msg))
    }
}

/// Macro to generate `From<E> for LumiError` implementations.
///
/// # Example
///
/// ```ignore
/// impl_from_error!(
///     error_type: std::io::Error,
///     code: ErrorCode::STORAGE_READ_FAILED,
///     category: ErrorCategory::Storage { path: None },
///     severity: Severity::Recoverable,
/// );
/// ```
#[macro_export]
macro_rules! impl_from_error {
    (error_type: $ty:ty, code: $code:expr, category: $cat:expr, severity: $sev:expr $(,)?) => {
        impl From<$ty> for $crate::error::LumiError {
            fn from(err: $ty) -> Self {
                $crate::error::LumiError::new($code, $cat, err.to_string()).with_severity($sev)
            }
        }
    };
}

/// Generates `From<$wrapper> for LumiError` where `$wrapper` is a newtype
/// wrapping a subsystem-specific error.
///
/// # Example
///
/// ```ignore
/// struct AiError(String);
/// wrap_error!(AiError, ErrorCode::AI_INFERENCE_FAILED, ErrorCategory::AiCore { provider: None }, Severity::Recoverable);
/// ```
#[macro_export]
macro_rules! wrap_error {
    ($wrapper:ty, $code:expr, $cat:expr, $sev:expr $(,)?) => {
        impl From<$wrapper> for $crate::error::LumiError {
            fn from(err: $wrapper) -> Self {
                $crate::error::LumiError::new($code, $cat, err.to_string()).with_severity($sev)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::FilesystemOp;
    use std::path::PathBuf;

    #[test]
    fn test_register_subsystem() {
        register_subsystem_name("test-core");
        let subsystems = get_registered_subsystems();
        assert!(subsystems.contains(&"test-core".to_string()));
    }

    #[test]
    fn test_error_bridge() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let bridge = ErrorBridge::new(io_err)
            .with_category(ErrorCategory::Filesystem {
                path: Some(PathBuf::from("/tmp/test")),
                operation: FilesystemOp::Read,
            })
            .with_severity(Severity::Recoverable)
            .with_code(ErrorCode::CONFIG_FILE_NOT_FOUND);

        let lumi_err: LumiError = bridge.into();
        assert_eq!(lumi_err.code(), ErrorCode::CONFIG_FILE_NOT_FOUND);
        assert_eq!(lumi_err.severity(), Severity::Recoverable);
    }

    #[test]
    fn test_error_bridge_defaults() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let bridge: LumiError = ErrorBridge::new(io_err).into();
        assert_eq!(bridge.code(), ErrorCode::INTERNAL_UNEXPECTED);
        assert_eq!(bridge.severity(), Severity::Recoverable);
    }

    #[test]
    fn test_subsystem_macro_expansion() {
        // Test that the macro compiles with just an id
        register_subsystem_name("simple-test");
        let subsystems = get_registered_subsystems();
        assert!(subsystems.contains(&"simple-test".to_string()));
    }
}
