// Lumas Fur Shell Shader
//
// Renders fur as concentric shells displaced along the vertex normal.
// Shell index is passed via push constants so LOD (shell count) can vary
// per-draw without rebinding uniforms.
//
// Push constants:
//   shell_index: u32  — current shell layer (0 = base, num_shells-1 = tip)
//   num_shells: u32   — total shell count (24 = full, 16 = medium, 8 = low)
//   fur_length: f32   — maximum fur displacement in world units
//
// Bind groups:
//   @group(0) @binding(0) — CameraUBO  (uniform, VERTEX | FRAGMENT)
//   @group(1) @binding(0) — BoneMatrices (uniform, VERTEX)
//   @group(3)             — Material textures (FRAGMENT):
//     binding 0: albedo texture
//     binding 1: fur_density texture (r = density, g = direction bias)
//     binding 2: sampler

// ──────────────────────────────────────────────
// Push Constants
// ──────────────────────────────────────────────

struct FurPushConstants {
    shell_index: u32,
    num_shells: u32,
    fur_length: f32,
    _padding: u32,
}
var<push_constant> push: FurPushConstants;

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

struct BoneMatrices {
    matrices: array<mat4x4<f32>, 96>,
};

@group(1) @binding(0)
var<uniform> bone_matrices: BoneMatrices;

// ──────────────────────────────────────────────
// Vertex Inputs / Outputs
// ──────────────────────────────────────────────

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) bone_indices: vec4<u32>,
    @location(5) bone_weights: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) shell_t: f32,
    @location(3) world_normal: vec3<f32>,
};

// ──────────────────────────────────────────────
// Vertex Shader
// ──────────────────────────────────────────────

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    // Skin position (same as character shader).
    var skinned_pos = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var skinned_normal = vec3<f32>(0.0, 0.0, 0.0);

    for (var i = 0u; i < 4u; i = i + 1u) {
        let bone_idx = min(input.bone_indices[i], 95u);
        let weight = input.bone_weights[i];
        if weight < 0.001 { continue; }

        let bone_mat = bone_matrices.matrices[bone_idx];
        skinned_pos = skinned_pos + bone_mat * vec4<f32>(input.position, 1.0) * weight;

        let normal_mat = mat3x3<f32>(
            bone_mat[0].xyz, bone_mat[1].xyz, bone_mat[2].xyz
        );
        skinned_normal = skinned_normal + normal_mat * input.normal * weight;
    }

    let total_weight = input.bone_weights[0] + input.bone_weights[1]
                     + input.bone_weights[2] + input.bone_weights[3];
    if total_weight < 0.001 {
        skinned_pos = vec4<f32>(input.position, 1.0);
        skinned_normal = input.normal;
    }

    let world_normal = normalize(skinned_normal);

    // Displace vertex along the surface normal for shell layering.
    // shell_t = 0 at the skin surface, 1 at the fur tips.
    // Use lerp: displacement = fur_length * (shell_index / num_shells)
    let shell_t = f32(push.shell_index) / f32(max(push.num_shells, 1u));
    let displacement = push.fur_length * shell_t;
    let world_pos = skinned_pos.xyz + world_normal * displacement;

    var output: VertexOutput;
    output.clip_pos = camera.view_proj * vec4<f32>(world_pos, 1.0);
    output.world_pos = world_pos;
    output.uv = input.uv;
    output.shell_t = shell_t;
    output.world_normal = world_normal;
    return output;
}

// ──────────────────────────────────────────────
// Fragment Shader
// ──────────────────────────────────────────────

// Material textures at group(3).
@group(3) @binding(0)
var albedo_texture: texture_2d<f32>;
@group(3) @binding(1)
var fur_density_texture: texture_2d<f32>;
@group(3) @binding(2)
var fur_sampler: sampler;

struct FragmentInput {
    @location(0) world_pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) shell_t: f32,
    @location(3) world_normal: vec3<f32>,
};

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    // Sample fur density texture.
    //   r channel = fur coverage density (0 = no fur, 1 = full fur)
    let fur_sample = textureSample(fur_density_texture, fur_sampler, input.uv);
    let fur_density = fur_sample.r;

    // Discard fragments where fur density is below the shell threshold.
    // This creates the tapered look: fewer fragments survive at the tips.
    if fur_density < shell_t * 0.95 {
        discard;
    }

    // Sample the base body albedo for fur color.
    let albedo_sample = textureSample(albedo_texture, fur_sampler, input.uv);
    let base_color = albedo_sample.rgb;
    let alpha = albedo_sample.a;

    // Fur gets darker and more transparent toward the tips.
    let tip_fade = 1.0 - shell_t;

    // Ambient lighting for fur (simplified — no full PBR for fur).
    // Use view-space normal for simple hemispherical lighting.
    let V = normalize(camera.camera_pos.xyz - input.world_pos);
    let N = normalize(input.world_normal);
    let n_dot_v = max(dot(N, V), 0.0);

    // Simple directional light from top-down.
    let simple_light = 0.3 + 0.5 * max(dot(N, vec3(0.0, 1.0, 0.0)), 0.0);

    // Fur color: base body color, slightly desaturated at tips.
    let color = base_color * simple_light * (0.7 + 0.3 * tip_fade);

    // Alpha: fur becomes more transparent at tips.
    let fur_alpha = alpha * tip_fade * min(fur_density + 0.1, 1.0);

    // Output pre-multiplied alpha.
    return vec4(color * fur_alpha, fur_alpha);
}
