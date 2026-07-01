//! # Process Monitor
//!
//! Health aggregation for managed processes.
//!
//! Collects health status from all managed processes and computes
//! an aggregate platform health state. Integrates with
//! `lumas_runtime::health::HealthMonitor` for runtime-level health
//! reporting.

use crate::id::ProcessId;
use crate::lifecycle::ProcessState;
use crate::metrics::ProcessMetrics;
use crate::registry::ProcessRegistry;
use lumas_runtime::service::{HealthStatus, ServiceHealth};
use std::collections::HashMap;
use std::sync::Arc;

/// Aggregates health information from all managed processes.
pub struct ProcessMonitor {
    /// Process registry for querying process states.
    registry: Arc<ProcessRegistry>,
    /// Process metrics.
    metrics: Arc<ProcessMetrics>,
}

impl ProcessMonitor {
    /// Create a new process monitor.
    pub fn new(registry: Arc<ProcessRegistry>, metrics: Arc<ProcessMetrics>) -> Self {
        Self { registry, metrics }
    }

    /// Returns the health status of a single process.
    pub fn process_health(&self, id: &ProcessId) -> ServiceHealth {
        match self.registry.get(id) {
            Some(handle) => {
                let state = handle.state();
                match state {
                    ProcessState::Ready | ProcessState::Running => {
                        ServiceHealth::healthy(format!("process running ({:?})", state))
                    }
                    ProcessState::Busy => ServiceHealth {
                        status: HealthStatus::Degraded,
                        score: 0.7,
                        message: "process is saturated".into(),
                        last_checked: chrono::Utc::now(),
                        consecutive_failures: 0,
                    },
                    ProcessState::Crashed | ProcessState::Failed => ServiceHealth {
                        status: HealthStatus::Unhealthy,
                        score: 0.0,
                        message: format!("process is {:?}", state),
                        last_checked: chrono::Utc::now(),
                        consecutive_failures: 1,
                    },
                    _ => ServiceHealth {
                        status: HealthStatus::Degraded,
                        score: 0.5,
                        message: format!("process is {:?}", state),
                        last_checked: chrono::Utc::now(),
                        consecutive_failures: 0,
                    },
                }
            }
            None => ServiceHealth {
                status: HealthStatus::Unknown,
                score: 0.0,
                message: "process not found".into(),
                last_checked: chrono::Utc::now(),
                consecutive_failures: 0,
            },
        }
    }

    /// Returns the aggregate health status of all processes.
    pub fn aggregate_health(&self) -> AggregateHealth {
        let states = self.registry.all_states();
        let mut total = 0;
        let mut healthy = 0;
        let mut unhealthy = 0;

        for state in states.iter() {
            total += 1;
            match *state.value() {
                ProcessState::Ready | ProcessState::Running => healthy += 1,
                ProcessState::Crashed | ProcessState::Failed => unhealthy += 1,
                _ => {}
            }
        }

        let status = if unhealthy > 0 {
            HealthStatus::Unhealthy
        } else if healthy == total {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        };

        AggregateHealth {
            status,
            total_processes: total,
            healthy_count: healthy,
            unhealthy_count: unhealthy,
            score: if total > 0 {
                healthy as f32 / total as f32
            } else {
                1.0
            },
        }
    }

    /// Returns a map of all process states.
    pub fn all_states(&self) -> HashMap<ProcessId, ProcessState> {
        let mut map = HashMap::new();
        for entry in self.registry.all_states().iter() {
            map.insert(entry.key().clone(), *entry.value());
        }
        map
    }
}

/// Aggregate health state of all managed processes.
#[derive(Debug, Clone)]
pub struct AggregateHealth {
    /// Overall health status.
    pub status: HealthStatus,
    /// Total number of processes.
    pub total_processes: u32,
    /// Number of healthy processes.
    pub healthy_count: u32,
    /// Number of unhealthy (crashed/failed) processes.
    pub unhealthy_count: u32,
    /// Health score (0.0–1.0).
    pub score: f32,
}

impl std::fmt::Debug for ProcessMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessMonitor").finish()
    }
}
