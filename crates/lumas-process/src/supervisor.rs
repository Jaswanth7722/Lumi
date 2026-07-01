//! # Supervisor — OTP-Inspired Supervision Tree
//!
//! Implements the Erlang/OTP supervisor pattern adapted for Rust.
//!
//! Each supervisor node manages a set of child processes and applies
//! restart strategies when a child fails. The root supervisor owns
//! all top-level Lumas processes (lumi-core, lumas-render, lumi-voice,
//! lumas-storage, lumi-plugin-host).
//!
//! # Supervision Strategies
//!
//! - **OneForOne**: Restart only the failed child.
//! - **OneForAll**: Restart all children when one fails.
//! - **RestForOne**: Restart the failed child and all started after it.
//! - **EscalateToParent**: Do not restart; escalate to parent supervisor.
//!
//! # Thread Safety
//!
//! `Supervisor` is `Send + Sync` and designed to be shared via `Arc`.
//! Child state is stored in a `DashMap` for concurrent access.

use crate::dependency::DependencyGraph;
use crate::descriptor::ProcessDescriptor;
use crate::error::{ExitStatus, ProcessError};
use crate::handle::ProcessHandle;
use crate::heartbeat::HeartbeatManager;
use crate::id::ProcessId;
use crate::launcher::ProcessLauncher;
use crate::lifecycle::ProcessState;
use crate::metrics::ProcessMetrics;
use crate::registry::ProcessRegistry;
use crate::restart::{RestartAction, RestartEngine, RestartPolicy, RestartRecord};
use dashmap::DashMap;
use lumas_runtime::event::EventBus;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use super::event::{
    ProcessCrashed, ProcessFailed, ProcessRestarted, SupervisorIntervention,
};

// ---------------------------------------------------------------------------
// SupervisionStrategy
// ---------------------------------------------------------------------------

/// Strategy controlling how a supervisor responds to child failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SupervisionStrategy {
    /// Restart only the failed child. Other children continue unaffected.
    OneForOne,
    /// When one child fails, stop and restart ALL children in order.
    OneForAll,
    /// When one child fails, stop and restart it and all children
    /// started after it (those that may depend on it).
    RestForOne,
    /// Do not restart on failure. Escalate to parent supervisor.
    EscalateToParent,
}

impl SupervisionStrategy {
    /// All strategy variants.
    pub const ALL: &'static [SupervisionStrategy] = &[
        SupervisionStrategy::OneForOne,
        SupervisionStrategy::OneForAll,
        SupervisionStrategy::RestForOne,
        SupervisionStrategy::EscalateToParent,
    ];
}

impl std::fmt::Display for SupervisionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SupervisionStrategy::OneForOne => write!(f, "one_for_one"),
            SupervisionStrategy::OneForAll => write!(f, "one_for_all"),
            SupervisionStrategy::RestForOne => write!(f, "rest_for_one"),
            SupervisionStrategy::EscalateToParent => write!(f, "escalate_to_parent"),
        }
    }
}

// ---------------------------------------------------------------------------
// SupervisedChild
// ---------------------------------------------------------------------------

/// Internal tracking for a child managed by a supervisor.
struct SupervisedChild {
    /// Handle to the running process.
    handle: ProcessHandle,
    /// Process descriptor.
    descriptor: Arc<ProcessDescriptor>,
    /// Restart tracking record.
    restart_record: RestartRecord,
    /// Join handle for the child watcher task.
    watcher: Option<JoinHandle<()>>,
}

// ---------------------------------------------------------------------------
// SupervisorCommand
// ---------------------------------------------------------------------------

/// Commands sent to the supervisor from watcher tasks and heartbeat checker.
#[derive(Debug)]
pub enum SupervisorCommand {
    /// A child process exited cleanly or with a status.
    ChildExited {
        /// The process that exited.
        id: ProcessId,
        /// The exit status.
        exit_status: ExitStatus,
    },
    /// A child process failure was detected.
    ChildFailure {
        /// The process that failed.
        id: ProcessId,
        /// The error describing the failure.
        error: Box<ProcessError>,
    },
    /// Heartbeat timeout detected.
    HeartbeatTimeout {
        /// The process that timed out.
        id: ProcessId,
        /// Milliseconds since last heartbeat.
        elapsed_ms: u64,
    },
    /// Stop a child process.
    StopChild {
        /// The process to stop.
        id: ProcessId,
    },
}

// ---------------------------------------------------------------------------
// SupervisorHandle
// ---------------------------------------------------------------------------

/// A handle to communicate with a running supervisor.
#[derive(Clone)]
pub struct SupervisorHandle {
    /// Sender for supervisor commands.
    pub command_tx: mpsc::UnboundedSender<SupervisorCommand>,
    /// The supervisor's process ID.
    pub id: ProcessId,
}

// ---------------------------------------------------------------------------
// Supervisor
// ---------------------------------------------------------------------------

/// A supervisor manages a set of child processes according to a
/// `SupervisionStrategy`.
///
/// # Examples
///
/// ```ignore
/// let supervisor = Supervisor::root(
///     SupervisionStrategy::OneForOne,
///     event_bus,
///     metrics,
///     launcher,
/// );
/// supervisor.register(descriptor).await?;
/// supervisor.start_all().await?;
/// ```
pub struct Supervisor {
    /// Supervisor's own process ID.
    id: ProcessId,
    /// Supervision strategy for child failures.
    strategy: SupervisionStrategy,
    /// Managed children.
    children: DashMap<ProcessId, SupervisedChild>,
    /// Restart engine for policy decisions.
    restart_engine: RestartEngine,
    /// Event bus for emitting supervision events.
    event_bus: Arc<EventBus>,
    /// Process metrics.
    metrics: Arc<ProcessMetrics>,
    /// Process launcher for starting children.
    launcher: Arc<ProcessLauncher>,
    /// Process registry.
    registry: Arc<ProcessRegistry>,
    /// Dependency graph.
    dependency_graph: Arc<RwLock<DependencyGraph>>,
    /// Heartbeat manager.
    heartbeat_manager: Arc<HeartbeatManager>,
    /// Command channel for supervisor communication.
    command_tx: mpsc::UnboundedSender<SupervisorCommand>,
    /// Command channel receiver.
    command_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<SupervisorCommand>>>,
}

impl Supervisor {
    /// Create the root supervisor (owns all top-level Lumas processes).
    #[allow(clippy::too_many_arguments)]
    pub fn root(
        strategy: SupervisionStrategy,
        event_bus: Arc<EventBus>,
        metrics: Arc<ProcessMetrics>,
        launcher: Arc<ProcessLauncher>,
        registry: Arc<ProcessRegistry>,
        dependency_graph: Arc<RwLock<DependencyGraph>>,
        heartbeat_manager: Arc<HeartbeatManager>,
    ) -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            id: ProcessId::root(),
            strategy,
            children: DashMap::new(),
            restart_engine: RestartEngine::new(),
            event_bus,
            metrics,
            launcher,
            registry,
            dependency_graph,
            heartbeat_manager,
            command_tx: tx,
            command_rx: Arc::new(tokio::sync::Mutex::new(rx)),
        })
    }

    /// Returns a handle for communicating with this supervisor.
    pub fn handle(&self) -> SupervisorHandle {
        SupervisorHandle {
            command_tx: self.command_tx.clone(),
            id: self.id.clone(),
        }
    }

    /// Register a child process descriptor with this supervisor.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::AlreadyRegistered` if the process is already
    /// registered as a child of this supervisor.
    pub async fn register(
        self: &Arc<Self>,
        descriptor: ProcessDescriptor,
    ) -> Result<(), ProcessError> {
        let id = descriptor.id.clone();

        if self.children.contains_key(&id) {
            return Err(ProcessError::AlreadyRegistered { id });
        }

        // Add to dependency graph
        {
            let mut graph = self.dependency_graph.write();
            graph.add_process(id.clone())?;
            for dep in &descriptor.dependencies {
                graph.add_process(dep.id.clone())?;
                graph.add_dependency(
                    id.clone(),
                    dep.id.clone(),
                    crate::dependency::DependencyEdge {
                        version_req: dep.version_req.clone(),
                        required: dep.required,
                        startup_order: dep.startup_order,
                    },
                )?;
            }
        }

        // Store child metadata
        let descriptor_arc = Arc::new(descriptor);
        let child = SupervisedChild {
            handle: ProcessHandle::dummy(&id), // Will be replaced on start
            descriptor: descriptor_arc,
            restart_record: RestartRecord::new(),
            watcher: None,
        };

        self.children.insert(id.clone(), child);
        debug!("Child registered: {}", id);
        Ok(())
    }

    /// Start all registered children in dependency order.
    pub async fn start_all(self: &Arc<Self>) -> Result<(), ProcessError> {
        let order = {
            let graph = self.dependency_graph.read();
            graph.startup_order()?
        };

        for pid in &order {
            if let Some(mut child) = self.children.get_mut(pid) {
                let desc = child.descriptor.clone();
                let handle = self
                    .launcher
                    .launch(desc, self.command_tx.clone())
                    .await?;

                // Register with heartbeat manager
                self.heartbeat_manager.register(
                    pid.clone(),
                    child.descriptor.heartbeat.clone(),
                    self.command_tx.clone(),
                );

                child.handle = handle;
                info!("Child started: {}", pid);
            }
        }

        info!("All children started");
        Ok(())
    }

    /// Start the command processing loop.
    ///
    /// Processes supervisor commands from watcher tasks and heartbeat checker.
    /// This should be spawned as a background task.
    pub async fn run(self: Arc<Self>) {
        let mut rx = self.command_rx.lock().await;
        while let Some(cmd) = rx.recv().await {
            match cmd {
                SupervisorCommand::ChildExited { id, exit_status } => {
                    if exit_status.success() {
                        info!("Child exited cleanly: {}", id);
                        if let Some(mut child) = self.children.get_mut(&id) {
                            let _ = child
                                .handle
                                .transition_state(ProcessState::Stopped, "clean exit");
                        }
                        self.heartbeat_manager.deregister(&id);
                    } else {
                        self.on_child_failure(&id, exit_status).await;
                    }
                }
                SupervisorCommand::ChildFailure { id, error } => {
                    self.on_child_failure(
                        &id,
                        ExitStatus::Failure {
                            code: -1,
                        },
                    )
                    .await;
                }
                SupervisorCommand::HeartbeatTimeout { id, elapsed_ms } => {
                    warn!("Heartbeat timeout for {} ({}ms)", id, elapsed_ms);
                    self.on_child_failure(
                        &id,
                        ExitStatus::Failure { code: -1 },
                    )
                    .await;
                }
                SupervisorCommand::StopChild { id } => {
                    if let Some(child) = self.children.get(&id) {
                        let _ = self.launcher.stop(&child.handle).await;
                    }
                }
            }
        }
    }

    /// Called when a child terminates unexpectedly.
    async fn on_child_failure(self: &Arc<Self>, id: &ProcessId, exit_status: ExitStatus) {
        info!("Child failure detected: {} ({})", id, exit_status);

        // Update state and record the crash
        let (descriptor, restart_policy) = {
            let mut child = match self.children.get_mut(id) {
                Some(c) => c,
                None => {
                    error!("Child {} not found in supervisor", id);
                    return;
                }
            };

            let _ = child
                .handle
                .transition_state(ProcessState::Crashed, exit_status.description());

            child.restart_record.last_exit_code = exit_status.exit_code();

            self.metrics.total_crashed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            *self
                .metrics
                .restart_by_process
                .entry(id.path().to_string())
                .or_insert_with(|| std::sync::atomic::AtomicU64::new(0))
                .get_mut() += 1;

            (child.descriptor.clone(), child.descriptor.restart_policy.clone())
        };

        // Emit crash event
        self.event_bus.publish(ProcessCrashed {
            id: id.clone(),
            exit_code: exit_status.exit_code(),
            reason: format!("child exited: {}", exit_status),
            restart_scheduled: matches!(restart_policy, RestartPolicy::Immediate { .. }
                | RestartPolicy::ExponentialBackoff { .. }
                | RestartPolicy::LinearBackoff { .. }
                | RestartPolicy::SafeMode { .. }),
            restart_delay_ms: None,
            crashed_at: chrono::Utc::now(),
        }).await;

        // Apply supervision strategy
        self.apply_strategy(id).await;
    }

    /// Apply the supervision strategy to a child failure.
    async fn apply_strategy(self: &Arc<Self>, failed: &ProcessId) {
        match self.strategy {
            SupervisionStrategy::OneForOne => {
                self.restart_child(failed).await;
            }
            SupervisionStrategy::OneForAll => {
                // Stop all children, then restart them
                let children: Vec<ProcessId> =
                    self.children.iter().map(|e| e.key().clone()).collect();
                for id in &children {
                    if let Some(child) = self.children.get(id) {
                        let _ = self.launcher.stop(&child.handle).await;
                    }
                }
                for id in &children {
                    self.restart_child(id).await;
                }
                // Emit intervention event
                self.event_bus.publish(SupervisorIntervention {
                    supervisor_id: self.id.clone(),
                    strategy: "one_for_all".into(),
                    affected_processes: children,
                    reason: format!("child {} failed", failed),
                    occurred_at: chrono::Utc::now(),
                }).await;
            }
            SupervisionStrategy::RestForOne => {
                // Stop the failed child and all started after it
                let order = self.dependency_graph.read().startup_order().ok();
                if let Some(order) = order {
                    let failed_pos = order.iter().position(|id| id == failed);
                    if let Some(pos) = failed_pos {
                        let to_restart: Vec<ProcessId> = order[pos..].to_vec();
                        for id in &to_restart {
                            if let Some(child) = self.children.get(id) {
                                let _ = self.launcher.stop(&child.handle).await;
                            }
                        }
                        for id in &to_restart {
                            self.restart_child(id).await;
                        }
                        self.event_bus.publish(SupervisorIntervention {
                            supervisor_id: self.id.clone(),
                            strategy: "rest_for_one".into(),
                            affected_processes: to_restart,
                            reason: format!("child {} failed", failed),
                            occurred_at: chrono::Utc::now(),
                        }).await;
                    }
                }
            }
            SupervisionStrategy::EscalateToParent => {
                warn!("Escalating failure of {} to parent supervisor", failed);
                // In the root supervisor, escalation triggers emergency shutdown.
                self.event_bus.publish(ProcessFailed {
                    id: failed.clone(),
                    final_error: "failure escalated to parent".into(),
                    total_restarts: 0,
                    failed_at: chrono::Utc::now(),
                }).await;
            }
        }
    }

    /// Restart a single child process.
    async fn restart_child(self: &Arc<Self>, id: &ProcessId) {
        let (descriptor, action) = {
            let mut child = match self.children.get_mut(id) {
                Some(c) => c,
                None => {
                    error!("Cannot restart {}: not found", id);
                    return;
                }
            };

            let policy = child.descriptor.restart_policy.clone();
            let action = self.restart_engine.next_action(id, &policy, &mut child.restart_record);
            (child.descriptor.clone(), action)
        };

        match action {
            RestartAction::RestartAfter { delay } => {
                info!("Restarting {} after {:?}", id, delay);
                tokio::time::sleep(delay).await;

                match self
                    .launcher
                    .launch(descriptor, self.command_tx.clone())
                    .await
                {
                    Ok(handle) => {
                        if let Some(mut child) = self.children.get_mut(id) {
                            // Re-register with heartbeat manager after restart
                            self.heartbeat_manager.register(
                                id.clone(),
                                child.descriptor.heartbeat.clone(),
                                self.command_tx.clone(),
                            );
                            child.handle = handle;
                            let _ = child.handle.transition_state(
                                ProcessState::Starting,
                                "restart",
                            );
                        }
                        self.metrics.total_restarts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        self.event_bus.publish(ProcessRestarted {
                            id: id.clone(),
                            restart_count: 0,
                            reason: "auto-restart".into(),
                            restarted_at: chrono::Utc::now(),
                        }).await;
                    }
                    Err(e) => {
                        error!("Failed to restart {}: {}", id, e);
                        self.metrics.total_crashed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        self.event_bus.publish(ProcessFailed {
                            id: id.clone(),
                            final_error: format!("restart failed: {}", e),
                            total_restarts: 0,
                            failed_at: chrono::Utc::now(),
                        }).await;
                    }
                }
            }
            RestartAction::GivingUp => {
                error!("Max restarts exceeded for {}, transitioning to Failed", id);
                if let Some(mut child) = self.children.get_mut(id) {
                    let _ = child
                        .handle
                        .transition_state(ProcessState::Failed, "max restarts exceeded");
                }
                self.event_bus.publish(ProcessFailed {
                    id: id.clone(),
                    final_error: "max restarts exceeded".into(),
                    total_restarts: 0,
                    failed_at: chrono::Utc::now(),
                }).await;
            }
            RestartAction::AwaitManual => {
                info!("Waiting for manual recovery of {}", id);
            }
            RestartAction::RestartInSafeMode => {
                info!("Restarting {} in safe mode", id);
                // Safe mode restart with reduced capabilities
                match self
                    .launcher
                    .launch(descriptor, self.command_tx.clone())
                    .await
                {
                    Ok(handle) => {
                        if let Some(mut child) = self.children.get_mut(id) {
                            child.handle = handle;
                        }
                    }
                    Err(e) => {
                        error!("Safe mode restart failed for {}: {}", id, e);
                    }
                }
            }
        }
    }

    /// Stop all children in reverse dependency order.
    pub async fn stop_all(self: &Arc<Self>) -> Result<(), ProcessError> {
        let order = {
            let graph = self.dependency_graph.read();
            graph.shutdown_order()?
        };

        for pid in order.iter().rev() {
            if let Some(child) = self.children.get(pid) {
                let _ = self.launcher.stop(&child.handle).await;
                self.heartbeat_manager.deregister(pid);
            }
        }

        info!("All children stopped");
        Ok(())
    }

    /// Number of registered children.
    pub fn child_count(&self) -> usize {
        self.children.len()
    }
}

impl std::fmt::Debug for Supervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Supervisor")
            .field("id", &self.id)
            .field("strategy", &self.strategy)
            .field("children", &self.children.len())
            .finish()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency::DependencyGraph;
    use crate::registry::ProcessRegistry;
    use crate::heartbeat::HeartbeatManager;
    use crate::metrics::ProcessMetrics;
    use parking_lot::RwLock;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_supervisor_create_root() {
        let event_bus = Arc::new(EventBus::new(16));
        let metrics = Arc::new(ProcessMetrics::new());
        let registry = Arc::new(ProcessRegistry::new());
        let dep_graph = Arc::new(RwLock::new(DependencyGraph::new()));
        let launcher = Arc::new(ProcessLauncher::new(
            registry.clone(),
            dep_graph.clone(),
            metrics.clone(),
        ));
        let hb_manager = Arc::new(HeartbeatManager::new(event_bus.clone(), metrics.clone()));

        let sup = Supervisor::root(
            SupervisionStrategy::OneForOne,
            event_bus,
            metrics,
            launcher,
            registry,
            dep_graph,
            hb_manager,
        );

        assert_eq!(sup.id.path(), "lumi");
        assert_eq!(sup.child_count(), 0);
    }

    #[tokio::test]
    async fn test_supervisor_register_child() {
        let event_bus = Arc::new(EventBus::new(16));
        let metrics = Arc::new(ProcessMetrics::new());
        let registry = Arc::new(ProcessRegistry::new());
        let dep_graph = Arc::new(RwLock::new(DependencyGraph::new()));
        let launcher = Arc::new(ProcessLauncher::new(
            registry.clone(),
            dep_graph.clone(),
            metrics.clone(),
        ));
        let hb_manager = Arc::new(HeartbeatManager::new(event_bus.clone(), metrics.clone()));

        let sup = Supervisor::root(
            SupervisionStrategy::OneForOne,
            event_bus,
            metrics,
            launcher,
            registry,
            dep_graph,
            hb_manager,
        );

        let desc = ProcessDescriptor::new(
            ProcessId::new("lumi.core"),
            "core",
            semver::Version::new(1, 0, 0),
            crate::descriptor::ProcessKind::Worker {
                worker_fn: Arc::new(|| Box::pin(async {})),
            },
        );

        sup.register(desc).await.unwrap();
        assert_eq!(sup.child_count(), 1);
    }
}
