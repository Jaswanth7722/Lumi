//! Depth prepass — renders character mesh depth only (enables early-z rejection).
//!
//! This pass writes depth but no color. The geometry pass then uses
//! `CompareFunction::Equal` to skip fragments occluded by the prepass.
//!
//! # Frame Budget
//! GPU: 0.3ms, CPU: 0.1ms

use crate::context::GpuContext;
use crate::error::RenderError;
use crate::graph::{FrameContext, PassId, PassResourceBuilder, RenderPass, ResolvedResources};
use crate::mesh::character_vertex_layout;

pub struct DepthPrepassPass {
    id: PassId,
    pipeline: wgpu::RenderPipeline,
    _bind_group_layouts: Vec<wgpu::BindGroupLayout>,
}

impl DepthPrepassPass {
    pub fn new(id: PassId, ctx: &GpuContext) -> Result<Self, RenderError> {
        let vs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("depth_prepass_vs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(DEPTH_PREPASS_VS)),
        });

        let camera_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("depth_prepass_camera_layout"),
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

        let bone_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("depth_prepass_bone_layout"),
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

        let bind_group_layouts = vec![camera_layout, bone_layout];
        let layout_refs: Vec<Option<&wgpu::BindGroupLayout>> = bind_group_layouts.iter().map(|l| Some(l)).collect();

        let pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("depth_prepass_layout"),
            bind_group_layouts: &layout_refs,
            immediate_size: 0,
        });

        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth_prepass"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[character_vertex_layout()],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
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

impl RenderPass for DepthPrepassPass {
    fn id(&self) -> PassId {
        self.id
    }

    fn name(&self) -> &'static str {
        "depth_prepass"
    }

    fn declare_resources(&self, builder: &mut PassResourceBuilder) {
        builder.write_texture(super::RESOURCE_DEPTH);
    }

    fn is_active(&self, fc: &FrameContext) -> bool {
        !fc.focus_mode && fc.lod_level < 3
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResolvedResources,
        _frame_ctx: &FrameContext,
        _ctx: &GpuContext,
    ) -> Result<(), RenderError> {
        let depth_view = resources.depth_texture.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "depth_prepass",
                cause: "Depth texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let vertex_buffer = resources.character_vertex_buffer.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "depth_prepass",
                cause: "Character vertex buffer not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let index_buffer = resources.character_index_buffer.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "depth_prepass",
                cause: "Character index buffer not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let camera_bg = resources.camera_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "depth_prepass",
                cause: "Camera bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let bone_bg = resources.bone_matrix_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "depth_prepass",
                cause: "Bone matrix bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth_prepass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, camera_bg, &[]);
        rpass.set_bind_group(1, bone_bg, &[]);
        rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
        rpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        rpass.draw_indexed(0..resources.character_index_count, 0, 0..1);

        drop(rpass);
        Ok(())
    }
}

/// Minimal vertex shader for the depth prepass.
/// Transforms vertices into clip space using camera + bone skinning.
const DEPTH_PREPASS_VS: &str = r#"
@group(0) @binding(0) var<uniform> CameraUBO {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    viewport_size: vec2<f32>,
    time_seconds: f32,
};

@group(1) @binding(0) var<uniform> BoneMatrices {
    bones: array<mat4x4<f32>, 96>,
};

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(4) bone_indices: vec4<u32>,
    @location(5) bone_weights: vec4<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> @builtin(position) vec4<f32> {
    var pos = vec4<f32>(0.0);
    for (var i = 0u; i < 4u; i = i + 1u) {
        let bone = BoneMatrices.bones[input.bone_indices[i]];
        let weight = input.bone_weights[i];
        pos = pos + weight * (bone * vec4<f32>(input.position, 1.0));
    }
    return CameraUBO.view_proj * vec4<f32>(pos.xyz / max(pos.w, 1e-6), 1.0);
}
"#;
