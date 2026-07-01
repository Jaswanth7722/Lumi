//! # Character Engine Configuration
//!
//! Mirrors the `[character]` section of the Lumas config.

use std::time::Duration;

/// Top-level Character Engine configuration.
#[derive(Debug, Clone)]
pub struct CharacterConfig {
    /// Default display name for the character.
    pub default_name: String,
    /// Behavior re-evaluation tick interval (decoupled from render FPS).
    pub tick_interval: Duration,
    /// Whether to immediately re-evaluate behavior on state change events.
    pub immediate_reeval_on_state_change: bool,
    /// Behavior selection configuration.
    pub behavior: BehaviorConfig,
    /// Default personality profile.
    pub personality: PersonalityConfig,
    /// Navigation configuration.
    pub navigation: NavigationConfig,
    /// Persistence configuration.
    pub persistence: PersistenceConfig,
}

impl Default for CharacterConfig {
    fn default() -> Self {
        Self {
            default_name: "Lumas".into(),
            tick_interval: Duration::from_millis(200),
            immediate_reeval_on_state_change: true,
            behavior: BehaviorConfig::default(),
            personality: PersonalityConfig::default(),
            navigation: NavigationConfig::default(),
            persistence: PersistenceConfig::default(),
        }
    }
}

/// Behavior selection configuration.
#[derive(Debug, Clone)]
pub struct BehaviorConfig {
    /// A new behavior must score at least this much higher than the current
    /// one's re-evaluated score to interrupt it.
    pub interrupt_margin: f32,
    /// Minimum time a behavior must run before it can be interrupted.
    pub min_run_time: Duration,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            interrupt_margin: 0.15,
            min_run_time: Duration::from_millis(1500),
        }
    }
}

/// Default personality weights for new characters.
#[derive(Debug, Clone)]
pub struct PersonalityConfig {
    /// Playfulness weight 0.0–1.0.
    pub playfulness: f32,
    /// Patience weight 0.0–1.0.
    pub patience: f32,
    /// Expressiveness weight 0.0–1.0.
    pub expressiveness: f32,
}

impl Default for PersonalityConfig {
    fn default() -> Self {
        Self {
            playfulness: 0.6,
            patience: 0.7,
            expressiveness: 0.65,
        }
    }
}

/// Navigation configuration.
#[derive(Debug, Clone)]
pub struct NavigationConfig {
    /// User-configured no-walk zones (rectangles in screen coordinates).
    pub no_walk_zones: Vec<ScreenRect>,
    /// Maximum distance from current position for exploration destinations.
    pub exploration_radius_px: f32,
}

impl Default for NavigationConfig {
    fn default() -> Self {
        Self {
            no_walk_zones: Vec::new(),
            exploration_radius_px: 400.0,
        }
    }
}

/// A rectangular region on screen.
#[derive(Debug, Clone)]
pub struct ScreenRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Persistence configuration.
#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    /// Interval between automatic profile saves.
    pub save_interval: Duration,
    /// Whether to save on appearance changes immediately.
    pub save_on_appearance_change: bool,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            save_interval: Duration::from_secs(30),
            save_on_appearance_change: true,
        }
    }
}
