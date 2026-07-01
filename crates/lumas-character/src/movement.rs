//! # Movement Intent
//!
//! The Character Engine computes **where Lumas wants to be** (`MovementIntent`).
//! The actual interpolated screen position is owned by the Desktop Engine's
//! `SpringInterpolator` — this crate does NOT reimplement spring interpolation,
//! path smoothing, or screen-space animation.
//!
//! # Authority
//! Character Engine — movement intent (destination + urgency).
//!
//! # Does NOT
//! - Compute spring interpolation or smooth paths (Desktop Engine's job)
//! - Execute actual screen positioning
//! - Own screen-space coordinates after they're set

use crate::error::CharacterResult;
use lumas_common::position::PositionTarget;
use std::sync::RwLock;

/// Urgency level for movement, affecting which spring config the Desktop Engine selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementUrgency {
    /// Slow, relaxed movement (exploration, idle wandering).
    Leisurely,
    /// Normal movement speed (following cursor, window changes).
    Normal,
    /// Immediate repositioning (user drag, important reaction).
    Immediate,
}

/// Reason for movement — for diagnostics/logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementReason {
    /// Exploring the desktop environment.
    BehaviorExploring,
    /// Following the active window.
    FollowingActiveWindow,
    /// User dragged the character.
    UserDragRequested,
    /// Returning to home/default position.
    ReturningHome,
    /// Moving to avoid obstruction or occlusion.
    AvoidingObstruction,
    /// Reacting to a notification.
    NotificationReaction,
}

/// Where Lumas wants to be — intent, not actual movement.
#[derive(Debug, Clone)]
pub struct MovementIntent {
    /// Target position on screen.
    pub target: PositionTarget,
    /// Urgency level for spring interpolation tuning.
    pub urgency: MovementUrgency,
    /// Why Lumas is moving (for diagnostics).
    pub reason: MovementReason,
}

impl MovementIntent {
    /// Create a new movement intent to an absolute position.
    pub fn to_absolute(x: f32, y: f32, reason: MovementReason) -> Self {
        Self {
            target: PositionTarget::Absolute { x, y },
            urgency: MovementUrgency::Normal,
            reason,
        }
    }

    /// Create a new movement intent near the cursor.
    pub fn near_cursor(offset_x: f32, offset_y: f32, reason: MovementReason) -> Self {
        Self {
            target: PositionTarget::NearCursor {
                offset_x,
                offset_y,
            },
            urgency: MovementUrgency::Normal,
            reason,
        }
    }

    /// Create a leisurely exploration movement intent.
    pub fn explore(target: PositionTarget) -> Self {
        Self {
            target,
            urgency: MovementUrgency::Leisurely,
            reason: MovementReason::BehaviorExploring,
        }
    }
}

/// Tracks the current movement intent. Consumed by the Desktop Engine.
#[derive(Debug)]
pub struct MovementPlanner {
    current_intent: RwLock<Option<MovementIntent>>,
}

impl MovementPlanner {
    /// Create a new movement planner with no initial intent.
    pub fn new() -> Self {
        Self {
            current_intent: RwLock::new(None),
        }
    }

    /// Set a new movement intent, replacing any previous intent.
    pub fn set_intent(&self, intent: MovementIntent) {
        if let Ok(mut current) = self.current_intent.write() {
            *current = Some(intent);
        }
    }

    /// Get the current movement intent, if any.
    pub fn current_intent(&self) -> Option<MovementIntent> {
        self.current_intent
            .read()
            .ok()
            .and_then(|g| g.clone())
    }

    /// Take the current intent (consumes it, leaving None).
    /// Called by the Desktop Engine when picking up the intent.
    pub fn take_intent(&self) -> Option<MovementIntent> {
        self.current_intent.write().ok().and_then(|mut intent| intent.take())
    }

    /// Clear the current intent.
    pub fn clear(&self) -> CharacterResult<()> {
        if let Ok(mut current) = self.current_intent.write() {
            *current = None;
        }
        Ok(())
    }
}

impl Default for MovementPlanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get_intent() {
        let planner = MovementPlanner::new();
        let intent = MovementIntent::to_absolute(100.0, 200.0, MovementReason::ReturningHome);
        planner.set_intent(intent.clone());
        assert_eq!(planner.current_intent().unwrap().reason, MovementReason::ReturningHome);
    }

    #[test]
    fn test_take_intent() {
        let planner = MovementPlanner::new();
        planner.set_intent(MovementIntent::to_absolute(50.0, 50.0, MovementReason::BehaviorExploring));
        assert!(planner.take_intent().is_some());
        assert!(planner.current_intent().is_none());
    }

    #[test]
    fn test_clear_intent() {
        let planner = MovementPlanner::new();
        planner.set_intent(MovementIntent::to_absolute(0.0, 0.0, MovementReason::UserDragRequested));
        planner.clear().unwrap();
        assert!(planner.current_intent().is_none());
    }

    #[test]
    fn test_explore_intent() {
        let intent = MovementIntent::explore(PositionTarget::Absolute { x: 400.0, y: 300.0 });
        assert_eq!(intent.urgency, MovementUrgency::Leisurely);
        assert_eq!(intent.reason, MovementReason::BehaviorExploring);
    }
}
