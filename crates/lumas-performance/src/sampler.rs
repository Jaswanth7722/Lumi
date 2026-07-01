//! # System Resource Sampler
//!
//! Platform-abstracted system resource sampling for CPU, memory, GPU, disk, and network.
//!
//! # Thread Safety
//! All sampler types are `Send + Sync`.

use crate::error::PerformanceResult;
use async_trait::async_trait;
use std::path::PathBuf;

/// CPU sample at a point in time.
#[derive(Debug, Clone)]
pub struct CpuSample {
    /// Overall CPU utilization percentage.
    pub overall_percent: f32,
    /// Per-core utilization percentages.
    pub per_core: Vec<f32>,
    /// Lumas process CPU percentage.
    pub lumas_process_percent: f32,
    /// Load average over 1 minute.
    pub load_avg_1m: f32,
}

/// Memory sample at a point in time.
#[derive(Debug, Clone)]
pub struct MemorySample {
    /// System total physical memory in bytes.
    pub system_total_bytes: u64,
    /// System available memory in bytes.
    pub system_available_bytes: u64,
    /// Lumas process RSS in bytes.
    pub lumi_rss_bytes: u64,
    /// Lumas process virtual memory in bytes.
    pub lumi_virtual_bytes: u64,
    /// Lumas process heap bytes (if available).
    pub lumi_heap_bytes: Option<u64>,
}

/// GPU sample at a point in time.
#[derive(Debug, Clone)]
pub struct GpuSample {
    /// GPU utilization percentage.
    pub utilization_percent: f32,
    /// VRAM used in bytes.
    pub vram_used_bytes: u64,
    /// Total VRAM in bytes.
    pub vram_total_bytes: u64,
    /// GPU temperature in Celsius.
    pub temperature_celsius: Option<f32>,
    /// Lumas process VRAM usage.
    pub lumi_vram_bytes: Option<u64>,
}

/// Disk sample at a point in time.
#[derive(Debug, Clone)]
pub struct DiskSample {
    /// Total disk space in bytes.
    pub total_bytes: u64,
    /// Available disk space in bytes.
    pub available_bytes: u64,
    /// Disk read bytes/second.
    pub read_bytes_per_sec: u64,
    /// Disk write bytes/second.
    pub write_bytes_per_sec: u64,
}

/// Network sample at a point in time.
#[derive(Debug, Clone)]
pub struct NetworkSample {
    /// Bytes received per second.
    pub rx_bytes_per_sec: u64,
    /// Bytes sent per second.
    pub tx_bytes_per_sec: u64,
}

/// Process sample (lumi-specific).
#[derive(Debug, Clone)]
pub struct ProcessSample {
    /// Number of threads.
    pub thread_count: u32,
    /// Number of open file descriptors.
    pub open_fds: u32,
    /// Process uptime in seconds.
    pub uptime_secs: u64,
}

/// System resource sampler trait.
#[async_trait]
pub trait SystemSampler: Send + Sync {
    /// Sample CPU metrics.
    async fn sample_cpu(&self) -> PerformanceResult<CpuSample>;
    /// Sample memory metrics.
    async fn sample_memory(&self) -> PerformanceResult<MemorySample>;
    /// Sample GPU metrics (returns None if unavailable).
    async fn sample_gpu(&self) -> PerformanceResult<Option<GpuSample>>;
    /// Sample disk metrics.
    async fn sample_disk(&self) -> PerformanceResult<DiskSample>;
    /// Sample network metrics.
    async fn sample_network(&self) -> PerformanceResult<NetworkSample>;
    /// Sample process metrics.
    async fn sample_process(&self) -> PerformanceResult<ProcessSample>;
}

/// Basic fallback sampler that provides minimal system info.
/// Does not require any platform-specific dependencies.
#[derive(Debug, Clone)]
pub struct BasicSampler;

#[async_trait]
impl SystemSampler for BasicSampler {
    async fn sample_cpu(&self) -> PerformanceResult<CpuSample> {
        Ok(CpuSample {
            overall_percent: 0.0,
            per_core: vec![],
            lumas_process_percent: 0.0,
            load_avg_1m: 0.0,
        })
    }

    async fn sample_memory(&self) -> PerformanceResult<MemorySample> {
        Ok(MemorySample {
            system_total_bytes: 0,
            system_available_bytes: 0,
            lumi_rss_bytes: 0,
            lumi_virtual_bytes: 0,
            lumi_heap_bytes: None,
        })
    }

    async fn sample_gpu(&self) -> PerformanceResult<Option<GpuSample>> {
        Ok(None)
    }

    async fn sample_disk(&self) -> PerformanceResult<DiskSample> {
        Ok(DiskSample {
            total_bytes: 0,
            available_bytes: 0,
            read_bytes_per_sec: 0,
            write_bytes_per_sec: 0,
        })
    }

    async fn sample_network(&self) -> PerformanceResult<NetworkSample> {
        Ok(NetworkSample {
            rx_bytes_per_sec: 0,
            tx_bytes_per_sec: 0,
        })
    }

    async fn sample_process(&self) -> PerformanceResult<ProcessSample> {
        Ok(ProcessSample {
            thread_count: 1,
            open_fds: 0,
            uptime_secs: 0,
        })
    }
}

/// Fake system sampler for testing. Returns configurable values.
#[derive(Debug, Clone)]
pub struct FakeSystemSampler {
    cpu: f32,
    memory: u64,
}

impl FakeSystemSampler {
    /// Create a new fake sampler with fixed values.
    pub fn new(cpu_percent: f32, memory_bytes: u64) -> Self {
        Self {
            cpu: cpu_percent,
            memory: memory_bytes,
        }
    }
}

#[async_trait]
impl SystemSampler for FakeSystemSampler {
    async fn sample_cpu(&self) -> PerformanceResult<CpuSample> {
        Ok(CpuSample {
            overall_percent: self.cpu,
            per_core: vec![self.cpu],
            lumas_process_percent: self.cpu,
            load_avg_1m: 0.0,
        })
    }

    async fn sample_memory(&self) -> PerformanceResult<MemorySample> {
        Ok(MemorySample {
            system_total_bytes: 16_000_000_000,
            system_available_bytes: 8_000_000_000,
            lumi_rss_bytes: self.memory,
            lumi_virtual_bytes: self.memory,
            lumi_heap_bytes: None,
        })
    }

    async fn sample_gpu(&self) -> PerformanceResult<Option<GpuSample>> {
        Ok(Some(GpuSample {
            utilization_percent: 30.0,
            vram_used_bytes: 2_000_000_000,
            vram_total_bytes: 8_000_000_000,
            temperature_celsius: Some(65.0),
            lumi_vram_bytes: Some(1_000_000_000),
        }))
    }

    async fn sample_disk(&self) -> PerformanceResult<DiskSample> {
        Ok(DiskSample {
            total_bytes: 500_000_000_000,
            available_bytes: 200_000_000_000,
            read_bytes_per_sec: 0,
            write_bytes_per_sec: 0,
        })
    }

    async fn sample_network(&self) -> PerformanceResult<NetworkSample> {
        Ok(NetworkSample {
            rx_bytes_per_sec: 1000,
            tx_bytes_per_sec: 500,
        })
    }

    async fn sample_process(&self) -> PerformanceResult<ProcessSample> {
        Ok(ProcessSample {
            thread_count: 32,
            open_fds: 64,
            uptime_secs: 3600,
        })
    }
}
