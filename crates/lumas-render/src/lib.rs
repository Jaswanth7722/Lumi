//! # Lumas Rendering Engine
//!
//! GPU-accelerated rendering engine for the Lumas desktop AI companion.
//! Built on wgpu for cross-platform Vulkan/Metal/DX12 support.
//!
//! ## Architecture: The Render Graph
//!
//! Lumi's rendering is organized as a **data-driven render graph**, not a fixed
//! sequential pipeline. A render graph is a directed acyclic graph where:
//!
//! - **Nodes** are render passes (depth prepass, geometry pass, fur pass, etc.)
//! - **Edges** are resource dependencies (pass B reads the depth buffer from pass A)
//! - **Compilation** resolves execution order, infers barriers, and culls unused passes
//!
//! This design allows the set of active passes to change per-frame without
//! conditional logic spaghetti. Examples:
//! - **Focus mode**: cull all passes except the minimal alpha-clear pass
//! - **Sleeping**: cull fur passes, particle pass, bloom pass
//! - **Celebration**: add particle burst pass
//!
//! ## Frame Budget
//!
//! The 16.6ms frame budget is explicitly allocated (hard ceilings):
//!
//! | Pass | GPU Time | CPU Time |
//! |---|---|---|
//! | DepthPrepass | 0.3ms | 0.1ms |
//! | GeometryPass | 2.0ms | 0.3ms |
//! | FurPass | 1.5ms | 0.2ms |
//! | CrystalVFX | 0.4ms | 0.1ms |
//! | Particles | 0.8ms | 0.2ms |
//! | Panels | 0.4ms | 0.2ms |
//! | Bloom | 0.6ms | 0.1ms |
//! | PostProcess | 0.3ms | 0.1ms |
//! | Composite | 0.2ms | 0.1ms |
//! | **Total** | **6.5ms** | **1.4ms** |
//!
//! ## Pre-Multiplied Alpha Compositing
//!
//! Lumas renders to a transparent desktop window. This requires **pre-multiplied
//! alpha** throughout the entire pipeline. A standard alpha blend produces
//! incorrect results on transparent backgrounds.
//!
//! 1. Character textures are stored pre-multiplied (during import)
//! 2. All blend states use `wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING`
//! 3. The final composite pass pre-multiplies alpha in the shader
//! 4. Surface uses `CompositeAlphaMode::PreMultiplied` where supported
//!
//! ## IPC Command → Render Graph Mapping
//!
//! | IPC Message | Channel | Render Engine Action | Latency |
//! |---|---|---|---|
//! | `CharacterMove(pos)` | render.command | Update world transform | < 1 frame |
//! | `AnimationPose(bones)` | render.command | Upload bone matrices to GPU | < 1 frame |
//! | `AiState(state)` | ai.state | Set crystal emission, schedule VFX | < 2 frames |
//! | `ShowPanel(type)` | render.command | Instantiate panel geometry | < 2 frames |
//!
//! ## Frame-in-Flight
//!
//! wgpu's submit model means the GPU may be executing frame N while the CPU
//! prepares frame N+1. The engine uses N=2 ring buffers for:
//! - Per-frame uniform data (bone matrices, camera UBO)
//! - Staging upload buffers
//! - GPU timestamp query results

pub mod assets;
pub mod camera;
pub mod compositor;
pub mod config;
pub mod context;
pub mod diagnostics;
pub mod error;
pub mod frame;
pub mod graph;
pub mod lighting;
pub mod material;
pub mod mesh;
pub mod metrics;
pub mod overlay;
pub mod renderer;
pub mod resource;
pub mod scene;
pub mod shader;
pub mod texture;
pub mod viewport;

// Pass modules
pub mod passes {
    use crate::graph::GraphResourceId;

    /// Resource IDs for the render graph's standard texture resources.
    pub const RESOURCE_DEPTH: GraphResourceId = GraphResourceId(0);
    pub const RESOURCE_COLOR: GraphResourceId = GraphResourceId(1);
    pub const RESOURCE_BLOOM_SOURCE: GraphResourceId = GraphResourceId(2);
    pub const RESOURCE_OUTPUT: GraphResourceId = GraphResourceId(3);
    pub const RESOURCE_SURFACE: GraphResourceId = GraphResourceId(4);

    pub mod bloom;
    pub mod crystal_vfx;
    pub mod depth_prepass;
    pub mod final_composite;
    pub mod fur;
    pub mod geometry;
    pub mod particle;
    pub mod postprocess;
    pub mod shadow;
    pub mod workspace_panel;
}

// Re-exports
pub use assets::AssetPipeline;
pub use camera::Camera;
pub use compositor::Compositor;
pub use config::RenderConfig;
pub use context::GpuContext;
pub use diagnostics::Diagnostics;
pub use error::RenderError;
pub use frame::{FrameScheduler, VsyncMode};
pub use graph::{RenderGraph, RenderPass, PassId, FrameContext, GraphResourceId};
pub use material::{MaterialKind, MaterialManager, PipelineManager, PipelineId, MaterialId};
pub use mesh::{GpuMesh, MeshId, CharacterVertex};
pub use metrics::{MetricsCollector, FrameMetrics, FrameBudgets, HealthStatus};
pub use overlay::OverlayRenderer;
pub use renderer::Renderer;
pub use scene::Scene;
pub use shader::{ShaderManager, ShaderId, ShaderSource};
pub use texture::{TextureManager, TextureId, SamplerId, ImageData};
pub use viewport::{Viewport, PhysicalSize, LogicalSize, DpiScale};

use std::sync::Arc;

/// Initialize the rendering engine with the given configuration.
/// This is a convenience function that creates a full Renderer.
pub async fn initialize(
    config: &RenderConfig,
) -> Result<Renderer, RenderError> {
    let renderer = Renderer::new(config, None).await?;
    Ok(renderer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_imports() {
        // Verify that core types can be constructed.
        let _ = config::RenderConfig::default();
        let _ = error::RenderError::adapter_not_found("test");
    }
}
