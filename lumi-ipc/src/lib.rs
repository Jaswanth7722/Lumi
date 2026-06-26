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
//! - Cross-process transport via sockets/pipes
//! - Automatic reconnection and health monitoring

pub mod wire;
pub mod transport;
pub mod peer;

use anyhow::Result;
use dashmap::DashMap;
use lumi_common::ipc::{Channel, LumiMessage, MessageType, ProcessId};
use peer::PeerManager;
use std::path::{Path, PathBuf};
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
// Internal Types
// ---------------------------------------------------------------------------

/// A channel sender for a specific IPC channel.
type ChannelSender = broadcast::Sender<LumiMessage>;

/// A channel receiver for a specific IPC channel.
pub type ChannelReceiver = broadcast::Receiver<LumiMessage>;

/// Pending request awaiting a response.
struct PendingRequest {
    response_tx: oneshot::Sender<LumiMessage>,
    timeout_at: tokio::time::Instant,
}

// ---------------------------------------------------------------------------
// Message Bus
// ---------------------------------------------------------------------------

/// Configuration for a MessageBus instance.
pub struct BusConfig {
    /// The runtime directory for IPC sockets/pipes.
    pub runtime_dir: PathBuf,
    /// Whether to enable cross-process transport.
    pub enable_transport: bool,
    /// List of peers to automatically connect to on startup.
    pub auto_connect: Vec<ProcessId>,
}

impl Default for BusConfig {
    fn default() -> Self {
        Self {
            runtime_dir: transport::default_runtime_dir(),
            enable_transport: true,
            auto_connect: Vec::new(),
        }
    }
}

/// The central IPC message bus for the Lumi platform.
///
/// Each process creates one `MessageBus` instance and uses it to send
/// and receive messages on various channels. The bus handles:
/// - In-process message routing via broadcast channels
/// - Cross-process message routing via the PeerManager
/// - Request/response correlation with timeout
/// - Channel-based message filtering
pub struct MessageBus {
    /// This process's identifier.
    process_id: ProcessId,
    /// Bus configuration.
    config: BusConfig,
    /// Map of channel name → broadcast sender (in-process routing).
    channels: DashMap<Channel, ChannelSender>,
    /// Pending requests waiting for responses, keyed by message ID.
    pending_requests: Arc<DashMap<String, PendingRequest>>,
    /// Whether the bus is running.
    running: Arc<AtomicBool>,
    /// Receiver for all outgoing messages on this process.
    rx: mpsc::Receiver<LumiMessage>,
    /// Sender for all outgoing messages (cloned to consumers).
    tx: mpsc::Sender<LumiMessage>,
    /// Peer manager for cross-process communication.
    peer_manager: Option<Arc<PeerManager>>,
}

impl MessageBus {
    /// Create a new message bus for this process.
    pub fn new(process_id: ProcessId) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        Self {
            process_id: process_id.clone(),
            config: BusConfig::default(),
            channels: DashMap::new(),
            pending_requests: Arc::new(DashMap::new()),
            running: Arc::new(AtomicBool::new(true)),
            rx,
            tx,
            peer_manager: None,
        }
    }

    /// Create a new message bus with custom configuration.
    pub fn new_with_config(process_id: ProcessId, config: BusConfig) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        let peer_manager = if config.enable_transport {
            Some(PeerManager::new(process_id.clone()))
        } else {
            None
        };

        Self {
            process_id: process_id.clone(),
            config,
            channels: DashMap::new(),
            pending_requests: Arc::new(DashMap::new()),
            running: Arc::new(AtomicBool::new(true)),
            rx,
            tx,
            peer_manager,
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

    /// Get the bus configuration.
    pub fn config(&self) -> &BusConfig {
        &self.config
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

    /// Start the transport layer — listen for incoming connections and
    /// connect to configured peers.
    pub async fn start_transport(&mut self) -> Result<()> {
        let peer_manager = match &self.peer_manager {
            Some(pm) => pm.clone(),
            None => return Ok(()), // Transport disabled
        };

        // Start listening for incoming connections
        let _listener_handle = peer_manager
            .start_listener(&self.config.runtime_dir)
            .await?;

        // Connect to configured peers
        for peer_id in &self.config.auto_connect {
            peer_manager
                .connect_to(peer_id.clone(), &self.config.runtime_dir)
                .await;
        }

        info!(
            "Transport started for {}: {}",
            self.process_id,
            self.config.runtime_dir.display()
        );

        Ok(())
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
            PendingRequest {
                response_tx,
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
    ///
    /// The message is routed to:
    /// 1. Local channel subscribers (via broadcast)
    /// 2. Remote peer (via transport, if destination is a different process)
    pub async fn send(&self, msg: LumiMessage) -> Result<()> {
        // Route to remote peer if target is a different process
        let target = msg.target.clone();
        if target != self.process_id {
            if let Some(ref pm) = self.peer_manager {
                pm.send_to(target, msg.clone()).await;
            }
        }

        // Push to the local mpsc channel — process_message() handles all local routing
        self.tx.send(msg).await?;

        Ok(())
    }

    /// Send a broadcast event to all connected peers on a channel.
    pub async fn broadcast(&self, channel: Channel, msg: LumiMessage) -> Result<()> {
        // Local broadcast
        if let Some(tx) = self.channels.get(&channel) {
            let _ = tx.send(msg.clone());
        }

        // Remote broadcast
        if let Some(ref pm) = self.peer_manager {
            pm.broadcast(&msg).await;
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
            "IPC message: {} → {} [{:?}] via {}\n  payload: {}",
            msg.source,
            msg.target,
            msg.msg_type,
            msg.channel,
            serde_json::to_string(&msg.payload).unwrap_or_default()
        );

        // Handle responses: complete pending requests
        if msg.msg_type == MessageType::Response || msg.msg_type == MessageType::Error {
            if let Some((_, pending)) = self.pending_requests.remove(&msg.id) {
                let _ = pending.response_tx.send(msg);
                return Ok(());
            }
        }

        // Broadcast to local channel subscribers
        if let Some(tx) = self.channels.get(&msg.channel) {
            let _ = tx.send(msg);
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
                drop(pending);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Process Configuration Helpers
// ---------------------------------------------------------------------------

/// Default peer connections for each process type.
pub fn default_peers_for(process_id: &ProcessId) -> Vec<ProcessId> {
    match process_id {
        ProcessId::Core => vec![
            ProcessId::Render,
            ProcessId::Voice,
            ProcessId::Storage,
            ProcessId::PluginHost,
        ],
        ProcessId::Render | ProcessId::Voice | ProcessId::Storage | ProcessId::PluginHost => {
            vec![ProcessId::Core]
        }
        ProcessId::Plugin(_) => vec![ProcessId::PluginHost, ProcessId::Core],
    }
}

/// Create a properly configured MessageBus for a given process.
pub fn create_bus(process_id: ProcessId, runtime_dir: Option<PathBuf>) -> MessageBus {
    let mut config = BusConfig {
        enable_transport: true,
        auto_connect: default_peers_for(&process_id),
        runtime_dir: runtime_dir.unwrap_or_else(transport::default_runtime_dir),
    };

    // Core doesn't need to connect to itself
    config.auto_connect.retain(|p| p != &process_id);

    MessageBus::new_with_config(process_id, config)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
        )
        .unwrap();

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
        )
        .unwrap();

        bus.broadcast(Channel::AiState, msg.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.channel, Channel::AiState);
        assert_eq!(received.payload, serde_json::json!({"state": "thinking"}));
    }

    #[tokio::test]
    async fn test_message_bus_send_recv() {
        let bus = MessageBus::new(ProcessId::Core);

        let msg = LumiMessage::new_request(
            ProcessId::Core,
            ProcessId::Storage,
            Channel::MemoryQuery,
            serde_json::json!({"query": "test"}),
        )
        .unwrap();

        bus.send(msg).await.unwrap();

        let received = bus.recv().await.unwrap();
        assert_eq!(received.target, ProcessId::Storage);
        assert_eq!(received.channel, Channel::MemoryQuery);
    }

    #[test]
    fn test_default_peers() {
        let core_peers = default_peers_for(&ProcessId::Core);
        assert!(core_peers.contains(&ProcessId::Render));
        assert!(core_peers.contains(&ProcessId::Voice));
        assert!(core_peers.contains(&ProcessId::Storage));
        assert!(core_peers.contains(&ProcessId::PluginHost));

        let render_peers = default_peers_for(&ProcessId::Render);
        assert_eq!(render_peers, vec![ProcessId::Core]);
    }

    #[tokio::test]
    async fn test_message_bus_with_config() {
        let config = BusConfig {
            enable_transport: false, // Disable transport for test
            ..Default::default()
        };
        let bus = MessageBus::new_with_config(ProcessId::Core, config);
        assert!(bus.peer_manager.is_none());
    }

    #[test]
    fn test_create_bus() {
        let bus = create_bus(ProcessId::Core, None);
        assert_eq!(*bus.process_id(), ProcessId::Core);
        assert!(!bus.config.auto_connect.contains(&ProcessId::Core));
    }
}
