//! # Transport Layer — Three-Tier Transport Abstraction
//!
//! Defines the unified `Transport` trait that all three transport tiers implement.
//! The trait provides a common interface for sending and receiving messages
//! regardless of the underlying transport mechanism.

pub mod inprocess;
#[cfg(any(feature = "unix-socket", feature = "named-pipe", platform_unix_socket, platform_named_pipe))]
pub mod socket;
#[cfg(feature = "shared-memory")]
pub mod shm;

use crate::error::TransportError;
use crate::message::LumiMessage;
use async_trait::async_trait;
use std::fmt;
use std::sync::Arc;

/// Transport tier classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportTier {
    /// Tier 1: Shared Memory Ring Buffer (sub-100µs)
    SharedMemory,
    /// Tier 2: Unix Domain Socket / Windows Named Pipe (< 1ms)
    Socket,
    /// Tier 3: In-Process Channel (nanoseconds)
    InProcess,
}

impl TransportTier {
    pub fn name(&self) -> &'static str {
        match self {
            TransportTier::SharedMemory => "shared-memory",
            TransportTier::Socket => "socket",
            TransportTier::InProcess => "in-process",
        }
    }
}

/// Transport metrics snapshot.
#[derive(Debug, Clone, Default)]
pub struct TransportMetrics {
    /// Total messages sent.
    pub messages_sent: u64,
    /// Total messages received.
    pub messages_received: u64,
    /// Total bytes sent.
    pub bytes_sent: u64,
    /// Total bytes received.
    pub bytes_received: u64,
    /// Current send queue depth.
    pub send_queue_depth: u32,
    /// Total send errors.
    pub send_errors: u64,
    /// Total recv errors.
    pub recv_errors: u64,
}

/// The unified transport interface.
///
/// All three transport tiers implement this trait, providing a common
/// interface for the bus layer. The appropriate transport is selected
/// per-channel based on the channel's declared properties.
#[async_trait]
pub trait Transport: Send + Sync + fmt::Debug + 'static {
    /// The transport tier.
    fn tier(&self) -> TransportTier;

    /// Human-readable transport name for diagnostics.
    fn name(&self) -> &'static str;

    /// The channel this transport is bound to.
    fn channel(&self) -> &str;

    /// Send a message through this transport.
    async fn send(&self, msg: LumiMessage) -> Result<(), TransportError>;

    /// Receive the next message, blocking until one arrives.
    async fn recv(&self) -> Result<LumiMessage, TransportError>;

    /// Non-blocking receive. Returns `Ok(None)` if no message is available.
    fn try_recv(&self) -> Result<Option<LumiMessage>, TransportError>;

    /// Close the transport and release resources.
    async fn close(&self) -> Result<(), TransportError>;

    /// Get transport metrics.
    fn metrics(&self) -> TransportMetrics;
}

/// Convenience type alias for an Arc-wrapped transport.
pub type SharedTransport = Arc<dyn Transport>;

/// Create a transport factory function that selects the appropriate transport
/// based on the tier.
pub fn create_transport(
    tier: TransportTier,
    channel: &str,
    config: &crate::config::ChannelConfig,
) -> Result<SharedTransport, TransportError> {
    match tier {
        TransportTier::InProcess => {
            Ok(Arc::new(inprocess::InProcessTransport::new(
                channel,
                config.max_payload_bytes,
            )))
        }
        TransportTier::Socket => {
            #[cfg(any(feature = "unix-socket", feature = "named-pipe"))]
            {
                Ok(Arc::new(socket::SocketTransport::new(
                    channel,
                    config.max_payload_bytes,
                )))
            }
            #[cfg(not(any(feature = "unix-socket", feature = "named-pipe")))]
            {
                Err(TransportError::UnsupportedPlatform)
            }
        }
        TransportTier::SharedMemory => {
            #[cfg(feature = "shared-memory")]
            {
                Ok(Arc::new(shm::SharedMemoryTransport::new(
                    channel,
                    config.shm_slots,
                    config.shm_slot_size_bytes,
                )))
            }
            #[cfg(not(feature = "shared-memory"))]
            {
                Err(TransportError::UnsupportedPlatform)
            }
        }
    }
}
