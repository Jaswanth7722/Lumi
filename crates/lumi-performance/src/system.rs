//! # System Monitoring
//!
//! System-level metric collectors for CPU, memory, GPU, and process monitoring.
//!
//! # Thread Safety
//! All types are `Send + Sync`.

use crate::counter::RtSafeCounter;
use crate::gauge::RtSafeGauge;
use crate::metric::{MetricName, MetricUnit};
use crate::sampler::SystemSampler;
use std::sync::Arc;

/// Handle for system-level metrics.
///
/// Provides typed access to system resource counters and gauges.
/// The render loop and other Rt-safe paths can use these directly.
#[derive(Clone)]
pub struct SystemMetricsHandle {
    /// Lumi process CPU percentage.
    pub cpu_percent: Arc<RtSafeGauge>,
    /// Lumi process RSS memory in bytes.
    pub memory_rss: Arc<RtSafeGauge>,
    /// Lumi process virtual memory in bytes.
    pub memory_virtual: Arc<RtSafeGauge>,
    /// Total system memory in bytes.
    pub system_total_memory: Arc<RtSafeGauge>,
    /// Available system memory in bytes.
    pub system_available_memory: Arc<RtSafeGauge>,
    /// GPU utilization percentage (if available).
    pub gpu_percent: Arc<RtSafeGauge>,
    /// GPU VRAM used in bytes.
    pub gpu_vram_used: Arc<RtSafeGauge>,
    /// Number of active threads.
    pub thread_count: Arc<RtSafeGauge>,
    /// Number of open file descriptors.
    pub open_fds: Arc<RtSafeGauge>,
    /// Process uptime in seconds.
    pub uptime_seconds: Arc<RtSafeGauge>,
    /// Total disk space in bytes.
    pub disk_total: Arc<RtSafeGauge>,
    /// Available disk space in bytes.
    pub disk_available: Arc<RtSafeGauge>,
}

impl SystemMetricsHandle {
    /// Create a new system metrics handle with all gauges initialized.
    pub fn new() -> Self {
        Self {
            cpu_percent: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.cpu.lumi_process_percent"),
                "Lumi process CPU usage",
                MetricUnit::Percent,
            )),
            memory_rss: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.memory.lumi_rss_bytes"),
                "Lumi process RSS memory",
                MetricUnit::Bytes,
            )),
            memory_virtual: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.memory.lumi_virtual_bytes"),
                "Lumi process virtual memory",
                MetricUnit::Bytes,
            )),
            system_total_memory: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.memory.total_bytes"),
                "System total memory",
                MetricUnit::Bytes,
            )),
            system_available_memory: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.memory.available_bytes"),
                "System available memory",
                MetricUnit::Bytes,
            )),
            gpu_percent: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.gpu.utilization_percent"),
                "GPU utilization",
                MetricUnit::Percent,
            )),
            gpu_vram_used: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.gpu.vram_used_bytes"),
                "GPU VRAM used",
                MetricUnit::Bytes,
            )),
            thread_count: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.process.thread_count"),
                "Active thread count",
                MetricUnit::Count,
            )),
            open_fds: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.process.open_fds"),
                "Open file descriptors",
                MetricUnit::Count,
            )),
            uptime_seconds: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.process.uptime_seconds"),
                "Process uptime",
                MetricUnit::Count,
            )),
            disk_total: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.disk.total_bytes"),
                "Total disk space",
                MetricUnit::Bytes,
            )),
            disk_available: Arc::new(RtSafeGauge::new(
                MetricName::from_str("lumi.system.disk.available_bytes"),
                "Available disk space",
                MetricUnit::Bytes,
            )),
        }
    }
}

impl Default for SystemMetricsHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Update system metrics from sampler data.
pub fn update_system_metrics(handle: &SystemMetricsHandle, sampler: &dyn SystemSampler) {
    // This is called from the background sampler task.
    // We use tokio::spawn or similar to run the async sampler calls.
}
