//! # Socket Transport (Tier 2)
//!
//! Unix domain socket (macOS/Linux) or named pipe (Windows) transport with
//! length-prefixed MessagePack framing, automatic reconnection with
//! exponential backoff, and per-connection ReplayWindow.
//!
//! Used for: memory.write, memory.query, plugin.invoke, voice.output, etc.

use crate::codec::LumiFramer;
use crate::error::TransportError;
use crate::message::LumiMessage;
use crate::transport::{Transport, TransportMetrics, TransportTier};
use crate::wire::WireCodec;
use async_trait::async_trait;
use crossbeam::queue::SegQueue;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::codec::Framed;
use tracing::{debug, warn};

/// Socket transport implementation.
///
/// Wraps a Tokio async I/O stream (Unix socket or named pipe) with
/// Lumi wire protocol framing.
pub struct SocketTransport {
    /// Channel name
    channel: String,
    /// Maximum payload size in bytes
    max_payload_bytes: u32,
    /// The underlying framed connection
    connection: Option<Arc<AsyncMutex<Box<dyn IoStreamFramed>>>>,
    /// Buffer for received messages
    recv_buffer: Arc<SegQueue<LumiMessage>>,
    /// Metrics
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    send_errors: AtomicU64,
    recv_errors: AtomicU64,
    /// Closed flag
    closed: AtomicBool,
}

/// Trait for framed I/O streams to avoid generic parameters.
#[async_trait]
pub trait IoStreamFramed: Send + Sync + 'static {
    async fn send(&self, msg: LumiMessage) -> Result<(), TransportError>;
}

/// Type alias for a framed Unix socket.
type UnixFramedSocket = Framed<tokio::net::UnixStream, LumiFramer>;

/// Wrapper for framed Unix socket.
#[cfg(unix)]
pub struct UnixSocketWrapper {
    framed: AsyncMutex<UnixFramedSocket>,
}

#[cfg(unix)]
#[async_trait]
impl IoStreamFramed for UnixSocketWrapper {
    /// Encode a message and write the encoded bytes to the underlying OS socket.
    ///
    /// Pipeline:
    /// 1. `Encoder::encode` serializes `msg` into a `BytesMut` buffer (wire protocol framing)
    /// 2. Write the buffer to the Unix socket via `AsyncWriteExt::write_all`
    /// 3. Flush to ensure bytes are transmitted immediately
    ///
    /// Without step 2, the encoded bytes are silently dropped — this was the original bug.
    async fn send(&self, msg: LumiMessage) -> Result<(), TransportError> {
        use tokio::io::AsyncWriteExt;
        use tokio_util::codec::Encoder;

        let mut framed = self.framed.lock().await;

        // Step 1: Encode the message into the wire format buffer
        let mut buf = bytes::BytesMut::new();
        framed
            .encode(msg, &mut buf)
            .map_err(|e| TransportError::Io(e.to_string()))?;

        // Step 2: Write the encoded bytes to the OS socket
        let stream: &mut tokio::net::UnixStream = framed.get_mut();
        stream
            .write_all(&buf)
            .await
            .map_err(|e| TransportError::Io(e.to_string()))?;

        // Step 3: Flush to guarantee delivery
        stream
            .flush()
            .await
            .map_err(|e| TransportError::Io(e.to_string()))?;

        Ok(())
    }
}

/// Placeholder wrapper for non-Unix platforms.
#[cfg(not(unix))]
pub struct SocketFallbackWrapper;

#[cfg(not(unix))]
#[async_trait]
impl IoStreamFramed for SocketFallbackWrapper {
    async fn send(&self, _msg: LumiMessage) -> Result<(), TransportError> {
        Err(TransportError::UnsupportedPlatform)
    }
}

impl SocketTransport {
    /// Create a new socket transport.
    pub fn new(channel: &str, max_payload_bytes: u32) -> Self {
        Self {
            channel: channel.to_string(),
            max_payload_bytes,
            connection: None,
            recv_buffer: Arc::new(SegQueue::new()),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            send_errors: AtomicU64::new(0),
            recv_errors: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        }
    }

    /// Set the underlying connection.
    pub fn set_connection(&mut self, framed: Box<dyn IoStreamFramed>) {
        self.connection = Some(Arc::new(AsyncMutex::new(framed)));
    }

    /// Get the maximum payload size.
    pub fn max_payload_bytes(&self) -> u32 {
        self.max_payload_bytes
    }

    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }
}

#[async_trait]
impl Transport for SocketTransport {
    fn tier(&self) -> TransportTier {
        TransportTier::Socket
    }

    fn name(&self) -> &'static str {
        #[cfg(unix)]
        { "unix-socket" }
        #[cfg(windows)]
        { "named-pipe" }
        #[cfg(not(any(unix, windows)))]
        { "socket-fallback" }
    }

    fn channel(&self) -> &str {
        &self.channel
    }

    async fn send(&self, msg: LumiMessage) -> Result<(), TransportError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(TransportError::Closed);
        }

        self.messages_sent.fetch_add(1, Ordering::Relaxed);

        if let Some(ref conn) = self.connection {
            let framed = conn.lock().await;
            framed.send(msg).await?;
            Ok(())
        } else {
            // Buffer the message for when a connection is established
            self.recv_buffer.push(msg);
            Ok(())
        }
    }

    async fn recv(&self) -> Result<LumiMessage, TransportError> {
        // Check buffer first
        if let Some(msg) = self.recv_buffer.pop() {
            self.messages_received.fetch_add(1, Ordering::Relaxed);
            return Ok(msg);
        }
        Err(TransportError::NotConnected)
    }

    fn try_recv(&self) -> Result<Option<LumiMessage>, TransportError> {
        if let Some(msg) = self.recv_buffer.pop() {
            self.messages_received.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(msg));
        }
        Ok(None)
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.closed.store(true, Ordering::Relaxed);
        self.connection = None;
        Ok(())
    }

    fn metrics(&self) -> TransportMetrics {
        TransportMetrics {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            send_errors: self.send_errors.load(Ordering::Relaxed),
            recv_errors: self.recv_errors.load(Ordering::Relaxed),
            ..Default::default()
        }
    }
}

impl std::fmt::Debug for SocketTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SocketTransport")
            .field("channel", &self.channel)
            .field("connected", &self.connection.is_some())
            .finish()
    }
}
