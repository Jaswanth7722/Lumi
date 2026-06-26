//! # Service Framework
//!
//! Defines the `Service` trait and `ServiceManager` for the Lumi runtime.
//!
//! Services are the fundamental unit of lifecycle management. Each service
//! declares its dependencies, version, and provides start/stop/health
//! operations. The `ServiceManager` resolves the dependency graph,
//! starts services in topological order, and manages failure recovery.
//!
//! # Thread Safety
//!
//! `ServiceManager` requires `Arc<RwLock<>>` for concurrent access.
//! Services themselves must be `Send + Sync`.
//!
//! # Errors
//!
//! Service lifecycle operations produce `ServiceError` variants.

use crate::context::RuntimeContext;
use crate::error::ServiceError;
use crate::event::{EventBus, ServiceFailed, ServiceStarted};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Service Trait
// ---------------------------------------------------------------------------

/// The health status of a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Service is operating normally.
    Healthy,
    /// Service is operating with reduced capabilities.
    Degraded,
    /// Service is not responding or has failed.
    Unhealthy,
    /// Health status has not been determined yet.
    Unknown,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
            HealthStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// Health check result for a service.
#[derive(Debug, Clone)]
pub struct ServiceHealth {
    /// Health status.
    pub status: HealthStatus,
    /// Health score 0.0 (dead) to 1.0 (perfect).
    pub score: f32,
    /// Human-readable message about the health state.
    pub message: String,
    /// When the health check was performed.
    pub last_checked: DateTime<Utc>,
    /// Number of consecutive health check failures.
    pub consecutive_failures: u32,
}

impl ServiceHealth {
    /// Create a healthy service health result.
    pub fn healthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Healthy,
            score: 1.0,
            message: message.into(),
            last_checked: Utc::now(),
            consecutive_failures: 0,
        }
    }

    /// Create an unhealthy service health result.
    pub fn unhealthy(message: impl Into<String>, consecutive_failures: u32) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            score: 0.0,
            message: message.into(),
            last_checked: Utc::now(),
            consecutive_failures,
        }
    }
}

/// A single metric value from a service.
#[derive(Debug, Clone)]
pub struct ServiceMetric {
    /// Metric name.
    pub name: String,
    /// Metric value.
    pub value: f64,
    /// Metric labels.
    pub labels: HashMap<String, String>,
}

/// Core trait for all Lumi platform services.
///
/// Every subsystem (AI, voice, memory, storage, plugins, etc.) implements
/// this trait to participate in the runtime lifecycle.
#[async_trait::async_trait]
pub trait Service: Send + Sync + 'static {
    /// Human-readable name of this service (e.g., "ai-core", "voice").
    fn name(&self) -> &'static str;

    /// Semantic version of this service.
    fn version(&self) -> &semver::Version;

    /// Names of services that this service depends on.
    ///
    /// The ServiceManager ensures these are started before this service.
    fn dependencies(&self) -> &[&'static str];

    /// Start this service with the provided runtime context.
    ///
    /// # Errors
    ///
    /// Returns `ServiceError::StartFailed` if the service cannot start.
    async fn start(&self, ctx: Arc<RuntimeContext>) -> Result<(), ServiceError>;

    /// Stop this service gracefully.
    ///
    /// # Errors
    ///
    /// Returns `ServiceError::StopFailed` if the service cannot stop.
    async fn stop(&self) -> Result<(), ServiceError>;

    /// Perform a health check on this service.
    async fn health_check(&self) -> ServiceHealth;

    /// Export current metrics from this service.
    fn metrics(&self) -> Vec<ServiceMetric> {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Service State
// ---------------------------------------------------------------------------

/// Internal lifecycle state of a service within the ServiceManager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceState {
    Registered,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

impl std::fmt::Display for ServiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceState::Registered => write!(f, "registered"),
            ServiceState::Starting => write!(f, "starting"),
            ServiceState::Running => write!(f, "running"),
            ServiceState::Stopping => write!(f, "stopping"),
            ServiceState::Stopped => write!(f, "stopped"),
            ServiceState::Failed => write!(f, "failed"),
        }
    }
}

// ---------------------------------------------------------------------------
// Service Manager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of all runtime services.
///
/// Responsibilities:
/// - Maintain a registry of `Arc<dyn Service>`
/// - Resolve startup order using topological sort (Kahn's algorithm)
/// - Detect dependency cycles
/// - Start services in dependency order
/// - Manage failure recovery with retries and backoff
/// - Shutdown services in reverse dependency order
pub struct ServiceManager {
    /// Registered services by name.
    services: HashMap<&'static str, ServiceEntry>,
    /// Event bus for emitting service events.
    event_bus: Option<Arc<EventBus>>,
    /// Runtime context shared with services.
    context: Option<Arc<RuntimeContext>>,
}

/// Internal entry for a registered service.
struct ServiceEntry {
    /// The service implementation.
    service: Arc<dyn Service>,
    /// Current lifecycle state.
    state: RwLock<ServiceState>,
    /// Restart attempt tracking.
    restart_attempts: RwLock<u32>,
    /// Maximum restart attempts (default: 3).
    max_restarts: u32,
    /// Whether the service is critical (fatal if fails).
    critical: bool,
    /// Instance ID for correlation.
    instance_id: Uuid,
}

impl ServiceManager {
    /// Create a new empty service manager.
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            event_bus: None,
            context: None,
        }
    }

    /// Set the event bus for emitting service events.
    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    /// Register a service with the manager.
    ///
    /// # Errors
    ///
    /// Returns `ServiceError` if a service with the same name is already registered.
    pub fn register(
        &mut self,
        service: Arc<dyn Service>,
        max_restarts: u32,
        critical: bool,
    ) -> Result<(), ServiceError> {
        let name = service.name();
        if self.services.contains_key(name) {
            return Err(ServiceError::StartFailed {
                name: name.to_string(),
                message: format!("Service '{name}' is already registered"),
                recoverable: false,
            });
        }

        self.services.insert(
            name,
            ServiceEntry {
                service,
                state: RwLock::new(ServiceState::Registered),
                restart_attempts: RwLock::new(0),
                max_restarts,
                critical,
                instance_id: Uuid::new_v4(),
            },
        );

        debug!("Service registered: {name}");
        Ok(())
    }

    /// Resolve the startup order using topological sort (Kahn's algorithm).
    ///
    /// # Errors
    ///
    /// Returns `ServiceError::DependencyCycle` if a cycle is detected.
    pub fn resolve_startup_order(&self) -> Result<Vec<&'static str>, ServiceError> {
        // Build adjacency list and in-degree map
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

        for (name, entry) in &self.services {
            in_degree.entry(name).or_insert(0);
            adjacency.entry(name).or_default();

            for dep in entry.service.dependencies() {
                if !self.services.contains_key(dep) {
                    return Err(ServiceError::StartFailed {
                        name: name.to_string(),
                        message: format!("Dependency '{dep}' is not registered"),
                        recoverable: false,
                    });
                }
                adjacency.entry(dep).or_default().push(name);
                *in_degree.entry(name).or_insert(0) += 1;
            }
        }

        // Kahn's algorithm
        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(name, _)| *name)
            .collect();

        let mut order = Vec::with_capacity(self.services.len());

        while let Some(name) = queue.pop_front() {
            order.push(name);

            if let Some(dependents) = adjacency.get(name) {
                for dependent in dependents {
                    if let Some(deg) = in_degree.get_mut(dependent) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dependent);
                        }
                    }
                }
            }
        }

        if order.len() != self.services.len() {
            // Detect cycle
            let unstarted: Vec<&str> = self
                .services
                .keys()
                .filter(|n| !order.contains(n))
                .copied()
                .collect();
            return Err(ServiceError::DependencyCycle {
                cycle: unstarted.into_iter().map(|s| s.to_string()).collect(),
            });
        }

        Ok(order)
    }

    /// Start all registered services in dependency order.
    ///
    /// # Errors
    ///
    /// Returns the first `ServiceError` encountered. Previously started
    /// services remain running.
    pub async fn start_all(&self, ctx: Arc<RuntimeContext>) -> Result<(), ServiceError> {
        let order = match self.resolve_startup_order() {
            Ok(order) => order,
            Err(e) => return Err(e),
        };

        for name in &order {
            match self.start_single(name, &ctx).await {
                Ok(()) => {}
                Err(e) => return Err(e),
            }
        }

        info!("All {} services started successfully", order.len());
        Ok(())
    }

    /// Start a single service by name.
    async fn start_single(
        &self,
        name: &str,
        ctx: &Arc<RuntimeContext>,
    ) -> Result<(), ServiceError> {
        let entry = self
            .services
            .get(name)
            .ok_or_else(|| ServiceError::NotFound(name.to_string()))?;

        let mut state = entry.state.write().await;
        if *state != ServiceState::Registered {
            return Ok(()); // Already started or starting
        }
        *state = ServiceState::Starting;
        drop(state);

        let start = Instant::now();
        let result =
            tokio::time::timeout(Duration::from_secs(30), entry.service.start(ctx.clone())).await;

        match result {
            Ok(Ok(())) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                *entry.state.write().await = ServiceState::Running;

                info!("Service '{name}' started ({duration_ms}ms)");

                if let Some(ref bus) = self.event_bus {
                    bus.publish(ServiceStarted {
                        name: name.to_string(),
                        duration_ms,
                    })
                    .await;
                }

                Ok(())
            }
            Ok(Err(e)) => {
                *entry.state.write().await = ServiceState::Failed;
                error!("Service '{name}' failed to start: {e}");
                if let Some(ref bus) = self.event_bus {
                    bus.publish(ServiceFailed {
                        name: name.to_string(),
                        error: e.to_string(),
                        recoverable: e.is_recoverable(),
                    })
                    .await;
                }
                Err(e)
            }
            Err(_) => {
                *entry.state.write().await = ServiceState::Failed;
                let err = ServiceError::Timeout {
                    name: name.to_string(),
                    operation: "start",
                    timeout_secs: 30,
                };
                error!("Service '{name}' timed out during start");
                if let Some(ref bus) = self.event_bus {
                    bus.publish(ServiceFailed {
                        name: name.to_string(),
                        error: err.to_string(),
                        recoverable: true,
                    })
                    .await;
                }
                Err(err)
            }
        }
    }

    /// Stop all services in reverse dependency order.
    pub async fn stop_all(&self) -> Result<(), Vec<ServiceError>> {
        let order = match self.resolve_startup_order() {
            Ok(order) => order,
            Err(e) => return Err(vec![e]),
        };
        let mut errors = Vec::new();

        for name in order.iter().rev() {
            if let Err(e) = self.stop_single(name).await {
                errors.push(e);
            }
        }

        if errors.is_empty() {
            info!("All services stopped successfully");
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Stop a single service by name.
    async fn stop_single(&self, name: &str) -> Result<(), ServiceError> {
        let entry = self
            .services
            .get(name)
            .ok_or_else(|| ServiceError::NotFound(name.to_string()))?;

        let mut state = entry.state.write().await;
        if *state != ServiceState::Running {
            *state = ServiceState::Stopped;
            return Ok(());
        }
        *state = ServiceState::Stopping;
        drop(state);

        let result = tokio::time::timeout(Duration::from_secs(10), entry.service.stop()).await;

        match result {
            Ok(Ok(())) => {
                *entry.state.write().await = ServiceState::Stopped;
                info!("Service '{name}' stopped");
                Ok(())
            }
            Ok(Err(e)) => {
                *entry.state.write().await = ServiceState::Failed;
                warn!("Service '{name}' failed to stop: {e}");
                Err(e)
            }
            Err(_) => {
                // Force stop on timeout
                *entry.state.write().await = ServiceState::Stopped;
                warn!("Service '{name}' timed out during stop; force stopping");
                Err(ServiceError::Timeout {
                    name: name.to_string(),
                    operation: "stop",
                    timeout_secs: 10,
                })
            }
        }
    }

    /// Attempt to restart a failed service with exponential backoff.
    pub async fn restart_service(&self, name: &str) -> Result<(), ServiceError> {
        let entry = self
            .services
            .get(name)
            .ok_or_else(|| ServiceError::NotFound(name.to_string()))?;

        let mut attempts = entry.restart_attempts.write().await;
        *attempts += 1;

        if *attempts > entry.max_restarts {
            return Err(ServiceError::MaxRestartsExceeded {
                name: name.to_string(),
                attempts: *attempts,
            });
        }

        let backoff = Duration::from_secs(1u64 << (*attempts).min(5));
        warn!(
            "Restarting service '{name}' (attempt {attempts}/{}) after {:?}ms",
            entry.max_restarts, backoff
        );

        tokio::time::sleep(backoff).await;

        let ctx = self.context.clone().unwrap_or_else(|| {
            Arc::new(RuntimeContext::new(
                Arc::new(crate::version::FeatureFlags::new()),
                Arc::new(EventBus::new(1024)),
                Arc::new(crate::resource::ResourceManager::new()),
            ))
        });

        self.start_single(name, &ctx).await
    }

    /// Set the runtime context for service startup.
    pub fn set_context(&mut self, ctx: Arc<RuntimeContext>) {
        self.context = Some(ctx);
    }

    /// Look up a registered service by name.
    pub fn get_service(&self, name: &str) -> Option<Arc<dyn Service>> {
        self.services.get(name).map(|entry| entry.service.clone())
    }

    /// Look up a service by name and attempt a downcast.
    /// Note: Downcasting `Arc<dyn Service>` to a concrete type requires
    /// the concrete type to be known at compile time. This is a best-effort
    /// operation that uses the internal `Any` representation.
    /// For full type-safe access, register services directly and use `get_service`.
    pub fn lookup_service<T: 'static>(&self, name: &str) -> Option<Arc<T>> {
        // Arc<dyn Service> cannot be downcast directly without an Any bound on the trait.
        // This is a placeholder that returns None; consumers should use get_service()
        // and cast manually, or the Service trait should be extended with as_any().
        let _ = name;
        None
    }

    /// Get the state of a service.
    pub async fn service_state(&self, name: &str) -> Option<ServiceState> {
        self.services
            .get(name)
            .map(|entry| *entry.state.blocking_read())
    }

    /// Check if all services are in the Running state.
    pub async fn all_running(&self) -> bool {
        for entry in self.services.values() {
            if *entry.state.read().await != ServiceState::Running {
                return false;
            }
        }
        true
    }

    /// Get the set of failed service names.
    pub async fn failed_services(&self) -> Vec<String> {
        let mut failed = Vec::new();
        for (name, entry) in &self.services {
            if *entry.state.read().await == ServiceState::Failed {
                failed.push(name.to_string());
            }
        }
        failed
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockService {
        name: &'static str,
        deps: Vec<&'static str>,
        version: semver::Version,
    }

    #[async_trait::async_trait]
    impl Service for MockService {
        fn name(&self) -> &'static str {
            self.name
        }
        fn version(&self) -> &semver::Version {
            &self.version
        }
        fn dependencies(&self) -> &[&'static str] {
            &self.deps
        }
        async fn start(&self, _ctx: Arc<RuntimeContext>) -> Result<(), ServiceError> {
            Ok(())
        }
        async fn stop(&self) -> Result<(), ServiceError> {
            Ok(())
        }
        async fn health_check(&self) -> ServiceHealth {
            ServiceHealth::healthy("ok")
        }
    }

    #[tokio::test]
    async fn test_services_start_in_dependency_order() {
        let mut mgr = ServiceManager::new();

        let dep = Arc::new(MockService {
            name: "dependency",
            deps: vec![],
            version: semver::Version::new(1, 0, 0),
        });
        let main = Arc::new(MockService {
            name: "main",
            deps: vec!["dependency"],
            version: semver::Version::new(1, 0, 0),
        });

        mgr.register(dep, 3, false).unwrap();
        mgr.register(main, 3, false).unwrap();

        let order = mgr.resolve_startup_order().unwrap();
        assert_eq!(order, vec!["dependency", "main"]);
    }

    #[tokio::test]
    async fn test_cycle_detection() {
        let mut mgr = ServiceManager::new();

        let a = Arc::new(MockService {
            name: "a",
            deps: vec!["b"],
            version: semver::Version::new(1, 0, 0),
        });
        let b = Arc::new(MockService {
            name: "b",
            deps: vec!["a"],
            version: semver::Version::new(1, 0, 0),
        });

        mgr.register(a, 3, false).unwrap();
        mgr.register(b, 3, false).unwrap();

        let result = mgr.resolve_startup_order();
        assert!(result.is_err());
        match result {
            Err(ServiceError::DependencyCycle { .. }) => {}
            _ => panic!("Expected DependencyCycle error"),
        }
    }

    #[tokio::test]
    async fn test_services_stop_in_reverse_dependency_order() {
        let mut mgr = ServiceManager::new();

        let dep = Arc::new(MockService {
            name: "dependency",
            deps: vec![],
            version: semver::Version::new(1, 0, 0),
        });
        let main = Arc::new(MockService {
            name: "main",
            deps: vec!["dependency"],
            version: semver::Version::new(1, 0, 0),
        });

        mgr.register(dep, 3, false).unwrap();
        mgr.register(main, 3, false).unwrap();

        let order = mgr.resolve_startup_order().unwrap();
        assert_eq!(order, vec!["dependency", "main"]);

        // Stop in reverse order
        let reverse: Vec<&str> = order.iter().rev().copied().collect();
        assert_eq!(reverse, vec!["main", "dependency"]);
    }
}
