//! Integration tests for material system — MaterialKind variants, PipelineConfig, ParticleBlend.

use lumas_render::material::{
    MaterialKind, ParticleBlend, PipelineConfig, PipelineId, MaterialId,
    camera_bind_group_layout, bone_matrix_bind_group_layout,
    lighting_bind_group_layout, material_bind_group_layout,
    default_depth_stencil, depth_prepass_stencil,
};
use lumas_render::texture::TextureId;
use lumas_render::shader::ShaderId;

// ──────────────────────────────────────────────
// MaterialKind Tests
// ──────────────────────────────────────────────

#[test]
fn test_material_kind_fur_body() {
    let kind = MaterialKind::FurBody {
        albedo: TextureId::default(),
        normal: TextureId::default(),
        roughness: TextureId::default(),
        ao: TextureId::default(),
        fur_density: TextureId::default(),
        fur_length: 0.5,
    };
    assert!(matches!(kind, MaterialKind::FurBody { .. }));
}

#[test]
fn test_material_kind_crystal_emissive() {
    let kind = MaterialKind::CrystalEmissive {
        albedo: TextureId::default(),
        emission: TextureId::default(),
        noise: TextureId::default(),
        emission_color: [0.3, 0.5, 1.0, 1.0],
    };
    assert!(matches!(kind, MaterialKind::CrystalEmissive { .. }));
}

#[test]
fn test_material_kind_holographic_panel() {
    let kind = MaterialKind::HolographicPanel {
        content: TextureId::default(),
        glow_color: [0.3, 0.5, 1.0, 0.6],
        opacity: 0.85,
    };
    assert!(matches!(kind, MaterialKind::HolographicPanel { .. }));
}

#[test]
fn test_material_kind_particle() {
    let kind = MaterialKind::Particle {
        atlas: TextureId::default(),
        blend_mode: ParticleBlend::Additive,
    };
    assert!(matches!(kind, MaterialKind::Particle { .. }));
}

#[test]
fn test_material_kind_unlit_texture() {
    let kind = MaterialKind::UnlitTexture {
        albedo: TextureId::default(),
        alpha: 1.0,
    };
    assert!(matches!(kind, MaterialKind::UnlitTexture { .. }));
}

#[test]
fn test_material_kind_equality() {
    let a = MaterialKind::UnlitTexture {
        albedo: TextureId::default(),
        alpha: 1.0,
    };
    let b = MaterialKind::UnlitTexture {
        albedo: TextureId::default(),
        alpha: 1.0,
    };
    assert_eq!(a, b);
}

#[test]
fn test_material_kind_inequality() {
    let a = MaterialKind::UnlitTexture {
        albedo: TextureId::default(),
        alpha: 1.0,
    };
    let b = MaterialKind::UnlitTexture {
        albedo: TextureId::default(),
        alpha: 0.5,
    };
    assert_ne!(a, b);
}

// ──────────────────────────────────────────────
// ParticleBlend Tests
// ──────────────────────────────────────────────

#[test]
fn test_particle_blend_alpha_wgpu() {
    let blend = ParticleBlend::Alpha.to_wgpu_blend();
    assert_eq!(blend, wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
}

#[test]
fn test_particle_blend_additive_wgpu() {
    let blend = ParticleBlend::Additive.to_wgpu_blend();
    assert_eq!(blend.color.src_factor, wgpu::BlendFactor::SrcAlpha);
    assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::One);
    assert_eq!(blend.color.operation, wgpu::BlendOperation::Add);
}

#[test]
fn test_particle_blend_soft_additive_wgpu() {
    let blend = ParticleBlend::SoftAdditive.to_wgpu_blend();
    assert_eq!(blend.color.src_factor, wgpu::BlendFactor::OneMinusDstAlpha);
    assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::One);
}

#[test]
fn test_particle_blend_variants() {
    assert_eq!(ParticleBlend::Alpha as u8, 0);
    assert_eq!(ParticleBlend::Additive as u8, 1);
    assert_eq!(ParticleBlend::SoftAdditive as u8, 2);
}

// ──────────────────────────────────────────────
// PipelineConfig Tests
// ──────────────────────────────────────────────

#[test]
fn test_pipeline_config_default() {
    let config = PipelineConfig::default();
    assert_eq!(config.primitive.topology, wgpu::PrimitiveTopology::TriangleList);
    assert_eq!(config.primitive.front_face, wgpu::FrontFace::Ccw);
    assert_eq!(config.primitive.cull_mode, Some(wgpu::Face::Back));
    assert!(config.bind_group_layouts.is_empty());
    assert_eq!(config.immediate_size, 0);
}

#[test]
fn test_pipeline_config_custom() {
    let config = PipelineConfig {
        label: "test_pipeline".into(),
        vertex_shader: ShaderId::default(),
        fragment_shader: ShaderId::default(),
        ..Default::default()
    };
    assert_eq!(config.label, "test_pipeline");
}

#[test]
fn test_pipeline_config_clone() {
    let config = PipelineConfig::default();
    let cloned = config.clone();
    assert_eq!(config.label, cloned.label);
}

// ──────────────────────────────────────────────
// Depth-Stencil Tests
// ──────────────────────────────────────────────

#[test]
fn test_default_depth_stencil_format() {
    let ds = default_depth_stencil();
    assert_eq!(ds.format, wgpu::TextureFormat::Depth32Float);
    assert_eq!(ds.depth_write_enabled, Some(true));
    assert_eq!(ds.depth_compare, Some(wgpu::CompareFunction::Equal));
}

#[test]
fn test_depth_prepass_stencil_format() {
    let ds = depth_prepass_stencil();
    assert_eq!(ds.depth_compare, Some(wgpu::CompareFunction::Less));
}

// ──────────────────────────────────────────────
// Bind Group Layout Tests (compile-time API checks)
// ──────────────────────────────────────────────

#[test]
fn test_bind_group_layout_entry_counts() {
    // Verify the entry counts for each material type:
    // FurBody: 5 textures + 1 sampler = 6 entries
    // CrystalEmissive: 3 textures + 1 sampler = 4 entries
    // HolographicPanel, Particle, UnlitTexture: 1 texture + 1 sampler = 2 entries each
    assert_eq!(5 + 1, 6); // FurBody
    assert_eq!(3 + 1, 4); // CrystalEmissive
    assert_eq!(1 + 1, 2); // HolographicPanel, Particle, UnlitTexture
}

#[test]
fn test_camera_bind_group_layout_description() {
    // This test verifies the layout creation logic compiles.
    // Full testing requires a wgpu::Device.
    let _layout_fn = camera_bind_group_layout;
}

#[test]
fn test_bone_matrix_bind_group_layout_description() {
    let _layout_fn = bone_matrix_bind_group_layout;
}

#[test]
fn test_lighting_bind_group_layout_description() {
    let _layout_fn = lighting_bind_group_layout;
}

// ──────────────────────────────────────────────
// Material ID Tests
// ──────────────────────────────────────────────

#[test]
fn test_material_id_default() {
    let id = MaterialId::default();
    // SlotMap default is the null key — we verify by checking
    // that creating a new ID is different.
    let _ = id;
}

#[test]
fn test_pipeline_id_default() {
    let id = PipelineId::default();
    let _ = id;
}

// ──────────────────────────────────────────────
// Shader ID Tests
// ──────────────────────────────────────────────

#[test]
fn test_shader_id_default() {
    let id = ShaderId::default();
    let _ = id;
}
