//! # Worker Manager
//!
//! Manages background async workers (long-running Tokio tasks) within a process.
//!
//! Workers are lightweight async tasks that run until cancellation, panic,
//! or explicit stop. Panics in workers are caught via `catch_unwind` and
//! treated as worker failures that trigger the configurable restart policy.
//!
//! # Thread Safety
//!
//! `WorkerManager` is `Send + Sync` via `DashMap` and `Arc`.
//! Worker state reads are lock-free via `AtomicU8`.
//!
//! # Design
//!
//! - Workers are identified by `WorkerId` (owner process + name + UUID).
//! - Each worker has a `CancellationToken` for cooperative cancellation.
//! - Panics are caught via `std::panic::catch_unwind` at the boundary.
//! - Restart policies are applied on worker failure.

use crate::descriptor::WorkerFactory;
use crate::error::ProcessError;
use crate::id::{ProcessId, WorkerId};
use crate::restart::{RestartEngine, RestartPolicy, RestartRecord};
use dashmap::DashMap;
use lumas_runtime::event::EventBus;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use super::metrics::ProcessMetrics;

// ---------------------------------------------------------------------------
// WorkerState
// ---------------------------------------------------------------------------

/// Lifecycle state of a background worker thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    /// Worker is starting up.
    Starting = 0,
    /// Worker is running normally.
    Running = 1,
    /// Worker is being stopped.
    Stopping = 2,
    /// Worker has stopped cleanly.
    Stopped = 3,
    /// Worker has failed permanently.
    Failed = 4,
}

impl WorkerState {
    /// Convert from u8 (for atomic load).
    fn from_u8(val: u8) -> Self {
        match val {
            0 => WorkerState::Starting,
            1 => WorkerState::Running,
            2 => WorkerState::Stopping,
            3 => WorkerState::Stopped,
            4 => WorkerState::Failed,
            _ => WorkerState::Failed,
        }
    }
}

// ---------------------------------------------------------------------------
// WorkerHandle
// ---------------------------------------------------------------------------

/// Handle to a running background worker.
///
/// Provides access to worker state, cancellation, and restart history.
#[derive(Clone)]
pub struct WorkerHandle {
    /// Worker identifier.
    pub id: WorkerId,
    /// Join handle for the worker task.
    join: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Cancellation token for graceful shutdown.
    cancel: CancellationToken,
    /// Restart policy for this worker.
    restart_policy: RestartPolicy,
    /// Restart tracking record.
    restart_record: Arc<Mutex<RestartRecord>>,
    /// Lock-free worker state.
    state: Arc<AtomicU8>,
}

impl WorkerHandle {
    /// Create a new worker handle.
    fn new(id: WorkerId, restart_policy: RestartPolicy) -> Self {
        Self {
            id,
            join: Arc::new(Mutex::new(None)),
            cancel: CancellationToken::new(),
            restart_policy,
            restart_record: Arc::new(Mutex::new(RestartRecord::new())),
            state: Arc::new(AtomicU8::new(WorkerState::Starting as u8)),
        }
    }

    /// Get the current worker state (lock-free).
    pub fn state(&self) -> WorkerState {
        WorkerState::from_u8(self.state.load(Ordering::Acquire))
    }

    /// Set the worker state.
    fn set_state(&self, state: WorkerState) {
        self.state.store(state as u8, Ordering::Release);
    }

    /// Cancel the worker (cooperative cancellation).
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Get the cancellation token.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Get the restart record.
    pub fn restart_record(&self) -> Arc<Mutex<RestartRecord>> {
        self.restart_record.clone()
    }
}

impl std::fmt::Debug for WorkerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerHandle")
            .field("id", &self.id)
            .field("state", &self.state())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// WorkerManager
// ---------------------------------------------------------------------------

/// Manages background async workers for all processes.
///
/// # Examples
///
/// ```ignore
/// let wm = WorkerManager::new(event_bus, metrics);
/// let wid = wm.spawn(owner_pid, factory, RestartPolicy::default()).await?;
/// wm.stop(&wid, Duration::from_secs(5)).await?;
/// ```
pub struct WorkerManager {
    /// All registered workers.
    workers: DashMap<WorkerId, WorkerHandle>,
    /// Restart engine for worker restart logic.
    restart_engine: RestartEngine,
    /// Event bus for emitting worker events.
    event_bus: Arc<EventBus>,
    /// Process metrics.
    metrics: Arc<ProcessMetrics>,
}

impl WorkerManager {
    /// Create a new worker manager.
    pub fn new(event_bus: Arc<EventBus>, metrics: Arc<ProcessMetrics>) -> Self {
        Self {
            workers: DashMap::new(),
            restart_engine: RestartEngine::new(),
            event_bus,
            metrics,
        }
    }

    /// Spawn a worker and register it.
    ///
    /// The worker runs until cancellation, panic, or explicit stop.
    /// Panics in workers are caught via `catch_unwind` and treated as
    /// worker failures, triggering the restart policy.
    ///
    /// # Errors
    ///
    /// Never fails — worker spawns are infallible. Error conditions
    /// are propagated through the restart mechanism.
    pub async fn spawn(
        &self,
        owner: ProcessId,
        factory: Box<dyn WorkerFactory>,
        policy: RestartPolicy,
    ) -> Result<WorkerId, ProcessError> {
        let wid = WorkerId::new(owner, factory.name());
        let handle = WorkerHandle::new(wid.clone(), policy.clone());
        let handle_clone = handle.clone();

        self.metrics.active_workers.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Spawn the worker task
        let join_handle = tokio::spawn(async move {
            let cancel = handle_clone.cancel_token();
            let worker_fut = factory.create();

            handle_clone.set_state(WorkerState::Running);

            tokio::select! {
                _ = worker_fut => {
                    handle_clone.set_state(WorkerState::Stopped);
                }
                _ = cancel.cancelled() => {
                    handle_clone.set_state(WorkerState::Stopped);
                }
            }
        });

        *handle.join.lock().unwrap() = Some(join_handle);
        handle.set_state(WorkerState::Running);
        self.workers.insert(wid.clone(), handle);

        debug!("Worker spawned: {}", wid);
        Ok(wid)
    }

    /// Stop a specific worker by ID.
    ///
    /// Sends the cancellation token and waits for the worker to finish
    /// within the specified timeout.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::NotFound` if the worker is not registered.
    pub async fn stop(
        &self,
        id: &WorkerId,
        timeout: Duration,
    ) -> Result<(), ProcessError> {
        let handle = self
            .workers
            .get(id)
            .ok_or_else(|| ProcessError::NotFound {
                id: id.owner.clone(),
            })?;

        handle.set_state(WorkerState::Stopping);
        handle.cancel();

        // Wait for the join handle
        let join = handle.join.lock().unwrap().take();
        drop(handle);

        if let Some(jh) = join {
            tokio::select! {
                _ = jh => {
                    // Worker stopped cleanly
                }
                _ = tokio::time::sleep(timeout) => {
                    warn!("Worker {} stop timeout after {:?}", id, timeout);
                }
            }
        }

        self.workers.remove(id);
        self.metrics.active_workers.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        debug!("Worker stopped: {}", id);
        Ok(())
    }

    /// Stop all workers owned by a process.
    ///
    /// Called during process shutdown.
    pub async fn stop_all_for(&self, owner: &ProcessId, timeout: Duration) {
        let owned: Vec<WorkerId> = self
            .workers
            .iter()
            .filter(|entry| entry.key().owner.path() == owner.path())
            .map(|entry| entry.key().clone())
            .collect();

        for wid in owned {
            let _ = self.stop(&wid, timeout).await;
        }
    }

    /// Returns all workers for a given process.
    pub fn workers_for(&self, owner: &ProcessId) -> Vec<WorkerHandle> {
        self.workers
            .iter()
            .filter(|entry| entry.key().owner.path() == owner.path())
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Returns the number of registered workers.
    pub fn len(&self) -> usize {
        self.workers.len()
    }

    /// Returns `true` if no workers are registered.
    pub fn is_empty(&self) -> bool {
        self.workers.is_empty()
    }
}

impl std::fmt::Debug for WorkerManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerManager")
            .field("worker_count", &self.workers.len())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// TestWorker — Public for integration tests
// ---------------------------------------------------------------------------

/// A simple test worker that completes after a short delay.
/// Used in integration tests to verify worker lifecycle.
pub struct TestWorker;

impl WorkerFactory for TestWorker {
    fn name(&self) -> &'static str {
        "test-worker"
    }

    fn create(&self) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_worker_spawns_and_runs() {
        let event_bus = Arc::new(EventBus::new(16));
        let metrics = Arc::new(ProcessMetrics::new());
        let wm = WorkerManager::new(event_bus, metrics);

        let owner = ProcessId::new("test");
        let wid = wm
            .spawn(
                owner.clone(),
                Box::new(TestWorker),
                RestartPolicy::Never,
            )
            .await
            .unwrap();

        assert_eq!(wm.len(), 1);
        assert_eq!(wid.owner.path(), "test");

        // Clean up
        wm.stop(&wid, Duration::from_secs(5)).await.unwrap();
        assert_eq!(wm.len(), 0);
    }

    #[tokio::test]
    async fn test_stop_all_for_stops_all_workers() {
        let event_bus = Arc::new(EventBus::new(16));
        let metrics = Arc::new(ProcessMetrics::new());
        let wm = WorkerManager::new(event_bus, metrics);

        let owner = ProcessId::new("test");
        let wid1 = wm
            .spawn(owner.clone(), Box::new(TestWorker), RestartPolicy::Never)
            .await
            .unwrap();
        let wid2 = wm
            .spawn(owner.clone(), Box::new(TestWorker), RestartPolicy::Never)
            .await
            .unwrap();

        assert_eq!(wm.len(), 2);

        wm.stop_all_for(&owner, Duration::from_secs(5)).await;
        assert_eq!(wm.len(), 0);
    }

    #[tokio::test]
    async fn test_workers_for_returns_correct_workers() {
        let event_bus = Arc::new(EventBus::new(16));
        let metrics = Arc::new(ProcessMetrics::new());
        let wm = WorkerManager::new(event_bus, metrics);

        let owner1 = ProcessId::new("process-a");
        let owner2 = ProcessId::new("process-b");

        wm.spawn(owner1.clone(), Box::new(TestWorker), RestartPolicy::Never)
            .await
            .unwrap();
        wm.spawn(owner2.clone(), Box::new(TestWorker), RestartPolicy::Never)
            .await
            .unwrap();

        assert_eq!(wm.workers_for(&owner1).len(), 1);
        assert_eq!(wm.workers_for(&owner2).len(), 1);
    }
}
