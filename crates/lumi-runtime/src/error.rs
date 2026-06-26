//! # Unified Error System
//!
//! Complete, structured error hierarchy for the Lumi runtime.
//! Every variant carries rich context (file, operation, subsystem name)
//! and provides recovery guidance to callers.
//!
//! # Thread Safety
//!
//! All error types are `Send + Sync` by construction via `thiserror` and `Arc`.
//!
//! # Organization
//!
//! - `RuntimeError` — top-level enum wrapping all subsystem errors
//! - `BootstrapError` — startup phase failures
//! - `ServiceError` — per-service lifecycle failures
//! - `ConfigError` — configuration loading, parsing, validation
//! - `SchedulerError` — task dispatch, cancellation, timeout
//! - `EventError` — publish/subscribe failures
//! - `HealthError` — health check failures
//! - `ShutdownError` — shutdown phase failures
//! - `ResourceError` — resource limit violations

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// RuntimeError — top-level
// ---------------------------------------------------------------------------

/// Top-level error type that wraps all subsystem errors.
///
/// This is the primary error type exposed by the `lumi-runtime` public API.
/// Consumers should match on this enum to handle different failure modes.
///
/// # Errors
///
/// Returns `RuntimeError` for any operation that fails within the runtime.
///
/// # Examples
///
/// ```ignore
/// match err {
///     RuntimeError::Bootstrap(e) => eprintln!("Bootstrap failed: {e}"),
///     RuntimeError::Service(e) => eprintln!("Service failed: {e}"),
///     _ => eprintln!("Other error: {err}"),
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// Error during the bootstrap/startup sequence.
    #[error("Bootstrap failed at phase '{phase}': {source}")]
    Bootstrap {
        /// The bootstrap phase that failed.
        phase: &'static str,
        /// The underlying bootstrap error.
        source: Box<BootstrapError>,
    },

    /// Error in a registered service.
    #[error("Service '{name}' failed during {phase}: {source}")]
    Service {
        /// The service name.
        name: String,
        /// The lifecycle phase during which the failure occurred.
        phase: &'static str,
        /// The underlying service error.
        source: ServiceError,
    },

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Scheduler error.
    #[error("Scheduler error: {0}")]
    Scheduler(#[from] SchedulerError),

    /// Event bus error.
    #[error("Event bus error: {0}")]
    Event(#[from] EventError),

    /// Health monitor error.
    #[error("Health monitor error: {0}")]
    Health(#[from] HealthError),

    /// Shutdown error.
    #[error("Shutdown error: {0}")]
    Shutdown(#[from] ShutdownError),

    /// Resource limit violation.
    #[error("Resource error: {0}")]
    Resource(#[from] ResourceError),

    /// Internal runtime invariant violation (should not occur in production).
    #[error("Internal runtime error: {message}")]
    Internal {
        /// Description of the invariant violation.
        message: String,
        /// Optional source location.
        location: Option<&'static str>,
    },
}

impl RuntimeError {
    /// Whether this error is recoverable without restarting the runtime.
    pub fn is_recoverable(&self) -> bool {
        match self {
            RuntimeError::Bootstrap { .. } => false,
            RuntimeError::Service { source, .. } => source.is_recoverable(),
            RuntimeError::Config(e) => e.is_recoverable(),
            RuntimeError::Scheduler(e) => e.is_recoverable(),
            RuntimeError::Event(e) => e.is_recoverable(),
            RuntimeError::Health(e) => e.is_recoverable(),
            RuntimeError::Shutdown(_) => false,
            RuntimeError::Resource(e) => e.is_recoverable(),
            RuntimeError::Internal { .. } => false,
        }
    }

    /// A human-readable suggestion for resolving this error.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            RuntimeError::Bootstrap { .. } => {
                "Check the bootstrap phase that failed. Review logs for details. Fix configuration or environment."
            }
            RuntimeError::Service { source, .. } => source.suggested_action(),
            RuntimeError::Config(e) => e.suggested_action(),
            RuntimeError::Scheduler(e) => e.suggested_action(),
            RuntimeError::Event(e) => e.suggested_action(),
            RuntimeError::Health(e) => e.suggested_action(),
            RuntimeError::Shutdown(_) => {
                "Ensure all services can shut down cleanly. Check for hung tasks."
            }
            RuntimeError::Resource(e) => e.suggested_action(),
            RuntimeError::Internal { .. } => "This is a bug. Please report it with the logs.",
        }
    }
}

// ---------------------------------------------------------------------------
// BootstrapError
// ---------------------------------------------------------------------------

/// Errors that occur during the bootstrap/startup sequence.
///
/// Each error records which phase failed and carries the primary error
/// plus any rollback errors from undoing already-initialized phases.
///
/// # Errors
///
/// Produced by `Bootstrap::start()` when any bootstrap step fails.
///
/// # Recovery
///
/// Bootstrap failures are generally not recoverable at runtime; the
/// process should exit and be restarted after fixing the root cause.
#[derive(Debug, thiserror::Error)]
pub struct BootstrapError {
    /// The bootstrap phase that failed (e.g., "LoadingConfig", "StartingServices").
    pub phase: &'static str,
    /// The primary error message.
    pub message: String,
    /// Errors encountered while rolling back already-initialized phases.
    pub rollback_errors: Vec<String>,
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "bootstrap failed at phase '{}': {}",
            self.phase, self.message
        )?;
        if !self.rollback_errors.is_empty() {
            write!(
                f,
                " (rollback encountered {} errors)",
                self.rollback_errors.len()
            )?;
        }
        Ok(())
    }
}

impl BootstrapError {
    /// Create a new bootstrap error.
    pub fn new(phase: &'static str, message: impl Into<String>) -> Self {
        Self {
            phase,
            message: message.into(),
            rollback_errors: Vec::new(),
        }
    }

    /// Add a rollback error that occurred while undoing this phase.
    pub fn with_rollback_error(mut self, error: impl Into<String>) -> Self {
        self.rollback_errors.push(error.into());
        self
    }

    /// Suggested action for recovery.
    pub fn suggested_action(&self) -> &'static str {
        match self.phase {
            "LoadingConfig" => {
                "Check your configuration file for errors. Run with --help for defaults."
            }
            "InitializingLogger" => "Check filesystem permissions for the log directory.",
            "InitializingIPC" => "Check if another instance is running. Clean stale socket files.",
            "InitializingStorage" => "Check filesystem permissions for the data directory.",
            "DiscoveringPlugins" => "Remove or repair the plugin that failed to load.",
            "RegisteringServices" => "This is an internal error. Check the service implementation.",
            "ResolvingDependencies" => "Fix circular or missing service dependencies.",
            "StartingServices" => "Check the service logs for the specific failure.",
            "StartingHealthMonitor" => {
                "This is an internal error. Check the health monitor configuration."
            }
            _ => "Check logs for the specific phase failure.",
        }
    }
}

// ---------------------------------------------------------------------------
// ServiceError
// ---------------------------------------------------------------------------

/// Errors that occur during service lifecycle operations.
///
/// # Errors
///
/// Produced by `Service::start()`, `Service::stop()`, or `ServiceManager`
/// when a service fails to start, stop, or encounters a runtime error.
///
/// # Recovery
///
/// Service failures may be recoverable via automatic restart depending
/// on the `recoverable` flag and the configured restart policy.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// Service failed to start.
    #[error("Service '{name}' failed to start: {message}")]
    StartFailed {
        /// The service name.
        name: String,
        /// Description of the failure.
        message: String,
        /// Whether this failure is recoverable.
        recoverable: bool,
    },

    /// Service failed to stop.
    #[error("Service '{name}' failed to stop: {message}")]
    StopFailed {
        /// The service name.
        name: String,
        /// Description of the failure.
        message: String,
        /// Whether this failure is recoverable.
        recoverable: bool,
    },

    /// Service health check failed.
    #[error("Service '{name}' health check failed: {message}")]
    HealthCheckFailed {
        /// The service name.
        name: String,
        /// Description of the failure.
        message: String,
        /// Whether this failure is recoverable.
        recoverable: bool,
    },

    /// Service not found in the registry.
    #[error("Service '{0}' not found")]
    NotFound(String),

    /// Dependency cycle detected during service resolution.
    #[error("Dependency cycle detected: {cycle:?}")]
    DependencyCycle {
        /// The cycle path (list of service names).
        cycle: Vec<String>,
    },

    /// Service timed out during start/stop.
    #[error("Service '{name}' timed out during {operation} after {timeout_secs}s")]
    Timeout {
        /// The service name.
        name: String,
        /// The operation that timed out.
        operation: &'static str,
        /// The timeout duration in seconds.
        timeout_secs: u64,
    },

    /// Service reached maximum restart attempts.
    #[error("Service '{name}' exceeded max restart attempts ({attempts})")]
    MaxRestartsExceeded {
        /// The service name.
        name: String,
        /// The number of restart attempts made.
        attempts: u32,
    },
}

impl ServiceError {
    /// Whether this error is recoverable without restarting the runtime.
    pub fn is_recoverable(&self) -> bool {
        match self {
            ServiceError::StartFailed { recoverable, .. } => *recoverable,
            ServiceError::StopFailed { .. } => false,
            ServiceError::HealthCheckFailed { recoverable, .. } => *recoverable,
            ServiceError::NotFound(_) => false,
            ServiceError::DependencyCycle { .. } => false,
            ServiceError::Timeout { .. } => true,
            ServiceError::MaxRestartsExceeded { .. } => false,
        }
    }

    /// A human-readable suggestion for resolving this error.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            ServiceError::StartFailed { name, .. } => match name.as_str() {
                "ai-core" => "Check the AI model path and provider configuration.",
                "voice" => "Check microphone permissions and audio device configuration.",
                "memory" => "Check the storage directory permissions and disk space.",
                _ => "Check the service logs for the specific failure reason.",
            },
            ServiceError::StopFailed { .. } => {
                "Force stop the process if graceful shutdown persists."
            }
            ServiceError::HealthCheckFailed { .. } => {
                "The service may be overloaded or crashed. Check system resources."
            }
            ServiceError::NotFound(_) => "Register the service before starting it.",
            ServiceError::DependencyCycle { .. } => {
                "Remove the circular dependency from the service definitions."
            }
            ServiceError::Timeout { .. } => {
                "The service may be hung. Increase the timeout or check for deadlocks."
            }
            ServiceError::MaxRestartsExceeded { .. } => {
                "The service is repeatedly failing. Check logs and disable the service if necessary."
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ConfigError
// ---------------------------------------------------------------------------

/// Errors that occur during configuration loading, parsing, or validation.
///
/// # Errors
///
/// Produced by `ConfigLoader` when a configuration file cannot be read,
/// parsed, or fails validation checks.
///
/// # Recovery
///
/// Configuration errors during bootstrap are unrecoverable. During hot reload,
/// the previous configuration remains active and the error is logged.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Configuration file not found or unreadable.
    #[error("Configuration file not found at '{path}': {message}")]
    FileNotFound {
        /// The path that was attempted.
        path: PathBuf,
        /// Why the file could not be read.
        message: String,
    },

    /// Configuration file parse error.
    #[error("Failed to parse configuration at {path}:{line}:{column} — {message}")]
    ParseError {
        /// The file being parsed.
        path: PathBuf,
        /// Line number where the error occurred.
        line: usize,
        /// Column number where the error occurred.
        column: usize,
        /// Description of the parse failure.
        message: String,
    },

    /// Environment variable parsing error.
    #[error("Invalid environment variable {var}='{value}': {message}")]
    EnvVarError {
        /// The environment variable name.
        var: String,
        /// The value that could not be parsed.
        value: String,
        /// Why the value is invalid.
        message: String,
    },

    /// Numeric value out of allowed range.
    #[error("Value '{key}' = {value} is out of range [{min}, {max}] in section '{section}'")]
    RangeError {
        /// The configuration section.
        section: &'static str,
        /// The configuration key.
        key: &'static str,
        /// The value provided.
        value: f64,
        /// The minimum allowed value.
        min: f64,
        /// The maximum allowed value.
        max: f64,
    },

    /// A required path does not exist.
    #[error("Required path '{path}' for config key '{key}' does not exist")]
    PathNotFound {
        /// The configuration key.
        key: &'static str,
        /// The path that was checked.
        path: PathBuf,
    },

    /// Cross-field constraint violation.
    #[error("Configuration constraint violation: {message}")]
    ConstraintViolation {
        /// Description of the violated constraint.
        message: String,
        /// The fields involved in the constraint.
        fields: Vec<String>,
    },

    /// Unknown configuration key.
    #[error("Unknown configuration key '{key}' in section '{section}'")]
    UnknownKey {
        /// The section containing the unknown key.
        section: &'static str,
        /// The unknown key name.
        key: String,
    },
}

impl ConfigError {
    /// Whether this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        matches!(self, ConfigError::UnknownKey { .. })
    }

    /// A human-readable suggestion for resolving this error.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            ConfigError::FileNotFound { .. } => {
                "Create a configuration file at the expected path or use defaults."
            }
            ConfigError::ParseError { .. } => {
                "Fix the TOML syntax error at the indicated location."
            }
            ConfigError::EnvVarError { .. } => {
                "Set the environment variable to a valid value matching the expected type."
            }
            ConfigError::RangeError { .. } => "Adjust the value to be within the allowed range.",
            ConfigError::PathNotFound { .. } => "Create the required directory or file.",
            ConfigError::ConstraintViolation { .. } => {
                "Adjust the related configuration fields to satisfy the constraint."
            }
            ConfigError::UnknownKey { .. } => "Remove the unknown key or check for typos.",
        }
    }
}

// ---------------------------------------------------------------------------
// SchedulerError
// ---------------------------------------------------------------------------

/// Errors that occur during task scheduling, dispatch, or execution.
///
/// # Errors
///
/// Produced by `Scheduler` when a task cannot be spawned, is cancelled,
/// times out, or encounters an execution error.
///
/// # Recovery
///
/// Most scheduler errors are recoverable — the scheduler will continue
/// accepting and executing new tasks.
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    /// The scheduler is shutting down and not accepting new tasks.
    #[error("Scheduler is shutting down; cannot accept new tasks")]
    ShuttingDown,

    /// Task execution failed.
    #[error("Task '{task_id}' failed: {message}")]
    TaskFailed {
        /// The task identifier.
        task_id: String,
        /// Description of the failure.
        message: String,
    },

    /// Task timed out.
    #[error("Task '{task_id}' timed out after {timeout_secs}s")]
    TaskTimeout {
        /// The task identifier.
        task_id: String,
        /// The timeout duration in seconds.
        timeout_secs: u64,
    },

    /// Task was cancelled.
    #[error("Task '{task_id}' was cancelled")]
    TaskCancelled {
        /// The task identifier.
        task_id: String,
    },

    /// Maximum concurrent task limit reached.
    #[error("Maximum concurrent tasks ({max}) reached")]
    MaxConcurrencyReached {
        /// The maximum number of concurrent tasks allowed.
        max: u32,
    },

    /// Background worker exceeded max restart attempts.
    #[error("Background worker '{name}' exceeded max restarts ({attempts})")]
    WorkerRestartLimitExceeded {
        /// The worker name.
        name: String,
        /// Number of restart attempts.
        attempts: u32,
    },
}

impl SchedulerError {
    /// Whether this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        !matches!(self, SchedulerError::ShuttingDown)
    }

    /// A human-readable suggestion for resolving this error.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            SchedulerError::ShuttingDown => "Wait for the runtime to restart or retry later.",
            SchedulerError::TaskFailed { .. } => "Check the task logic for errors.",
            SchedulerError::TaskTimeout { .. } => "Increase the timeout or optimize the task.",
            SchedulerError::TaskCancelled { .. } => "The task was cancelled; re-spawn if needed.",
            SchedulerError::MaxConcurrencyReached { .. } => {
                "Increase the concurrency limit or wait for running tasks to complete."
            }
            SchedulerError::WorkerRestartLimitExceeded { .. } => {
                "Check the worker for recurring failures."
            }
        }
    }
}

// ---------------------------------------------------------------------------
// EventError
// ---------------------------------------------------------------------------

/// Errors that occur during event publishing or subscription.
///
/// # Errors
///
/// Produced by `EventBus` when a subscriber is lagged, the channel is full,
/// or a publish operation fails.
///
/// # Recovery
///
/// Event errors are typically transient and recoverable.
#[derive(Debug, thiserror::Error)]
pub enum EventError {
    /// Subscriber is too slow and missed events.
    #[error("Subscriber lagged on event type '{event_type}'; missed messages")]
    SubscriberLagged {
        /// The event type name.
        event_type: &'static str,
    },

    /// Event dropped because channel is full.
    #[error("Event dropped (channel full) for event type '{event_type}'")]
    ChannelFull {
        /// The event type name.
        event_type: &'static str,
    },

    /// Event type not registered.
    #[error("No subscribers registered for event type '{0}'")]
    NoSubscribers(&'static str),
}

impl EventError {
    /// Whether this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        true
    }

    /// A human-readable suggestion for resolving this error.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            EventError::SubscriberLagged { .. } => {
                "The subscriber is too slow; increase capacity or optimize the handler."
            }
            EventError::ChannelFull { .. } => {
                "Increase the channel capacity or reduce event publishing frequency."
            }
            EventError::NoSubscribers(_) => "This is informational; no action required.",
        }
    }
}

// ---------------------------------------------------------------------------
// HealthError
// ---------------------------------------------------------------------------

/// Errors that occur during health check execution.
///
/// # Errors
///
/// Produced by `HealthMonitor` when a health check fails to execute
/// or returns an unexpected result.
///
/// # Recovery
///
/// Health check errors are typically transient and the next check
/// interval may succeed.
#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    /// Health check timed out.
    #[error("Health check for '{service}' timed out after {timeout_secs}s")]
    CheckTimeout {
        /// The service name.
        service: String,
        /// The timeout duration in seconds.
        timeout_secs: u64,
    },

    /// Service not registered for health monitoring.
    #[error("Service '{0}' is not registered for health monitoring")]
    NotRegistered(String),
}

impl HealthError {
    /// Whether this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        true
    }

    /// A human-readable suggestion for resolving this error.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            HealthError::CheckTimeout { .. } => {
                "Increase the health check timeout or optimize the service."
            }
            HealthError::NotRegistered(_) => "Register the service with the health monitor.",
        }
    }
}

// ---------------------------------------------------------------------------
// ShutdownError
// ---------------------------------------------------------------------------

/// Errors that occur during runtime shutdown.
///
/// # Errors
///
/// Produced by `ShutdownManager` when a service fails to stop,
/// a task refuses to drain, or resources cannot be released.
///
/// # Recovery
///
/// Shutdown errors are logged but do not prevent the process from exiting.
#[derive(Debug, thiserror::Error)]
pub enum ShutdownError {
    /// Service failed to stop during shutdown.
    #[error("Service '{name}' failed to stop during shutdown: {message}")]
    ServiceStopFailed {
        /// The service name.
        name: String,
        /// Description of the failure.
        message: String,
    },

    /// Draining tasks timed out.
    #[error("Task drain timed out after {timeout_secs}s; {remaining} tasks remaining")]
    DrainTimeout {
        /// The drain timeout in seconds.
        timeout_secs: u64,
        /// Number of tasks that did not complete.
        remaining: u32,
    },

    /// Resource release failed.
    #[error("Failed to release resource '{resource}': {message}")]
    ResourceReleaseFailed {
        /// The resource identifier.
        resource: String,
        /// Description of the failure.
        message: String,
    },
}

impl ShutdownError {
    /// Whether this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        false
    }

    /// A human-readable suggestion for resolving this error.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            ShutdownError::ServiceStopFailed { .. } => "Force kill the process if shutdown hangs.",
            ShutdownError::DrainTimeout { .. } => "Increase the drain timeout or force shutdown.",
            ShutdownError::ResourceReleaseFailed { .. } => {
                "The resource will be cleaned up on next startup."
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ResourceError
// ---------------------------------------------------------------------------

/// Errors that occur when resource limits are exceeded.
///
/// # Errors
///
/// Produced by `ResourceManager` when a subsystem exceeds its configured
/// resource budget or when a global resource limit is violated.
///
/// # Recovery
///
/// Resource errors are recoverable — the runtime may trigger graceful
/// degradation to reduce resource usage.
#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    /// Exceeded the memory limit.
    #[error("Memory limit exceeded: {current_bytes}/{limit_bytes} bytes")]
    MemoryLimitExceeded {
        /// Current memory usage in bytes.
        current_bytes: u64,
        /// The configured limit in bytes.
        limit_bytes: u64,
    },

    /// Exceeded the task concurrency limit.
    #[error("Task limit exceeded: {current}/{limit} tasks")]
    TaskLimitExceeded {
        /// Current number of tasks.
        current: u32,
        /// The configured limit.
        limit: u32,
    },

    /// Exceeded the file handle limit.
    #[error("File handle limit exceeded: {current}/{limit}")]
    FileHandleLimitExceeded {
        /// Current number of open handles.
        current: u32,
        /// The configured limit.
        limit: u32,
    },

    /// A specific subsystem has exceeded its budget.
    #[error("Subsystem '{subsystem}' exceeded resource '{resource}' budget ({used}/{budget})")]
    SubsystemBudgetExceeded {
        /// The subsystem name.
        subsystem: String,
        /// The resource name (e.g., "memory", "cpu").
        resource: String,
        /// Current usage.
        used: f64,
        /// The configured budget.
        budget: f64,
    },
}

impl ResourceError {
    /// Whether this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        true
    }

    /// A human-readable suggestion for resolving this error.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            ResourceError::MemoryLimitExceeded { .. } => {
                "Reduce memory usage or increase the limit in configuration."
            }
            ResourceError::TaskLimitExceeded { .. } => {
                "Reduce task concurrency or increase the limit."
            }
            ResourceError::FileHandleLimitExceeded { .. } => {
                "Close unused file handles or increase the limit."
            }
            ResourceError::SubsystemBudgetExceeded { .. } => {
                "Reduce the subsystem's resource usage or increase its budget."
            }
        }
    }
}
