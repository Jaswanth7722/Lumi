//! # Peer Registry
//!
//! Manages peer process entries — their identity, capabilities, connection
//! state, and health. The peer registry is the source of truth for which
//! processes are connected to the IPC bus.

use crate::connection::Connection;
use crate::heartbeat::PeerHealth;
use crate::message::CapabilitySet;
use crate::message::ProcessId;
use dashmap::DashMap;
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Instant;

/// Process type classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessType {
    /// Main core process
    Core,
    /// GPU rendering process
    Render,
    /// Audio/voice process
    Voice,
    /// Persistent storage process
    Storage,
    /// Plugin host process
    PluginHost,
    /// Diagnostics/tooling process
    Diagnostics,
    /// External or plugin process
    External { name: Cow<'static, str> },
}

impl ProcessType {
    pub fn name(&self) -> &'static str {
        match self {
            ProcessType::Core => "core",
            ProcessType::Render => "render",
            ProcessType::Voice => "voice",
            ProcessType::Storage => "storage",
            ProcessType::PluginHost => "plugin-host",
            ProcessType::Diagnostics => "diagnostics",
            ProcessType::External { .. } => "external",
        }
    }
}

impl From<ProcessId> for ProcessType {
    fn from(id: ProcessId) -> Self {
        use ProcessId as P;
        match id {
            P::Core => ProcessType::Core,
            P::Render => ProcessType::Render,
            P::Voice => ProcessType::Voice,
            P::Storage => ProcessType::Storage,
            P::PluginHost => ProcessType::PluginHost,
            P::Plugin(name) => ProcessType::External { name: Cow::Owned(name) },
        }
    }
}

/// Entry for a connected peer process.
#[derive(Debug, Clone)]
pub struct PeerEntry {
    /// Process ID
    pub id: ProcessId,
    /// Human-readable name
    pub name: Cow<'static, str>,
    /// Process type
    pub process_type: ProcessType,
    /// Capabilities declared during handshake
    pub capabilities: CapabilitySet,
    /// Connection handle
    pub connection: Option<Arc<dyn Connection>>,
    /// Peer health monitor
    pub health: Arc<PeerHealth>,
    /// When this peer was registered
    pub registered_at: Instant,
    /// Custom metadata
    pub metadata: std::collections::HashMap<String, String>,
}

impl PeerEntry {
    /// Create a new peer entry.
    pub fn new(
        id: ProcessId,
        capabilities: CapabilitySet,
    ) -> Self {
        let process_type = ProcessType::from(id.clone());
        let name: Cow<'static, str> = Cow::Owned(id.to_string());

        Self {
            id,
            name,
            process_type,
            capabilities,
            connection: None,
            health: Arc::new(PeerHealth::new()),
            registered_at: Instant::now(),
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Set the connection for this peer.
    pub fn with_connection(mut self, conn: Arc<dyn Connection>) -> Self {
        self.connection = Some(conn);
        self
    }
}

/// Peer registry — the source of truth for connected processes.
pub struct PeerRegistry {
    /// All known peers, keyed by ProcessId
    peers: DashMap<ProcessId, Arc<PeerEntry>>,
    /// Index: channel name → set of peers that can handle it
    channel_index: DashMap<String, Vec<ProcessId>>,
}

impl PeerRegistry {
    /// Create a new peer registry.
    pub fn new() -> Self {
        Self {
            peers: DashMap::new(),
            channel_index: DashMap::new(),
        }
    }

    /// Register a peer.
    pub fn register(&self, entry: PeerEntry) {
        let id = entry.id.clone();
        let arc_entry = Arc::new(entry);

        // Index by channel
        for channel in &arc_entry.capabilities.can_publish {
            self.channel_index
                .entry(channel.clone())
                .or_insert_with(Vec::new)
                .push(id.clone());
        }

        self.peers.insert(id, arc_entry);
    }

    /// Remove a peer by ID.
    pub fn remove(&self, id: &ProcessId) -> Option<Arc<PeerEntry>> {
        if let Some((_, entry)) = self.peers.remove(id) {
            // Clean up channel index
            self.channel_index.retain(|_, peers| {
                peers.retain(|p| p != id);
                !peers.is_empty()
            });
            Some(entry)
        } else {
            None
        }
    }

    /// Look up a peer by ID.
    pub fn get(&self, id: &ProcessId) -> Option<Arc<PeerEntry>> {
        self.peers.get(id).map(|e| e.clone())
    }

    /// Check if a peer is registered.
    pub fn contains(&self, id: &ProcessId) -> bool {
        self.peers.contains_key(id)
    }

    /// Get the number of registered peers.
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// Find peers that can handle a specific channel.
    pub fn find_by_channel(&self, channel: &str) -> Vec<Arc<PeerEntry>> {
        self.channel_index
            .get(channel)
            .map(|peers| {
                peers
                    .iter()
                    .filter_map(|id| self.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all registered peers.
    pub fn all_peers(&self) -> Vec<Arc<PeerEntry>> {
        self.peers
            .iter()
            .map(|e| e.clone())
            .collect()
    }

    /// Get all healthy peers.
    pub fn healthy_peers(&self) -> Vec<Arc<PeerEntry>> {
        self.peers
            .iter()
            .filter(|e| matches!(e.health.status(), crate::event::PeerHealthStatus::Healthy))
            .map(|e| e.clone())
            .collect()
    }

    /// Update peer health.
    pub fn update_health(
        &self,
        id: &ProcessId,
        _status: crate::event::PeerHealthStatus,
    ) -> Option<()> {
        self.peers.get(id).map(|_| ())
    }
}

impl Default for PeerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
