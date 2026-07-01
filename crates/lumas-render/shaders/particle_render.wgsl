// Lumas Particle Render Shader
//
// Render pass for billboarded particle sprites. Each particle instance
// generates a camera-facing quad. The atlas texture provides the sprite
// appearance with pre-multiplied alpha.
//
// Bind groups:
//   @group(0) @binding(0) — CameraUBO (uniform)
//   @group(1) @binding(0) — ParticleData storage buffer (read-only for positions)
//   @group(2)             — Atlas texture + sampler (FRAGMENT only):
//     binding 0: atlas_texture
//     binding 1: sampler

// ──────────────────────────────────────────────
// Uniforms
// ──────────────────────────────────────────────

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

// ──────────────────────────────────────────────
// Particle Data (storage buffer — read-only in render)
// ──────────────────────────────────────────────

struct Particle {
    position: vec4<f32>,
    velocity: vec4<f32>,
    life: f32,
    max_life: f32,
    size: f32,
    flags: u32,
    color: vec4<f32>,
};

struct Particles {
    data: array<Particle>,
};

@group(1) @binding(0)
var<storage, read> particles: Particles;

// ──────────────────────────────────────────────
// Vertex Input / Output
// ──────────────────────────────────────────────

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

// ──────────────────────────────────────────────
// Vertex Shader — Billboard Quad
// ──────────────────────────────────────────────

// Billboard quad vertices (pre-defined in a 4-vertex strip).
// Each particle instance emits 4 vertices forming a camera-facing quad.
// The vertex_id within the instance determines which corner.

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_id: u32,
    @builtin(instance_index) instance_id: u32,
) -> VertexOutput {
    let particle = particles.data[instance_id];

    // Skip dead particles by pushing them off-screen.
    if particle.flags == 0u {
        var output: VertexOutput;
        output.clip_pos = vec4(0.0, 0.0, 2.0, 1.0);  // Behind far plane
        output.uv = vec2(0.0, 0.0);
        output.color = vec4(0.0);
        return output;
    }

    // Quad corners in local space: [-1,-1] to [1,1].
    let corner_uv = array<vec2<f32>, 4>(
        vec2(-1.0, -1.0),  // bottom-left
        vec2(1.0, -1.0),   // bottom-right
        vec2(-1.0, 1.0),   // top-left
        vec2(1.0, 1.0),    // top-right
    );

    // UV coordinates for sprite atlas (full texture, no atlas slicing yet).
    let quad_uv = array<vec2<f32>, 4>(
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(1.0, 1.0),
    );

    // Get camera right and up vectors for billboarding.
    // In view space, right = (1,0,0), up = (0,1,0).
    // Transform to world space using the inverse view matrix.
    let view_mat = camera.view;
    let right = vec3(view_mat[0][0], view_mat[1][0], view_mat[2][0]);  // First column of view = camera right
    let up = vec3(view_mat[0][1], view_mat[1][1], view_mat[2][1]);    // Second column = camera up

    let corner = corner_uv[vertex_id % 4u];
    let half_size = particle.size * 0.5;
    let world_offset = right * corner.x * half_size + up * corner.y * half_size;

    let world_pos = particle.position.xyz + world_offset;
    let clip_pos = camera.view_proj * vec4(world_pos, 1.0);

    var output: VertexOutput;
    output.clip_pos = clip_pos;
    output.uv = quad_uv[vertex_id % 4u];
    output.color = particle.color;
    return output;
}

// ──────────────────────────────────────────────
// Fragment Shader
// ──────────────────────────────────────────────

@group(3) @binding(0)
var atlas_texture: texture_2d<f32>;
@group(3) @binding(1)
var atlas_sampler: sampler;

struct FragmentInput {
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    let sprite_sample = textureSample(atlas_texture, atlas_sampler, input.uv);

    // Pre-multiply: atlas sample already has pre-multiplied alpha.
    // Multiply by particle color for tinting.
    let final_color = sprite_sample * input.color;

    return final_color;
}
