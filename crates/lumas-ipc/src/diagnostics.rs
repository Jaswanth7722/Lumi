//! # Diagnostics and Visualization
//!
//! Provides diagnostics for the IPC framework: message history, statistics,
//! and integration with the state machine diagnostics system.

use crate::bus::MessageBus;
use crate::message::{LumiMessage, MessageId, ProcessId};

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::SystemTime;

/// A recorded message in the history store.
#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub message_id: MessageId,
    pub channel: String,
    pub sender: ProcessId,
    pub receiver: String,
    pub kind: String,
    pub size_bytes: u32,
    pub timestamp: SystemTime,
    pub duration_us: Option<u64>,
}

/// Transition history store for message tracking.
pub struct MessageHistoryStore {
    /// Maximum number of records to keep
    max_records: usize,
    /// Ring buffer of message records
    records: std::sync::Mutex<Vec<MessageRecord>>,
    /// Current index for ring buffer writes
    index: std::sync::atomic::AtomicUsize,
}

impl MessageHistoryStore {
    /// Create a new history store.
    pub fn new(max_records: usize) -> Self {
        Self {
            max_records,
            records: std::sync::Mutex::new(Vec::with_capacity(max_records)),
            index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Record a message in the history store.
    pub fn record(&self, msg: &LumiMessage, duration_us: Option<u64>) {
        let record = MessageRecord {
            message_id: msg.id.clone(),
            channel: msg.channel.0.clone(),
            sender: msg.sender.clone(),
            receiver: format!("{:?}", msg.receiver),
            kind: format!("{:?}", msg.kind),
            size_bytes: rmp_serde::to_vec(msg).map(|b| b.len() as u32).unwrap_or(0),
            timestamp: SystemTime::now(),
            duration_us,
        };

        if let Ok(mut records) = self.records.lock() {
            let idx = self.index.fetch_add(1, Ordering::Relaxed) % self.max_records;
            if idx < records.len() {
                records[idx] = record;
            } else {
                records.push(record);
            }
        }
    }

    /// Get the message history.
    pub fn history(&self) -> Vec<MessageRecord> {
        self.records.lock().unwrap().clone()
    }

    /// Clear the history store.
    pub fn clear(&self) {
        if let Ok(mut records) = self.records.lock() {
            records.clear();
        }
    }
}

/// IPC system diagnostics.
pub struct IpcDiagnostics {
    /// Message bus reference
    bus: Arc<MessageBus>,
    /// Message history store
    history: Arc<MessageHistoryStore>,
}

impl IpcDiagnostics {
    /// Create a new diagnostics instance.
    pub fn new(bus: Arc<MessageBus>) -> Self {
        Self {
            bus,
            history: Arc::new(MessageHistoryStore::new(5000)),
        }
    }

    /// Get the message history store.
    pub fn history(&self) -> &Arc<MessageHistoryStore> {
        &self.history
    }

    /// Get statistics for a specific channel.
    pub fn channel_statistics(&self, channel: &str) -> ChannelStats {
        let history = self.history.history();
        let channel_records: Vec<_> = history.iter()
            .filter(|r| r.channel == channel)
            .collect();

        let total = channel_records.len();
        let total_bytes: u32 = channel_records.iter().map(|r| r.size_bytes).sum();

        ChannelStats {
            channel: channel.to_string(),
            total_messages: total,
            total_bytes,
            avg_size_bytes: if total > 0 { total_bytes / total as u32 } else { 0 },
        }
    }

    /// Get a platform snapshot of all channels and their state.
    pub fn platform_snapshot(&self) -> PlatformSnapshot {
        let mut channels = Vec::new();

        for name in self.bus.channels.channel_names() {
            let stats = self.channel_statistics(&name);
            channels.push(stats);
        }

        PlatformSnapshot {
            channels,
            active_connections: self.bus.connections.len(),
            timestamp: SystemTime::now(),
        }
    }
}

/// Statistics for a single channel.
#[derive(Debug, Clone)]
pub struct ChannelStats {
    pub channel: String,
    pub total_messages: u64,
    pub total_bytes: u32,
    pub avg_size_bytes: u32,
}

/// Platform-level snapshot of the IPC system.
#[derive(Debug, Clone)]
pub struct PlatformSnapshot {
    pub channels: Vec<ChannelStats>,
    pub active_connections: usize,
    pub timestamp: SystemTime,
}
