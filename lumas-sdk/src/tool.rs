//! # Tool Definitions (Chapter 32)
//!
//! Helper types for defining tools in the SDK.

use lumas_common::tool::{Capability, ToolCategory, ToolDefinition};

/// Builder for creating tool definitions in plugins.
pub struct ToolDef {
    definition: ToolDefinition,
}

impl ToolDef {
    /// Create a new tool definition with the given name.
    pub fn new(name: &str) -> Self {
        Self {
            definition: ToolDefinition {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                description: String::new(),
                category: ToolCategory::Plugin,
                input_schema: serde_json::json!({}),
                output_schema: serde_json::json!({}),
                capabilities_required: Vec::new(),
                requires_approval: false,
                is_reversible: false,
                timeout_ms: 30000,
                cost_estimate: None,
            },
        }
    }

    /// Set the tool description.
    pub fn description(mut self, description: &str) -> Self {
        self.definition.description = description.to_string();
        self
    }

    /// Set the input JSON Schema.
    pub fn input_schema(mut self, schema: serde_json::Value) -> Self {
        self.definition.input_schema = schema;
        self
    }

    /// Add a required capability.
    pub fn capability(mut self, capability: Capability) -> Self {
        self.definition.capabilities_required.push(capability);
        self
    }

    /// Set whether this tool requires user approval.
    pub fn requires_approval(mut self, requires: bool) -> Self {
        self.definition.requires_approval = requires;
        self
    }

    /// Set the timeout in milliseconds.
    pub fn timeout_ms(mut self, ms: u64) -> Self {
        self.definition.timeout_ms = ms;
        self
    }

    /// Build the tool definition.
    pub fn build(self) -> ToolDefinition {
        self.definition
    }
}
