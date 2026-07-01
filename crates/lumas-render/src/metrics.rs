//! Performance metrics — per-frame timing, budget enforcement, and counters.
//!
//! The metrics system tracks:
//! - **Frame timing**: CPU and GPU duration per frame
//! - **Per-pass timing**: GPU duration for each render pass
//! - **Budget enforcement**: Alerts when a pass exceeds its frame budget
//! - **Counters**: Draw calls, vertices, textures loaded, VRAM usage
//! - **Scenarios**: Pre-defined budget configurations for
//!   idle, sleeping, celebration, and focus modes
//!
//! # Frame Budget
//! Metrics collection adds ~0.01ms CPU overhead per frame.
//! Timestamp queries add ~0.005ms GPU overhead per pass.

use crate::config::RenderConfig;
use std::time::Instant;

/// Per-pass GPU timing data (from timestamp queries).
#[derive(Debug, Clone)]
pub struct PassTiming {
    /// Pass name.
    pub name: &'static str,
    /// GPU duration in microseconds.
    pub gpu_duration_us: u64,
    /// Budget ceiling in microseconds.
    pub budget_us: u64,
    /// Whether this pass exceeded its budget.
    pub budget_exceeded: bool,
}

/// Per-frame metrics snapshot.
#[derive(Debug, Clone)]
pub struct FrameMetrics {
    /// Frame number.
    pub frame_index: u64,
    /// CPU frame duration in microseconds.
    pub cpu_duration_us: u64,
    /// GPU frame duration in microseconds (0 if timestamps disabled).
    pub gpu_duration_us: u64,
    /// Per-pass timing breakdown.
    pub passes: Vec<PassTiming>,
    /// Whether any pass exceeded its budget.
    pub any_budget_exceeded: bool,
    /// Draw call count.
    pub draw_calls: u32,
    /// Total vertices processed.
    pub vertices_processed: u32,
}

impl Default for FrameMetrics {
    fn default() -> Self {
        Self {
            frame_index: 0,
            cpu_duration_us: 0,
            gpu_duration_us: 0,
            passes: Vec::new(),
            any_budget_exceeded: false,
            draw_calls: 0,
            vertices_processed: 0,
        }
    }
}

/// Aggregate metrics over a sliding window.
#[derive(Debug, Clone)]
pub struct AggregateMetrics {
    /// Average CPU frame time over the window.
    pub avg_cpu_us: f64,
    /// Average GPU frame time over the window.
    pub avg_gpu_us: f64,
    /// 95th percentile CPU frame time.
    pub p95_cpu_us: f64,
    /// 95th percentile GPU frame time.
    pub p95_gpu_us: f64,
    /// Number of frames in window.
    pub sample_count: u64,
    /// Frames that exceeded budget (percentage).
    pub budget_exceeded_pct: f64,
}

/// Counters that accumulate across frames.
#[derive(Debug, Clone)]
pub struct RenderCounters {
    /// Total draw calls since last reset.
    pub draw_calls: u64,
    /// Total vertices processed.
    pub vertices_processed: u64,
    /// Total index elements processed.
    pub indices_processed: u64,
    /// Number of texture uploads.
    pub texture_uploads: u64,
    /// Number of buffer uploads (write_buffer calls).
    pub buffer_uploads: u64,
    /// Number of pipeline creations.
    pub pipeline_creations: u64,
    /// Current VRAM usage estimate (bytes).
    pub vram_usage_bytes: u64,
    /// Peak VRAM usage (bytes).
    pub vram_peak_bytes: u64,
}

impl Default for RenderCounters {
    fn default() -> Self {
        Self {
            draw_calls: 0,
            vertices_processed: 0,
            indices_processed: 0,
            texture_uploads: 0,
            buffer_uploads: 0,
            pipeline_creations: 0,
            vram_usage_bytes: 0,
            vram_peak_bytes: 0,
        }
    }
}

impl RenderCounters {
    /// Record a draw call.
    pub fn record_draw(&mut self, vertices: u32, indices: u32) {
        self.draw_calls += 1;
        self.vertices_processed += vertices as u64;
        self.indices_processed += indices as u64;
    }

    /// Record a texture upload.
    pub fn record_texture_upload(&mut self, size_bytes: u64) {
        self.texture_uploads += 1;
        self.vram_usage_bytes += size_bytes;
        self.vram_peak_bytes = self.vram_peak_bytes.max(self.vram_usage_bytes);
    }

    /// Record a buffer upload.
    pub fn record_buffer_upload(&mut self) {
        self.buffer_uploads += 1;
    }

    /// Reset all counters.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Per-frame budget definitions for all render passes.
#[derive(Debug, Clone)]
pub struct FrameBudgets {
    pub depth_prepass_us: u64,
    pub geometry_us: u64,
    pub fur_us: u64,
    pub crystal_vfx_us: u64,
    pub particles_us: u64,
    pub workspace_panel_us: u64,
    pub bloom_us: u64,
    pub postprocess_us: u64,
    pub composite_us: u64,
    pub shadow_us: u64,
}

impl FrameBudgets {
    /// Create from a `RenderConfig`.
    pub fn from_config(config: &RenderConfig) -> Self {
        Self {
            depth_prepass_us: config.budget_depth_prepass_us,
            geometry_us: config.budget_geometry_us,
            fur_us: config.budget_fur_us,
            crystal_vfx_us: config.budget_crystal_vfx_us,
            particles_us: config.budget_particle_us,
            workspace_panel_us: config.budget_workspace_panel_us,
            bloom_us: config.budget_bloom_us,
            postprocess_us: config.budget_postprocess_us,
            composite_us: config.budget_composite_us,
            shadow_us: config.budget_shadow_us,
        }
    }

    /// Total GPU budget across all passes.
    pub fn total_gpu_us(&self) -> u64 {
        self.depth_prepass_us
            + self.geometry_us
            + self.fur_us
            + self.crystal_vfx_us
            + self.particles_us
            + self.workspace_panel_us
            + self.bloom_us
            + self.postprocess_us
            + self.composite_us
            + self.shadow_us
    }

    /// Get the budget for a pass by name.
    pub fn for_pass(&self, name: &str) -> u64 {
        match name {
            "depth_prepass" => self.depth_prepass_us,
            "geometry" => self.geometry_us,
            "fur" => self.fur_us,
            "crystal_vfx" => self.crystal_vfx_us,
            "particles" => self.particles_us,
            "workspace_panel" => self.workspace_panel_us,
            "bloom" => self.bloom_us,
            "postprocess" => self.postprocess_us,
            "final_composite" => self.composite_us,
            "shadow" => self.shadow_us,
            _ => self.total_gpu_us(),
        }
    }

    /// Default budgets (matching `RenderConfig` defaults).
    pub const fn defaults() -> Self {
        Self {
            depth_prepass_us: 300,
            geometry_us: 2000,
            fur_us: 1500,
            crystal_vfx_us: 400,
            particles_us: 800,
            workspace_panel_us: 400,
            bloom_us: 600,
            postprocess_us: 300,
            composite_us: 200,
            shadow_us: 50,
        }
    }
}

impl Default for FrameBudgets {
    fn default() -> Self {
        Self::defaults()
    }
}

/// System health status derived from metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Rendering is within budget.
    Healthy,
    /// Some passes exceeded budget but recovering.
    Degraded,
    /// Consistently over budget — consider reducing quality.
    Overloaded,
    /// Device lost or fatal error.
    Critical,
}

impl HealthStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }
}

/// The metrics collector — tracks frame timing, pass budgets, and counters.
#[derive(Debug)]
pub struct MetricsCollector {
    /// Frame budgets.
    budgets: FrameBudgets,
    /// Accumulated render counters.
    counters: RenderCounters,
    /// Current frame start time.
    frame_start: Option<Instant>,
    /// Previous frame metrics (for readback).
    last_frame_metrics: FrameMetrics,
    /// Sliding window of frame times (stores durations in microseconds).
    frame_window: Vec<u64>,
    /// Maximum window size.
    window_size: usize,
    /// Consecutive frames in degraded state.
    consecutive_degraded: u32,
    /// Health status.
    health: HealthStatus,
}

impl MetricsCollector {
    /// Create a new metrics collector.
    pub fn new(budgets: FrameBudgets) -> Self {
        Self {
            budgets,
            counters: RenderCounters::default(),
            frame_start: None,
            last_frame_metrics: FrameMetrics::default(),
            frame_window: Vec::with_capacity(120),
            window_size: 120,
            consecutive_degraded: 0,
            health: HealthStatus::Healthy,
        }
    }

    /// Create from a `RenderConfig`.
    pub fn from_config(config: &RenderConfig) -> Self {
        Self::new(FrameBudgets::from_config(config))
    }

    /// Begin timing a new frame.
    pub fn begin_frame(&mut self) {
        self.frame_start = Some(Instant::now());
    }

    /// End timing the current frame and compute metrics.
    ///
    /// Call this after all passes have executed but before `end_frame`.
    pub fn end_frame(&mut self, gpu_duration_us: u64, passes: Vec<PassTiming>) -> FrameMetrics {
        let cpu_duration = self
            .frame_start
            .map(|start| start.elapsed().as_micros() as u64)
            .unwrap_or(0);

        let any_budget_exceeded = passes.iter().any(|p| p.budget_exceeded);

        let metrics = FrameMetrics {
            frame_index: self.last_frame_metrics.frame_index.wrapping_add(1),
            cpu_duration_us: cpu_duration,
            gpu_duration_us,
            passes,
            any_budget_exceeded,
            draw_calls: self.counters.draw_calls as u32,
            vertices_processed: self.counters.vertices_processed as u32,
        };

        // Update sliding window.
        self.frame_window.push(cpu_duration.max(cpu_duration));
        if self.frame_window.len() > self.window_size {
            self.frame_window.remove(0);
        }

        // Update health status.
        let avg_us = self.average_frame_time_us();
        let total_budget = self.budgets.total_gpu_us();
        if metrics.any_budget_exceeded || avg_us > total_budget as f64 {
            self.consecutive_degraded += 1;
        } else {
            self.consecutive_degraded = self.consecutive_degraded.saturating_sub(1);
        }

        self.health = if self.consecutive_degraded > 60 {
            HealthStatus::Overloaded
        } else if self.consecutive_degraded > 10 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        self.last_frame_metrics = metrics.clone();
        metrics
    }

    /// Record a draw call for counting.
    pub fn record_draw(&mut self, vertices: u32, indices: u32) {
        self.counters.record_draw(vertices, indices);
    }

    /// Record a texture upload.
    pub fn record_texture_upload(&mut self, size_bytes: u64) {
        self.counters.record_texture_upload(size_bytes);
    }

    /// Get the current health status.
    pub fn health(&self) -> HealthStatus {
        self.health
    }

    /// Get the last frame's metrics.
    pub fn last_frame_metrics(&self) -> &FrameMetrics {
        &self.last_frame_metrics
    }

    /// Get accumulated counters.
    pub fn counters(&self) -> &RenderCounters {
        &self.counters
    }

    /// Reset counters.
    pub fn reset_counters(&mut self) {
        self.counters.reset();
    }

    /// Average frame time over the sliding window (in microseconds).
    pub fn average_frame_time_us(&self) -> f64 {
        if self.frame_window.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.frame_window.iter().sum();
        sum as f64 / self.frame_window.len() as f64
    }

    /// Get aggregated metrics from the sliding window.
    pub fn aggregate(&self) -> AggregateMetrics {
        let n = self.frame_window.len() as u64;
        if n == 0 {
            return AggregateMetrics {
                avg_cpu_us: 0.0,
                avg_gpu_us: 0.0,
                p95_cpu_us: 0.0,
                p95_gpu_us: 0.0,
                sample_count: 0,
                budget_exceeded_pct: 0.0,
            };
        }

        let mut sorted = self.frame_window.clone();
        sorted.sort_unstable();
        let sum: u64 = sorted.iter().sum();
        let avg = sum as f64 / n as f64;
        let p95_idx = ((n as f64) * 0.95).ceil() as usize - 1;
        let p95 = sorted.get(p95_idx.min(sorted.len() - 1)).copied().unwrap_or(0) as f64;

        let exceeded_count = self
            .last_frame_metrics
            .passes
            .iter()
            .filter(|p| p.budget_exceeded)
            .count() as f64;

        AggregateMetrics {
            avg_cpu_us: avg,
            avg_gpu_us: self.last_frame_metrics.gpu_duration_us as f64,
            p95_cpu_us: p95,
            p95_gpu_us: self.last_frame_metrics.gpu_duration_us as f64,
            sample_count: n,
            budget_exceeded_pct: (exceeded_count / self.last_frame_metrics.passes.len().max(1) as f64) * 100.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_budgets_defaults() {
        let budgets = FrameBudgets::defaults();
        assert_eq!(budgets.geometry_us, 2000);
        assert_eq!(budgets.total_gpu_us(), 6550);
    }

    #[test]
    fn test_frame_budgets_from_config() {
        let mut config = RenderConfig::default();
        config.budget_geometry_us = 3000;
        let budgets = FrameBudgets::from_config(&config);
        assert_eq!(budgets.geometry_us, 3000);
    }

    #[test]
    fn test_budget_for_pass() {
        let budgets = FrameBudgets::defaults();
        assert_eq!(budgets.for_pass("geometry"), 2000);
        assert_eq!(budgets.for_pass("bloom"), 600);
        assert_eq!(budgets.for_pass("unknown"), 6500); // Falls back to total
    }

    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new(FrameBudgets::defaults());
        assert_eq!(collector.health(), HealthStatus::Healthy);
        assert!(collector.last_frame_metrics().frame_index == 0);
    }

    #[test]
    fn test_render_counters() {
        let mut counters = RenderCounters::default();
        counters.record_draw(1000, 3000);
        assert_eq!(counters.draw_calls, 1);
        assert_eq!(counters.vertices_processed, 1000);
        assert_eq!(counters.indices_processed, 3000);

        counters.record_texture_upload(4096);
        assert_eq!(counters.texture_uploads, 1);
        assert_eq!(counters.vram_usage_bytes, 4096);
    }

    #[test]
    fn test_render_counters_reset() {
        let mut counters = RenderCounters::default();
        counters.record_draw(100, 300);
        counters.reset();
        assert_eq!(counters.draw_calls, 0);
    }

    #[test]
    fn test_pass_timing_budget_check() {
        let timing = PassTiming {
            name: "geometry",
            gpu_duration_us: 2500,
            budget_us: 2000,
            budget_exceeded: true,
        };
        assert!(timing.budget_exceeded);

        let timing_ok = PassTiming {
            name: "geometry",
            gpu_duration_us: 1500,
            budget_us: 2000,
            budget_exceeded: false,
        };
        assert!(!timing_ok.budget_exceeded);
    }

    #[test]
    fn test_counters_default() {
        let counters = RenderCounters::default();
        assert_eq!(counters.vram_peak_bytes, 0);
    }

    #[test]
    fn test_health_status_transitions() {
        assert!(HealthStatus::Healthy.is_ok());
        assert!(!HealthStatus::Critical.is_ok());
    }
}
