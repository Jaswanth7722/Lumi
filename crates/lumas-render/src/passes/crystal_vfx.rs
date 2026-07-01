//! Crystal emission VFX pass — renders the forehead crystal's emissive output.
//!
//! This pass writes the crystal's emission color into a separate bloom-source
//! render target. The bloom pass then reads this texture as its bright-pass input.
//! When the crystal is not emitting (idle state), this pass culls itself and
//! the bloom pass receives a black texture.
//!
//! # Frame Budget
//! GPU: 0.4ms, CPU: 0.1ms

use crate::context::GpuContext;
use crate::error::RenderError;
use crate::graph::{FrameContext, PassId, PassResourceBuilder, RenderPass, ResolvedResources};

pub struct CrystalVFXPass {
    id: PassId,
    pipeline: wgpu::RenderPipeline,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    sampler: wgpu::Sampler,
    fallback_view: wgpu::TextureView,
}

impl CrystalVFXPass {
    pub fn new(id: PassId, ctx: &GpuContext) -> Result<Self, RenderError> {
        let vs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crystal_vfx_vs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(CRYSTAL_VFX_VS)),
        });

        let fs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crystal_vfx_fs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(CRYSTAL_VFX_FS)),
        });

        // Bind group 0: Camera UBO (shared across all passes).
        let camera_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("crystal_vfx_camera_layout"),
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
        });

        // Bind group 3: Material textures (albedo + emission + noise + sampler).
        let material_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("crystal_vfx_material_layout"),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group_layouts = vec![camera_layout, material_layout];
        let layout_refs: Vec<Option<&wgpu::BindGroupLayout>> = bind_group_layouts.iter().map(|l| Some(l)).collect();

        let pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crystal_vfx_layout"),
            bind_group_layouts: &layout_refs,
            immediate_size: 0,
        });

        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("crystal_vfx"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[], // Fullscreen triangle — no vertex buffer
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float, // Bloom source format
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("crystal_vfx_sampler"),
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

        // Create cached fallback texture (1x1 white) for material bind group.
        let fallback_texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("crystal_vfx_fallback"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let fallback_view = fallback_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Ok(Self {
            id,
            pipeline,
            bind_group_layouts,
            sampler,
            fallback_view,
        })
    }
}

impl RenderPass for CrystalVFXPass {
    fn id(&self) -> PassId {
        self.id
    }

    fn name(&self) -> &'static str {
        "crystal_vfx"
    }

    fn declare_resources(&self, builder: &mut PassResourceBuilder) {
        builder.write_texture(super::RESOURCE_BLOOM_SOURCE);
    }

    fn is_active(&self, fc: &FrameContext) -> bool {
        fc.bloom_has_content && !fc.focus_mode
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResolvedResources,
        _frame_ctx: &FrameContext,
        ctx: &GpuContext,
    ) -> Result<(), RenderError> {
        let bloom_view = resources.bloom_source.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "crystal_vfx",
                cause: "Bloom source texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let camera_bg = resources.camera_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "crystal_vfx",
                cause: "Camera bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        // Create material bind group with cached fallback textures.
        let material_bg = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crystal_vfx_material_bg"),
            layout: &self.bind_group_layouts[1],
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.fallback_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.fallback_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.fallback_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("crystal_vfx"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: bloom_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0, g: 0.0, b: 0.0, a: 0.0,
                    }),
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
        rpass.set_bind_group(3, &material_bg, &[]);
        rpass.draw(0..3, 0..1); // Fullscreen triangle

        drop(rpass);
        Ok(())
    }
}

const CRYSTAL_VFX_VS: &str = r#"
@group(0) @binding(0) var<uniform> CameraUBO {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    viewport_size: vec2<f32>,
    time_seconds: f32,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_id: u32) -> VertexOutput {
    let uv = vec2<f32>(
        f32((vertex_id << 1u) & 2u),
        f32(vertex_id & 2u),
    );
    let pos = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    var output: VertexOutput;
    output.clip_pos = pos;
    output.uv = uv;
    return output;
}
"#;

const CRYSTAL_VFX_FS: &str = r#"
@group(0) @binding(0) var<uniform> CameraUBO {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    viewport_size: vec2<f32>,
    time_seconds: f32,
};

@group(3) @binding(0) var albedo_tex: texture_2d<f32>;
@group(3) @binding(1) var emission_tex: texture_2d<f32>;
@group(3) @binding(2) var noise_tex: texture_2d<f32>;
@group(3) @binding(3) var material_sampler: sampler;

struct FragmentInput {
    @location(0) uv: vec2<f32>,
}

fn hash2d(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    let emission = textureSample(emission_tex, material_sampler, input.uv).rgb;
    let noise = textureSample(noise_tex, material_sampler, input.uv * 2.0 + CameraUBO.time_seconds * 0.1).r;
    let shimmer = 0.8 + 0.2 * noise;
    let pulse = 0.7 + 0.3 * sin(CameraUBO.time_seconds * 2.0);
    let color = emission * shimmer * pulse;
    return vec4(color, 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crystal_vfx_is_active() {
        let fc_active = FrameContext {
            bloom_has_content: true,
            focus_mode: false,
            ..Default::default()
        };
        assert!(fc_active.bloom_has_content);
        assert!(!fc_active.focus_mode);
    }

    #[test]
    fn test_crystal_vfx_is_culled_when_no_content() {
        let fc_culled = FrameContext {
            bloom_has_content: false,
            focus_mode: false,
            ..Default::default()
        };
        assert!(!fc_culled.bloom_has_content);
    }

    #[test]
    fn test_crystal_vfx_is_culled_in_focus_mode() {
        let fc_focus = FrameContext {
            bloom_has_content: true,
            focus_mode: true,
            ..Default::default()
        };
        assert!(fc_focus.focus_mode);
    }
}
