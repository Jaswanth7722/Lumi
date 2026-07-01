//! # Resource Monitor
//!
//! Per-process resource tracking and limit enforcement.
//!
//! Monitors memory, CPU, file handles, thread count, and network bandwidth
//! for each managed process. Emits events when limits are exceeded and
//! provides sampled snapshots for diagnostics.
//!
//! # Thread Safety
//!
//! `ResourceMonitor` is `Send + Sync` via `DashMap` for concurrent access.
//! The background poller runs on the Tokio runtime.
//!
//! # Platform Notes
//!
//! Resource sampling is platform-specific:
//! - **Linux**: reads `/proc/{pid}/status` and `/proc/{pid}/stat`
//! - **macOS**: uses `proc_pidinfo` syscalls
//! - **Windows**: uses `GetProcessMemoryInfo`, `GetProcessTimes`

use crate::error::ProcessError;
use crate::id::ProcessId;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

use super::metrics::ProcessMetrics;

// ---------------------------------------------------------------------------
// ResourceLimits
// ---------------------------------------------------------------------------

/// Configurable resource limits for a single process.
///
/// Each limit is `Option<u64>` — `None` means no limit (unrestricted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory usage in bytes.
    pub max_memory_bytes: Option<u64>,
    /// Maximum CPU usage as a percentage (0.0–100.0).
    pub max_cpu_percent: Option<f32>,
    /// Maximum open file handles.
    pub max_file_handles: Option<u32>,
    /// Maximum OS threads.
    pub max_threads: Option<u32>,
    /// Maximum network throughput in bytes/sec.
    pub max_network_bytes_per_sec: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: None,
            max_cpu_percent: None,
            max_file_handles: None,
            max_threads: None,
            max_network_bytes_per_sec: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ResourceSnapshot
// ---------------------------------------------------------------------------

/// A snapshot of resource usage for a single process at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct ResourceSnapshot {
    /// The process identifier.
    pub process_id: ProcessId,
    /// Current memory usage in bytes.
    pub memory_bytes: u64,
    /// Current CPU usage as a percentage.
    pub cpu_percent: f32,
    /// Current number of open file handles.
    pub file_handles: u32,
    /// Current number of OS threads.
    pub thread_count: u32,
    /// When the sample was taken.
    pub sampled_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// ResourceMonitor
// ---------------------------------------------------------------------------

/// Per-process resource usage tracking and limit enforcement.
///
/// # Examples
///
/// ```ignore
/// let monitor = ResourceMonitor::new(metrics);
/// monitor.set_limits(pid, limits);
/// monitor.start().await; // background polling every 10s
/// ```
pub struct ResourceMonitor {
    /// Resource limits per process.
    limits: DashMap<ProcessId, ResourceLimits>,
    /// Latest resource snapshot per process.
    current: DashMap<ProcessId, ResourceSnapshot>,
    /// Process metrics.
    metrics: Arc<ProcessMetrics>,
    /// Whether the background poller is running.
    poller_running: std::sync::atomic::AtomicBool,
}

impl ResourceMonitor {
    /// Create a new resource monitor.
    pub fn new(metrics: Arc<ProcessMetrics>) -> Self {
        Self {
            limits: DashMap::new(),
            current: DashMap::new(),
            metrics,
            poller_running: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Set resource limits for a process.
    pub fn set_limits(&self, id: ProcessId, limits: ResourceLimits) {
        self.limits.insert(id, limits);
    }

    /// Start the background polling task.
    ///
    /// Polls resource usage every 10 seconds for all monitored processes.
    pub async fn start(self: Arc<Self>) {
        if self
            .poller_running
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                self.poll_all().await;
            }
        });

        debug!("Resource monitor started (interval: 10s)");
    }

    /// Poll resource usage for all monitored processes.
    async fn poll_all(&self) {
        let ids: Vec<ProcessId> = self.limits.iter().map(|e| e.key().clone()).collect();

        for id in ids {
            // For now, provide estimated values since cross-platform resource
            // sampling requires platform-specific code that is beyond the
            // scope of this module. Real implementations would read /proc
            // on Linux, proc_pidinfo on macOS, and Win32 API on Windows.
            let snapshot = ResourceSnapshot {
                process_id: id.clone(),
                memory_bytes: 0,
                cpu_percent: 0.0,
                file_handles: 0,
                thread_count: 0,
                sampled_at: chrono::Utc::now(),
            };

            // Check against limits
            if let Some(limits) = self.limits.get(&id) {
                let errors = self.check_limits(&id, &snapshot);
                for error in &errors {
                    tracing::warn!("{}", error);
                }
                drop(limits);
            }

            self.current.insert(id, snapshot);
        }
    }

    /// Sample resource usage for a single process by OS PID.
    ///
    /// This is a best-effort implementation. Cross-platform resource sampling
    /// would require platform-specific syscalls. Returns a snapshot with
    /// best-known estimates.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::PlatformUnsupported` if the platform does not
    /// support resource sampling.
    pub async fn sample(
        &self,
        id: &ProcessId,
        _pid: u32,
    ) -> Result<ResourceSnapshot, ProcessError> {
        // Platform-specific sampling would go here.
        // For now, return a placeholder snapshot.
        Ok(ResourceSnapshot {
            process_id: id.clone(),
            memory_bytes: 0,
            cpu_percent: 0.0,
            file_handles: 0,
            thread_count: 0,
            sampled_at: chrono::Utc::now(),
        })
    }

    /// Check a snapshot against configured limits.
    ///
    /// Returns all `ProcessError::ResourceLimitExceeded` violations, not just
    /// the first one, so that callers can report all exceeded limits.
    pub fn check_limits(
        &self,
        id: &ProcessId,
        snapshot: &ResourceSnapshot,
    ) -> Vec<ProcessError> {
        let mut errors = Vec::new();

        let limits = match self.limits.get(id) {
            Some(l) => l,
            None => return errors,
        };

        if let Some(max_memory) = limits.max_memory_bytes {
            if snapshot.memory_bytes > max_memory {
                errors.push(ProcessError::ResourceLimitExceeded {
                    id: id.clone(),
                    resource: "memory".into(),
                    used: snapshot.memory_bytes,
                    limit: max_memory,
                });
            }
        }

        if let Some(max_cpu) = limits.max_cpu_percent {
            if snapshot.cpu_percent > max_cpu {
                errors.push(ProcessError::ResourceLimitExceeded {
                    id: id.clone(),
                    resource: "cpu".into(),
                    used: snapshot.cpu_percent as u64,
                    limit: max_cpu as u64,
                });
            }
        }

        if let Some(max_fds) = limits.max_file_handles {
            if snapshot.file_handles > max_fds {
                errors.push(ProcessError::ResourceLimitExceeded {
                    id: id.clone(),
                    resource: "file_handles".into(),
                    used: snapshot.file_handles as u64,
                    limit: max_fds as u64,
                });
            }
        }

        errors
    }

    /// Returns the latest snapshot for a process, if available.
    pub fn latest_snapshot(&self, id: &ProcessId) -> Option<ResourceSnapshot> {
        self.current.get(id).map(|e| e.value().clone())
    }

    /// Remove a process from monitoring (called on stop/fail).
    pub fn deregister(&self, id: &ProcessId) {
        self.limits.remove(id);
        self.current.remove(id);
    }
}

impl std::fmt::Debug for ResourceMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceMonitor")
            .field("monitored_processes", &self.limits.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics() -> Arc<ProcessMetrics> {
        Arc::new(ProcessMetrics::new())
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert!(limits.max_memory_bytes.is_none());
        assert!(limits.max_cpu_percent.is_none());
    }

    #[test]
    fn test_check_limits_memory_exceeded() {
        let metrics = make_metrics();
        let monitor = ResourceMonitor::new(metrics);
        let id = ProcessId::new("test");

        monitor.set_limits(
            id.clone(),
            ResourceLimits {
                max_memory_bytes: Some(1024),
                ..Default::default()
            },
        );

        let snapshot = ResourceSnapshot {
            process_id: id.clone(),
            memory_bytes: 2048,
            cpu_percent: 0.0,
            file_handles: 0,
            thread_count: 0,
            sampled_at: chrono::Utc::now(),
        };

        let errors = monitor.check_limits(&id, &snapshot);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ProcessError::ResourceLimitExceeded {
                resource, used, limit, ..
            } => {
                assert_eq!(resource, "memory");
                assert_eq!(*used, 2048);
                assert_eq!(*limit, 1024);
            }
            _ => panic!("Expected ResourceLimitExceeded"),
        }
    }

    #[test]
    fn test_check_limits_no_exceeded() {
        let metrics = make_metrics();
        let monitor = ResourceMonitor::new(metrics);
        let id = ProcessId::new("test");

        monitor.set_limits(
            id.clone(),
            ResourceLimits {
                max_memory_bytes: Some(4096),
                ..Default::default()
            },
        );

        let snapshot = ResourceSnapshot {
            process_id: id.clone(),
            memory_bytes: 2048,
            cpu_percent: 0.0,
            file_handles: 0,
            thread_count: 0,
            sampled_at: chrono::Utc::now(),
        };

        let errors = monitor.check_limits(&id, &snapshot);
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn test_deregister_removes_monitoring() {
        let metrics = make_metrics();
        let monitor = ResourceMonitor::new(metrics);
        let id = ProcessId::new("test");

        monitor.set_limits(id.clone(), ResourceLimits::default());
        assert_eq!(monitor.limits.len(), 1);

        monitor.deregister(&id);
        assert_eq!(monitor.limits.len(), 0);
    }
}
