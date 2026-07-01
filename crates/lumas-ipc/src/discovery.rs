//! # Service Discovery
//!
//! Service registry for discovering which processes handle which channels.
//! Supports service registration, deregistration, health tracking,
//! and channel-based lookup.

use crate::event::PeerHealthStatus;
use crate::message::{CapabilitySet, ProcessId};
use dashmap::DashMap;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Unique service identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceId(pub String);

impl From<String> for ServiceId {
    fn from(s: String) -> Self {
        ServiceId(s)
    }
}

impl From<&str> for ServiceId {
    fn from(s: &str) -> Self {
        ServiceId(s.to_string())
    }
}

/// A registered service entry.
#[derive(Debug, Clone)]
pub struct ServiceEntry {
    pub id: ServiceId,
    pub name: Cow<'static, str>,
    pub process: ProcessId,
    pub channels: Vec<String>,
    pub capabilities: CapabilitySet,
    pub health: PeerHealthStatus,
    pub registered_at: Instant,
    pub metadata: HashMap<String, String>,
}

impl ServiceEntry {
    /// Create a new service entry.
    pub fn new(
        name: impl Into<Cow<'static, str>>,
        process: ProcessId,
        channels: Vec<String>,
    ) -> Self {
        Self {
            id: ServiceId(process.to_string()),
            name: name.into(),
            process,
            channels,
            capabilities: CapabilitySet::default(),
            health: PeerHealthStatus::Healthy,
            registered_at: Instant::now(),
            metadata: HashMap::new(),
        }
    }

    /// Set the capabilities for this service.
    pub fn with_capabilities(mut self, caps: CapabilitySet) -> Self {
        self.capabilities = caps;
        self
    }

    /// Add metadata to this service entry.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Service registry for service discovery.
pub struct ServiceRegistry {
    /// Services keyed by ID
    services: DashMap<ServiceId, Arc<ServiceEntry>>,
    /// Index: channel name → set of service IDs
    channel_index: DashMap<String, Vec<ServiceId>>,
}

impl ServiceRegistry {
    /// Create a new service registry.
    pub fn new() -> Self {
        Self {
            services: DashMap::new(),
            channel_index: DashMap::new(),
        }
    }

    /// Register a service.
    pub fn register(&self, entry: ServiceEntry) {
        let id = entry.id.clone();
        let channels = entry.channels.clone();
        let arc_entry = Arc::new(entry);

        // Index by channel
        for channel in &channels {
            self.channel_index
                .entry(channel.clone())
                .or_insert_with(Vec::new)
                .push(id.clone());
        }

        self.services.insert(id, arc_entry);
    }

    /// Remove a service by ID.
    pub fn deregister(&self, id: &ServiceId) -> Option<Arc<ServiceEntry>> {
        if let Some((_, entry)) = self.services.remove(id) {
            // Clean up channel index
            for channel in &entry.channels {
                self.channel_index.retain(|_, services| {
                    services.retain(|s| s != id);
                    !services.is_empty()
                });
            }
            Some(entry)
        } else {
            None
        }
    }

    /// Look up a service by ID.
    pub fn get(&self, id: &ServiceId) -> Option<Arc<ServiceEntry>> {
        self.services.get(id).map(|e| e.clone())
    }

    /// Find services by channel name.
    pub fn find_by_channel(&self, channel: &str) -> Vec<Arc<ServiceEntry>> {
        self.channel_index
            .get(channel)
            .map(|services| {
                services
                    .iter()
                    .filter_map(|id| self.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all registered services.
    pub fn all_services(&self) -> Vec<Arc<ServiceEntry>> {
        self.services.iter().map(|e| e.value().clone()).collect()
    }

    /// Get all healthy services.
    pub fn healthy_services(&self) -> Vec<Arc<ServiceEntry>> {
        self.services
            .iter()
            .filter(|e| matches!(e.health, PeerHealthStatus::Healthy))
            .map(|e| e.value().clone())
            .collect()
    }

    /// Update the health status of a service.
    pub fn update_health(&self, id: &ServiceId, health: PeerHealthStatus) -> Option<()> {
        self.services.get_mut(id).map(|mut entry| {
            entry.health = health;
        })
    }

    /// Get the number of registered services.
    pub fn len(&self) -> usize {
        self.services.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }

    /// Check if a service is registered.
    pub fn contains(&self, id: &ServiceId) -> bool {
        self.services.contains_key(id)
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
