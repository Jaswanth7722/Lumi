//! # Performance Engineering — Budgets and Optimization (Chapter 25)
//!
//! Defines performance budgets, frame pacing, response caching,
//! and optimization strategies for the Lumi platform.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Performance Budgets
// ---------------------------------------------------------------------------

/// CPU, GPU, and memory budget for a single subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsystemBudget {
    pub name: &'static str,
    pub cpu_percent_avg: f32,
    pub cpu_percent_burst: f32,
    pub gpu_time_ms: f32,
    pub memory_mb: u32,
}

/// Complete performance budget summary for all subsystems.
pub fn default_subsystem_budgets() -> Vec<SubsystemBudget> {
    vec![
        SubsystemBudget { name: "render", cpu_percent_avg: 4.0, cpu_percent_burst: 12.0, gpu_time_ms: 5.8, memory_mb: 512 },
        SubsystemBudget { name: "core-idle", cpu_percent_avg: 0.5, cpu_percent_burst: 0.5, gpu_time_ms: 0.0, memory_mb: 128 },
        SubsystemBudget { name: "core-inference", cpu_percent_avg: 12.0, cpu_percent_burst: 60.0, gpu_time_ms: 0.0, memory_mb: 256 },
        SubsystemBudget { name: "voice-listening", cpu_percent_avg: 2.0, cpu_percent_burst: 2.0, gpu_time_ms: 0.0, memory_mb: 64 },
        SubsystemBudget { name: "voice-stt", cpu_percent_avg: 15.0, cpu_percent_burst: 15.0, gpu_time_ms: 0.0, memory_mb: 128 },
        SubsystemBudget { name: "storage", cpu_percent_avg: 0.2, cpu_percent_burst: 0.2, gpu_time_ms: 0.0, memory_mb: 64 },
        SubsystemBudget { name: "plugin-host", cpu_percent_avg: 2.0, cpu_percent_burst: 2.0, gpu_time_ms: 0.0, memory_mb: 64 },
    ]
}

// ---------------------------------------------------------------------------
// Frame Pacing
// ---------------------------------------------------------------------------

/// Configuration for the render frame pacer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FramePacerConfig {
    /// Target frames per second.
    pub target_fps: u32,
    /// Whether adaptive VSync is enabled.
    pub adaptive_vsync: bool,
    /// Whether to use a fixed frame budget or adaptive.
    pub fixed_budget: bool,
}

impl Default for FramePacerConfig {
    fn default() -> Self {
        Self {
            target_fps: 60,
            adaptive_vsync: true,
            fixed_budget: false,
        }
    }
}

impl FramePacerConfig {
    /// The frame budget in microseconds.
    pub fn frame_budget_us(&self) -> u64 {
        1_000_000 / self.target_fps as u64
    }
}

/// Stateful frame pacer for stable 60 FPS rendering.
#[derive(Debug, Clone)]
pub struct FramePacer {
    pub config: FramePacerConfig,
    /// Timestamp of the last frame.
    pub last_frame: Option<std::time::Instant>,
    /// Accumulated frame time statistics.
    pub frame_times: Vec<f64>,
}

impl FramePacer {
    pub fn new(config: FramePacerConfig) -> Self {
        Self {
            config,
            last_frame: None,
            frame_times: Vec::with_capacity(120),
        }
    }

    /// Wait for the next frame based on elapsed time since last frame.
    pub fn wait_for_next_frame(&mut self) {
        let budget_us = self.config.frame_budget_us();
        let headroom_us = 500;

        if let Some(last) = self.last_frame {
            let elapsed_us = last.elapsed().as_micros() as u64;
            if elapsed_us < budget_us {
                let sleep_us = budget_us.saturating_sub(elapsed_us).saturating_sub(headroom_us);
                if sleep_us > 0 {
                    std::thread::sleep(std::time::Duration::from_micros(sleep_us));
                }
            }
        }

        // Track frame time
        if let Some(last) = self.last_frame {
            let frame_time = last.elapsed().as_secs_f64();
            self.frame_times.push(frame_time);
            if self.frame_times.len() > 120 {
                self.frame_times.remove(0);
            }
        }

        self.last_frame = Some(std::time::Instant::now());
    }

    /// Get the average frame time (in seconds) over the last 120 frames.
    pub fn average_frame_time(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        self.frame_times.iter().sum::<f64>() / self.frame_times.len() as f64
    }

    /// Get the current FPS based on frame time history.
    pub fn current_fps(&self) -> f64 {
        let avg = self.average_frame_time();
        if avg > 0.0 { 1.0 / avg } else { 0.0 }
    }
}

// ---------------------------------------------------------------------------
// Response Cache
// ---------------------------------------------------------------------------

/// Configuration for the AI response cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseCacheConfig {
    pub max_entries: usize,
    pub max_age_secs: u64,
    pub enabled: bool,
}

impl Default for ResponseCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 100,
            max_age_secs: 3600,
            enabled: true,
        }
    }
}

/// A cached AI response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResponse {
    pub response_text: String,
    pub created_at: i64,
    pub access_count: u64,
}

/// Key for the response cache.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResponseCacheKey {
    pub query_hash: u64,
    pub system_prompt_hash: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_budget() {
        let config = FramePacerConfig::default();
        assert_eq!(config.frame_budget_us(), 16_666); // 1,000,000 / 60
    }

    #[test]
    fn test_frame_pacer_tracks_fps() {
        let mut pacer = FramePacer::new(FramePacerConfig::default());
        pacer.last_frame = Some(std::time::Instant::now());
        // Add some simulated frame times
        pacer.frame_times.push(0.016);
        pacer.frame_times.push(0.017);
        pacer.frame_times.push(0.015);
        let fps = pacer.current_fps();
        assert!((fps - 62.5).abs() < 10.0); // ~60 FPS
    }

    #[test]
    fn test_subsystem_budgets() {
        let budgets = default_subsystem_budgets();
        assert!(budgets.len() >= 7);
        assert!(budgets.iter().any(|b| b.name == "render" && b.memory_mb == 512));
        let render = budgets.iter().find(|b| b.name == "render").unwrap();
        assert!((render.gpu_time_ms - 5.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_response_cache_config() {
        let config = ResponseCacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_entries, 100);
    }
}
