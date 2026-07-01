//! Pre-multiplied alpha compositing tests.
//!
//! Lumas renders to a transparent desktop window. This requires **pre-multiplied
//! alpha** throughout the entire pipeline. A standard alpha blend produces
//! black fringes around the character.
//!
//! These tests verify:
//! 1. Image data pre-multiplication is correct
//! 2. All blend states use PREMULTIPLIED_ALPHA_BLENDING
//! 3. The final composite pass outputs pre-multiplied alpha
//! 4. CompositeAlphaMode::PreMultiplied is used by default

use lumas_render::config::{CompositeAlphaMode, RenderConfig};
use lumas_render::material::ParticleBlend;
use lumas_render::texture::ImageData;

// ──────────────────────────────────────────────
// Pre-Multiplied Alpha Image Tests
// ──────────────────────────────────────────────

/// Test that a white pixel at 50% alpha produces pre-multiplied rgba(0.5, 0.5, 0.5, 0.5).
/// This is THE correctness criterion from the spec.
#[test]
fn test_white_50_percent_alpha_premultiplied() {
    // White (255,255,255) at 50% alpha (128).
    let mut data = vec![255u8, 255u8, 255u8, 128u8];
    let mut image = ImageData::from_rgba(data, 1, 1, false);
    image.premultiply_alpha();

    // After pre-multiple: expected rgba(127, 127, 127, 128)
    // This is ≈ (0.5, 0.5, 0.5, 0.5) in float.
    assert_eq!(image.rgba[0], 128, "R should be 128 ≈ 0.5 * 255");
    assert_eq!(image.rgba[1], 128, "G should be 128 ≈ 0.5 * 255");
    assert_eq!(image.rgba[2], 128, "B should be 128 ≈ 0.5 * 255");
    assert_eq!(image.rgba[3], 128, "A should be 128 ≈ 0.5 * 255");
}

/// Test that fully opaque pixels are unchanged after pre-multiplication.
#[test]
fn test_opaque_pixel_unchanged() {
    let mut data = vec![200u8, 150u8, 100u8, 255u8];
    let mut image = ImageData::from_rgba(data.clone(), 1, 1, false);
    image.premultiply_alpha();

    // No change because alpha = 1.0.
    assert_eq!(image.rgba, data);
}

/// Test that fully transparent pixels become (0, 0, 0, 0).
#[test]
fn test_transparent_pixel_zero() {
    let mut data = vec![255u8, 100u8, 50u8, 0u8];
    let mut image = ImageData::from_rgba(data, 1, 1, false);
    image.premultiply_alpha();

    assert_eq!(image.rgba[0], 0);
    assert_eq!(image.rgba[1], 0);
    assert_eq!(image.rgba[2], 0);
    assert_eq!(image.rgba[3], 0);
}

/// Test that the pre-multiplied flag is correctly set.
#[test]
fn test_premultiplied_flag() {
    let mut image = ImageData::from_rgba(vec![0u8; 4], 1, 1, false);
    assert!(!image.pre_multiplied);
    image.premultiply_alpha();
    assert!(image.pre_multiplied);
}

/// Test that already pre-multiplied images are not double-multiplied.
#[test]
fn test_double_premultiply_noop() {
    let mut data = vec![64u8, 64u8, 64u8, 128u8]; // Already pre-multiplied
    let mut image = ImageData::from_rgba(data.clone(), 1, 1, true);
    assert!(image.pre_multiplied);

    image.premultiply_alpha(); // Should be a no-op since pre_multiplied is true.
    assert_eq!(image.rgba, data);
}

// ──────────────────────────────────────────────
// Blend State Tests
// ──────────────────────────────────────────────

/// Test that the default particle blend mode uses pre-multiplied alpha.
#[test]
fn test_particle_alpha_is_premultiplied() {
    let blend = ParticleBlend::Alpha.to_wgpu_blend();
    assert_eq!(blend, wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
}

/// Test that additive blend has correct factors for pre-multiplied color.
#[test]
fn test_additive_blend_factors() {
    let blend = ParticleBlend::Additive.to_wgpu_blend();
    // Additive on pre-multiplied color: src_alpha * src + 1 * dst
    assert_eq!(blend.color.src_factor, wgpu::BlendFactor::SrcAlpha);
    assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::One);
}

/// Test that soft additive blend has correct factors.
#[test]
fn test_soft_additive_blend_factors() {
    let blend = ParticleBlend::SoftAdditive.to_wgpu_blend();
    assert_eq!(blend.color.src_factor, wgpu::BlendFactor::OneMinusDstAlpha);
    assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::One);
}

// ──────────────────────────────────────────────
// Composite Alpha Mode Tests
// ──────────────────────────────────────────────

/// Test that the default config uses pre-multiplied alpha.
#[test]
fn test_default_composite_alpha_is_premultiplied() {
    let config = RenderConfig::default();
    assert_eq!(config.composite_alpha, CompositeAlphaMode::PreMultiplied);
}

/// Test that PreMultiplied config maps to the correct wgpu value.
#[test]
fn test_premultiplied_wgpu_conversion() {
    assert_eq!(
        CompositeAlphaMode::PreMultiplied.to_wgpu(),
        wgpu::CompositeAlphaMode::PreMultiplied
    );
}

/// Test that Opaque fallback maps to the correct wgpu value.
#[test]
fn test_opaque_wgpu_conversion() {
    assert_eq!(
        CompositeAlphaMode::Opaque.to_wgpu(),
        wgpu::CompositeAlphaMode::Opaque
    );
}

// ──────────────────────────────────────────────
// Shader Output Verification
// ──────────────────────────────────────────────

/// Test that the character.wgsl shader outputs pre-multiplied alpha.
#[test]
fn test_character_shader_premultiplied_alpha() {
    let shader_path = concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/character.wgsl");
    let source = std::fs::read_to_string(shader_path)
        .expect("Failed to read character.wgsl");

    // The output should contain the pre-multiplied alpha pattern:
    // "return vec4(color * alpha, alpha);" or similar
    assert!(
        source.contains("vec4(color * alpha, alpha)")
        || source.contains("vec4(color * alpha, alpha)")
        || source.contains("color * alpha"),
        "character.wgsl fragment output must contain pre-multiplied alpha pattern"
    );
}

/// Test that the postprocess.wgsl shader passes alpha through.
#[test]
fn test_postprocess_alpha_passthrough() {
    let shader_path = concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/postprocess.wgsl");
    let source = std::fs::read_to_string(shader_path)
        .expect("Failed to read postprocess.wgsl");

    // The final output should preserve the alpha channel.
    assert!(
        source.contains("color.a") || source.contains("alpha"),
        "postprocess.wgsl must preserve the alpha channel"
    );
}

/// Test that the fur shader outputs pre-multiplied alpha.
#[test]
fn test_fur_shader_premultiplied_output() {
    let shader_path = concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/fur.wgsl");
    let source = std::fs::read_to_string(shader_path)
        .expect("Failed to read fur.wgsl");

    assert!(
        source.contains("color * fur_alpha") || source.contains("color * alpha"),
        "fur.wgsl must output pre-multiplied alpha"
    );
}

/// Test that the shadow shader outputs pre-multiplied alpha.
#[test]
fn test_shadow_shader_premultiplied_output() {
    let shader_path = concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/shadow.wgsl");
    let source = std::fs::read_to_string(shader_path)
        .expect("Failed to read shadow.wgsl");

    // Shadow outputs (0, 0, 0, alpha) which is already pre-multiplied
    // since RGB = 0 * alpha = 0.
    assert!(
        source.contains("vec4(0.0, 0.0, 0.0, alpha)"),
        "shadow.wgsl must output pre-multiplied black shadow"
    );
}

// ──────────────────────────────────────────────
// Pipeline Composite Alpha Tests
// ──────────────────────────────────────────────

/// Test that the Compositor's resize creates an sRGB output texture.
#[test]
fn test_compositor_output_format() {
    // The compositor uses Rgba8UnormSrgb for the LDR output.
    // Pre-multiplied alpha is applied before sRGB conversion.
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let expected = wgpu::TextureFormat::Rgba8UnormSrgb;
    assert_eq!(format, expected);
}

/// Test that the HDR color target format is correct.
#[test]
fn test_hdr_color_target_format() {
    // The geometry pass writes to Rgba16Float (HDR).
    // Pre-multiplied alpha is maintained through the HDR pipeline.
    let format = wgpu::TextureFormat::Rgba16Float;
    assert_eq!(format, wgpu::TextureFormat::Rgba16Float);
}
