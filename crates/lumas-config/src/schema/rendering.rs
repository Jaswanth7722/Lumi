use serde::{Deserialize, Serialize};

/// Rendering engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RenderingConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_true")]
    pub vsync: bool,
    #[serde(default = "default_msaa")]
    pub msaa_samples: u32,
    #[serde(default)]
    pub enable_shadows: bool,
    #[serde(default = "default_true")]
    pub enable_post_processing: bool,
}

fn default_backend() -> String {
    "auto".into()
}
fn default_true() -> bool {
    true
}
fn default_msaa() -> u32 {
    4
}

impl Default for RenderingConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            vsync: true,
            msaa_samples: 4,
            enable_shadows: false,
            enable_post_processing: true,
        }
    }
}
