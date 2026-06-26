//! # Animation Engine — Blend Trees and Procedural Animation (Chapter 16)
//!
//! Defines the animation blend tree architecture, clip library categories,
//! cursor look-at system, ear controller, and procedural animation types.

use serde::{Deserialize, Serialize};
use crate::ai::AIState;
use crate::character::Viseme;
use crate::emotion::Emotion;

// ---------------------------------------------------------------------------
// Animation Clip
// ---------------------------------------------------------------------------

/// Unique identifier for an animation clip.
pub type ClipId = String;

/// Category of an animation clip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipCategory {
    Idle,
    Walk,
    Sit,
    Sleep,
    Emotion,
    Task,
    Reaction,
    Transition,
}

/// A single animation clip from the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationClip {
    pub id: ClipId,
    pub category: ClipCategory,
    pub duration_seconds: f32,
    pub looping: bool,
    pub blend_in_ms: u64,
    pub blend_out_ms: u64,
    pub speed: f32,
    pub additive: bool,
}

/// Mode for blending animations together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlendMode {
    /// Full-body blend (replaces base layer).
    FullBody,
    /// Additive blend (layered on top of base).
    Additive { weight: f32 },
    /// Smooth cross-fade between two animations.
    CrossFade { duration_ms: u64, curve: BlendCurve },
}

/// Easing curve for animation blending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendCurve {
    Linear,
    Smooth,
    EaseIn,
    EaseOut,
}

// ---------------------------------------------------------------------------
// Blend Tree Nodes
// ---------------------------------------------------------------------------

/// A node in the hierarchical blend tree evaluated each frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlendNode {
    /// A single animation clip playing.
    Clip {
        clip_id: ClipId,
        time: f32,
        speed: f32,
        looping: bool,
    },
    /// Cross-fade between two blend nodes.
    CrossFade {
        from: Box<BlendNode>,
        to: Box<BlendNode>,
        progress: f32,
        curve: BlendCurve,
    },
    /// Additive layer on top of a base node.
    Additive {
        base: Box<BlendNode>,
        layer: Box<BlendNode>,
        weight: f32,
    },
    /// Procedural (code-driven) animation.
    Procedural(String),
}

// ---------------------------------------------------------------------------
// Cursor Look-At System
// ---------------------------------------------------------------------------

/// Configuration for the cursor look-at IK system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorLookAtConfig {
    /// How much the head turns to follow the cursor (0.0 to 1.0).
    pub head_weight: f32,
    /// How much the eyes follow the cursor (0.0 to 1.0).
    pub eye_weight: f32,
    /// Maximum head yaw in radians.
    pub max_head_yaw: f32,
    /// Maximum head pitch in radians.
    pub max_head_pitch: f32,
    /// Spring smoothing constant.
    pub smoothing: f32,
}

impl Default for CursorLookAtConfig {
    fn default() -> Self {
        Self {
            head_weight: 0.6,
            eye_weight: 0.9,
            max_head_yaw: 0.4,    // ~23 degrees
            max_head_pitch: 0.2,  // ~11 degrees
            smoothing: 8.0,
        }
    }
}

/// A single bone override produced by a procedural animator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoneOverride {
    pub bone_name: String,
    /// Rotation as (x, y, z, w) quaternion.
    pub rotation: (f32, f32, f32, f32),
    pub weight: f32,
}

/// Pose contribution from a procedural animator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoseContribution {
    pub bone_overrides: Vec<BoneOverride>,
}

// ---------------------------------------------------------------------------
// Ear Controller
// ---------------------------------------------------------------------------

/// Ear position target for the procedural ear controller.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EarPose {
    /// -1.0 (back) to 1.0 (forward).
    pub forward_back: f32,
    /// -1.0 (droop) to 1.0 (raised).
    pub up_down: f32,
}

impl EarPose {
    pub fn neutral() -> Self {
        Self { forward_back: 0.0, up_down: 0.0 }
    }

    pub fn attentive() -> Self {
        Self { forward_back: 1.0, up_down: 0.8 }
    }

    pub fn thinking() -> Self {
        Self { forward_back: 0.3, up_down: 0.5 }
    }

    pub fn sad() -> Self {
        Self { forward_back: -0.5, up_down: -0.3 }
    }

    pub fn happy() -> Self {
        Self { forward_back: 0.6, up_down: 1.0 }
    }
}

/// Maps AI state to ear poses for expressive ear animation.
pub fn ear_pose_for_ai_state(state: &AIState) -> EarPose {
    match state {
        AIState::Listening | AIState::ReceivingInput => EarPose::attentive(),
        AIState::Thinking | AIState::RetrievingMemory => EarPose::thinking(),
        AIState::Planning | AIState::ExecutingTool => EarPose { forward_back: 0.5, up_down: 0.6 },
        AIState::Speaking => EarPose { forward_back: 0.3, up_down: 0.3 },
        AIState::Error => EarPose::sad(),
        AIState::Success => EarPose::happy(),
        AIState::AwaitingConfirmation => EarPose { forward_back: 0.7, up_down: 0.4 },
        _ => EarPose::neutral(),
    }
}

// ---------------------------------------------------------------------------
// Animation Context
// ---------------------------------------------------------------------------

/// Context provided to the animation engine each frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationContext {
    pub delta_time: f32,
    pub cursor_screen_position: (f32, f32),
    pub cursor_world_position: (f32, f32, f32),
    pub character_eye_position: (f32, f32, f32),
    pub ai_state: AIState,
    pub emotion: Emotion,
    pub current_viseme: Viseme,
    pub audio_level: f32,
    pub is_speaking: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ear_pose_for_ai_state() {
        assert_eq!(ear_pose_for_ai_state(&AIState::Listening), EarPose::attentive());
        assert_eq!(ear_pose_for_ai_state(&AIState::Thinking), EarPose::thinking());
        assert_eq!(ear_pose_for_ai_state(&AIState::Error), EarPose::sad());
        assert_eq!(ear_pose_for_ai_state(&AIState::Success), EarPose::happy());
        assert_eq!(ear_pose_for_ai_state(&AIState::Idle), EarPose::neutral());
    }

    #[test]
    fn test_clip_category_variants() {
        let categories = vec![
            ClipCategory::Idle,
            ClipCategory::Walk,
            ClipCategory::Emotion,
            ClipCategory::Reaction,
        ];
        for cat in categories {
            let json = serde_json::to_value(&cat).unwrap();
            let back: ClipCategory = serde_json::from_value(json).unwrap();
            assert_eq!(format!("{cat:?}"), format!("{back:?}"));
        }
    }

    #[test]
    fn test_default_cursor_lookat() {
        let config = CursorLookAtConfig::default();
        assert!((config.head_weight - 0.6).abs() < f32::EPSILON);
        assert!(config.max_head_yaw > 0.0);
        assert!(config.max_head_pitch > 0.0);
    }
}
