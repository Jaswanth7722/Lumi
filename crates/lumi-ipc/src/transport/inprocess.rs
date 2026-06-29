//! # In-Process Transport (Tier 3)
//!
//! Wraps `tokio::sync::broadcast` for pub/sub and `tokio::sync::mpsc` for
//! request/response patterns. No serialization overhead — passes `LumiMessage`
//! by value with zero-copy semantics.
//!
//! Used for `desktop.event` and any channel where both endpoints are
//! in the same process (nanosecond latency).

use crate::error::TransportError;
use crate::message::LumiMessage;
use crate::transport::{Transport, TransportMetrics, TransportTier};
use async_trait::async_trait;
use crossbeam::queue::SegQueue;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

/// In-process transport implementation.
///
/// Uses a broadcast channel for pub/sub and a SegQueue for buffering.
/// No serialization is performed — messages are passed by value.
pub struct InProcessTransport {
    /// Channel name
    channel: String,
    /// Broadcast sender for pub/sub
    tx: broadcast::Sender<LumiMessage>,
    /// Buffer for messages in case no receiver is active
    buffer: Arc<SegQueue<LumiMessage>>,
    /// Metrics
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    /// Maximum payload size (not enforced in-process, but tracked)
    max_payload_bytes: u32,
    /// Closed flag
    closed: std::sync::atomic::AtomicBool,
}

impl InProcessTransport {
    /// Create a new in-process transport.
    pub fn new(channel: &str, max_payload_bytes: u32) -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            channel: channel.to_string(),
            tx,
            buffer: Arc::new(SegQueue::new()),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            max_payload_bytes,
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Get a broadcast receiver for subscribing.
    pub fn subscribe(&self) -> broadcast::Receiver<LumiMessage> {
        self.tx.subscribe()
    }

    /// Get the maximum payload size.
    pub fn max_payload_bytes(&self) -> u32 {
        self.max_payload_bytes
    }
}

#[async_trait]
impl Transport for InProcessTransport {
    fn tier(&self) -> TransportTier {
        TransportTier::InProcess
    }

    fn name(&self) -> &'static str {
        "in-process"
    }

    fn channel(&self) -> &str {
        &self.channel
    }

    async fn send(&self, msg: LumiMessage) -> Result<(), TransportError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(TransportError::Closed);
        }

        self.messages_sent.fetch_add(1, Ordering::Relaxed);

        // Broadcast to all subscribers
        let subscriber_count = self.tx.receiver_count();
        if subscriber_count > 0 {
            let _ = self.tx.send(msg.clone());
        } else {
            // Buffer message if no active subscriber
            self.buffer.push(msg);
        }

        Ok(())
    }

    async fn recv(&self) -> Result<LumiMessage, TransportError> {
        // Check buffer first
        if let Some(msg) = self.buffer.pop() {
            self.messages_received.fetch_add(1, Ordering::Relaxed);
            return Ok(msg);
        }

        // Otherwise fall back to creating a new receiver
        // Note: In practice, the bus layer manages receivers, not the transport
        Err(TransportError::NotConnected)
    }

    fn try_recv(&self) -> Result<Option<LumiMessage>, TransportError> {
        if let Some(msg) = self.buffer.pop() {
            self.messages_received.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(msg));
        }
        Ok(None)
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.closed.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn metrics(&self) -> TransportMetrics {
        TransportMetrics {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            ..Default::default()
        }
    }
}

impl std::fmt::Debug for InProcessTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InProcessTransport")
            .field("channel", &self.channel)
            .field("subscriber_count", &self.tx.receiver_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_send_and_subscribe() {
        let transport = InProcessTransport::new("test.channel", 1024);
        let mut rx = transport.subscribe();

        let msg = LumiMessage::new_event(
            crate::message::ProcessId::Core,
            "test.channel",
            crate::message::MessagePayload::Empty,
        );

        transport.send(msg.clone()).await.unwrap();

        // Receive on the subscriber
        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, msg.id);
    }

    #[tokio::test]
    async fn test_buffer_when_no_subscriber() {
        let transport = InProcessTransport::new("test.channel", 1024);

        let msg = LumiMessage::new_event(
            crate::message::ProcessId::Core,
            "test.channel",
            crate::message::MessagePayload::Empty,
        );

        transport.send(msg.clone()).await.unwrap();

        // Message should be buffered
        let received = transport.try_recv().unwrap().unwrap();
        assert_eq!(received.id, msg.id);
    }

    #[test]
    fn test_transport_tier() {
        let transport = InProcessTransport::new("test", 1024);
        assert_eq!(transport.tier(), TransportTier::InProcess);
    }
}
