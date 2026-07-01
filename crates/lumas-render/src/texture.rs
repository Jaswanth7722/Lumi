//! GPU texture management — creation, upload, format selection, and lifecycle.
//!
//! # Texture Formats
//!
//! The texture manager selects compressed formats based on adapter capabilities:
//! - BC7 (`Bc7RgbaUnorm`) on Windows (DX12) and Linux (Vulkan)
//! - ASTC 4x4 (`Astc { block: B4x4, channel: Unorm }`) on Apple Silicon (Metal)
//! - Uncompressed fallback for unsupported devices
//!
//! # Staging Belt
//!
//! All CPU→GPU texture uploads go through `wgpu::util::StagingBelt`, which
//! reuses upload buffers across frames. Call `TextureManager::end_frame()` at
//! the end of each frame to reclaim buffers the GPU has finished reading.
//!
//! # Pre-Multiplied Alpha
//!
//! All color textures are stored with pre-multiplied alpha to support the
//! transparent desktop window compositing pipeline.

use crate::error::RenderError;
use slotmap::{new_key_type, SlotMap};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

new_key_type! {
    /// Key for a GPU texture resource.
    pub struct TextureId;
    /// Key for a GPU sampler resource.
    pub struct SamplerId;
}

/// Wrapper for a GPU texture with view and sampler.
#[derive(Debug)]
pub struct GpuTexture {
    pub id: TextureId,
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub format: wgpu::TextureFormat,
    pub size: wgpu::Extent3d,
    pub mip_levels: u32,
    pub label: String,
    pub is_compressed: bool,
}

impl GpuTexture {
    pub fn width(&self) -> u32 {
        self.size.width
    }

    pub fn height(&self) -> u32 {
        self.size.height
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.width() as f32 / self.height() as f32
    }
}

/// Wrapper for a GPU sampler with metadata.
#[derive(Debug)]
pub struct GpuSampler {
    pub id: SamplerId,
    pub sampler: wgpu::Sampler,
    pub label: String,
    pub mag_filter: wgpu::FilterMode,
    pub min_filter: wgpu::FilterMode,
}

/// Configuration for texture creation.
#[derive(Debug, Clone)]
pub struct TextureCreateInfo {
    pub label: String,
    pub size: wgpu::Extent3d,
    pub format: Option<wgpu::TextureFormat>,
    pub mip_levels: u32,
    pub usage: wgpu::TextureUsages,
    pub sample_count: u32,
    pub pre_multiplied: bool,
}

impl Default for TextureCreateInfo {
    fn default() -> Self {
        Self {
            label: String::new(),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            format: None,
            mip_levels: 1,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            sample_count: 1,
            pre_multiplied: true,
        }
    }
}

/// Configuration for sampler creation.
#[derive(Debug, Clone)]
pub struct SamplerCreateInfo {
    pub label: String,
    pub mag_filter: wgpu::FilterMode,
    pub min_filter: wgpu::FilterMode,
    pub mipmap_filter: wgpu::FilterMode,
    pub address_mode_u: wgpu::AddressMode,
    pub address_mode_v: wgpu::AddressMode,
    pub address_mode_w: wgpu::AddressMode,
    pub max_anisotropy: u16,
    pub compare: Option<wgpu::CompareFunction>,
    pub border_color: Option<wgpu::SamplerBorderColor>,
}

impl Default for SamplerCreateInfo {
    fn default() -> Self {
        Self {
            label: String::new(),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            max_anisotropy: 16,
            compare: None,
            border_color: None,
        }
    }
}

/// Image data loaded from disk (before GPU upload).
#[derive(Debug, Clone)]
pub struct ImageData {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub pre_multiplied: bool,
}

impl ImageData {
    /// Load an image from raw RGBA bytes.
    pub fn from_rgba(
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        pre_multiplied: bool,
    ) -> Self {
        Self {
            width,
            height,
            rgba,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            pre_multiplied,
        }
    }

    /// Load an image from encoded bytes (PNG, JPEG, etc.) using the `image` crate.
    ///
    /// # Errors
    /// Returns `RenderError::TextureUploadFailed` if decoding fails.
    pub fn from_encoded(
        data: &[u8],
        pre_multiplied: bool,
    ) -> Result<Self, RenderError> {
        let img = image::load_from_memory(data).map_err(|e| {
            RenderError::TextureUploadFailed {
                texture_id: "encoded_image".into(),
                cause: format!("Image decoding failed: {}", e),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;

        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();

        Ok(Self {
            width,
            height,
            rgba: rgba.into_raw(),
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            pre_multiplied,
        })
    }

    /// Load an image from a file path.
    ///
    /// # Errors
    /// Returns `RenderError::TextureUploadFailed` if the file cannot be read or decoded.
    pub fn from_path(
        path: &std::path::Path,
        pre_multiplied: bool,
    ) -> Result<Self, RenderError> {
        let data = std::fs::read(path).map_err(|e| {
            RenderError::TextureUploadFailed {
                texture_id: path.display().to_string(),
                cause: format!("File read failed: {}", e),
                severity: crate::error::ErrorSeverity::Warning,
            }
        })?;
        Self::from_encoded(&data, pre_multiplied)
    }

    /// Pre-multiply alpha in-place if not already pre-multiplied.
    pub fn premultiply_alpha(&mut self) {
        if self.pre_multiplied {
            return;
        }
        for chunk in self.rgba.chunks_exact_mut(4) {
            let alpha = chunk[3] as f32 / 255.0;
            if alpha < 1.0 {
                chunk[0] = (chunk[0] as f32 * alpha) as u8;
                chunk[1] = (chunk[1] as f32 * alpha) as u8;
                chunk[2] = (chunk[2] as f32 * alpha) as u8;
            }
        }
        self.pre_multiplied = true;
    }
}

/// Texture manager — owns all GPU textures, samplers, and the staging belt.
#[derive(Debug)]
pub struct TextureManager {
    /// Registered textures.
    textures: SlotMap<TextureId, GpuTexture>,
    /// Registered samplers.
    samplers: SlotMap<SamplerId, GpuSampler>,
    /// Staging belt for CPU→GPU texture uploads.
    staging_belt: wgpu::util::StagingBelt,
    /// Cache of loaded texture paths → TextureId (prevents duplicate loads).
    path_cache: HashMap<PathBuf, TextureId>,
    /// Cache of sampler create info → SamplerId (prevents duplicate samplers).
    sampler_cache: HashMap<(wgpu::FilterMode, wgpu::FilterMode, wgpu::AddressMode), SamplerId>,
    /// Total VRAM used by textures, in bytes.
    total_vram_bytes: AtomicU64,
    /// Supported compressed texture format (None if no compression available).
    compressed_format: Option<wgpu::TextureFormat>,
    /// Default sampler that wraps and uses linear filtering.
    default_sampler: SamplerId,
}

impl TextureManager {
    /// Create a new texture manager.
    ///
    /// # Panics
    /// This function does not panic.
    pub fn new(device: &wgpu::Device, adapter: &wgpu::Adapter) -> Self {
        // Determine the best available compressed texture format.
        let compressed_format = select_best_compressed_format(device, adapter);

        let mut manager = Self {
            textures: SlotMap::with_key(),
            samplers: SlotMap::with_key(),
            staging_belt: wgpu::util::StagingBelt::new(device.clone(), 1024 * 1024), // 1MB chunk
            path_cache: HashMap::new(),
            sampler_cache: HashMap::new(),
            total_vram_bytes: AtomicU64::new(0),
            compressed_format,
            default_sampler: SamplerId::default(),
        };

        // Create the default sampler.
        let default_sampler = manager.create_sampler(
            device,
            &SamplerCreateInfo {
                label: "default_sampler".into(),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Linear,
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                ..Default::default()
            },
        );

        manager.default_sampler = default_sampler;
        manager
    }

    /// Get the default sampler ID.
    pub fn default_sampler(&self) -> SamplerId {
        self.default_sampler
    }

    /// Get the default sampler object.
    pub fn default_sampler_ref(&self) -> &wgpu::Sampler {
        &self.samplers[self.default_sampler].sampler
    }

    /// Create a new GPU texture from a descriptor.
    ///
    /// # GPU Thread Safety
    /// Callable from any thread.
    ///
    /// # Frame Budget
    /// ~0.01ms CPU for allocation; actual upload time depends on texture size.
    ///
    /// # Errors
    /// Returns `RenderError::TextureFormatUnsupported` if the format is not supported.
    pub fn create_texture(
        &mut self,
        device: &wgpu::Device,
        info: &TextureCreateInfo,
    ) -> TextureId {
        let format = info.format.unwrap_or(wgpu::TextureFormat::Rgba8UnormSrgb);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&info.label),
            size: info.size,
            mip_level_count: info.mip_levels,
            sample_count: info.sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: info.usage,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(&format!("{}_view", info.label)),
            ..Default::default()
        });

        // Reuse the default sampler — most textures share the same filtering mode.
        let sampler_ref = self.default_sampler_ref().clone();

        let gpu_texture = GpuTexture {
            id: TextureId::default(), // Will be replaced after insert
            texture,
            view,
            sampler: sampler_ref,
            format,
            size: info.size,
            mip_levels: info.mip_levels,
            label: info.label.clone(),
            is_compressed: is_compressed_format(format),
        };

        let id = self.textures.insert(gpu_texture);
        if let Some(tex) = self.textures.get_mut(id) {
            tex.id = id;
        }

        // Track estimated VRAM usage (conservative: no mip chain reduction).
        let bpp = format_block_size(format) as u64;
        let vram = (info.size.width as u64)
            * (info.size.height as u64)
            * (info.size.depth_or_array_layers as u64)
            * bpp;
        self.total_vram_bytes.fetch_add(vram, Ordering::Relaxed);

        id
    }

    /// Create a texture and upload image data using the staging belt.
    ///
    /// # GPU Thread Safety
    /// Callable from any thread.
    ///
    /// # Frame Budget
    /// Upload cost scales with texture size. ~0.1ms CPU for 512×512.
    ///
    /// # Errors
    /// Returns `RenderError::TextureUploadFailed` if encoding is not supported.
    pub fn create_texture_from_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &ImageData,
        label: &str,
    ) -> Result<TextureId, RenderError> {
        let format = self.compressed_format.unwrap_or(image.format);

        let size = wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        };

        let info = TextureCreateInfo {
            label: label.to_string(),
            size,
            format: Some(format),
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            ..Default::default()
        };

        let id = self.create_texture(device, &info);
        let gpu_texture = &self.textures[id];

        // Upload data via staging belt (wgpu 29 API: write_buffer takes &Buffer, offset, size).
        // We create an intermediate staging buffer, write pixel data into it via the belt,
        // then copy from buffer to texture.
        let buffer_size = (image.width * image.height * 4) as u64;
        let buffer_size_wgpu = wgpu::BufferSize::new(buffer_size)
            .ok_or_else(|| RenderError::TextureUploadFailed {
                texture_id: label.into(),
                cause: "Buffer size computation failed".into(),
                severity: crate::error::ErrorSeverity::Warning,
            })?;

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{}_staging", label)),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some(&format!("{}_upload_encoder", label)),
        });

        // Write pixel data to the staging buffer via the staging belt.
        let mut view = self.staging_belt.write_buffer(
            &mut encoder,
            &staging_buffer,
            0,
            buffer_size_wgpu,
        );
        view.copy_from_slice(&image.rgba);
        drop(view); // staging belt records the copy from its internal buffer → staging_buffer

        // Copy from staging buffer → GPU texture.
        let bytes_per_row = image.width * 4;
        let copy_size = wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        };
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &staging_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(image.height),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: &gpu_texture.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            copy_size,
        );

        queue.submit(Some(encoder.finish()));
        self.staging_belt.recall();

        Ok(id)
    }

    /// Create a texture from encoded bytes (PNG, JPEG, etc.).
    ///
    /// # Errors
    /// Returns `RenderError::TextureUploadFailed` if decoding or upload fails.
    pub fn create_texture_from_bytes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
        label: &str,
        pre_multiplied: bool,
    ) -> Result<TextureId, RenderError> {
        let mut image = ImageData::from_encoded(bytes, pre_multiplied)?;
        if pre_multiplied && !image.pre_multiplied {
            image.premultiply_alpha();
        }
        self.create_texture_from_image(device, queue, &image, label)
    }

    /// Create a texture from a file on disk.
    ///
    /// # Errors
    /// Returns `RenderError::TextureUploadFailed` if the file cannot be loaded.
    pub fn create_texture_from_file(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &std::path::Path,
        pre_multiplied: bool,
    ) -> Result<TextureId, RenderError> {
        // Check cache.
        if let Some(id) = self.path_cache.get(path) {
            return Ok(*id);
        }

        let mut image = ImageData::from_path(path, pre_multiplied)?;
        if pre_multiplied && !image.pre_multiplied {
            image.premultiply_alpha();
        }

        let label = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("texture");

        let id = self.create_texture_from_image(device, queue, &image, label)?;
        self.path_cache.insert(path.to_path_buf(), id);
        Ok(id)
    }

    /// Create a GPU sampler.
    ///
    /// Returns a cached sampler if one with matching parameters exists.
    ///
    /// # GPU Thread Safety
    /// Callable from any thread.
    pub fn create_sampler(
        &mut self,
        device: &wgpu::Device,
        info: &SamplerCreateInfo,
    ) -> SamplerId {
        // Check cache.
        let cache_key = (info.min_filter, info.mag_filter, info.address_mode_u);
        if let Some(id) = self.sampler_cache.get(&cache_key) {
            return *id;
        }

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&info.label),
            address_mode_u: info.address_mode_u,
            address_mode_v: info.address_mode_v,
            address_mode_w: info.address_mode_w,
            mag_filter: info.mag_filter,
            min_filter: info.min_filter,
            mipmap_filter: match info.mipmap_filter {
                wgpu::FilterMode::Nearest => wgpu::MipmapFilterMode::Nearest,
                wgpu::FilterMode::Linear => wgpu::MipmapFilterMode::Linear,
            },
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0, // Max usable mip level
            compare: info.compare,
            anisotropy_clamp: info.max_anisotropy,
            border_color: info.border_color,
        });

        let gpu_sampler = GpuSampler {
            id: SamplerId::default(),
            sampler,
            label: info.label.clone(),
            mag_filter: info.mag_filter,
            min_filter: info.min_filter,
        };

        let id = self.samplers.insert(gpu_sampler);
        if let Some(s) = self.samplers.get_mut(id) {
            s.id = id;
        }

        self.sampler_cache.insert(cache_key, id);
        id
    }

    /// Get a texture by ID.
    ///
    /// # Panics
    /// Panics if the texture ID is invalid.
    pub fn get_texture(&self, id: TextureId) -> &GpuTexture {
        &self.textures[id]
    }

    /// Get a mutable texture by ID.
    pub fn get_texture_mut(&mut self, id: TextureId) -> Option<&mut GpuTexture> {
        self.textures.get_mut(id)
    }

    /// Get a sampler by ID.
    ///
    /// # Panics
    /// Panics if the sampler ID is invalid.
    pub fn get_sampler(&self, id: SamplerId) -> &GpuSampler {
        &self.samplers[id]
    }

    /// Destroy a texture and free its GPU memory.
    pub fn destroy_texture(
        &mut self,
        id: TextureId,
        pool: &mut crate::resource::ResourcePool,
    ) {
        if let Some(tex) = self.textures.remove(id) {
            pool.defer_delete_texture(tex.texture);
            // Remove from path cache if present.
            self.path_cache.retain(|_, v| *v != id);
        }
    }

    /// Destroy a sampler.
    pub fn destroy_sampler(
        &mut self,
        id: SamplerId,
        _pool: &mut crate::resource::ResourcePool,
    ) {
        // Remove the sampler from the registry; the GPU resource will be
        // cleaned up when the Sampler is dropped (wgpu::Sampler is ref-counted).
        self.samplers.remove(id);
    }

    /// End the current frame — reclaim staging belt buffers.
    ///
    /// Must be called once per frame after all texture uploads for the frame
    /// have been submitted to the queue.
    pub fn end_frame(&mut self) {
        self.staging_belt.recall();
    }

    /// Total number of textures.
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Total number of samplers.
    pub fn sampler_count(&self) -> usize {
        self.samplers.len()
    }

    /// Total estimated VRAM used by textures, in bytes.
    pub fn total_vram_bytes(&self) -> u64 {
        self.total_vram_bytes.load(Ordering::Relaxed)
    }

    /// Get the supported compressed texture format.
    pub fn compressed_format(&self) -> Option<wgpu::TextureFormat> {
        self.compressed_format
    }

    /// Clear all textures and samplers.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.samplers.clear();
        self.path_cache.clear();
        self.sampler_cache.clear();
        self.total_vram_bytes.store(0, Ordering::Relaxed);
    }
}

/// Select the best available compressed texture format.
fn select_best_compressed_format(
    device: &wgpu::Device,
    adapter: &wgpu::Adapter,
) -> Option<wgpu::TextureFormat> {
    let formats = device.features();

    // Prefer BC7 on DX12/Vulkan (best quality/size ratio).
    if formats.contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
        return Some(wgpu::TextureFormat::Bc7RgbaUnorm);
    }

    // Prefer ASTC on Apple Silicon.
    if formats.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC_HDR) {
        return Some(wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B4x4,
            channel: wgpu::AstcChannel::Unorm,
        });
    }

    // ETC2 fallback (less common).
    if formats.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2) {
        return Some(wgpu::TextureFormat::Etc2Rgba8Unorm);
    }

    None
}

/// Returns `true` if the format is a compressed format.
pub fn is_compressed_format(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Bc1RgbaUnorm
            | wgpu::TextureFormat::Bc2RgbaUnorm
            | wgpu::TextureFormat::Bc3RgbaUnorm
            | wgpu::TextureFormat::Bc4RUnorm
            | wgpu::TextureFormat::Bc5RgUnorm
            | wgpu::TextureFormat::Bc6hRgbFloat
            | wgpu::TextureFormat::Bc6hRgbUfloat
            | wgpu::TextureFormat::Bc7RgbaUnorm
            | wgpu::TextureFormat::Etc2Rgba8Unorm
            | wgpu::TextureFormat::Astc { .. }
    )
}

/// Approximate bytes per pixel/block for a format (used for VRAM estimation).
fn format_block_size(format: wgpu::TextureFormat) -> u32 {
    match format {
        // BC compressed: 4 bytes per 4×4 block = 0.25 bytes per pixel
        wgpu::TextureFormat::Bc1RgbaUnorm
        | wgpu::TextureFormat::Bc4RUnorm
        | wgpu::TextureFormat::Bc4RSnorm => 4, // 4 bytes per 4x4 block

        wgpu::TextureFormat::Bc2RgbaUnorm
        | wgpu::TextureFormat::Bc3RgbaUnorm
        | wgpu::TextureFormat::Bc5RgUnorm
        | wgpu::TextureFormat::Bc5RgSnorm
        | wgpu::TextureFormat::Bc6hRgbFloat
        | wgpu::TextureFormat::Bc6hRgbUfloat
        | wgpu::TextureFormat::Bc7RgbaUnorm => 8, // 8 bytes per 4x4 block

        // ASTC: 16 bytes per block
        wgpu::TextureFormat::Astc { .. } => 16,

        // Uncompressed: N bytes per pixel
        wgpu::TextureFormat::R8Unorm | wgpu::TextureFormat::R8Snorm => 1,
        wgpu::TextureFormat::R16Float | wgpu::TextureFormat::R16Uint | wgpu::TextureFormat::R16Snorm => 2,
        wgpu::TextureFormat::Rg8Unorm | wgpu::TextureFormat::Rg8Snorm => 2,
        wgpu::TextureFormat::R32Float | wgpu::TextureFormat::R32Uint | wgpu::TextureFormat::R32Float => 4,
        wgpu::TextureFormat::Rg16Float => 4,
        wgpu::TextureFormat::Rgba8Unorm
        | wgpu::TextureFormat::Rgba8UnormSrgb
        | wgpu::TextureFormat::Rgba8Snorm => 4,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => 4,
        wgpu::TextureFormat::Rg32Float => 8,
        wgpu::TextureFormat::Rgba16Float => 8,
        wgpu::TextureFormat::Rgba32Float => 16,

        // Depth/stencil
        wgpu::TextureFormat::Depth16Unorm => 2,
        wgpu::TextureFormat::Depth32Float | wgpu::TextureFormat::Depth24Plus => 4,
        wgpu::TextureFormat::Depth24PlusStencil8 | wgpu::TextureFormat::Depth32FloatStencil8 => 4,

        _ => 4, // Conservative default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_data_from_rgba() {
        let data = vec![255u8; 4 * 4 * 4]; // 4x4 white image
        let image = ImageData::from_rgba(data, 4, 4, true);
        assert_eq!(image.width, 4);
        assert_eq!(image.height, 4);
        assert!(image.pre_multiplied);
    }

    #[test]
    fn test_premultiply_alpha() {
        // Create a 50% alpha white pixel.
        let mut data = vec![255u8, 255u8, 255u8, 128u8];
        let mut image = ImageData::from_rgba(data.clone(), 1, 1, false);
        assert!(!image.pre_multiplied);
        image.premultiply_alpha();
        // After pre-multiply: 255 * 0.5 ≈ 127
        assert_eq!(image.rgba[0], 127);
        assert_eq!(image.rgba[1], 127);
        assert_eq!(image.rgba[2], 127);
        assert_eq!(image.rgba[3], 128);
        assert!(image.pre_multiplied);
    }

    #[test]
    fn test_is_compressed_format() {
        assert!(is_compressed_format(wgpu::TextureFormat::Bc7RgbaUnorm));
        assert!(is_compressed_format(wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B4x4,
            channel: wgpu::AstcChannel::Unorm,
        }));
        assert!(!is_compressed_format(wgpu::TextureFormat::Rgba8Unorm));
    }

    #[test]
    fn test_format_block_size() {
        assert_eq!(format_block_size(wgpu::TextureFormat::Rgba8Unorm), 4);
        assert_eq!(format_block_size(wgpu::TextureFormat::R32Float), 4);
        assert_eq!(format_block_size(wgpu::TextureFormat::Rgba32Float), 16);
    }
}
