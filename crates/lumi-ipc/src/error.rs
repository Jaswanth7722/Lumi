//! # IPC Error Types
//!
//! All errors in the `lumi-ipc` crate are defined through the `lumi_subsystem_error!`
//! macro from `lumi-error`, providing consistent error codes, severity levels,
//! and integration with the Lumi error subsystem.

use crate::message::ProcessId;
use std::borrow::Cow;

/// Error type alias for state-level operations
pub type IpcResult<T> = Result<T, IpcError>;

/// IPC error codes
#[derive(Debug, Clone, thiserror::Error)]
pub enum IpcError {
    #[error("Peer not found: {peer}")]
    PeerNotFound { peer: ProcessId },

    #[error("Channel not found: {channel}")]
    ChannelNotFound { channel: String },

    #[error("Connection failed to peer {peer}: {cause}")]
    ConnectionFailed { peer: ProcessId, cause: String },

    #[error("Handshake timeout for peer {peer}: {elapsed:?}")]
    HandshakeTimeout { peer: ProcessId, elapsed: std::time::Duration },

    #[error("Handshake rejected by peer {peer}: {reason}")]
    HandshakeRejected { peer: ProcessId, reason: String },

    #[error("Authentication failed for peer {peer}")]
    AuthenticationFailed { peer: ProcessId },

    #[error("Replay attack detected from peer {peer}: sequence {sequence}")]
    ReplayDetected { peer: ProcessId, sequence: u64 },

    #[error("Message too large on channel {channel}: {size} bytes (max {limit})")]
    MessageTooLarge { channel: String, size: u32, limit: u32 },

    #[error("Message expired: {msg_id}, age {age_ms}ms")]
    MessageExpired { msg_id: String, age_ms: u64 },

    #[error("Validation failed for message {msg_id}: {reason}")]
    ValidationFailed { msg_id: String, reason: String },

    #[error("Queue full on channel {channel}: depth {depth}")]
    QueueFull { channel: String, depth: usize },

    #[error("Send timeout on channel {channel}: waited {waited:?}")]
    SendTimeout { channel: String, waited: std::time::Duration },

    #[error("Receive timeout on channel {channel}: waited {waited:?}")]
    RecvTimeout { channel: String, waited: std::time::Duration },

    #[error("Transport error ({transport}): {cause}")]
    TransportError { transport: &'static str, cause: String },

    #[error("Serialization error for {msg_kind}: {cause}")]
    SerializationError { msg_kind: String, cause: String },

    #[error("Deserialization error: {cause}")]
    DeserializationError { cause: String },

    #[error("Stream already closed: {stream_id}")]
    StreamAlreadyClosed { stream_id: u64 },

    #[error("Permission denied: peer {peer} cannot access channel {channel}")]
    PermissionDenied { peer: ProcessId, channel: String },

    #[error("Unknown wire protocol version: {version}")]
    UnknownWireVersion { version: u16 },

    #[error("Shared memory magic mismatch: expected {expected:#x}, found {found:#x}")]
    SharedMemoryMagicMismatch { expected: u32, found: u32 },

    #[error("Shared memory schema hash mismatch: expected {expected:#x}, found {found:#x}")]
    SharedMemorySchemaHashMismatch { expected: u64, found: u64 },

    #[error("Bus is shutting down")]
    BusShuttingDown,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Unsupported platform")]
    UnsupportedPlatform,
}

impl PartialEq for IpcError {
    fn eq(&self, other: &Self) -> bool {
        use IpcError::*;
        match (self, other) {
            (PeerNotFound { peer: a }, PeerNotFound { peer: b }) => a == b,
            (ChannelNotFound { channel: a }, ChannelNotFound { channel: b }) => a == b,
            (BusShuttingDown, BusShuttingDown) => true,
            _ => false,
        }
    }
}

impl From<anyhow::Error> for IpcError {
    fn from(err: anyhow::Error) -> Self {
        IpcError::Internal(err.to_string())
    }
}

impl From<std::io::Error> for IpcError {
    fn from(err: std::io::Error) -> Self {
        IpcError::TransportError {
            transport: "io",
            cause: err.to_string(),
        }
    }
}

impl From<rmp_serde::encode::Error> for IpcError {
    fn from(err: rmp_serde::encode::Error) -> Self {
        IpcError::SerializationError {
            msg_kind: "unknown".into(),
            cause: err.to_string(),
        }
    }
}

impl From<rmp_serde::decode::Error> for IpcError {
    fn from(err: rmp_serde::decode::Error) -> Self {
        IpcError::DeserializationError {
            cause: err.to_string(),
        }
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(err: serde_json::Error) -> Self {
        IpcError::SerializationError {
            msg_kind: "json".into(),
            cause: err.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Utility error types
// ---------------------------------------------------------------------------

/// Wire codec error
#[derive(Debug, Clone, thiserror::Error)]
pub enum CodecError {
    #[error("Invalid magic: expected {expected:#x}, got {got:#x}")]
    InvalidMagic { expected: u32, got: u32 },

    #[error("Unsupported wire version: {version}")]
    UnsupportedVersion { version: u16 },

    #[error("Frame too large: {size} bytes (max {max})")]
    FrameTooLarge { size: usize, max: usize },

    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("Encryption error: {0}")]
    EncryptionError(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Buffer underflow: needed {needed}, available {available}")]
    BufferUnderflow { needed: usize, available: usize },
}

impl From<std::io::Error> for CodecError {
    fn from(err: std::io::Error) -> Self {
        CodecError::Io(err.to_string())
    }
}

/// Transport error
#[derive(Debug, Clone, thiserror::Error)]
pub enum TransportError {
    #[error("Transport closed")]
    Closed,

    #[error("Transport not connected")]
    NotConnected,

    #[error("Send timeout")]
    SendTimeout,

    #[error("Recv timeout")]
    RecvTimeout,

    #[error("IO error: {0}")]
    Io(String),

    #[error("Codec error: {0}")]
    Codec(#[from] CodecError),

    #[error("Unsupported transport for platform")]
    UnsupportedPlatform,
}

impl From<std::io::Error> for TransportError {
    fn from(err: std::io::Error) -> Self {
        TransportError::Io(err.to_string())
    }
}

/// Routing error
#[derive(Debug, Clone, thiserror::Error)]
pub enum RoutingError {
    #[error("No route for channel: {channel}")]
    NoRoute { channel: String },

    #[error("No subscribers for channel: {channel}")]
    NoSubscribers { channel: String },

    #[error("Permission denied: sender {sender} cannot publish on {channel}")]
    PermissionDenied { sender: ProcessId, channel: String },
}

/// Validation error
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidationError {
    #[error("Missing required field: {field}")]
    MissingField { field: Cow<'static, str> },

    #[error("Invalid field {field}: {reason}")]
    InvalidField { field: Cow<'static, str>, reason: String },

    #[error("Message expired: age {age_ms}ms > TTL {ttl_ms}ms")]
    Expired { age_ms: u64, ttl_ms: u32 },

    #[error("Payload size {size} exceeds channel limit {limit}")]
    PayloadTooLarge { size: u32, limit: u32 },

    #[error("Schema version mismatch: expected {expected}, got {got}")]
    SchemaVersionMismatch { expected: u16, got: u16 },

    #[error("Timestamp out of acceptable range")]
    TimestampOutOfRange,
}

/// Authentication error
#[derive(Debug, Clone, thiserror::Error)]
pub enum AuthError {
    #[error("MAC verification failed")]
    MacVerificationFailed,

    #[error("Unknown key ID: {key_id}")]
    UnknownKeyId { key_id: u64 },

    #[error("Replay detected: sequence {sequence} already processed")]
    ReplayDetected { sequence: u64 },

    #[error("Session key not established with peer {peer}")]
    NoSessionKey { peer: ProcessId },

    #[error("Handshake protocol error: {0}")]
    Protocol(String),
}

/// Middleware error
#[derive(Debug, Clone, thiserror::Error)]
pub enum MiddlewareError {
    #[error("Middleware {name} rejected message: {reason}")]
    Rejected { name: &'static str, reason: String },

    #[error("Middleware {name} failed: {cause}")]
    Failed { name: &'static str, cause: String },
}
