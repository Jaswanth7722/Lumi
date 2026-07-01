//! PBR geometry pass — renders character with full PBR lighting.
//!
//! Cook-Torrance BRDF (GGX distribution, Smith geometry, Fresnel-Schlick)
//! with spherical harmonics ambient, directional, and point lights.
//!
//! # Frame Budget
//! GPU: 2.0ms, CPU: 0.3ms

use crate::context::GpuContext;
use crate::error::RenderError;
use crate::graph::{FrameContext, PassId, PassResourceBuilder, RenderPass, ResolvedResources};
use crate::mesh::character_vertex_layout;

pub struct GeometryPass {
    id: PassId,
    pipeline: wgpu::RenderPipeline,
    _bind_group_layouts: Vec<wgpu::BindGroupLayout>,
}

impl GeometryPass {
    pub fn new(id: PassId, ctx: &GpuContext) -> Result<Self, RenderError> {
        let vs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("geometry_vs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(PBR_VS)),
        });

        let fs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("geometry_fs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(PBR_FS)),
        });

        // Bind group 0: Camera UBO
        let camera_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("geometry_camera_layout"),
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

        // Bind group 1: Bone matrices
        let bone_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("geometry_bone_layout"),
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

        // Bind group 2: Lighting UBO
        let lighting_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("geometry_lighting_layout"),
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
        });

        // Bind group 3: Material textures (albedo, normal, roughness, ao, sampler)
        let material_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("geometry_material_layout"),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group_layouts = vec![camera_layout, bone_layout, lighting_layout, material_layout];
        let layout_refs: Vec<Option<&wgpu::BindGroupLayout>> = bind_group_layouts.iter().map(|l| Some(l)).collect();

        let pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("geometry_layout"),
            bind_group_layouts: &layout_refs,
            immediate_size: 0,
        });

        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("geometry_pbr"),
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
                depth_compare: Some(wgpu::CompareFunction::Equal),
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

impl RenderPass for GeometryPass {
    fn id(&self) -> PassId {
        self.id
    }

    fn name(&self) -> &'static str {
        "geometry"
    }

    fn declare_resources(&self, builder: &mut PassResourceBuilder) {
        builder.write_texture(super::RESOURCE_COLOR);
        builder.read_texture(super::RESOURCE_DEPTH);
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
        let color_view = resources.color_texture.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "geometry",
                cause: "Color texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let depth_view = resources.depth_texture.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "geometry",
                cause: "Depth texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let vertex_buffer = resources.character_vertex_buffer.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "geometry",
                cause: "Character vertex buffer not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let index_buffer = resources.character_index_buffer.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "geometry",
                cause: "Character index buffer not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let camera_bg = resources.camera_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "geometry", cause: "Camera bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let bone_bg = resources.bone_matrix_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "geometry", cause: "Bone matrix bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let lighting_bg = resources.lighting_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "geometry", cause: "Lighting bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let material_bg = resources.material_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "geometry", cause: "Material bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("geometry_pbr"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0, g: 0.0, b: 0.0, a: 0.0,
                    }),
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
        rpass.set_bind_group(2, lighting_bg, &[]);
        rpass.set_bind_group(3, material_bg, &[]);
        rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
        rpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        rpass.draw_indexed(0..resources.character_index_count, 0, 0..1);

        drop(rpass);
        Ok(())
    }
}

/// Vertex shader for PBR geometry: skinning + camera transform.
const PBR_VS: &str = r#"
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
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) bone_indices: vec4<u32>,
    @location(5) bone_weights: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
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
    return VertexOutput(
        CameraUBO.view_proj * vec4<f32>(world_pos, 1.0),
        vec4<f32>(world_pos, 1.0),
        normalize(norm),
        input.tangent,
        input.uv,
    );
}
"#;

/// Fragment shader: Cook-Torrance PBR with SH ambient + directional + point lights.
const PBR_FS: &str = r#"
@group(0) @binding(0) var<uniform> CameraUBO {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    viewport_size: vec2<f32>,
    time_seconds: f32,
};

@group(2) @binding(0) var<uniform> LightingUBO {
    ambient_sh: array<vec4<f32>, 7>,
    directional: array<vec4<f32>, 2>,
    point_lights: array<vec4<f32>, 12>,
    point_light_count: u32,
};

@group(3) @binding(0) var albedo_tex: texture_2d<f32>;
@group(3) @binding(1) var normal_tex: texture_2d<f32>;
@group(3) @binding(2) var rough_tex: texture_2d<f32>;
@group(3) @binding(3) var ao_tex: texture_2d<f32>;
@group(3) @binding(4) var material_sampler: sampler;

struct FragmentInput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
}

fn ndf_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let n_dot_h2 = n_dot_h * n_dot_h;
    let denom = 3.14159 * (a2 + (1.0 - a2) * n_dot_h2) * (a2 + (1.0 - a2) * n_dot_h2);
    return a2 / max(denom, 1e-6);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let ggx_v = n_dot_v / max(n_dot_v * (1.0 - a) + a, 1e-6);
    let ggx_l = n_dot_l / max(n_dot_l * (1.0 - a) + a, 1e-6);
    return ggx_v * ggx_l;
}

fn eval_sh(sh: array<vec4<f32>, 7>, dir: vec3<f32>) -> vec3<f32> {
    let x = dir.x;
    let y = dir.y;
    let z = dir.z;
    return sh[0].rgb
        - sh[1].rgb * y
        + sh[2].rgb * z
        - sh[3].rgb * x;
}

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    let albedo = textureSample(albedo_tex, material_sampler, input.uv).rgb;
    let roughness = textureSample(rough_tex, material_sampler, input.uv).r;
    let ao = textureSample(ao_tex, material_sampler, input.uv).r;
    let metallic: f32 = 0.0;

    let N = normalize(input.normal);
    let V = normalize(CameraUBO.camera_pos.xyz - input.world_pos.xyz);
    let NdotV = max(dot(N, V), 0.0);
    let F0 = mix(vec3<f32>(0.04), albedo, metallic);

    // Ambient from spherical harmonics
    var color = eval_sh(LightingUBO.ambient_sh, N) * albedo * ao;

    // Directional light
    let dir = normalize(LightingUBO.directional[0].xyz);
    let radiance = LightingUBO.directional[0].w * LightingUBO.directional[1].rgb;
    let L = -dir;
    let H = normalize(V + L);
    let NdotL = max(dot(N, L), 0.0);
    let NdotH = max(dot(N, H), 0.0);
    let HdotV = max(dot(H, V), 0.0);

    let NDF = ndf_ggx(NdotH, roughness);
    let G = geometry_smith(NdotV, NdotL, roughness);
    let F = fresnel_schlick(HdotV, F0);
    let kD = (1.0 - F) * (1.0 - metallic);
    let spec = (NDF * G * F) / max(4.0 * NdotV * NdotL, 1e-6);
    color = color + (kD * albedo / 3.14159 + spec) * radiance * NdotL;

    // Point lights — each light is 3 vec4<f32> (position, color, range+pad)
    for (var i = 0u; i < min(LightingUBO.point_light_count, 4u); i = i + 1u) {
        let light_idx = i * 3u;
        let pl_pos = LightingUBO.point_lights[light_idx];
        let pl_color = LightingUBO.point_lights[light_idx + 1u];
        let pl_range = LightingUBO.point_lights[light_idx + 2u].x;

        let to_light = pl_pos.xyz - input.world_pos.xyz;
        let distance = length(to_light);
        if (distance < pl_range && distance > 0.001) {
            let L = normalize(to_light);
            let H = normalize(V + L);
            let NdotL = max(dot(N, L), 0.0);
            let NdotH = max(dot(N, H), 0.0);
            let HdotV = max(dot(H, V), 0.0);
            let attenuation = 1.0 / (distance * distance);
            let rad = pl_color.rgb * attenuation;
            let NDF2 = ndf_ggx(NdotH, roughness);
            let G2 = geometry_smith(NdotV, NdotL, roughness);
            let F2 = fresnel_schlick(HdotV, F0);
            let spec2 = (NDF2 * G2 * F2) / max(4.0 * NdotV * NdotL, 1e-6);
            color = color + (kD * albedo / 3.14159 + spec2) * rad * NdotL;
        }
    }

    return vec4<f32>(color.rgb, 1.0);
}
"#;
