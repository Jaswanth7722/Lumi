//! # Runtime Context
//!
//! Shared runtime context that every subsystem receives during startup.
//!
//! The context is constructed once during bootstrap and never mutated
//! structurally after construction. Configuration can be hot-reloaded
//! via `ArcSwap`, allowing lock-free reads.
//!
//! # Thread Safety
//!
//! `RuntimeContext` is `Send + Sync`. All fields are `Arc<T>` to allow
//! cheap cloning. Cloning the context is O(1).

use crate::config::ConfigLoader;
use crate::event::EventBus;
use crate::health::HealthMonitor;
use crate::metrics::MetricsRegistry;
use crate::resource::ResourceManager;
use crate::scheduler::Scheduler;
use crate::service::{Service, ServiceManager};
use crate::version::{FeatureFlags, RuntimeVersion};
use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use lumi_config::LumiConfig;
use std::sync::Arc;
use uuid::Uuid;

/// Registry for looking up running services by name.
///
/// Populated during service startup. Services can use this to
/// communicate with each other through their public interfaces.
#[derive(Clone, Default)]
pub struct ServiceRegistry {
    /// The underlying service manager.
    inner: Arc<ServiceManager>,
}

impl ServiceRegistry {
    /// Create a new empty service registry.
    pub fn new(manager: Arc<ServiceManager>) -> Self {
        Self { inner: manager }
    }

    /// Look up a service by name and downcast to a concrete type.
    pub fn get<T: Service>(&self, name: &str) -> Option<Arc<T>> {
        self.inner.lookup_service::<T>(name)
    }

    /// Check if a service is registered.
    pub fn has(&self, name: &str) -> bool {
        self.inner.get_service(name).is_some()
    }
}

/// Shared runtime context for all subsystems.
///
/// Constructed once during bootstrap and cloned to each service.
pub struct RuntimeContext {
    /// Runtime configuration (hot-reloadable via ArcSwap).
    pub config: Arc<ArcSwap<LumiConfig>>,
    /// Typed event bus.
    pub event_bus: Arc<EventBus>,
    /// Async task scheduler.
    pub scheduler: Arc<Scheduler>,
    /// Health monitor.
    pub health: Arc<HealthMonitor>,
    /// Resource manager.
    pub resources: Arc<ResourceManager>,
    /// Version metadata.
    pub version: RuntimeVersion,
    /// Unique instance ID for this runtime session.
    pub instance_id: Uuid,
    /// When the runtime started.
    pub started_at: DateTime<Utc>,
    /// Feature flags.
    pub feature_flags: Arc<FeatureFlags>,
    /// Configuration loader (for hot reload).
    pub config_loader: Arc<ConfigLoader>,
    /// Metrics registry.
    pub metrics: Arc<MetricsRegistry>,
    /// Service registry for inter-service communication.
    pub services: ServiceRegistry,
}

impl RuntimeContext {
    /// Create a new runtime context.
    ///
    /// This is typically called once during bootstrap.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        feature_flags: Arc<FeatureFlags>,
        event_bus: Arc<EventBus>,
        resources: Arc<ResourceManager>,
    ) -> Self {
        Self {
            config: Arc::new(ArcSwap::new(Arc::new(LumiConfig::default()))),
            event_bus,
            scheduler: Arc::new(Scheduler::new(128)),
            health: Arc::new(HealthMonitor::new()),
            resources,
            version: RuntimeVersion::current(),
            instance_id: Uuid::new_v4(),
            started_at: Utc::now(),
            feature_flags,
            config_loader: Arc::new(ConfigLoader::new()),
            metrics: Arc::new(MetricsRegistry::new()),
            services: ServiceRegistry::default(),
        }
    }

    /// Create a full runtime context with all subsystems initialized.
    #[allow(clippy::too_many_arguments)]
    pub fn with_all(
        config: Arc<LumiConfig>,
        event_bus: Arc<EventBus>,
        scheduler: Arc<Scheduler>,
        health: Arc<HealthMonitor>,
        resources: Arc<ResourceManager>,
        feature_flags: Arc<FeatureFlags>,
        config_loader: Arc<ConfigLoader>,
        metrics: Arc<MetricsRegistry>,
        services: ServiceRegistry,
    ) -> Self {
        Self {
            config: Arc::new(ArcSwap::new(config)),
            event_bus,
            scheduler,
            health,
            resources,
            version: RuntimeVersion::current(),
            instance_id: Uuid::new_v4(),
            started_at: Utc::now(),
            feature_flags,
            config_loader,
            metrics,
            services,
        }
    }

    /// Get the current configuration (lock-free via ArcSwap).
    pub fn current_config(&self) -> Arc<LumiConfig> {
        self.config.load_full()
    }

    /// Hot-reload the configuration.
    pub fn reload_config(&self, new_config: LumiConfig) {
        self.config.store(Arc::new(new_config));
    }

    /// Get a service by name and downcast to a concrete type.
    pub fn service<T: Service>(&self, name: &str) -> Option<Arc<T>> {
        self.services.get::<T>(name)
    }
}

impl std::fmt::Debug for RuntimeContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeContext")
            .field("instance_id", &self.instance_id)
            .field("version", &self.version.to_string())
            .field("started_at", &self.started_at)
            .field("services", &"...")
            .finish()
    }
}

// All fields are Arc<T> or plain values, so RuntimeContext is automatically Send + Sync.
// No unsafe impl needed.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let ctx = RuntimeContext::new(
            Arc::new(FeatureFlags::new()),
            Arc::new(EventBus::new(16)),
            Arc::new(ResourceManager::new()),
        );
        assert_eq!(ctx.instance_id.get_version(), Some(uuid::Version::Random)); // Uuid v4
        assert!(ctx.started_at.timestamp() > 0);
    }

    #[test]
    fn test_context_clone_is_cheap() {
        let ctx = RuntimeContext::new(
            Arc::new(FeatureFlags::new()),
            Arc::new(EventBus::new(16)),
            Arc::new(ResourceManager::new()),
        );
        // Cloning Arc is O(1)
        let _cloned = Arc::new(ctx);
    }

    #[test]
    fn test_current_config_returns_default() {
        let ctx = RuntimeContext::new(
            Arc::new(FeatureFlags::new()),
            Arc::new(EventBus::new(16)),
            Arc::new(ResourceManager::new()),
        );
        let config = ctx.current_config();
        assert_eq!(config.general.language, "en");
    }
}
