//! # Test Utilities
//!
//! Provides test helpers for integration testing of the IPC framework:
//! - `TestBus` — fully functional in-memory bus
//! - `MockTransport` — records sent messages, delivers injected messages
//! - `PeerPair` — two connected in-process peers
//! - Assertion macros for message verification

use crate::bus::MessageBus;
use crate::error::{TransportError, IpcResult};
use crate::message::{LumiMessage, ProcessId};
use crate::transport::{Transport, TransportMetrics, TransportTier};
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// A fully functional in-memory bus for integration tests.
pub struct TestBus {
    bus: Arc<MessageBus>,
    /// Received messages buffer
    received: Arc<Mutex<Vec<LumiMessage>>>,
}

impl TestBus {
    /// Create a new test bus for a process.
    pub fn new(process_id: ProcessId) -> Self {
        let bus = Arc::new(MessageBus::new(process_id));
        Self {
            bus,
            received: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get the underlying bus.
    pub fn bus(&self) -> &Arc<MessageBus> {
        &self.bus
    }

    /// Record a received message.
    pub fn record(&self, msg: LumiMessage) {
        if let Ok(mut received) = self.received.lock() {
            received.push(msg);
        }
    }

    /// Get all received messages.
    pub fn received_messages(&self) -> Vec<LumiMessage> {
        self.received.lock().unwrap().clone()
    }

    /// Clear received messages.
    pub fn clear(&self) {
        if let Ok(mut received) = self.received.lock() {
            received.clear();
        }
    }
}

/// Mock transport that records sent messages and delivers injected messages.
pub struct MockTransport {
    /// Channel name
    channel: String,
    /// Recorded sent messages
    sent: Arc<Mutex<Vec<LumiMessage>>>,
    /// Messages to deliver on recv()
    to_deliver: Arc<Mutex<Vec<LumiMessage>>>,
    /// Metrics
    send_count: AtomicU64,
    recv_count: AtomicU64,
    /// Whether to fail on send
    fail_send: std::sync::atomic::AtomicBool,
}

impl MockTransport {
    /// Create a new mock transport.
    pub fn new(channel: &str) -> Self {
        Self {
            channel: channel.to_string(),
            sent: Arc::new(Mutex::new(Vec::new())),
            to_deliver: Arc::new(Mutex::new(Vec::new())),
            send_count: AtomicU64::new(0),
            recv_count: AtomicU64::new(0),
            fail_send: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Get recorded sent messages.
    pub fn sent_messages(&self) -> Vec<LumiMessage> {
        self.sent.lock().unwrap().clone()
    }

    /// Inject a message to be returned by recv().
    pub fn inject(&self, msg: LumiMessage) {
        if let Ok(mut to_deliver) = self.to_deliver.lock() {
            to_deliver.push(msg);
        }
    }

    /// Set whether send should fail.
    pub fn set_fail_send(&self, fail: bool) {
        self.fail_send.store(fail, Ordering::Relaxed);
    }

    /// Clear recorded sent messages.
    pub fn clear(&self) {
        if let Ok(mut sent) = self.sent.lock() {
            sent.clear();
        }
    }
}

#[async_trait]
impl Transport for MockTransport {
    fn tier(&self) -> TransportTier {
        TransportTier::InProcess
    }

    fn name(&self) -> &'static str {
        "mock"
    }

    fn channel(&self) -> &str {
        &self.channel
    }

    async fn send(&self, msg: LumiMessage) -> Result<(), TransportError> {
        if self.fail_send.load(Ordering::Relaxed) {
            return Err(TransportError::Io("Mock send failure".into()));
        }
        self.send_count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut sent) = self.sent.lock() {
            sent.push(msg);
        }
        Ok(())
    }

    async fn recv(&self) -> Result<LumiMessage, TransportError> {
        if let Ok(mut to_deliver) = self.to_deliver.lock() {
            if let Some(msg) = to_deliver.pop() {
                self.recv_count.fetch_add(1, Ordering::Relaxed);
                return Ok(msg);
            }
        }
        Err(TransportError::NotConnected)
    }

    fn try_recv(&self) -> Result<Option<LumiMessage>, TransportError> {
        if let Ok(mut to_deliver) = self.to_deliver.lock() {
            if let Some(msg) = to_deliver.pop() {
                self.recv_count.fetch_add(1, Ordering::Relaxed);
                return Ok(Some(msg));
            }
        }
        Ok(None)
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }

    fn metrics(&self) -> TransportMetrics {
        TransportMetrics {
            messages_sent: self.send_count.load(Ordering::Relaxed),
            messages_received: self.recv_count.load(Ordering::Relaxed),
            ..Default::default()
        }
    }
}

impl std::fmt::Debug for MockTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockTransport")
            .field("channel", &self.channel)
            .field("sent", &self.sent.lock().map(|s| s.len()).unwrap_or(0))
            .finish()
    }
}

/// A pair of connected peers for testing.
pub struct PeerPair {
    /// First peer's bus
    pub peer_a: Arc<TestBus>,
    /// Second peer's bus
    pub peer_b: Arc<TestBus>,
}

impl PeerPair {
    /// Create a new peer pair with Core and Render.
    pub fn new() -> Self {
        Self {
            peer_a: Arc::new(TestBus::new(ProcessId::Core)),
            peer_b: Arc::new(TestBus::new(ProcessId::Render)),
        }
    }
}

// ---------------------------------------------------------------------------
// Assertion macros
// ---------------------------------------------------------------------------

/// Assert that a bus received a specific channel of messages.
#[macro_export]
macro_rules! assert_message_received {
    ($bus:expr, $channel:expr) => {
        let received = $bus.received_messages();
        let found = received.iter().any(|m| m.channel.0 == $channel);
        assert!(
            found,
            "Expected message on channel '{}' but none found. Received channels: {:?}",
            $channel,
            received.iter().map(|m| m.channel.0.clone()).collect::<Vec<_>>()
        );
    };
    ($bus:expr, $channel:expr, $count:expr) => {
        let received = $bus.received_messages();
        let channel_count = received.iter().filter(|m| m.channel.0 == $channel).count();
        assert_eq!(
            channel_count, $count,
            "Expected {} messages on channel '{}' but found {}",
            $count, $channel, channel_count
        );
    };
}

/// Inject a replay attack for testing replay prevention.
#[macro_export]
macro_rules! inject_replay_attack {
    ($bus:expr, $peer:expr, $msg:expr) => {
        // Clone the message to simulate replay
        let replay_msg = $msg.clone();
        $bus.record(replay_msg);
    };
}
