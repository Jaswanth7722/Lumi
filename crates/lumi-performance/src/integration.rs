//! # Subsystem Integration Layer
//!
//! Provides zero-boilerplate integration for every Lumi subsystem.
//! Each subsystem obtains a typed handle at initialization time.
//!
//! # Thread Safety
//! All handles are `Send + Sync`.

use crate::collector::SubsystemId;
use crate::config::PerformanceConfig;
use crate::manager::PerformanceManager;
use std::sync::Arc;

/// Register a subsystem with the performance monitoring system.
///
/// # Example
///
/// ```ignore
/// register_performance_subsystem!(manager, "render");
/// ```
#[macro_export]
macro_rules! register_performance_subsystem {
    ($manager:expr, $id:expr) => {{
        let id = $crate::collector::SubsystemId::new($id);
        $manager.subsystem_handle(id)
    }};
}

/// Create a performance metric with the standard naming schema.
///
/// # Example
///
/// ```ignore
/// let counter = make_metric!(counter, "render.frame_time", "Frame time", MetricUnit::Microseconds);
/// ```
#[macro_export]
macro_rules! make_metric {
    (counter, $name:expr, $desc:expr) => {
        $crate::counter::RtSafeCounter::new($crate::metric::MetricName::from_str($name), $desc)
    };
    (gauge, $name:expr, $desc:expr, $unit:expr) => {
        $crate::gauge::RtSafeGauge::new($crate::metric::MetricName::from_str($name), $desc, $unit)
    };
}

/// Initialize the performance system during bootstrap.
///
/// # Example
///
/// ```ignore
/// let perf_manager = init_performance!(config).await?;
/// ```
#[macro_export]
macro_rules! init_performance {
    ($config:expr) => {
        $crate::manager::PerformanceManager::start($config).await
    };
}

/// Get a render handle from the performance manager.
pub fn into_render_handle(manager: &PerformanceManager) -> crate::manager::SubsystemMetricsHandle {
    manager.subsystem_handle(SubsystemId::new("render"))
}

/// Get an AI handle from the performance manager.
pub fn into_ai_handle(manager: &PerformanceManager) -> crate::manager::SubsystemMetricsHandle {
    manager.subsystem_handle(SubsystemId::new("ai"))
}

/// Get an IPC handle from the performance manager.
pub fn into_ipc_handle(manager: &PerformanceManager) -> crate::manager::SubsystemMetricsHandle {
    manager.subsystem_handle(SubsystemId::new("ipc"))
}

/// Get a voice handle from the performance manager.
pub fn into_voice_handle(manager: &PerformanceManager) -> crate::manager::SubsystemMetricsHandle {
    manager.subsystem_handle(SubsystemId::new("voice"))
}

/// Get a storage handle from the performance manager.
pub fn into_storage_handle(manager: &PerformanceManager) -> crate::manager::SubsystemMetricsHandle {
    manager.subsystem_handle(SubsystemId::new("storage"))
}
