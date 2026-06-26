//! # Lumi Developer SDK (Chapters 32-33)
//!
//! The Lumi SDK enables third-party developers to build tool plugins,
//! character behavior extensions, external service integrations,
//! and custom Workspace panel components.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use lumi_sdk::prelude::*;
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

pub mod prelude;
pub mod plugin;
pub mod tool;

/// Re-export lumi_common types for plugin developers.
pub use lumi_common::tool::{Capability, ToolDefinition, ToolError};
pub use lumi_common::plan::ToolResult;
