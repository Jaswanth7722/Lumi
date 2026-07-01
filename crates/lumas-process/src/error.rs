//! # Process Error Hierarchy
//!
//! Complete, structured error types for the Lumas process management system.
//! Every variant carries rich context for diagnostics and recovery guidance.
//!
//! # Thread Safety
//!
//! All error types are `Send + Sync` by construction via `thiserror`.
//!
//! # Design
//!
//! Errors are classified as recoverable or non-recoverable. The supervisor
//! uses `is_recoverable()` to determine whether to attempt a restart or
//! escalate to the parent supervisor. `suggested_action()` provides
//! operator guidance for diagnostics.

use crate::id::ProcessId;
use crate::lifecycle::ProcessState;
use std::fmt;

// ---------------------------------------------------------------------------
// ProcessError
// ---------------------------------------------------------------------------

/// Primary error type for the process management system.
///
/// Every operation in the process lifecycle — registration, startup,
/// dependency validation, heartbeat monitoring, capability enforcement,
/// shutdown — returns `ProcessError` on failure.
///
/// # Errors
///
/// Each variant documents the specific failure mode. Callers should
/// match on the variant to determine the appropriate recovery action.
///
/// # Thread Safety
///
/// `ProcessError` is `Send + Sync` and can be propagated across thread
/// boundaries and async task boundaries.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    /// Process ID not found in the registry.
    #[error("Process '{id}' not found in registry")]
    NotFound {
        /// The process identifier that was not found.
        id: ProcessId,
    },

    /// Process ID is already registered.
    #[error("Process '{id}' already registered")]
    AlreadyRegistered {
        /// The duplicate process identifier.
        id: ProcessId,
    },

    /// A dependency cycle was detected in the process graph.
    #[error("Dependency cycle detected involving: {cycle:?}")]
    DependencyCycle {
        /// The cycle path (list of process IDs forming the cycle).
        cycle: Vec<ProcessId>,
    },

    /// A required dependency is missing from the registry.
    #[error("Dependency '{dep}' required by '{requirer}' is not registered")]
    MissingDependency {
        /// The dependency that is missing.
        dep: ProcessId,
        /// The process that requires this dependency.
        requirer: ProcessId,
    },

    /// Version incompatibility between a process and its dependency.
    #[error("Version incompatibility: '{id}' requires '{dep}' >= {required}, found {found}")]
    VersionIncompatible {
        /// The process with the dependency requirement.
        id: ProcessId,
        /// The dependency process.
        dep: ProcessId,
        /// The version requirement.
        required: semver::VersionReq,
        /// The actual version found.
        found: semver::Version,
    },

    /// Process failed to start.
    #[error("Process '{id}' failed to start: {reason}")]
    StartFailed {
        /// The process that failed to start.
        id: ProcessId,
        /// The reason for the failure.
        reason: String,
    },

    /// Process failed to stop within the configured timeout.
    #[error("Process '{id}' failed to stop within {timeout_ms}ms; force-killed")]
    StopTimeout {
        /// The process that timed out.
        id: ProcessId,
        /// The timeout duration in milliseconds.
        timeout_ms: u64,
    },

    /// Process crashed with an exit code.
    #[error("Process '{id}' crashed with exit code {exit_code:?}: {reason}")]
    Crashed {
        /// The process that crashed.
        id: ProcessId,
        /// Optional OS exit code.
        exit_code: Option<i32>,
        /// The reason for the crash.
        reason: String,
    },

    /// Process exceeded its maximum restart attempts.
    #[error("Process '{id}' exceeded max restarts ({max}) within {window_secs}s")]
    MaxRestartsExceeded {
        /// The process that exceeded limits.
        id: ProcessId,
        /// The maximum number of restarts allowed.
        max: u32,
        /// The sliding window duration in seconds.
        window_secs: u64,
    },

    /// Heartbeat timeout — process stopped sending liveness signals.
    #[error("Heartbeat timeout for '{id}': last seen {elapsed_ms}ms ago")]
    HeartbeatTimeout {
        /// The process that timed out.
        id: ProcessId,
        /// Milliseconds since the last heartbeat was received.
        elapsed_ms: u64,
    },

    /// A capability claim is not permitted by the process descriptor.
    #[error("Capability '{capability}' claimed by '{id}' is not permitted by its descriptor")]
    UnauthorizedCapability {
        /// The process making the claim.
        id: ProcessId,
        /// The capability name.
        capability: String,
    },

    /// Two processes claim the same exclusive capability.
    #[error("Duplicate capability '{capability}' claimed by both '{first}' and '{second}'")]
    DuplicateCapability {
        /// The capability name.
        capability: String,
        /// The first process to claim it.
        first: ProcessId,
        /// The second (conflicting) process.
        second: ProcessId,
    },

    /// A resource limit was exceeded for a process.
    #[error("Resource limit exceeded for '{id}': {resource} at {used}/{limit}")]
    ResourceLimitExceeded {
        /// The process exceeding limits.
        id: ProcessId,
        /// The resource name (e.g., "memory", "cpu").
        resource: String,
        /// Current usage value.
        used: u64,
        /// The configured limit.
        limit: u64,
    },

    /// An OS-level process operation failed.
    #[error("OS process operation failed for '{id}': {source}")]
    OsError {
        /// The process involved.
        id: ProcessId,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// An invalid state transition was attempted.
    #[error("Invalid state transition for '{id}': {from:?} → {to:?} is not allowed")]
    InvalidStateTransition {
        /// The process.
        id: ProcessId,
        /// The source state.
        from: ProcessState,
        /// The target state.
        to: ProcessState,
    },

    /// The process manager is shutting down; no new registrations accepted.
    #[error("Process manager is shutting down; no new processes may be registered")]
    ShuttingDown,

    /// A platform-specific operation is not supported on the current OS.
    #[error("Platform operation not supported: {operation}")]
    PlatformUnsupported {
        /// The operation that was attempted.
        operation: &'static str,
    },
}

impl ProcessError {
    /// Returns `true` if the supervisor can automatically recover from this error.
    ///
    /// Recoverable errors trigger the restart policy. Non-recoverable errors
    /// escalate to the parent supervisor immediately.
    pub fn is_recoverable(&self) -> bool {
        match self {
            ProcessError::NotFound { .. } => false,
            ProcessError::AlreadyRegistered { .. } => false,
            ProcessError::DependencyCycle { .. } => false,
            ProcessError::MissingDependency { .. } => false,
            ProcessError::VersionIncompatible { .. } => false,
            ProcessError::StartFailed { .. } => true,
            ProcessError::StopTimeout { .. } => false,
            // CRASHED is recoverable — the restart policy determines retry behavior
            ProcessError::Crashed { .. } => true,
            ProcessError::MaxRestartsExceeded { .. } => false,
            ProcessError::HeartbeatTimeout { .. } => true,
            ProcessError::UnauthorizedCapability { .. } => false,
            ProcessError::DuplicateCapability { .. } => false,
            ProcessError::ResourceLimitExceeded { .. } => true,
            ProcessError::OsError { .. } => true,
            ProcessError::InvalidStateTransition { .. } => false,
            ProcessError::ShuttingDown { .. } => false,
            ProcessError::PlatformUnsupported { .. } => false,
        }
    }

    /// Returns a human-readable suggested action for the operator.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            ProcessError::NotFound { .. } => "Verify the process is registered before operating on it.",
            ProcessError::AlreadyRegistered { .. } => {
                "Use a unique process ID or deregister the existing process."
            }
            ProcessError::DependencyCycle { .. } => {
                "Remove the circular dependency from the process definitions."
            }
            ProcessError::MissingDependency { .. } => {
                "Register the missing dependency before starting this process."
            }
            ProcessError::VersionIncompatible { .. } => {
                "Update the dependency to satisfy the version requirement."
            }
            ProcessError::StartFailed { .. } => {
                "Check the process logs for the specific startup failure."
            }
            ProcessError::StopTimeout { .. } => {
                "Increase shutdown_timeout_ms or investigate why the process hangs on stop."
            }
            ProcessError::Crashed { .. } => {
                "Check crash logs and fix the underlying bug. The supervisor will attempt restart."
            }
            ProcessError::MaxRestartsExceeded { .. } => {
                "The process is repeatedly crashing. Investigate the root cause or increase max_restarts."
            }
            ProcessError::HeartbeatTimeout { .. } => {
                "The process may be deadlocked or hung. Check process state and resource usage."
            }
            ProcessError::UnauthorizedCapability { .. } => {
                "Remove the capability from the process declaration or add it to the descriptor."
            }
            ProcessError::DuplicateCapability { .. } => {
                "Only one process may claim an exclusive capability. Resolve the conflict."
            }
            ProcessError::ResourceLimitExceeded { .. } => {
                "Increase the resource limit or reduce the process's resource consumption."
            }
            ProcessError::OsError { .. } => {
                "Check OS-level permissions and resource availability."
            }
            ProcessError::InvalidStateTransition { .. } => {
                "This indicates a bug in the state machine. Report with logs."
            }
            ProcessError::ShuttingDown => {
                "Wait for shutdown to complete before restarting."
            }
            ProcessError::PlatformUnsupported { .. } => {
                "This operation is not available on the current operating system."
            }
        }
    }

    /// Returns `true` if this error should escalate to the parent supervisor.
    ///
    /// Escalation is triggered for non-recoverable errors and for crashes
    /// where the restart policy has been exhausted.
    pub fn should_escalate(&self) -> bool {
        !self.is_recoverable()
    }
}

impl From<std::io::Error> for ProcessError {
    fn from(source: std::io::Error) -> Self {
        ProcessError::OsError {
            id: ProcessId::root(),
            source,
        }
    }
}

// ---------------------------------------------------------------------------
// ExitStatus
// ---------------------------------------------------------------------------

/// Encapsulates the exit status of a child process.
///
/// Used by the supervisor to classify process terminations and
/// determine whether to restart or escalate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// Process exited with a zero exit code (success).
    Success,
    /// Process exited with a non-zero exit code (failure).
    Failure {
        /// The exit code.
        code: i32,
    },
    /// Process was terminated by a signal (Unix only).
    Signaled {
        /// The signal number.
        signal: i32,
    },
    /// Unknown exit status.
    Unknown,
}

impl ExitStatus {
    /// Create an exit status from an exit code.
    ///
    /// Zero is treated as `Success`. Non-zero is `Failure`.
    pub fn from_code(code: i32) -> Self {
        if code == 0 {
            ExitStatus::Success
        } else {
            ExitStatus::Failure { code }
        }
    }

    /// Returns `true` if the process exited successfully.
    pub fn success(&self) -> bool {
        matches!(self, ExitStatus::Success)
    }

    /// Returns the exit code if available.
    pub fn exit_code(&self) -> Option<i32> {
        match self {
            ExitStatus::Success => Some(0),
            ExitStatus::Failure { code } => Some(*code),
            ExitStatus::Signaled { .. } => None,
            ExitStatus::Unknown => None,
        }
    }

    /// Returns a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            ExitStatus::Success => "exited successfully",
            ExitStatus::Failure { .. } => "exited with error code",
            ExitStatus::Signaled { .. } => "terminated by signal",
            ExitStatus::Unknown => "unknown exit status",
        }
    }
}

impl std::fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExitStatus::Success => write!(f, "Success"),
            ExitStatus::Failure { code } => write!(f, "ExitCode({code})"),
            ExitStatus::Signaled { signal } => write!(f, "Signal({signal})"),
            ExitStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::ProcessId;

    #[test]
    fn test_process_error_display() {
        let id = ProcessId::new("test.process");
        let err = ProcessError::NotFound { id: id.clone() };
        let display = err.to_string();
        assert!(display.contains("not found"));
        assert!(display.contains("test.process"));
    }

    #[test]
    fn test_crashed_is_recoverable() {
        let id = ProcessId::new("test");
        let err = ProcessError::Crashed {
            id,
            exit_code: Some(1),
            reason: "segfault".into(),
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_dependency_cycle_is_not_recoverable() {
        let err = ProcessError::DependencyCycle {
            cycle: vec![ProcessId::new("a"), ProcessId::new("b")],
        };
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_exit_status_from_code() {
        assert!(ExitStatus::from_code(0).success());
        assert!(!ExitStatus::from_code(1).success());
        assert_eq!(ExitStatus::from_code(42).exit_code(), Some(42));
    }

    #[test]
    fn test_error_suggested_action_non_empty() {
        let id = ProcessId::new("test");
        let err = ProcessError::Crashed {
            id,
            exit_code: Some(1),
            reason: "test".into(),
        };
        let action = err.suggested_action();
        assert!(!action.is_empty());
    }
}
