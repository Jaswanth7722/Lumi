//! # Tool Framework — Tool Registration and Execution (Chapter 11)
//!
//! Manages tool definitions, capability checks, input sanitization,
//! and tool invocation lifecycle.

use lumi_common::tool::{
    Capability, InvokeToolRequest, SanitizationError, ToolDefinition, ToolError,
    ToolInputSanitizer, builtin_tools,
};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// The Tool Framework manages all executable capabilities available to the AI Core.
pub struct ToolFramework {
    /// Registered tools by name.
    tools: HashMap<String, ToolDefinition>,
    /// Granted capabilities for the current session.
    granted_capabilities: Vec<Capability>,
    /// Input sanitizer for preventing injection attacks.
    sanitizer: ToolInputSanitizer,
}

impl ToolFramework {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            granted_capabilities: Vec::new(),
            sanitizer: ToolInputSanitizer::new(8192),
        }
    }

    /// Register the built-in tools (filesystem, terminal, web, etc.).
    pub fn register_builtin_tools(&mut self) {
        for tool in builtin_tools() {
            self.register_tool(tool);
        }
        info!("Registered {} built-in tools", self.tools.len());
    }

    /// Register a single tool definition.
    pub fn register_tool(&mut self, tool: ToolDefinition) {
        debug!("Registering tool: {} ({})", tool.name, tool.category);
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Check if all required capabilities are granted.
    pub fn check_capabilities(&self, required: &[Capability]) -> Result<(), ToolError> {
        for cap in required {
            if !self.granted_capabilities.contains(cap) {
                return Err(ToolError::PermissionDenied(format!(
                    "Required capability not granted: {cap:?}"
                )));
            }
        }
        Ok(())
    }

    /// Grant a capability for the current session.
    pub fn grant_capability(&mut self, cap: Capability) {
        if !self.granted_capabilities.contains(&cap) {
            self.granted_capabilities.push(cap);
            debug!("Capability granted: {:?}", cap);
        }
    }

    /// Validate input against a tool's JSON Schema.
    pub fn validate_input(
        &self,
        tool: &ToolDefinition,
        input: &serde_json::Value,
    ) -> Result<(), ToolError> {
        // In production, use a JSON Schema validator library like jsonschema.
        // For the skeleton, just verify required fields exist.
        if let Some(required) = tool.input_schema.get("required").and_then(|r| r.as_array()) {
            for field in required {
                if let Some(field_name) = field.as_str() {
                    if !input
                        .get(field_name)
                        .and_then(|v| if v.is_null() { None } else { Some(v) })
                        .is_some()
                    {
                        return Err(ToolError::InvalidInput(format!(
                            "Missing required field: {field_name}"
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Execute a tool with the given input.
    pub fn execute_tool(
        &self,
        request: &InvokeToolRequest,
    ) -> Result<serde_json::Value, ToolError> {
        // Validate input
        let tool = self
            .tools
            .get(&request.name)
            .ok_or_else(|| ToolError::NotFound(format!("Tool '{}' not found", request.name)))?;

        self.validate_input(tool, &request.input)?;

        // Check capabilities
        self.check_capabilities(&tool.capabilities_required)?;

        debug!(
            "Executing tool: {} (timeout: {}ms)",
            tool.name, tool.timeout_ms
        );

        // In production, this would dispatch to the actual implementation.
        // For the skeleton, return a placeholder result.
        Ok(serde_json::json!({
            "status": "executed",
            "tool": request.name,
            "duration_ms": 0,
        }))
    }

    /// Get a tool definition by name.
    pub fn get_tool(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// List all registered tools.
    pub fn list_tools(&self) -> Vec<&ToolDefinition> {
        self.tools.values().collect()
    }

    /// List tools by category.
    pub fn list_tools_by_category(&self, category: &str) -> Vec<&ToolDefinition> {
        self.tools
            .values()
            .filter(|t| format!("{:?}", t.category).to_lowercase() == category.to_lowercase())
            .collect()
    }

    /// Sanitize a shell command.
    pub fn sanitize_command(&self, cmd: &str) -> Result<String, SanitizationError> {
        self.sanitizer.sanitize_shell_command(cmd)
    }

    /// Sanitize a file path.
    pub fn sanitize_path(&self, path: &str) -> Result<String, SanitizationError> {
        self.sanitizer.sanitize_path(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_builtins() {
        let mut framework = ToolFramework::new();
        framework.register_builtin_tools();
        assert!(framework.get_tool("fs.read_file").is_some());
        assert!(framework.get_tool("terminal.execute").is_some());
        assert!(framework.get_tool("web.fetch").is_some());
    }

    #[test]
    fn test_capability_check() {
        let mut framework = ToolFramework::new();
        framework.grant_capability(Capability::FilesystemRead);

        // Should pass
        assert!(
            framework
                .check_capabilities(&[Capability::FilesystemRead])
                .is_ok()
        );

        // Should fail
        assert!(
            framework
                .check_capabilities(&[Capability::FilesystemDelete])
                .is_err()
        );
    }

    #[test]
    fn test_input_validation() {
        let framework = ToolFramework::new();
        let tool = builtin_tools()
            .into_iter()
            .find(|t| t.name == "fs.read_file")
            .unwrap();

        // Valid input
        let valid = serde_json::json!({"path": "/tmp/test.txt"});
        assert!(framework.validate_input(&tool, &valid).is_ok());

        // Missing required field
        let invalid = serde_json::json!({});
        assert!(framework.validate_input(&tool, &invalid).is_err());
    }

    #[test]
    fn test_sanitization() {
        let framework = ToolFramework::new();
        assert!(framework.sanitize_command("echo hello").is_ok());
        assert!(framework.sanitize_command("rm -rf /").is_err());
    }

    #[test]
    fn test_tool_execution() {
        let mut framework = ToolFramework::new();
        framework.grant_capability(Capability::FilesystemRead);
        framework.register_builtin_tools();

        let request = InvokeToolRequest {
            name: "fs.read_file".into(),
            input: serde_json::json!({"path": "/tmp/test.txt"}),
            trace_id: None,
        };

        let result = framework.execute_tool(&request);
        assert!(result.is_ok());
    }
}
