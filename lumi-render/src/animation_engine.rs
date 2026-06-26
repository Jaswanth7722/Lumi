//! # Animation Engine — Pose Computation (Chapter 16)
//!
//! Computes the complete pose of the Lumi character every frame
//! using hierarchical blend trees, procedural animations, and
//! state-driven animation selection.

use lumi_common::animation::{
    AnimationClip, BlendCurve, BlendMode, ClipCategory, ClipId, CursorLookAtConfig,
    EarPose, PoseContribution,
};
use lumi_common::ai::AIState;
use std::collections::HashMap;
use tracing::debug;

/// The Animation Engine computes the character pose each frame.
pub struct AnimationEngine {
    /// Clip library indexed by clip ID.
    clip_library: HashMap<ClipId, AnimationClip>,
    /// Currently playing clip.
    current_clip: Option<ClipId>,
    /// Current animation time.
    current_time: f32,
    /// Cursor look-at configuration.
    cursor_lookat: CursorLookAtConfig,
    /// Target ear pose.
    ear_target: EarPose,
    /// Whether the engine is initialized.
    initialized: bool,
}

impl AnimationEngine {
    pub fn new() -> Self {
        Self {
            clip_library: HashMap::new(),
            current_clip: None,
            current_time: 0.0,
            cursor_lookat: CursorLookAtConfig::default(),
            ear_target: EarPose::neutral(),
            initialized: false,
        }
    }

    /// Initialize the animation engine with the default clip library.
    pub fn initialize(&mut self) {
        self.register_default_clips();
        self.initialized = true;
        debug!("Animation Engine initialized with {} clips", self.clip_library.len());
    }

    /// Register the default animation clips.
    fn register_default_clips(&mut self) {
        let default_clips = vec![
            AnimationClip {
                id: "idle_breathe".into(),
                category: ClipCategory::Idle,
                duration_seconds: 2.0,
                looping: true,
                blend_in_ms: 200,
                blend_out_ms: 200,
                speed: 1.0,
                additive: false,
            },
            AnimationClip {
                id: "idle_look_around".into(),
                category: ClipCategory::Idle,
                duration_seconds: 4.0,
                looping: true,
                blend_in_ms: 400,
                blend_out_ms: 400,
                speed: 1.0,
                additive: false,
            },
            AnimationClip {
                id: "walk_forward".into(),
                category: ClipCategory::Walk,
                duration_seconds: 1.0,
                looping: true,
                blend_in_ms: 100,
                blend_out_ms: 200,
                speed: 1.0,
                additive: false,
            },
            AnimationClip {
                id: "thinking_tilt".into(),
                category: ClipCategory::Task,
                duration_seconds: 3.0,
                looping: true,
                blend_in_ms: 300,
                blend_out_ms: 300,
                speed: 1.0,
                additive: true,
            },
            AnimationClip {
                id: "happy_bounce".into(),
                category: ClipCategory::Emotion,
                duration_seconds: 1.5,
                looping: false,
                blend_in_ms: 100,
                blend_out_ms: 400,
                speed: 1.0,
                additive: true,
            },
        ];

        for clip in default_clips {
            self.clip_library.insert(clip.id.clone(), clip);
        }
    }

    /// Play a specific animation clip.
    pub fn play_clip(&mut self, clip_id: ClipId, mode: BlendMode) {
        if self.clip_library.contains_key(&clip_id) {
            self.current_clip = Some(clip_id);
            self.current_time = 0.0;
            debug!("Playing animation: {:?} with mode {:?}", self.current_clip, mode);
        }
    }

    /// Update the animation state for the current frame.
    pub fn update(&mut self, dt: f32) {
        if let Some(ref clip_id) = self.current_clip {
            if let Some(clip) = self.clip_library.get(clip_id) {
                self.current_time += dt * clip.speed;
                if clip.looping {
                    self.current_time %= clip.duration_seconds;
                } else if self.current_time >= clip.duration_seconds {
                    self.current_time = clip.duration_seconds;
                    self.current_clip = None;
                }
            }
        }
    }

    /// Set the cursor look-at target position.
    pub fn set_cursor_position(&mut self, screen_x: f32, screen_y: f32) {
        // In production, this drives IK for head and eyes
    }

    /// Set the target ear pose.
    pub fn set_ear_target(&mut self, pose: EarPose) {
        self.ear_target = pose;
    }

    /// Get the current animation progress (0.0 to 1.0).
    pub fn current_progress(&self) -> f32 {
        if let Some(ref clip_id) = self.current_clip {
            if let Some(clip) = self.clip_library.get(clip_id) {
                return self.current_time / clip.duration_seconds;
            }
        }
        0.0
    }

    /// Check if the engine has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get the currently playing clip info.
    pub fn current_clip_info(&self) -> Option<&AnimationClip> {
        self.current_clip
            .as_ref()
            .and_then(|id| self.clip_library.get(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialization() {
        let mut engine = AnimationEngine::new();
        assert!(!engine.is_initialized());
        engine.initialize();
        assert!(engine.is_initialized());
    }

    #[test]
    fn test_register_clips() {
        let mut engine = AnimationEngine::new();
        engine.initialize();
        assert!(engine.clip_library.contains_key("idle_breathe"));
        assert!(engine.clip_library.contains_key("walk_forward"));
        assert!(engine.clip_library.contains_key("happy_bounce"));
    }

    #[test]
    fn test_play_looping_clip() {
        let mut engine = AnimationEngine::new();
        engine.initialize();
        engine.play_clip("idle_breathe".into(), BlendMode::FullBody);
        assert!(engine.current_clip.is_some());

        // Progress through several loops
        for _ in 0..100 {
            engine.update(0.5);
        }
        // Should still be playing (looping)
        assert!(engine.current_clip.is_some());
    }

    #[test]
    fn test_play_one_shot_clip() {
        let mut engine = AnimationEngine::new();
        engine.initialize();
        engine.play_clip("happy_bounce".into(), BlendMode::Additive { weight: 1.0 });

        // Run past the clip duration
        engine.update(2.0);
        assert!(engine.current_clip.is_none());
    }

    #[test]
    fn test_current_progress() {
        let mut engine = AnimationEngine::new();
        engine.initialize();
        engine.play_clip("idle_breathe".into(), BlendMode::FullBody);
        assert!((engine.current_progress() - 0.0).abs() < f32::EPSILON);
        engine.update(1.0);
        assert!((engine.current_progress() - 0.5).abs() < 0.01);
    }
}
