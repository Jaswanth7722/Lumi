// Lumas Shadow Sprite Shader
//
// Renders a soft drop shadow sprite beneath the character.
// The shadow is a simple blurred circle/ellipse sprite rendered
// at the character's foot position, projected onto the desktop plane.
//
// Bind groups:
//   @group(0) @binding(0) — CameraUBO (uniform, VERTEX)
//   @group(3) @binding(0) — Shadow texture (FRAGMENT)
//   @group(3) @binding(1) — Sampler (FRAGMENT)

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
// Vertex Input / Output
// ──────────────────────────────────────────────

struct VertexInput {
    @location(0) position: vec3<f32>,   // Quad corner offset (local space)
    @location(1) uv: vec2<f32>,          // Texture UV
};

struct ShadowInstance {
    world_pos: vec4<f32>,                // Character foot position (world space xyz, w=1.0)
    size: f32,                           // Shadow size
    opacity: f32,                        // Shadow opacity
    _pad0: f32,
    _pad1: f32,
};

// Instance data is passed via storage buffer for GPU-instanced rendering.
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

// ──────────────────────────────────────────────
// Vertex Shader
// ──────────────────────────────────────────────

@vertex
fn vs_main(
    input: VertexInput,
    @builtin(instance_index) instance_id: u32,
) -> VertexOutput {
    let instance = instances.data[instance_id];

    // Offset the quad corner by the instance position + size.
    let world_offset = input.position * instance.size;
    let world_pos = instance.world_pos + vec4(world_offset, 0.0);

    // Project to clip space.
    let clip_pos = camera.view_proj * world_pos;

    var output: VertexOutput;
    output.clip_pos = clip_pos;
    output.uv = input.uv;
    output.opacity = instance.opacity;
    return output;
}

// ──────────────────────────────────────────────
// Fragment Shader
// ──────────────────────────────────────────────

@group(3) @binding(0)
var shadow_texture: texture_2d<f32>;
@group(3) @binding(1)
var shadow_sampler: sampler;

struct FragmentInput {
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
};

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    // Sample the shadow sprite (soft radial gradient).
    let shadow_sample = textureSample(shadow_texture, shadow_sampler, input.uv);

    // Shadow is a dark, semi-transparent sprite.
    // The texture contains the alpha mask; color is uniform dark.
    let alpha = shadow_sample.r * input.opacity;

    // Output pre-multiplied alpha shadow color (dark / black).
    // Use the alpha channel for shadow density.
    return vec4(0.0, 0.0, 0.0, alpha);
}
