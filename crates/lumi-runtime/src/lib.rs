//! # Lumi Runtime
//!
//! Production-grade core runtime for the Lumi desktop AI platform.
//!
//! This crate provides the runtime orchestration layer that manages
//! subsystem lifecycle, configuration, event dispatch, task scheduling,
//! health monitoring, resource tracking, and graceful shutdown.
//!
//! ## Architecture
//!
//! The runtime is organized around a bootstrap sequence that initializes
//! subsystems in dependency order, a lifecycle state machine that tracks
//! the runtime's operational state, and a service framework for managing
//! pluggable subsystems.
//!
//! ## Quick Start
//!
//! ```no_run
//! use lumi_runtime::bootstrap::Bootstrap;
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut boot = Bootstrap::new();
//!     let handle = boot.bootstrap().await.unwrap();
//!     // Runtime is now running...
//!     handle.shutdown().await;
//! }
//! ```
//!
//! ## Module Organization
//!
//! | Module | Description |
//! |--------|-------------|
//! | `bootstrap` | Boot sequence orchestrator |
//! | `config` | Configuration loading, validation, hot reload |
//! | `context` | Shared runtime context |
//! | `error` | Unified error hierarchy |
//! | `event` | Typed event bus |
//! | `health` | Health monitor |
//! | `lifecycle` | Runtime lifecycle state machine |
//! | `metrics` | Metrics registry |
//! | `resource` | Resource manager |
//! | `scheduler` | Async task scheduler |
//! | `service` | Service trait + service manager |
//! | `shutdown` | Graceful shutdown orchestrator |
//! | `version` | Version management + feature flags |

// Public modules
pub mod bootstrap;
pub mod config;
pub mod context;
pub mod error;
pub mod event;
pub mod health;
pub mod lifecycle;
pub mod metrics;
pub mod resource;
pub mod scheduler;
pub mod service;
pub mod shutdown;
pub mod version;

// Re-exports for convenience
pub use bootstrap::{Bootstrap, RuntimeHandle};
pub use config::ConfigLoader;
pub use context::RuntimeContext;
pub use error::RuntimeError;
pub use event::EventBus;
pub use health::HealthMonitor;
pub use lifecycle::LifecycleManager;
pub use metrics::MetricsRegistry;
pub use resource::ResourceManager;
pub use scheduler::Scheduler;
pub use service::{HealthStatus, Service, ServiceHealth, ServiceManager};
pub use shutdown::ShutdownManager;
pub use version::{BuildProfile, FeatureFlags, RuntimeVersion};

/// The current runtime version.
pub static VERSION: once_cell::sync::Lazy<RuntimeVersion> =
    once_cell::sync::Lazy::new(RuntimeVersion::current);

/// Re-export key dependencies for convenience.
pub mod exports {
    pub use arc_swap;
    pub use chrono;
    pub use dashmap;
    pub use lumi_config;
    pub use parking_lot;
    pub use semver;
    pub use tokio;
    pub use tokio_util;
    pub use tracing;
    pub use uuid;
}
