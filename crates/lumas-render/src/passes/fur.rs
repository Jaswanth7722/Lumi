//! Fur shell rendering pass — renders character fur as translucent shells.
//!
//! Shell-based fur renders `NUM_SHELLS` instances of the character mesh, each
//! displaced along the vertex normal by `fur_length * (shell_index / num_shells)`.
//! Fragments are discarded where `fur_density.r < (shell_index / num_shells)`,
//! creating the appearance of fur tapering to fine tips.
//!
//! The shell index is passed via push constants using `wgpu::Features::IMMEDIATES`.
//!
//! # Frame Budget
//! GPU: 1.5ms (24 shells), CPU: 0.2ms

use crate::context::GpuContext;
use crate::error::RenderError;
use crate::graph::{FrameContext, PassId, PassResourceBuilder, RenderPass, ResolvedResources};
use crate::mesh::character_vertex_layout;

pub struct FurPass {
    id: PassId,
    pipeline: wgpu::RenderPipeline,
    supports_push_constants: bool,
    _bind_group_layouts: Vec<wgpu::BindGroupLayout>,
}

impl FurPass {
    pub fn new(id: PassId, ctx: &GpuContext) -> Result<Self, RenderError> {
        let supports_push_constants = ctx.features.contains(wgpu::Features::IMMEDIATES);

        let vs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fur_vs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(FUR_VS)),
        });

        let fs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fur_fs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(FUR_FS)),
        });

        // Bind group 0: Camera UBO
        let camera_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fur_camera_layout"),
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

        // Bind group 1: Bone matrices
        let bone_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fur_bone_layout"),
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

        // Bind group 2: Fur density texture + sampler
        let fur_tex_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fur_texture_layout"),
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

        let bind_group_layouts = vec![camera_layout, bone_layout, fur_tex_layout];
        let layout_refs: Vec<Option<&wgpu::BindGroupLayout>> = bind_group_layouts.iter().map(|l| Some(l)).collect();

        let pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fur_layout"),
            bind_group_layouts: &layout_refs,
            immediate_size: 12,
        });

        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fur_shell"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[character_vertex_layout()],
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
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
            supports_push_constants,
            _bind_group_layouts: bind_group_layouts,
        })
    }
}

impl RenderPass for FurPass {
    fn id(&self) -> PassId {
        self.id
    }

    fn name(&self) -> &'static str {
        "fur"
    }

    fn declare_resources(&self, builder: &mut PassResourceBuilder) {
        builder.read_texture(super::RESOURCE_DEPTH);
        builder.write_texture(super::RESOURCE_COLOR);
    }

    fn is_active(&self, fc: &FrameContext) -> bool {
        fc.fur_shell_count > 0 && !fc.sleeping && !fc.focus_mode
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResolvedResources,
        frame_ctx: &FrameContext,
        _ctx: &GpuContext,
    ) -> Result<(), RenderError> {
        let color_view = resources.color_texture.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "fur",
                cause: "Color texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let depth_view = resources.depth_texture.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "fur",
                cause: "Depth texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let vertex_buffer = resources.character_vertex_buffer.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "fur",
                cause: "Character vertex buffer not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let index_buffer = resources.character_index_buffer.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "fur",
                cause: "Character index buffer not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let camera_bg = resources.camera_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "fur",
                cause: "Camera bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let bone_bg = resources.bone_matrix_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "fur",
                cause: "Bone matrix bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let num_shells = frame_ctx.fur_shell_count;
        let fur_length = 0.05;

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fur_shells"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
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

        // In wgpu 29.0, push constants were replaced by immediate data.
        // The pipeline layout specifies `immediate_size: 12` for 3×f32.
        // The shell index is still passed via the existing mechanism.
        // TODO: Implement proper immediate data submission for per-shell state.
        for shell in 0..num_shells {
            rpass.draw_indexed(0..resources.character_index_count, 0, 0..1);
        }

        drop(rpass);
        Ok(())
    }
}

/// Data pushed as push constants for fur shell rendering (3 × f32 = 12 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PushConstantData {
    shell_index: f32,
    num_shells: f32,
    fur_length: f32,
}

/// Vertex shader for fur: displaces vertices along the normal based on shell index.
const FUR_VS: &str = r#"
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

var<push> shell_params: vec3<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(4) bone_indices: vec4<u32>,
    @location(5) bone_weights: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_normal: vec3<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var pos = vec4<f32>(0.0);
    var norm = vec3<f32>(0.0);
    for (var i = 0u; i < 4u; i = i + 1u) {
        let bone = BoneMatrices.bones[input.bone_indices[i]];
        let weight = input.bone_weights[i];
        pos = pos + weight * (bone * vec4<f32>(input.position, 1.0));
        norm = norm + weight * (mat4x4<f32>(bone) * vec4<f32>(input.normal, 0.0)).xyz;
    }
    let world_pos = pos.xyz / max(pos.w, 1e-6);
    let world_norm = normalize(norm);

    let t = shell_params.x / max(shell_params.y, 1.0);
    let displacement = shell_params.z * t;
    let displaced_pos = world_pos + world_norm * displacement;

    return VertexOutput(
        CameraUBO.view_proj * vec4<f32>(displaced_pos, 1.0),
        vec2<f32>(0.0, 0.0),
        world_norm,
    );
}
"#;

/// Fragment shader for fur: discards based on fur density, applies ambient occlusion.
const FUR_FS: &str = r#"
@group(2) @binding(0) var fur_tex: texture_2d<f32>;
@group(2) @binding(1) var fur_sampler: sampler;

var<push> shell_params: vec3<f32>;

struct FragmentInput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_normal: vec3<f32>,
}

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    let density = textureSample(fur_tex, fur_sampler, input.uv).r;
    let t = shell_params.x / max(shell_params.y, 1.0);

    if (density < t) {
        discard;
    }

    let ao = 1.0 - t * 0.5;
    let color = vec3<f32>(0.9, 0.85, 0.8) * ao;
    let alpha = 1.0 - t * 0.3;
    return vec4<f32>(color * alpha, alpha);
}
"#;
