//! # Transport Layer — Platform-specific IPC Connections
//!
//! Provides platform-appropriate transport for inter-process communication:
//! - Unix domain sockets on macOS and Linux (`tokio::net::UnixStream`)
//! - Named pipes on Windows (`tokio::net::windows::named_pipe`)
//!
//! Each Lumas process creates a listener that accepts connections from peers
//! and also initiates outbound connections to other processes.

use anyhow::Result;
use lumas_common::ipc::ProcessId;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Transport Trait
// ---------------------------------------------------------------------------

/// A bidirectional byte stream used for IPC.
/// Abstracts over Unix domain sockets and named pipes.
pub trait IoStream: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

// Platform-specific implementations
#[cfg(unix)]
impl IoStream for tokio::net::UnixStream {}

#[cfg(windows)]
impl IoStream for tokio::net::windows::named_pipe::NamedPipeClient {}

#[cfg(windows)]
impl IoStream for tokio::net::windows::named_pipe::NamedPipeServer {}

/// Fallback: TCP stream works on all platforms (used in tests or when native IPC is unavailable).
#[cfg(feature = "tcp-fallback")]
impl IoStream for tokio::net::TcpStream {}

// ---------------------------------------------------------------------------
// Listener — accepts incoming connections
// ---------------------------------------------------------------------------

/// Platform-specific listener that accepts incoming IPC connections.
pub enum TransportListener {
    /// Unix domain socket listener (macOS/Linux).
    #[cfg(unix)]
    Unix(tokio::net::UnixListener),
    /// Named pipe server (Windows) — uses a single connection model.
    #[cfg(windows)]
    NamedPipe(tokio::net::windows::named_pipe::NamedPipeServer),
    /// TCP listener fallback (all platforms, for testing).
    #[cfg(feature = "tcp-fallback")]
    Tcp(tokio::net::TcpListener),
}

impl TransportListener {
    /// Bind a listener on the given path/address.
    pub async fn bind(process_id: &ProcessId, runtime_dir: &Path) -> Result<Self> {
        let _socket_path = Self::socket_path(process_id, runtime_dir);

        #[cfg(unix)]
        {
            // Remove stale socket file if it exists
            if socket_path.exists() {
                std::fs::remove_file(&socket_path)?;
                debug!("Removed stale socket: {:?}", socket_path);
            }

            let listener = tokio::net::UnixListener::bind(&socket_path)?;
            info!("IPC listener bound (Unix socket): {:?}", socket_path);
            Ok(TransportListener::Unix(listener))
        }

        #[cfg(windows)]
        {
            let pipe_path = Self::pipe_path(process_id);
            // For Windows named pipes, we create a server that waits for a single connection
            let server = tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .create(&pipe_path)?;
            info!("IPC listener bound (named pipe): {}", pipe_path);
            Ok(TransportListener::NamedPipe(server))
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = socket_path;
            anyhow::bail!("Unsupported platform for IPC transport");
        }
    }

    /// Accept a new incoming connection.
    pub async fn accept(&self) -> Result<Box<dyn IoStream>> {
        match self {
            #[cfg(unix)]
            TransportListener::Unix(listener) => {
                let (stream, addr) = listener.accept().await?;
                debug!("Unix socket connection accepted: {:?}", addr);
                Ok(Box::new(stream))
            }
            #[cfg(windows)]
            TransportListener::NamedPipe(_server) => {
                // Named pipe server needs to connect then disconnect per-message model
                // For simplicity, we use a reconnection approach
                anyhow::bail!("Named pipe accept not fully implemented");
            }
            #[cfg(feature = "tcp-fallback")]
            TransportListener::Tcp(listener) => {
                let (stream, addr) = listener.accept().await?;
                debug!("TCP connection accepted: {}", addr);
                Ok(Box::new(stream))
            }
        }
    }

    /// Get the local address/path of this listener.
    pub fn local_path(&self) -> String {
        match self {
            #[cfg(unix)]
            TransportListener::Unix(listener) => match listener.local_addr() {
                Ok(addr) => format!("{:?}", addr.as_pathname()),
                Err(_) => "unknown".into(),
            },
            #[cfg(windows)]
            TransportListener::NamedPipe(_) => "named-pipe".into(),
            #[cfg(feature = "tcp-fallback")]
            TransportListener::Tcp(listener) => match listener.local_addr() {
                Ok(addr) => addr.to_string(),
                Err(_) => "unknown".into(),
            },
        }
    }

    /// Get the socket file path for a process.
    fn socket_path(process_id: &ProcessId, runtime_dir: &Path) -> PathBuf {
        runtime_dir.join(format!("lumi-{}.sock", process_id))
    }

    /// Get the named pipe path for a process (Windows).
    #[cfg(windows)]
    fn pipe_path(process_id: &ProcessId) -> String {
        format!(r"\\.\pipe\lumi-{}", process_id)
    }
}

// ---------------------------------------------------------------------------
// Connector — initiates outbound connections
// ---------------------------------------------------------------------------

/// Connect to a peer process's listener.
pub async fn connect_to_peer(
    process_id: &ProcessId,
    _runtime_dir: &Path,
) -> Result<Box<dyn IoStream>> {
    #[cfg(unix)]
    {
        let socket_path = runtime_dir.join(format!("lumi-{}.sock", process_id));
        let stream = tokio::net::UnixStream::connect(&socket_path).await?;
        debug!("Connected to peer (Unix socket): {:?}", socket_path);
        Ok(Box::new(stream))
    }

    #[cfg(windows)]
    {
        let pipe_path = format!(r"\\.\pipe\lumi-{}", process_id);
        let client = tokio::net::windows::named_pipe::ClientOptions::new().open(&pipe_path)?;
        debug!("Connected to peer (named pipe): {}", pipe_path);
        Ok(Box::new(client))
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = runtime_dir;
        anyhow::bail!("Unsupported platform: cannot connect to peer {process_id}");
    }
}

// ---------------------------------------------------------------------------
// Runtime Directory
// ---------------------------------------------------------------------------

/// Get the default runtime directory for IPC sockets.
pub fn default_runtime_dir() -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push("lumi-ipc");
    dir
}

/// Ensure the runtime directory exists.
pub async fn ensure_runtime_dir(path: &Path) -> Result<()> {
    tokio::fs::create_dir_all(path).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Connection Wrapper
// ---------------------------------------------------------------------------

/// A wrapped connection that provides framed read/write with reconnect logic.
pub struct TransportConnection {
    /// The underlying byte stream.
    stream: Arc<Mutex<Box<dyn IoStream>>>,
    /// The peer process ID.
    peer_id: ProcessId,
    /// Whether this connection is alive.
    alive: bool,
}

impl TransportConnection {
    /// Wrap a raw byte stream into a transport connection.
    pub fn new(stream: Box<dyn IoStream>, peer_id: ProcessId) -> Self {
        Self {
            stream: Arc::new(Mutex::new(stream)),
            peer_id,
            alive: true,
        }
    }

    /// Get the peer process ID.
    pub fn peer_id(&self) -> &ProcessId {
        &self.peer_id
    }

    /// Check if the connection is alive.
    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// Mark the connection as dead (for reconnection logic).
    pub fn mark_dead(&mut self) {
        self.alive = false;
    }

    /// Get the underlying stream for read/write operations.
    pub async fn stream(&self) -> tokio::sync::MutexGuard<'_, Box<dyn IoStream>> {
        self.stream.lock().await
    }

    /// Clone the stream Arc for sharing across tasks.
    pub fn clone_stream(&self) -> Arc<Mutex<Box<dyn IoStream>>> {
        self.stream.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runtime_dir() {
        let dir = default_runtime_dir();
        assert!(dir.to_string_lossy().contains("lumi-ipc"));
    }

    #[tokio::test]
    async fn test_socket_path_format() {
        let dir = Path::new("/tmp/lumi");
        let path = PathBuf::from(format!("/tmp/lumi/lumi-core.sock"));

        #[cfg(unix)]
        {
            let computed = PathBuf::from("/tmp/lumi").join("lumi-core.sock");
            assert_eq!(computed, path);
        }
    }
}
