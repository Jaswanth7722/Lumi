// Lumas UI Panel Shader
//
// Renders holographic workspace panels as textured quads with
// translucency and a glow effect.
//
// Bind groups:
//   @group(0) @binding(0) — CameraUBO (uniform, VERTEX)
//   @group(3) @binding(0) — Panel content texture (FRAGMENT)
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
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_pos: vec3<f32>,
};

// ──────────────────────────────────────────────
// Vertex Shader
// ──────────────────────────────────────────────

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

// ──────────────────────────────────────────────
// Fragment Shader
// ──────────────────────────────────────────────

@group(3) @binding(0)
var panel_texture: texture_2d<f32>;
@group(3) @binding(1)
var panel_sampler: sampler;

struct FragmentInput {
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_pos: vec3<f32>,
};

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    // Sample the panel content texture (UI render output).
    let tex_color = textureSample(panel_texture, panel_sampler, input.uv);

    // Blend the vertex color (which includes glow and opacity) with the texture.
    let color = tex_color * input.color;

    // Holographic glow effect: scanline + edge glow.
    let scanline = sin(input.world_pos.y * 200.0 + camera.time_seconds * 4.0) * 0.5 + 0.5;
    let scanline_effect = 0.9 + 0.1 * scanline;  // Subtle scanline

    // Edge glow: brighter near the panel border.
    let edge_uv = min(input.uv, 1.0 - input.uv);
    let edge_dist = min(edge_uv.x, edge_uv.y);
    let edge_glow = exp(-edge_dist * 20.0) * 0.15;

    // Apply bloom-friendly glow: add glow to emission (linear HDR).
    let glow = vec4(color.rgb * (1.0 + edge_glow) * scanline_effect, color.a);

    // Output pre-multiplied alpha.
    return glow;
}
