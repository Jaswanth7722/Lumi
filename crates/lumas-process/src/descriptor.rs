//! # Process Descriptor
//!
//! Static metadata describing a process. Defined at registration time
//! and immutable thereafter. The descriptor is the contract between the
//! process and the supervisor.
//!
//! # Thread Safety
//!
//! `ProcessDescriptor` is `Send + Sync`, `Clone`, and intended to be
//! shared via `Arc<ProcessDescriptor>`. Once registered, a descriptor
//! is never mutated.
//!
//! # Design
//!
//! The descriptor captures everything the supervisor needs to know to
//! manage a process: its identity, kind (internal service, child process,
//! WASM plugin, or worker), dependencies, declared capabilities, restart
//! policy, heartbeat configuration, resource limits, shutdown/startup
//! timeouts, and priority.

use crate::dependency::ProcessDependency;
use crate::heartbeat::HeartbeatConfig;
use crate::id::ProcessId;
use crate::resource::ResourceLimits;
use crate::restart::RestartPolicy;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// ProcessKind
// ---------------------------------------------------------------------------

/// The type of a managed process.
///
/// Determines how the process is launched and monitored:
/// - `InternalService`: an async Rust service on the Tokio runtime
/// - `ChildProcess`: an OS-level subprocess
/// - `WasmPlugin`: a plugin running in the WASM sandbox
/// - `Worker`: a background async task owned by a process
#[derive(Debug, Clone, Serialize)]
pub enum ProcessKind {
    /// An internal Rust async service implementing a factory function.
    InternalService {
        /// Factory function that creates the service future.
        /// Stores a type-erased constructor for async service start.
        #[serde(skip)]
        factory: Arc<dyn ServiceFactory>,
    },
    /// A child OS process spawned via `tokio::process::Command`.
    ChildProcess {
        /// Path to the executable.
        executable: PathBuf,
        /// Command-line arguments.
        args: Vec<String>,
        /// Environment variables.
        env: HashMap<String, String>,
        /// Optional working directory.
        working_dir: Option<PathBuf>,
    },
    /// A WASM plugin running inside `lumi-plugin-host`.
    WasmPlugin {
        /// The plugin identifier.
        plugin_id: String,
        /// Path to the WASM binary.
        wasm_path: PathBuf,
    },
    /// A background worker (async task on the Tokio runtime).
    Worker {
        /// Factory function that creates the worker future.
        #[serde(skip)]
        worker_fn: Arc<dyn WorkerFactory>,
    },
}

impl ProcessKind {
    /// Returns a human-readable type name for diagnostics.
    pub fn type_name(&self) -> &'static str {
        match self {
            ProcessKind::InternalService { .. } => "internal_service",
            ProcessKind::ChildProcess { .. } => "child_process",
            ProcessKind::WasmPlugin { .. } => "wasm_plugin",
            ProcessKind::Worker { .. } => "worker",
        }
    }
}

// ---------------------------------------------------------------------------
// ProcessPriority
// ---------------------------------------------------------------------------

/// Priority level for a managed process.
///
/// Affects startup ordering within the same dependency tier and
/// the order of shutdown. Higher-priority processes start first
/// and stop last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProcessPriority {
    /// Root supervisor, Core Runtime, Storage.
    Critical = 0,
    /// Render, Voice, AI Core.
    High = 1,
    /// Plugin Host, Workers.
    Normal = 2,
    /// Optional plugins, diagnostics workers.
    Low = 3,
}

impl ProcessPriority {
    /// All priority levels in order.
    pub const ALL: &'static [ProcessPriority] = &[
        ProcessPriority::Critical,
        ProcessPriority::High,
        ProcessPriority::Normal,
        ProcessPriority::Low,
    ];
}

impl std::fmt::Display for ProcessPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessPriority::Critical => write!(f, "critical"),
            ProcessPriority::High => write!(f, "high"),
            ProcessPriority::Normal => write!(f, "normal"),
            ProcessPriority::Low => write!(f, "low"),
        }
    }
}

// ---------------------------------------------------------------------------
// ProcessDescriptor
// ---------------------------------------------------------------------------

/// Static metadata describing a process. Defined at registration time
/// and immutable thereafter.
///
/// The descriptor is the contract between the process and the supervisor.
/// It declares everything the supervisor needs to manage the process
/// lifecycle, enforce capabilities, monitor health, and apply restart
/// policies.
///
/// # Thread Safety
///
/// `ProcessDescriptor` is `Send + Sync` and `Clone`. In practice, it is
/// wrapped in `Arc<ProcessDescriptor>` and shared across threads.
///
/// # Examples
///
/// ```ignore
/// use lumas_process::{
///     ProcessId, ProcessDescriptor, ProcessKind, ProcessPriority,
///     RestartPolicy, HeartbeatConfig, ResourceLimits,
/// };
///
/// let desc = ProcessDescriptor::new(
///     ProcessId::new("lumi.core"),
///     "ai-core",
///     semver::Version::new(1, 0, 0),
///     ProcessKind::InternalService { factory: /* ... */ },
/// ).with_priority(ProcessPriority::High);
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct ProcessDescriptor {
    /// Unique process identifier.
    pub id: ProcessId,
    /// Human-readable name.
    pub name: String,
    /// Semantic version of this process.
    pub version: semver::Version,
    /// The type of process.
    pub kind: ProcessKind,
    /// Declared dependencies on other processes.
    pub dependencies: Vec<ProcessDependency>,
    /// Declared capabilities (from SRS capability model).
    pub capabilities: Vec<String>,
    /// Restart policy for crash recovery.
    pub restart_policy: RestartPolicy,
    /// Heartbeat configuration for liveness monitoring.
    pub heartbeat: HeartbeatConfig,
    /// Resource limits enforced by the resource monitor.
    pub resources: ResourceLimits,
    /// Graceful shutdown timeout in milliseconds (default: 10_000).
    pub shutdown_timeout_ms: u64,
    /// Startup timeout in milliseconds (default: 30_000).
    pub startup_timeout_ms: u64,
    /// Process priority for startup/shutdown ordering.
    pub priority: ProcessPriority,
    /// When this descriptor was created.
    pub created_at: DateTime<Utc>,
}

impl ProcessDescriptor {
    /// Create a new process descriptor with default values.
    ///
    /// Defaults:
    /// - `dependencies`: empty
    /// - `capabilities`: empty
    /// - `restart_policy`: `RestartPolicy::Immediate { max_restarts: 3, window_secs: 60 }`
    /// - `heartbeat`: `HeartbeatConfig::default()`
    /// - `resources`: `ResourceLimits::default()`
    /// - `shutdown_timeout_ms`: 10_000
    /// - `startup_timeout_ms`: 30_000
    /// - `priority`: `ProcessPriority::Normal`
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ProcessId,
        name: impl Into<String>,
        version: semver::Version,
        kind: ProcessKind,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            version,
            kind,
            dependencies: Vec::new(),
            capabilities: Vec::new(),
            restart_policy: RestartPolicy::Immediate {
                max_restarts: 3,
                window_secs: 60,
            },
            heartbeat: HeartbeatConfig::default(),
            resources: ResourceLimits::default(),
            shutdown_timeout_ms: 10_000,
            startup_timeout_ms: 30_000,
            priority: ProcessPriority::Normal,
            created_at: Utc::now(),
        }
    }

    /// Builder pattern: set dependencies.
    pub fn with_dependencies(mut self, deps: Vec<ProcessDependency>) -> Self {
        self.dependencies = deps;
        self
    }

    /// Builder pattern: set capabilities.
    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }

    /// Builder pattern: set restart policy.
    pub fn with_restart_policy(mut self, policy: RestartPolicy) -> Self {
        self.restart_policy = policy;
        self
    }

    /// Builder pattern: set heartbeat config.
    pub fn with_heartbeat(mut self, hb: HeartbeatConfig) -> Self {
        self.heartbeat = hb;
        self
    }

    /// Builder pattern: set resource limits.
    pub fn with_resources(mut self, rl: ResourceLimits) -> Self {
        self.resources = rl;
        self
    }

    /// Builder pattern: set shutdown timeout.
    pub fn with_shutdown_timeout(mut self, ms: u64) -> Self {
        self.shutdown_timeout_ms = ms;
        self
    }

    /// Builder pattern: set startup timeout.
    pub fn with_startup_timeout(mut self, ms: u64) -> Self {
        self.startup_timeout_ms = ms;
        self
    }

    /// Builder pattern: set priority.
    pub fn with_priority(mut self, p: ProcessPriority) -> Self {
        self.priority = p;
        self
    }
}

// ---------------------------------------------------------------------------
// ServiceFactory
// ---------------------------------------------------------------------------

/// A type-erased factory for creating internal service futures.
///
/// Implementors provide a `create()` method that returns a boxed future
/// representing the service's main async entry point. The supervisor
/// spawns this future on the Tokio runtime.
pub trait ServiceFactory: Send + Sync + 'static {
    /// Create the service's async entry point.
    ///
    /// The returned future should run until the service is stopped
    /// (via cancellation token, signal, or normal completion).
    fn create(&self) -> Pin<Box<dyn Future<Output = ()> + Send>>;

    /// Optional human-readable name for diagnostics.
    fn name(&self) -> &'static str {
        "internal_service"
    }
}

impl std::fmt::Debug for dyn ServiceFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceFactory")
            .field("name", &self.name())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// WorkerFactory
// ---------------------------------------------------------------------------

/// A type-erased factory for creating background worker futures.
pub trait WorkerFactory: Send + Sync + 'static {
    /// The worker's name for diagnostics.
    fn name(&self) -> &'static str;

    /// Create the worker's async entry point.
    fn create(&self) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

impl std::fmt::Debug for dyn WorkerFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerFactory")
            .field("name", &self.name())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptor_defaults() {
        let id = ProcessId::new("test.service");
        let desc = ProcessDescriptor::new(
            id.clone(),
            "test",
            semver::Version::new(1, 0, 0),
            ProcessKind::Worker {
                worker_fn: Arc::new(|| -> Pin<Box<dyn Future<Output = ()> + Send>> {
                    Box::pin(async {})
                }),
            },
        );
        assert_eq!(desc.id.path(), "test.service");
        assert_eq!(desc.shutdown_timeout_ms, 10_000);
        assert_eq!(desc.startup_timeout_ms, 30_000);
        assert_eq!(desc.priority, ProcessPriority::Normal);
    }

    #[test]
    fn test_builder_pattern() {
        let id = ProcessId::new("test");
        let desc = ProcessDescriptor::new(id, "test", semver::Version::new(1, 0, 0), {
            let factory = || -> Pin<Box<dyn Future<Output = ()> + Send>> { Box::pin(async {}) };
            ProcessKind::Worker {
                worker_fn: Arc::new(factory),
            }
        })
        .with_priority(ProcessPriority::High)
        .with_shutdown_timeout(30_000);
        assert_eq!(desc.priority, ProcessPriority::High);
        assert_eq!(desc.shutdown_timeout_ms, 30_000);
    }

    #[test]
    fn test_process_kind_type_names() {
        let factory = || -> Pin<Box<dyn Future<Output = ()> + Send>> { Box::pin(async {}) };
        assert_eq!(
            ProcessKind::Worker {
                worker_fn: Arc::new(factory)
            }
            .type_name(),
            "worker"
        );
    }

    #[test]
    fn test_priority_ordering() {
        assert!(ProcessPriority::Critical < ProcessPriority::High);
        assert!(ProcessPriority::High < ProcessPriority::Normal);
        assert!(ProcessPriority::Normal < ProcessPriority::Low);
    }
}
