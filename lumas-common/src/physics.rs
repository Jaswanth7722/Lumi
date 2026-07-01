//! # Physics and Movement — Locomotion and Secondary Physics (Chapter 18)
//!
//! Defines the locomotion controller, path planning, tail physics
//! simulation, and crystal orb spring-pendulum system.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Locomotion System
// ---------------------------------------------------------------------------

/// Walking state for the locomotion controller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WalkState {
    /// Character is stationary.
    Idle,
    /// Initiating walk in a direction.
    StartWalk { direction: Direction },
    /// Actively walking.
    Walking {
        direction: Direction,
        speed_blend: f32,
    },
    /// Decelerating to a stop.
    StopWalk { last_direction: Direction },
    /// Turning to face a new direction.
    Turn {
        from: Direction,
        to: Direction,
        progress: f32,
    },
}

/// Cardinal direction for character facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    /// Returns the opposite direction.
    pub fn opposite(&self) -> Self {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }
}

/// Configuration for the locomotion controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocomotionConfig {
    /// Maximum movement speed in pixels/second.
    pub max_speed: f32,
    /// Acceleration in pixels/second².
    pub acceleration: f32,
    /// Deceleration in pixels/second².
    pub deceleration: f32,
    /// Minimum distance to target before stopping.
    pub stopping_distance: f32,
}

impl Default for LocomotionConfig {
    fn default() -> Self {
        Self {
            max_speed: 320.0,
            acceleration: 800.0,
            deceleration: 1200.0,
            stopping_distance: 5.0,
        }
    }
}

/// Complete state of the locomotion controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocomotionState {
    pub current_position: (f32, f32),
    pub target_position: (f32, f32),
    pub velocity: (f32, f32),
    pub walk_state: WalkState,
    pub is_moving: bool,
}

// ---------------------------------------------------------------------------
// No-Walk Zones
// ---------------------------------------------------------------------------

/// A rectangular area where Lumas is not allowed to walk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoWalkZone {
    pub label: String,
    pub bounds: (f32, f32, f32, f32), // x, y, width, height
}

// ---------------------------------------------------------------------------
// Tail Physics (Position-Based Dynamics)
// ---------------------------------------------------------------------------

/// A single node in the tail simulation chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailNode {
    pub position: (f32, f32, f32),
    pub prev_position: (f32, f32, f32),
    pub is_pinned: bool,
}

/// Configuration for the tail physics simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailConfig {
    /// Number of tail nodes (default 3: root, mid, tip).
    pub node_count: usize,
    /// Rest length between nodes.
    pub rest_length: f32,
    /// Stiffness (default 0.7).
    pub stiffness: f32,
    /// Damping (default 0.92).
    pub damping: f32,
    /// Gravity scale (default 0.3).
    pub gravity_scale: f32,
    /// Number of constraint satisfaction iterations (default 8).
    pub constraint_iterations: u32,
}

impl Default for TailConfig {
    fn default() -> Self {
        Self {
            node_count: 3,
            rest_length: 0.05,
            stiffness: 0.7,
            damping: 0.92,
            gravity_scale: 0.3,
            constraint_iterations: 8,
        }
    }
}

/// Complete state of the tail simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailState {
    pub nodes: Vec<TailNode>,
    pub config: TailConfig,
}

// ---------------------------------------------------------------------------
// Crystal Orb Spring Physics
// ---------------------------------------------------------------------------

/// Configuration for the crystal orb spring-pendulum simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrbConfig {
    /// Rest offset from the tail tip (default floating position).
    pub rest_offset: (f32, f32, f32),
    /// Spring stiffness (default 40.0).
    pub stiffness: f32,
    /// Spring damping (default 6.0).
    pub damping: f32,
    /// Mass of the orb (default 1.0).
    pub mass: f32,
}

impl Default for OrbConfig {
    fn default() -> Self {
        Self {
            rest_offset: (0.0, 0.15, 0.0),
            stiffness: 40.0,
            damping: 6.0,
            mass: 1.0,
        }
    }
}

/// Complete state of the orb spring simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrbState {
    pub offset: (f32, f32, f32),
    pub velocity: (f32, f32, f32),
    pub config: OrbConfig,
}

// ---------------------------------------------------------------------------
// Path Planning
// ---------------------------------------------------------------------------

/// A grid cell for pathfinding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridCell {
    pub x: i32,
    pub y: i32,
}

/// Configuration for the path planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathPlannerConfig {
    /// Grid cell size in pixels (default 64).
    pub cell_size_px: f32,
    /// Whether to smooth the path after A* search.
    pub smooth_path: bool,
}

impl Default for PathPlannerConfig {
    fn default() -> Self {
        Self {
            cell_size_px: 64.0,
            smooth_path: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_opposite() {
        assert_eq!(Direction::Left.opposite(), Direction::Right);
        assert_eq!(Direction::Up.opposite(), Direction::Down);
    }

    #[test]
    fn test_locmotion_defaults() {
        let config = LocomotionConfig::default();
        assert!((config.max_speed - 320.0).abs() < f32::EPSILON);
        assert!((config.acceleration - 800.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_tail_config_default() {
        let config = TailConfig::default();
        assert_eq!(config.node_count, 3);
        assert!((config.damping - 0.92).abs() < f32::EPSILON);
        assert_eq!(config.constraint_iterations, 8);
    }

    #[test]
    fn test_orb_config_default() {
        let config = OrbConfig::default();
        assert!((config.stiffness - 40.0).abs() < f32::EPSILON);
        assert!((config.damping - 6.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_walk_state_variants() {
        let states = vec![
            WalkState::Idle,
            WalkState::StartWalk {
                direction: Direction::Right,
            },
            WalkState::Walking {
                direction: Direction::Left,
                speed_blend: 0.5,
            },
            WalkState::StopWalk {
                last_direction: Direction::Up,
            },
        ];
        for state in states {
            let json = serde_json::to_value(&state).unwrap();
            let back: WalkState = serde_json::from_value(json).unwrap();
            assert_eq!(format!("{state:?}"), format!("{back:?}"));
        }
    }
}
