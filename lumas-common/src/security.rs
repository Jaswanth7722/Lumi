//! # Security Model — Trust Boundaries and Isolation (Chapter 23)
//!
//! Defines secret management, tool approval gates, threat model,
//! process isolation boundaries, and audit event types.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Threat Model
// ---------------------------------------------------------------------------

/// Categories of security threats to the Lumas platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatCategory {
    MaliciousPlugin,
    PromptInjection,
    CredentialTheft,
    MemoryPoisoning,
    IPCSpoofing,
    LLMOutputInjection,
}

/// A documented threat with mitigation strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatModelEntry {
    pub category: ThreatCategory,
    pub description: String,
    pub impact: String,
    pub mitigation: String,
}

/// Returns the standard threat model entries.
pub fn default_threat_model() -> Vec<ThreatModelEntry> {
    vec![
        ThreatModelEntry {
            category: ThreatCategory::MaliciousPlugin,
            description: "Plugin developer introduces malicious code".into(),
            impact: "Data exfiltration, system damage".into(),
            mitigation: "WASM sandbox, capability restrictions, network allowlist".into(),
        },
        ThreatModelEntry {
            category: ThreatCategory::PromptInjection,
            description: "Malicious web/file content injects commands".into(),
            impact: "Unauthorized tool execution".into(),
            mitigation: "System prompt enforcement, tool approval gates".into(),
        },
        ThreatModelEntry {
            category: ThreatCategory::CredentialTheft,
            description: "Local attacker accesses stored API keys".into(),
            impact: "Cloud API key exposure".into(),
            mitigation: "OS keychain storage, never in config files".into(),
        },
        ThreatModelEntry {
            category: ThreatCategory::MemoryPoisoning,
            description: "False beliefs injected via malicious conversation".into(),
            impact: "False belief injection".into(),
            mitigation: "Memory confidence scoring, user verification".into(),
        },
        ThreatModelEntry {
            category: ThreatCategory::IPCSpoofing,
            description: "Local malware sends fake IPC messages".into(),
            impact: "False AI state, unauthorized actions".into(),
            mitigation: "Process token authentication on IPC channels".into(),
        },
        ThreatModelEntry {
            category: ThreatCategory::LLMOutputInjection,
            description: "Crafted AI response exploits UI rendering".into(),
            impact: "UI exploitation, cross-process injection".into(),
            mitigation: "Output rendering sandboxed, no eval()".into(),
        },
    ]
}

// ---------------------------------------------------------------------------
// Secret Management
// ---------------------------------------------------------------------------

/// Errors that can occur during secret management operations.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum SecretError {
    #[error("Secret not found: {0}")]
    NotFound(String),
    #[error("Access denied to secret: {0}")]
    AccessDenied(String),
    #[error("Platform keychain unavailable")]
    KeychainUnavailable,
    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Platform-agnostic secret store interface.
pub trait SecretStore: Send + Sync {
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn delete(&self, key: &str) -> Result<(), SecretError>;
}

/// Describes how a secret should be stored and accessed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretDescriptor {
    pub key: String,
    pub label: String,
    pub required: bool,
    pub service_name: String,
}

// ---------------------------------------------------------------------------
// Tool Approval Gates
// ---------------------------------------------------------------------------

/// A pattern for matching tool names (glob-style).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPattern {
    pub pattern: String,
}

impl ToolPattern {
    pub fn matches(&self, tool_name: &str) -> bool {
        tool_name.starts_with(&self.pattern.replace('*', ""))
    }
}

/// Condition under which an approval rule applies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalCondition {
    Always,
    Never,
    WhenInputMatches(String),
    WhenCapabilityIncludes(String),
}

/// Action to take when an approval rule matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalAction {
    AutoApprove,
    RequireUserConfirmation,
    Deny,
    DenyWithMessage(String),
}

/// A single tool approval rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRule {
    pub pattern: ToolPattern,
    pub condition: ApprovalCondition,
    pub action: ApprovalAction,
}

/// Gate that evaluates tool execution requests against approval rules.
#[derive(Debug, Clone)]
pub struct ApprovalGate {
    pub rules: Vec<ApprovalRule>,
}

impl ApprovalGate {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Evaluate a tool name against all rules.
    pub fn evaluate(&self, tool_name: &str) -> ApprovalAction {
        for rule in &self.rules {
            if rule.pattern.matches(tool_name) {
                match &rule.condition {
                    ApprovalCondition::Always => return rule.action.clone(),
                    ApprovalCondition::Never => return ApprovalAction::Deny,
                    _ => continue,
                }
            }
        }
        ApprovalAction::RequireUserConfirmation
    }
}

impl Default for ApprovalGate {
    fn default() -> Self {
        let rules = vec![
            ApprovalRule {
                pattern: ToolPattern {
                    pattern: "fs.delete".into(),
                },
                condition: ApprovalCondition::Always,
                action: ApprovalAction::RequireUserConfirmation,
            },
            ApprovalRule {
                pattern: ToolPattern {
                    pattern: "terminal.execute".into(),
                },
                condition: ApprovalCondition::Always,
                action: ApprovalAction::RequireUserConfirmation,
            },
            ApprovalRule {
                pattern: ToolPattern {
                    pattern: "fs.read".into(),
                },
                condition: ApprovalCondition::Always,
                action: ApprovalAction::AutoApprove,
            },
        ];
        Self { rules }
    }
}

// ---------------------------------------------------------------------------
// Process Capabilities
// ---------------------------------------------------------------------------

/// Declared capabilities for each Lumas process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessCapabilities {
    pub process_name: String,
    pub gpu_access: bool,
    pub network_access: bool,
    pub filesystem_access: bool,
    pub microphone_access: bool,
    pub sandbox_type: SandboxType,
}

/// Type of sandbox applied to a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxType {
    /// macOS App Sandbox or equivalent.
    OSSandbox,
    /// WebAssembly sandbox (for plugins).
    Wasm,
    /// No sandbox (core process only, minimal).
    None,
}

/// Returns the default capability declarations for each process.
pub fn default_process_capabilities() -> Vec<ProcessCapabilities> {
    vec![
        ProcessCapabilities {
            process_name: "render".into(),
            gpu_access: true,
            network_access: false,
            filesystem_access: false,
            microphone_access: false,
            sandbox_type: SandboxType::OSSandbox,
        },
        ProcessCapabilities {
            process_name: "core".into(),
            gpu_access: false,
            network_access: true,
            filesystem_access: true,
            microphone_access: false,
            sandbox_type: SandboxType::OSSandbox,
        },
        ProcessCapabilities {
            process_name: "voice".into(),
            gpu_access: false,
            network_access: false,
            filesystem_access: false,
            microphone_access: true,
            sandbox_type: SandboxType::OSSandbox,
        },
        ProcessCapabilities {
            process_name: "storage".into(),
            gpu_access: false,
            network_access: false,
            filesystem_access: true,
            microphone_access: false,
            sandbox_type: SandboxType::OSSandbox,
        },
        ProcessCapabilities {
            process_name: "plugin-host".into(),
            gpu_access: false,
            network_access: true,
            filesystem_access: false,
            microphone_access: false,
            sandbox_type: SandboxType::Wasm,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_gate_auto_approve() {
        let gate = ApprovalGate::default();
        match gate.evaluate("fs.read_file") {
            ApprovalAction::AutoApprove => {}
            other => panic!("Expected auto-approve, got {:?}", other),
        }
    }

    #[test]
    fn test_approval_gate_require_confirm() {
        let gate = ApprovalGate::default();
        match gate.evaluate("fs.delete_file") {
            ApprovalAction::RequireUserConfirmation => {}
            other => panic!("Expected confirmation, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_pattern_matching() {
        let pattern = ToolPattern {
            pattern: "fs.*".into(),
        };
        assert!(pattern.matches("fs.read_file"));
        assert!(pattern.matches("fs.delete"));
        assert!(!pattern.matches("terminal.execute"));
    }

    #[test]
    fn test_threat_model_count() {
        let model = default_threat_model();
        assert_eq!(model.len(), 6);
    }

    #[test]
    fn test_process_capabilities() {
        let caps = default_process_capabilities();
        assert_eq!(caps.len(), 5);
        assert!(
            caps.iter()
                .any(|c| c.process_name == "render" && c.gpu_access)
        );
        assert!(
            caps.iter()
                .any(|c| c.process_name == "plugin-host" && c.sandbox_type == SandboxType::Wasm)
        );
    }

    #[test]
    fn test_secret_store_trait_object() {
        // Just verify the trait is object-safe
        fn _take_store(_store: &dyn SecretStore) {}
    }
}
