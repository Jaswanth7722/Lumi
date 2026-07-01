// Lumas Post-Process Shader
//
// Fullscreen triangle pass implementing:
//   1. ACES filmic tonemapping (Hill 2016 polynomial approximation)
//   2. sRGB gamma correction (linear → sRGB)
//   3. Bloom composite (additive blend)
//   4. FXAA anti-aliasing (Lottes 3.11 PC Quality algorithm)
//
// Uses a fullscreen triangle (no vertex buffer needed — vertex positions
// are computed from @builtin(vertex_index)).
//
// Bind groups:
//   @group(0) @binding(0) — PostProcessConfig (uniform)
//   @group(0) @binding(1) — HDR color input texture
//   @group(0) @binding(2) — Bloom input texture (optional — black if no bloom)
//   @group(0) @binding(3) — Sampler

// ──────────────────────────────────────────────
// Uniforms
// ──────────────────────────────────────────────

struct PostProcessConfig {
    bloom_strength: f32,
    bloom_enabled: f32,       // 0.0 or 1.0
    fxaa_enabled: f32,        // 0.0 or 1.0
    gamma: f32,               // Default: 2.2
    exposure: f32,            // Default: 1.0
    fxaa_subpixel_quality: f32,  // Default: 0.75
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0)
var<uniform> config: PostProcessConfig;

@group(0) @binding(1)
var hdr_texture: texture_2d<f32>;
@group(0) @binding(2)
var bloom_texture: texture_2d<f32>;
@group(0) @binding(3)
var pp_sampler: sampler;

// ──────────────────────────────────────────────
// Vertex Shader — Fullscreen Triangle
// ──────────────────────────────────────────────

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Generate a fullscreen triangle covering clip-space [-1, 1].
// No vertex buffer needed — compute from vertex index.
@vertex
fn vs_main(@builtin(vertex_index) vertex_id: u32) -> VertexOutput {
    // Fullscreen triangle: 3 vertices cover the entire viewport.
    // UV goes from (0,0) at bottom-left to (1,1) at top-right.
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

// ──────────────────────────────────────────────
// Fragment Shader — Tone Mapping + FXAA
// ──────────────────────────────────────────────

struct FragmentInput {
    @location(0) uv: vec2<f32>,
};

// ── ACES Filmic Tonemapping (Hill 2016) ──

fn aces_filmic(color: vec3<f32>) -> vec3<f32> {
    // ACES approximation by Stephen Hill.
    // https://github.com/TheRealMJP/BakingLab/blob/master/BakingLab/ACES.hlsl
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return saturate((color * (a * color + b)) / (color * (c * color + d) + e));
}

// ── sRGB Gamma Correction ──

fn linear_to_srgb(color: vec3<f32>) -> vec3<f32> {
    // Standard sRGB conversion with linear segment for low values.
    let cutoff = 0.0031308;
    return select(
        1.055 * pow(color, vec3(1.0 / 2.4)) - 0.055,  // Gamma curve
        12.92 * color,                                   // Linear segment
        color < vec3(cutoff)
    );
}

// ── Lumasnance ──

fn luminance(color: vec3<f32>) -> f32 {
    return dot(color, vec3(0.2126, 0.7152, 0.0722));
}

// ── FXAA 3.11 PC Quality (Timothy Lottes) ──

fn fxaa_run(uv: vec2<f32>, color: ptr<function, vec4<f32>>) -> bool {
    if config.fxaa_enabled < 0.5 {
        return false;
    }

    let tex_dims = vec2<f32>(textureDimensions(hdr_texture));
    let rcp_dims = 1.0 / tex_dims;

    // Sample neighborhood for edge detection.
    let luma_center = luminance((*color).rgb);
    let luma_tl = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(-1.0, -1.0) * rcp_dims, 0.0).rgb);
    let luma_tr = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(1.0, -1.0) * rcp_dims, 0.0).rgb);
    let luma_bl = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(-1.0, 1.0) * rcp_dims, 0.0).rgb);
    let luma_br = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(1.0, 1.0) * rcp_dims, 0.0).rgb);

    // Compute luma extents.
    let luma_min = min(luma_center, min(min(luma_tl, luma_tr), min(luma_bl, luma_br)));
    let luma_max = max(luma_center, max(max(luma_tl, luma_tr), max(luma_bl, luma_br)));

    let luma_range = luma_max - luma_min;
    if luma_range < max(0.0312, 0.0312) {  // Skip if little contrast
        return false;
    }

    // Edge direction detection.
    let luma_up = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(-1.0, 0.0) * rcp_dims, 0.0).rgb);
    let luma_down = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(1.0, 0.0) * rcp_dims, 0.0).rgb);
    let luma_left = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(0.0, -1.0) * rcp_dims, 0.0).rgb);
    let luma_right = luminance(textureSampleLevel(hdr_texture, pp_sampler, uv + vec2(0.0, 1.0) * rcp_dims, 0.0).rgb);

    // Horizontal vs vertical edge detection.
    let edge_h = abs(luma_up - luma_down) + abs(luma_tl - luma_bl) + abs(luma_tr - luma_br);
    let edge_v = abs(luma_left - luma_right) + abs(luma_tl - luma_tr) + abs(luma_bl - luma_br);

    // Determine blend direction.
    var blend_dir = vec2(0.0, 0.0);
    if edge_h > edge_v {
        blend_dir.x = 1.0;
    } else {
        blend_dir.y = 1.0;
    }

    // Sub-pixel shift estimation.
    let luma_avg = (luma_center + luma_up + luma_down + luma_left + luma_right) / 5.0;
    let subpixel_shift = clamp(abs(luma_center - luma_avg) / max(luma_range, 0.0001), 0.0, 0.5);

    // Compute final blend offset.
    let offset = blend_dir * rcp_dims * (0.5 + config.fxaa_subpixel_quality * subpixel_shift);

    // Sample two points along the edge, average them.
    let sample1 = textureSampleLevel(hdr_texture, pp_sampler, uv + offset * (1.0 / 3.0 - 0.5), 0.0);
    let sample2 = textureSampleLevel(hdr_texture, pp_sampler, uv + offset * (2.0 / 3.0 - 0.5), 0.0);

    *color = (sample1 + sample2) * 0.5;

    return true;
}

// ──────────────────────────────────────────────
// Fragment Main
// ──────────────────────────────────────────────

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    // Sample HDR color.
    var color = textureSampleLevel(hdr_texture, pp_sampler, input.uv, 0.0);

    // Apply FXAA if enabled.
    fxaa_run(input.uv, &color);

    // Bloom composite: add bloom texture.
    if config.bloom_enabled > 0.5 {
        let bloom = textureSampleLevel(bloom_texture, pp_sampler, input.uv, 0.0);
        color = color + bloom * config.bloom_strength;
    }

    // Apply exposure.
    color.rgb = color.rgb * config.exposure;

    // ACES filmic tonemapping.
    let tonemapped = aces_filmic(color.rgb);

    // sRGB gamma correction.
    let srgb = linear_to_srgb(tonemapped);

    // Copy alpha through (pre-multiplied).
    return vec4(srgb, color.a);
}
