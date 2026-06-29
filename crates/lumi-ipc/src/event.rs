//! # IPC Bus Events
//!
//! Events emitted by the IPC bus for monitoring, diagnostics, and
//! integration with `lumi-state`.

use crate::connection::ConnectionId;
use crate::message::{MessageId, ProcessId};
use std::time::Instant;

/// Peer health status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerHealthStatus {
    /// Peer is responsive and healthy.
    Healthy,
    /// Peer is responsive but has high latency.
    Degraded { latency_us: u64 },
    /// Peer is not responding but may recover.
    Unresponsive { since: Instant },
    /// Peer is considered dead.
    Dead,
}

/// Events emitted by the IPC bus.
#[derive(Debug, Clone)]
pub enum BusEvent {
    /// A peer has connected.
    PeerConnected {
        peer: ProcessId,
        connection_id: ConnectionId,
    },
    /// A peer has disconnected.
    PeerDisconnected {
        peer: ProcessId,
        reason: DisconnectReason,
    },
    /// A peer is considered dead.
    PeerDead {
        peer: ProcessId,
        last_seen: Instant,
    },
    /// A peer's latency has degraded.
    PeerDegraded {
        peer: ProcessId,
        latency_us: u64,
    },
    /// A peer has recovered from degraded state.
    PeerRecovered {
        peer: ProcessId,
    },
    /// A message was sent on a channel.
    MessageSent {
        channel: String,
        msg_id: MessageId,
        size_bytes: u32,
    },
    /// A message was received on a channel.
    MessageReceived {
        channel: String,
        msg_id: MessageId,
        latency_us: u64,
    },
    /// A message was rejected by the validator or auth.
    MessageRejected {
        channel: String,
        msg_id: MessageId,
        reason: RejectionReason,
    },
    /// A message was dropped due to backpressure.
    MessageDropped {
        channel: String,
        msg_id: MessageId,
        reason: DropReason,
    },
    /// An authentication failure occurred.
    AuthFailure {
        sender: ProcessId,
        reason: String,
    },
    /// A replay attack was detected.
    ReplayDetected {
        sender: ProcessId,
        sequence: u64,
    },
    /// A stream was opened.
    StreamOpened {
        stream_id: u64,
        channel: String,
    },
    /// A stream was closed.
    StreamClosed {
        stream_id: u64,
        reason: StreamCloseReason,
    },
    /// A sender exceeded the rate limit.
    RateLimitExceeded {
        sender: ProcessId,
        channel: String,
        rate: f64,
    },
    /// A service was registered.
    ServiceRegistered {
        service_id: String,
    },
    /// A service was deregistered.
    ServiceDeregistered {
        service_id: String,
    },
    /// A service's health changed.
    ServiceHealthChanged {
        service_id: String,
        new_health: PeerHealthStatus,
    },
    /// A handshake failed.
    HandshakeFailed {
        peer: ProcessId,
        reason: String,
    },
}

/// Reason for disconnection.
#[derive(Debug, Clone)]
pub enum DisconnectReason {
    Shutdown,
    Upgrade,
    ProtocolError(String),
    AuthFailure(String),
    HeartbeatTimeout,
    Unknown,
}

/// Reason for message rejection.
#[derive(Debug, Clone)]
pub enum RejectionReason {
    AuthFailed(String),
    ValidationFailed(String),
    PermissionDenied(String),
    UnknownVersion(u16),
    Expired,
}

/// Reason for message drop.
#[derive(Debug, Clone)]
pub enum DropReason {
    QueueFull,
    Backpressure,
    TTLExpired,
    StreamClosed,
}

/// Reason for stream closure.
#[derive(Debug, Clone)]
pub enum StreamCloseReason {
    Completed,
    Cancelled,
    Error(String),
    Timeout,
}
