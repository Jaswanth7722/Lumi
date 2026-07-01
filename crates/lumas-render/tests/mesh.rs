//! Integration tests for mesh system — GpuMesh creation, LOD selection, vertex layout.

use lumas_render::mesh::{CharacterVertex, LodLevel, MeshId, Aabb};
use lumas_render::scene::{MAX_BONES, BoneMatrices, ShadowInstanceGPU};

// ──────────────────────────────────────────────
// Mesh ID Tests
// ──────────────────────────────────────────────

#[test]
fn test_mesh_id_creation() {
    let id = MeshId(42);
    assert_eq!(id.0, 42);
}

#[test]
fn test_mesh_id_equality() {
    assert_eq!(MeshId(0), MeshId(0));
    assert_ne!(MeshId(0), MeshId(1));
}

// ──────────────────────────────────────────────
// AABB Tests
// ──────────────────────────────────────────────

#[test]
fn test_aabb_creation() {
    let aabb = Aabb::new(
        glam::Vec3::new(-1.0, -1.0, -1.0),
        glam::Vec3::new(1.0, 1.0, 1.0),
    );
    assert_eq!(aabb.min.x, -1.0);
    assert_eq!(aabb.max.x, 1.0);
}

// ──────────────────────────────────────────────
// Character Vertex Tests
// ──────────────────────────────────────────────

#[test]
fn test_character_vertex_size() {
    // position(12) + normal(12) + tangent(16) + uv(8) + bone_indices(16) + bone_weights(16) = 80
    assert_eq!(std::mem::size_of::<CharacterVertex>(), 80);
}

#[test]
fn test_character_vertex_is_bytemuck_castable() {
    let vertex = CharacterVertex {
        position: [0.0; 3],
        normal: [0.0; 3],
        tangent: [0.0; 4],
        uv: [0.0; 2],
        bone_indices: [0u32; 4],
        bone_weights: [0.0; 4],
    };
    let bytes: &[u8] = bytemuck::bytes_of(&vertex);
    assert_eq!(bytes.len(), 80);
}

#[test]
fn test_character_vertex_fields() {
    let vertex = CharacterVertex {
        position: [1.0, 2.0, 3.0],
        normal: [0.0, 1.0, 0.0],
        tangent: [1.0, 0.0, 0.0, 1.0],
        uv: [0.5, 0.5],
        bone_indices: [0, 1, 2, 3],
        bone_weights: [0.5, 0.3, 0.15, 0.05],
    };
    assert_eq!(vertex.position[0], 1.0);
    assert_eq!(vertex.normal[1], 1.0);
    assert_eq!(vertex.uv[0], 0.5);
    assert_eq!(vertex.bone_indices[0], 0);
    assert!((vertex.bone_weights[0] - 0.5).abs() < f32::EPSILON);
}

// ──────────────────────────────────────────────
// LOD Level Tests
// ──────────────────────────────────────────────

#[test]
fn test_lod_level_creation() {
    let lod = LodLevel {
        first_index: 0,
        index_count: 100,
        vertex_count: 50,
        screen_size_threshold: 200.0,
    };
    assert_eq!(lod.first_index, 0);
    assert_eq!(lod.index_count, 100);
    assert_eq!(lod.screen_size_threshold, 200.0);
}

/// Simulate LOD selection logic (mirroring GpuMesh::select_lod).
fn select_lod(lod_levels: &[LodLevel; 3], screen_height: f32) -> &LodLevel {
    if screen_height < lod_levels[2].screen_size_threshold {
        &lod_levels[2]
    } else if screen_height < lod_levels[1].screen_size_threshold {
        &lod_levels[1]
    } else {
        &lod_levels[0]
    }
}

#[test]
fn test_lod_selection_high_detail() {
    let lods = [
        LodLevel { first_index: 0, index_count: 1000, vertex_count: 500, screen_size_threshold: f32::MAX },
        LodLevel { first_index: 500, index_count: 500, vertex_count: 250, screen_size_threshold: 300.0 },
        LodLevel { first_index: 750, index_count: 250, vertex_count: 125, screen_size_threshold: 100.0 },
    ];

    // Large on screen → use LOD 0 (high detail).
    let selected = select_lod(&lods, 500.0);
    assert_eq!(selected.index_count, 1000);
}

#[test]
fn test_lod_selection_medium_detail() {
    let lods = [
        LodLevel { first_index: 0, index_count: 1000, vertex_count: 500, screen_size_threshold: f32::MAX },
        LodLevel { first_index: 500, index_count: 500, vertex_count: 250, screen_size_threshold: 300.0 },
        LodLevel { first_index: 750, index_count: 250, vertex_count: 125, screen_size_threshold: 100.0 },
    ];

    // Medium on screen → use LOD 1.
    let selected = select_lod(&lods, 200.0);
    assert_eq!(selected.index_count, 500);
}

#[test]
fn test_lod_selection_low_detail() {
    let lods = [
        LodLevel { first_index: 0, index_count: 1000, vertex_count: 500, screen_size_threshold: f32::MAX },
        LodLevel { first_index: 500, index_count: 500, vertex_count: 250, screen_size_threshold: 300.0 },
        LodLevel { first_index: 750, index_count: 250, vertex_count: 125, screen_size_threshold: 100.0 },
    ];

    // Small on screen → use LOD 2 (low detail).
    let selected = select_lod(&lods, 50.0);
    assert_eq!(selected.index_count, 250);
}

#[test]
fn test_lod_selection_boundary() {
    let lods = [
        LodLevel { first_index: 0, index_count: 1000, vertex_count: 500, screen_size_threshold: f32::MAX },
        LodLevel { first_index: 500, index_count: 500, vertex_count: 250, screen_size_threshold: 300.0 },
        LodLevel { first_index: 750, index_count: 250, vertex_count: 125, screen_size_threshold: 100.0 },
    ];

    // Exactly at medium threshold → still use LOD 0 (not less than).
    let selected = select_lod(&lods, 300.0);
    assert_eq!(selected.index_count, 1000);
}

#[test]
fn test_lod_selection_zero_height() {
    let lods = [
        LodLevel { first_index: 0, index_count: 1000, vertex_count: 500, screen_size_threshold: f32::MAX },
        LodLevel { first_index: 500, index_count: 500, vertex_count: 250, screen_size_threshold: 300.0 },
        LodLevel { first_index: 750, index_count: 250, vertex_count: 125, screen_size_threshold: 100.0 },
    ];

    // Zero height → use LOD 2.
    let selected = select_lod(&lods, 0.0);
    assert_eq!(selected.index_count, 250);
}

// ──────────────────────────────────────────────
// Vertex Layout Tests
// ──────────────────────────────────────────────

#[test]
fn test_vertex_layout_stride() {
    let layout = lumas_render::mesh::character_vertex_layout();
    assert_eq!(layout.array_stride as usize, std::mem::size_of::<CharacterVertex>());
}

#[test]
fn test_vertex_layout_attribute_count() {
    let layout = lumas_render::mesh::character_vertex_layout();
    assert_eq!(layout.attributes.len(), 6); // pos, normal, tangent, uv, bone_idx, bone_wt
}

#[test]
fn test_vertex_layout_locations() {
    let layout = lumas_render::mesh::character_vertex_layout();
    for (i, attr) in layout.attributes.iter().enumerate() {
        assert_eq!(attr.shader_location as usize, i, "Attribute at index {} should have location {}", i, i);
    }
}

// ──────────────────────────────────────────────
// Bone Matrix Tests
// ──────────────────────────────────────────────

#[test]
fn test_max_bones_constant() {
    assert_eq!(MAX_BONES, 96);
}

#[test]
fn test_bone_matrices_size() {
    assert_eq!(std::mem::size_of::<BoneMatrices>(), 96 * 64); // 96 × 4×4 f32 = 6144
}

#[test]
fn test_bone_matrices_bytes() {
    use lumas_render::scene::Scene;
    let scene = Scene::new();
    let bytes = scene.bone_matrices_bytes();
    assert_eq!(bytes.len(), 96 * 64);
}

// ──────────────────────────────────────────────
// Shadow Instance GPU Layout Tests
// ──────────────────────────────────────────────

#[test]
fn test_shadow_instance_gpu_size() {
    assert_eq!(std::mem::size_of::<ShadowInstanceGPU>(), 32);
}

#[test]
fn test_shadow_instance_gpu_default() {
    let shadow = ShadowInstanceGPU::default();
    assert_eq!(shadow.world_pos, [0.0; 4]);
    assert_eq!(shadow.size, 1.0);
    assert_eq!(shadow.opacity, 0.3);
}

#[test]
fn test_shadow_instance_gpu_is_bytemuck_castable() {
    let shadow = ShadowInstanceGPU::default();
    let bytes: &[u8] = bytemuck::bytes_of(&shadow);
    assert_eq!(bytes.len(), 32);
}
