//! # Character Engine — Character State and Crystal System (Chapter 7)
//!
//! Defines the crystal state system, blend shape targets, material types,
//! and the character draw call structure consumed by the Render Engine.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Crystal State System
// ---------------------------------------------------------------------------

/// The crystal forehead and floating orb are driven by a unified CrystalState
/// transmitted from the AI Core via the `ai.state` IPC channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrystalState {
    pub mode: CrystalMode,
    /// Intensity from 0.0 to 1.0.
    pub intensity: f32,
    pub color: CrystalColor,
    /// Pulse rate in Hz (0 = no pulse).
    pub pulse_rate: f32,
    /// Whether particles should be emitted.
    pub particle_emit: bool,
}

/// Operating mode of the crystal, corresponding to AI processing state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrystalMode {
    Idle,
    Thinking,
    Planning,
    Working,
    Listening,
    Speaking,
    Remembering,
    Learning,
    Success,
    Error,
    Warning,
    Sleep,
}

/// Color palette for the crystal system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrystalColor {
    /// #5BC8F5 — Default idle state
    BlueDefault,
    /// #F5A623 — Warning or alert
    AmberWarning,
    /// #E74C3C — Error state
    RedError,
    /// #2ECC71 — Task success
    GreenSuccess,
    /// #9B59B6 — Memory retrieval in progress
    PurpleMemory,
    /// #F0F4F8 — Sleep/dim state
    WhiteSleep,
    /// #F1C40F — Learning in progress
    GoldLearning,
}

impl CrystalColor {
    /// Returns the hex color string for this crystal color.
    pub fn hex(&self) -> &'static str {
        match self {
            CrystalColor::BlueDefault => "#5BC8F5",
            CrystalColor::AmberWarning => "#F5A623",
            CrystalColor::RedError => "#E74C3C",
            CrystalColor::GreenSuccess => "#2ECC71",
            CrystalColor::PurpleMemory => "#9B59B6",
            CrystalColor::WhiteSleep => "#F0F4F8",
            CrystalColor::GoldLearning => "#F1C40F",
        }
    }

    /// Returns RGB components as f32 in [0.0, 1.0].
    pub fn rgb_f32(&self) -> (f32, f32, f32) {
        match self {
            CrystalColor::BlueDefault => (0.357, 0.784, 0.961),
            CrystalColor::AmberWarning => (0.961, 0.651, 0.137),
            CrystalColor::RedError => (0.906, 0.298, 0.235),
            CrystalColor::GreenSuccess => (0.180, 0.804, 0.443),
            CrystalColor::PurpleMemory => (0.607, 0.349, 0.714),
            CrystalColor::WhiteSleep => (0.941, 0.957, 0.973),
            CrystalColor::GoldLearning => (0.945, 0.769, 0.059),
        }
    }
}

impl Default for CrystalState {
    fn default() -> Self {
        Self {
            mode: CrystalMode::Idle,
            intensity: 0.5,
            color: CrystalColor::BlueDefault,
            pulse_rate: 0.0,
            particle_emit: false,
        }
    }
}

impl CrystalState {
    /// Create a crystal state for thinking mode.
    pub fn thinking() -> Self {
        Self {
            mode: CrystalMode::Thinking,
            intensity: 0.8,
            color: CrystalColor::BlueDefault,
            pulse_rate: 2.0,
            particle_emit: false,
        }
    }

    /// Create a crystal state for success.
    pub fn success() -> Self {
        Self {
            mode: CrystalMode::Success,
            intensity: 1.0,
            color: CrystalColor::GreenSuccess,
            pulse_rate: 4.0,
            particle_emit: true,
        }
    }

    /// Create a crystal state for error.
    pub fn error() -> Self {
        Self {
            mode: CrystalMode::Error,
            intensity: 1.0,
            color: CrystalColor::RedError,
            pulse_rate: 1.5,
            particle_emit: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Blend Shape Targets
// ---------------------------------------------------------------------------

/// Viseme targets for lip-sync animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Viseme {
    PP,
    FF,
    TH,
    DD,
    Kk,
    CH,
    SS,
    Nn,
    RR,
    Aa,
    Ee,
    Ih,
    Oh,
    Ou,
    Rest,
}

/// Emotion brow blend shape targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowTarget {
    Neutral,
    Curious,
    Concerned,
    Happy,
    Focused,
    Surprised,
    Sad,
    Alert,
}

/// Eye openness blend shape targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EyeOpenness {
    Open,
    Half,
    Closed,
    Squint,
}

/// Mouth blend shape targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouthTarget {
    Neutral,
    Open,
    Smile,
    Frown,
    Grin,
    Pout,
}

/// Ear position blend shape targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EarTarget {
    Forward,
    Back,
    Up,
    Droop,
}

/// All blend shape weights for a single frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlendWeights {
    pub viseme: [f32; 15],
    pub brow: [f32; 8],
    pub eye_openness: [f32; 4],
    pub mouth: [f32; 6],
    pub crystal_glow: f32,
    pub ear_position: [f32; 4],
}

impl Default for BlendWeights {
    fn default() -> Self {
        let mut weights = Self {
            viseme: [0.0; 15],
            brow: [0.0; 8],
            eye_openness: [0.0; 4],
            mouth: [0.0; 6],
            crystal_glow: 0.0,
            ear_position: [0.0; 4],
        };
        weights.brow[0] = 1.0; // neutral
        weights.eye_openness[0] = 1.0; // open
        weights.mouth[0] = 1.0; // neutral
        weights.ear_position[0] = 1.0; // forward
        weights
    }
}

// ---------------------------------------------------------------------------
// Material Types
// ---------------------------------------------------------------------------

/// PBR material types used on the Lumi character.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaterialType {
    FurBody,
    Eyes,
    CrystalForehead,
    CrystalOrb,
    InnerEar,
    ClawsPaws,
}

/// A material override for a specific draw call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialOverride {
    pub material_type: MaterialType,
    pub emission_color: Option<(f32, f32, f32)>,
    pub emission_intensity: f32,
    pub roughness: Option<f32>,
    pub metalness: Option<f32>,
}

// ---------------------------------------------------------------------------
// Character Draw Call
// ---------------------------------------------------------------------------

/// The complete draw call submitted by the Character Engine to the Render Engine
/// each frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterDrawCall {
    pub mesh_id: String,
    pub lod_level: u8,
    /// Bone matrices for skeletal animation (up to 96 bones).
    pub bone_matrices: Vec<[f32; 16]>,
    pub blend_weights: BlendWeights,
    pub material_overrides: Vec<MaterialOverride>,
    pub crystal_state: CrystalState,
    pub world_transform: [f32; 16],
    pub shadow_caster: bool,
}

// ---------------------------------------------------------------------------
// Level of Detail
// ---------------------------------------------------------------------------

/// LOD level for the character mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LODLevel {
    Full = 0,
    Medium = 1,
    Low = 2,
}

impl LODLevel {
    /// Triangle count for each LOD level.
    pub fn triangle_count(&self) -> u32 {
        match self {
            LODLevel::Full => 18000,
            LODLevel::Medium => 9000,
            LODLevel::Low => 4000,
        }
    }

    /// Fur shell count for each LOD level.
    pub fn fur_shells(&self) -> u32 {
        match self {
            LODLevel::Full => 24,
            LODLevel::Medium => 16,
            LODLevel::Low => 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crystal_color_hex() {
        assert_eq!(CrystalColor::BlueDefault.hex(), "#5BC8F5");
        assert_eq!(CrystalColor::GreenSuccess.hex(), "#2ECC71");
        assert_eq!(CrystalColor::RedError.hex(), "#E74C3C");
    }

    #[test]
    fn test_crystal_rgb_range() {
        for color in &[
            CrystalColor::BlueDefault,
            CrystalColor::AmberWarning,
            CrystalColor::RedError,
            CrystalColor::GreenSuccess,
            CrystalColor::PurpleMemory,
            CrystalColor::WhiteSleep,
            CrystalColor::GoldLearning,
        ] {
            let (r, g, b) = color.rgb_f32();
            assert!((0.0..=1.0).contains(&r), "R out of range for {color:?}");
            assert!((0.0..=1.0).contains(&g), "G out of range for {color:?}");
            assert!((0.0..=1.0).contains(&b), "B out of range for {color:?}");
        }
    }

    #[test]
    fn test_lod_triangle_counts() {
        assert_eq!(LODLevel::Full.triangle_count(), 18000);
        assert_eq!(LODLevel::Medium.triangle_count(), 9000);
        assert_eq!(LODLevel::Low.triangle_count(), 4000);
    }

    #[test]
    fn test_crystal_state_builders() {
        let thinking = CrystalState::thinking();
        assert_eq!(thinking.mode, CrystalMode::Thinking);
        assert!((thinking.intensity - 0.8).abs() < f32::EPSILON);

        let success = CrystalState::success();
        assert_eq!(success.mode, CrystalMode::Success);
        assert!(success.particle_emit);
    }
}
