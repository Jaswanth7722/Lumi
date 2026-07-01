//! # Expression Target Computation
//!
//! Computes target expression **parameters** (look-at weight, blink schedule,
//! gesture selection) — not the actual blend shape weights or bone rotations.
//! Output is consumed by the Animation Engine's blend tree (SRS Chapter 16).
//!
//! # Authority
//! Character Engine — expression parameter targets.
//!
//! # Does NOT
//! - Compute blend shape weights or bone rotations (Animation Engine's job)
//! - Own animation clips or blend nodes
//! - Execute facial animation

use rand::Rng;
use std::time::{Duration, Instant};

/// A target for the character's gaze/look-at.
#[derive(Debug, Clone)]
pub struct LookAtTarget {
    /// Screen-space X coordinate.
    pub x: f32,
    /// Screen-space Y coordinate.
    pub y: f32,
    /// How strongly to look at this target (0.0–1.0).
    pub weight: f32,
}

/// Blink state for the character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlinkState {
    /// Eyes are open (normal state).
    Open,
    /// Eyes are closing.
    Closing,
    /// Eyes are fully closed (mid-blink).
    Closed,
    /// Eyes are opening.
    Opening,
}

/// Parameters for gesture selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureId {
    /// A small head nod.
    Nod,
    /// A head shake.
    HeadShake,
    /// A shoulder shrug.
    Shrug,
    /// Pointing gesture.
    Point,
    /// Waving gesture.
    Wave,
    /// A thinking pose (hand to chin).
    ThinkPose,
}

/// Computed expression parameters for the Animation Engine.
#[derive(Debug, Clone)]
pub struct ExpressionTargets {
    /// Where Lumas should look, if anywhere.
    pub look_at: Option<LookAtTarget>,
    /// Current blink state.
    pub blink_state: BlinkState,
    /// Optional gesture to perform.
    pub gesture: Option<GestureId>,
    /// Head tilt bias suggestion (-1.0 to 1.0, negative = left, positive = right).
    pub head_tilt_bias: f32,
}

/// Scheduler for blink timing.
///
/// Deciding *when* Lumas should blink is a behavioral/personality decision,
/// not an animation-system decision. The Animation Engine handles the actual
/// blink animation blend shapes.
#[derive(Debug, Clone)]
pub struct BlinkScheduler {
    next_blink_at: Instant,
    interval_min: Duration,
    interval_max: Duration,
    /// Whether blinking is currently suppressed (e.g., during a stare reaction).
    suppressed: bool,
}

impl BlinkScheduler {
    /// Create a new blink scheduler with the given interval range.
    pub fn new(interval_min: Duration, interval_max: Duration) -> Self {
        let next = Self::schedule_next(interval_min, interval_max);
        Self {
            next_blink_at: next,
            interval_min,
            interval_max,
            suppressed: false,
        }
    }

    /// Create a blink scheduler with default intervals (2-6 seconds, typical for humans).
    pub fn default() -> Self {
        Self::new(Duration::from_millis(2000), Duration::from_millis(6000))
    }

    /// Schedule the next blink time.
    fn schedule_next(min: Duration, max: Duration) -> Instant {
        let mut rng = rand::thread_rng();
        let range_ms = rng.gen_range(min.as_millis() as u64..=max.as_millis() as u64);
        Instant::now() + Duration::from_millis(range_ms)
    }

    /// Check if Lumas should blink now, advancing the scheduler if so.
    /// Returns the target blink state.
    pub fn check_blink(&mut self) -> BlinkState {
        if self.suppressed {
            return BlinkState::Open;
        }

        if Instant::now() >= self.next_blink_at {
            self.next_blink_at = Self::schedule_next(self.interval_min, self.interval_max);
            // Return Open because the blink animation is handled as a rapid
            // Open → Closing → Closed → Opening → Open cycle by the Animation Engine.
            // This scheduler just triggers the start of that cycle.
            BlinkState::Closing
        } else {
            BlinkState::Open
        }
    }

    /// Temporarily suppress blinking (e.g., during surprise reaction).
    pub fn suppress(&mut self, duration: Duration) {
        self.suppressed = true;
        self.next_blink_at = Instant::now() + duration;
    }

    /// Resume normal blink scheduling.
    pub fn resume(&mut self) {
        self.suppressed = false;
        self.next_blink_at = Self::schedule_next(self.interval_min, self.interval_max);
    }

    /// Time until the next scheduled blink.
    pub fn time_until_next_blink(&self) -> Duration {
        self.next_blink_at.saturating_duration_since(Instant::now())
    }

    /// Update the blink interval range.
    pub fn set_interval(&mut self, min: Duration, max: Duration) {
        self.interval_min = min;
        self.interval_max = max;
    }
}

/// Compute expression targets given context.
pub fn compute_expression_targets(
    look_at_target: Option<LookAtTarget>,
    blink_scheduler: &mut BlinkScheduler,
    current_emotion: &lumas_common::emotion::Emotion,
    head_tilt: f32,
) -> ExpressionTargets {
    let blink_state = blink_scheduler.check_blink();

    // Select gesture based on emotion
    let gesture = match current_emotion {
        lumas_common::emotion::Emotion::Thinking => Some(GestureId::ThinkPose),
        lumas_common::emotion::Emotion::Curious => Some(GestureId::HeadShake),
        lumas_common::emotion::Emotion::Happy | lumas_common::emotion::Emotion::Proud => {
            Some(GestureId::Nod)
        }
        lumas_common::emotion::Emotion::Surprised => {
            // Suppress blink briefly during surprise
            blink_scheduler.suppress(Duration::from_millis(800));
            None
        }
        _ => None,
    };

    ExpressionTargets {
        look_at: look_at_target,
        blink_state,
        gesture,
        head_tilt_bias: head_tilt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blink_scheduler_default_creation() {
        let scheduler = BlinkScheduler::default();
        assert!(!scheduler.suppressed);
        assert!(scheduler.time_until_next_blink().as_millis() > 0);
    }

    #[test]
    fn test_blink_scheduler_triggers() {
        let mut scheduler = BlinkScheduler::new(Duration::from_millis(1), Duration::from_millis(1));
        // Interval is 1ms, so next blink is essentially immediate
        std::thread::sleep(Duration::from_millis(10));
        let state = scheduler.check_blink();
        // The scheduler should trigger a blink
        assert_eq!(state, BlinkState::Closing);
    }

    #[test]
    fn test_blink_suppression() {
        let mut scheduler = BlinkScheduler::new(Duration::from_millis(1), Duration::from_millis(1));
        scheduler.suppress(Duration::from_secs(60));
        std::thread::sleep(Duration::from_millis(10));
        let state = scheduler.check_blink();
        assert_eq!(state, BlinkState::Open);
    }

    #[test]
    fn test_gesture_selection_by_emotion() {
        let mut scheduler = BlinkScheduler::default();

        let thinking_emotion = lumas_common::emotion::Emotion::Thinking;
        let targets = compute_expression_targets(None, &mut scheduler, &thinking_emotion, 0.0);
        assert_eq!(targets.gesture, Some(GestureId::ThinkPose));

        let happy_emotion = lumas_common::emotion::Emotion::Happy;
        let targets = compute_expression_targets(None, &mut scheduler, &happy_emotion, 0.0);
        assert_eq!(targets.gesture, Some(GestureId::Nod));
    }

    #[test]
    fn test_surprise_suppresses_blink() {
        let mut scheduler = BlinkScheduler::new(Duration::from_millis(1), Duration::from_millis(1));
        let surprise = lumas_common::emotion::Emotion::Surprised;

        compute_expression_targets(None, &mut scheduler, &surprise, 0.0);

        // Blink should be suppressed
        let state = scheduler.check_blink();
        assert_eq!(state, BlinkState::Open);
    }

    #[test]
    fn test_look_at_target_creation() {
        let target = LookAtTarget {
            x: 100.0,
            y: 200.0,
            weight: 0.8,
        };
        assert!((target.weight - 0.8).abs() < f32::EPSILON);
    }
}
