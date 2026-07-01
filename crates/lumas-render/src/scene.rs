//! Scene management — holds all renderable entities for a frame.
//!
//! The scene is the bridge between the State Machine (via IPC) and the
//! render graph. It owns the character mesh reference, camera, lighting,
//! particle systems, workspace panels, and shadow data.
//!
//! # Thread Safety
//!
//! The scene is written by the IPC receiver thread (updating bone matrices,
//! camera position, etc.) and read by the render thread once per frame.
//! All mutable fields use `ArcSwap` or are behind a lock for thread safety.
//! The preferred pattern is to double-buffer the scene: the IPC thread
//! writes to scene N+1 while the render thread reads from scene N.

use crate::camera::{Camera, CameraUBO};
use crate::lighting::{LightingScene, LightingUBO};
use crate::mesh::{GpuMesh, MeshId};
use crate::material::MaterialId;
use glam::{Mat4, Vec3};
use std::sync::Arc;

/// Maximum number of bone matrices supported.
pub const MAX_BONES: usize = 96;

/// Bone matrix storage: 96 × 4×4 column-major matrices.
pub type BoneMatrices = [[f32; 16]; MAX_BONES];

/// A single workspace panel rendered as a holographic quad.
#[derive(Debug, Clone)]
pub struct ScenePanel {
    /// Panel position in world space (center of quad).
    pub position: Vec3,
    /// Panel size (width, height) in world units.
    pub size: (f32, f32),
    /// Panel rotation (Euler angles in radians).
    pub rotation: (f32, f32, f32),
    /// Panel content material (the texture rendered by the UI system).
    pub material_id: MaterialId,
    /// Glow color (RGBA, pre-multiplied).
    pub glow_color: [f32; 4],
    /// Opacity (0.0–1.0).
    pub opacity: f32,
}

impl Default for ScenePanel {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            size: (1.0, 0.75),
            rotation: (0.0, 0.0, 0.0),
            material_id: MaterialId::default(),
            glow_color: [0.3, 0.5, 1.0, 0.6],
            opacity: 0.85,
        }
    }
}

/// Shadow instance data for the shadow sprite.
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct ShadowInstanceGPU {
    /// Character foot position (xyz) + padding.
    pub world_pos: [f32; 4],
    /// Shadow sprite size.
    pub size: f32,
    /// Shadow opacity.
    pub opacity: f32,
    /// Padding for 16-byte alignment.
    pub _pad0: f32,
    pub _pad1: f32,
}

impl Default for ShadowInstanceGPU {
    fn default() -> Self {
        Self {
            world_pos: [0.0; 4],
            size: 1.0,
            opacity: 0.3,
            _pad0: 0.0,
            _pad1: 0.0,
        }
    }
}

/// Particle system state (mirrors the compute-side data for CPU tracking).
#[derive(Debug, Clone)]
pub struct SceneParticleState {
    /// Active particle count (estimated from CPU-side counter).
    pub active_count: u32,
    /// Max particle count (from config).
    pub max_count: u32,
    /// Particle emitter position in world space.
    pub emitter_position: Vec3,
    /// Whether the particle system is emitting.
    pub emitting: bool,
}

impl Default for SceneParticleState {
    fn default() -> Self {
        Self {
            active_count: 0,
            max_count: 4096,
            emitter_position: Vec3::ZERO,
            emitting: false,
        }
    }
}

/// The complete scene for a single frame.
///
/// This is the data structure read by the render graph passes. It is
/// written by the IPC receiver and consumed by the render thread.
#[derive(Debug, Clone)]
pub struct Scene {
    /// The character's root transform in world space.
    pub character_transform: Vec3,
    /// The character's rotation (Euler angles).
    pub character_rotation: Vec3,
    /// The character's scale.
    pub character_scale: f32,
    /// Bone matrices for GPU skinning (96 bones).
    pub bone_matrices: BoneMatrices,
    /// The camera.
    pub camera: Camera,
    /// The lighting scene.
    pub lighting: LightingScene,
    /// Active workspace panels.
    pub panels: Vec<ScenePanel>,
    /// Particle system state.
    pub particles: SceneParticleState,
    /// Shadow instance data.
    pub shadow: ShadowInstanceGPU,
    /// Whether the character mesh is loaded.
    pub has_mesh: bool,
    /// Character mesh vertex buffer (set by asset pipeline).
    pub mesh_vertex_buffer: Option<wgpu::Buffer>,
    /// Character mesh index buffer (set by asset pipeline).
    pub mesh_index_buffer: Option<wgpu::Buffer>,
    /// Character mesh index count.
    pub mesh_index_count: u32,
    /// Current LOD level (0=high, 1=medium, 2=low).
    pub lod_level: u8,
    /// Whether the character is sleeping (reduced rendering).
    pub sleeping: bool,
    /// Whether the character is in focus mode (minimal rendering).
    pub focus_mode: bool,
    /// Elapsed time in seconds (for animations).
    pub time_seconds: f32,
}

impl Default for Scene {
    fn default() -> Self {
        let camera = Camera::orthographic(1920.0, 1080.0);
        Self {
            character_transform: Vec3::ZERO,
            character_rotation: Vec3::ZERO,
            character_scale: 1.0,
            bone_matrices: [Mat4::IDENTITY.to_cols_array(); MAX_BONES],
            camera,
            lighting: LightingScene::default(),
            panels: Vec::new(),
            particles: SceneParticleState::default(),
            shadow: ShadowInstanceGPU::default(),
            has_mesh: false,
            mesh_vertex_buffer: None,
            mesh_index_buffer: None,
            mesh_index_count: 0,
            lod_level: 0,
            sleeping: false,
            focus_mode: false,
            time_seconds: 0.0,
        }
    }
}

impl Scene {
    /// Create a new empty scene.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the camera uniform buffer from the scene's camera state.
    pub fn build_camera_ubo(&self) -> CameraUBO {
        self.camera.build_ubo(self.time_seconds)
    }

    /// Build the lighting uniform buffer from the scene's lighting state.
    pub fn build_lighting_ubo(&self) -> LightingUBO {
        self.lighting.build_ubo()
    }

    /// Get the bone matrices as a flat byte slice for GPU upload.
    pub fn bone_matrices_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.bone_matrices)
    }

    /// Update the camera to follow the character transform.
    pub fn camera_follow_character(&mut self) {
        self.camera.target = self.character_transform;
    }

    /// Set the character position and update camera follow.
    pub fn set_character_position(&mut self, pos: Vec3) {
        self.character_transform = pos;
        self.camera_follow_character();

        // Update shadow position to follow character.
        self.shadow.world_pos = [pos.x, 0.0, pos.z, 1.0];

        // Update particle emitter to follow character.
        self.particles.emitter_position = pos + Vec3::new(0.0, 2.0, 0.0); // Crystal position
    }

    /// Set the character rotation.
    pub fn set_character_rotation(&mut self, rot: Vec3) {
        self.character_rotation = rot;
    }

    /// Set the time (for shader animations).
    pub fn set_time(&mut self, time: f32) {
        self.time_seconds = time;
    }

    /// Add a workspace panel to the scene.
    pub fn add_panel(&mut self, panel: ScenePanel) {
        self.panels.push(panel);
    }

    /// Remove all workspace panels.
    pub fn clear_panels(&mut self) {
        self.panels.clear();
    }

    /// Set the particle emitter to emit or stop.
    pub fn set_particle_emitting(&mut self, emitting: bool) {
        self.particles.emitting = emitting;
    }

    /// Check if the scene has any active particles.
    pub fn has_active_particles(&self) -> bool {
        self.particles.active_count > 0 || self.particles.emitting
    }

    /// Check if the scene has any active panels.
    pub fn has_active_panels(&self) -> bool {
        !self.panels.is_empty()
    }

    /// Number of active panels.
    pub fn panel_count(&self) -> u32 {
        self.panels.len() as u32
    }

    /// Set the LOD level.
    pub fn set_lod(&mut self, level: u8) {
        self.lod_level = level.min(2);
    }

    /// Get the appropriate fur shell count based on current LOD/state.
    pub fn fur_shell_count(&self, config: &crate::config::RenderConfig) -> u32 {
        if self.sleeping {
            return 0;
        }
        match self.lod_level {
            0 => config.fur_shells_high,
            1 => config.fur_shells_medium,
            2 => config.fur_shells_low,
            _ => config.fur_shells_high,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scene_default() {
        let scene = Scene::default();
        assert!(!scene.has_mesh);
        assert_eq!(scene.lod_level, 0);
        assert!(!scene.sleeping);
        assert!(scene.panels.is_empty());
    }

    #[test]
    fn test_character_position_updates_camera() {
        let mut scene = Scene::new();
        scene.set_character_position(Vec3::new(100.0, 0.0, 50.0));
        assert_eq!(scene.character_transform, Vec3::new(100.0, 0.0, 50.0));
        assert_eq!(scene.camera.target, Vec3::new(100.0, 0.0, 50.0));
    }

    #[test]
    fn test_fur_shell_count_sleeping() {
        let config = crate::config::RenderConfig::default();
        let mut scene = Scene::new();
        assert_eq!(scene.fur_shell_count(&config), config.fur_shells_high);

        scene.sleeping = true;
        assert_eq!(scene.fur_shell_count(&config), 0);
    }

    #[test]
    fn test_fur_shell_count_lod() {
        let config = crate::config::RenderConfig::default();
        let mut scene = Scene::new();

        scene.set_lod(0);
        assert_eq!(scene.fur_shell_count(&config), config.fur_shells_high);

        scene.set_lod(1);
        assert_eq!(scene.fur_shell_count(&config), config.fur_shells_medium);

        scene.set_lod(2);
        assert_eq!(scene.fur_shell_count(&config), config.fur_shells_low);
    }

    #[test]
    fn test_shadow_instance_gpu_layout() {
        // world_pos: [f32; 4] = 16 bytes
        // size: f32 = 4 bytes
        // opacity: f32 = 4 bytes
        // _pad0: f32 = 4 bytes
        // _pad1: f32 = 4 bytes
        // Total = 32 bytes
        assert_eq!(std::mem::size_of::<ShadowInstanceGPU>(), 32);
    }

    #[test]
    fn test_max_bones() {
        assert_eq!(MAX_BONES, 96);
    }

    #[test]
    fn test_panel_count() {
        let mut scene = Scene::new();
        assert_eq!(scene.panel_count(), 0);

        scene.add_panel(ScenePanel::default());
        assert_eq!(scene.panel_count(), 1);

        scene.clear_panels();
        assert_eq!(scene.panel_count(), 0);
    }

    #[test]
    fn test_has_active_particles() {
        let mut scene = Scene::new();
        assert!(!scene.has_active_particles());

        scene.particles.emitting = true;
        assert!(scene.has_active_particles());

        scene.particles.emitting = false;
        scene.particles.active_count = 10;
        assert!(scene.has_active_particles());
    }
}
