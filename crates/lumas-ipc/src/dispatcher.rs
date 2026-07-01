//! # Message Dispatcher
//!
//! Delivers messages to channel subscribers. Responsible for fan-out
//! to multiple subscribers and handling subscription lifecycle.

use crate::message::{LumiMessage, ProcessId};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

/// A subscriber handle wraps a broadcast receiver.
#[derive(Debug)]
pub struct SubscriberHandle {
    /// Subscriber process ID
    pub id: ProcessId,
    /// Channel subscribed to
    pub channel: String,
    /// Receiver for messages
    pub rx: tokio::sync::Mutex<broadcast::Receiver<LumiMessage>>,
    /// Whether the subscriber is active
    pub active: bool,
}

impl SubscriberHandle {
    pub fn new(id: ProcessId, channel: String, rx: broadcast::Receiver<LumiMessage>) -> Self {
        Self {
            id,
            channel,
            rx: tokio::sync::Mutex::new(rx),
            active: true,
        }
    }
}

/// Subscriber identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubscriberId(pub String);

impl From<ProcessId> for SubscriberId {
    fn from(p: ProcessId) -> Self {
        SubscriberId(p.to_string())
    }
}

/// Dispatcher for delivering messages to subscribers.
pub struct Dispatcher {
    /// Subscribers keyed by ID
    subscribers: DashMap<SubscriberId, Arc<SubscriberHandle>>,
    /// Index: channel → set of subscriber IDs
    channel_index: DashMap<String, Vec<SubscriberId>>,
}

impl Dispatcher {
    /// Create a new dispatcher.
    pub fn new() -> Self {
        Self {
            subscribers: DashMap::new(),
            channel_index: DashMap::new(),
        }
    }

    /// Register a subscriber.
    pub fn register(
        &self,
        id: impl Into<SubscriberId>,
        channel: impl Into<String>,
        rx: broadcast::Receiver<LumiMessage>,
    ) {
        let id = id.into();
        let channel = channel.into();

        let handle = Arc::new(SubscriberHandle::new(
            ProcessId::Core,
            channel.clone(),
            rx,
        ));

        self.subscribers.insert(id.clone(), handle);
        self.channel_index
            .entry(channel)
            .or_insert_with(Vec::new)
            .push(id);
    }

    /// Remove a subscriber.
    pub fn remove(&self, id: &SubscriberId) {
        if let Some((_, handle)) = self.subscribers.remove(id) {
            self.channel_index.retain(|_, subscribers| {
                subscribers.retain(|s| s != id);
                !subscribers.is_empty()
            });
        }
    }

    /// Get subscribers for a channel.
    pub fn subscribers_for_channel(&self, channel: &str) -> Vec<Arc<SubscriberHandle>> {
        self.channel_index
            .get(channel)
            .map(|subscribers| {
                subscribers
                    .iter()
                    .filter_map(|id| self.subscribers.get(id).map(|h| h.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Dispatch a message to all subscribers of its channel.
    pub fn dispatch(&self, msg: &LumiMessage) -> usize {
        let subscribers = self.subscribers_for_channel(&msg.channel.0);
        let mut delivered = 0;

        for handle in &subscribers {
            // Try to send — if the channel is full, the message is dropped
            // (per-channel backpressure policy determines handling)
            let _ = handle.rx.try_lock().map(|mut rx| {
                let _ = rx.try_send(msg.clone());
                delivered += 1;
            });
        }

        delivered
    }

    /// Get the subscriber count for a channel.
    pub fn subscriber_count(&self, channel: &str) -> usize {
        self.channel_index
            .get(channel)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    /// Get total subscriber count across all channels.
    pub fn total_subscribers(&self) -> usize {
        self.subscribers.len()
    }

    /// Check if a subscriber is registered.
    pub fn has_subscriber(&self, id: &SubscriberId) -> bool {
        self.subscribers.contains_key(id)
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}
