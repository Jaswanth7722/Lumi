//! # Lumas Process Management System
//!
//! The authoritative orchestration layer for the entire Lumas platform.
//!
//! This crate provides a complete, production-grade process management system
//! inspired by the Erlang/OTP supervisor model. It manages the entire lifecycle
//! of every Lumas process: internal async services, OS child processes, WASM
//! plugin sandboxes, and background workers.
//!
//! ## Architecture
//!
//! ```text
//! lumi (root supervisor)
//! ├── lumi-core         (AI Core, Planning, Memory, State Machine, Tool Framework)
//! ├── lumas-render       (Character Engine, Animation, Physics, Workspace UI)
//! ├── lumi-voice        (Wake Word, STT, TTS, Audio I/O)
//! ├── lumas-storage      (Memory Store, Config Store, Asset Cache)
//! └── lumi-plugin-host  (Plugin Registry, WASM Sandbox, Capability Broker)
//!     ├── plugin:<id>   (one WASM sandbox per loaded plugin)
//!     └── ...
//! ```
//!
//! ## Core Concepts
//!
//! - **ProcessId**: Hierarchical dot-notation identifiers (`lumi.render.animation`)
//! - **ProcessState**: 14-state typed state machine enforcing legal transitions
//! - **Supervisor**: OTP-inspired supervision with OneForOne/OneForAll/RestForOne strategies
//! - **DependencyGraph**: petgraph-based DAG with cycle detection and topological sort
//! - **HeartbeatManager**: Lock-free liveness detection via AtomicI64 timestamps
//! - **CapabilityRegistry**: Compile-time constant exclusive capabilities
//! - **RestartEngine**: Sliding-window restart counting with exponential backoff and jitter
//!
//! ## WORKSPACE AUDIT
//!
//! Before implementing this crate, the following existing infrastructure was
//! identified and extended (not duplicated):
//!
//! ### lumas-runtime (crates/lumas-runtime/)
//! - **Service trait** (`crates/lumas-runtime/src/service.rs`): `#[async_trait] pub trait Service`
//!   with `start()`, `stop()`, `health_check()`, `dependencies()`, `version()` methods.
//!   `ServiceManager` provides register/resolve/start/stop with dependency ordering (Kahn's algorithm).
//!   This process system is a *complement* to ServiceManager — it manages the OS-level and
//!   supervision-tree concerns that ServiceManager does not address.
//! - **EventBus** (`crates/lumas-runtime/src/event.rs`): Typed broadcast event bus with
//!   `Event` trait requiring `Send + Sync + Clone + Debug + 'static` plus `event_type()`.
//!   Process events implement this trait for full integration.
//! - **HealthMonitor** (`crates/lumas-runtime/src/health.rs`): Periodic health checks via
//!   `Service::health_check()`. Heartbeat-based liveness is provided by this crate instead.
//! - **Scheduler** (`crates/lumas-runtime/src/scheduler.rs`): Async task scheduler with
//!   priorities, cancellation, and background workers. `WorkerManager` uses it for dispatch.
//! - **MetricsRegistry** (`crates/lumas-runtime/src/metrics.rs`): Atomic counter/gauge/histogram
//!   registry. `ProcessMetrics::register_with()` integrates with it.
//! - **RuntimeContext** (`crates/lumas-runtime/src/context.rs`): Shared context with `ArcSwap`
//!   config, event bus, scheduler, health monitor, etc. `ProcessManager` is stored here.
//! - **LifecycleManager** (`crates/lumas-runtime/src/lifecycle.rs`): Runtime-level lifecycle
//!   (Uninitialized → Bootstrapping → Running → ShuttingDown → Stopped).
//!   Process-level lifecycle is separate and managed by `ProcessStateMachine`.
//! - **ShutdownManager** (`crates/lumas-runtime/src/shutdown.rs`): Graceful shutdown sequence.
//!   Calls `ProcessManager::stop_all()` during the StoppingServices phase.
//!
//! ### lumi-logging (crates/lumi-logging/)
//! - **LogManager** (`crates/lumi-logging/src/manager.rs`): Global tracing subscriber installed
//!   once at bootstrap. All process lifecycle events are logged via tracing macros.
//! - **LoggingMetrics** (`crates/lumi-logging/src/metrics.rs`): Atomic logging metrics.
//!
//! ### Design Decisions
//!
//! - Process management is a *new subsystem* that complements (does not replace)
//!   lumas-runtime's ServiceManager. The supervisor manages OS processes and internal
//!   services uniformly through `ProcessDescriptor`, while ServiceManager handles
//!   lumas-runtime-specific service lifecycle.
//! - Platform-specific process operations (signal handling, job objects) use
//!   conditional compilation with `#[cfg(unix)]` and `#[cfg(windows)]`.
//! - Heartbeat monitoring uses `AtomicI64` timestamps for lock-free hot-path performance.
//! - The dependency graph uses `petgraph` for deterministic cycle detection and
//!   topological sort, supporting Mermaid export for diagnostics.

// Public modules
pub mod capability;
pub mod config;
pub mod dependency;
pub mod descriptor;
pub mod diagnostics;
pub mod error;
pub mod event;
pub mod handle;
pub mod heartbeat;
pub mod id;
pub mod launcher;
pub mod lifecycle;
pub mod manager;
pub mod metrics;
pub mod monitor;
pub mod platform;
pub mod registry;
pub mod resource;
pub mod restart;
pub mod supervisor;
pub mod worker;

// Public re-exports for convenience
pub use capability::CapabilityRegistry;
pub use config::ProcessManagementConfig;
pub use dependency::{DependencyEdge, DependencyGraph, ProcessDependency};
pub use descriptor::{ProcessDescriptor, ProcessKind, ProcessPriority, ServiceFactory, WorkerFactory};
pub use diagnostics::{CrashReport, ProcessDiagnostics};
pub use error::{ExitStatus, ProcessError};
pub use event::{
    CapabilityViolation, HeartbeatMissed, HeartbeatRecovered, ProcessCrashed, ProcessFailed,
    ProcessRegistered, ProcessRestarted, ProcessStarted, ProcessStopped, SupervisorIntervention,
};
pub use handle::{ProcessCommand, ProcessHandle};
pub use heartbeat::{HeartbeatConfig, HeartbeatManager, HeartbeatMetadata, HeartbeatSignal};
pub use id::{ProcessId, WorkerId};
pub use launcher::ProcessLauncher;
pub use lifecycle::{ProcessState, ProcessStateMachine, StateTransitionRecord};
pub use manager::ProcessManager;
pub use metrics::{ProcessInstanceMetrics, ProcessMetrics, ProcessMetricsSnapshot};
pub use registry::ProcessRegistry;
pub use resource::{ResourceLimits, ResourceMonitor, ResourceSnapshot};
pub use restart::{RestartAction, RestartEngine, RestartPolicy, RestartRecord};
pub use supervisor::{SupervisionStrategy, Supervisor};
pub use worker::{WorkerHandle, WorkerManager, WorkerState};

/// The current version of the process management system.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
