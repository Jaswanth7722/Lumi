//! # Lumas Developer SDK (Chapters 32-33)
//!
//! The Lumas SDK enables third-party developers to build tool plugins,
//! character behavior extensions, external service integrations,
//! and custom Workspace panel components.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use lumas_sdk::prelude::*;
//!
//! #[lumi_plugin]
//! struct MyPlugin;
//!
//! #[plugin_impl]
//! impl MyPlugin {
//!     fn manifest() -> PluginManifest { /* ... */ }
//!     async fn on_tool_call(tool: &str, input: Value) -> ToolResult { /* ... */ }
//! }
//! ```

pub mod plugin;
pub mod prelude;
pub mod tool;

pub use lumas_common::plan::ToolResult;
/// Re-export lumas_common types for plugin developers.
pub use lumas_common::tool::{Capability, ToolDefinition, ToolError};
