//! # Process Handle
//!
//! A live reference to a running managed process. Provides access to
//! state, metadata, uptime, and the ability to send commands and
//! subscribe to state transitions.
//!
//! # Thread Safety
//!
//! `ProcessHandle` is `Clone` (O(1) via `Arc`), `Send`, and `Sync`.
//! All state reads are non-blocking via atomic operations or
//! `parking_lot::RwLock`. Handles remain valid until the process
//! reaches a terminal state (`Stopped` or `Failed`).

use crate::descriptor::ProcessDescriptor;
use crate::error::ProcessError;
use crate::heartbeat::{HeartbeatMetadata, HeartbeatSignal};
use crate::id::ProcessId;
use crate::lifecycle::{ProcessState, ProcessStateMachine, StateTransitionRecord};
use crate::metrics::ProcessInstanceMetrics;
use chrono::{DateTime, Utc};
use crossbeam_channel::Sender as CrossbeamSender;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use parking_lot::RwLock as ParkingRwLock;

// ---------------------------------------------------------------------------
// ProcessCommand
// ---------------------------------------------------------------------------

/// Commands that can be sent to a managed process.
///
/// These are enqueued and processed by the supervisor or the process
/// itself, depending on the process kind.
#[derive(Debug, Clone)]
pub enum ProcessCommand {
    /// Stop the process gracefully with a timeout.
    Stop {
        /// Timeout in milliseconds before force-kill.
        timeout_ms: u64,
    },
    /// Pause the process (suspend work).
    Pause,
    /// Resume a paused process.
    Resume,
    /// Restart the process for the given reason.
    Restart {
        /// Human-readable reason for the restart.
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// ProcessHandleInner
// ---------------------------------------------------------------------------

/// Internal shared state for a process handle.
struct ProcessHandleInner {
    /// The process identifier.
    id: ProcessId,
    /// State machine for lifecycle tracking.
    state: ParkingRwLock<ProcessStateMachine>,
    /// Immutable process descriptor (shared reference).
    descriptor: Arc<ProcessDescriptor>,
    /// Per-instance metrics.
    metrics: Arc<ProcessInstanceMetrics>,
    /// Channel for sending heartbeats from the process to the heartbeat manager.
    heartbeat_tx: CrossbeamSender<HeartbeatSignal>,
    /// Channel for sending commands to the process.
    command_tx: mpsc::Sender<ProcessCommand>,
    /// When this process instance was started.
    started_at: DateTime<Utc>,
    /// OS PID if this is a child process (None for internal services and workers).
    os_pid: Option<u32>,
}

// ---------------------------------------------------------------------------
// ProcessHandle
// ---------------------------------------------------------------------------

/// A live reference to a running managed process.
///
/// Cloning a handle is O(1). Handles are valid until the process
/// reaches a terminal state (`Stopped` or `Failed`). After that,
/// operations return `ProcessError::NotFound`.
///
/// # Examples
///
/// ```ignore
/// let handle = manager.handle(&pid).unwrap();
/// println!("Process {} is {:?}", handle.id(), handle.state());
/// ```
#[derive(Clone)]
pub struct ProcessHandle {
    inner: Arc<ProcessHandleInner>,
}

impl ProcessHandle {
    /// Create a new process handle. Called by the launcher/supervisor.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: ProcessId,
        descriptor: Arc<ProcessDescriptor>,
        state: ProcessStateMachine,
        metrics: Arc<ProcessInstanceMetrics>,
        heartbeat_tx: CrossbeamSender<HeartbeatSignal>,
        command_tx: mpsc::Sender<ProcessCommand>,
        os_pid: Option<u32>,
    ) -> Self {
        Self {
            inner: Arc::new(ProcessHandleInner {
                id,
                state: ParkingRwLock::new(state),
                descriptor,
                metrics,
                heartbeat_tx,
                command_tx,
                started_at: Utc::now(),
                os_pid,
            }),
        }
    }

    /// Returns the process identifier.
    pub fn id(&self) -> &ProcessId {
        &self.inner.id
    }

    /// Returns the current state of the process.
    ///
    /// This is a non-blocking read via parking_lot::RwLock.
    pub fn state(&self) -> ProcessState {
        self.inner.state.read().current()
    }

    /// Returns the process descriptor (immutable metadata).
    pub fn descriptor(&self) -> Arc<ProcessDescriptor> {
        self.inner.descriptor.clone()
    }

    /// Returns the OS PID, if this is a child process.
    pub fn os_pid(&self) -> Option<u32> {
        self.inner.os_pid
    }

    /// Returns the uptime of this process instance.
    pub fn uptime(&self) -> Duration {
        let elapsed = Utc::now() - self.inner.started_at;
        elapsed.to_std()
            .unwrap_or(Duration::ZERO)
    }

    /// Returns when this process instance was started.
    pub fn started_at(&self) -> DateTime<Utc> {
        self.inner.started_at
    }

    /// Returns the instance metrics.
    pub fn metrics(&self) -> &Arc<ProcessInstanceMetrics> {
        &self.inner.metrics
    }

    /// Send a heartbeat signal from within the managed process.
    ///
    /// Called by the process itself on each heartbeat interval.
    /// Non-blocking; returns an error if the heartbeat channel is full.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::NotFound` if the process is not registered
    /// with the heartbeat manager.
    pub fn heartbeat(&self, metadata: HeartbeatMetadata) -> Result<(), ProcessError> {
        self.inner
            .heartbeat_tx
            .send(HeartbeatSignal::Pulse {
                id: self.inner.id.clone(),
                metadata,
            })
            .map_err(|_| ProcessError::NotFound {
                id: self.inner.id.clone(),
            })
    }

    /// Subscribe to state transitions for this process.
    ///
    /// Returns a `mpsc::Receiver` that yields `StateTransitionRecord`
    /// values for each transition.
    pub fn subscribe_state(&self) -> mpsc::Receiver<StateTransitionRecord> {
        self.inner.state.write().subscribe()
    }

    /// Send a command to the process (stop, pause, resume, restart).
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::NotFound` if the command channel is closed
    /// (process already terminated).
    pub async fn send_command(&self, cmd: ProcessCommand) -> Result<(), ProcessError> {
        self.inner
            .command_tx
            .send(cmd)
            .await
            .map_err(|_| ProcessError::NotFound {
                id: self.inner.id.clone(),
            })
    }

    /// Transition the internal state machine to a new state.
    ///
    /// This is called internally by the launcher and supervisor.
    /// # Panics
    ///
    /// Never panics. Returns `ProcessError::InvalidStateTransition` on invalid transitions.
    pub(crate) fn transition_state(
        &self,
        to: ProcessState,
        reason: impl Into<String>,
    ) -> Result<(), ProcessError> {
        self.inner.state.write().transition(to, reason)
    }

    /// Wait until this process reaches a specific state or a terminal state.
    ///
    /// Returns the current state if it already matches or is terminal.
    /// Otherwise, subscribes to transitions and waits.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::NotFound` if the channel closes unexpectedly.
    pub async fn wait_for_state(
        &self,
        target: ProcessState,
        timeout: Duration,
    ) -> Result<ProcessState, ProcessError> {
        let current = self.state();
        if current == target || current.is_terminal() {
            return Ok(current);
        }

        let mut rx = self.subscribe_state();
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Ok(self.state());
            }

            tokio::select! {
                maybe_record = rx.recv() => {
                    match maybe_record {
                        Some(record) => {
                            if record.to == target || record.to.is_terminal() {
                                return Ok(record.to);
                            }
                        }
                        None => return Err(ProcessError::NotFound { id: self.inner.id.clone() }),
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    // Check current state periodically
                    let current = self.state();
                    if current == target || current.is_terminal() {
                        return Ok(current);
                    }
                }
            }
        }
    }
}

impl std::fmt::Debug for ProcessHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessHandle")
            .field("id", &self.inner.id)
            .field("state", &self.state())
            .field("os_pid", &self.inner.os_pid)
            .field("uptime", &self.uptime())
            .finish()
    }
}

impl ProcessHandle {
    /// Create a dummy handle used during registration before the real launch.
    /// The descriptor is a minimal placeholder; it is replaced with the real
    /// handle once the process is started.
    pub(crate) fn dummy(id: &ProcessId) -> Self {
        use crate::heartbeat::HeartbeatSignal;
        use crate::metrics::ProcessInstanceMetrics;
        use crate::lifecycle::ProcessStateMachine;
        use crossbeam_channel::bounded;

        let (hb_tx, _) = bounded(16);
        let (cmd_tx, _) = tokio::sync::mpsc::channel(16);

        struct DummyWorker;
        impl crate::descriptor::WorkerFactory for DummyWorker {
            fn name(&self) -> &'static str { "dummy" }
            fn create(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
                Box::pin(async {})
            }
        }
        ProcessHandle::new(
            id.clone(),
            Arc::new(ProcessDescriptor::new(
                id.clone(),
                id.short_name(),
                semver::Version::new(0, 0, 0),
                crate::descriptor::ProcessKind::Worker {
                    worker_fn: Arc::new(DummyWorker),
                },
            )),
            ProcessStateMachine::new(),
            Arc::new(ProcessInstanceMetrics::new()),
            hb_tx,
            cmd_tx,
            None,
        )
    }
}

impl PartialEq for ProcessHandle {
    fn eq(&self, other: &Self) -> bool {
        self.inner.id == other.inner.id
    }
}

impl Eq for ProcessHandle {}
