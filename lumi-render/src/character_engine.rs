//! # Character Engine — Character State Management (Chapter 7)
//!
//! Manages the Lumi character's mesh state, crystal system, blend shapes,
//! materials, and produces CharacterDrawCall structures each frame.

use lumi_common::character::{
    BlendWeights, CharacterDrawCall, CrystalColor, CrystalMode, CrystalState, LODLevel,
    MaterialOverride, MaterialType,
};

/// The Character Engine manages the complete state of the Lumi 3D character.
pub struct CharacterEngine {
    /// Current mesh ID being used.
    mesh_id: String,
    /// Current LOD level.
    lod_level: LODLevel,
    /// Current blend shape weights.
    blend_weights: BlendWeights,
    /// Current crystal state.
    crystal_state: CrystalState,
    /// Active material overrides.
    material_overrides: Vec<MaterialOverride>,
    /// World transform for the character.
    world_transform: [f32; 16],
    /// Whether shadow casting is enabled.
    shadow_caster: bool,
}

impl CharacterEngine {
    pub fn new() -> Self {
        Self {
            mesh_id: "lumi_body".into(),
            lod_level: LODLevel::Full,
            blend_weights: BlendWeights::default(),
            crystal_state: CrystalState::default(),
            material_overrides: Vec::new(),
            world_transform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            shadow_caster: true,
        }
    }

    /// Update the crystal state (called from AI state changes).
    pub fn update_crystal(&mut self, state: CrystalState) {
        self.crystal_state = state;
    }

    /// Set blend shape weights for a specific frame.
    pub fn set_blend_weights(&mut self, weights: BlendWeights) {
        self.blend_weights = weights;
    }

    /// Set the LOD level based on distance.
    pub fn set_lod(&mut self, level: LODLevel) {
        self.lod_level = level;
    }

    /// Add a material override.
    pub fn add_material_override(&mut self, override_: MaterialOverride) {
        self.material_overrides.push(override_);
    }

    /// Set the world transform.
    pub fn set_world_transform(&mut self, transform: [f32; 16]) {
        self.world_transform = transform;
    }

    /// Build the draw call for the current frame.
    pub fn build_draw_call(&self) -> CharacterDrawCall {
        CharacterDrawCall {
            mesh_id: self.mesh_id.clone(),
            lod_level: self.lod_level as u8,
            bone_matrices: vec![],
            blend_weights: self.blend_weights.clone(),
            material_overrides: self.material_overrides.clone(),
            crystal_state: self.crystal_state.clone(),
            world_transform: self.world_transform,
            shadow_caster: self.shadow_caster,
        }
    }

    /// Get the current crystal state.
    pub fn crystal_state(&self) -> &CrystalState {
        &self.crystal_state
    }

    /// Get the current LOD level.
    pub fn lod_level(&self) -> LODLevel {
        self.lod_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_crystal_state() {
        let engine = CharacterEngine::new();
        assert_eq!(engine.crystal_state().mode, CrystalMode::Idle);
        assert_eq!(engine.crystal_state().color, CrystalColor::BlueDefault);
    }

    #[test]
    fn test_update_crystal() {
        let mut engine = CharacterEngine::new();
        engine.update_crystal(CrystalState::thinking());
        assert_eq!(engine.crystal_state().mode, CrystalMode::Thinking);
    }

    #[test]
    fn test_draw_call_building() {
        let engine = CharacterEngine::new();
        let draw_call = engine.build_draw_call();
        assert_eq!(draw_call.mesh_id, "lumi_body");
        assert!(draw_call.shadow_caster);
    }

    #[test]
    fn test_lod_changes() {
        let mut engine = CharacterEngine::new();
        assert_eq!(engine.lod_level(), LODLevel::Full);
        engine.set_lod(LODLevel::Low);
        assert_eq!(engine.lod_level(), LODLevel::Low);
    }
}
