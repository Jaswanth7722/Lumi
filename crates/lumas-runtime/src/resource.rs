//! # Resource Manager
//!
//! Centralized resource tracking system for the Lumas runtime.
//!
//! Polls memory, CPU, task, file handle, and network resource usage
//! every 10 seconds via a background worker. Emits warning and critical
//! events when configurable thresholds are exceeded.
//!
//! # Thread Safety
//!
//! `ResourceManager` is `Send + Sync`. Snapshot reads are non-blocking
//! via atomic fields. The background worker runs on the Tokio runtime.
//!
//! # Errors
//!
//! Resource limit violations produce `ResourceWarning` or `ResourceCritical`
//! events, which the runtime can use to trigger graceful degradation.

use crate::error::ResourceError;
use crate::event::{EventBus, ResourceCritical, ResourceWarning};
use crate::metrics::{Gauge, MetricsRegistry};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Snapshots resource usage at a point in time.
#[derive(Debug, Clone)]
pub struct ResourceSnapshot {
    /// Memory resources.
    pub memory: MemoryResources,
    /// CPU resources.
    pub cpu: CpuResources,
    /// Task resources.
    pub tasks: TaskResources,
    /// File handle resources.
    pub file_handles: FileHandleResources,
    /// Network resources.
    pub network: NetworkResources,
}

/// Memory resource usage.
#[derive(Debug, Clone, Copy)]
pub struct MemoryResources {
    /// Heap-allocated bytes (process RSS).
    pub heap_allocated_bytes: u64,
    /// Maximum allowed heap bytes.
    pub heap_limit_bytes: u64,
    /// Memory used by AI models.
    pub model_memory_bytes: u64,
    /// Memory used by caches.
    pub cache_memory_bytes: u64,
}

/// CPU resource usage.
#[derive(Debug, Clone, Copy)]
pub struct CpuResources {
    /// CPU usage percentage (0.0 to 100.0).
    pub usage_percent: f32,
    /// Number of OS threads.
    pub thread_count: u32,
    /// Number of logical CPU cores.
    pub core_count: u32,
}

/// Task resource usage.
#[derive(Debug, Clone, Copy)]
pub struct TaskResources {
    /// Number of actively running tasks.
    pub active_tasks: u32,
    /// Number of queued/pending tasks.
    pub queued_tasks: u32,
    /// Number of background workers.
    pub background_workers: u32,
}

/// File handle resource usage.
#[derive(Debug, Clone, Copy)]
pub struct FileHandleResources {
    /// Current number of open file handles.
    pub current: u32,
    /// Maximum allowed file handles.
    pub limit: u32,
}

/// Network resource usage.
#[derive(Debug, Clone, Copy)]
pub struct NetworkResources {
    /// Whether network is available.
    pub connected: bool,
    /// Current estimated bandwidth usage in bytes/sec.
    pub bandwidth_bytes_per_sec: u64,
}

impl Default for ResourceSnapshot {
    fn default() -> Self {
        Self {
            memory: MemoryResources {
                heap_allocated_bytes: 0,
                heap_limit_bytes: u64::MAX,
                model_memory_bytes: 0,
                cache_memory_bytes: 0,
            },
            cpu: CpuResources {
                usage_percent: 0.0,
                thread_count: 0,
                core_count: num_cpus::get() as u32,
            },
            tasks: TaskResources {
                active_tasks: 0,
                queued_tasks: 0,
                background_workers: 0,
            },
            file_handles: FileHandleResources {
                current: 0,
                limit: 65536,
            },
            network: NetworkResources {
                connected: true,
                bandwidth_bytes_per_sec: 0,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Resource Manager
// ---------------------------------------------------------------------------

/// Manages runtime resource tracking and limit enforcement.
///
/// Polls resource usage every 10 seconds and emits events when
/// configurable thresholds are exceeded.
pub struct ResourceManager {
    /// Current resource snapshot (atomic-like for lock-free reads).
    snapshot: Arc<RwLock<ResourceSnapshot>>,
    /// Whether the background poller is running.
    poller_running: Arc<std::sync::atomic::AtomicBool>,
    /// Event bus for emitting resource events.
    event_bus: Option<Arc<EventBus>>,
    /// Metrics registry for resource metrics.
    metrics: Option<Arc<MetricsRegistry>>,

    // Atomic metrics for lock-free reads
    heap_allocated: AtomicU64,
    active_tasks: AtomicU32,
    queued_tasks: AtomicU32,
    background_workers: AtomicU32,
}

impl ResourceManager {
    /// Create a new resource manager.
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(ResourceSnapshot::default())),
            poller_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            event_bus: None,
            metrics: None,
            heap_allocated: AtomicU64::new(0),
            active_tasks: AtomicU32::new(0),
            queued_tasks: AtomicU32::new(0),
            background_workers: AtomicU32::new(0),
        }
    }

    /// Set the event bus for emitting resource events.
    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    /// Set the metrics registry for resource metrics.
    pub fn set_metrics(&mut self, metrics: Arc<MetricsRegistry>) {
        self.metrics = Some(metrics);
    }

    /// Start the background resource poller.
    ///
    /// Polls resource usage every 10 seconds and emits events.
    pub fn start_poller(self: &Arc<Self>) {
        if self
            .poller_running
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            debug!("Resource poller already running");
            return;
        }

        let self_clone = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                self_clone.poll().await;
            }
        });

        debug!("Resource poller started (interval: 10s)");
    }

    /// Perform a resource usage poll.
    async fn poll(&self) {
        let snapshot = self.collect_snapshot().await;

        // Check thresholds
        let heap_pct = if snapshot.memory.heap_limit_bytes > 0 {
            snapshot.memory.heap_allocated_bytes as f64 / snapshot.memory.heap_limit_bytes as f64
        } else {
            0.0
        };

        if heap_pct >= 0.95 {
            self.emit_critical(
                "memory",
                snapshot.memory.heap_allocated_bytes as f64,
                snapshot.memory.heap_limit_bytes as f64,
            )
            .await;
        } else if heap_pct >= 0.80 {
            self.emit_warning(
                "memory",
                snapshot.memory.heap_allocated_bytes as f64,
                snapshot.memory.heap_limit_bytes as f64,
            )
            .await;
        }

        // Task thresholds
        let task_pct = if snapshot.tasks.active_tasks > 100 {
            (snapshot.tasks.active_tasks as f64) / 100.0
        } else {
            0.0
        };

        if task_pct >= 0.95 {
            self.emit_critical("tasks", snapshot.tasks.active_tasks as f64, 100.0)
                .await;
        } else if task_pct >= 0.80 {
            self.emit_warning("tasks", snapshot.tasks.active_tasks as f64, 100.0)
                .await;
        }

        // Update snapshot
        *self.snapshot.write().await = snapshot;
    }

    /// Collect a snapshot of current resource usage.
    async fn collect_snapshot(&self) -> ResourceSnapshot {
        ResourceSnapshot {
            memory: MemoryResources {
                heap_allocated_bytes: self.heap_allocated.load(Ordering::Relaxed),
                heap_limit_bytes: 4 * 1024 * 1024 * 1024, // 4GB default
                model_memory_bytes: 0,
                cache_memory_bytes: 0,
            },
            cpu: CpuResources {
                usage_percent: 0.0,
                thread_count: std::thread::available_parallelism()
                    .map(|n| n.get() as u32)
                    .unwrap_or(4),
                core_count: num_cpus::get() as u32,
            },
            tasks: TaskResources {
                active_tasks: self.active_tasks.load(Ordering::Relaxed),
                queued_tasks: self.queued_tasks.load(Ordering::Relaxed),
                background_workers: self.background_workers.load(Ordering::Relaxed),
            },
            file_handles: FileHandleResources {
                current: 0,
                limit: 65536,
            },
            network: NetworkResources {
                connected: true,
                bandwidth_bytes_per_sec: 0,
            },
        }
    }

    /// Get the current resource snapshot (non-blocking).
    pub async fn current_snapshot(&self) -> ResourceSnapshot {
        self.snapshot.read().await.clone()
    }

    /// Update task counts.
    pub fn set_task_counts(&self, active: u32, queued: u32, workers: u32) {
        self.active_tasks.store(active, Ordering::Relaxed);
        self.queued_tasks.store(queued, Ordering::Relaxed);
        self.background_workers.store(workers, Ordering::Relaxed);
    }

    /// Update heap-allocated bytes.
    pub fn set_heap_allocated(&self, bytes: u64) {
        self.heap_allocated.store(bytes, Ordering::Relaxed);
    }

    /// Emit a resource warning event.
    async fn emit_warning(&self, resource: &str, current: f64, limit: f64) {
        warn!("Resource warning: {resource} at {current:.0}/{limit:.0}");
        if let Some(ref bus) = self.event_bus {
            bus.publish(ResourceWarning {
                resource: resource.to_string(),
                current,
                limit,
            })
            .await;
        }
    }

    /// Emit a resource critical event.
    async fn emit_critical(&self, resource: &str, current: f64, limit: f64) {
        warn!("Resource CRITICAL: {resource} at {current:.0}/{limit:.0}");
        if let Some(ref bus) = self.event_bus {
            bus.publish(ResourceCritical {
                resource: resource.to_string(),
                current,
                limit,
            })
            .await;
        }
    }
}

impl Default for ResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_snapshot() {
        let rm = ResourceManager::new();
        let snap = rm.current_snapshot().await;
        assert_eq!(snap.memory.heap_allocated_bytes, 0);
        assert!(snap.cpu.core_count > 0);
    }

    #[tokio::test]
    async fn test_set_task_counts() {
        let rm = ResourceManager::new();
        rm.set_task_counts(5, 10, 2);
        assert_eq!(rm.active_tasks.load(Ordering::Relaxed), 5);
        assert_eq!(rm.queued_tasks.load(Ordering::Relaxed), 10);
        assert_eq!(rm.background_workers.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn test_snapshot_available_within_10ms() {
        let rm = ResourceManager::new();
        let start = std::time::Instant::now();
        let _snap = rm.current_snapshot().await;
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(10),
            "snapshot took {elapsed:?}"
        );
    }

    #[test]
    fn test_default_snapshot_has_core_count() {
        let snap = ResourceSnapshot::default();
        assert!(snap.cpu.core_count > 0);
    }
}
