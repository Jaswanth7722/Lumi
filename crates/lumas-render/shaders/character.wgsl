// Lumas PBR Character Shader
//
// Vertex: GPU skinning via bone matrices (up to 96 bones).
// Fragment: Cook-Torrance BRDF (GGX distribution, Smith geometry, Fresnel-Schlick),
//           spherical harmonics ambient, directional + point lights.
// Output: Linear HDR color in pre-multiplied alpha format.
//
// Bind group layouts:
//   @group(0) @binding(0) — CameraUBO  (uniform buffer, VERTEX | FRAGMENT)
//   @group(1) @binding(0) — BoneMatrices (uniform buffer, VERTEX only)
//   @group(2) @binding(0) — LightingUBO (uniform buffer, FRAGMENT only)
//   @group(3)             — Material textures + sampler (FRAGMENT only)
//     FurBody:        binding 0-4 = textures (albedo, normal, roughness, ao, fur),
//                     binding 5 = sampler
//     CrystalEmissive: binding 0-2 = textures (albedo, emission, noise),
//                     binding 3 = sampler
//     Other materials: binding 0..N = textures, binding N = sampler

// ──────────────────────────────────────────────
// Uniforms
// ──────────────────────────────────────────────

struct CameraUBO {
    view_proj: mat4x4<f32>,       // @offset(0) @size(64)
    view: mat4x4<f32>,             // @offset(64) @size(64)
    proj: mat4x4<f32>,             // @offset(128) @size(64)
    camera_pos: vec4<f32>,         // @offset(192) @size(16)  — xyz = world position, w = 1.0
    viewport_size: vec2<f32>,      // @offset(208) @size(8)
    time_seconds: f32,             // @offset(216) @size(4)
    _padding: f32,                 // @offset(220) @size(4)
};  // @size(224)

@group(0) @binding(0)
var<uniform> camera: CameraUBO;

struct BoneMatrices {
    matrices: array<mat4x4<f32>, 96>,
};

@group(1) @binding(0)
var<uniform> bone_matrices: BoneMatrices;

// --- Lighting ---

struct DirectionalLightGPU {
    direction: vec4<f32>,
    color: vec4<f32>,
};

struct PointLightGPU {
    position: vec4<f32>,
    color: vec4<f32>,
    range: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
};

struct LightingUBO {
    ambient_sh: array<vec4<f32>, 7>,        // SH coefficients, padded to vec4
    directional: array<DirectionalLightGPU, 1>,
    point_lights: array<PointLightGPU, 4>,
    point_light_count: u32,
    _pad: array<u32, 3>,
};

@group(2) @binding(0)
var<uniform> lighting: LightingUBO;

// ──────────────────────────────────────────────
// Vertex Inputs
// ──────────────────────────────────────────────

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,      // .xyz = tangent, .w = sign
    @location(3) uv: vec2<f32>,
    @location(4) bone_indices: vec4<u32>,
    @location(5) bone_weights: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) world_tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
};

// ──────────────────────────────────────────────
// Vertex Shader — GPU Skinning
// ──────────────────────────────────────────────

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    // Skin position: transform by weighted bone matrices.
    var skinned_pos = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var skinned_normal = vec3<f32>(0.0, 0.0, 0.0);
    var skinned_tangent = vec3<f32>(0.0, 0.0, 0.0);

    // Accumulate contributions from up to 4 bone influences.
    for (var i = 0u; i < 4u; i = i + 1u) {
        let bone_idx = min(input.bone_indices[i], 95u);  // Clamp to prevent OOB.
        let weight = input.bone_weights[i];
        if weight < 0.001 {
            continue;
        }

        let bone_mat = bone_matrices.matrices[bone_idx];
        skinned_pos = skinned_pos + bone_mat * vec4<f32>(input.position, 1.0) * weight;

        // Skin normal: transform by inverse-transpose (upper 3x3 of bone matrix).
        let normal_mat = mat3x3<f32>(
            bone_mat[0].xyz, bone_mat[1].xyz, bone_mat[2].xyz
        );
        skinned_normal = skinned_normal + normal_mat * input.normal * weight;
        skinned_tangent = skinned_tangent + normal_mat * input.tangent.xyz * weight;
    }

    // If no bone weights (unskinned mesh), use identity transform.
    let total_weight = input.bone_weights[0] + input.bone_weights[1]
                     + input.bone_weights[2] + input.bone_weights[3];
    if total_weight < 0.001 {
        skinned_pos = vec4<f32>(input.position, 1.0);
        skinned_normal = input.normal;
        skinned_tangent = input.tangent.xyz;
    }

    // Transform to world space (model matrix is identity — Lumas's character
    // is positioned in world space directly via the camera).
    let world_pos = skinned_pos.xyz;
    let world_normal = normalize(skinned_normal);
    let world_tangent = normalize(skinned_tangent);

    var output: VertexOutput;
    output.clip_pos = camera.view_proj * vec4<f32>(world_pos, 1.0);
    output.world_pos = world_pos;
    output.world_normal = world_normal;
    output.world_tangent = vec4<f32>(world_tangent, input.tangent.w);
    output.uv = input.uv;
    return output;
}

// ──────────────────────────────────────────────
// Fragment Shader — Cook-Torrance PBR BRDF
// ──────────────────────────────────────────────

// Material texture bindings — these are overridden by the material bind group.
// The character/fur pipeline uses 5 textures + 1 sampler at group(3).
@group(3) @binding(0)
var albedo_texture: texture_2d<f32>;
@group(3) @binding(1)
var normal_texture: texture_2d<f32>;
@group(3) @binding(2)
var roughness_texture: texture_2d<f32>;
@group(3) @binding(3)
var ao_texture: texture_2d<f32>;
@group(3) @binding(4)
var fur_density_texture: texture_2d<f32>;
@group(3) @binding(5)
var material_sampler: sampler;

struct FragmentInput {
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) world_tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
};

// ── PBR Constants ──

const PI: f32 = 3.14159265358979323846;

// ── PBR Helper Functions ──

// GGX normal distribution function (Trowbridge-Reitz).
fn ggx_distribution(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let n_dot_h2 = n_dot_h * n_dot_h;
    let denom = n_dot_h2 * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom);
}

// Smith geometry function (Schlick-GGX approximation).
fn smith_geometry(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let k = (a + 1.0) * (a + 1.0) / 8.0;

    let g_v = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let g_l = n_dot_l / (n_dot_l * (1.0 - k) + k);
    return g_v * g_l;
}

// Fresnel-Schlick approximation.
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
}

// Sample the normal texture and decode tangent-space normal.
fn sample_normal(uv: vec2<f32>, world_normal: vec3<f32>, world_tangent: vec4<f32>) -> vec3<f32> {
    let tangent_normal = textureSample(normal_texture, material_sampler, uv).rgb;
    let decoded_normal = normalize(tangent_normal * 2.0 - 1.0);

    // Build TBN matrix.
    let T = normalize(world_tangent.xyz);
    let B = normalize(cross(world_normal, T) * world_tangent.w);
    let N = normalize(world_normal);

    // Transform normal from tangent to world space.
    return normalize(decoded_normal.x * T + decoded_normal.y * B + decoded_normal.z * N);
}

// Evaluate spherical harmonics for ambient lighting.
fn eval_sh_ambient(normal: vec3<f32>) -> vec3<f32> {
    // L0 band (DC term).
    var color = lighting.ambient_sh[0].rgb;  // L0 * 0.5 * sqrt(1/PI) factor is baked in.

    // L1 band: 3 directional terms.
    // SH basis functions: Y_1^-1 = y, Y_1^0 = z, Y_1^1 = x
    let n = normalize(normal);
    color = color
        + lighting.ambient_sh[1].rgb * n.y  // Y_1^-1 term
        + lighting.ambient_sh[2].rgb * n.z  // Y_1^0 term
        + lighting.ambient_sh[3].rgb * n.x; // Y_1^1 term

    return max(color, vec3(0.0));
}

// Evaluate a single point light contribution.
fn eval_point_light(
    light_pos: vec3<f32>,
    light_color: vec3<f32>,
    range: f32,
    frag_pos: vec3<f32>,
    normal: vec3<f32>,
    view_dir: vec3<f32>,
    f0: vec3<f32>,
    roughness: f32,
    metallic: f32,
    albedo: vec3<f32>,
) -> vec3<f32> {
    let light_vec = light_pos - frag_pos;
    let distance = length(light_vec);
    if distance > range {
        return vec3(0.0);
    }
    let light_dir = normalize(light_vec);
    let half_vec = normalize(light_dir + view_dir);

    let n_dot_l = max(dot(normal, light_dir), 0.0);
    let n_dot_v = max(dot(normal, view_dir), 0.001);
    let n_dot_h = max(dot(normal, half_vec), 0.001);

    // Inverse square falloff.
    let attenuation = 1.0 / (distance * distance + 0.01);

    let radiance = light_color * attenuation;

    // Cook-Torrance BRDF.
    let NDF = ggx_distribution(n_dot_h, roughness);
    let G = smith_geometry(n_dot_v, n_dot_l, roughness);
    let F = fresnel_schlick(max(dot(half_vec, view_dir), 0.0), f0);

    let k_s = F;
    let k_d = (1.0 - k_s) * (1.0 - metallic);

    let specular = (NDF * G * F) / max(4.0 * n_dot_v * n_dot_l, 0.001);
    let diffuse = k_d * albedo / PI;

    return (diffuse + specular) * radiance * n_dot_l;
}

// Evaluate the directional light contribution.
fn eval_directional(
    normal: vec3<f32>,
    view_dir: vec3<f32>,
    f0: vec3<f32>,
    roughness: f32,
    metallic: f32,
    albedo: vec3<f32>,
) -> vec3<f32> {
    let light_dir = normalize(-lighting.directional[0].direction.xyz);
    let half_vec = normalize(light_dir + view_dir);

    let n_dot_l = max(dot(normal, light_dir), 0.0);
    let n_dot_v = max(dot(normal, view_dir), 0.001);
    let n_dot_h = max(dot(normal, half_vec), 0.001);

    let radiance = lighting.directional[0].color.rgb;  // Pre-multiplied by intensity.

    let NDF = ggx_distribution(n_dot_h, roughness);
    let G = smith_geometry(n_dot_v, n_dot_l, roughness);
    let F = fresnel_schlick(max(dot(half_vec, view_dir), 0.0), f0);

    let k_s = F;
    let k_d = (1.0 - k_s) * (1.0 - metallic);

    let specular = (NDF * G * F) / max(4.0 * n_dot_v * n_dot_l, 0.001);
    let diffuse = k_d * albedo / PI;

    return (diffuse + specular) * radiance * n_dot_l;
}

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    // Sample material textures.
    let albedo_sample = textureSample(albedo_texture, material_sampler, input.uv);
    let roughness_texel = textureSample(roughness_texture, material_sampler, input.uv);
    let ao_sample = textureSample(ao_texture, material_sampler, input.uv).r;

    // Decode material parameters.
    let albedo = albedo_sample.rgb;
    let alpha = albedo_sample.a;  // Pre-multiplied alpha — color already multiplied by alpha.

    // Packed ORM texture: roughness in R, metallic in G.
    let roughness = max(roughness_texel.r, 0.04);  // Clamp to avoid division issues.
    let metallic = roughness_texel.g;

    // F0 for dielectrics (0.04) vs. metals (albedo).
    let f0 = mix(vec3(0.04), albedo, metallic);

    // Decode world-space normal from normal map.
    let N = sample_normal(input.uv, input.world_normal, input.world_tangent);

    // View direction.
    let V = normalize(camera.camera_pos.xyz - input.world_pos);

    // --- Accumulate lighting ---

    // Ambient (spherical harmonics + AO).
    var color = eval_sh_ambient(N) * ao_sample * albedo;

    // Directional light.
    color = color + eval_directional(N, V, f0, roughness, metallic, albedo);

    // Point lights (up to 4).
    for (var i = 0u; i < min(lighting.point_light_count, 4u); i = i + 1u) {
        let pl = lighting.point_lights[i];
        color = color + eval_point_light(
            pl.position.xyz,
            pl.color.rgb,
            pl.range,
            input.world_pos,
            N,
            V,
            f0,
            roughness,
            metallic,
            albedo,
        );
    }

    // Add a subtle rim light for edge definition (useful against desktop backgrounds).
    let rim_dot = 1.0 - max(dot(N, V), 0.0);
    let rim = pow(rim_dot, 3.0) * vec3(0.3, 0.35, 0.4) * (1.0 - roughness * 0.5);
    color = color + rim;

    // Output pre-multiplied alpha: color.rgb already multiplied by alpha from
    // the albedo texture (which stores pre-multiplied values). Final output
    // must have alpha = original alpha for compositing.
    return vec4(color * alpha, alpha);
}
