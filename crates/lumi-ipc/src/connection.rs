//! # Connection Manager
//!
//! Manages IPC connections to peer processes, including:
//! - Connection state machine (Disconnected → Connecting → Handshaking → Ready → Disconnecting)
//! - Reconnection with exponential backoff
//! - Heartbeat integration
//! - Connection metrics

use crate::error::{IpcError, IpcResult};
use crate::message::{LumiMessage, ProcessId};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected = 0,
    Connecting = 1,
    Handshaking = 2,
    Ready = 3,
    Disconnecting = 4,
}

impl ConnectionState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => ConnectionState::Disconnected,
            1 => ConnectionState::Connecting,
            2 => ConnectionState::Handshaking,
            3 => ConnectionState::Ready,
            4 => ConnectionState::Disconnecting,
            _ => ConnectionState::Disconnected,
        }
    }

    pub fn to_u8(&self) -> u8 {
        *self as u8
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, ConnectionState::Ready)
    }
}

/// Connection trait for abstracting over different connection types.
pub trait Connection: Send + Sync + 'static {
    fn id(&self) -> u64;
    fn peer(&self) -> &ProcessId;
    fn state(&self) -> ConnectionState;
    fn established_at(&self) -> Option<Instant>;
    fn send(&self, msg: LumiMessage) -> impl std::future::Future<Output = Result<(), IpcError>> + Send;
    fn close(&self) -> impl std::future::Future<Output = Result<(), IpcError>> + Send;
}

/// Connection identifier type.
pub type ConnectionId = u64;

/// Reconnection policy with exponential backoff.
#[derive(Debug, Clone)]
pub struct ReconnectPolicy {
    /// Maximum reconnection attempts
    pub max_attempts: u32,
    /// Initial delay before first reconnect
    pub initial_delay: Duration,
    /// Maximum delay between attempts
    pub max_delay: Duration,
    /// Exponential backoff multiplier
    pub backoff_factor: f64,
    /// Jitter percentage (0-100) to prevent thundering herd
    pub jitter_percent: u8,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 10,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_factor: 2.0,
            jitter_percent: 20,
        }
    }
}

impl ReconnectPolicy {
    /// Calculate the delay for a given reconnection attempt.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay = self.initial_delay.as_secs_f64()
            * self.backoff_factor.powi(attempt as i32);

        let delay = delay.min(self.max_delay.as_secs_f64());

        // Add jitter
        let jitter = if self.jitter_percent > 0 {
            let jitter_range = delay * (self.jitter_percent as f64 / 100.0);
            let jitter = jitter_range * (rand::random::<f64>() - 0.5);
            jitter
        } else {
            0.0
        };

        Duration::from_secs_f64((delay + jitter).max(0.001))
    }
}

/// Connection manager.
pub struct ConnectionManager {
    /// Active connections, keyed by connection ID
    connections: dashmap::DashMap<ConnectionId, Arc<dyn Connection>>,
    /// Index by peer ID
    peer_index: dashmap::DashMap<ProcessId, ConnectionId>,
    /// Reconnection policy
    reconnect_policy: ReconnectPolicy,
    /// Global closed flag
    closed: AtomicBool,
    /// Next connection ID
    next_id: AtomicU64,
}

impl ConnectionManager {
    /// Create a new connection manager.
    pub fn new(reconnect_policy: ReconnectPolicy) -> Self {
        Self {
            connections: dashmap::DashMap::new(),
            peer_index: dashmap::DashMap::new(),
            reconnect_policy,
            closed: AtomicBool::new(false),
            next_id: AtomicU64::new(1),
        }
    }

    /// Register a connection.
    pub fn register(&self, conn: Arc<dyn Connection>) {
        let id = conn.id();
        let peer = conn.peer().clone();

        self.connections.insert(id, conn);
        self.peer_index.insert(peer, id);
    }

    /// Remove a connection.
    pub fn remove(&self, id: ConnectionId) -> Option<Arc<dyn Connection>> {
        if let Some((_, conn)) = self.connections.remove(&id) {
            self.peer_index.remove(conn.peer());
            Some(conn)
        } else {
            None
        }
    }

    /// Find a connection by peer ID.
    pub fn find_by_peer(&self, peer: &ProcessId) -> Option<Arc<dyn Connection>> {
        self.peer_index
            .get(peer)
            .and_then(|entry| self.connections.get(entry.value()))
            .map(|e| e.clone())
    }

    /// Get a connection by ID.
    pub fn get(&self, id: ConnectionId) -> Option<Arc<dyn Connection>> {
        self.connections.get(&id).map(|e| e.clone())
    }

    /// Get the number of active connections.
    pub fn len(&self) -> usize {
        self.connections.len()
    }

    /// Check if the connection manager is empty.
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Get all active connections.
    pub fn all_connections(&self) -> Vec<Arc<dyn Connection>> {
        self.connections
            .iter()
            .map(|e| e.clone())
            .collect()
    }

    /// Close all connections.
    pub async fn close_all(&self) {
        for conn in self.all_connections() {
            let _ = conn.close().await;
        }
        self.closed.store(true, Ordering::Relaxed);
    }

    /// Get the next connection ID.
    pub fn next_id(&self) -> ConnectionId {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Get the reconnect policy.
    pub fn reconnect_policy(&self) -> &ReconnectPolicy {
        &self.reconnect_policy
    }

    /// Check if the manager is closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }
}
