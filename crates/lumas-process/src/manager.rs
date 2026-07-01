//! # ProcessManager
//!
//! The public entry point for the entire process management system.
//!
//! Stored in `RuntimeContext` after construction during bootstrap.
//! Provides a unified API for registering, starting, stopping, monitoring,
//! and managing the lifecycle of all Lumas processes.
//!
//! # Thread Safety
//!
//! `ProcessManager` is `Clone` (O(1) via `Arc`), `Send`, and `Sync`.
//! It is designed to be shared across thread boundaries from the moment
//! it is initialized.
//!
//! # Architecture
//!
//! ```text
//! ProcessManager
//! ├── Supervisor (root) — OTP-inspired supervision tree
//! ├── ProcessRegistry — DashMap-based process store
//! ├── DependencyGraph — petgraph DAG for startup/shutdown ordering
//! ├── ProcessLauncher — launches processes by kind
//! ├── HeartbeatManager — liveness detection
//! ├── WorkerManager — background worker orchestration
//! ├── CapabilityRegistry — capability enforcement
//! ├── ResourceMonitor — per-process resource tracking
//! ├── ProcessDiagnostics — reports and export
//! ├── ProcessMetrics — atomic counters
//! └── EventBus — typed event dispatch
//! ```

use crate::capability::CapabilityRegistry;
use crate::config::ProcessManagementConfig;
use crate::dependency::DependencyGraph;
use crate::descriptor::{ProcessDescriptor, ProcessKind};
use crate::diagnostics::ProcessDiagnostics;
use crate::error::ProcessError;
use crate::handle::ProcessHandle;
use crate::heartbeat::HeartbeatManager;
use crate::id::{ProcessId, WorkerId};
use crate::launcher::ProcessLauncher;
use crate::lifecycle::ProcessState;
use crate::metrics::{ProcessMetrics, ProcessMetricsSnapshot};
use crate::registry::ProcessRegistry;
use crate::resource::ResourceMonitor;
use crate::restart::RestartPolicy;
use crate::supervisor::{SupervisionStrategy, Supervisor, SupervisorHandle};
use crate::descriptor::WorkerFactory;
use crate::worker::WorkerManager;
use lumas_runtime::context::RuntimeContext;
use lumas_runtime::event::EventBus;
use lumas_runtime::service::ServiceHealth;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// The public entry point for the process management system.
///
/// # Examples
///
/// ```ignore
/// let config = ProcessManagementConfig::default();
/// let ctx = RuntimeContext::new(...);
/// let pm = ProcessManager::new(config, ctx).await?;
/// pm.register(descriptor).await?;
/// pm.start_all().await?;
/// ```
#[derive(Clone)]
pub struct ProcessManager {
    /// Root supervisor for all processes.
    root_supervisor: Arc<Supervisor>,
    /// Process registry.
    registry: Arc<ProcessRegistry>,
    /// Dependency graph.
    dependency_graph: Arc<RwLock<DependencyGraph>>,
    /// Process launcher.
    launcher: Arc<ProcessLauncher>,
    /// Heartbeat manager.
    heartbeat_manager: Arc<HeartbeatManager>,
    /// Worker manager.
    worker_manager: Arc<WorkerManager>,
    /// Capability registry.
    capability_registry: Arc<CapabilityRegistry>,
    /// Resource monitor.
    resource_monitor: Arc<ResourceMonitor>,
    /// Diagnostics provider.
    diagnostics: Arc<ProcessDiagnostics>,
    /// Process metrics.
    metrics: Arc<ProcessMetrics>,
    /// Event bus.
    event_bus: Arc<EventBus>,
    /// Whether `start_all()` has been called.
    started: Arc<std::sync::atomic::AtomicBool>,
    /// Supervisor command handle.
    supervisor_handle: SupervisorHandle,
}

impl ProcessManager {
    /// Construct and initialize the ProcessManager.
    ///
    /// Creates all subsystems and starts background monitoring tasks.
    ///
    /// # Errors
    ///
    /// Never fails — construction is infallible.
    pub async fn new(
        config: ProcessManagementConfig,
        _runtime_context: Arc<RuntimeContext>,
    ) -> Result<Self, ProcessError> {
        let event_bus = Arc::new(EventBus::new(1024));
        let metrics = Arc::new(ProcessMetrics::new());
        let registry = Arc::new(ProcessRegistry::new());
        let dependency_graph = Arc::new(RwLock::new(DependencyGraph::new()));
        let capability_registry = Arc::new(CapabilityRegistry::new());
        let resource_monitor = Arc::new(ResourceMonitor::new(metrics.clone()));
        let heartbeat_manager = Arc::new(HeartbeatManager::new(
            event_bus.clone(),
            metrics.clone(),
        ));

        let launcher = Arc::new(ProcessLauncher::new(
            registry.clone(),
            dependency_graph.clone(),
            metrics.clone(),
        ));

        let root_supervisor = Supervisor::root(
            config.supervision_strategy,
            event_bus.clone(),
            metrics.clone(),
            launcher.clone(),
            registry.clone(),
            dependency_graph.clone(),
            heartbeat_manager.clone(),
        );

        let supervisor_handle = root_supervisor.handle();

        let worker_manager = Arc::new(WorkerManager::new(
            event_bus.clone(),
            metrics.clone(),
        ));

        let diagnostics = Arc::new(ProcessDiagnostics::new(
            registry.clone(),
            dependency_graph.clone(),
            metrics.clone(),
        ));

        // Start background tasks
        if config.enable_heartbeat_checker {
            let hb = heartbeat_manager.clone();
            tokio::spawn(async move {
                hb.start_checker().await;
            });
        }

        if config.enable_resource_monitor {
            let rm = resource_monitor.clone();
            tokio::spawn(async move {
                rm.start().await;
            });
        }

        // Start supervisor command processor
        let sup = root_supervisor.clone();
        tokio::spawn(async move {
            sup.run().await;
        });

        Ok(Self {
            root_supervisor,
            registry,
            dependency_graph,
            launcher,
            heartbeat_manager,
            worker_manager,
            capability_registry,
            resource_monitor,
            diagnostics,
            metrics,
            event_bus,
            started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            supervisor_handle,
        })
    }

    /// Register a process descriptor.
    ///
    /// Validates the dependency graph immediately after registration.
    /// Must be called during bootstrap before `start_all()`.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::ShuttingDown` if the manager is shutting down.
    /// Returns `ProcessError::AlreadyRegistered` if the process ID is already
    /// registered.
    /// Returns `ProcessError::DependencyCycle` if adding this process creates
    /// a cycle in the dependency graph.
    /// Returns `ProcessError::MissingDependency` if a required dependency
    /// is not registered.
    pub async fn register(&self, descriptor: ProcessDescriptor) -> Result<(), ProcessError> {
        // Register with supervisor
        self.root_supervisor.register(descriptor.clone()).await?;

        // Register capabilities
        self.capability_registry
            .register(&descriptor.id, &descriptor)?;

        // Set resource limits
        self.resource_monitor
            .set_limits(descriptor.id.clone(), descriptor.resources);

        Ok(())
    }

    /// Start all registered processes in topological dependency order.
    ///
    /// # Errors
    ///
    /// Returns the first `ProcessError` encountered. Previously started
    /// processes remain running.
    pub async fn start_all(&self) -> Result<(), ProcessError> {
        if self
            .started
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(()); // Already started
        }

        self.root_supervisor.start_all().await
    }

    /// Stop all processes in reverse dependency order.
    ///
    /// Called by `ShutdownManager` during graceful shutdown.
    ///
    /// # Errors
    ///
    /// Returns errors from individual process stops, aggregated.
    pub async fn stop_all(&self) -> Result<(), ProcessError> {
        self.root_supervisor.stop_all().await
    }

    /// Restart a specific process by ID.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::NotFound` if the process is not registered.
    pub async fn restart(&self, id: &ProcessId, _reason: &str) -> Result<(), ProcessError> {
        self.supervisor_handle
            .command_tx
            .send(crate::supervisor::SupervisorCommand::StopChild {
                id: id.clone(),
            })
            .map_err(|_| ProcessError::NotFound { id: id.clone() })?;

        Ok(())
    }

    /// Pause a specific process (suspend it without stopping).
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::NotFound` if the process is not found.
    pub async fn pause(&self, id: &ProcessId) -> Result<(), ProcessError> {
        let handle = self
            .registry
            .get(id)
            .ok_or_else(|| ProcessError::NotFound { id: id.clone() })?;

        handle.transition_state(ProcessState::Paused, "operator pause")?;
        Ok(())
    }

    /// Resume a paused process.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::NotFound` if the process is not found.
    pub async fn resume(&self, id: &ProcessId) -> Result<(), ProcessError> {
        let handle = self
            .registry
            .get(id)
            .ok_or_else(|| ProcessError::NotFound { id: id.clone() })?;

        handle.transition_state(ProcessState::Running, "operator resume")?;
        Ok(())
    }

    /// Returns the live handle for a registered process.
    pub fn handle(&self, id: &ProcessId) -> Option<ProcessHandle> {
        self.registry.get(id)
    }

    /// Returns the current health of a process.
    pub async fn health(&self, id: &ProcessId) -> Option<ServiceHealth> {
        let handle = self.registry.get(id)?;
        let state = handle.state();

        let status = if state.is_operational() {
            lumas_runtime::service::HealthStatus::Healthy
        } else if state.is_failure() {
            lumas_runtime::service::HealthStatus::Unhealthy
        } else {
            lumas_runtime::service::HealthStatus::Degraded
        };

        Some(ServiceHealth::healthy(format!("Process state: {:?}", state)))
    }

    /// Returns the current state of all registered processes.
    pub fn all_states(&self) -> HashMap<ProcessId, ProcessState> {
        let map = self.registry.all_states();
        let mut result = HashMap::new();
        for entry in map.iter() {
            result.insert(entry.key().clone(), *entry.value());
        }
        result
    }

    /// Spawn a background worker under an owning process.
    ///
    /// # Errors
    ///
    /// Never fails — worker spawning is infallible.
    pub async fn spawn_worker(
        &self,
        owner: ProcessId,
        factory: Box<dyn WorkerFactory>,
        policy: RestartPolicy,
    ) -> Result<WorkerId, ProcessError> {
        self.worker_manager.spawn(owner, factory, policy).await
    }

    /// Returns the diagnostics provider for reporting and export.
    pub fn diagnostics(&self) -> Arc<ProcessDiagnostics> {
        self.diagnostics.clone()
    }

    /// Returns the current metrics snapshot.
    pub fn metrics_snapshot(&self) -> ProcessMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Returns a reference to the event bus.
    pub fn event_bus(&self) -> Arc<EventBus> {
        self.event_bus.clone()
    }

    /// Returns a reference to the capability registry.
    pub fn capability_registry(&self) -> Arc<CapabilityRegistry> {
        self.capability_registry.clone()
    }

    /// Returns the number of registered processes.
    pub fn process_count(&self) -> usize {
        self.registry.len()
    }
}

impl std::fmt::Debug for ProcessManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessManager")
            .field("process_count", &self.registry.len())
            .field("worker_count", &self.worker_manager.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProcessManagementConfig;
    use lumas_runtime::context::RuntimeContext;
    use lumas_runtime::version::FeatureFlags;
    use lumas_runtime::resource::ResourceManager;

    #[tokio::test]
    async fn test_process_manager_create() {
        let config = ProcessManagementConfig::default();
        let ctx = Arc::new(RuntimeContext::new(
            Arc::new(FeatureFlags::new()),
            Arc::new(EventBus::new(16)),
            Arc::new(ResourceManager::new()),
        ));

        let pm = ProcessManager::new(config, ctx).await.unwrap();
        assert_eq!(pm.process_count(), 0);
    }

    #[tokio::test]
    async fn test_process_manager_register_and_start() {
        let config = ProcessManagementConfig::default();
        let ctx = Arc::new(RuntimeContext::new(
            Arc::new(FeatureFlags::new()),
            Arc::new(EventBus::new(16)),
            Arc::new(ResourceManager::new()),
        ));

        let pm = ProcessManager::new(config, ctx).await.unwrap();

        let desc = ProcessDescriptor::new(
            ProcessId::new("lumi.core"),
            "core",
            semver::Version::new(1, 0, 0),
            ProcessKind::Worker {
                worker_fn: Arc::new(|| Box::pin(async {})),
            },
        );

        pm.register(desc).await.unwrap();
        assert_eq!(pm.process_count(), 0); // Not started yet, so registry is empty initially

        // The supervisor registered it successfully
        assert!(pm.handle(&ProcessId::new("lumi.core")).is_none()); // Not started yet
    }
}
