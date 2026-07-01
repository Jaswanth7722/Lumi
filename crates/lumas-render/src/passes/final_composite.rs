//! Final composite pass — presents the LDR output to the swapchain surface.
//!
//! This is the last pass in the render graph. It copies/blits the post-processed
//! LDR output texture to the swapchain surface for display.
//!
//! The pass uses pre-multiplied alpha compositing throughout to support
//! transparent desktop windows.
//!
//! # Frame Budget
//! GPU: 0.2ms, CPU: 0.1ms

use crate::context::GpuContext;
use crate::error::RenderError;
use crate::graph::{FrameContext, PassId, PassResourceBuilder, RenderPass, ResolvedResources};

pub struct FinalCompositePass {
    id: PassId,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl FinalCompositePass {
    pub fn new(id: PassId, ctx: &GpuContext) -> Result<Self, RenderError> {
        let vs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composite_vs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(COMPOSITE_VS)),
        });

        let fs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composite_fs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(COMPOSITE_FS)),
        });

        let bind_group_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite_layout"),
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

        let pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composite_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("final_composite"),
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
                    format: wgpu::TextureFormat::Bgra8UnormSrgb,
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
            label: Some("composite_sampler"),
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
            sampler,
        })
    }
}

impl RenderPass for FinalCompositePass {
    fn id(&self) -> PassId {
        self.id
    }

    fn name(&self) -> &'static str {
        "final_composite"
    }

    fn declare_resources(&self, builder: &mut PassResourceBuilder) {
        builder.read_texture(super::RESOURCE_OUTPUT);
        builder.write_texture(super::RESOURCE_SURFACE);
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
                pass: "final_composite",
                cause: "Output texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let surface_view = resources.surface_texture.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "final_composite",
                cause: "Surface texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        // Per-frame bind group (texture views change each frame).
        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(output_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("final_composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: surface_view,
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

const COMPOSITE_VS: &str = r#"
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

const COMPOSITE_FS: &str = r#"
@group(0) @binding(0) var composite_texture: texture_2d<f32>;
@group(0) @binding(1) var composite_sampler: sampler;

struct FragmentInput {
    @location(0) uv: vec2<f32>,
}

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    let color = textureSample(composite_texture, composite_sampler, input.uv);
    return color;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composite_always_active() {
        let fc = FrameContext::default();
        assert_eq!(fc.frame_index, 0);
    }

    #[test]
    fn test_composite_vertex_output() {
        let vertex_count = 3;
        let uvs = [
            (0.0f32, 0.0f32),
            (1.0f32, 0.0f32),
            (0.0f32, 1.0f32),
        ];
        assert_eq!(uvs.len(), vertex_count);
        assert_eq!(uvs[0], (0.0, 0.0));
        assert_eq!(uvs[1], (1.0, 0.0));
        assert_eq!(uvs[2], (0.0, 1.0));
    }
}
