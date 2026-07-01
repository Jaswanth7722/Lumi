//! LOD (Level of Detail) tests.
//!
//! Tests that LOD selection works correctly across different screen sizes,
//! quality presets, and character states.

use lumas_render::config::{QualityPreset, RenderConfig};
use lumas_render::scene::Scene;
use lumas_render::mesh::LodLevel;

// ──────────────────────────────────────────────
// Simulated LOD Selection
// ──────────────────────────────────────────────

/// Simulate LOD selection logic (matching GpuMesh::select_lod).
fn select_lod(lod_levels: &[LodLevel; 3], screen_height: f32) -> &LodLevel {
    if screen_height < lod_levels[2].screen_size_threshold {
        &lod_levels[2]
    } else if screen_height < lod_levels[1].screen_size_threshold {
        &lod_levels[1]
    } else {
        &lod_levels[0]
    }
}

/// Create a standard set of LOD levels for testing.
fn test_lod_levels() -> [LodLevel; 3] {
    [
        LodLevel {
            first_index: 0,
            index_count: 3000,
            vertex_count: 1500,
            screen_size_threshold: f32::MAX,
        },
        LodLevel {
            first_index: 1500,
            index_count: 1500,
            vertex_count: 750,
            screen_size_threshold: 300.0,
        },
        LodLevel {
            first_index: 2250,
            index_count: 750,
            vertex_count: 375,
            screen_size_threshold: 100.0,
        },
    ]
}

// ──────────────────────────────────────────────
// LOD Selection Tests
// ──────────────────────────────────────────────

#[test]
fn test_lod_high_detail_large_screen() {
    let lods = test_lod_levels();
    // Screen height > 300 → LOD 0 (high detail)
    let selected = select_lod(&lods, 1080.0);
    assert_eq!(selected.vertex_count, 1500);
    assert_eq!(selected.index_count, 3000);
}

#[test]
fn test_lod_medium_detail() {
    let lods = test_lod_levels();
    // 100 < screen height < 300 → LOD 1 (medium detail)
    let selected = select_lod(&lods, 200.0);
    assert_eq!(selected.vertex_count, 750);
    assert_eq!(selected.index_count, 1500);
}

#[test]
fn test_lod_low_detail_small_screen() {
    let lods = test_lod_levels();
    // Screen height < 100 → LOD 2 (low detail)
    let selected = select_lod(&lods, 50.0);
    assert_eq!(selected.vertex_count, 375);
    assert_eq!(selected.index_count, 750);
}

#[test]
fn test_lod_at_threshold_high() {
    let lods = test_lod_levels();
    // Exactly at the medium threshold → still LOD 0
    let selected = select_lod(&lods, 300.0);
    assert_eq!(selected.vertex_count, 1500);
}

#[test]
fn test_lod_at_threshold_low() {
    let lods = test_lod_levels();
    // Exactly at the low threshold → still LOD 1
    let selected = select_lod(&lods, 100.0);
    assert_eq!(selected.vertex_count, 750);
}

#[test]
fn test_lod_below_low_threshold() {
    let lods = [
        LodLevel { first_index: 0, index_count: 3000, vertex_count: 1500, screen_size_threshold: f32::MAX },
        LodLevel { first_index: 1500, index_count: 1500, vertex_count: 750, screen_size_threshold: 300.0 },
        LodLevel { first_index: 2250, index_count: 750, vertex_count: 375, screen_size_threshold: 100.0 },
    ];

    let selected = select_lod(&lods, 50.0);
    assert_eq!(selected.vertex_count, 375);
}

#[test]
fn test_lod_zero_height() {
    let lods = test_lod_levels();
    let selected = select_lod(&lods, 0.0);
    assert_eq!(selected.vertex_count, 375);
    assert_eq!(selected.index_count, 750);
}

#[test]
fn test_lod_negative_height() {
    let lods = test_lod_levels();
    // Negative height treated as small → LOD 2
    let selected = select_lod(&lods, -10.0);
    assert_eq!(selected.vertex_count, 375);
}

// ──────────────────────────────────────────────
// Scene LOD Tests
// ──────────────────────────────────────────────

#[test]
fn test_scene_lod_default() {
    let scene = Scene::new();
    assert_eq!(scene.lod_level, 0);
}

#[test]
fn test_scene_set_lod() {
    let mut scene = Scene::new();
    scene.set_lod(1);
    assert_eq!(scene.lod_level, 1);

    scene.set_lod(2);
    assert_eq!(scene.lod_level, 2);

    // Clamped at 2.
    scene.set_lod(5);
    assert_eq!(scene.lod_level, 2);
}

#[test]
fn test_scene_lod_clamping() {
    let mut scene = Scene::new();
    scene.set_lod(10); // Should clamp to 2
    assert_eq!(scene.lod_level, 2);
}

// ──────────────────────────────────────────────
// Fur Shell Count by LOD
// ──────────────────────────────────────────────

#[test]
fn test_fur_shell_count_lod_0() {
    let config = RenderConfig::default();
    let mut scene = Scene::new();
    scene.set_lod(0);
    assert_eq!(scene.fur_shell_count(&config), config.fur_shells_high);
}

#[test]
fn test_fur_shell_count_lod_1() {
    let config = RenderConfig::default();
    let mut scene = Scene::new();
    scene.set_lod(1);
    assert_eq!(scene.fur_shell_count(&config), config.fur_shells_medium);
}

#[test]
fn test_fur_shell_count_lod_2() {
    let config = RenderConfig::default();
    let mut scene = Scene::new();
    scene.set_lod(2);
    assert_eq!(scene.fur_shell_count(&config), config.fur_shells_low);
}

#[test]
fn test_fur_shell_count_sleeping_overrides_lod() {
    let config = RenderConfig::default();
    let mut scene = Scene::new();
    scene.sleeping = true;
    scene.set_lod(0); // Should be ignored when sleeping
    assert_eq!(scene.fur_shell_count(&config), 0);
}

// ──────────────────────────────────────────────
// Quality Preset LOD Tests
// ──────────────────────────────────────────────

#[test]
fn test_quality_low_fur_disabled() {
    let mut config = RenderConfig::default();
    QualityPreset::Low.apply(&mut config);
    assert_eq!(config.fur_shells_high, 0);
    assert_eq!(config.fur_shells_medium, 0);
    assert_eq!(config.fur_shells_low, 0);
}

#[test]
fn test_quality_medium_fur_limited() {
    let mut config = RenderConfig::default();
    QualityPreset::Medium.apply(&mut config);
    assert_eq!(config.fur_shells_high, 8);
    assert_eq!(config.fur_shells_medium, 8);
    assert_eq!(config.fur_shells_low, 0);
}

#[test]
fn test_quality_high_fur_defaults() {
    let config = RenderConfig::default();
    assert_eq!(config.fur_shells_high, 24);
    assert_eq!(config.fur_shells_medium, 16);
    assert_eq!(config.fur_shells_low, 8);
}

#[test]
fn test_quality_ultra_fur() {
    let mut config = RenderConfig::default();
    QualityPreset::Ultra.apply(&mut config);
    assert_eq!(config.fur_shells_high, 32);
    assert_eq!(config.fur_shells_medium, 24);
    assert_eq!(config.fur_shells_low, 16);
}

// ──────────────────────────────────────────────
// Integration Tests
// ──────────────────────────────────────────────

#[test]
fn test_lod_quality_preset_interaction() {
    // Configure for medium quality.
    let mut config = RenderConfig::default();
    QualityPreset::Medium.apply(&mut config);

    let mut scene = Scene::new();

    // At LOD 0 with medium quality → 8 fur shells.
    scene.set_lod(0);
    assert_eq!(scene.fur_shell_count(&config), 8);

    // At LOD 2 with medium quality → 0 fur shells (low).
    scene.set_lod(2);
    assert_eq!(scene.fur_shell_count(&config), 0);
}

#[test]
fn test_lod_indices_packing() {
    // Verify that LOD levels are packed correctly in the index buffer.
    // LOD 0 starts at first_index = 0, LOD 1 starts after LOD 0's indices, etc.
    let lods = [
        LodLevel { first_index: 0, index_count: 3000, vertex_count: 1500, screen_size_threshold: f32::MAX },
        LodLevel { first_index: 3000, index_count: 1500, vertex_count: 750, screen_size_threshold: 300.0 },
        LodLevel { first_index: 4500, index_count: 750, vertex_count: 375, screen_size_threshold: 100.0 },
    ];

    // Verify packing.
    assert_eq!(lods[1].first_index, lods[0].index_count);
    assert_eq!(lods[2].first_index, lods[0].index_count + lods[1].index_count);
    assert_eq!(lods[2].first_index + lods[2].index_count, 4500 + 750);
    assert_eq!(lods[2].first_index + lods[2].index_count, 5250);
}

#[test]
fn test_vertex_count_reduction() {
    let lods = test_lod_levels();
    // Each LOD should have fewer or equal vertices than the previous.
    assert!(lods[0].vertex_count >= lods[1].vertex_count);
    assert!(lods[1].vertex_count >= lods[2].vertex_count);
}

#[test]
fn test_index_count_reduction() {
    let lods = test_lod_levels();
    // Each LOD should have fewer or equal indices than the previous.
    assert!(lods[0].index_count >= lods[1].index_count);
    assert!(lods[1].index_count >= lods[2].index_count);
}
