//! # IPC Metrics
//!
//! Integration with `lumas-performance` for monitoring IPC performance.
//! Tracks per-channel message counts, latency histograms, connection counts,
//! and security event counters.

use crate::config::MessagePriority;
use crate::message::ChannelName;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// IPC metrics collector.
pub struct IpcMetrics {
    /// Per-channel message counters
    pub messages_sent: DashMap<String, AtomicU64>,
    pub messages_received: DashMap<String, AtomicU64>,
    pub messages_dropped: DashMap<String, AtomicU64>,
    pub messages_rejected: DashMap<String, AtomicU64>,

    /// Global gauges
    pub active_connections: AtomicU64,
    pub active_streams: AtomicU64,
    pub queue_depth_total: AtomicU64,

    /// Security counters
    pub auth_failures: AtomicU64,
    pub replay_rejections: AtomicU64,
    pub ttl_expirations: AtomicU64,
}

impl IpcMetrics {
    /// Create a new metrics collector.
    pub fn new() -> Self {
        Self {
            messages_sent: DashMap::new(),
            messages_received: DashMap::new(),
            messages_dropped: DashMap::new(),
            messages_rejected: DashMap::new(),
            active_connections: AtomicU64::new(0),
            active_streams: AtomicU64::new(0),
            queue_depth_total: AtomicU64::new(0),
            auth_failures: AtomicU64::new(0),
            replay_rejections: AtomicU64::new(0),
            ttl_expirations: AtomicU64::new(0),
        }
    }

    /// Record a message sent on a channel.
    pub fn record_sent(&self, channel: &str) {
        self.messages_sent
            .entry(channel.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record a message received on a channel.
    pub fn record_received(&self, channel: &str) {
        self.messages_received
            .entry(channel.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record a dropped message.
    pub fn record_dropped(&self, channel: &str) {
        self.messages_dropped
            .entry(channel.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record a rejected message.
    pub fn record_rejected(&self, channel: &str) {
        self.messages_rejected
            .entry(channel.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record an authentication failure.
    pub fn record_auth_failure(&self) {
        self.auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a replay rejection.
    pub fn record_replay_rejection(&self) {
        self.replay_rejections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a TTL expiration.
    pub fn record_ttl_expiration(&self) {
        self.ttl_expirations.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment active connections.
    pub fn increment_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active connections.
    pub fn decrement_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get messages sent on a channel.
    pub fn sent_on(&self, channel: &str) -> u64 {
        self.messages_sent
            .get(channel)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Get messages received on a channel.
    pub fn received_on(&self, channel: &str) -> u64 {
        self.messages_received
            .get(channel)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
}

impl Default for IpcMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for IpcMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpcMetrics")
            .field("active_connections", &self.active_connections.load(Ordering::Relaxed))
            .field("active_streams", &self.active_streams.load(Ordering::Relaxed))
            .field("auth_failures", &self.auth_failures.load(Ordering::Relaxed))
            .finish()
    }
}
