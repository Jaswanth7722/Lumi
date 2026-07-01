//! Particle render pass — compute-driven particle simulation and billboard sprite rendering.
//!
//! The particle system uses compute shaders for updating particle state (position,
//! velocity, lifetime) on the GPU, then renders the particles as camera-facing
//! billboard quads from an atlas texture.
//!
//! Workgroup size: 64 particles per thread group.
//! Draw calls use indirect draw from a buffer written by the compute shader.
//!
//! # Frame Budget
//! GPU: 0.8ms, CPU: 0.2ms

use crate::context::GpuContext;
use crate::error::RenderError;
use crate::graph::{FrameContext, PassId, PassResourceBuilder, RenderPass, ResolvedResources};

pub struct ParticlePass {
    id: PassId,
    pipeline: wgpu::RenderPipeline,
    compute_pipeline: wgpu::ComputePipeline,
    particle_buffer: wgpu::Buffer,
    indirect_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    _bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    compute_bind_group: wgpu::BindGroup,
    render_particle_bind_group: wgpu::BindGroup,
    num_particles: u32,
}

impl ParticlePass {
    pub fn new(id: PassId, ctx: &GpuContext, max_particles: u32) -> Result<Self, RenderError> {
        let max_particles = max_particles.max(64).next_multiple_of(64);

        let compute_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle_update_cs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(PARTICLE_UPDATE_CS)),
        });

        let vs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle_vs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(PARTICLE_VS)),
        });

        let fs_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle_fs"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(PARTICLE_FS)),
        });

        // ── Compute bind group layout ──
        let compute_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle_compute_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // ── Render bind group layouts ──
        // @group(0) — CameraUBO (uniform)
        let camera_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle_camera_layout"),
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

        // @group(1) — ParticleData (storage, read-only in render)
        let particle_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle_data_layout"),
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

        // @group(3) — Atlas texture + sampler
        let atlas_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle_atlas_layout"),
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

        // ── Create GPU buffers ──
        let particle_size = std::mem::size_of::<ParticleGPU>() as u64;
        let particle_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle_data"),
            size: particle_size * max_particles as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let indirect_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle_indirect"),
            size: std::mem::size_of::<wgpu::util::DrawIndexedIndirectArgs>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle_ubo"),
            size: std::mem::size_of::<ParticleSystemUBO>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Create compute bind group (3 bindings) ──
        let compute_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle_compute_bg"),
            layout: &compute_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &particle_buffer, offset: 0, size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &indirect_buffer, offset: 0, size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer, offset: 0, size: None,
                    }),
                },
            ],
        });

        // ── Create dedicated render bind group for particle storage (group 1, binding 0 only) ──
        let render_particle_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle_render_bg"),
            layout: &particle_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &particle_buffer, offset: 0, size: None,
                    }),
                },
            ],
        });

        // Initialize indirect buffer.
        let initial_indirect = wgpu::util::DrawIndexedIndirectArgs {
            index_count: 0, instance_count: 1,
            first_index: 0, base_vertex: 0, first_instance: 0,
        };
        ctx.queue.write_buffer(&indirect_buffer, 0, bytemuck::bytes_of(&initial_indirect));

        // ── Compute pipeline ──
        let compute_pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle_compute_layout"),
            bind_group_layouts: &[Some(&compute_layout)],
            immediate_size: 0,
        });

        let compute_pipeline = ctx.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("particle_compute"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_module,
            entry_point: Some("cs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // ── Render pipeline ──
        let render_bind_group_layouts = vec![camera_layout, particle_layout, atlas_layout];
        let render_layout_refs: Vec<Option<&wgpu::BindGroupLayout>> = render_bind_group_layouts.iter().map(|l| Some(l)).collect();

        let render_pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle_render_layout"),
            bind_group_layouts: &render_layout_refs,
            immediate_size: 0,
        });

        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particle_render"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
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
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: Some(wgpu::IndexFormat::Uint32),
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
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
            compute_pipeline,
            particle_buffer,
            indirect_buffer,
            uniform_buffer,
            _bind_group_layouts: render_bind_group_layouts,
            compute_bind_group,
            render_particle_bind_group,
            num_particles: max_particles,
        })
    }

    pub fn update_uniforms(&self, queue: &wgpu::Queue, ubo: &ParticleSystemUBO) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(ubo));
    }

    pub fn particle_buffer(&self) -> &wgpu::Buffer {
        &self.particle_buffer
    }

    pub fn indirect_buffer(&self) -> &wgpu::Buffer {
        &self.indirect_buffer
    }
}

impl RenderPass for ParticlePass {
    fn id(&self) -> PassId {
        self.id
    }

    fn name(&self) -> &'static str {
        "particles"
    }

    fn declare_resources(&self, builder: &mut PassResourceBuilder) {
        builder.read_texture(super::RESOURCE_DEPTH);
        builder.write_texture(super::RESOURCE_COLOR);
    }

    fn is_active(&self, fc: &FrameContext) -> bool {
        fc.active_particles > 0 && !fc.sleeping && !fc.focus_mode
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
                pass: "particles",
                cause: "Color texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let depth_view = resources.depth_texture.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "particles",
                cause: "Depth texture not allocated".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        // Compute pass: update particles.
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("particle_update"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.compute_pipeline);
            cpass.set_bind_group(0, &self.compute_bind_group, &[]);
            let workgroup_count = (self.num_particles + 63) / 64;
            cpass.dispatch_workgroups(workgroup_count, 1, 1);
        }

        // Render pass: billboard particles.
        let camera_bg = resources.camera_bind_group.as_ref().ok_or_else(|| {
            RenderError::RenderPassFailed {
                pass: "particles",
                cause: "Camera bind group not set".into(),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("particle_render"),
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
        rpass.set_bind_group(1, &self.render_particle_bind_group, &[]);
        if let Some(material_bg) = resources.material_bind_group.as_ref() {
            rpass.set_bind_group(3, material_bg, &[]);
        }
        rpass.draw_indexed_indirect(&self.indirect_buffer, 0);

        drop(rpass);
        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ParticleGPU {
    pub position: [f32; 4],
    pub velocity: [f32; 4],
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
    pub flags: u32,
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ParticleSystemUBO {
    pub delta_time: f32,
    pub elapsed_time: f32,
    pub emitter_position: [f32; 4],
    pub gravity: [f32; 4],
    pub spawn_rate: f32,
    pub lifetime: f32,
    pub initial_speed: f32,
    pub speed_spread: f32,
    pub _pad: f32,
}

const PARTICLE_UPDATE_CS: &str = r#"
struct Particle {
    position: vec4<f32>,
    velocity: vec4<f32>,
    life: f32,
    max_life: f32,
    size: f32,
    flags: u32,
    color: vec4<f32>,
};
struct Particles { data: array<Particle>, };

@group(0) @binding(0) var<storage, read_write> particles: Particles;
@group(0) @binding(1) var<storage, read_write> indirect: array<u32>;
@group(0) @binding(2) var<uniform> ubo: ParticleSystemUBO;

struct ParticleSystemUBO {
    delta_time: f32, elapsed_time: f32,
    emitter_position: vec4<f32>, gravity: vec4<f32>,
    spawn_rate: f32, lifetime: f32, initial_speed: f32, speed_spread: f32, _pad: f32,
};

fn rand_xorshift(state: ptr<function, u32>) -> u32 {
    var x = *state; x = x ^ (x << 13u); x = x ^ (x >> 17u); x = x ^ (x << 5u); *state = x; return x;
}
fn rand_f32(state: ptr<function, u32>) -> f32 {
    return f32(rand_xorshift(state) & 0x007fffffffu) / f32(0x007fffffffu);
}
fn rand_range(state: ptr<function, u32>, min_val: f32, max_val: f32) -> f32 {
    return min_val + rand_f32(state) * (max_val - min_val);
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let index = gid.x;
    let total = arrayLength(&particles.data);
    if index >= total { return; }
    var p = particles.data[index];
    var rng = u32(index) * 2654435761u + u32(ubo.elapsed_time * 1000.0);
    if p.flags == 1u {
        p.life -= ubo.delta_time;
        if p.life <= 0.0 { p.flags = 0u; particles.data[index] = p; return; }
        p.velocity += ubo.gravity * ubo.delta_time;
        p.position += p.velocity * ubo.delta_time;
        let t = p.life / max(p.max_life, 0.001);
        p.color.a *= min(t * 2.0, 1.0);
        particles.data[index] = p;
    } else {
        let prob = ubo.spawn_rate * ubo.delta_time / f32(total);
        if rand_f32(&rng) < prob {
            let theta = rand_f32(&rng) * 6.2831853;
            let phi = rand_f32(&rng) * 3.1415927;
            let r = rand_f32(&rng) * 0.5;
            let sp = ubo.emitter_position.xyz + vec3(r*sin(phi)*cos(theta), r*cos(phi), r*sin(phi)*sin(theta));
            let speed = rand_range(&rng, ubo.initial_speed - ubo.speed_spread, ubo.initial_speed + ubo.speed_spread);
            let sv = vec3(speed*sin(phi)*cos(theta), speed*cos(phi), speed*sin(phi)*sin(theta)) * max(speed, 0.0);
            p.position = vec4(sp, 1.0); p.velocity = vec4(sv, 0.0);
            p.max_life = ubo.lifetime; p.life = ubo.lifetime*(0.5+rand_f32(&rng)*0.5);
            p.size = 0.15+rand_f32(&rng)*0.1; p.flags = 1u; p.color = vec4(1.0);
            particles.data[index] = p;
        }
    }
    if index == 0u {
        var alive = 0u;
        for (var i = 0u; i < total; i++) { if particles.data[i].flags == 1u { alive++; } }
        indirect[0] = alive * 6u; indirect[1] = 1u; indirect[2] = 0u; indirect[3] = 0u; indirect[4] = 0u;
    }
}
"#;

const PARTICLE_VS: &str = r#"
@group(0) @binding(0) var<uniform> camera: CameraUBO;
struct CameraUBO {
    view_proj: mat4x4<f32>, view: mat4x4<f32>, proj: mat4x4<f32>,
    camera_pos: vec4<f32>, viewport_size: vec2<f32>, time_seconds: f32, _pad: f32,
};
struct Particle { position: vec4<f32>, velocity: vec4<f32>, life: f32, max_life: f32, size: f32, flags: u32, color: vec4<f32> };
struct Particles { data: array<Particle> };
@group(1) @binding(0) var<storage, read> particles: Particles;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> VertexOutput {
    let p = particles.data[iid];
    if p.flags == 0u {
        var o: VertexOutput; o.clip_pos = vec4(0.0,0.0,2.0,1.0); o.uv = vec2(0.0); o.color = vec4(0.0); return o;
    }
    let c = array<vec2<f32>,4>(vec2(-1.0,-1.0),vec2(1.0,-1.0),vec2(-1.0,1.0),vec2(1.0,1.0));
    let u = array<vec2<f32>,4>(vec2(0.0,0.0),vec2(1.0,0.0),vec2(0.0,1.0),vec2(1.0,1.0));
    let vm = camera.view;
    let r = vec3(vm[0][0],vm[1][0],vm[2][0]);
    let up = vec3(vm[0][1],vm[1][1],vm[2][1]);
    let corner = c[vid%4u];
    let h = p.size*0.5;
    let wo = r*corner.x*h+up*corner.y*h;
    let wp = p.position.xyz+wo;
    let cp = camera.view_proj*vec4(wp,1.0);
    var o: VertexOutput; o.clip_pos=cp; o.uv=u[vid%4u]; o.color=p.color; return o;
}
"#;

const PARTICLE_FS: &str = r#"
@group(3) @binding(0) var atlas_texture: texture_2d<f32>;
@group(3) @binding(1) var atlas_sampler: sampler;

struct FragmentInput { @location(0) uv: vec2<f32>, @location(1) color: vec4<f32> }

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    return textureSample(atlas_texture, atlas_sampler, input.uv) * input.color;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_particles_aligned() {
        assert_eq!(64u32.next_multiple_of(64), 64);
        assert_eq!(4096u32.next_multiple_of(64), 4096);
    }

    #[test]
    fn test_particle_gpu_size() {
        assert_eq!(std::mem::size_of::<ParticleGPU>(), 64);
    }

    #[test]
    fn test_particle_ubo_size() {
        assert_eq!(std::mem::size_of::<ParticleSystemUBO>(), 64);
    }

    #[test]
    fn test_particle_is_active_logic() {
        let fc = FrameContext { active_particles: 100, sleeping: false, focus_mode: false, ..Default::default() };
        assert!(fc.active_particles > 0 && !fc.sleeping && !fc.focus_mode);
        let fc_sleep = FrameContext { active_particles: 100, sleeping: true, ..Default::default() };
        assert!(fc_sleep.sleeping);
        let fc_inactive = FrameContext { active_particles: 0, ..Default::default() };
        assert_eq!(fc_inactive.active_particles, 0);
    }
}
