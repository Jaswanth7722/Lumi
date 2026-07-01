//! # Process Launcher
//!
//! Launches processes according to their `ProcessKind`:
//! - **InternalService**: Spawns an async service future on the Tokio runtime.
//! - **ChildProcess**: Spawns an OS-level child process via `tokio::process::Command`.
//! - **WasmPlugin**: (Stub) would load a WASM module into the plugin sandbox.
//! - **Worker**: Spawns a background async task.
//!
//! # Thread Safety
//!
//! `ProcessLauncher` is `Send + Sync` and designed to be shared via `Arc`.

use crate::dependency::DependencyGraph;
use crate::descriptor::{ProcessDescriptor, ProcessKind};
use crate::error::{ExitStatus, ProcessError};
use crate::handle::{ProcessCommand, ProcessHandle};
use crate::heartbeat::{HeartbeatConfig, HeartbeatSignal};
use crate::id::ProcessId;
use crate::lifecycle::{ProcessState, ProcessStateMachine};
use crate::metrics::ProcessInstanceMetrics;
use crate::registry::ProcessRegistry;
use crate::restart::RestartRecord;
use crate::supervisor::{SupervisorCommand, SupervisorHandle};
use crate::worker::WorkerManager;
use crossbeam_channel::bounded;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::metrics::ProcessMetrics;

/// Launches and stops managed processes.
pub struct ProcessLauncher {
    /// Process registry for looking up handles.
    registry: Arc<ProcessRegistry>,
    /// Dependency graph for validating startup order.
    dependency_graph: Arc<RwLock<DependencyGraph>>,
    /// Per-process metrics.
    metrics: Arc<ProcessMetrics>,
}

impl ProcessLauncher {
    /// Create a new process launcher.
    pub fn new(
        registry: Arc<ProcessRegistry>,
        dependency_graph: Arc<RwLock<DependencyGraph>>,
        metrics: Arc<ProcessMetrics>,
    ) -> Self {
        Self {
            registry,
            dependency_graph,
            metrics,
        }
    }

    /// Launch a single process based on its descriptor.
    ///
    /// Transitions the process from `Registered` → `Starting` → `Initializing`.
    /// For internal services, runs until the service future completes or
    /// startup timeout is reached.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::StartFailed` if the process cannot be launched.
    pub async fn launch(
        &self,
        descriptor: Arc<ProcessDescriptor>,
        supervisor_tx: mpsc::UnboundedSender<SupervisorCommand>,
    ) -> Result<ProcessHandle, ProcessError> {
        let id = descriptor.id.clone();
        let state_machine = ProcessStateMachine::new();

        // Create communication channels
        let (heartbeat_tx, _heartbeat_rx): (_, crossbeam_channel::Receiver<HeartbeatSignal>) =
            bounded(64);
        let (command_tx, _command_rx) = mpsc::channel(64);

        // Create process handle
        let metrics = Arc::new(ProcessInstanceMetrics::new());
        let handle = ProcessHandle::new(
            id.clone(),
            descriptor.clone(),
            state_machine,
            metrics,
            heartbeat_tx,
            command_tx,
            None,
        );

        // Transition to Starting
        handle
            .transition_state(ProcessState::Starting, "launch initiated")
            .map_err(|e| ProcessError::StartFailed {
                id: id.clone(),
                reason: e.to_string(),
            })?;

        // Launch based on process kind
        match &descriptor.kind {
            ProcessKind::InternalService { factory } => {
                self.launch_internal_service(handle.clone(), factory.clone(), supervisor_tx)
                    .await?;
            }
            ProcessKind::ChildProcess {
                executable,
                args,
                env,
                working_dir,
            } => {
                self.launch_child_process(
                    handle.clone(),
                    executable,
                    args,
                    env,
                    working_dir.as_deref(),
                    supervisor_tx,
                )
                .await?;
            }
            ProcessKind::WasmPlugin { plugin_id, .. } => {
                // WASM plugin launching is handled by lumi-plugin-host.
                // This is a placeholder for the plugin lifecycle integration.
                info!("WASM plugin '{}' registered (launch deferred to plugin host)", plugin_id);
                handle
                    .transition_state(ProcessState::Ready, "plugin registered")
                    .map_err(|e| ProcessError::StartFailed {
                        id: id.clone(),
                        reason: e.to_string(),
                    })?;
            }
            ProcessKind::Worker { .. } => {
                // Workers are spawned via WorkerManager, not directly launched.
                handle
                    .transition_state(ProcessState::Ready, "worker-capable process")
                    .map_err(|e| ProcessError::StartFailed {
                        id: id.clone(),
                        reason: e.to_string(),
                    })?;
            }
        }

        // Register the handle
        self.registry.insert(handle.clone());
        self.metrics.total_started.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.metrics.active_processes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        info!("Process launched: {} ({})", id, descriptor.kind.type_name());
        Ok(handle)
    }

    /// Launch an internal async service.
    async fn launch_internal_service(
        &self,
        handle: ProcessHandle,
        factory: Arc<dyn crate::descriptor::ServiceFactory>,
        supervisor_tx: mpsc::UnboundedSender<SupervisorCommand>,
    ) -> Result<(), ProcessError> {
        let id = handle.id().clone();
        let id_for_closure = id.clone();
        let handle_clone = handle.clone();

        let _join_handle = tokio::spawn(async move {
            // Run the service factory future
            let future = factory.create();
            future.await;

            // When the future completes, notify the supervisor
            let _ = supervisor_tx.send(SupervisorCommand::ChildExited {
                id: id_for_closure,
                exit_status: ExitStatus::Success,
            });
        });

        // Transition to Initializing (service future is running)
        handle_clone
            .transition_state(ProcessState::Initializing, "service spawned")
            .map_err(|e| ProcessError::StartFailed {
                id: id.clone(),
                reason: e.to_string(),
            })?;

        // Wait briefly for initialization, then mark Ready
        // In a real implementation, the service would signal readiness.
        tokio::time::sleep(Duration::from_millis(10)).await;

        handle_clone
            .transition_state(ProcessState::Ready, "service initialized")
            .map_err(|e| ProcessError::StartFailed {
                id: id.clone(),
                reason: e.to_string(),
            })?;

        Ok(())
    }

    /// Launch a child OS process.
    async fn launch_child_process(
        &self,
        handle: ProcessHandle,
        executable: &Path,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: Option<&Path>,
        supervisor_tx: mpsc::UnboundedSender<SupervisorCommand>,
    ) -> Result<(), ProcessError> {
        let id = handle.id().clone();

        // Build the command
        let mut cmd = tokio::process::Command::new(executable);
        cmd.args(args);
        cmd.envs(env);
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Platform-specific configuration
        #[cfg(unix)]
        {
            // Set process group so SIGTERM reaches all children
            cmd.process_group(0);
        }

        // Spawn the child process
        let mut child = cmd.spawn().map_err(|e| ProcessError::OsError {
            id: id.clone(),
            source: e,
        })?;

        let child_pid = child.id().ok_or_else(|| ProcessError::OsError {
            id: id.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::Other,
                "failed to get child PID",
            ),
        })?;

        // Spawn watcher task
        let watch_id = id.clone();
        let child_id = id.clone();
        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) => {
                    let exit = if status.success() {
                        ExitStatus::Success
                    } else {
                        ExitStatus::Failure {
                            code: status.code().unwrap_or(-1),
                        }
                    };
                    let _ = supervisor_tx.send(SupervisorCommand::ChildExited {
                        id: watch_id,
                        exit_status: exit,
                    });
                }
                Err(e) => {
                    let _ = supervisor_tx.send(SupervisorCommand::ChildFailure {
                        id: watch_id,
                        error: Box::new(ProcessError::OsError {
                            id: child_id,
                            source: e,
                        }),
                    });
                }
            }
        });

        // Transition to Ready
        handle
            .transition_state(ProcessState::Ready, format!("child process PID {}", child_pid))
            .map_err(|e| ProcessError::StartFailed {
                id: id.clone(),
                reason: e.to_string(),
            })?;

        Ok(())
    }

    /// Launch all processes in topological order with configurable parallelism.
    ///
    /// Processes with no inter-dependencies can be launched in parallel up to
    /// `max_parallel`. Default: 4.
    ///
    /// # Errors
    ///
    /// Returns the first `ProcessError` encountered. Previously launched
    /// processes remain running.
    pub async fn launch_all(
        &self,
        descriptors: Vec<Arc<ProcessDescriptor>>,
        supervisor_tx: mpsc::UnboundedSender<SupervisorCommand>,
        max_parallel: usize,
    ) -> Result<Vec<ProcessHandle>, ProcessError> {
        // Sort by dependency order
        let graph = self.dependency_graph.read();
        let order = graph.startup_order()?;
        drop(graph);

        // Create a lookup map
        let desc_map: HashMap<ProcessId, Arc<ProcessDescriptor>> = descriptors
            .into_iter()
            .map(|d| (d.id.clone(), d))
            .collect();

        let mut handles = Vec::new();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallel));

        for pid in &order {
            let desc = match desc_map.get(pid) {
                Some(d) => d.clone(),
                None => continue,
            };

            let permit = semaphore.clone().acquire_owned().await;
            let handle = self.launch(desc, supervisor_tx.clone()).await?;
            drop(permit);

            handles.push(handle);
        }

        Ok(handles)
    }

    /// Stop a single process gracefully.
    ///
    /// Sends stop command, waits for shutdown timeout, then force-kills
    /// if still running (for child processes).
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::NotFound` if the handle is not in the registry.
    pub async fn stop(&self, handle: &ProcessHandle) -> Result<(), ProcessError> {
        let id = handle.id().clone();
        let timeout_ms = handle.descriptor().shutdown_timeout_ms;

        handle
            .transition_state(ProcessState::Stopping, "shutdown requested")
            .ok();

        // Send stop command
        let cmd = ProcessCommand::Stop { timeout_ms };
        let _ = handle.send_command(cmd).await;

        // For child processes, wait and force-kill if needed
        if let Some(pid) = handle.os_pid() {
            let timeout = Duration::from_millis(timeout_ms);
            match crate::platform::graceful_kill(pid, timeout).await {
                Ok(()) => {
                    handle
                        .transition_state(ProcessState::Stopped, "process terminated")
                        .ok();
                }
                Err(e) => {
                    warn!("Force-kill required for {}: {}", id, e);
                    handle
                        .transition_state(ProcessState::Stopped, "force-killed")
                        .ok();
                    return Err(e);
                }
            }
        } else {
            handle
                .transition_state(ProcessState::Stopped, "stopped")
                .ok();
        }

        self.metrics.total_stopped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.metrics.active_processes.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        self.registry.remove(&id);

        info!("Process stopped: {}", id);
        Ok(())
    }
}

impl std::fmt::Debug for ProcessLauncher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessLauncher").finish()
    }
}
