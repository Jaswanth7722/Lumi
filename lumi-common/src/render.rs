//! # Rendering Engine — Pipeline and GPU Types (Chapter 17)
//!
//! Defines the rendering pipeline stages, wgpu configuration,
//! transparency compositing, bloom effect, and LOD system.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Render Pipeline
// ---------------------------------------------------------------------------

/// Stages of the Lumi rendering pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderPass {
    DepthPrePass,
    GeometryPass,
    FurPass,
    TransparencyPass,
    LightingPass,
    PostProcessingPass,
    UICompositePass,
    FinalCompositePass,
}

/// Configuration for the rendering pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderPipelineConfig {
    /// Target frame rate for the application.
    pub target_fps: u32,
    /// Whether VSync is enabled.
    pub vsync: bool,
    /// Whether bloom post-processing is enabled.
    pub bloom_enabled: bool,
    /// Whether FXAA anti-aliasing is enabled.
    pub fxaa_enabled: bool,
    /// Whether vignette effect is enabled.
    pub vignette_enabled: bool,
    /// Fur shell count (24 default, 12 for low-end GPUs).
    pub fur_shells: u32,
}

impl Default for RenderPipelineConfig {
    fn default() -> Self {
        Self {
            target_fps: 60,
            vsync: true,
            bloom_enabled: true,
            fxaa_enabled: true,
            vignette_enabled: true,
            fur_shells: 24,
        }
    }
}

impl RenderPipelineConfig {
    /// Create a low-end GPU config with reduced quality.
    pub fn low_performance() -> Self {
        Self {
            fur_shells: 12,
            bloom_enabled: false,
            fxaa_enabled: false,
            vignette_enabled: false,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Graphics Backend
// ---------------------------------------------------------------------------

/// The GPU backend API being used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphicsBackend {
    Metal,
    DirectX12,
    Vulkan,
}

/// GPU adapter information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GPUAdapterInfo {
    pub name: String,
    pub backend: GraphicsBackend,
    pub dedicated_memory_mb: u64,
    pub supports_raytracing: bool,
    pub max_texture_size: u32,
}

// ---------------------------------------------------------------------------
// Compositing
// ---------------------------------------------------------------------------

/// Alpha composite mode for transparent window rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompositeAlphaMode {
    /// Standard alpha blending.
    Auto,
    /// Pre-multiplied alpha for correct transparent compositing.
    PreMultiplied,
    /// Opaque (no transparency).
    Opaque,
}

// ---------------------------------------------------------------------------
// Bloom Effect
// ---------------------------------------------------------------------------

/// Configuration for the bloom post-processing effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloomConfig {
    /// Luminance threshold for bloom extraction (default 0.8).
    pub threshold: f32,
    /// Number of blur passes (default 4).
    pub blur_passes: u32,
    /// Bloom intensity (default 0.3).
    pub intensity: f32,
    /// Whether crystal emission is always bloomed (exempt from threshold).
    pub crystal_always_bloom: bool,
}

impl Default for BloomConfig {
    fn default() -> Self {
        Self {
            threshold: 0.8,
            blur_passes: 4,
            intensity: 0.3,
            crystal_always_bloom: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Level of Detail
// ---------------------------------------------------------------------------

/// A single LOD level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LODConfig {
    /// Maximum distance (in screen pixels from monitor center) for this LOD.
    pub max_distance_px: f32,
    /// Triangle budget for this LOD.
    pub triangle_budget: u32,
    /// Number of fur shells for this LOD.
    pub fur_shells: u32,
    /// Cross-fade duration in milliseconds for LOD transitions.
    pub transition_ms: u64,
}

/// Complete LOD system configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LODSystemConfig {
    pub levels: Vec<LODConfig>,
}

impl Default for LODSystemConfig {
    fn default() -> Self {
        Self {
            levels: vec![
                LODConfig {
                    max_distance_px: 400.0,
                    triangle_budget: 18000,
                    fur_shells: 24,
                    transition_ms: 50,
                },
                LODConfig {
                    max_distance_px: 700.0,
                    triangle_budget: 9000,
                    fur_shells: 16,
                    transition_ms: 50,
                },
                LODConfig {
                    max_distance_px: f32::MAX,
                    triangle_budget: 4000,
                    fur_shells: 8,
                    transition_ms: 50,
                },
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Lighting
// ---------------------------------------------------------------------------

/// Lighting configuration for the character render.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightingConfig {
    /// Whether to sample ambient color from the desktop wallpaper.
    pub ambient_from_wallpaper: bool,
    /// Key light angle based on time of day.
    pub time_of_day_lighting: bool,
    /// Crystal fill light intensity multiplier.
    pub crystal_fill_intensity: f32,
    /// Orb fill light radius in world units.
    pub orb_fill_radius: f32,
    /// Rim light intensity.
    pub rim_light_intensity: f32,
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            ambient_from_wallpaper: true,
            time_of_day_lighting: true,
            crystal_fill_intensity: 0.5,
            orb_fill_radius: 0.3,
            rim_light_intensity: 0.2,
        }
    }
}

// ---------------------------------------------------------------------------
// Performance Budget
// ---------------------------------------------------------------------------

/// Per-pass GPU time budget in milliseconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderBudget {
    pub depth_pre_pass_ms: f32,
    pub geometry_pbr_ms: f32,
    pub fur_shells_ms: f32,
    pub lighting_ms: f32,
    pub post_processing_ms: f32,
    pub ui_composite_ms: f32,
    pub final_composite_ms: f32,
}

impl Default for RenderBudget {
    fn default() -> Self {
        Self {
            depth_pre_pass_ms: 0.3,
            geometry_pbr_ms: 2.0,
            fur_shells_ms: 1.5,
            lighting_ms: 0.8,
            post_processing_ms: 0.6,
            ui_composite_ms: 0.4,
            final_composite_ms: 0.2,
        }
    }
}

impl RenderBudget {
    /// Total GPU time budget across all passes.
    pub fn total_ms(&self) -> f32 {
        self.depth_pre_pass_ms
            + self.geometry_pbr_ms
            + self.fur_shells_ms
            + self.lighting_ms
            + self.post_processing_ms
            + self.ui_composite_ms
            + self.final_composite_ms
    }

    /// Equivalent frame rate at this budget.
    pub fn equivalent_fps(&self) -> f32 {
        1000.0 / self.total_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_budget() {
        let budget = RenderBudget::default();
        let total = budget.total_ms();
        assert!((total - 5.8).abs() < 0.01);
        let fps = budget.equivalent_fps();
        assert!(fps > 60.0); // headroom above 60 FPS
    }

    #[test]
    fn test_low_performance_config() {
        let config = RenderPipelineConfig::low_performance();
        assert_eq!(config.fur_shells, 12);
        assert!(!config.bloom_enabled);
        assert!(!config.fxaa_enabled);
    }

    #[test]
    fn test_lod_levels() {
        let lod = LODSystemConfig::default();
        assert_eq!(lod.levels.len(), 3);
        assert_eq!(lod.levels[0].triangle_budget, 18000);
        assert_eq!(lod.levels[1].triangle_budget, 9000);
        assert_eq!(lod.levels[2].triangle_budget, 4000);
    }

    #[test]
    fn test_bloom_default() {
        let bloom = BloomConfig::default();
        assert!(bloom.crystal_always_bloom);
        assert!((bloom.threshold - 0.8).abs() < f32::EPSILON);
    }
}
