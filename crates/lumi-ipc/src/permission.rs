//! # Permission System
//!
//! Defines channel-level permissions: which processes may publish to or
//! subscribe to which channels. Permissions are checked at channel
//! registration time and on every message dispatch.

use crate::error::RoutingError;
use crate::message::ProcessId;
use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::Arc;

/// An allowlist of process IDs for a specific permission type.
#[derive(Debug, Clone)]
pub struct AllowList {
    inner: HashSet<ProcessId>,
    /// If true, all processes are allowed (unless explicitly denied).
    allow_all: bool,
}

impl AllowList {
    /// Create a new empty allowlist.
    pub fn new() -> Self {
        Self {
            inner: HashSet::new(),
            allow_all: false,
        }
    }

    /// Create a new allowlist that allows all processes.
    pub fn allow_all() -> Self {
        Self {
            inner: HashSet::new(),
            allow_all: true,
        }
    }

    /// Add a process to the allowlist.
    pub fn add(&mut self, process: ProcessId) {
        self.inner.insert(process);
    }

    /// Remove a process from the allowlist.
    pub fn remove(&mut self, process: &ProcessId) {
        self.inner.remove(process);
    }

    /// Check if a process is allowed.
    pub fn contains(&self, process: &ProcessId) -> bool {
        self.allow_all || self.inner.contains(process)
    }
}

impl Default for AllowList {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<ProcessId>> for AllowList {
    fn from(v: Vec<ProcessId>) -> Self {
        Self {
            inner: v.into_iter().collect(),
            allow_all: false,
        }
    }
}

/// Permission level for a channel.
#[derive(Debug, Clone)]
pub enum PermissionLevel {
    /// Any process can publish/subscribe.
    Public,
    /// Only process with a specific capability can publish/subscribe.
    Capability(String),
    /// Only explicitly allowed processes can publish/subscribe.
    Restricted(AllowList),
}

impl Default for PermissionLevel {
    fn default() -> Self {
        PermissionLevel::Public
    }
}

/// Permissions for a single channel.
#[derive(Debug, Clone)]
pub struct ChannelPermissions {
    /// Which processes may publish to this channel.
    pub publish: PermissionLevel,
    /// Which processes may subscribe to this channel.
    pub subscribe: PermissionLevel,
    /// Whether receiving a message from an unauthorized source is an error.
    pub strict: bool,
}

impl ChannelPermissions {
    /// Create a new channel permission with public access.
    pub fn public() -> Self {
        Self {
            publish: PermissionLevel::Public,
            subscribe: PermissionLevel::Public,
            strict: false,
        }
    }

    /// Create a new channel permission with restricted access.
    pub fn restricted(publishers: AllowList, subscribers: AllowList) -> Self {
        Self {
            publish: PermissionLevel::Restricted(publishers),
            subscribe: PermissionLevel::Restricted(subscribers),
            strict: true,
        }
    }

    /// Check if a process can publish to this channel.
    pub fn can_publish(&self, process: &ProcessId) -> bool {
        match &self.publish {
            PermissionLevel::Public => true,
            PermissionLevel::Capability(_) => true, // Capability check not yet implemented
            PermissionLevel::Restricted(list) => list.contains(process),
        }
    }

    /// Check if a process can subscribe to this channel.
    pub fn can_subscribe(&self, process: &ProcessId) -> bool {
        match &self.subscribe {
            PermissionLevel::Public => true,
            PermissionLevel::Capability(_) => true,
            PermissionLevel::Restricted(list) => list.contains(process),
        }
    }
}

/// Full permission registry across all channels.
pub struct PermissionRegistry {
    /// Permissions per channel name.
    permissions: DashMap<String, Arc<ChannelPermissions>>,
    /// Default permissions for channels without explicit config.
    default_permissions: Arc<ChannelPermissions>,
}

impl PermissionRegistry {
    /// Create a new permission registry.
    pub fn new() -> Self {
        Self {
            permissions: DashMap::new(),
            default_permissions: Arc::new(ChannelPermissions::public()),
        }
    }

    /// Register permissions for a channel.
    pub fn register(
        &self,
        channel: impl Into<String>,
        permissions: ChannelPermissions,
    ) {
        self.permissions.insert(channel.into(), Arc::new(permissions));
    }

    /// Check if a process can publish to a channel.
    pub fn can_publish(
        &self,
        channel: &str,
        process: &ProcessId,
    ) -> Result<(), RoutingError> {
        let perms = self.permissions
            .get(channel)
            .map(|p| p.clone())
            .unwrap_or_else(|| self.default_permissions.clone());

        if perms.can_publish(process) {
            Ok(())
        } else {
            Err(RoutingError::PermissionDenied {
                sender: process.clone(),
                channel: channel.to_string(),
            })
        }
    }

    /// Check if a process can subscribe to a channel.
    pub fn can_subscribe(
        &self,
        channel: &str,
        process: &ProcessId,
    ) -> Result<(), RoutingError> {
        let perms = self.permissions
            .get(channel)
            .map(|p| p.clone())
            .unwrap_or_else(|| self.default_permissions.clone());

        if perms.can_subscribe(process) {
            Ok(())
        } else {
            Err(RoutingError::PermissionDenied {
                sender: process.clone(),
                channel: channel.to_string(),
            })
        }
    }
}

impl Default for PermissionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
