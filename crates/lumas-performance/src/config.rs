//! # Performance System Configuration
//!
//! Configuration for the performance monitoring system.
//! Mirrors the `[performance]` section of the Lumas config.

use std::path::PathBuf;
use std::time::Duration;

/// Top-level performance system configuration.
#[derive(Debug, Clone)]
pub struct PerformanceConfig {
    /// Whether the performance system is enabled.
    pub enabled: bool,
    /// Collection mode.
    pub collection_mode: CollectionMode,
    /// System sampler configuration.
    pub system_sampler: SystemSamplerConfig,
    /// Histogram configuration.
    pub histogram: HistogramConfig,
    /// Threshold configuration.
    pub thresholds: ThresholdConfig,
    /// Export configuration.
    pub export: ExportConfig,
    /// Profiler configuration (only used when feature = "profiler").
    pub profiler: ProfilerConfig,
    /// Anomaly detection configuration.
    pub anomaly_detection: AnomalyDetectionConfig,
    /// Dashboard configuration.
    pub dashboard: DashboardConfig,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_mode: CollectionMode::Production,
            system_sampler: SystemSamplerConfig::default(),
            histogram: HistogramConfig::default(),
            thresholds: ThresholdConfig::default(),
            export: ExportConfig::default(),
            profiler: ProfilerConfig::default(),
            anomaly_detection: AnomalyDetectionConfig::default(),
            dashboard: DashboardConfig::default(),
        }
    }
}

/// Collection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionMode {
    /// Full collection (default for production).
    Production,
    /// Extended collection with debugging data.
    Development,
    /// Minimal collection (lowest overhead).
    Minimal,
}

impl Default for CollectionMode {
    fn default() -> Self {
        Self::Production
    }
}

/// System sampler configuration.
#[derive(Debug, Clone)]
pub struct SystemSamplerConfig {
    /// Sampling interval in milliseconds.
    pub interval_ms: u64,
    /// Whether GPU metrics are enabled.
    pub gpu_enabled: bool,
}

impl Default for SystemSamplerConfig {
    fn default() -> Self {
        Self {
            interval_ms: 1000,
            gpu_enabled: false,
        }
    }
}

/// Histogram configuration.
#[derive(Debug, Clone)]
pub struct HistogramConfig {
    /// Interval between histogram merge operations in milliseconds.
    pub merge_interval_ms: u64,
    /// Number of thread-local buffer slots for RtSafeHistogram.
    pub thread_local_buffer_slots: usize,
}

impl Default for HistogramConfig {
    fn default() -> Self {
        Self {
            merge_interval_ms: 100,
            thread_local_buffer_slots: 1024,
        }
    }
}

/// Threshold configuration.
#[derive(Debug, Clone)]
pub struct ThresholdConfig {
    /// Whether threshold evaluation is enabled.
    pub enabled: bool,
    /// Interval between threshold evaluations in milliseconds.
    pub evaluation_interval_ms: u64,
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            evaluation_interval_ms: 500,
        }
    }
}

/// Export configuration.
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Interval between exports in seconds.
    pub interval_secs: u64,
    /// Export formats to enable.
    pub formats: Vec<String>,
    /// Output directory for file-based exports.
    pub output_dir: PathBuf,
    /// Maximum number of export files to retain.
    pub max_export_files: usize,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            interval_secs: 30,
            formats: vec!["json".into()],
            output_dir: PathBuf::from("metrics"),
            max_export_files: 48,
        }
    }
}

/// Profiler configuration.
#[derive(Debug, Clone)]
pub struct ProfilerConfig {
    /// Output directory for profiles.
    pub output_dir: PathBuf,
    /// Sampling rate in Hz.
    pub sampling_rate_hz: u32,
}

impl Default for ProfilerConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("profiles"),
            sampling_rate_hz: 99,
        }
    }
}

/// Anomaly detection configuration.
#[derive(Debug, Clone)]
pub struct AnomalyDetectionConfig {
    /// Whether anomaly detection is enabled.
    pub enabled: bool,
    /// Number of days for the baseline window.
    pub baseline_window_days: u32,
    /// Deviation threshold multiplier (observed > baseline_p99 × this).
    pub deviation_threshold: f64,
}

impl Default for AnomalyDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            baseline_window_days: 7,
            deviation_threshold: 2.0,
        }
    }
}

/// Dashboard configuration.
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// Whether the dashboard API is enabled.
    pub enabled: bool,
    /// Minimum subscription interval in milliseconds.
    pub min_subscription_interval_ms: u64,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_subscription_interval_ms: 100,
        }
    }
}

impl PerformanceConfig {
    /// Validate the configuration, returning an error if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.system_sampler.interval_ms == 0 {
            return Err("system_sampler.interval_ms must be > 0".into());
        }
        if self.histogram.merge_interval_ms == 0 {
            return Err("histogram.merge_interval_ms must be > 0".into());
        }
        if self.thresholds.evaluation_interval_ms == 0 {
            return Err("thresholds.evaluation_interval_ms must be > 0".into());
        }
        if self.export.interval_secs == 0 {
            return Err("export.interval_secs must be > 0".into());
        }
        if self.dashboard.min_subscription_interval_ms < 10 {
            return Err("dashboard.min_subscription_interval_ms must be >= 10".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PerformanceConfig::default();
        assert!(config.enabled);
        assert_eq!(config.collection_mode, CollectionMode::Production);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation() {
        let mut config = PerformanceConfig::default();
        config.system_sampler.interval_ms = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_collection_mode_debug() {
        assert_eq!(format!("{:?}", CollectionMode::Development), "Development");
    }
}
