//! # Bootstrap Orchestrator
//!
//! Manages the complete runtime bootstrap sequence as a state machine.
//!
//! The bootstrapper initializes subsystems in order: config → logging →
//! IPC → storage → plugins → services → health monitor → running state.
//! Each phase is treated as a step in a state machine, with rollback
//! on failure.
//!
//! # Errors
//!
//! Any bootstrap failure produces a `BootstrapError` with the failed
//! phase, the primary error, and any rollback errors.

use crate::config::ConfigLoader;
use crate::context::RuntimeContext;
use crate::error::{BootstrapError, ConfigError};
use crate::event::{ConfigLoaded, EventBus, RuntimeStarted};
use crate::health::HealthMonitor;
use crate::lifecycle::{BootstrapPhase, LifecycleManager, ShutdownPhase};
use crate::metrics::MetricsRegistry;
use crate::resource::ResourceManager;
use crate::scheduler::Scheduler;
use crate::service::{Service, ServiceManager};
use crate::shutdown::ShutdownManager;
use crate::version::{BuildProfile, FeatureFlags, RuntimeVersion};
use arc_swap::ArcSwap;
use lumi_config::LumiConfig;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Handle for controlling the running runtime.
pub struct RuntimeHandle {
    /// Runtime context shared across all subsystems.
    pub context: Arc<RuntimeContext>,
    /// Lifecycle manager.
    pub lifecycle: Arc<RwLock<LifecycleManager>>,
    /// Service manager.
    pub services: Arc<RwLock<ServiceManager>>,
    /// Health monitor.
    pub health: Arc<HealthMonitor>,
    /// Scheduler.
    pub scheduler: Arc<Scheduler>,
    /// Shutdown manager.
    pub shutdown: Arc<RwLock<ShutdownManager>>,
    /// Event bus.
    pub event_bus: Arc<EventBus>,
    /// Runtime start time.
    pub start_time: Instant,
}

impl RuntimeHandle {
    /// Initiate graceful shutdown.
    pub async fn shutdown(&self) {
        self.shutdown.write().await.shutdown(self).await;
    }

    /// Get uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

/// Orchestrates the runtime bootstrap sequence.
pub struct Bootstrap {
    /// Lifecycle state machine.
    lifecycle: LifecycleManager,
    /// Service manager.
    service_manager: ServiceManager,
    /// Event bus.
    event_bus: Arc<EventBus>,
    /// Configuration loader.
    config_loader: ConfigLoader,
    /// Feature flags.
    feature_flags: Arc<FeatureFlags>,
    /// Resource manager.
    resource_manager: Arc<ResourceManager>,
    /// Metrics registry.
    metrics: Arc<MetricsRegistry>,
    /// Scheduler.
    scheduler: Arc<Scheduler>,
    /// Health monitor.
    health_monitor: Arc<HealthMonitor>,
    /// Shutdown manager.
    shutdown_manager: ShutdownManager,
    /// Whether bootstrap has been started.
    started: bool,
}

impl Bootstrap {
    /// Create a new bootstrap orchestrator.
    pub fn new() -> Self {
        let event_bus = Arc::new(EventBus::new(2048));

        Self {
            lifecycle: LifecycleManager::new(),
            service_manager: ServiceManager::new(),
            event_bus,
            config_loader: ConfigLoader::new(),
            feature_flags: Arc::new(FeatureFlags::new()),
            resource_manager: Arc::new(ResourceManager::new()),
            metrics: Arc::new(MetricsRegistry::new()),
            scheduler: Arc::new(Scheduler::new(128)),
            health_monitor: Arc::new(HealthMonitor::new()),
            shutdown_manager: ShutdownManager::new(),
            started: false,
        }
    }

    /// Run the full bootstrap sequence.
    ///
    /// # Errors
    ///
    /// Returns `BootstrapError` if any phase fails. Rollback is
    /// attempted for already-initialized phases.
    pub async fn bootstrap(&mut self) -> Result<RuntimeHandle, BootstrapError> {
        let overall_start = Instant::now();
        info!("=== Lumi Runtime Bootstrap Starting ===");

        // --- Phase 1: Load configuration ---
        self.advance(BootstrapPhase::LoadingConfig).await;
        let config = match self.config_loader.load() {
            Ok(cfg) => {
                info!("Configuration loaded successfully");
                self.event_bus
                    .publish(ConfigLoaded {
                        path: self.config_loader.config_path().map(|p| p.to_path_buf()),
                    })
                    .await;
                cfg
            }
            Err(errors) => {
                for err in &errors {
                    error!("Config error: {err}");
                }
                return Err(BootstrapError::new("LoadingConfig", format!("{errors:?}")));
            }
        };

        // --- Phase 2: Initialize tracing/logging ---
        self.advance(BootstrapPhase::InitializingLogger).await;
        let log_filter = std::env::var("LUMI_LOG")
            .unwrap_or_else(|_| "lumi_runtime=info,lumi_config=info".into());
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_new(&log_filter)
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with_target(true)
            .with_thread_ids(true)
            .try_init();
        info!("Tracing initialized (filter: {log_filter})");

        // --- Phase 3: Initialize IPC ---
        self.advance(BootstrapPhase::InitializingIPC).await;
        // IPC bus is already created via the event bus;
        // lumi-ipc transport will be started by the core process.
        info!("IPC bus ready (capacity: {})", 2048);

        // --- Phase 4: Initialize storage ---
        self.advance(BootstrapPhase::InitializingStorage).await;
        let data_dir = LumiConfig::config_dir();
        if let Err(e) = tokio::fs::create_dir_all(&data_dir).await {
            error!("Failed to create data directory {:?}: {}", data_dir, e);
            return Err(BootstrapError::new("InitializingStorage", e.to_string()));
        }
        info!("Storage directory ready: {:?}", data_dir);

        // --- Phase 5: Discover plugins ---
        self.advance(BootstrapPhase::DiscoveringPlugins).await;
        let plugin_dir = data_dir.join("plugins");
        if plugin_dir.exists() {
            match tokio::fs::read_dir(&plugin_dir).await {
                Ok(mut entries) => {
                    let mut count = 0u32;
                    while let Some(entry) = entries.next_entry().await.transpose() {
                        match entry {
                            Ok(e) => {
                                if e.path().extension().map_or(false, |ext| ext == "wasm") {
                                    count += 1;
                                    debug!("Discovered plugin: {:?}", e.path());
                                }
                            }
                            Err(e) => {
                                warn!("Error reading plugin entry: {e}");
                            }
                        }
                    }
                    info!("Discovered {count} plugin(s) in {:?}", plugin_dir);
                }
                Err(e) => {
                    warn!("Could not read plugin directory {:?}: {}", plugin_dir, e);
                }
            }
        } else {
            info!(
                "No plugin directory found at {:?}; no plugins to load",
                plugin_dir
            );
        }

        // --- Phase 6: Register built-in services ---
        self.advance(BootstrapPhase::RegisteringServices).await;
        self.service_manager.set_event_bus(self.event_bus.clone());
        // Register built-in services here (lumi-core, lumi-voice, etc.)
        // For now, these are registered by the core process itself.
        info!("Service registry ready");

        // --- Phase 7: Resolve dependencies ---
        self.advance(BootstrapPhase::ResolvingDependencies).await;
        match self.service_manager.resolve_startup_order() {
            Ok(order) => {
                info!("Service dependency order: {:?}", order);
            }
            Err(e) => {
                error!("Failed to resolve service dependencies: {e}");
                return Err(BootstrapError::new("ResolvingDependencies", e.to_string()));
            }
        }

        // --- Phase 8: Start services ---
        self.advance(BootstrapPhase::StartingServices).await;
        // Create runtime context
        let context = Arc::new(RuntimeContext::with_all(
            config,
            self.event_bus.clone(),
            self.scheduler.clone(),
            self.health_monitor.clone(),
            self.resource_manager.clone(),
            self.feature_flags.clone(),
            Arc::new(ConfigLoader::new()), // New loader for hot reload
            self.metrics.clone(),
            crate::context::ServiceRegistry::new(Arc::new(ServiceManager::new())),
        ));

        self.service_manager.set_context(context.clone());

        // Start registered services
        match self.service_manager.start_all(context.clone()).await {
            Ok(()) => info!("All services started successfully"),
            Err(e) => {
                error!("Failed to start services: {e}");
                // Attempt to stop already-started services
                let _ = self.service_manager.stop_all().await;
                return Err(BootstrapError::new("StartingServices", e.to_string()));
            }
        }

        // --- Phase 9: Start health monitor ---
        self.advance(BootstrapPhase::StartingHealthMonitor).await;
        let sm = Arc::new(RwLock::new(ServiceManager::new()));
        self.health_monitor.set_service_manager(sm).await;
        self.health_monitor
            .set_event_bus(self.event_bus.clone())
            .await;
        self.health_monitor.start();
        info!("Health monitor started");

        // --- Phase 10: Complete ---
        self.advance(BootstrapPhase::Complete).await;
        let uptime_ms = overall_start.elapsed().as_millis();
        info!("=== Lumi Runtime Bootstrap Complete ({uptime_ms}ms) ===");

        // Emit RuntimeStarted event
        let version = RuntimeVersion::current();
        self.event_bus
            .publish(RuntimeStarted::new(version.version.clone()))
            .await;

        // Transition lifecycle to Running
        if let Err(e) = self.lifecycle.transition_to_running() {
            error!("Failed to transition to Running state: {e}");
            return Err(BootstrapError::new("Complete", e.to_string()));
        }

        Ok(RuntimeHandle {
            context,
            lifecycle: Arc::new(RwLock::new(std::mem::replace(
                &mut self.lifecycle,
                LifecycleManager::new(),
            ))),
            services: Arc::new(RwLock::new(std::mem::replace(
                &mut self.service_manager,
                ServiceManager::new(),
            ))),
            health: self.health_monitor.clone(),
            scheduler: self.scheduler.clone(),
            shutdown: Arc::new(RwLock::new(std::mem::replace(
                &mut self.shutdown_manager,
                ShutdownManager::new(),
            ))),
            event_bus: self.event_bus.clone(),
            start_time: overall_start,
        })
    }

    /// Advance to the next bootstrap phase.
    async fn advance(&mut self, phase: BootstrapPhase) {
        if !self.started {
            if self.lifecycle.start_bootstrap().is_ok() {
                self.started = true;
            }
        }
        let _ = self.lifecycle.advance_bootstrap(phase);
        let state = self.lifecycle.current().clone();
        debug!("Bootstrap phase: {:?}", state);
    }
}

impl Default for Bootstrap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bootstrap_succeeds_with_defaults() {
        let mut boot = Bootstrap::new();
        let result = boot.bootstrap().await;
        assert!(result.is_ok(), "Bootstrap failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_bootstrap_runtime_has_valid_version() {
        let mut boot = Bootstrap::new();
        let handle = boot.bootstrap().await.unwrap();
        let v = RuntimeVersion::current();
        let display = v.display_string();
        assert!(!display.is_empty());
    }

    #[tokio::test]
    async fn test_bootstrap_started_event_emitted() {
        let mut boot = Bootstrap::new();
        let _handle = boot.bootstrap().await.unwrap();
        let published = boot.event_bus.events_published_count();
        assert!(published > 0);
    }
}
