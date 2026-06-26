use serde::{Deserialize, Serialize};

/// Performance tuning configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PerformanceConfig {
    #[serde(default = "default_fps")]
    pub render_fps_cap: u32,
    #[serde(default = "default_quality")]
    pub render_quality: String,
    #[serde(default = "default_gpu_mem")]
    pub gpu_memory_limit_mb: u32,
    #[serde(default = "default_anim_quality")]
    pub animation_quality: String,
    #[serde(default = "default_true")]
    pub hardware_acceleration: bool,
    #[serde(default = "default_threads")]
    pub worker_thread_count: u32,
}

fn default_fps() -> u32 {
    60
}
fn default_quality() -> String {
    "auto".into()
}
fn default_gpu_mem() -> u32 {
    1200
}
fn default_anim_quality() -> String {
    "full".into()
}
fn default_true() -> bool {
    true
}
fn default_threads() -> u32 {
    4
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            render_fps_cap: 60,
            render_quality: default_quality(),
            gpu_memory_limit_mb: 1200,
            animation_quality: default_anim_quality(),
            hardware_acceleration: true,
            worker_thread_count: 4,
        }
    }
}
