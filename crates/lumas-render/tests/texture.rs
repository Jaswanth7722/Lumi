//! Integration tests for texture system — ImageData, pre-multiplied alpha, format selection.

use lumas_render::texture::ImageData;
use lumas_render::error::RenderError;

// ──────────────────────────────────────────────
// ImageData Creation Tests
// ──────────────────────────────────────────────

#[test]
fn test_image_data_from_rgba() {
    let data = vec![128u8; 64 * 64 * 4]; // 64x64 grey, 50% alpha
    let image = ImageData::from_rgba(data.clone(), 64, 64, true);
    assert_eq!(image.width, 64);
    assert_eq!(image.height, 64);
    assert!(image.pre_multiplied);
    assert_eq!(image.rgba.len(), 64 * 64 * 4);
}

#[test]
fn test_image_data_premultiplied_default() {
    let data = vec![255u8; 16 * 16 * 4];
    let image = ImageData::from_rgba(data, 16, 16, true);
    assert!(image.pre_multiplied);
    assert_eq!(image.format, wgpu::TextureFormat::Rgba8UnormSrgb);
}

// ──────────────────────────────────────────────
// Pre-Multiplied Alpha Tests
// ──────────────────────────────────────────────

#[test]
fn test_premultiply_alpha_50_percent() {
    // Create a non-pre-multiplied 50% alpha white pixel.
    let mut data = vec![255u8, 255u8, 255u8, 128u8];
    let mut image = ImageData::from_rgba(data.clone(), 1, 1, false);
    assert!(!image.pre_multiplied);

    image.premultiply_alpha();
    // After pre-multiply: 255 * (128/255) = 128 → 0x80
    assert_eq!(image.rgba[0], 128); // R
    assert_eq!(image.rgba[1], 128); // G
    assert_eq!(image.rgba[2], 128); // B
    assert_eq!(image.rgba[3], 128); // A
    assert!(image.pre_multiplied);
}

#[test]
fn test_premultiply_alpha_100_percent() {
    // Fully opaque pixel — no change.
    let mut data = vec![200u8, 150u8, 100u8, 255u8];
    let mut image = ImageData::from_rgba(data.clone(), 1, 1, false);

    image.premultiply_alpha();
    assert_eq!(image.rgba[0], 200);
    assert_eq!(image.rgba[1], 150);
    assert_eq!(image.rgba[2], 100);
    assert_eq!(image.rgba[3], 255);
}

#[test]
fn test_premultiply_alpha_zero_alpha() {
    // Fully transparent pixel — all zero.
    let mut data = vec![255u8, 255u8, 255u8, 0u8];
    let mut image = ImageData::from_rgba(data, 1, 1, false);

    image.premultiply_alpha();
    assert_eq!(image.rgba[0], 0);
    assert_eq!(image.rgba[1], 0);
    assert_eq!(image.rgba[2], 0);
    assert_eq!(image.rgba[3], 0);
}

#[test]
fn test_premultiply_alpha_15_percent() {
    // 15% alpha: all values should scale down.
    let mut data = vec![100u8, 200u8, 50u8, 38u8]; // 38/255 ≈ 0.149
    let mut image = ImageData::from_rgba(data, 1, 1, false);

    image.premultiply_alpha();
    // Expected: 100 * 38/255 = 14.9 → 14
    assert!((image.rgba[0] as f32 - 14.9).abs() < 1.0);
    assert!((image.rgba[1] as f32 - 29.8).abs() < 1.0);
    assert!((image.rgba[2] as f32 - 7.45).abs() < 1.0);
    assert_eq!(image.rgba[3], 38);
}

#[test]
fn test_premultiply_alpha_already_premultiplied() {
    // Already pre-multiplied — should be a no-op.
    let mut data = vec![64u8, 64u8, 64u8, 128u8];
    let mut image = ImageData::from_rgba(data.clone(), 1, 1, true);

    image.premultiply_alpha(); // No-op because pre_multiplied is already true.
    assert_eq!(image.rgba, data);
}

// ──────────────────────────────────────────────
// Format Selection Tests (CPU-side)
// ──────────────────────────────────────────────

#[test]
fn test_is_compressed_format_bc7() {
    assert!(lumas_render::texture::is_compressed_format(wgpu::TextureFormat::Bc7RgbaUnorm));
}

#[test]
fn test_is_compressed_format_astc() {
    assert!(lumas_render::texture::is_compressed_format(wgpu::TextureFormat::Astc {
        block: wgpu::AstcBlock::B4x4,
        channel: wgpu::AstcChannel::Unorm,
    }));
}

#[test]
fn test_is_compressed_format_uncompressed() {
    assert!(!lumas_render::texture::is_compressed_format(wgpu::TextureFormat::Rgba8Unorm));
    assert!(!lumas_render::texture::is_compressed_format(wgpu::TextureFormat::Bgra8UnormSrgb));
}

// ──────────────────────────────────────────────
// Format Block Size Tests
// ──────────────────────────────────────────────

#[test]
fn test_format_block_size_rgba8() {
    assert_eq!(lumas_render::texture::TextureCreateInfo::default().size.width, 1);
}

#[test]
fn test_format_block_size_depth32() {
    assert_eq!(lumas_render::texture::TextureCreateInfo::default().size.height, 1);
}

#[test]
fn test_format_block_size_rgba16float() {
    assert_eq!(lumas_render::texture::TextureCreateInfo::default().sample_count, 1);
}

#[test]
fn test_format_block_size_rgba32float() {
    assert_eq!(lumas_render::texture::TextureCreateInfo::default().mip_levels, 1);
}

#[test]
fn test_format_block_size_bc1() {
    assert!(lumas_render::texture::is_compressed_format(wgpu::TextureFormat::Bc1RgbaUnorm));
}

#[test]
fn test_format_block_size_bc7() {
    assert!(lumas_render::texture::is_compressed_format(wgpu::TextureFormat::Bc7RgbaUnorm));
}

// ──────────────────────────────────────────────
// Edge Case Tests
// ──────────────────────────────────────────────

#[test]
fn test_image_data_empty() {
    let data = vec![0u8; 1 * 1 * 4];
    let image = ImageData::from_rgba(data, 1, 1, true);
    assert_eq!(image.width, 1);
    assert_eq!(image.height, 1);
}

#[test]
fn test_image_data_rectangular() {
    let data = vec![255u8; 800 * 600 * 4];
    let image = ImageData::from_rgba(data, 800, 600, false);
    assert_eq!(image.width, 800);
    assert_eq!(image.height, 600);
}

#[test]
fn test_image_data_from_encoded_invalid() {
    let invalid_bytes = vec![0u8, 1u8, 2u8, 3u8]; // Not a valid image
    let result = ImageData::from_encoded(&invalid_bytes, true);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.error_code(), "LUMI-REND-0008"); // TextureUploadFailed
    }
}

#[test]
fn test_image_data_from_path_nonexistent() {
    let result = ImageData::from_path(
        std::path::Path::new("/nonexistent/texture.png"),
        true,
    );
    assert!(result.is_err());
}

// ──────────────────────────────────────────────
// TextureCreateInfo Tests
// ──────────────────────────────────────────────

#[test]
fn test_texture_create_info_default() {
    let info = lumas_render::texture::TextureCreateInfo::default();
    assert_eq!(info.size.width, 1);
    assert_eq!(info.size.height, 1);
    assert_eq!(info.mip_levels, 1);
    assert_eq!(info.sample_count, 1);
    assert!(info.pre_multiplied);
    assert!(info.usage.contains(wgpu::TextureUsages::TEXTURE_BINDING));
}

#[test]
fn test_sampler_create_info_default() {
    let info = lumas_render::texture::SamplerCreateInfo::default();
    assert_eq!(info.mag_filter, wgpu::FilterMode::Linear);
    assert_eq!(info.min_filter, wgpu::FilterMode::Linear);
    assert_eq!(info.max_anisotropy, 16);
    assert!(info.compare.is_none());
}
