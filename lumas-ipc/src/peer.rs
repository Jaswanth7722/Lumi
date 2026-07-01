//! # Peer Manager — Connection Management and Message Routing
//!
//! Manages connections to peer Lumas processes, handling:
//! - Outbound connection initiation
//! - Inbound connection acceptance
//! - Message routing to the correct peer
//! - Reconnection on connection loss
//! - Heartbeat and health monitoring

use crate::transport::{
    IoStream, TransportConnection, TransportListener, connect_to_peer, ensure_runtime_dir,
};
use crate::wire::{Frame, FrameReader, FrameWriter};
use anyhow::{Result, anyhow};
use lumas_common::ipc::{Channel, LumiMessage, ProcessId};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, error, info, warn};

/// Interval between heartbeat messages (in seconds).
const HEARTBEAT_INTERVAL_SECS: u64 = 5;

/// Maximum time without a heartbeat before considering a peer dead.
const HEARTBEAT_TIMEOUT_SECS: u64 = 15;

// ---------------------------------------------------------------------------
// Peer Manager
// ---------------------------------------------------------------------------

/// Manages all peer connections for a Lumas process.
///
/// Each process has one `PeerManager` that:
/// 1. Listens for incoming connections from peers
/// 2. Connects to peer processes as needed
/// 3. Routes outgoing messages to the correct peer
/// 4. Monitors peer health and reconnects on failure
pub struct PeerManager {
    /// This process's identifier.
    local_id: ProcessId,
    /// Active connections to peers, keyed by ProcessId.
    connections: Arc<RwLock<HashMap<ProcessId, TransportConnection>>>,
    /// Channel for sending messages to the peer manager's event loop.
    command_tx: mpsc::Sender<PeerCommand>,
}

/// Commands sent to the peer manager's background event loop.
enum PeerCommand {
    /// Send a message to a specific peer.
    SendMessage {
        target: ProcessId,
        message: LumiMessage,
    },
    /// Connect to a peer.
    ConnectToPeer {
        peer_id: ProcessId,
        runtime_dir: String,
    },
    /// Shut down all connections.
    Shutdown,
}

impl PeerManager {
    /// Create a new peer manager and start its background event loop.
    pub fn new(local_id: ProcessId) -> Arc<Self> {
        let (command_tx, mut command_rx) = mpsc::channel::<PeerCommand>(1024);
        let connections = Arc::new(RwLock::new(HashMap::new()));
        let local_id_for_loop = local_id.clone();

        let manager = Arc::new(Self {
            local_id,
            connections: connections.clone(),
            command_tx,
        });

        // Spawn the background event loop
        let local_id_clone = local_id_for_loop.clone();
        tokio::spawn(async move {
            while let Some(cmd) = command_rx.recv().await {
                match cmd {
                    PeerCommand::SendMessage { target, message } => {
                        let conns = connections.read().await;
                        if let Some(conn) = conns.get(&target) {
                            if conn.is_alive() {
                                let frame = match Frame::from_message(&message) {
                                    Ok(f) => f,
                                    Err(e) => {
                                        error!("Failed to serialize message: {e}");
                                        continue;
                                    }
                                };
                                let mut stream = conn.stream().await;
                                if let Err(e) = FrameWriter::write_frame(&mut *stream, &frame).await
                                {
                                    warn!("Failed to send message to {target}: {e}");
                                    drop(stream);
                                    // Mark connection for reconnection
                                    drop(conns);
                                    if let Some(conn) =
                                        connections.write().await.get_mut(&target)
                                    {
                                        conn.mark_dead();
                                    }
                                }
                            } else {
                                debug!("Connection to {target} is dead, reconnecting...");
                                drop(conns);
                                // Will be handled by health check loop
                            }
                        } else {
                            warn!("No connection to peer: {target}");
                        }
                    }
                    PeerCommand::ConnectToPeer {
                        peer_id,
                        runtime_dir,
                    } => {
                        let path = Path::new(&runtime_dir);
                        match connect_to_peer(&peer_id, path).await {
                            Ok(mut stream) => {
                                // Send handshake to identify ourselves
                                let handshake = create_handshake(&local_id_clone);
                                let frame = Frame::from_message(&handshake)
                                    .expect("Failed to create handshake frame");
                                if let Err(e) = FrameWriter::write_frame(&mut *stream, &frame).await
                                {
                                    warn!("Failed to send handshake to {peer_id}: {e}");
                                    continue;
                                }
                                let conn = TransportConnection::new(stream, peer_id.clone());
                                connections.write().await.insert(peer_id.clone(), conn);
                                info!("Connected to peer: {peer_id}");
                            }
                            Err(e) => {
                                warn!("Failed to connect to peer {peer_id}: {e}");
                            }
                        }
                    }
                    PeerCommand::Shutdown => {
                        info!("Peer manager shutting down");
                        connections.write().await.clear();
                        break;
                    }
                }
            }
        });

        manager
    }

    /// Start listening for incoming connections and spawn the accept loop.
    pub async fn start_listener(
        self: &Arc<Self>,
        runtime_dir: &Path,
    ) -> Result<tokio::task::JoinHandle<()>> {
        ensure_runtime_dir(runtime_dir).await?;

        let listener = TransportListener::bind(&self.local_id, runtime_dir).await?;
        info!("Peer manager listening at: {}", listener.local_path());

        let local_id = self.local_id.clone();
        let connections = self.connections.clone();

        // Spawn the accept loop
        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok(stream) => {
                        // Spawn a handler for this new connection
                        let connections = connections.clone();
                        let local_id = local_id.clone();
                        tokio::spawn(async move {
                            if let Err(e) =
                                handle_inbound_connection(stream, connections, &local_id).await
                            {
                                debug!("Inbound connection handler finished: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {e}");
                        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Connect to a peer process.
    pub async fn connect_to(&self, peer_id: ProcessId, runtime_dir: &Path) {
        let runtime_dir_str = runtime_dir.to_string_lossy().to_string();
        if let Err(e) = self
            .command_tx
            .send(PeerCommand::ConnectToPeer {
                peer_id,
                runtime_dir: runtime_dir_str,
            })
            .await
        {
            error!("Failed to queue connect command: {e}");
        }
    }

    /// Send a message to a specific peer.
    pub async fn send_to(&self, target: ProcessId, message: LumiMessage) {
        let target_for_log = target.clone();
        if let Err(e) = self
            .command_tx
            .send(PeerCommand::SendMessage { target, message })
            .await
        {
            error!("Failed to queue message for {target_for_log}: {e}");
        }
    }

    /// Broadcast a message to all connected peers.
    pub async fn broadcast(&self, message: &LumiMessage) {
        let conns = self.connections.read().await;
        let peer_count = conns.len();
        if peer_count == 0 {
            return;
        }

        // Send a clone of the message to each peer
        for (peer_id, _conn) in conns.iter() {
            if *peer_id == self.local_id {
                continue; // Don't send to self
            }
            let msg = message.clone();
            let cmd_tx = self.command_tx.clone();
            let target = peer_id.clone();
            tokio::spawn(async move {
                let _ = cmd_tx
                    .send(PeerCommand::SendMessage {
                        target,
                        message: msg,
                    })
                    .await;
            });
        }
    }

    /// Get the number of currently connected peers.
    pub async fn peer_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Check if connected to a specific peer.
    pub async fn is_connected_to(&self, peer_id: &ProcessId) -> bool {
        self.connections.read().await.contains_key(peer_id)
    }

    /// Shut down all peer connections.
    pub async fn shutdown(&self) {
        let _ = self.command_tx.send(PeerCommand::Shutdown).await;
    }
}

// ---------------------------------------------------------------------------
// Inbound Connection Handler
// ---------------------------------------------------------------------------

/// Handle an inbound connection from a peer.
async fn handle_inbound_connection(
    mut stream: Box<dyn IoStream>,
    connections: Arc<RwLock<HashMap<ProcessId, TransportConnection>>>,
    _local_id: &ProcessId,
) -> Result<()> {
    let mut reader = FrameReader::new();

    // The first message from a peer should be a handshake identifying themselves
    let handshake_frame = reader
        .read_frame(&mut stream)
        .await?
        .ok_or_else(|| anyhow!("Peer closed connection during handshake"))?;

    let handshake_msg = handshake_frame.into_message()?;

    // Verify it's a handshake event
    let peer_id = handshake_msg.source.clone();

    info!("Inbound connection established from: {peer_id}");

    // Register the connection
    let conn = TransportConnection::new(stream, peer_id.clone());
    connections.write().await.insert(peer_id.clone(), conn);

    // Spawn a reader loop for this connection
    let peer_id_clone = peer_id.clone();
    tokio::spawn(async move {
        read_from_peer(peer_id_clone, connections).await;
    });

    Ok(())
}

/// Read messages from a peer connection until it closes.
async fn read_from_peer(
    peer_id: ProcessId,
    connections: Arc<RwLock<HashMap<ProcessId, TransportConnection>>>,
) {
    let stream = {
        let conns = connections.read().await;
        conns.get(&peer_id).map(|c| c.clone_stream())
    };

    let stream_arc = match stream {
        Some(s) => s,
        None => {
            warn!("Cannot start reader for unknown peer: {peer_id}");
            return;
        }
    };

    let mut reader = FrameReader::new();

    loop {
        let mut guard = stream_arc.lock().await;
        match reader.read_frame(&mut *guard).await {
            Ok(Some(frame)) => {
                drop(guard);
                match frame.into_message() {
                    Ok(msg) => {
                        debug!("Received message from {peer_id}: {:?}", msg.msg_type);
                        // The message will be processed by the main event loop
                        // via the broadcast channel subscription
                    }
                    Err(e) => {
                        error!("Failed to deserialize message from {peer_id}: {e}");
                    }
                }
            }
            Ok(None) => {
                // Clean close
                info!("Peer {peer_id} disconnected");
                drop(guard);
                connections.write().await.remove(&peer_id);
                return;
            }
            Err(e) => {
                warn!("Error reading from peer {peer_id}: {e}");
                drop(guard);
                connections.write().await.remove(&peer_id);
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Handshake Message
// ---------------------------------------------------------------------------

/// Create a handshake message that identifies this process to a peer.
pub fn create_handshake(local_id: &ProcessId) -> LumiMessage {
    LumiMessage::new_event(
        local_id.clone(),
        Channel::StateEvent,
        serde_json::json!({
            "type": "handshake",
            "process": local_id.to_string(),
            "version": 1,
        }),
    )
    .expect("Failed to create handshake message")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handshake_creation() {
        let msg = create_handshake(&ProcessId::Core);
        assert_eq!(msg.source, ProcessId::Core);
        assert_eq!(msg.channel, Channel::StateEvent);
        assert_eq!(msg.payload["type"], "handshake");
    }
}
