//! # Security Model — Trust Boundaries and Secret Management (Chapter 23)

use lumi_common::security::{
    ApprovalAction, ApprovalCondition, ApprovalGate, ApprovalRule, SecretError, SecretStore,
    ToolPattern,
};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::debug;

/// In-memory secret store wrapping the SecretStore trait.
pub struct MemorySecretStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self {
            secrets: Mutex::new(HashMap::new()),
        }
    }
}

impl SecretStore for MemorySecretStore {
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        let mut map = self
            .secrets
            .lock()
            .map_err(|e| SecretError::StorageError(e.to_string()))?;
        map.insert(key.to_string(), value.to_string());
        debug!("Secret stored: {}", key);
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        let map = self
            .secrets
            .lock()
            .map_err(|e| SecretError::StorageError(e.to_string()))?;
        Ok(map.get(key).cloned())
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        let mut map = self
            .secrets
            .lock()
            .map_err(|e| SecretError::StorageError(e.to_string()))?;
        map.remove(key);
        debug!("Secret deleted: {}", key);
        Ok(())
    }
}

/// Manages security policies including approval gates for tool execution.
pub struct SecurityManager {
    secret_store: Box<dyn SecretStore + Send + Sync>,
    approval_gate: ApprovalGate,
}

impl SecurityManager {
    pub fn new() -> Self {
        Self {
            secret_store: Box::new(MemorySecretStore::new()),
            approval_gate: ApprovalGate::default(),
        }
    }

    pub fn check_approval(&self, tool_name: &str) -> ApprovalAction {
        self.approval_gate.evaluate(tool_name)
    }

    pub fn store_secret(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.secret_store.set(key, value)
    }

    pub fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        self.secret_store.get(key)
    }

    pub fn delete_secret(&self, key: &str) -> Result<(), SecretError> {
        self.secret_store.delete(key)
    }

    pub fn add_approval_rule(&mut self, rule: ApprovalRule) {
        self.approval_gate.rules.push(rule);
    }

    pub fn approval_rules(&self) -> &[ApprovalRule] {
        &self.approval_gate.rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_store() {
        let store = MemorySecretStore::new();
        assert!(store.set("api_key", "sk-1234").is_ok());
        assert_eq!(store.get("api_key").unwrap(), Some("sk-1234".into()));
        assert!(store.delete("api_key").is_ok());
        assert_eq!(store.get("api_key").unwrap(), None);
    }

    #[test]
    fn test_approval() {
        let manager = SecurityManager::new();
        match manager.check_approval("fs.read_file") {
            ApprovalAction::AutoApprove => {}
            _ => panic!("Expected auto-approve"),
        }
        match manager.check_approval("fs.delete_file") {
            ApprovalAction::RequireUserConfirmation => {}
            _ => panic!("Expected confirmation"),
        }
    }
}
