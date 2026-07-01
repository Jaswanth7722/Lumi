//! # Plugin Framework (Chapter 32)
//!
//! Defines the plugin manifest structure and builder for the Lumas SDK.

use lumas_common::tool::{Capability, ToolDefinition};
use std::collections::HashMap;

/// A plugin manifest describing a Lumas plugin's metadata and capabilities.
#[derive(Debug, Clone)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub homepage: Option<String>,
    pub license: String,
    pub capabilities_required: Vec<Capability>,
    pub tools: Vec<ToolDefinition>,
    pub config_schema: HashMap<String, serde_json::Value>,
}

/// Builder for constructing a PluginManifest.
pub struct PluginManifestBuilder {
    manifest: PluginManifest,
}

impl PluginManifestBuilder {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            manifest: PluginManifest {
                id: id.to_string(),
                name: name.to_string(),
                version: "1.0.0".to_string(),
                description: String::new(),
                author: String::new(),
                homepage: None,
                license: "MIT".to_string(),
                capabilities_required: Vec::new(),
                tools: Vec::new(),
                config_schema: HashMap::new(),
            },
        }
    }

    pub fn version(mut self, version: &str) -> Self {
        self.manifest.version = version.to_string();
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.manifest.description = description.to_string();
        self
    }

    pub fn author(mut self, author: &str) -> Self {
        self.manifest.author = author.to_string();
        self
    }

    pub fn homepage(mut self, url: &str) -> Self {
        self.manifest.homepage = Some(url.to_string());
        self
    }

    pub fn license(mut self, license: &str) -> Self {
        self.manifest.license = license.to_string();
        self
    }

    pub fn capability(mut self, capability: Capability) -> Self {
        self.manifest.capabilities_required.push(capability);
        self
    }

    pub fn tool(mut self, tool: ToolDefinition) -> Self {
        self.manifest.tools.push(tool);
        self
    }

    pub fn build(self) -> PluginManifest {
        self.manifest
    }
}

impl PluginManifest {
    pub fn builder(id: &str, name: &str) -> PluginManifestBuilder {
        PluginManifestBuilder::new(id, name)
    }
}

/// Result of a plugin tool call.
pub type PluginResult = Result<serde_json::Value, String>;
