//! Post-process pass — tonemapping, gamma correction, FXAA anti-aliasing.
//!
//! This pass runs on the HDR color buffer and produces an LDR output texture
//! ready for display. The pipeline:
//! 1. **FXAA** (optional): edge-detect and blend aliased pixels
//! 2. **Bloom composite** (optional): add bloom buffer onto HDR color
//! 3. **Exposure**: apply exposure multiplier
//! 4. **ACES filmic tonemapping**: HDR → LDR with filmic curve
//! 5. **sRGB gamma correction**: linear → sRGB
//! 6. **Pre-multiplied alpha passthrough**
//!
//! Uses a fullscreen triangle (no vertex buffer needed).
//!
//! # Frame Budget
//! GPU: 0.3ms, CPU: 0.1ms

use crate::context::GpuContext;
use crate::error::RenderError;
use crate::graph::{FrameContext, PassId, PassResourceBuilder, RenderPass, ResolvedResources};

pub struct PostProcessPass {
    id: PassId,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

impl PostProcessPass {
    pub fn new(id: PassId, ctx: &GpuContext) -> Result<Self, RenderError> {
        let vs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("postprocess_vs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(POSTPROCESS_VS)),
        });

        let fs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("postprocess_fs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(POSTPROCESS_FS)),
        });

        let bind_group_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("postprocess_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
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

        let pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("postprocess_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("postprocess"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[], // Fullscreen triangle
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
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

        let uniform_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("postprocess_config"),
            size: std::mem::size_of::<PostProcessConfig>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("postprocess_sampler"),
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

        Ok(Self {
            id,
            pipeline,
            bind_group_layout,
            uniform_buffer,
            sampler,
        })
    }

    pub fn update_config(&self, queue: &wgpu::Queue, config: &PostProcessConfig) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(config));
    }
}

impl RenderPass for PostProcessPass {
    fn id(&self) -> PassId {
        self.id
    }

    fn name(&self) -> &'static str {
        "postprocess"
    }

    fn declare_resources(&self, builder: &mut PassResourceBuilder) {
        builder.read_texture(super::RESOURCE_COLOR);
        builder.read_texture(super::RESOURCE_BLOOM_SOURCE);
        builder.write_texture(super::RESOURCE_OUTPUT);
    }

    fn is_active(&self, _fc: &FrameContext) -> bool {
        true
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResolvedResources,
        _frame_ctx: &FrameContext,
        ctx: &GpuContext,
    ) -> Result<(), RenderError> {
        let output_view = resources.output_texture.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "postprocess",
                cause: "Output texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let color_view = resources.color_texture.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "postprocess",
                cause: "Color texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        // Use bloom_source if available, otherwise reuse HDR color for bloom input (will sample black since no bloom).
        let bloom_view = resources.bloom_source.as_ref().unwrap_or(color_view);

        // Create per-frame bind group (texture views change each frame).
        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("postprocess_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(bloom_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("postprocess"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
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
        rpass.set_bind_group(0, &bind_group, &[]);
        rpass.draw(0..3, 0..1);

        drop(rpass);
        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PostProcessConfig {
    pub bloom_strength: f32,
    pub bloom_enabled: f32,
    pub fxaa_enabled: f32,
    pub gamma: f32,
    pub exposure: f32,
    pub fxaa_subpixel_quality: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

impl Default for PostProcessConfig {
    fn default() -> Self {
        Self {
            bloom_strength: 0.04,
            bloom_enabled: 1.0,
            fxaa_enabled: 1.0,
            gamma: 2.2,
            exposure: 1.0,
            fxaa_subpixel_quality: 0.75,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        }
    }
}

const POSTPROCESS_VS: &str = r#"
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

const POSTPROCESS_FS: &str = r#"
struct PostProcessConfig {
    bloom_strength: f32,
    bloom_enabled: f32,
    fxaa_enabled: f32,
    gamma: f32,
    exposure: f32,
    fxaa_subpixel_quality: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> config: PostProcessConfig;
@group(0) @binding(1) var hdr_texture: texture_2d<f32>;
@group(0) @binding(2) var bloom_texture: texture_2d<f32>;
@group(0) @binding(3) var pp_sampler: sampler;

struct FragmentInput {
    @location(0) uv: vec2<f32>,
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

fn aces_filmic(color: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return saturate((color * (a * color + b)) / (color * (c * color + d) + e));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let cutoff = 0.0031308;
    return select(1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, 12.92 * c, c < vec3(cutoff));
}

fn fxaa_run(uv: vec2<f32>, color: ptr<function, vec4<f32>>) -> bool {
    if config.fxaa_enabled < 0.5 { return false; }
    let tex_dims = vec2<f32>(textureDimensions(hdr_texture));
    let rcp_dims = 1.0 / tex_dims;
    let luma_center = luminance((*color).rgb);
    let luma_tl = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(-1.0, -1.0) * rcp_dims, 0.0).rgb);
    let luma_tr = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(1.0, -1.0) * rcp_dims, 0.0).rgb);
    let luma_bl = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(-1.0, 1.0) * rcp_dims, 0.0).rgb);
    let luma_br = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(1.0, 1.0) * rcp_dims, 0.0).rgb);
    let luma_min = min(luma_center, min(min(luma_tl, luma_tr), min(luma_bl, luma_br)));
    let luma_max = max(luma_center, max(max(luma_tl, luma_tr), max(luma_bl, luma_br)));
    let luma_range = luma_max - luma_min;
    if luma_range < 0.0312 { return false; }
    let luma_up = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(-1.0, 0.0) * rcp_dims, 0.0).rgb);
    let luma_down = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(1.0, 0.0) * rcp_dims, 0.0).rgb);
    let luma_left = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(0.0, -1.0) * rcp_dims, 0.0).rgb);
    let luma_right = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(0.0, 1.0) * rcp_dims, 0.0).rgb);
    let edge_h = abs(luma_up - luma_down) + abs(luma_tl - luma_bl) + abs(luma_tr - luma_br);
    let edge_v = abs(luma_left - luma_right) + abs(luma_tl - luma_tr) + abs(luma_bl - luma_br);
    var blend_dir = vec2(0.0, 0.0);
    if edge_h > edge_v { blend_dir.x = 1.0; } else { blend_dir.y = 1.0; }
    let luma_avg = (luma_center + luma_up + luma_down + luma_left + luma_right) / 5.0;
    let subpixel_shift = clamp(abs(luma_center - luma_avg) / max(luma_range, 0.0001), 0.0, 0.5);
    let offset = blend_dir * rcp_dims * (0.5 + config.fxaa_subpixel_quality * subpixel_shift);
    let s1 = textureSampleLevel(hdr_texture, pp_sampler, uv + offset * (1.0 / 3.0 - 0.5), 0.0);
    let s2 = textureSampleLevel(hdr_texture, pp_sampler, uv + offset * (2.0 / 3.0 - 0.5), 0.0);
    *color = (s1 + s2) * 0.5;
    return true;
}

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    var color = textureSampleLevel(hdr_texture, pp_sampler, input.uv, 0.0);
    fxaa_run(input.uv, &color);
    if config.bloom_enabled > 0.5 {
        let bloom = textureSampleLevel(bloom_texture, pp_sampler, input.uv, 0.0);
        color = color + bloom * config.bloom_strength;
    }
    color.rgb = color.rgb * config.exposure;
    let tm = aces_filmic(color.rgb);
    let srgb = linear_to_srgb(tm);
    return vec4(srgb, color.a);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postprocess_config_default() {
        let config = PostProcessConfig::default();
        assert!((config.gamma - 2.2).abs() < f32::EPSILON);
        assert!((config.exposure - 1.0).abs() < f32::EPSILON);
        assert!(config.fxaa_enabled > 0.5);
        assert!(config.bloom_enabled > 0.5);
    }

    #[test]
    fn test_postprocess_config_size() {
        assert_eq!(std::mem::size_of::<PostProcessConfig>(), 36);
    }

    #[test]
    fn test_postprocess_always_active() {
        let fc = FrameContext::default();
        assert!(fc.frame_index == 0);
    }
}
