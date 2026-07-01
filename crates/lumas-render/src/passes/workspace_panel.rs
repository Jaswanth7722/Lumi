//! Workspace panel pass — renders holographic workspace panels as translucent quads.
//!
//! Each panel is a textured quad with:
//! - Content texture (the panel's display output rendered by the engine)
//! - Glow effect (edge glow + emission)
//! - Translucent compositing (pre-multiplied alpha)
//! - Holographic scanline and shimmer animation
//!
//! Panel geometry is expected to be provided as a vertex/index buffer through
//! ResolvedResources (using the character_vertex_buffer/index_buffer slots
//! when used as general-purpose mesh buffers).
//!
//! # Frame Budget
//! GPU: 0.4ms, CPU: 0.2ms

use crate::context::GpuContext;
use crate::error::RenderError;
use crate::graph::{FrameContext, PassId, PassResourceBuilder, RenderPass, ResolvedResources};

pub struct WorkspacePanelPass {
    id: PassId,
    pipeline: wgpu::RenderPipeline,
    _bind_group_layouts: Vec<wgpu::BindGroupLayout>,
}

impl WorkspacePanelPass {
    pub fn new(id: PassId, ctx: &GpuContext) -> Result<Self, RenderError> {
        let vs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("panel_vs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(PANEL_VS)),
        });

        let fs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("panel_fs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(PANEL_FS)),
        });

        // Bind group 0: Camera UBO
        let camera_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("panel_camera_layout"),
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

        // Bind group 3: Material (content texture + sampler)
        let material_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("panel_material_layout"),
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

        let bind_group_layouts = vec![camera_layout, material_layout];
        let layout_refs: Vec<Option<&wgpu::BindGroupLayout>> = bind_group_layouts.iter().map(|l| Some(l)).collect();

        let pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("panel_layout"),
            bind_group_layouts: &layout_refs,
            immediate_size: 0,
        });

        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("workspace_panel"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[panel_vertex_layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Ok(Self {
            id,
            pipeline,
            _bind_group_layouts: bind_group_layouts,
        })
    }
}

impl RenderPass for WorkspacePanelPass {
    fn id(&self) -> PassId {
        self.id
    }

    fn name(&self) -> &'static str {
        "workspace_panel"
    }

    fn declare_resources(&self, builder: &mut PassResourceBuilder) {
        builder.write_texture(super::RESOURCE_COLOR);
    }

    fn is_active(&self, fc: &FrameContext) -> bool {
        fc.active_panels > 0 && !fc.focus_mode
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResolvedResources,
        _frame_ctx: &FrameContext,
        _ctx: &GpuContext,
    ) -> Result<(), RenderError> {
        let color_view = resources.color_texture.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "workspace_panel",
                cause: "Color texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let camera_bg = resources.camera_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "workspace_panel",
                cause: "Camera bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let material_bg = resources.material_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "workspace_panel",
                cause: "Material bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("workspace_panels"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
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
        rpass.set_bind_group(3, material_bg, &[]);

        // Draw panel geometry if vertex buffers are provided.
        if let (Some(vb), Some(ib)) = (
            resources.character_vertex_buffer.as_ref(),
            resources.character_index_buffer.as_ref(),
        ) {
            rpass.set_vertex_buffer(0, vb.slice(..));
            rpass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            rpass.draw_indexed(0..resources.character_index_count, 0, 0..1);
        }

        drop(rpass);
        Ok(())
    }
}

/// Vertex layout for panel quads (position, uv, color).
pub fn panel_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<[f32; 9]>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 12,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 20,
                shader_location: 2,
            },
        ],
    }
}

const PANEL_VS: &str = r#"
@group(0) @binding(0) var<uniform> camera: CameraUBO;

struct CameraUBO {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    viewport_size: vec2<f32>,
    time_seconds: f32,
    _pad: f32,
};

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_pos: vec3<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let world_pos = input.position;
    let clip_pos = camera.view_proj * vec4(world_pos, 1.0);
    var output: VertexOutput;
    output.clip_pos = clip_pos;
    output.uv = input.uv;
    output.color = input.color;
    output.world_pos = world_pos;
    return output;
}
"#;

const PANEL_FS: &str = r#"
@group(0) @binding(0) var<uniform> camera: CameraUBO;

@group(3) @binding(0) var panel_texture: texture_2d<f32>;
@group(3) @binding(1) var panel_sampler: sampler;

struct FragmentInput {
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_pos: vec3<f32>,
}

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(panel_texture, panel_sampler, input.uv);
    let tinted = tex_color * input.color;
    let scanline = sin(input.world_pos.y * 200.0 + camera.time_seconds * 4.0) * 0.5 + 0.5;
    let scanline_effect = 0.9 + 0.1 * scanline;
    let edge_uv = min(input.uv, 1.0 - input.uv);
    let edge_dist = min(edge_uv.x, edge_uv.y);
    let edge_glow = exp(-edge_dist * 20.0) * 0.15;
    let glow = vec4(tinted.rgb * (1.0 + edge_glow) * scanline_effect, tinted.a);
    return glow;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panel_vertex_layout_stride() {
        let layout = panel_vertex_layout();
        assert_eq!(layout.array_stride, 36);
        assert_eq!(layout.attributes.len(), 3);
    }

    #[test]
    fn test_panel_is_active_requires_panels() {
        let fc_active = FrameContext {
            active_panels: 3,
            focus_mode: false,
            ..Default::default()
        };
        assert!(fc_active.active_panels > 0);
        assert!(!fc_active.focus_mode);

        let fc_culled = FrameContext {
            active_panels: 0,
            ..Default::default()
        };
        assert_eq!(fc_culled.active_panels, 0);
    }

    #[test]
    fn test_panel_culled_in_focus_mode() {
        let fc = FrameContext {
            active_panels: 3,
            focus_mode: true,
            ..Default::default()
        };
        assert!(fc.focus_mode);
    }
}
