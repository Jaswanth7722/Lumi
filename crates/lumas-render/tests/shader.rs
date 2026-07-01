//! Shader validation tests.
//!
//! Tests that all 8 WGSL shaders compile without errors under naga validation.
//! These tests require the `shader-validation` feature (included in default).

use lumas_render::shader::ShaderSource;

/// Path to the shaders directory relative to the crate root.
const SHADERS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/shaders");

/// Helper: load a WGSL shader file and validate it with naga.
fn validate_wgsl_shader(name: &str) -> Result<(), Vec<String>> {
    let path = format!("{}/{}.wgsl", SHADERS_DIR, name);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Failed to read shader file: {}", path));

    match naga::front::wgsl::parse_str(&source) {
        Ok(module) => {
            let caps = naga::valid::Capabilities::all();
            match naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                caps,
            )
            .validate(&module)
            {
                Ok(_) => Ok(()),
                Err(e) => {
                    Err(vec![format!("  - {}", e)])
                }
            }
        }
        Err(e) => {
            Err(vec![format!("  - {}", e)])
        }
    }
}

// ──────────────────────────────────────────────
// Shader Validation Tests
// ──────────────────────────────────────────────

/// Test that the character PBR shader compiles and validates.
#[test]
fn test_character_shader_compiles() {
    let result = validate_wgsl_shader("character");
    if let Err(errors) = &result {
        panic!(
            "character.wgsl validation failed:\n{}",
            errors.join("\n")
        );
    }
    assert!(result.is_ok());
}

/// Test that the fur shader compiles and validates.
#[test]
fn test_fur_shader_compiles() {
    let result = validate_wgsl_shader("fur");
    if let Err(errors) = &result {
        panic!(
            "fur.wgsl validation failed:\n{}",
            errors.join("\n")
        );
    }
    assert!(result.is_ok());
}

/// Test that the particle update compute shader compiles.
#[test]
fn test_particle_update_shader_compiles() {
    let result = validate_wgsl_shader("particle_update");
    if let Err(errors) = &result {
        panic!(
            "particle_update.wgsl validation failed:\n{}",
            errors.join("\n")
        );
    }
    assert!(result.is_ok());
}

/// Test that the particle render shader compiles.
#[test]
fn test_particle_render_shader_compiles() {
    let result = validate_wgsl_shader("particle_render");
    if let Err(errors) = &result {
        panic!(
            "particle_render.wgsl validation failed:\n{}",
            errors.join("\n")
        );
    }
    assert!(result.is_ok());
}

/// Test that the bloom compute shader compiles.
#[test]
fn test_bloom_shader_compiles() {
    let result = validate_wgsl_shader("bloom");
    if let Err(errors) = &result {
        panic!(
            "bloom.wgsl validation failed:\n{}",
            errors.join("\n")
        );
    }
    assert!(result.is_ok());
}

/// Test that the postprocess shader compiles.
#[test]
fn test_postprocess_shader_compiles() {
    let result = validate_wgsl_shader("postprocess");
    if let Err(errors) = &result {
        panic!(
            "postprocess.wgsl validation failed:\n{}",
            errors.join("\n")
        );
    }
    assert!(result.is_ok());
}

/// Test that the UI panel shader compiles.
#[test]
fn test_ui_shader_compiles() {
    let result = validate_wgsl_shader("ui");
    if let Err(errors) = &result {
        panic!(
            "ui.wgsl validation failed:\n{}",
            errors.join("\n")
        );
    }
    assert!(result.is_ok());
}

/// Test that the shadow sprite shader compiles.
#[test]
fn test_shadow_shader_compiles() {
    let result = validate_wgsl_shader("shadow");
    if let Err(errors) = &result {
        panic!(
            "shadow.wgsl validation failed:\n{}",
            errors.join("\n")
        );
    }
    assert!(result.is_ok());
}

// ──────────────────────────────────────────────
// Batch Validation Tests
// ──────────────────────────────────────────────

/// Test which shader file names exist.
#[test]
fn test_all_shader_files_exist() {
    let expected = [
        "character", "fur", "particle_update", "particle_render",
        "bloom", "postprocess", "ui", "shadow",
    ];

    for name in &expected {
        let path = format!("{}/{}.wgsl", SHADERS_DIR, name);
        assert!(
            std::path::Path::new(&path).exists(),
            "Missing shader file: {}",
            path
        );
    }
}

/// Test that ALL shaders compile without errors.
/// This provides a single pass/fail for the entire shader suite.
#[test]
fn test_all_shaders_compile() {
    let shaders = [
        "character", "fur", "particle_update", "particle_render",
        "bloom", "postprocess", "ui", "shadow",
    ];

    let mut failures = Vec::new();

    for name in &shaders {
        match validate_wgsl_shader(name) {
            Ok(()) => {} // OK
            Err(errors) => {
                failures.push(format!(
                    "{}:\n{}",
                    name,
                    errors.join("\n")
                ));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "Shader validation failures:\n{}",
            failures.join("\n---\n")
        );
    }
}

// ──────────────────────────────────────────────
// Shader Source Tests
// ──────────────────────────────────────────────

/// Test that the ShaderSource enum handles all variants correctly.
#[test]
fn test_shader_source_variants() {
    let embedded = ShaderSource::Embedded("shader_code");
    assert!(matches!(embedded, ShaderSource::Embedded(_)));

    let inline = ShaderSource::Inline("inline_code".into());
    assert!(matches!(inline, ShaderSource::Inline(_)));
}

/// Test that a minimal valid WGSL fragment shader passes validation.
#[test]
fn test_minimal_valid_wgsl() {
    let source = r#"
@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
"#;

    let result = naga::front::wgsl::parse_str(source);
    assert!(result.is_ok(), "Minimal valid WGSL should parse");
}

/// Test that invalid WGSL is rejected.
#[test]
fn test_invalid_wgsl_rejected() {
    let source = "this is not valid wgsl at all";
    let result = naga::front::wgsl::parse_str(source);
    assert!(result.is_err(), "Invalid WGSL should fail to parse");
}

/// Test that each shader contains certain expected struct names.
#[test]
fn test_shader_contains_expected_keywords() {
    let source = std::fs::read_to_string(format!("{}/character.wgsl", SHADERS_DIR))
        .expect("Failed to read character.wgsl");

    assert!(source.contains("CameraUBO"), "character.wgsl should contain CameraUBO");
    assert!(source.contains("LightingUBO"), "character.wgsl should contain LightingUBO");
    assert!(source.contains("BoneMatrices"), "character.wgsl should contain BoneMatrices");
    assert!(source.contains("vs_main"), "character.wgsl should contain vs_main");
    assert!(source.contains("fs_main"), "character.wgsl should contain fs_main");
}

#[test]
fn test_fur_shader_contains_push_constants() {
    let source = std::fs::read_to_string(format!("{}/fur.wgsl", SHADERS_DIR))
        .expect("Failed to read fur.wgsl");

    assert!(source.contains("push_constant"), "fur.wgsl should use push constants");
    assert!(source.contains("FurPushConstants"), "fur.wgsl should contain FurPushConstants");
    assert!(source.contains("shell_index"), "fur.wgsl should contain shell_index");
    assert!(source.contains("num_shells"), "fur.wgsl should contain num_shells");
}

#[test]
fn test_bloom_shader_contains_compute_entry_points() {
    let source = std::fs::read_to_string(format!("{}/bloom.wgsl", SHADERS_DIR))
        .expect("Failed to read bloom.wgsl");

    assert!(source.contains("bloom_extract"), "bloom.wgsl should contain bloom_extract");
    assert!(source.contains("bloom_downsample"), "bloom.wgsl should contain bloom_downsample");
    assert!(source.contains("bloom_upsample"), "bloom.wgsl should contain bloom_upsample");
    assert!(source.contains("bloom_composite"), "bloom.wgsl should contain bloom_composite");
    assert!(source.contains("@compute"), "bloom.wgsl should use compute shaders");
}

#[test]
fn test_postprocess_shader_contains_fullscreen_triangle() {
    let source = std::fs::read_to_string(format!("{}/postprocess.wgsl", SHADERS_DIR))
        .expect("Failed to read postprocess.wgsl");

    assert!(source.contains("aces_filmic"), "postprocess.wgsl should contain ACES tonemapping");
    assert!(source.contains("linear_to_srgb"), "postprocess.wgsl should contain gamma correction");
    assert!(source.contains("fxaa"), "postprocess.wgsl should contain FXAA (lowercase)");
}
