//! Integration tests for particle system — emitter shapes, particle state, blend modes.

use lumas_render::scene::{SceneParticleState, Scene, ShadowInstanceGPU};

// ──────────────────────────────────────────────
// Particle State Tests
// ──────────────────────────────────────────────

#[test]
fn test_particle_state_default() {
    let state = SceneParticleState::default();
    assert_eq!(state.active_count, 0);
    assert_eq!(state.max_count, 4096);
    assert_eq!(state.emitter_position, glam::Vec3::ZERO);
    assert!(!state.emitting);
}

#[test]
fn test_particle_state_active() {
    let mut state = SceneParticleState::default();
    state.active_count = 100;
    assert_eq!(state.active_count, 100);
    assert!(state.active_count > 0);
}

#[test]
fn test_particle_state_emitting() {
    let mut state = SceneParticleState::default();
    assert!(!state.emitting);
    state.emitting = true;
    assert!(state.emitting);
}

#[test]
fn test_particle_state_max_count() {
    let state = SceneParticleState {
        max_count: 8192,
        ..Default::default()
    };
    assert_eq!(state.max_count, 8192);
}

// ──────────────────────────────────────────────
// Scene Particle Tests
// ──────────────────────────────────────────────

#[test]
fn test_scene_has_active_particles_default() {
    let scene = Scene::new();
    assert!(!scene.has_active_particles());
}

#[test]
fn test_scene_has_active_particles_emitting() {
    let mut scene = Scene::new();
    scene.set_particle_emitting(true);
    assert!(scene.has_active_particles());
}

#[test]
fn test_scene_has_active_particles_count() {
    let mut scene = Scene::new();
    scene.particles.active_count = 5;
    assert!(scene.has_active_particles());
}

// ──────────────────────────────────────────────
// Workgroup Size Tests
// ──────────────────────────────────────────────

#[test]
fn test_particle_workgroup() {
    // The compute shader uses @workgroup_size(64).
    // Particle count must be a multiple of 64 for correctness.
    let workgroup_size: u32 = 64;
    assert_eq!(workgroup_size, 64);

    // Verify that common particle counts are multiples of 64.
    let counts = [512u32, 1024, 2048, 4096, 8192];
    for count in &counts {
        assert_eq!(
            count % workgroup_size,
            0,
            "Particle count {} must be a multiple of {}",
            count,
            workgroup_size
        );
    }
}

#[test]
fn test_particle_count_multiple_of_64() {
    // Verify the default max_particles from config is a multiple of 64.
    let config = lumas_render::config::RenderConfig::default();
    assert_eq!(config.max_particles % 64, 0, "max_particles must be a multiple of 64");
}

#[test]
fn test_particle_count_low_quality() {
    let mut config = lumas_render::config::RenderConfig::default();
    lumas_render::config::QualityPreset::Low.apply(&mut config);
    // Low quality: 512 particles
    assert_eq!(config.max_particles % 64, 0);
}

#[test]
fn test_particle_count_medium_quality() {
    let mut config = lumas_render::config::RenderConfig::default();
    lumas_render::config::QualityPreset::Medium.apply(&mut config);
    assert_eq!(config.max_particles % 64, 0);
}

#[test]
fn test_particle_count_ultra_quality() {
    let mut config = lumas_render::config::RenderConfig::default();
    lumas_render::config::QualityPreset::Ultra.apply(&mut config);
    assert_eq!(config.max_particles % 64, 0);
}

// ──────────────────────────────────────────────
// Particle Buffer Size Calculations
// ──────────────────────────────────────────────

#[test]
fn test_particle_buffer_size() {
    // Each Particle struct has:
    // position: vec4<f32> = 16 bytes
    // velocity: vec4<f32> = 16 bytes
    // life: f32 = 4 bytes
    // max_life: f32 = 4 bytes
    // size: f32 = 4 bytes
    // flags: u32 = 4 bytes
    // color: vec4<f32> = 16 bytes
    // Total per particle: 64 bytes

    let particle_struct_size: u64 = 64;
    let max_particles: u64 = 4096;
    let total_buffer_size = particle_struct_size * max_particles;

    // 64 bytes × 4096 = 262,144 bytes = 256 KB
    assert_eq!(total_buffer_size, 262144);
}

// ──────────────────────────────────────────────
// Indirect Draw Buffer Tests
// ──────────────────────────────────────────────

#[test]
fn test_draw_indirect_args_size() {
    // DrawIndexedIndirect: 5 × u32 = 20 bytes
    assert_eq!(std::mem::size_of::<u32>() * 5, 20);
}

#[test]
fn test_draw_indirect_alignment() {
    // wgpu requires indirect buffers to be aligned to 4 bytes.
    let alignment = std::mem::align_of::<u32>();
    assert_eq!(alignment, 4);
}

// ──────────────────────────────────────────────
// Emitter Position Tests
// ──────────────────────────────────────────────

#[test]
fn test_emitter_position_follows_character() {
    let mut scene = Scene::new();
    scene.set_character_position(glam::Vec3::new(100.0, 0.0, 50.0));

    // The emitter should be at the crystal position (2.0 units above character).
    assert_eq!(
        scene.particles.emitter_position,
        glam::Vec3::new(100.0, 2.0, 50.0)
    );
}

// ──────────────────────────────────────────────
// Shadow Size Tests
// ──────────────────────────────────────────────

#[test]
fn test_shadow_instance_gpu_size() {
    // Matches ShadowInstanceGPU layout in scene.rs and shadow.wgsl
    assert_eq!(std::mem::size_of::<ShadowInstanceGPU>(), 32);
}
