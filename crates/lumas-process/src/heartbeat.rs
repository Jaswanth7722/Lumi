//! # Heartbeat Manager
//!
//! Liveness detection for managed processes using heartbeat signals.
//!
//! Every process sends periodic heartbeat signals to demonstrate it is
//! still alive and functioning. The heartbeat manager tracks these signals
//! and detects when a process has stopped sending them, triggering a
//! supervisor intervention.
//!
//! # Thread Safety
//!
//! `HeartbeatManager` is `Send + Sync` via `DashMap` and atomic operations.
//! Heartbeat reception is lock-free using `AtomicI64` for the timestamp.
//!
//! # Design
//!
//! - Each process has a `HeartbeatMonitor` with configurable interval,
//!   timeout, and max missed count.
//! - Heartbeats are received via `crossbeam_channel` for non-blocking delivery.
//! - A background checker task runs every 1 second and evaluates all monitors.

use crate::error::ProcessError;
use crate::event::{HeartbeatMissed, HeartbeatRecovered};
use crate::id::ProcessId;
use crate::lifecycle::ProcessState;
use crossbeam_channel::{Receiver, Sender};
use dashmap::DashMap;
use lumas_runtime::event::{Event, EventBus};
use lumas_runtime::service::HealthStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use super::metrics::ProcessMetrics;
use super::supervisor::{SupervisorCommand, SupervisorHandle};

// ---------------------------------------------------------------------------
// HeartbeatSignal
// ---------------------------------------------------------------------------

/// Signals sent from processes to the heartbeat manager.
#[derive(Debug, Clone)]
pub enum HeartbeatSignal {
    /// A normal heartbeat pulse from a process.
    Pulse {
        /// The process sending the heartbeat.
        id: ProcessId,
        /// Metadata about the process state.
        metadata: HeartbeatMetadata,
    },
    /// A termination signal (process stopped intentionally).
    Terminated {
        /// The process that terminated.
        id: ProcessId,
    },
}

// ---------------------------------------------------------------------------
// HeartbeatConfig
// ---------------------------------------------------------------------------

/// Configuration for heartbeat-based liveness detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    /// How often the process should send heartbeats (milliseconds).
    pub interval_ms: u64,
    /// How long without a heartbeat before considering it missed (milliseconds).
    pub timeout_ms: u64,
    /// Consecutive missed heartbeats before declaring the process dead.
    pub max_missed: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_ms: 1_000,
            timeout_ms: 5_000,
            max_missed: 3,
        }
    }
}

// ---------------------------------------------------------------------------
// HeartbeatMetadata
// ---------------------------------------------------------------------------

/// Metadata carried with each heartbeat signal.
#[derive(Debug, Clone)]
pub struct HeartbeatMetadata {
    /// Current process state.
    pub state: ProcessState,
    /// Load percentage (0.0–100.0).
    pub load_percent: f32,
    /// Number of active tasks.
    pub active_tasks: u32,
    /// Current memory usage in bytes.
    pub memory_bytes: u64,
    /// Custom key-value metadata.
    pub custom: HashMap<String, String>,
}

impl Default for HeartbeatMetadata {
    fn default() -> Self {
        Self {
            state: ProcessState::Running,
            load_percent: 0.0,
            active_tasks: 0,
            memory_bytes: 0,
            custom: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// HeartbeatMonitor (internal)
// ---------------------------------------------------------------------------

/// Per-process heartbeat tracking state.
struct HeartbeatMonitor {
    /// Heartbeat configuration.
    config: HeartbeatConfig,
    /// Last heartbeat receive time as Unix timestamp milliseconds.
    last_received: AtomicI64,
    /// Consecutive missed heartbeats.
    consecutive_missed: AtomicU32,
    /// Channel to send supervisor commands when process is declared dead.
    supervisor_tx: mpsc::UnboundedSender<SupervisorCommand>,
}

impl HeartbeatMonitor {
    fn new(
        config: HeartbeatConfig,
        supervisor_tx: mpsc::UnboundedSender<SupervisorCommand>,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        Self {
            config,
            last_received: AtomicI64::new(now),
            consecutive_missed: AtomicU32::new(0),
            supervisor_tx,
        }
    }

    fn record_heartbeat(&self) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        self.last_received.store(now, Ordering::Release);
        self.consecutive_missed.store(0, Ordering::Release);
    }

    fn elapsed_ms(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let last = self.last_received.load(Ordering::Acquire);
        (now - last).max(0) as u64
    }

    /// Check for missed heartbeats.
    /// Returns an optional error with a placeholder ID (caller should set the real ID).
    fn check(&self, actual_id: &ProcessId) -> Option<ProcessError> {
        let elapsed = self.elapsed_ms();
        if elapsed > self.config.timeout_ms {
            let missed = self.consecutive_missed.fetch_add(1, Ordering::AcqRel) + 1;
            if missed >= self.config.max_missed {
                return Some(ProcessError::HeartbeatTimeout {
                    id: actual_id.clone(),
                    elapsed_ms: elapsed,
                });
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// HeartbeatManager
// ---------------------------------------------------------------------------

/// Manages heartbeat-based liveness detection for all processes.
///
/// # Examples
///
/// ```ignore
/// let hb_manager = HeartbeatManager::new(event_bus, metrics);
/// hb_manager.register(pid, config, supervisor_tx);
/// hb_manager.start_checker().await;
/// ```
pub struct HeartbeatManager {
    /// Per-process heartbeat monitors.
    monitors: DashMap<ProcessId, HeartbeatMonitor>,
    /// Event bus for emitting heartbeat events.
    event_bus: Arc<EventBus>,
    /// Process metrics.
    metrics: Arc<ProcessMetrics>,
}

impl HeartbeatManager {
    /// Create a new heartbeat manager.
    pub fn new(event_bus: Arc<EventBus>, metrics: Arc<ProcessMetrics>) -> Self {
        Self {
            monitors: DashMap::new(),
            event_bus,
            metrics,
        }
    }

    /// Register a process for heartbeat monitoring.
    ///
    /// # Parameters
    ///
    /// * `id` — The process identifier.
    /// * `config` — Heartbeat configuration.
    /// * `supervisor_tx` — Channel to the supervisor for crash notifications.
    pub fn register(
        &self,
        id: ProcessId,
        config: HeartbeatConfig,
        supervisor_tx: mpsc::UnboundedSender<SupervisorCommand>,
    ) {
        let monitor = HeartbeatMonitor::new(config, supervisor_tx);
        self.monitors.insert(id, monitor);
    }

    /// Record a received heartbeat for a process.
    ///
    /// Called by `ProcessHandle::heartbeat()` from within the managed process.
    /// Non-blocking, lock-free via `AtomicI64`.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::NotFound` if the process is not registered.
    pub fn record(
        &self,
        id: &ProcessId,
        metadata: HeartbeatMetadata,
    ) -> Result<(), ProcessError> {
        let monitor = self
            .monitors
            .get(id)
            .ok_or_else(|| ProcessError::NotFound { id: id.clone() })?;

        // Reset missed counter if there were prior misses.
        let prior_missed = monitor.consecutive_missed.load(Ordering::Acquire);
        if prior_missed > 0 {
            self.metrics.total_heartbeats_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let event = HeartbeatRecovered {
                id: id.clone(),
                missed_count: prior_missed,
                recovered_at: chrono::Utc::now(),
            };
            // Fire-and-forget event emission
            let bus = self.event_bus.clone();
            tokio::spawn(async move {
                bus.publish(event).await;
            });
        }

        monitor.record_heartbeat();
        self.metrics.total_heartbeats_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    }

    /// Start the background checker task.
    ///
    /// Runs every 1 second, checking all monitors for missed heartbeats.
    /// When a process is declared dead, sends a `SupervisorCommand::HeartbeatTimeout`
    /// to the supervisor.
    pub async fn start_checker(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                self.check_all().await;
            }
        });
    }

    /// Check all monitors for missed heartbeats.
    async fn check_all(&self) {
        let mut to_notify: Vec<(ProcessId, ProcessError)> = Vec::new();

        for entry in self.monitors.iter() {
            let id = entry.key().clone();
            let monitor = entry.value();

            if let Some(err) = monitor.check(&id) {
                self.metrics.total_heartbeats_missed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // Emit HeartbeatMissed event (fire-and-forget)
                let bus = self.event_bus.clone();
                let event_id = id.clone();
                let elapsed = monitor.elapsed_ms();
                let missed = monitor.consecutive_missed.load(Ordering::Acquire);
                tokio::spawn(async move {
                    bus.publish(HeartbeatMissed {
                        id: event_id,
                        elapsed_ms: elapsed,
                        consecutive_count: missed,
                        detected_at: chrono::Utc::now(),
                    })
                    .await;
                });

                to_notify.push((id, err));
            }
        }

        // Notify supervisors (outside the monitor iteration to avoid holding locks)
        for (id, error) in to_notify {
            if let Some(monitor) = self.monitors.get(&id) {
                let cmd = SupervisorCommand::ChildFailure {
                    id: id.clone(),
                    error: Box::new(error),
                };
                let _ = monitor.supervisor_tx.send(cmd);
            }
        }
    }

    /// Deregister a process (called when it reaches Stopped/Failed).
    pub fn deregister(&self, id: &ProcessId) {
        self.monitors.remove(id);
    }

    /// Returns the number of registered monitors.
    pub fn len(&self) -> usize {
        self.monitors.len()
    }

    /// Returns `true` if no monitors are registered.
    pub fn is_empty(&self) -> bool {
        self.monitors.is_empty()
    }

    /// Returns the current health status of a monitored process.
    pub fn health_status(&self, id: &ProcessId) -> HealthStatus {
        match self.monitors.get(id) {
            Some(monitor) => {
                let elapsed = monitor.elapsed_ms();
                if elapsed > monitor.config.timeout_ms * monitor.config.max_missed as u64 {
                    HealthStatus::Unhealthy
                } else if elapsed > monitor.config.timeout_ms {
                    HealthStatus::Degraded
                } else {
                    HealthStatus::Healthy
                }
            }
            None => HealthStatus::Unknown,
        }
    }
}

impl std::fmt::Debug for HeartbeatManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeartbeatManager")
            .field("monitor_count", &self.monitors.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_heartbeat_config_defaults() {
        let config = HeartbeatConfig::default();
        assert_eq!(config.interval_ms, 1_000);
        assert_eq!(config.timeout_ms, 5_000);
        assert_eq!(config.max_missed, 3);
    }

    #[test]
    fn test_heartbeat_metadata_default() {
        let meta = HeartbeatMetadata::default();
        assert_eq!(meta.state, ProcessState::Running);
        assert_eq!(meta.load_percent, 0.0);
    }
}
