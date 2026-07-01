//! # Character Engine Diagnostics
//!
//! Diagnostics and debugging utilities for the Character Engine.
//! Provides snapshot/inspection of the engine's internal state for
//! debugging, profiling, and observability.
//!
//! # Authority
//! Character Engine — diagnostics data.
//!
//! # Does NOT
//! - Replace proper tracing/logging infrastructure
//! - Provide production monitoring (see `lumas-performance`)

use crate::behavior::BehaviorSelector;
use crate::emotion::EmotionSystem;
use crate::identity::CharacterIdentity;
use crate::lifecycle::EngineLifecycle;
use crate::movement::MovementPlanner;
use std::time::Duration;

/// A diagnostic snapshot of the character engine's internal state.
#[derive(Debug, Clone)]
pub struct EngineDiagnostics {
    /// Current engine lifecycle state.
    pub lifecycle: EngineLifecycle,
    /// Character identity info.
    pub identity: Option<CharacterIdentitySnapshot>,
    /// Current behavior info.
    pub current_behavior: Option<BehaviorSnapshot>,
    /// Current emotion state.
    pub current_emotion: Option<EmotionSnapshot>,
    /// Current movement intent.
    pub current_movement: Option<MovementSnapshot>,
    /// Engine uptime.
    pub uptime: Duration,
    /// Number of ticks processed.
    pub tick_count: u64,
}

/// Snapshot of character identity for diagnostics.
#[derive(Debug, Clone)]
pub struct CharacterIdentitySnapshot {
    pub id: String,
    pub name: String,
    pub version: String,
}

/// Snapshot of current behavior for diagnostics.
#[derive(Debug, Clone)]
pub struct BehaviorSnapshot {
    pub behavior_id: String,
    pub elapsed_ms: u64,
}

/// Snapshot of current emotion for diagnostics.
#[derive(Debug, Clone)]
pub struct EmotionSnapshot {
    pub primary: String,
    pub intensity: f32,
}

/// Snapshot of current movement for diagnostics.
#[derive(Debug, Clone)]
pub struct MovementSnapshot {
    pub reason: String,
    pub urgency: String,
}

/// Build a diagnostic snapshot from engine components.
pub fn build_diagnostics(
    lifecycle: &EngineLifecycle,
    identity: Option<&CharacterIdentity>,
    selector: &BehaviorSelector,
    emotion: &EmotionSystem,
    movement: &MovementPlanner,
    tick_count: u64,
    uptime: Duration,
) -> EngineDiagnostics {
    let identity = identity.map(|id| CharacterIdentitySnapshot {
        id: id.id.to_string(),
        name: id.name.clone(),
        version: id.version.to_string(),
    });

    let current_behavior = selector.current_behavior().map(|id| {
        let elapsed = selector
            .current_execution()
            .map(|e| e.elapsed().as_millis() as u64)
            .unwrap_or(0);
        BehaviorSnapshot {
            behavior_id: id.to_string(),
            elapsed_ms: elapsed,
        }
    });

    let current_emotion = {
        let state = emotion.current();
        Some(EmotionSnapshot {
            primary: format!("{:?}", state.primary),
            intensity: state.intensity,
        })
    };

    let current_movement = movement.current_intent().map(|intent| MovementSnapshot {
        reason: format!("{:?}", intent.reason),
        urgency: format!("{:?}", intent.urgency),
    });

    EngineDiagnostics {
        lifecycle: lifecycle.clone(),
        identity,
        current_behavior,
        current_emotion,
        current_movement,
        uptime,
        tick_count,
    }
}

impl std::fmt::Display for EngineDiagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Character Engine Diagnostics ===")?;
        writeln!(f, "Lifecycle: {:?}", self.lifecycle)?;
        writeln!(f, "Uptime: {:?}", self.uptime)?;
        writeln!(f, "Ticks: {}", self.tick_count)?;
        if let Some(ref identity) = self.identity {
            writeln!(f, "Character: {} (v{})", identity.name, identity.version)?;
        }
        if let Some(ref behavior) = self.current_behavior {
            writeln!(
                f,
                "Behavior: {} ({}ms)",
                behavior.behavior_id, behavior.elapsed_ms
            )?;
        }
        if let Some(ref emotion) = self.current_emotion {
            writeln!(f, "Emotion: {} ({:.2})", emotion.primary, emotion.intensity)?;
        }
        if let Some(ref movement) = self.current_movement {
            writeln!(f, "Movement: {} ({})", movement.reason, movement.urgency)?;
        }
        Ok(())
    }
}
