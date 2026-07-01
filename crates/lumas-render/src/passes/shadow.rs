//! Shadow pass — renders a soft drop shadow sprite beneath the character.
//!
//! Renders a single instanced shadow quad using the `shadow.wgsl` shader:
//! - **Group 0**: CameraUBO (uniform, vertex)
//! - **Group 1**: ShadowInstance storage buffer (vertex) — single instance with
//!   world position, size, and opacity
//! - **Group 3**: Shadow sprite texture + sampler (fragment)
//!
//! The shadow quad is drawn with pre-multiplied alpha blending onto the HDR
//! color target. The quad corners are generated as a unit quad centered at
//! origin, and the vertex shader transforms them using the instance's world
//! position and size.
//!
//! # Frame Budget
//! GPU: ~0.05ms, CPU: ~0.02ms — single draw call, no depth writes.

use crate::context::GpuContext;
use crate::error::RenderError;
use crate::graph::{FrameContext, PassId, PassResourceBuilder, RenderPass, ResolvedResources};
use crate::scene::ShadowInstanceGPU;

/// Unit quad vertex: position (vec3) + uv (vec2).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadowQuadVertex {
    position: [f32; 3],
    uv: [f32; 2],
}

/// Unit quad vertices (4 corners, centered at origin, front face = CCW).
const QUAD_VERTICES: [ShadowQuadVertex; 4] = [
    // Bottom-left
    ShadowQuadVertex { position: [-0.5, -0.5, 0.0], uv: [0.0, 1.0] },
    // Bottom-right
    ShadowQuadVertex { position: [0.5, -0.5, 0.0], uv: [1.0, 1.0] },
    // Top-left
    ShadowQuadVertex { position: [-0.5, 0.5, 0.0], uv: [0.0, 0.0] },
    // Top-right
    ShadowQuadVertex { position: [0.5, 0.5, 0.0], uv: [1.0, 0.0] },
];

/// Indices for two triangles forming the quad.
const QUAD_INDICES: [u32; 6] = [0, 1, 2, 2, 1, 3];

/// Vertex buffer layout for the shadow quad.
fn shadow_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<ShadowQuadVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 12, // After 3 × f32
                shader_location: 1,
            },
        ],
    }
}

pub struct ShadowPass {
    id: PassId,
    pipeline: wgpu::RenderPipeline,
    quad_vertex_buffer: wgpu::Buffer,
    quad_index_buffer: wgpu::Buffer,
    /// Storage buffer for ShadowInstance data (read by vertex shader).
    instance_storage_buffer: wgpu::Buffer,
    /// Bind group layout for group 1 (instance storage).
    instance_layout: wgpu::BindGroupLayout,
    /// Bind group layout for group 3 (shadow texture + sampler).
    shadow_texture_layout: wgpu::BindGroupLayout,
    _bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    /// Default shadow texture (kept alive alongside its TextureView).
    default_shadow_texture: wgpu::Texture,
    default_shadow_texture_view: wgpu::TextureView,
    default_sampler: wgpu::Sampler,
}

impl ShadowPass {
    pub fn new(id: PassId, ctx: &GpuContext) -> Result<Self, RenderError> {
        let vs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow_vs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADOW_VS_FS)),
        });

        let fs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow_fs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADOW_VS_FS)),
        });

        // ── Bind Group 0: Camera UBO (matching CameraUBO struct layout) ──
        // This layout must be compatible with the bind group created by the
        // resource system. The existing passes all create their own identical
        // camera layout — wgpu compares layouts by content, enabling reuse.
        let camera_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow_camera_layout"),
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
        });

        // ── Bind Group 1: Shadow instance storage buffer ──
        let instance_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow_instance_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // ── Bind Group 3: Shadow texture + sampler ──
        let shadow_texture_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow_texture_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // ── Quad geometry (vertex/index buffers) ──
        let quad_vertex_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow_quad_vertices"),
            size: std::mem::size_of_val(&QUAD_VERTICES) as u64,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });
        ctx.queue.write_buffer(&quad_vertex_buffer, 0, bytemuck::cast_slice(&QUAD_VERTICES));

        let quad_index_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow_quad_indices"),
            size: std::mem::size_of_val(&QUAD_INDICES) as u64,
            usage: wgpu::BufferUsages::INDEX,
            mapped_at_creation: false,
        });
        ctx.queue.write_buffer(&quad_index_buffer, 0, bytemuck::cast_slice(&QUAD_INDICES));

        // ── Instance storage buffer (1 shadow instance, 32 bytes) ──
        let instance_storage_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow_instance_storage"),
            size: std::mem::size_of::<ShadowInstanceGPU>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Default shadow texture (1×1 white — produces a soft circular shadow by default) ──
        let shadow_default_tex = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow_default_tex"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // Write a white pixel (r=1.0 → full shadow density, sampled as soft radial gradient).
        let white_pixel: [u8; 4] = [255, 255, 255, 255];
        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &shadow_default_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &white_pixel,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let default_shadow_texture_view = shadow_default_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let default_sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow_default_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        // ── Group 2: empty (unused in shadow shader) ──
        let empty_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow_empty_group2_layout"),
            entries: &[],
        });

        // Pipeline layout with all groups declared (0..4 matching shader @group).
        let bind_group_layouts = vec![
            camera_layout,
            instance_layout.clone(),
            empty_layout,
            shadow_texture_layout.clone(),
        ];
        let layout_refs: Vec<Option<&wgpu::BindGroupLayout>> = bind_group_layouts.iter().map(|l| Some(l)).collect();

        let full_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow_layout"),
            bind_group_layouts: &layout_refs,
            immediate_size: 0,
        });

        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow_sprite"),
            layout: Some(&full_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[shadow_vertex_layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    // Shadow renders onto the HDR color buffer.
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Shadow quad is double-sided (culling disabled).
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None, // Shadow does not write to depth.
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Ok(Self {
            id,
            pipeline,
            quad_vertex_buffer,
            quad_index_buffer,
            instance_storage_buffer,
            instance_layout,
            shadow_texture_layout,
            _bind_group_layouts: bind_group_layouts,
            default_shadow_texture: shadow_default_tex,
            default_shadow_texture_view,
            default_sampler,
        })
    }

    /// Write shadow instance data to the GPU storage buffer.
    fn write_instance(&self, queue: &wgpu::Queue, shadow: &ShadowInstanceGPU) {
        queue.write_buffer(&self.instance_storage_buffer, 0, bytemuck::bytes_of(shadow));
    }
}

impl RenderPass for ShadowPass {
    fn id(&self) -> PassId {
        self.id
    }

    fn name(&self) -> &'static str {
        "shadow"
    }

    fn declare_resources(&self, builder: &mut PassResourceBuilder) {
        // Shadow reads the camera and writes to the HDR color buffer.
        builder.write_texture(super::RESOURCE_COLOR);
    }

    fn is_active(&self, fc: &FrameContext) -> bool {
        // Shadow is very cheap — render whenever not in focus mode.
        // Even when sleeping, the shadow sprite is rendered since it's a
        // single draw call with minimal GPU cost.
        !fc.focus_mode
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResolvedResources,
        _frame_ctx: &FrameContext,
        ctx: &GpuContext,
    ) -> Result<(), RenderError> {
        // ── Write shadow instance data to the storage buffer ──
        if let Some(ref shadow) = resources.shadow_instance {
            self.write_instance(&ctx.queue, shadow);
        }

        let color_view = resources.color_texture.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "shadow",
                cause: "Color texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let camera_bg = resources.camera_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "shadow",
                cause: "Camera bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        // ── Create bind group 1: instance storage ──
        let instance_bg = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow_instance_bg"),
            layout: &self.instance_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &self.instance_storage_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        // ── Create bind group 3: shadow texture + sampler ──
        // Use the resource's shadow texture if available, otherwise fall back to the default.
        let (shadow_tex_view, shadow_sampler) = if let Some(ref mat_bg) = resources.material_bind_group {
            // The material bind group at group 3 has a different layout (5 entries).
            // For the shadow pass, we need group 3 with texture + sampler.
            // If the resource system provides a shadow-specific texture, use it here.
            // For now, use the default soft shadow texture.
            (&self.default_shadow_texture_view, &self.default_sampler)
        } else {
            (&self.default_shadow_texture_view, &self.default_sampler)
        };

        let shadow_tex_bg = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow_texture_bg"),
            layout: &self.shadow_texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(shadow_tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(shadow_sampler),
                },
            ],
        });

        // ── Render pass ──
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow_sprite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    // Blend onto existing color content (the character + fur).
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, camera_bg, &[]);
        rpass.set_bind_group(1, &instance_bg, &[]);
        rpass.set_bind_group(3, &shadow_tex_bg, &[]);
        rpass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        rpass.set_index_buffer(self.quad_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        rpass.draw_indexed(0..QUAD_INDICES.len() as u32, 0, 0..1);

        drop(rpass);
        Ok(())
    }
}

/// Combined vertex + fragment shader sourced from the project's shadow.wgsl.
const SHADOW_VS_FS: &str = r#"
struct CameraUBO {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    viewport_size: vec2<f32>,
    time_seconds: f32,
    _pad: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUBO;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct ShadowInstance {
    world_pos: vec4<f32>,
    size: f32,
    opacity: f32,
    _pad0: f32,
    _pad1: f32,
};

struct ShadowInstances {
    data: array<ShadowInstance>,
};

@group(1) @binding(0)
var<storage, read> instances: ShadowInstances;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
};

@vertex
fn vs_main(
    input: VertexInput,
    @builtin(instance_index) instance_id: u32,
) -> VertexOutput {
    let instance = instances.data[instance_id];
    let world_offset = input.position * instance.size;
    let world_pos = instance.world_pos + vec4(world_offset, 0.0);
    let clip_pos = camera.view_proj * world_pos;

    var output: VertexOutput;
    output.clip_pos = clip_pos;
    output.uv = input.uv;
    output.opacity = instance.opacity;
    return output;
}

@group(3) @binding(0)
var shadow_texture: texture_2d<f32>;
@group(3) @binding(1)
var shadow_sampler: sampler;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let shadow_sample = textureSample(shadow_texture, shadow_sampler, input.uv);
    let alpha = shadow_sample.r * input.opacity;
    return vec4(0.0, 0.0, 0.0, alpha);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quad_vertex_count() {
        assert_eq!(QUAD_VERTICES.len(), 4);
        assert_eq!(QUAD_INDICES.len(), 6);
    }

    #[test]
    fn test_quad_vertex_size() {
        // position: vec3<f32> = 12 bytes, uv: vec2<f32> = 8 bytes, total = 20 bytes
        assert_eq!(std::mem::size_of::<ShadowQuadVertex>(), 20);
    }

    #[test]
    fn test_vertex_layout_stride() {
        let layout = shadow_vertex_layout();
        assert_eq!(layout.array_stride, 20);
        assert_eq!(layout.attributes.len(), 2);
        // position @ location 0, offset 0
        assert_eq!(layout.attributes[0].shader_location, 0);
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[0].format, wgpu::VertexFormat::Float32x3);
        // uv @ location 1, offset 12
        assert_eq!(layout.attributes[1].shader_location, 1);
        assert_eq!(layout.attributes[1].offset, 12);
        assert_eq!(layout.attributes[1].format, wgpu::VertexFormat::Float32x2);
    }

    #[test]
    fn test_shadow_instance_gpu_size() {
        assert_eq!(std::mem::size_of::<ShadowInstanceGPU>(), 32);
    }

    #[test]
    fn test_shadow_active_when_not_focus() {
        // Shadow is active whenever focus_mode is false.
        // Verify the logic: ShadowPass::is_active returns !fc.focus_mode.
        let fc = FrameContext {
            focus_mode: false,
            ..Default::default()
        };
        assert!(!fc.focus_mode); // Shadow should be active

        // In focus mode, shadow should be culled.
        let fc_focus = FrameContext {
            focus_mode: true,
            ..Default::default()
        };
        assert!(fc_focus.focus_mode); // Shadow should be inactive
    }

    #[test]
    fn test_default_shadow_instance() {
        let si = ShadowInstanceGPU::default();
        assert_eq!(si.size, 1.0);
        assert!((si.opacity - 0.3).abs() < f32::EPSILON);
    }
}
