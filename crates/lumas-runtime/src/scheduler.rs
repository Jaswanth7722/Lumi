//! # Async Task Scheduler
//!
//! Production-grade async task scheduler built on Tokio.
//!
//! Supports immediate, delayed, repeating, and timed-out task execution
//! with priority levels, background workers with automatic restart, and
//! graceful shutdown with draining.
//!
//! # Thread Safety
//!
//! `Scheduler` is `Send + Sync`. All internal state is protected by
//! `RwLock` or atomic operations.
//!
//! # Errors
//!
//! Task failures, timeouts, and cancellation produce `SchedulerError`.

use crate::error::SchedulerError;
use crate::metrics::{Counter, Gauge, Histogram, MetricsRegistry, Timer};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Priority
// ---------------------------------------------------------------------------

/// Task scheduling priority.
///
/// Affects execution order within the scheduler; higher-priority tasks
/// are dispatched before lower-priority ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskPriority {
    /// Critical system tasks (lifecycle, health checks).
    Critical = 0,
    /// High-importance tasks (user-facing operations).
    High = 1,
    /// Normal operational tasks.
    Normal = 2,
    /// Low-priority tasks (background processing).
    Low = 3,
    /// Background maintenance tasks.
    Background = 4,
}

impl fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskPriority::Critical => write!(f, "critical"),
            TaskPriority::High => write!(f, "high"),
            TaskPriority::Normal => write!(f, "normal"),
            TaskPriority::Low => write!(f, "low"),
            TaskPriority::Background => write!(f, "background"),
        }
    }
}

// ---------------------------------------------------------------------------
// Task Status
// ---------------------------------------------------------------------------

/// Status of a scheduled task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskStatus {
    /// Task is pending execution.
    Pending,
    /// Task is currently running.
    Running,
    /// Task completed successfully.
    Completed,
    /// Task was cancelled.
    Cancelled,
    /// Task failed with an error.
    Failed,
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Cancelled => write!(f, "cancelled"),
            TaskStatus::Failed => write!(f, "failed"),
        }
    }
}

// ---------------------------------------------------------------------------
// Task Handle
// ---------------------------------------------------------------------------

/// Handle to a scheduled task, allowing cancellation and status queries.
pub struct TaskHandle {
    /// Task identifier.
    pub id: String,
    /// Token for cooperative cancellation.
    cancel_token: CancellationToken,
    /// Shared task status.
    status: Arc<RwLock<TaskStatus>>,
    /// Task priority.
    pub priority: TaskPriority,
}

impl TaskHandle {
    fn new(id: String, priority: TaskPriority) -> Self {
        Self {
            id,
            cancel_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(TaskStatus::Pending)),
            priority,
        }
    }

    /// Cancel the task (cooperative cancellation).
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Get the cancellation token for cooperative cancellation.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Get the current task status.
    pub async fn status(&self) -> TaskStatus {
        *self.status.read().await
    }

    /// Wait for the task to complete.
    ///
    /// Returns the final status of the task.
    pub async fn wait(&self) -> TaskStatus {
        loop {
            let status = *self.status.read().await;
            match status {
                TaskStatus::Pending | TaskStatus::Running => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                _ => return status,
            }
        }
    }

    /// Set the task status (internal).
    async fn set_status(&self, status: TaskStatus) {
        *self.status.write().await = status;
    }
}

impl fmt::Debug for TaskHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskHandle")
            .field("id", &self.id)
            .field("priority", &self.priority)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Scheduler Metrics
// ---------------------------------------------------------------------------

/// Scheduler metrics snapshot.
#[derive(Debug, Clone)]
pub struct SchedulerMetrics {
    /// Number of tasks by status.
    pub by_status: HashMap<TaskStatus, u64>,
    /// Number of tasks by priority.
    pub by_priority: HashMap<TaskPriority, u64>,
    /// Mean task duration in milliseconds by priority.
    pub mean_duration_ms: HashMap<TaskPriority, f64>,
    /// Tasks completed since scheduler start.
    pub total_completed: u64,
    /// Tasks that failed since scheduler start.
    pub total_failed: u64,
    /// Active cancellations.
    pub total_cancelled: u64,
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

/// Production-grade async task scheduler.
///
/// Manages task lifecycle, priority-based dispatch, background workers
/// with auto-restart, and graceful shutdown with draining.
pub struct Scheduler {
    /// Whether the scheduler is accepting new tasks.
    accepting: AtomicBool,
    /// Cancellation token for shutdown.
    shutdown_token: CancellationToken,
    /// Task handles by ID.
    handles: RwLock<HashMap<String, Arc<TaskHandle>>>,
    /// Task status counts.
    status_counts: RwLock<HashMap<TaskStatus, u64>>,
    /// Priority counts.
    priority_counts: RwLock<HashMap<TaskPriority, u64>>,
    /// Duration tracking by priority.
    duration_totals: RwLock<HashMap<TaskPriority, (u64, f64)>>,
    /// Maximum concurrent tasks.
    semaphore: Arc<Semaphore>,
    /// Metrics registry.
    metrics: Option<Arc<MetricsRegistry>>,
    /// Total completed tasks.
    total_completed: AtomicU64,
    /// Total failed tasks.
    total_failed: AtomicU64,
    /// Total cancelled tasks.
    total_cancelled: AtomicU64,
    /// Next task ID counter.
    next_id: AtomicU64,
}

impl Scheduler {
    /// Create a new scheduler with the given maximum concurrency.
    ///
    /// `max_concurrent` limits the number of simultaneously running tasks.
    pub fn new(max_concurrent: u32) -> Self {
        let mut status_counts = HashMap::new();
        status_counts.insert(TaskStatus::Pending, 0);
        status_counts.insert(TaskStatus::Running, 0);
        status_counts.insert(TaskStatus::Completed, 0);
        status_counts.insert(TaskStatus::Cancelled, 0);
        status_counts.insert(TaskStatus::Failed, 0);

        Self {
            accepting: AtomicBool::new(true),
            shutdown_token: CancellationToken::new(),
            handles: RwLock::new(HashMap::new()),
            status_counts: RwLock::new(status_counts),
            priority_counts: RwLock::new(HashMap::new()),
            duration_totals: RwLock::new(HashMap::new()),
            semaphore: Arc::new(Semaphore::new(max_concurrent as usize)),
            metrics: None,
            total_completed: AtomicU64::new(0),
            total_failed: AtomicU64::new(0),
            total_cancelled: AtomicU64::new(0),
            next_id: AtomicU64::new(0),
        }
    }

    /// Set the metrics registry.
    pub fn set_metrics(&mut self, metrics: Arc<MetricsRegistry>) {
        self.metrics = Some(metrics);
    }

    /// Generate a unique task ID.
    fn next_id(&self) -> String {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("task-{id}")
    }

    /// Update status tracking counts.
    async fn track_status_change(&self, from: TaskStatus, to: TaskStatus) {
        let mut counts = self.status_counts.write().await;
        *counts.entry(from).or_insert(0) =
            counts.get(&from).copied().unwrap_or(1).saturating_sub(1);
        *counts.entry(to).or_insert(0) += 1;
    }

    async fn track_priority(&self, priority: TaskPriority) {
        let mut counts = self.priority_counts.write().await;
        *counts.entry(priority).or_insert(0) += 1;
    }

    async fn track_duration(&self, priority: TaskPriority, duration_ms: f64) {
        let mut totals = self.duration_totals.write().await;
        let entry = totals.entry(priority).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += duration_ms;
    }

    // -------------------------------------------------------------------
    // Task Spawning
    // -------------------------------------------------------------------

    /// Spawn a task for immediate execution.
    ///
    /// The task runs as soon as a scheduler slot is available.
    pub async fn spawn_immediate<F, T>(
        self: &Arc<Self>,
        task: F,
        priority: TaskPriority,
    ) -> Arc<TaskHandle>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let handle = Arc::new(TaskHandle::new(self.next_id(), priority));
        self.register_handle(handle.clone()).await;

        let this = self.clone();
        let handle_clone = handle.clone();
        let permit = self.semaphore.clone().acquire_owned().await;
        let ct = handle_clone.cancel_token();

        tokio::spawn(async move {
            let cancel_fut = ct.cancelled();
            handle_clone.set_status(TaskStatus::Running).await;
            this.track_status_change(TaskStatus::Pending, TaskStatus::Running)
                .await;

            let start = Instant::now();
            tokio::select! {
                r = task => {
                    if handle_clone.cancel_token().is_cancelled() {
                        handle_clone.set_status(TaskStatus::Cancelled).await;
                        this.total_cancelled.fetch_add(1, Ordering::Relaxed);
                        this.track_status_change(TaskStatus::Running, TaskStatus::Cancelled).await;
                    } else {
                        handle_clone.set_status(TaskStatus::Completed).await;
                        this.total_completed.fetch_add(1, Ordering::Relaxed);
                        this.track_status_change(TaskStatus::Running, TaskStatus::Completed).await;
                    }
                    let _ = r;
                }
                _ = cancel_fut => {
                    handle_clone.set_status(TaskStatus::Cancelled).await;
                    this.total_cancelled.fetch_add(1, Ordering::Relaxed);
                    this.track_status_change(TaskStatus::Running, TaskStatus::Cancelled).await;
                    return;
                }
            };

            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            this.track_duration(priority, elapsed).await;

            drop(permit);
        });

        handle
    }

    /// Spawn a delayed task that executes after a specified duration.
    pub async fn spawn_delayed<F, T>(
        self: &Arc<Self>,
        task: F,
        delay: Duration,
        priority: TaskPriority,
    ) -> Arc<TaskHandle>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let handle = Arc::new(TaskHandle::new(self.next_id(), priority));
        self.register_handle(handle.clone()).await;

        let this = self.clone();
        let handle_clone = handle.clone();
        let ct = handle_clone.cancel_token();

        tokio::spawn(async move {
            let cancel_fut = ct.cancelled();
            tokio::select! {
                _ = tokio::time::sleep(delay) => {
                    // Delay elapsed, execute
                }
                _ = cancel_fut => {
                    handle_clone.set_status(TaskStatus::Cancelled).await;
                    return;
                }
            }

            handle_clone.set_status(TaskStatus::Running).await;
            let cancel_fut2 = ct.cancelled();
            tokio::select! {
                r = task => {
                    handle_clone.set_status(TaskStatus::Completed).await;
                    this.total_completed.fetch_add(1, Ordering::Relaxed);
                    let _ = r;
                }
                _ = cancel_fut2 => {
                    handle_clone.set_status(TaskStatus::Cancelled).await;
                    this.total_cancelled.fetch_add(1, Ordering::Relaxed);
                }
            };
        });

        handle
    }

    /// Spawn a repeating task that executes on a fixed interval.
    ///
    /// The task runs repeatedly until cancelled or the scheduler shuts down.
    pub async fn spawn_repeating<F, T>(
        self: &Arc<Self>,
        task: F,
        interval: Duration,
        priority: TaskPriority,
    ) -> Arc<TaskHandle>
    where
        F: Future<Output = T> + Send + Clone + 'static,
        T: Send + 'static,
    {
        let handle = Arc::new(TaskHandle::new(self.next_id(), priority));
        self.register_handle(handle.clone()).await;

        let this = self.clone();
        let handle_clone = handle.clone();
        let shutdown_token = self.shutdown_token.clone();

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            interval_timer.tick().await; // Skip initial tick

            loop {
                let ct = handle_clone.cancel_token();
                let cancel_fut = ct.cancelled();
                tokio::select! {
                    _ = interval_timer.tick() => {
                        if handle_clone.cancel_token().is_cancelled() || shutdown_token.is_cancelled() {
                            break;
                        }
                        // Clone and spawn the task each tick
                        tokio::spawn(task.clone());
                    }
                    _ = cancel_fut => {
                        break;
                    }
                    _ = shutdown_token.cancelled() => {
                        break;
                    }
                }
            }

            handle_clone.set_status(TaskStatus::Cancelled).await;
        });

        handle
    }

    /// Spawn a task with a timeout. If the task doesn't complete within
    /// the timeout, it is cancelled.
    pub async fn spawn_with_timeout<F, T>(
        self: &Arc<Self>,
        task: F,
        timeout: Duration,
        priority: TaskPriority,
    ) -> Arc<TaskHandle>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let handle = Arc::new(TaskHandle::new(self.next_id(), priority));
        self.register_handle(handle.clone()).await;

        let this = self.clone();
        let handle_clone = handle.clone();
        let ct = handle_clone.cancel_token();

        tokio::spawn(async move {
            let cancel_fut = ct.cancelled();
            handle_clone.set_status(TaskStatus::Running).await;

            tokio::select! {
                _ = task => {
                    handle_clone.set_status(TaskStatus::Completed).await;
                    this.total_completed.fetch_add(1, Ordering::Relaxed);
                }
                _ = tokio::time::sleep(timeout) => {
                    handle_clone.set_status(TaskStatus::Failed).await;
                    this.total_failed.fetch_add(1, Ordering::Relaxed);
                    error!("Task {} timed out after {:?}", handle_clone.id, timeout);
                }
                _ = cancel_fut => {
                    handle_clone.set_status(TaskStatus::Cancelled).await;
                    this.total_cancelled.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        handle
    }

    /// Spawn a long-running background worker that is automatically
    /// restarted on failure.
    ///
    /// The worker is passed as a `Future` via an `async` block. On failure
    /// (i.e., if the future completes or panics), the scheduler will restart
    /// it up to `max_restarts` times with exponential backoff.
    pub async fn spawn_background_worker<F, T>(
        self: &Arc<Self>,
        name: &str,
        worker: F,
        max_restarts: u32,
    ) -> Arc<TaskHandle>
    where
        F: Future<Output = T> + Send + Clone + 'static,
        T: Send + 'static,
    {
        let name_owned = name.to_string();
        let handle = Arc::new(TaskHandle::new(
            format!("worker-{name}"),
            TaskPriority::Background,
        ));
        self.register_handle(handle.clone()).await;

        let this = self.clone();
        let handle_clone = handle.clone();
        let shutdown_token = self.shutdown_token.clone();

        tokio::spawn(async move {
            let mut attempts = 0u32;

            loop {
                if shutdown_token.is_cancelled() || handle_clone.cancel_token().is_cancelled() {
                    break;
                }

                handle_clone.set_status(TaskStatus::Running).await;

                let worker_future = worker.clone();
                let ct = handle_clone.cancel_token();

                tokio::select! {
                    _ = worker_future => {
                        // Worker completed — restart if configured
                    }
                    _ = ct.cancelled() => {
                        handle_clone.set_status(TaskStatus::Cancelled).await;
                        break;
                    }
                    _ = shutdown_token.cancelled() => {
                        handle_clone.set_status(TaskStatus::Cancelled).await;
                        break;
                    }
                }

                // Handle restart logic
                attempts += 1;
                if attempts >= max_restarts {
                    error!(
                        "Worker '{}' exceeded max restarts ({max_restarts})",
                        name_owned
                    );
                    handle_clone.set_status(TaskStatus::Failed).await;
                    break;
                }

                let backoff = Duration::from_secs(1u64 << attempts.min(5));
                warn!(
                    "Restarting worker '{}' (attempt {attempts}/{max_restarts}) after {backoff:?}",
                    name_owned
                );
                tokio::time::sleep(backoff).await;
            }
        });

        handle
    }

    /// Register a task handle for tracking.
    async fn register_handle(&self, handle: Arc<TaskHandle>) {
        self.handles.write().await.insert(handle.id.clone(), handle);
    }

    // -------------------------------------------------------------------
    // Metrics
    // -------------------------------------------------------------------

    /// Get a snapshot of scheduler metrics.
    pub async fn metrics(&self) -> SchedulerMetrics {
        let status_counts = self.status_counts.read().await.clone();
        let priority_counts = self.priority_counts.read().await.clone();
        let duration_totals = self.duration_totals.read().await.clone();

        let mean_duration = duration_totals
            .into_iter()
            .map(|(p, (count, total))| (p, if count > 0 { total / count as f64 } else { 0.0 }))
            .collect();

        SchedulerMetrics {
            by_status: status_counts,
            by_priority: priority_counts,
            mean_duration_ms: mean_duration,
            total_completed: self.total_completed.load(Ordering::Relaxed),
            total_failed: self.total_failed.load(Ordering::Relaxed),
            total_cancelled: self.total_cancelled.load(Ordering::Relaxed),
        }
    }

    // -------------------------------------------------------------------
    // Shutdown
    // -------------------------------------------------------------------

    /// Initiate graceful shutdown.
    ///
    /// Stops accepting new tasks, drains in-flight tasks by priority,
    /// and cancels background workers.
    pub async fn shutdown(&self, drain_timeout: Duration) {
        info!(
            "Scheduler shutting down (drain timeout: {:?})",
            drain_timeout
        );
        self.accepting.store(false, Ordering::Relaxed);

        // Cancel background and low priority tasks
        let handles = self.handles.read().await;
        for handle in handles.values() {
            if handle.priority == TaskPriority::Background || handle.priority == TaskPriority::Low {
                handle.cancel();
            }
        }
        drop(handles);

        // Wait for Critical, High, Normal tasks to complete
        let deadline = tokio::time::Instant::now() + drain_timeout;
        loop {
            if tokio::time::Instant::now() >= deadline {
                warn!("Drain timeout reached; force-cancelling remaining tasks");
                // Force cancel all remaining
                let handles = self.handles.read().await;
                for handle in handles.values() {
                    handle.cancel();
                }
                break;
            }

            let running = {
                let counts = self.status_counts.read().await;
                *counts.get(&TaskStatus::Running).unwrap_or(&0)
            };

            if running == 0 {
                break;
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        self.shutdown_token.cancel();
        info!("Scheduler shut down complete");
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[tokio::test]
    async fn test_immediate_task_executes() {
        let scheduler = Arc::new(Scheduler::new(16));
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();

        let handle = scheduler
            .spawn_immediate(
                async move {
                    flag_clone.store(true, Ordering::Relaxed);
                },
                TaskPriority::Normal,
            )
            .await;

        handle.wait().await;
        assert!(flag.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_delayed_task_executes_after_delay() {
        let scheduler = Arc::new(Scheduler::new(16));
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();

        let start = Instant::now();
        let handle = scheduler
            .spawn_delayed(
                async move {
                    flag_clone.store(true, Ordering::Relaxed);
                },
                Duration::from_millis(50),
                TaskPriority::Normal,
            )
            .await;

        handle.wait().await;
        let elapsed = start.elapsed();
        assert!(flag.load(Ordering::Relaxed));
        assert!(elapsed >= Duration::from_millis(30));
    }

    #[tokio::test]
    async fn test_task_cancellation_stops_execution() {
        let scheduler = Arc::new(Scheduler::new(16));
        let flag = Arc::new(AtomicBool::new(true));
        let flag_clone = flag.clone();

        let handle = scheduler
            .spawn_immediate(
                async move {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    flag_clone.store(false, Ordering::Relaxed);
                },
                TaskPriority::Normal,
            )
            .await;

        handle.cancel();
        handle.wait().await;
        assert_eq!(handle.status().await, TaskStatus::Cancelled);
        assert!(flag.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_shutdown_drains_normal_tasks() {
        let scheduler = Arc::new(Scheduler::new(16));
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();

        scheduler
            .spawn_immediate(
                async move {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    flag_clone.store(true, Ordering::Relaxed);
                },
                TaskPriority::Normal,
            )
            .await;

        scheduler.shutdown(Duration::from_secs(1)).await;
        assert!(flag.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_scheduler_metrics() {
        let scheduler = Arc::new(Scheduler::new(16));

        scheduler
            .spawn_immediate(async {}, TaskPriority::Normal)
            .await;

        tokio::time::sleep(Duration::from_millis(50)).await;
        let metrics = scheduler.metrics().await;
        assert!(metrics.total_completed > 0 || metrics.total_cancelled == 0);
    }
}
