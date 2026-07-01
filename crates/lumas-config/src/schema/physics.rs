use serde::{Deserialize, Serialize};

/// Physics engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PhysicsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_physics_fps")]
    pub physics_fps: u32,
    #[serde(default)]
    pub gravity_multiplier: f64,
    #[serde(default)]
    pub collision_detection: bool,
}

fn default_true() -> bool {
    true
}
fn default_physics_fps() -> u32 {
    60
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            physics_fps: 60,
            gravity_multiplier: 1.0,
            collision_detection: false,
        }
    }
}
