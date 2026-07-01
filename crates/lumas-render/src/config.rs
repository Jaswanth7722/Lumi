//! Render configuration with frame budget allocations, feature flags, and quality settings.
//!
//! # Frame Budget
//!
//! The 16.6ms frame budget is explicitly allocated with hard ceilings for each pass.
//! These values are enforced by `criterion` benchmarks and `RenderMetrics` alerts.

/// Core rendering configuration.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    // --- Window/Surface ---
    /// Initial surface width in physical pixels.
    pub surface_width: u32,
    /// Initial surface height in physical pixels.
    pub surface_height: u32,
    /// Preferred present mode (vsync strategy).
    pub present_mode: PresentMode,
    /// Composite alpha mode for transparent desktop window.
    pub composite_alpha: CompositeAlphaMode,
    /// Surface format preference.
    pub surface_format: Option<wgpu::TextureFormat>,

    // --- Frame Budgets (GPU time in microseconds) ---
    pub budget_depth_prepass_us: u64,
    pub budget_geometry_us: u64,
    pub budget_fur_us: u64,
    pub budget_crystal_vfx_us: u64,
    pub budget_particle_us: u64,
    pub budget_workspace_panel_us: u64,
    pub budget_bloom_us: u64,
    pub budget_postprocess_us: u64,
    pub budget_composite_us: u64,
    pub budget_shadow_us: u64,

    // --- Quality ---
    /// Fur shell count at full quality.
    pub fur_shells_high: u32,
    /// Fur shell count at medium quality.
    pub fur_shells_medium: u32,
    /// Fur shell count at low quality.
    pub fur_shells_low: u32,
    /// Maximum number of active particles.
    pub max_particles: u32,
    /// Whether bloom is enabled.
    pub bloom_enabled: bool,
    /// Whether FXAA is enabled.
    pub fxaa_enabled: bool,
    /// Bloom strength factor.
    pub bloom_strength: f32,
    /// Bloom luminance threshold.
    pub bloom_threshold: f32,

    // --- Adapter Selection ---
    /// Preferred GPU backend.
    pub preferred_backend: Option<GpuBackend>,
    /// Force integrated GPU (default: true for laptops).
    pub prefer_integrated_gpu: bool,

    // --- Debug ---
    /// Enable GPU validation layers and debug labels.
    pub gpu_debug: bool,
    /// Enable shader hot-reloading.
    pub hot_reload: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            surface_width: 1920,
            surface_height: 1080,
            present_mode: PresentMode::Adaptive,
            composite_alpha: CompositeAlphaMode::PreMultiplied,
            surface_format: None,

            // Frame budgets (hard ceilings enforced by benchmarks).
            budget_depth_prepass_us: 300,
            budget_geometry_us: 2000,
            budget_fur_us: 1500,
            budget_crystal_vfx_us: 400,
            budget_particle_us: 800,
            budget_workspace_panel_us: 400,
            budget_bloom_us: 600,
            budget_postprocess_us: 300,
            budget_composite_us: 200,
            budget_shadow_us: 50,

            // Quality defaults.
            fur_shells_high: 24,
            fur_shells_medium: 16,
            fur_shells_low: 8,
            max_particles: 4096,
            bloom_enabled: true,
            fxaa_enabled: true,
            bloom_strength: 0.04,
            bloom_threshold: 0.8,

            preferred_backend: None,
            prefer_integrated_gpu: true,

            gpu_debug: cfg!(debug_assertions),
            hot_reload: false,
        }
    }
}

/// V-sync / present mode strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentMode {
    /// Standard vsync (wgpu::PresentMode::Fifo).
    Fifo,
    /// Low-latency vsync (wgpu::PresentMode::Mailbox).
    Mailbox,
    /// No vsync, uncapped (wgpu::PresentMode::Immediate).
    Immediate,
    /// Dynamic: Fifo when healthy, Immediate when over budget.
    Adaptive,
}

impl PresentMode {
    /// Convert to wgpu present mode.
    pub fn to_wgpu(&self) -> wgpu::PresentMode {
        match self {
            PresentMode::Fifo => wgpu::PresentMode::Fifo,
            PresentMode::Mailbox => wgpu::PresentMode::Mailbox,
            PresentMode::Immediate | PresentMode::Adaptive => wgpu::PresentMode::Immediate,
        }
    }
}

/// Composite alpha mode for transparent window compositing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositeAlphaMode {
    /// Pre-multiplied alpha (correct for transparent windows).
    PreMultiplied,
    /// Opaque (fallback when PreMultiplied is not supported).
    Opaque,
}

impl CompositeAlphaMode {
    /// Convert to wgpu composite alpha mode.
    pub fn to_wgpu(&self) -> wgpu::CompositeAlphaMode {
        match self {
            CompositeAlphaMode::PreMultiplied => wgpu::CompositeAlphaMode::PreMultiplied,
            CompositeAlphaMode::Opaque => wgpu::CompositeAlphaMode::Opaque,
        }
    }
}

/// GPU backend identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuBackend {
    Metal,
    Dx12,
    Vulkan,
    Gl,
}

impl GpuBackend {
    pub fn to_wgpu(&self) -> wgpu::Backend {
        match self {
            GpuBackend::Metal => wgpu::Backend::Metal,
            GpuBackend::Dx12 => wgpu::Backend::Dx12,
            GpuBackend::Vulkan => wgpu::Backend::Vulkan,
            GpuBackend::Gl => wgpu::Backend::Gl,
        }
    }
}

/// Quality preset for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityPreset {
    Low,
    Medium,
    High,
    Ultra,
}

impl QualityPreset {
    /// Apply quality preset to the given config.
    pub fn apply(&self, config: &mut RenderConfig) {
        match self {
            QualityPreset::Low => {
                config.fur_shells_high = 0;
                config.fur_shells_medium = 0;
                config.fur_shells_low = 0;
                config.bloom_enabled = false;
                config.fxaa_enabled = false;
                config.max_particles = 512;
            }
            QualityPreset::Medium => {
                config.fur_shells_high = 8;
                config.fur_shells_medium = 8;
                config.fur_shells_low = 0;
                config.bloom_enabled = true;
                config.fxaa_enabled = true;
                config.max_particles = 2048;
            }
            QualityPreset::High => {
                // Defaults are already high quality.
            }
            QualityPreset::Ultra => {
                config.fur_shells_high = 32;
                config.fur_shells_medium = 24;
                config.fur_shells_low = 16;
                config.bloom_enabled = true;
                config.fxaa_enabled = true;
                config.max_particles = 8192;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RenderConfig::default();
        assert!(config.budget_geometry_us > 0);
        assert!(config.fur_shells_high > 0);
        assert!(config.max_particles > 0);
    }

    #[test]
    fn test_quality_preset_low() {
        let mut config = RenderConfig::default();
        QualityPreset::Low.apply(&mut config);
        assert_eq!(config.fur_shells_high, 0);
        assert!(!config.bloom_enabled);
    }

    #[test]
    fn test_present_mode_wgpu_conversion() {
        assert_eq!(PresentMode::Fifo.to_wgpu(), wgpu::PresentMode::Fifo);
        assert_eq!(PresentMode::Mailbox.to_wgpu(), wgpu::PresentMode::Mailbox);
    }
}


