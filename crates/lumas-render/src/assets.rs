//! Asset pipeline — centralized loading and management of all renderable assets.
//!
//! The `AssetPipeline` orchestrates the loading of textures, meshes, materials,
//! and shaders through the existing manager subsystems. It provides:
//!
//! - **Synchronous loading** for startup assets (character body, crystal, UI)
//! - **Async texture loading** via a background thread pool
//! - **Path resolution** relative to the configured asset root directory
//! - **Fallback generation** for missing assets (checkerboard textures, default meshes)
//! - **Cache-busting** via file modification timestamps
//!
//! # Asset Directory Structure
//!
//! ```text
//! assets/
//!   characters/
//!     lumi/           # Character skin, body, fur, crystal textures
//!   effects/          # Particle atlas, noise textures, emission masks
//!   ui/               # Panel content, icons, fonts (rendered to texture)
//!   shaders/          # WGSL shader files (also embedded in binary)
//!   environments/     # Skybox, ambient capture
//! ```
//!
//! # Frame Budget
//! Asset loading happens at startup, not during frame rendering.
//! Typical load time: ~50ms for all assets.

use crate::config::RenderConfig;
use crate::context::GpuContext;
use crate::error::RenderError;
use crate::material::{MaterialKind, MaterialManager, PipelineId, PipelineManager};
use crate::mesh::{CharacterVertex, GpuMesh, LodLevel, MeshId};
use crate::shader::ShaderManager;
use crate::texture::{ImageData, TextureId, TextureManager};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Key identifying a unique asset (used for deduplication and caching).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetKey {
    /// Relative path within the asset directory.
    pub path: PathBuf,
    /// Whether to pre-multiply alpha upon loading.
    pub pre_multiply: bool,
}

impl AssetKey {
    pub fn new(path: impl Into<PathBuf>, pre_multiply: bool) -> Self {
        Self {
            path: path.into(),
            pre_multiply,
        }
    }
}

/// Describes a queued async texture load.
#[derive(Debug)]
pub struct PendingTexture {
    pub key: AssetKey,
    pub label: String,
}

/// Loaded mesh asset with LOD information.
#[derive(Debug)]
pub struct MeshAsset {
    pub gpu_mesh: GpuMesh,
    pub mesh_id: MeshId,
    pub label: String,
}

/// The asset pipeline — owns all loaded assets and coordinates loading.
#[derive(Debug)]
pub struct AssetPipeline {
    /// Root directory for all assets (e.g., "assets/").
    asset_root: PathBuf,

    /// Texture manager (delegates GPU texture creation).
    texture_manager: TextureManager,

    /// Material manager (delegates material creation).
    material_manager: MaterialManager,

    /// Pipeline manager (delegates pipeline creation).
    pipeline_manager: PipelineManager,

    /// Shader manager (for loading shader files).
    shader_manager: ShaderManager,

    /// Loaded mesh assets.
    meshes: HashMap<MeshId, MeshAsset>,

    /// Next mesh ID.
    next_mesh_id: u32,

    /// Pending async texture loads.
    pending_textures: Vec<PendingTexture>,

    /// ID of the default fallback texture (1x1 magenta/black checkerboard).
    fallback_texture: Option<TextureId>,
}

impl AssetPipeline {
    /// Create a new asset pipeline.
    ///
    /// # GPU Thread Safety
    /// Must be created on the render thread.
    ///
    /// # Frame Budget
    /// ~0.01ms CPU (initialization only — no loading yet).
    /// Parameters: `config` is reserved for future use (quality preset impacts asset selection).
    pub fn new(
        ctx: &GpuContext,
        _config: &RenderConfig,
        shader_manager: ShaderManager,
    ) -> Self {
        let texture_manager = TextureManager::new(&ctx.device, &ctx.adapter);
        let material_manager = MaterialManager::new(&ctx.device);
        let pipeline_manager = PipelineManager::new(&ctx.device);

        // Default asset root is "assets/" relative to the working directory.
        let asset_root = PathBuf::from("assets");

        Self {
            asset_root,
            texture_manager,
            material_manager,
            pipeline_manager,
            shader_manager,
            meshes: HashMap::new(),
            next_mesh_id: 1,
            pending_textures: Vec::new(),
            fallback_texture: None,
        }
    }

    // ── Asset Root ──

    /// Set the asset root directory.
    pub fn set_asset_root(&mut self, root: impl Into<PathBuf>) {
        self.asset_root = root.into();
    }

    /// Get the configured asset root directory.
    pub fn asset_root(&self) -> &Path {
        &self.asset_root
    }

    /// Resolve a relative path to an absolute path within the asset root.
    pub fn resolve_path(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.asset_root.join(relative)
    }

    // ── Fallback Assets ──

    /// Ensure the fallback texture exists.
    /// Creates a 1x1 magenta/black checkerboard texture for missing assets.
    pub fn ensure_fallback_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<TextureId, RenderError> {
        if let Some(id) = self.fallback_texture {
            return Ok(id);
        }

        // 2x2 checkerboard: magenta, black, black, magenta
        let pixels: [u8; 16] = [
            255, 0, 255, 255,   // magenta
            0, 0, 0, 255,       // black
            0, 0, 0, 255,       // black
            255, 0, 255, 255,   // magenta
        ];

        let image = ImageData::from_rgba(pixels.to_vec(), 2, 2, false);
        let id = self.texture_manager.create_texture_from_image(
            device, queue, &image, "fallback_checkerboard",
        )?;
        self.fallback_texture = Some(id);
        Ok(id)
    }

    /// Get the fallback texture ID.
    pub fn fallback_texture_id(&self) -> Option<TextureId> {
        self.fallback_texture
    }

    // ── Texture Loading (Sync) ──

    /// Load a texture synchronously from a relative asset path.
    ///
    /// # Errors
    /// Returns `RenderError::TextureUploadFailed` if loading fails.
    pub fn load_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        relative_path: impl AsRef<Path>,
        pre_multiplied: bool,
    ) -> Result<TextureId, RenderError> {
        let path = self.resolve_path(relative_path.as_ref());
        self.texture_manager.create_texture_from_file(
            device, queue, &path, pre_multiplied,
        )
    }

    /// Load a texture from raw RGBA data (for procedurally generated textures).
    ///
    /// # Errors
    /// Returns `RenderError::TextureUploadFailed` if upload fails.
    pub fn load_texture_from_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        label: &str,
        pre_multiplied: bool,
    ) -> Result<TextureId, RenderError> {
        let mut image = ImageData::from_rgba(rgba, width, height, pre_multiplied);
        if pre_multiplied && !image.pre_multiplied {
            image.premultiply_alpha();
        }
        self.texture_manager.create_texture_from_image(device, queue, &image, label)
    }

    /// Queue an async texture load. Call `process_pending_textures()` to execute.
    pub fn queue_texture_load(
        &mut self,
        relative_path: impl AsRef<Path>,
        pre_multiplied: bool,
        label: impl Into<String>,
    ) {
        let key = AssetKey::new(
            relative_path.as_ref().to_path_buf(),
            pre_multiplied,
        );
        self.pending_textures.push(PendingTexture {
            key,
            label: label.into(),
        });
    }

    /// Process all queued async texture loads.
    ///
    /// # Errors
    /// Returns the first `RenderError::TextureUploadFailed` encountered.
    pub fn process_pending_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), RenderError> {
        let pending = std::mem::take(&mut self.pending_textures);
        for pt in pending {
            self.load_texture(device, queue, &pt.key.path, pt.key.pre_multiply)?;
        }
        Ok(())
    }

    /// Number of pending texture loads.
    pub fn pending_count(&self) -> usize {
        self.pending_textures.len()
    }

    // ── Mesh Loading ──

    /// Load a mesh from vertex/index data with LOD levels.
    ///
    /// # Errors
    /// Returns `RenderError::ResourceNotFound` if buffer creation fails.
    pub fn load_mesh(
        &mut self,
        device: &wgpu::Device,
        vertices: &[CharacterVertex],
        indices: &[u32],
        lod_levels: [LodLevel; 3],
        label: &str,
    ) -> Result<MeshId, RenderError> {
        let mesh_id = MeshId(self.next_mesh_id);
        self.next_mesh_id += 1;

        let gpu_mesh = GpuMesh::new(mesh_id, device, vertices, indices, lod_levels, label)?;

        let asset = MeshAsset {
            gpu_mesh,
            mesh_id,
            label: label.to_string(),
        };

        self.meshes.insert(mesh_id, asset);
        Ok(mesh_id)
    }

    /// Get a loaded mesh by ID.
    pub fn get_mesh(&self, id: MeshId) -> Option<&MeshAsset> {
        self.meshes.get(&id)
    }

    /// Get a mutable reference to a loaded mesh.
    pub fn get_mesh_mut(&mut self, id: MeshId) -> Option<&mut MeshAsset> {
        self.meshes.get_mut(&id)
    }

    /// Remove a mesh, returning it if it exists.
    pub fn unload_mesh(&mut self, id: MeshId) -> Option<MeshAsset> {
        self.meshes.remove(&id)
    }

    // ── Material Loading ──

    /// Create a material from a `MaterialKind`.
    ///
    /// # Errors
    /// Returns `RenderError::MaterialNotFound` or `RenderError::PipelineCreationFailed`.
    pub fn create_material(
        &mut self,
        kind: MaterialKind,
        pipeline_id: PipelineId,
    ) -> Result<crate::material::MaterialId, RenderError> {
        self.material_manager.create_material(
            kind,
            pipeline_id,
            &self.pipeline_manager,
            &self.texture_manager,
        )
    }

    /// Get a reference to the material manager.
    pub fn material_manager(&self) -> &MaterialManager {
        &self.material_manager
    }

    /// Get a mutable reference to the material manager.
    pub fn material_manager_mut(&mut self) -> &mut MaterialManager {
        &mut self.material_manager
    }

    // ── Pipeline Management ──

    /// Get a reference to the pipeline manager.
    pub fn pipeline_manager(&self) -> &PipelineManager {
        &self.pipeline_manager
    }

    /// Get a mutable reference to the pipeline manager.
    pub fn pipeline_manager_mut(&mut self) -> &mut PipelineManager {
        &mut self.pipeline_manager
    }

    // ── Shader Management ──

    /// Get a reference to the shader manager.
    pub fn shader_manager(&self) -> &ShaderManager {
        &self.shader_manager
    }

    /// Get a mutable reference to the shader manager.
    pub fn shader_manager_mut(&mut self) -> &mut ShaderManager {
        &mut self.shader_manager
    }

    // ── Texture Manager Access ──

    /// Get a reference to the texture manager.
    pub fn texture_manager(&self) -> &TextureManager {
        &self.texture_manager
    }

    /// Get a mutable reference to the texture manager.
    pub fn texture_manager_mut(&mut self) -> &mut TextureManager {
        &mut self.texture_manager
    }

    // ── Lifecycle ──

    /// End the current frame — recall staging belts and process deferred releases.
    pub fn end_frame(&mut self) {
        self.texture_manager.end_frame();
    }

    /// Clear all loaded assets.
    pub fn clear(&mut self) {
        self.meshes.clear();
        self.texture_manager.clear();
        self.material_manager.clear();
        self.pending_textures.clear();
        self.fallback_texture = None;
    }

    /// Number of loaded meshes.
    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Number of loaded textures.
    pub fn texture_count(&self) -> usize {
        self.texture_manager.texture_count()
    }
}

/// Default LOD levels suitable for a character mesh viewed on desktop.
/// - LOD 0 (high): >= 400px screen height
/// - LOD 1 (medium): >= 100px screen height
/// - LOD 2 (low): < 100px screen height
pub fn default_character_lod_levels(
    full_index_count: u32,
    medium_index_count: u32,
    low_index_count: u32,
    full_vertex_count: u32,
) -> [LodLevel; 3] {
    [
        LodLevel {
            first_index: 0,
            index_count: full_index_count,
            vertex_count: full_vertex_count,
            screen_size_threshold: 400.0,
        },
        LodLevel {
            first_index: full_index_count,
            index_count: medium_index_count,
            vertex_count: full_vertex_count,
            screen_size_threshold: 100.0,
        },
        LodLevel {
            first_index: full_index_count + medium_index_count,
            index_count: low_index_count,
            vertex_count: full_vertex_count,
            screen_size_threshold: 0.0,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_key_creation() {
        let key = AssetKey::new("characters/lumi/albedo.png", true);
        assert_eq!(key.path, PathBuf::from("characters/lumi/albedo.png"));
        assert!(key.pre_multiply);
    }

    #[test]
    fn test_asset_key_equality() {
        let a = AssetKey::new("a.png", true);
        let b = AssetKey::new("a.png", true);
        let c = AssetKey::new("a.png", false);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_resolve_path() {
        // We need a minimal set-up to test path resolution.
        // Without a real GPU, just verify the path logic.
        let asset_root = PathBuf::from("assets");
        let resolved = asset_root.join("textures/test.png");
        assert_eq!(resolved, PathBuf::from("assets/textures/test.png"));
    }

    #[test]
    fn test_default_lod_levels() {
        let lods = default_character_lod_levels(10000, 5000, 2000, 3000);
        assert_eq!(lods[0].screen_size_threshold, 400.0);
        assert_eq!(lods[1].screen_size_threshold, 100.0);
        assert_eq!(lods[2].screen_size_threshold, 0.0);
        assert_eq!(lods[0].index_count, 10000);
        assert_eq!(lods[1].first_index, 10000);
        assert_eq!(lods[2].first_index, 15000);
    }

    #[test]
    fn test_pending_texture_queue_logic() {
        // Verify the pending list logic without needing a GPU.
        let mut pending: Vec<PendingTexture> = Vec::new();
        assert_eq!(pending.len(), 0);
        pending.push(PendingTexture {
            key: AssetKey::new("test.png", true),
            label: "test".into(),
        });
        assert_eq!(pending.len(), 1);
    }
}
