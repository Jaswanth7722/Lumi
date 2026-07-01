use serde::{Deserialize, Serialize};

/// Animation system configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AnimationConfig {
    #[serde(default = "default_quality")]
    pub quality: String,
    #[serde(default = "default_true")]
    pub idle_animations: bool,
    #[serde(default = "default_true")]
    pub transition_animations: bool,
    #[serde(default = "default_max_fps")]
    pub max_animation_fps: u32,
}

fn default_quality() -> String {
    "full".into()
}
fn default_true() -> bool {
    true
}
fn default_max_fps() -> u32 {
    60
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            quality: default_quality(),
            idle_animations: true,
            transition_animations: true,
            max_animation_fps: 60,
        }
    }
}
