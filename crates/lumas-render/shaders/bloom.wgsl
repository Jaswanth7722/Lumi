// Lumas Bloom Compute Shaders
//
// GPU bloom implementation using compute shaders:
//   1. Extract bright pixels above a luminance threshold
//   2. Downsample 2x (dual-kawase)
//   3. Downsample 4x (dual-kawase)
//   4. Upsample 2x (blend with 4x)
//   5. Upsample 1x (blend with 2x)
//
// All passes use compute shaders instead of fullscreen triangles
// for better performance on small render targets.
//
// Bind groups:
//   @group(0) @binding(0) — BloomConfig (uniform)
//   @group(0) @binding(1) — Input texture (read)
//   @group(0) @binding(2) — Output texture (write)

// ──────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────

const WORKGROUP_SIZE_X: u32 = 16u;
const WORKGROUP_SIZE_Y: u32 = 16u;

// ──────────────────────────────────────────────
// Uniforms
// ──────────────────────────────────────────────

struct BloomConfig {
    threshold: f32,         // Lumasnance threshold for bright-pass extraction
    strength: f32,          // Bloom intensity multiplier
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0)
var<uniform> config: BloomConfig;

// ──────────────────────────────────────────────
// Texture bindings (overridden per-pass)
// ──────────────────────────────────────────────

@group(0) @binding(1)
var bloom_input: texture_2d<f32>;
@group(0) @binding(2)
var bloom_output: texture_storage_2d<rgba16float, write>;

// ──────────────────────────────────────────────
// Lumasnance Helper
// ──────────────────────────────────────────────

fn luminance(color: vec3<f32>) -> f32 {
    // BT.709 luminance weights.
    return dot(color, vec3(0.2126, 0.7152, 0.0722));
}

// ──────────────────────────────────────────────
// 1. Bright-Pass Extraction
// ──────────────────────────────────────────────

@compute @workgroup_size(WORKGROUP_SIZE_X, WORKGROUP_SIZE_Y)
fn bloom_extract(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(bloom_input);
    if gid.x >= dims.x || gid.y >= dims.y { return; }

    let coord = vec2<i32>(gid.xy);
    let color = textureLoad(bloom_input, coord, 0);

    // Extract bright pixels above the luminance threshold.
    let luma = luminance(color.rgb);
    var bright = color;
    if luma > config.threshold {
        // Preserve color, only boost bright parts.
        bright = vec4(color.rgb * (luma - config.threshold) / max(luma, 0.001), color.a);
    } else {
        bright = vec4(0.0);
    }

    textureStore(bloom_output, coord.xy, bright);
}

// ──────────────────────────────────────────────
// 2. Downsample (Dual-Kawase)
// ──────────────────────────────────────────────

// Kawase downsampling: sample 4 texels at half-pixel offsets.
// This creates a smoother bloom than simple box downsample.

fn kawase_downsample_coord(center: vec2<f32>, offset: f32, dims: vec2<f32>) -> vec2<f32> {
    let uv = (center + vec2(0.5, 0.5)) / dims;
    return uv;
}

@compute @workgroup_size(WORKGROUP_SIZE_X, WORKGROUP_SIZE_Y)
fn bloom_downsample(@builtin(global_invocation_id) gid: vec3<u32>) {
    let out_dims = textureDimensions(bloom_output);
    if gid.x >= out_dims.x || gid.y >= out_dims.y { return; }

    let out_coord = vec2<i32>(gid.xy);
    let in_dims = vec2<f32>(textureDimensions(bloom_input));
    let half_offset = 0.5;  // Kawase half-pixel offset

    // 4 tap positions in the source texture.
    let center = vec2<f32>(out_coord) * 2.0;  // Source position (2x scale)

    // Kawase kernel: 4 taps at half-pixel offsets.
    let uv_center = kawase_downsample_coord(center, 0.0, in_dims);
    let uv_tl = (center + vec2(-half_offset, -half_offset)) / in_dims;
    let uv_tr = (center + vec2(half_offset, -half_offset)) / in_dims;
    let uv_bl = (center + vec2(-half_offset, half_offset)) / in_dims;
    let uv_br = (center + vec2(half_offset, half_offset)) / in_dims;

    // Use textureSampleLevel for proper mip-level sampling.
    let s0 = textureSampleLevel(bloom_input, sampler_, uv_tl, 0.0);
    let s1 = textureSampleLevel(bloom_input, sampler_, uv_tr, 0.0);
    let s2 = textureSampleLevel(bloom_input, sampler_, uv_bl, 0.0);
    let s3 = textureSampleLevel(bloom_input, sampler_, uv_br, 0.0);

    let result = (s0 + s1 + s2 + s3) * 0.25;
    textureStore(bloom_output, out_coord.xy, result);
}

// We need a sampler for the compute shader.
@group(0) @binding(3)
var sampler_: sampler;

// ──────────────────────────────────────────────
// 3. Upsample (Dual-Kawase)
// ──────────────────────────────────────────────

@compute @workgroup_size(WORKGROUP_SIZE_X, WORKGROUP_SIZE_Y)
fn bloom_upsample(@builtin(global_invocation_id) gid: vec3<u32>) {
    let out_dims = textureDimensions(bloom_output);
    if gid.x >= out_dims.x || gid.y >= out_dims.y { return; }

    let out_coord = vec2<i32>(gid.xy);
    let in_dims = vec2<f32>(textureDimensions(bloom_input));
    let half_offset = 0.5;

    // Source position: half resolution of output.
    let center = vec2<f32>(out_coord) * 0.5;

    // Kawase upsample: 4 taps with half-pixel offsets (wider spread).
    let uv_tl = (center * 2.0 + vec2(-half_offset, -half_offset) * 2.0) / in_dims;
    let uv_tr = (center * 2.0 + vec2(half_offset, -half_offset) * 2.0) / in_dims;
    let uv_bl = (center * 2.0 + vec2(-half_offset, half_offset) * 2.0) / in_dims;
    let uv_br = (center * 2.0 + vec2(half_offset, half_offset) * 2.0) / in_dims;

    let s0 = textureSampleLevel(bloom_input, sampler_, uv_tl, 0.0);
    let s1 = textureSampleLevel(bloom_input, sampler_, uv_tr, 0.0);
    let s2 = textureSampleLevel(bloom_input, sampler_, uv_bl, 0.0);
    let s3 = textureSampleLevel(bloom_input, sampler_, uv_br, 0.0);

    let result = (s0 + s1 + s2 + s3) * 0.25;
    textureStore(bloom_output, out_coord.xy, result);
}

// ──────────────────────────────────────────────
// 4. Bloom Composite (additive blend on top of HDR)
// ──────────────────────────────────────────────

@compute @workgroup_size(WORKGROUP_SIZE_X, WORKGROUP_SIZE_Y)
fn bloom_composite(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(bloom_output);
    if gid.x >= dims.x || gid.y >= dims.y { return; }

    let coord = vec2<i32>(gid.xy);
    let hdr_color = textureLoad(bloom_input, coord, 0);
    let bloom_color = textureLoad(bloom_output, coord, 0);

    // Additive blend of bloom onto HDR.
    let final_color = hdr_color + bloom_color * config.strength;

    textureStore(bloom_output, coord.xy, final_color);
}
