use serde::{Deserialize, Serialize};

/// Workspace panel configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    #[serde(default = "default_side")]
    pub default_side: String,
    #[serde(default = "default_width")]
    pub default_width: u32,
    #[serde(default = "default_true")]
    pub auto_hide: bool,
    #[serde(default)]
    pub remember_position: bool,
    #[serde(default)]
    pub snap_to_edges: bool,
}

fn default_side() -> String {
    "right".into()
}
fn default_width() -> u32 {
    400
}
fn default_true() -> bool {
    true
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            default_side: default_side(),
            default_width: 400,
            auto_hide: true,
            remember_position: false,
            snap_to_edges: false,
        }
    }
}
