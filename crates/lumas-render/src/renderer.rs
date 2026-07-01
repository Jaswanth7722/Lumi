//! Main renderer — orchestrates the frame pipeline.
//!
//! The `Renderer` is the top-level orchestrator that ties together:
//! - `GpuContext` — GPU device, queue, and surface
//! - `FrameScheduler` — N=2 frame-in-flight ring buffers
//! - `Scene` — all renderable entities for the current frame
//! - `RenderGraph` — compiled DAG of render passes
//! - `Compositor` — surface presentation and swapchain management
//! - `OverlayRenderer` — hit-test mask generation
//!
//! # Frame Pipeline
//!
//! Each frame follows this pipeline:
//! 1. `begin_frame()` — acquire surface texture, prepare frame context
//! 2. `upload_uniforms()` — write camera, lighting, bone data to ring buffer
//! 3. `compile_graph()` — cull inactive passes, resolve execution order
//! 4. `execute_graph()` — run all active passes with resolved resources
//! 5. `end_frame()` — present to surface, advance frame counter
//!
//! # Thread Safety
//!
//! The renderer must be used from a single render thread. The IPC receiver
//! writes to the Scene (which is `Send + Sync`), and the render thread reads
//! from it once per frame via double-buffering.

use crate::camera::CameraUBO;
use crate::compositor::Compositor;
use crate::config::RenderConfig;
use crate::context::GpuContext;
use crate::error::{ErrorSeverity, RenderError};
use crate::frame::{FrameScheduler, VsyncMode};
use crate::graph::{FrameContext, GraphResourceId, PassId, RenderGraph, RenderPass, ResolvedResources};
use crate::lighting::LightingUBO;
use crate::metrics::{FrameBudgets, MetricsCollector, PassTiming};
use crate::overlay::OverlayRenderer;
use crate::resource::ResourcePool;
use crate::scene::{BoneMatrices, Scene, ShadowInstanceGPU, MAX_BONES};
use crate::shader::ShaderManager;
use crate::texture::TextureManager;

/// Intermediate framebuffer textures required by the render graph.
pub struct FramebufferSet {
    /// Depth buffer (Depth32Float) — written by depth_prepass, read by geometry + fur.
    pub depth: Option<wgpu::Texture>,
    pub depth_view: Option<wgpu::TextureView>,
    /// HDR color buffer (Rgba16Float) — primary render target for all pass output.
    pub color_hdr: Option<wgpu::Texture>,
    pub color_hdr_view: Option<wgpu::TextureView>,
    /// Bloom source buffer (Rgba16Float) — written by crystal_vfx, read by bloom.
    pub bloom_source: Option<wgpu::Texture>,
    pub bloom_source_view: Option<wgpu::TextureView>,
    /// LDR output buffer (Rgba8UnormSrgb) — written by postprocess, read by final_composite.
    pub ldr_output: Option<wgpu::Texture>,
    pub ldr_output_view: Option<wgpu::TextureView>,
    /// Surface dimensions (physical pixels).
    pub width: u32,
    pub height: u32,
}

impl FramebufferSet {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let mut fb = Self {
            depth: None,
            depth_view: None,
            color_hdr: None,
            color_hdr_view: None,
            bloom_source: None,
            bloom_source_view: None,
            ldr_output: None,
            ldr_output_view: None,
            width,
            height,
        };
        fb.allocate(device, width, height);
        fb
    }

    /// Allocate or reallocate all framebuffer textures at the given size.
    pub fn allocate(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        self.width = w;
        self.height = h;

        // Depth buffer.
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("framebuffer_depth"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        self.depth = Some(depth);
        self.depth_view = Some(depth_view);

        // HDR color buffer.
        let color_hdr = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("framebuffer_color_hdr"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let color_hdr_view = color_hdr.create_view(&wgpu::TextureViewDescriptor::default());
        self.color_hdr = Some(color_hdr);
        self.color_hdr_view = Some(color_hdr_view);

        // Bloom source buffer.
        let bloom_source = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("framebuffer_bloom_source"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let bloom_source_view = bloom_source.create_view(&wgpu::TextureViewDescriptor::default());
        self.bloom_source = Some(bloom_source);
        self.bloom_source_view = Some(bloom_source_view);

        // LDR output buffer.
        let ldr_output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("framebuffer_ldr_output"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let ldr_output_view = ldr_output.create_view(&wgpu::TextureViewDescriptor::default());
        self.ldr_output = Some(ldr_output);
        self.ldr_output_view = Some(ldr_output_view);
    }
}

impl std::fmt::Debug for FramebufferSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FramebufferSet")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("depth", &self.depth.is_some())
            .field("color_hdr", &self.color_hdr.is_some())
            .field("bloom_source", &self.bloom_source.is_some())
            .field("ldr_output", &self.ldr_output.is_some())
            .finish()
    }
}

/// Statistics collected during rendering.
#[derive(Debug, Clone, Default)]
pub struct RenderStatistics {
    /// Number of frames rendered.
    pub frames_rendered: u64,
    /// Number of consecutive frames that exceeded the budget.
    pub consecutive_over_budget: u32,
    /// Whether the last frame hit the budget.
    pub last_frame_on_budget: bool,
    /// Average CPU frame time (in microseconds).
    pub avg_cpu_frame_time_us: f64,
    /// Average GPU frame time (in microseconds).
    pub avg_gpu_frame_time_us: f64,
}

/// The main renderer orchestrator.
///
/// Create via `Renderer::new()` or the convenience function `crate::initialize()`.
pub struct Renderer {
    /// GPU context.
    pub ctx: GpuContext,
    /// Frame scheduler with N=2 ring buffers.
    frame_scheduler: FrameScheduler,
    /// Render graph (DAG of passes).
    graph: RenderGraph,
    /// Resource pool with deferred deletion.
    resource_pool: ResourcePool,
    /// Shader manager.
    shader_manager: ShaderManager,
    /// Texture manager.
    texture_manager: TextureManager,
    /// Compositor (surface management).
    compositor: Compositor,
    /// Overlay renderer (hit-test masks).
    overlay: OverlayRenderer,
    /// The current scene (read by the render thread).
    scene: Scene,
    /// Configuration.
    config: RenderConfig,
    /// Statistics.
    stats: RenderStatistics,
    /// Whether the renderer has been initialized with scene data.
    initialized: bool,
    /// Intermediate framebuffers.
    framebuffers: FramebufferSet,
    /// Camera bind group layout (shared across passes).
    camera_bind_group_layout: wgpu::BindGroupLayout,
    /// Bone matrix bind group layout (shared across passes).
    bone_bind_group_layout: wgpu::BindGroupLayout,
    /// Lighting bind group layout (shared across passes).
    lighting_bind_group_layout: wgpu::BindGroupLayout,
    /// Per-frame camera bind group (recreated each frame).
    camera_bind_group: Option<wgpu::BindGroup>,
    /// Per-frame bone matrix bind group.
    bone_matrix_bind_group: Option<wgpu::BindGroup>,
    /// Per-frame lighting bind group.
    lighting_bind_group: Option<wgpu::BindGroup>,
    /// Acquired surface texture for the current frame (dropped after present).
    surface_texture: Option<wgpu::SurfaceTexture>,
    /// Guard: true between begin_frame() and end_frame(). Prevents double-begin.
    frame_in_flight: bool,
    /// Per-frame metrics collector (timing, budgets, counters, health).
    metrics: MetricsCollector,
}

impl Renderer {
    /// Create a new renderer with all subsystems.
    ///
    /// # GPU Thread Safety
    /// Must be created on the render thread.
    ///
    /// # Errors
    /// Returns `RenderError::AdapterNotFound` if no suitable GPU is found.
    /// Returns `RenderError::DeviceLost` if device creation fails.
    pub async fn new(
        config: &RenderConfig,
        raw_handle: Option<&raw_window_handle::RawWindowHandle>,
    ) -> Result<Self, RenderError> {
        let ctx = GpuContext::new(raw_handle, config).await?;
        let shader_manager = ShaderManager::new(&ctx.device);
        let texture_manager = TextureManager::new(&ctx.device, &ctx.adapter);

        let (width, height) = ctx.surface_config
            .as_ref()
            .map(|c| (c.width, c.height))
            .unwrap_or((config.surface_width, config.surface_height));

        let frame_scheduler = FrameScheduler::new(
            &ctx.device,
            config,
            ctx.timestamp_queries_available,
            ctx.timestamp_period_ns,
        );

        let compositor = Compositor::new(&ctx, config);
        let overlay = OverlayRenderer::new(
            width,
            height,
            if config.composite_alpha == crate::config::CompositeAlphaMode::PreMultiplied {
                crate::overlay::OverlayCompositeMode::PreMultiplied
            } else {
                crate::overlay::OverlayCompositeMode::Opaque
            },
        );

        // Allocate intermediate framebuffers.
        let framebuffers = FramebufferSet::new(&ctx.device, width, height);

        // Create shared bind group layouts for camera, bone, and lighting.
        // These must match the layouts created by each pass.
        let camera_bind_group_layout = ctx.device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("renderer_camera_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            },
        );

        let bone_bind_group_layout = ctx.device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("renderer_bone_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            },
        );

        let lighting_bind_group_layout = ctx.device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("renderer_lighting_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            },
        );

        Ok(Self {
            ctx,
            frame_scheduler,
            graph: RenderGraph::new(),
            resource_pool: ResourcePool::new(),
            shader_manager,
            texture_manager,
            compositor,
            overlay,
            scene: Scene::new(),
            config: config.clone(),
            stats: RenderStatistics::default(),
            initialized: false,
            framebuffers,
            camera_bind_group_layout,
            bone_bind_group_layout,
            lighting_bind_group_layout,
            camera_bind_group: None,
            bone_matrix_bind_group: None,
            lighting_bind_group: None,
            surface_texture: None,
            frame_in_flight: false,
            metrics: MetricsCollector::from_config(config),
        })
    }

    /// Initialize the renderer with all built-in passes.
    ///
    /// Registers the standard render graph passes. Call this after `new()`
    /// and before the first frame.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    ///
    /// # Errors
    /// Returns `RenderError::RenderPassFailed` if a pass cannot be registered.
    pub fn initialize_passes(&mut self) -> Result<(), RenderError> {
        use crate::passes::*;

        let depth_prepass = Box::new(depth_prepass::DepthPrepassPass::new(
            PassId(0), &self.ctx,
        )?);
        self.graph.register_pass(depth_prepass)?;

        let geometry = Box::new(geometry::GeometryPass::new(
            PassId(1), &self.ctx,
        )?);
        self.graph.register_pass(geometry)?;

        let fur = Box::new(fur::FurPass::new(
            PassId(2), &self.ctx,
        )?);
        self.graph.register_pass(fur)?;

        let crystal_vfx = Box::new(crystal_vfx::CrystalVFXPass::new(
            PassId(3), &self.ctx,
        )?);
        self.graph.register_pass(crystal_vfx)?;

        let particle = Box::new(particle::ParticlePass::new(
            PassId(4), &self.ctx, self.config.max_particles,
        )?);
        self.graph.register_pass(particle)?;

        let workspace_panel = Box::new(workspace_panel::WorkspacePanelPass::new(
            PassId(5), &self.ctx,
        )?);
        self.graph.register_pass(workspace_panel)?;

        // TODO: Bloom pass not yet implemented — bloom.rs is a stub.
        // let bloom = Box::new(bloom::BloomPass::new(
        //     PassId(6), &self.ctx,
        // )?);
        // self.graph.register_pass(bloom)?;

        let postprocess = Box::new(postprocess::PostProcessPass::new(
            PassId(7), &self.ctx,
        )?);
        self.graph.register_pass(postprocess)?;

        let final_composite = Box::new(final_composite::FinalCompositePass::new(
            PassId(8), &self.ctx,
        )?);
        self.graph.register_pass(final_composite)?;

        let shadow = Box::new(shadow::ShadowPass::new(
            PassId(9), &self.ctx,
        )?);
        self.graph.register_pass(shadow)?;

        self.initialized = true;
        Ok(())
    }

    /// Begin a new frame — acquire the surface texture and prepare the frame context.
    ///
    /// This is step 1 of the frame pipeline.
    ///
    /// # Panics
    /// Panics if `begin_frame()` is called twice without an intervening `end_frame()`.
    /// This indicates a bug in the render loop — each frame must begin and end in order.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    ///
    /// # Errors
    /// Returns `RenderError::SurfaceOutdated` if the window was resized.
    pub fn begin_frame(&mut self) -> Result<(), RenderError> {
        // Defensive: detect double-begin (would leak a surface texture).
        assert!(
            !self.frame_in_flight,
            "begin_frame() called twice without end_frame() — frame {} is still in flight",
            self.frame_scheduler.frame_index(),
        );

        // Acquire the surface texture (stored for later presentation).
        let surface_texture = self.compositor.acquire_surface_texture(&self.ctx)?;

        // Begin the frame in the scheduler.
        let _frame_ctx = self.frame_scheduler.begin_frame(&self.ctx, &surface_texture)?;

        // Store the surface texture for the final composite pass.
        self.surface_texture = Some(surface_texture);
        self.frame_in_flight = true;

        Ok(())
    }

    /// Upload per-frame uniforms to the GPU (camera, lighting, bone matrices).
    ///
    /// This is step 2 of the frame pipeline.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    pub fn upload_uniforms(&mut self, scene: &Scene) {
        let camera_ubo = scene.build_camera_ubo();
        let lighting_ubo = scene.build_lighting_ubo();

        self.frame_scheduler.upload_uniforms(
            &self.ctx.queue,
            &camera_ubo,
            &lighting_ubo,
            &scene.bone_matrices,
        );
    }

    /// Recreate per-frame bind groups from the current frame scheduler slot.
    fn recreate_bind_groups(&mut self) {
        // Camera bind group (group 0).
        if let Some(buf) = self.frame_scheduler.get_camera_buffer() {
            let bg = self.ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("renderer_camera_bg"),
                layout: &self.camera_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: buf,
                        offset: 0,
                        size: None,
                    }),
                }],
            });
            self.camera_bind_group = Some(bg);
        }

        // Bone matrix bind group (group 1).
        if let Some(buf) = self.frame_scheduler.get_bone_matrix_buffer() {
            let bg = self.ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("renderer_bone_bg"),
                layout: &self.bone_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: buf,
                        offset: 0,
                        size: None,
                    }),
                }],
            });
            self.bone_matrix_bind_group = Some(bg);
        }

        // Lighting bind group (group 2).
        if let Some(buf) = self.frame_scheduler.get_lighting_buffer() {
            let bg = self.ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("renderer_lighting_bg"),
                layout: &self.lighting_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: buf,
                        offset: 0,
                        size: None,
                    }),
                }],
            });
            self.lighting_bind_group = Some(bg);
        }
    }

    /// Build the per-frame resolved resources from the current state.
    fn build_resolved_resources(&self) -> ResolvedResources {
        let surface_view = self.surface_texture.as_ref().map(|st| {
            st.texture.create_view(&wgpu::TextureViewDescriptor::default())
        });

        ResolvedResources {
            depth_texture: self.framebuffers.depth_view.clone(),
            color_texture: self.framebuffers.color_hdr_view.clone(),
            bloom_source: self.framebuffers.bloom_source_view.clone(),
            output_texture: self.framebuffers.ldr_output_view.clone(),
            surface_texture: surface_view,
            character_vertex_buffer: self.scene.mesh_vertex_buffer.clone(),
            character_index_buffer: self.scene.mesh_index_buffer.clone(),
            character_index_count: self.scene.mesh_index_count,
            camera_bind_group: self.camera_bind_group.clone(),
            bone_matrix_buffer: self.frame_scheduler.get_bone_matrix_buffer().cloned(),
            bone_matrix_bind_group: self.bone_matrix_bind_group.clone(),
            lighting_bind_group: self.lighting_bind_group.clone(),
            material_bind_group: None, // Set by the material system per-draw
            shadow_instance: Some(self.scene.shadow),
        }
    }

    /// Compile the render graph — cull inactive passes, resolve execution order.
    ///
    /// This is step 3 of the frame pipeline.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    ///
    /// # Errors
    /// Returns `RenderError::GraphCompilationFailed` if a cycle is detected.
    pub fn compile_graph(&mut self, frame_ctx: &FrameContext) -> Result<(), RenderError> {
        self.graph.compile(frame_ctx)
    }

    /// Execute all active render passes.
    ///
    /// This is step 4 of the frame pipeline. Builds resolved resources from
    /// current frame state and runs all passes through the graph.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    ///
    /// # Errors
    /// Returns `RenderError::RenderPassFailed` if any pass fails.
    pub fn execute_graph(&mut self, frame_ctx: &FrameContext) -> Result<(), RenderError> {
        // Recreate per-frame bind groups.
        self.recreate_bind_groups();

        // Build resolved resources from current frame state.
        let resources = self.build_resolved_resources();

        // Create a single command encoder for the entire frame.
        let mut encoder = self.ctx.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("Lumas Frame Encoder"),
            },
        );

        // Execute the render graph with all resources.
        self.graph.execute(&mut encoder, &resources, frame_ctx, &self.ctx)?;

        // Schedule overlay readback for hit-test mask.
        self.overlay.schedule_readback(&mut encoder, &self.compositor);

        // Submit the frame's command buffer.
        self.ctx.queue.submit(Some(encoder.finish()));

        // Update overlay mask from the composited output.
        self.overlay.update_mask(&self.ctx, &self.compositor)?;

        Ok(())
    }

    /// End the frame — present to surface, advance frame counter, collect metrics.
    ///
    /// This is step 5 of the frame pipeline.
    ///
    /// # Panics
    /// Panics if `end_frame()` is called without a matching `begin_frame()`.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    pub fn end_frame(&mut self) {
        // Defensive: detect end_frame without begin_frame.
        assert!(
            self.frame_in_flight,
            "end_frame() called without begin_frame() — frame {} has no in-flight frame",
            self.frame_scheduler.frame_index(),
        );

        // Present the surface texture (blocks until vsync if Fifo mode).
        if let Some(surface_texture) = self.surface_texture.take() {
            surface_texture.present();
        }

        // Mark the current slot as submitted.
        self.frame_scheduler.mark_submitted();

        // Advance the frame counter.
        self.frame_scheduler.end_frame();

        // End the resource pool frame (process deferred deletions).
        self.resource_pool.end_frame();

        // End the texture manager frame (recall staging belt).
        self.texture_manager.end_frame();

        // Update statistics.
        self.stats.frames_rendered += 1;

        // Clear in-flight guard.
        self.frame_in_flight = false;
    }

    /// Execute a complete frame pipeline in one call.
    ///
    /// Combines `begin_frame`, `upload_uniforms`, `compile_graph`,
    /// `execute_graph`, and `end_frame`.
    ///
    /// This method wraps the pipeline with `MetricsCollector` timing:
    /// - `metrics.begin_frame()` is called before `begin_frame()`
    /// - `metrics.end_frame()` is called after `end_frame()`
    /// - Per-pass timing is built from the active compiled passes
    /// - Draw calls are recorded on each pass execution
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    ///
    /// # Errors
    /// Returns any error from the pipeline stages.
    pub fn render_frame(&mut self) -> Result<(), RenderError> {
        // Step 0: Start metrics timing for this frame.
        self.metrics.begin_frame();

        // Step 1: Begin frame — acquire surface texture.
        self.begin_frame()?;

        // Step 2: Upload per-frame uniforms.
        let scene = self.scene.clone();
        self.upload_uniforms(&scene);

        // Build the frame context from current scene state.
        let frame_ctx = FrameContext {
            frame_index: self.frame_scheduler.frame_index(),
            delta_time: self.frame_scheduler.delta_time(),
            total_time: self.frame_scheduler.frame_index() as f32 * self.frame_scheduler.delta_time(),
            surface_width: self.framebuffers.width,
            surface_height: self.framebuffers.height,
            focus_mode: self.scene.focus_mode,
            sleeping: self.scene.sleeping,
            active_particles: self.scene.particles.active_count,
            active_panels: self.scene.panels.len() as u32,
            fur_shell_count: self.scene.fur_shell_count(&self.config),
            lod_level: self.scene.lod_level,
            bloom_has_content: self.scene.has_active_particles(),
        };

        // Step 3: Compile the graph (culls inactive passes).
        self.compile_graph(&frame_ctx)?;

        // Step 4: Execute all active passes.
        self.execute_graph(&frame_ctx)?;

        // Step 5: End frame — present, advance counters.
        self.end_frame();

        // Build per-pass timing from the graph's active passes.
        let pass_timing: Vec<PassTiming> = self
            .graph
            .topology()
            .map(|compiled| {
                let budgets = FrameBudgets::from_config(&self.config);
                compiled
                    .execution_order
                    .iter()
                    .map(|pass_id| {
                        // Pass names are hard-coded; in a full implementation
                        // they'd come from the pass itself. GPU durations will
                        // come from timestamp queries when enabled.
                        let name = match pass_id.0 {
                            0 => "depth_prepass",
                            1 => "geometry",
                            2 => "fur",
                            3 => "crystal_vfx",
                            4 => "particles",
                            5 => "workspace_panel",
                            6 => "bloom",
                            7 => "postprocess",
                            8 => "final_composite",
                            9 => "shadow",
                            _ => "unknown",
                        };
                        let budget_us = budgets.for_pass(name);
                        PassTiming {
                            name,
                            gpu_duration_us: 0, // Populated by timestamp queries
                            budget_us,
                            budget_exceeded: false,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Step 6: End metrics — compute frame timing and update health.
        self.metrics.end_frame(0, pass_timing); // GPU duration from timestamp queries

        Ok(())
    }

    /// Resize the renderer's output surface and all intermediate framebuffers.
    ///
    /// Must be called when the window resizes or the DPI scale changes.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    pub fn resize(&mut self, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);

        // Reconfigure the GPU surface.
        self.ctx.resize(w, h);

        // Reallocate all intermediate framebuffers.
        self.framebuffers.allocate(&self.ctx.device, w, h);

        // Resize compositor and overlay.
        self.compositor.resize(&self.ctx, w, h);
        self.overlay.resize(w, h);
        self.scene.camera.resize(w, h);
    }

    // ── Accessors ──

    /// Set the scene to render.
    pub fn set_scene(&mut self, scene: Scene) {
        self.scene = scene;
    }

    /// Get a mutable reference to the scene.
    pub fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }

    /// Get an immutable reference to the scene.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Register an additional render pass in the graph.
    pub fn register_pass(&mut self, pass: Box<dyn RenderPass>) -> Result<(), RenderError> {
        self.graph.register_pass(pass)?;
        Ok(())
    }

    /// Get a reference to the GPU context.
    pub fn ctx(&self) -> &GpuContext {
        &self.ctx
    }

    /// Get a mutable reference to the GPU context.
    pub fn ctx_mut(&mut self) -> &mut GpuContext {
        &mut self.ctx
    }

    /// Get a reference to the frame scheduler.
    pub fn frame_scheduler(&self) -> &FrameScheduler {
        &self.frame_scheduler
    }

    /// Get a reference to the render graph.
    pub fn graph(&self) -> &RenderGraph {
        &self.graph
    }

    /// Get a reference to the compositor.
    pub fn compositor(&self) -> &Compositor {
        &self.compositor
    }

    /// Get a reference to the overlay renderer.
    pub fn overlay(&self) -> &OverlayRenderer {
        &self.overlay
    }

    /// Get a reference to the shader manager.
    pub fn shader_manager(&self) -> &ShaderManager {
        &self.shader_manager
    }

    /// Get a reference to the texture manager.
    pub fn texture_manager(&self) -> &TextureManager {
        &self.texture_manager
    }

    /// Get a reference to the resource pool.
    pub fn resource_pool(&self) -> &ResourcePool {
        &self.resource_pool
    }

    /// Get a reference to the render statistics.
    pub fn stats(&self) -> &RenderStatistics {
        &self.stats
    }

    /// Get the current frame index.
    pub fn frame_index(&self) -> u64 {
        self.frame_scheduler.frame_index()
    }

    /// Get a reference to the metrics collector.
    pub fn metrics(&self) -> &MetricsCollector {
        &self.metrics
    }

    /// Get a mutable reference to the metrics collector.
    pub fn metrics_mut(&mut self) -> &mut MetricsCollector {
        &mut self.metrics
    }

    /// Get a reference to the framebuffer set.
    pub fn framebuffers(&self) -> &FramebufferSet {
        &self.framebuffers
    }

    /// Get the current configuration.
    pub fn config(&self) -> &RenderConfig {
        &self.config
    }

    /// Set the vsync mode.
    pub fn set_vsync_mode(&mut self, mode: VsyncMode) {
        self.frame_scheduler.set_vsync_mode(mode);
        match mode {
            VsyncMode::Fifo => {
                self.ctx.set_present_mode(wgpu::PresentMode::Fifo);
            }
            VsyncMode::Mailbox => {
                self.ctx.set_present_mode(wgpu::PresentMode::Mailbox);
            }
            VsyncMode::Immediate | VsyncMode::Adaptive => {
                self.ctx.set_present_mode(wgpu::PresentMode::Immediate);
            }
        }
    }

    /// Set the target FPS.
    pub fn set_target_fps(&mut self, fps: f32) {
        self.frame_scheduler.set_target_fps(fps);
    }

    /// Check if the renderer is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("frame", &self.frame_index())
            .field("passes", &self.graph.pass_count())
            .field("active_passes", &self.graph.active_pass_count())
            .field("health", &self.metrics.health())
            .field("initialized", &self.initialized)
            .field("framebuffers", &self.framebuffers)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_statistics_default() {
        let stats = RenderStatistics::default();
        assert_eq!(stats.frames_rendered, 0);
        assert_eq!(stats.consecutive_over_budget, 0);
        assert!(stats.last_frame_on_budget);
    }

    #[test]
    fn test_max_bones_constant() {
        assert_eq!(MAX_BONES, 96);
    }

    #[test]
    fn test_framebuffer_set_creation() {
        // Without a GPU device we can't create real textures,
        // but we can verify the struct layout and default state.
        let fb = FramebufferSet {
            depth: None,
            depth_view: None,
            color_hdr: None,
            color_hdr_view: None,
            bloom_source: None,
            bloom_source_view: None,
            ldr_output: None,
            ldr_output_view: None,
            width: 1920,
            height: 1080,
        };
        assert_eq!(fb.width, 1920);
        assert_eq!(fb.height, 1080);
        assert!(fb.depth.is_none());
        assert!(fb.color_hdr.is_none());
    }

    #[test]
    fn test_scene_swap() {
        let scene1 = Scene::new();
        let scene2 = Scene::new();
        assert_eq!(scene1.character_transform, scene2.character_transform);
    }
}
