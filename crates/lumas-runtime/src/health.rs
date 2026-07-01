//! # Health Monitor
//!
//! Runs periodic health checks on all registered services and maintains
//! aggregate platform health state.
//!
//! The monitor runs checks concurrently on a configurable interval,
//! maintains a rolling window of results per service, computes an
//! aggregate health score, and emits events on state changes.
//!
//! # Thread Safety
//!
//! `HealthMonitor` is `Send + Sync`. Service health data is protected
//! by `RwLock`; event publishing is non-blocking.
//!
//! # Errors
//!
//! Health check failures produce `HealthCheckFailed` events. Persistent
//! failures trigger `RuntimeDegraded` and eventual `RuntimeRecovered` events.

use crate::error::HealthError;
use crate::event::{
    EventBus, HealthCheckFailed, HealthCheckPassed, RuntimeDegraded, RuntimeRecovered,
};
use crate::service::{HealthStatus, Service, ServiceHealth, ServiceManager};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Number of recent health results to retain per service.
const ROLLING_WINDOW_SIZE: usize = 10;

/// Platform health snapshot.
#[derive(Debug, Clone)]
pub struct PlatformHealth {
    /// Overall platform health status.
    pub status: HealthStatus,
    /// Aggregate health score (0.0 to 1.0).
    pub score: f32,
    /// Per-service health results.
    pub services: HashMap<String, ServiceHealth>,
    /// When the health check was performed.
    pub checked_at: DateTime<Utc>,
    /// Runtime uptime in seconds.
    pub uptime_secs: u64,
}

// ---------------------------------------------------------------------------
// Health Monitor
// ---------------------------------------------------------------------------

/// Monitors service health and maintains aggregate platform health state.
///
/// Runs concurrent health checks every `check_interval` seconds and
/// emits events when service health changes.
pub struct HealthMonitor {
    /// Service manager for health check dispatch. Interior mutability via tokio::sync::Mutex
    /// because this is set once during bootstrap on an Arc<HealthMonitor>.
    service_manager: tokio::sync::Mutex<Option<Arc<RwLock<ServiceManager>>>>,
    /// Rolling window of health results per service.
    results: RwLock<HashMap<String, VecDeque<ServiceHealth>>>,
    /// Whether the runtime is in degraded state.
    degraded: RwLock<bool>,
    /// Event bus for emitting health events. Interior mutability via tokio::sync::Mutex
    /// because this is set once during bootstrap on an Arc<HealthMonitor>.
    event_bus: tokio::sync::Mutex<Option<Arc<EventBus>>>,
    /// Check interval in seconds.
    check_interval_secs: u64,
    /// Per-check timeout in seconds.
    check_timeout_secs: u64,
    /// Runtime uptime start.
    started_at: tokio::time::Instant,
}

impl HealthMonitor {
    /// Create a new health monitor.
    ///
    /// By default, checks run every 30 seconds with a 5-second per-check timeout.
    pub fn new() -> Self {
        Self {
            service_manager: tokio::sync::Mutex::new(None),
            results: RwLock::new(HashMap::new()),
            degraded: RwLock::new(false),
            event_bus: tokio::sync::Mutex::new(None),
            check_interval_secs: 30,
            check_timeout_secs: 5,
            started_at: tokio::time::Instant::now(),
        }
    }

    /// Create a health monitor with custom intervals.
    pub fn with_intervals(check_interval_secs: u64, check_timeout_secs: u64) -> Self {
        Self {
            service_manager: tokio::sync::Mutex::new(None),
            results: RwLock::new(HashMap::new()),
            degraded: RwLock::new(false),
            event_bus: tokio::sync::Mutex::new(None),
            check_interval_secs,
            check_timeout_secs,
            started_at: tokio::time::Instant::now(),
        }
    }

    /// Set the service manager for health check dispatch.
    ///
    /// Uses interior mutability so this can be called on an `Arc<HealthMonitor>`.
    pub async fn set_service_manager(&self, sm: Arc<RwLock<ServiceManager>>) {
        *self.service_manager.lock().await = Some(sm);
    }

    /// Set the event bus for emitting health events.
    ///
    /// Uses interior mutability so this can be called on an `Arc<HealthMonitor>`.
    pub async fn set_event_bus(&self, event_bus: Arc<EventBus>) {
        *self.event_bus.lock().await = Some(event_bus);
    }

    /// Start the health check loop.
    ///
    /// Runs health checks on all registered services every `check_interval_secs`.
    pub fn start(self: &Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(this.check_interval_secs)).await;
                this.run_checks().await;
            }
        });
        info!(
            "Health monitor started (interval: {}s, timeout: {}s)",
            self.check_interval_secs, self.check_timeout_secs
        );
    }

    /// Run health checks on all registered services.
    async fn run_checks(&self) {
        debug!("Running health checks...");

        let sm = {
            let sm_guard = self.service_manager.lock().await;
            match sm_guard.as_ref() {
                Some(sm) => sm.clone(),
                None => return,
            }
        };

        let service_names: Vec<String> = {
            match sm.read().await.resolve_startup_order() {
                Ok(names) => names.into_iter().map(|n| n.to_string()).collect(),
                Err(_) => return,
            }
        };
        drop(sm);

        let mut results = HashMap::new();

        for name in &service_names {
            let result = self.check_service(name).await;
            results.insert(name.clone(), result);
        }

        // Update rolling windows
        let mut store = self.results.write().await;
        for (name, health) in &results {
            store
                .entry(name.clone())
                .or_insert_with(|| VecDeque::with_capacity(ROLLING_WINDOW_SIZE))
                .push_back(health.clone());
            if store
                .get(name)
                .map_or(false, |w| w.len() > ROLLING_WINDOW_SIZE)
            {
                if let Some(window) = store.get_mut(name) {
                    window.pop_front();
                }
            }
        }
        drop(store);

        // Compute aggregate health
        let platform_health = self.compute_platform_health(&results).await;

        // Get event bus for publishing
        let event_bus = {
            let eb_guard = self.event_bus.lock().await;
            eb_guard.as_ref().cloned()
        };

        // Emit events for unhealthy services
        if let Some(ref bus) = event_bus {
            for (name, health) in &results {
                if health.status == HealthStatus::Unhealthy {
                    bus.publish(HealthCheckFailed {
                        service: name.clone(),
                        reason: health.message.clone(),
                    })
                    .await;
                } else if health.status == HealthStatus::Healthy {
                    bus.publish(HealthCheckPassed {
                        service: name.clone(),
                        score: health.score,
                    })
                    .await;
                }
            }

            // Emit degraded/recovered events
            let was_degraded = *self.degraded.read().await;
            let is_degraded = platform_health.status == HealthStatus::Unhealthy;

            if is_degraded && !was_degraded {
                *self.degraded.write().await = true;
                let failed: Vec<String> = results
                    .iter()
                    .filter(|(_, h)| h.status == HealthStatus::Unhealthy)
                    .map(|(n, _)| n.clone())
                    .collect();
                bus.publish(RuntimeDegraded {
                    failed_services: failed,
                })
                .await;
            } else if !is_degraded && was_degraded {
                *self.degraded.write().await = false;
                let recovered: Vec<String> = results.keys().cloned().collect();
                bus.publish(RuntimeRecovered {
                    recovered_services: recovered,
                })
                .await;
            }
        }
    }

    /// Check a single service's health.
    async fn check_service(&self, name: &str) -> ServiceHealth {
        let sm = {
            let sm_guard = self.service_manager.lock().await;
            match sm_guard.as_ref() {
                Some(sm) => sm.clone(),
                None => return ServiceHealth::unhealthy("No service manager", 0),
            }
        };

        let service = sm.read().await.get_service(name);
        drop(sm);

        match service {
            Some(svc) => {
                let result = tokio::time::timeout(
                    Duration::from_secs(self.check_timeout_secs),
                    svc.health_check(),
                )
                .await;

                match result {
                    Ok(health) => health,
                    Err(_) => ServiceHealth::unhealthy(
                        format!("Health check timed out after {}s", self.check_timeout_secs),
                        0,
                    ),
                }
            }
            None => ServiceHealth::unhealthy("Service not found", 0),
        }
    }

    /// Compute aggregate platform health from per-service results.
    async fn compute_platform_health(
        &self,
        results: &HashMap<String, ServiceHealth>,
    ) -> PlatformHealth {
        if results.is_empty() {
            return PlatformHealth {
                status: HealthStatus::Unknown,
                score: 0.0,
                services: HashMap::new(),
                checked_at: Utc::now(),
                uptime_secs: self.started_at.elapsed().as_secs(),
            };
        }

        let mut total_score = 0.0f32;
        let mut has_unhealthy = false;

        for (_, health) in results {
            total_score += health.score;
            if health.status == HealthStatus::Unhealthy {
                has_unhealthy = true;
            }
        }

        let avg_score = total_score / results.len() as f32;

        let status = if has_unhealthy {
            HealthStatus::Unhealthy
        } else if avg_score < 0.8 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        PlatformHealth {
            status,
            score: avg_score,
            services: results.clone(),
            checked_at: Utc::now(),
            uptime_secs: self.started_at.elapsed().as_secs(),
        }
    }

    /// Get the current overall platform health.
    pub async fn overall_health(&self) -> PlatformHealth {
        let results = self.results.read().await;
        let latest: HashMap<String, ServiceHealth> = results
            .iter()
            .filter_map(|(name, window)| window.back().map(|h| (name.clone(), h.clone())))
            .collect();
        drop(results);

        self.compute_platform_health(&latest).await
    }

    /// Get the degraded state.
    pub async fn is_degraded(&self) -> bool {
        *self.degraded.read().await
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::ServiceManager;

    #[tokio::test]
    async fn test_empty_health_returns_unknown() {
        let monitor = HealthMonitor::new();
        let health = monitor.overall_health().await;
        assert_eq!(health.status, HealthStatus::Unknown);
    }

    #[tokio::test]
    async fn test_aggregate_score_empty() {
        let monitor = HealthMonitor::new();
        let results = HashMap::new();
        let platform = monitor.compute_platform_health(&results).await;
        assert_eq!(platform.status, HealthStatus::Unknown);
    }

    #[test]
    fn test_uptime_tracking() {
        let monitor = HealthMonitor::new();
        let uptime = monitor.started_at.elapsed().as_secs();
        assert!(uptime < 5); // Should be near-zero
    }
}
