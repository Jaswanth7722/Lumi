//! # Channel Registry
//!
//! Manages channel definitions, transport instances, and subscriber tracking.
//! The registry is the source of truth for all active IPC channels.

use crate::config::{BackpressurePolicy, ChannelConfig, TransportKind};
use crate::message::{ChannelName, ProcessId};
use crate::transport::{SharedTransport, Transport};
use dashmap::DashMap;
use std::sync::Arc;

/// A registered channel entry.
#[derive(Debug, Clone)]
pub struct ChannelEntry {
    /// Channel name
    pub name: String,
    /// Channel configuration
    pub config: ChannelConfig,
    /// Transport instance
    pub transport: SharedTransport,
    /// Subscriber process IDs
    pub subscribers: Vec<ProcessId>,
    /// Publisher process IDs
    pub publishers: Vec<ProcessId>,
}

/// Registry of all active IPC channels.
pub struct ChannelRegistry {
    /// Channels keyed by name
    channels: DashMap<String, Arc<ChannelEntry>>,
}

impl ChannelRegistry {
    /// Create a new channel registry.
    pub fn new() -> Self {
        Self {
            channels: DashMap::new(),
        }
    }

    /// Register a channel with its transport and configuration.
    pub fn register(
        &self,
        name: impl Into<String>,
        config: ChannelConfig,
        transport: SharedTransport,
    ) -> Arc<ChannelEntry> {
        let name = name.into();
        let entry = Arc::new(ChannelEntry {
            name: name.clone(),
            config,
            transport,
            subscribers: Vec::new(),
            publishers: Vec::new(),
        });
        self.channels.insert(name, entry.clone());
        entry
    }

    /// Get a channel entry by name.
    pub fn get(&self, name: &str) -> Option<Arc<ChannelEntry>> {
        self.channels.get(name).map(|e| e.clone())
    }

    /// Remove a channel from the registry.
    pub fn remove(&self, name: &str) -> Option<Arc<ChannelEntry>> {
        self.channels.remove(name).map(|(_, e)| e)
    }

    /// Check if a channel is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.channels.contains_key(name)
    }

    /// Get the number of registered channels.
    pub fn len(&self) -> usize {
        self.channels.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }

    /// Get all registered channel names.
    pub fn channel_names(&self) -> Vec<String> {
        self.channels.iter().map(|e| e.key().clone()).collect()
    }

    /// Get all registered channel entries.
    pub fn all_channels(&self) -> Vec<Arc<ChannelEntry>> {
        self.channels.iter().map(|e| e.value().clone()).collect()
    }

    /// Get the transport for a channel.
    pub fn get_transport(&self, name: &str) -> Option<SharedTransport> {
        self.channels.get(name).map(|e| e.transport.clone())
    }

    /// Add a subscriber to a channel.
    pub fn add_subscriber(&self, name: &str, subscriber: ProcessId) -> bool {
        if let Some(mut entry) = self.channels.get_mut(name) {
            if !entry.subscribers.contains(&subscriber) {
                entry.subscribers.push(subscriber);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Remove a subscriber from a channel.
    pub fn remove_subscriber(&self, name: &str, subscriber: &ProcessId) -> bool {
        if let Some(mut entry) = self.channels.get_mut(name) {
            let len_before = entry.subscribers.len();
            entry.subscribers.retain(|s| s != subscriber);
            entry.subscribers.len() < len_before
        } else {
            false
        }
    }

    /// Get subscribers for a channel.
    pub fn subscribers(&self, name: &str) -> Vec<ProcessId> {
        self.channels
            .get(name)
            .map(|e| e.subscribers.clone())
            .unwrap_or_default()
    }

    /// Add a publisher to a channel.
    pub fn add_publisher(&self, name: &str, publisher: ProcessId) -> bool {
        if let Some(mut entry) = self.channels.get_mut(name) {
            if !entry.publishers.contains(&publisher) {
                entry.publishers.push(publisher);
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::new()
    }
}
