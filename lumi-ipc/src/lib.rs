//! # Lumi IPC — Inter-Process Communication Bus (Chapter 5)
//!
//! Typed message bus implemented over Unix domain sockets (macOS/Linux)
//! or named pipes (Windows). Messages are serialized via MessagePack for
//! performance with JSON fallback for debugging.
//!
//! The bus provides:
//! - Request/response correlation with optional tracing
//! - Broadcast event delivery to multiple subscribers
//! - Channel-based message routing
//! - Error handling and timeout support

use anyhow::Result;
use dashmap::DashMap;
use lumi_common::ipc::{Channel, LumiMessage, MessageType, ProcessId};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Message Serialization
// ---------------------------------------------------------------------------

/// Serialize a LumiMessage to MessagePack bytes.
pub fn serialize_message(msg: &LumiMessage) -> Result<Vec<u8>> {
    Ok(rmp_serde::to_vec(msg)?)
}

/// Deserialize a LumiMessage from MessagePack bytes.
pub fn deserialize_message(bytes: &[u8]) -> Result<LumiMessage> {
    Ok(rmp_serde::from_slice(bytes)?)
}

/// Serialize a LumiMessage to JSON string (for debugging).
pub fn serialize_message_json(msg: &LumiMessage) -> Result<String> {
    Ok(serde_json::to_string_pretty(msg)?)
}

// ---------------------------------------------------------------------------
// Message Bus
// ---------------------------------------------------------------------------

/// A channel sender for a specific IPC channel.
type ChannelSender = broadcast::Sender<LumiMessage>;

/// A channel receiver for a specific IPC channel.
type ChannelReceiver = broadcast::Receiver<LumiMessage>;

/// Pending request awaiting a response.
struct PendingRequest {
    response_tx: oneshot::Sender<LumiMessage>,
    timeout_at: tokio::time::Instant,
}

/// The central IPC message bus for the Lumi platform.
///
/// Each process creates one `MessageBus` instance and uses it to send
/// and receive messages on various channels.
pub struct MessageBus {
    /// This process's identifier.
    process_id: ProcessId,
    /// Map of channel name → broadcast sender.
    channels: DashMap<Channel, ChannelSender>,
    /// Pending requests waiting for responses, keyed by message ID.
    pending_requests: Arc<DashMap<String, PendingRequest>>,
    /// Whether the bus is running.
    running: Arc<AtomicBool>,
    /// Receiver for all incoming messages on this process.
    rx: mpsc::Receiver<LumiMessage>,
    /// Sender for all outgoing messages.
    tx: mpsc::Sender<LumiMessage>,
}

impl MessageBus {
    /// Create a new message bus for this process.
    pub fn new(process_id: ProcessId) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        Self {
            process_id,
            channels: DashMap::new(),
            pending_requests: Arc::new(DashMap::new()),
            running: Arc::new(AtomicBool::new(true)),
            rx,
            tx,
        }
    }

    /// Get a sender for sending messages into the bus.
    pub fn sender(&self) -> mpsc::Sender<LumiMessage> {
        self.tx.clone()
    }

    /// Get the process ID of this bus instance.
    pub fn process_id(&self) -> &ProcessId {
        &self.process_id
    }

    /// Subscribe to a specific IPC channel.
    ///
    /// Returns a receiver that will receive all messages on that channel.
    pub fn subscribe(&self, channel: Channel) -> ChannelReceiver {
        self.channels
            .entry(channel)
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(256);
                tx
            })
            .value()
            .subscribe()
    }

    /// Send a message and wait for a response.
    ///
    /// The message must be of type `Request`. This function will wait
    /// for a correlated `Response` or `Error` message.
    pub async fn request(&self, msg: LumiMessage) -> Result<LumiMessage> {
        let (response_tx, response_rx) = oneshot::channel();

        let msg_id = msg.id.clone();
        let timeout = tokio::time::Duration::from_secs(30);
        self.pending_requests.insert(
            msg_id.clone(),
            PendingRequest {            response_tx,
                    timeout_at: tokio::time::Instant::now() + timeout,
                },
            );

        self.send(msg).await?;

        tokio::select! {
            response = response_rx => {
                match response {
                    Ok(msg) => Ok(msg),
                    Err(_) => anyhow::bail!("Response channel closed"),
                }
            }
            _ = tokio::time::sleep(timeout) => {
                self.pending_requests.remove(&msg_id);
                anyhow::bail!("Request timed out after {:?}", timeout);
            }
        }
    }

    /// Send a message on the bus (fire-and-forget).
    pub async fn send(&self, msg: LumiMessage) -> Result<()> {
        self.tx.send(msg).await?;
        Ok(())
    }

    /// Send a broadcast event on a channel.
    pub async fn broadcast(&self, channel: Channel, msg: LumiMessage) -> Result<()> {
        if let Some(tx) = self.channels.get(&channel) {
            tx.send(msg)?;
        }
        Ok(())
    }

    /// Receive the next message from the bus.
    pub async fn recv(&mut self) -> Option<LumiMessage> {
        self.rx.recv().await
    }

    /// Process an incoming message (routes to subscribers, handles responses).
    pub async fn process_message(&self, msg: LumiMessage) -> Result<()> {
        debug!(
            "IPC message: {} → {} [{:?}] via {}",
            msg.source, msg.target, msg.msg_type, msg.channel
        );

        // Handle responses: complete pending requests
        if msg.msg_type == MessageType::Response || msg.msg_type == MessageType::Error {
            if let Some((_, pending)) = self.pending_requests.remove(&msg.id) {
                let _ = pending.response_tx.send(msg);
                return Ok(());
            }
        }

        // Broadcast to channel subscribers
        if let Some(tx) = self.channels.get(&msg.channel) {
            tx.send(msg)?;
        }

        Ok(())
    }

    /// Run the message bus event loop.
    ///
    /// Processes incoming messages until the bus is shut down.
    pub async fn run(&mut self) {
        info!("IPC bus started for process: {}", self.process_id);

        while self.running.load(Ordering::Relaxed) {
            tokio::select! {
                Some(msg) = self.rx.recv() => {
                    if let Err(e) = self.process_message(msg).await {
                        error!("Error processing IPC message: {e}");
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // Periodic cleanup of timed-out pending requests
                    self.cleanup_pending_requests();
                }
            }
        }

        info!("IPC bus shut down for process: {}", self.process_id);
    }

    /// Shut down the message bus.
    pub fn shutdown(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// Remove pending requests that have timed out.
    fn cleanup_pending_requests(&self) {
        let now = tokio::time::Instant::now();
        let timed_out: Vec<String> = self
            .pending_requests
            .iter()
            .filter(|entry| entry.timeout_at < now)
            .map(|entry| entry.key().clone())
            .collect();

        for id in timed_out {
            if let Some((_, pending)) = self.pending_requests.remove(&id) {
                warn!("Pending request {id} timed out");
                drop(pending); // drops the sender, signaling timeout to the caller
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Transport Layer
// ---------------------------------------------------------------------------

/// Platform-specific transport for IPC messages.
pub enum Transport {
    /// Unix domain socket path (macOS/Linux).
    UnixSocket(String),
    /// Named pipe path (Windows).
    NamedPipe(String),
    /// In-memory channel (for testing within the same process).
    InMemory,
}

impl Transport {
    /// Create a platform-appropriate transport for this process.
    #[allow(unused_variables)]
    pub fn for_process(process_id: &ProcessId, runtime_dir: &std::path::Path) -> Self {
        let name = process_id.to_string();
        #[cfg(unix)]
        {
            let path = runtime_dir.join(format!("lumi-{name}.sock"));
            Transport::UnixSocket(path.to_string_lossy().to_string())
        }
        #[cfg(windows)]
        {
            let path = format!(r"\\.\pipe\lumi-{name}");
            Transport::NamedPipe(path)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = runtime_dir;
            Transport::InMemory
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumi_common::ipc::Channel;

    #[test]
    fn test_message_serialization_roundtrip() {
        let msg = LumiMessage::new_request(
            ProcessId::Core,
            ProcessId::Render,
            Channel::RenderCommand,
            serde_json::json!({"animation": "walk"}),
        ).unwrap();

        let bytes = serialize_message(&msg).unwrap();
        let deserialized = deserialize_message(&bytes).unwrap();

        assert_eq!(msg.id, deserialized.id);
        assert_eq!(msg.source, deserialized.source);
        assert_eq!(msg.target, deserialized.target);
        assert_eq!(msg.channel, deserialized.channel);
        assert_eq!(msg.msg_type, deserialized.msg_type);
    }

    #[tokio::test]
    async fn test_message_bus_broadcast() {
        let bus = MessageBus::new(ProcessId::Core);

        let mut rx = bus.subscribe(Channel::AiState);

        let msg = LumiMessage::new_event(
            ProcessId::Core,
            Channel::AiState,
            serde_json::json!({"state": "thinking"}),
        ).unwrap();

        bus.broadcast(Channel::AiState, msg.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.channel, Channel::AiState);
        assert_eq!(received.payload, serde_json::json!({"state": "thinking"}));
    }

    #[tokio::test]
    async fn test_message_serialization_json() {
        let msg = LumiMessage::new_request(
            ProcessId::Core,
            ProcessId::Storage,
            Channel::MemoryQuery,
            serde_json::json!({"query": "test"}),
        ).unwrap();

        let json = serialize_message_json(&msg).unwrap();
        assert!(json.contains(r#""MemoryQuery""#));
        assert!(json.contains(r#""Core""#));
        assert!(json.contains(r#""Storage""#));
    }
}
