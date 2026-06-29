//! # IPC Configuration
//!
//! Configuration for the IPC framework, including channel definitions,
//! transport selection, heartbeat settings, reconnection policy,
//! authentication settings, and rate limits.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Top-level IPC configuration.
#[derive(Debug, Clone)]
pub struct IpcConfig {
    /// Process ID (set per-process at launch, not from shared config)
    pub process_id: Option<String>,
    /// Bus configuration
    pub bus: BusConfig,
    /// Per-channel transport and policy configuration
    pub channels: HashMap<String, ChannelConfig>,
    /// Heartbeat settings
    pub heartbeat: HeartbeatConfig,
    /// Reconnection policy
    pub reconnect: ReconnectConfig,
    /// Authentication settings
    pub auth: AuthConfig,
    /// Rate limiting defaults
    pub rate_limits: RateLimitConfig,
    /// Runtime directory for IPC sockets
    pub runtime_dir: PathBuf,
}

impl Default for IpcConfig {
    fn default() -> Self {
        let mut channels = HashMap::new();

        // Tier 1 — Shared Memory
        channels.insert("render.command".into(), ChannelConfig {
            transport: TransportKind::SharedMemory,
            shm_slots: 64,
            shm_slot_size_bytes: 4096,
            backpressure: BackpressurePolicy::DropOldest,
            priority: MessagePriority::Critical,
            max_payload_bytes: 4096,
            block_timeout_ms: None,
            flow_credit_window: None,
        });
        channels.insert("ai.state".into(), ChannelConfig {
            transport: TransportKind::SharedMemory,
            shm_slots: 32,
            shm_slot_size_bytes: 1024,
            backpressure: BackpressurePolicy::DropOldest,
            priority: MessagePriority::High,
            max_payload_bytes: 1024,
            block_timeout_ms: None,
            flow_credit_window: None,
        });

        // Tier 2 — Socket
        channels.insert("render.input".into(), ChannelConfig {
            transport: TransportKind::Socket,
            shm_slots: 0,
            shm_slot_size_bytes: 0,
            backpressure: BackpressurePolicy::Block { timeout_ms: 1000 },
            priority: MessagePriority::Normal,
            max_payload_bytes: 512,
            block_timeout_ms: Some(1000),
            flow_credit_window: None,
        });
        channels.insert("voice.input".into(), ChannelConfig {
            transport: TransportKind::Socket,
            shm_slots: 0,
            shm_slot_size_bytes: 0,
            backpressure: BackpressurePolicy::FlowControl { credit_window: 8 },
            priority: MessagePriority::High,
            max_payload_bytes: 32768,
            block_timeout_ms: None,
            flow_credit_window: Some(8),
        });
        channels.insert("voice.output".into(), ChannelConfig {
            transport: TransportKind::Socket,
            shm_slots: 0,
            shm_slot_size_bytes: 0,
            backpressure: BackpressurePolicy::Block { timeout_ms: 5000 },
            priority: MessagePriority::Normal,
            max_payload_bytes: 65536,
            block_timeout_ms: Some(5000),
            flow_credit_window: None,
        });
        channels.insert("memory.write".into(), ChannelConfig {
            transport: TransportKind::Socket,
            shm_slots: 0,
            shm_slot_size_bytes: 0,
            backpressure: BackpressurePolicy::Block { timeout_ms: 5000 },
            priority: MessagePriority::Normal,
            max_payload_bytes: 8192,
            block_timeout_ms: Some(5000),
            flow_credit_window: None,
        });
        channels.insert("memory.query".into(), ChannelConfig {
            transport: TransportKind::Socket,
            shm_slots: 0,
            shm_slot_size_bytes: 0,
            backpressure: BackpressurePolicy::Block { timeout_ms: 5000 },
            priority: MessagePriority::Normal,
            max_payload_bytes: 8192,
            block_timeout_ms: Some(5000),
            flow_credit_window: None,
        });
        channels.insert("plugin.capability".into(), ChannelConfig {
            transport: TransportKind::Socket,
            shm_slots: 0,
            shm_slot_size_bytes: 0,
            backpressure: BackpressurePolicy::Block { timeout_ms: 500 },
            priority: MessagePriority::Normal,
            max_payload_bytes: 16384,
            block_timeout_ms: Some(500),
            flow_credit_window: None,
        });
        channels.insert("plugin.invoke".into(), ChannelConfig {
            transport: TransportKind::Socket,
            shm_slots: 0,
            shm_slot_size_bytes: 0,
            backpressure: BackpressurePolicy::ErrorOnFull,
            priority: MessagePriority::Normal,
            max_payload_bytes: 262144,
            block_timeout_ms: None,
            flow_credit_window: None,
        });

        // Tier 3 — In-Process
        channels.insert("desktop.event".into(), ChannelConfig {
            transport: TransportKind::InProcess,
            shm_slots: 0,
            shm_slot_size_bytes: 0,
            backpressure: BackpressurePolicy::DropOldest,
            priority: MessagePriority::Normal,
            max_payload_bytes: 1024,
            block_timeout_ms: None,
            flow_credit_window: None,
        });

        Self {
            process_id: None,
            bus: BusConfig::default(),
            channels,
            heartbeat: HeartbeatConfig::default(),
            reconnect: ReconnectConfig::default(),
            auth: AuthConfig::default(),
            rate_limits: RateLimitConfig::default(),
            runtime_dir: std::env::temp_dir().join("lumi-ipc"),
        }
    }
}

/// Bus-wide configuration.
#[derive(Debug, Clone)]
pub struct BusConfig {
    /// Maximum pending messages in the bus queue
    pub max_pending_messages: usize,
    /// Default message TTL in milliseconds
    pub message_ttl_default_ms: u32,
    /// Enable message tracing (dev only, very verbose)
    pub enable_message_tracing: bool,
}

impl Default for BusConfig {
    fn default() -> Self {
        Self {
            max_pending_messages: 4096,
            message_ttl_default_ms: 30000,
            enable_message_tracing: false,
        }
    }
}

/// Per-channel configuration.
#[derive(Debug, Clone)]
pub struct ChannelConfig {
    /// Transport type
    pub transport: TransportKind,
    /// Shared memory ring buffer slot count (SHM only)
    pub shm_slots: u32,
    /// Shared memory slot size in bytes (SHM only)
    pub shm_slot_size_bytes: u32,
    /// Backpressure policy
    pub backpressure: BackpressurePolicy,
    /// Message priority
    pub priority: MessagePriority,
    /// Maximum payload size in bytes
    pub max_payload_bytes: u32,
    /// Block timeout in ms (Block policy only)
    pub block_timeout_ms: Option<u64>,
    /// Flow control credit window (FlowControl policy only)
    pub flow_credit_window: Option<usize>,
}

/// Transport selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    /// Tier 1: Lock-free shared memory ring buffer
    SharedMemory,
    /// Tier 2: Unix domain socket / Windows named pipe
    Socket,
    /// Tier 3: In-process broadcast/mpsc
    InProcess,
}

impl TransportKind {
    pub fn name(&self) -> &'static str {
        match self {
            TransportKind::SharedMemory => "shared-memory",
            TransportKind::Socket => "socket",
            TransportKind::InProcess => "in-process",
        }
    }
}

/// Backpressure policy for a channel.
#[derive(Debug, Clone)]
pub enum BackpressurePolicy {
    /// Block the sender until space is available
    Block { timeout_ms: u64 },
    /// Drop the oldest message when queue is full
    DropOldest,
    /// Drop the newest message when queue is full
    DropNewest,
    /// Return an error to the sender immediately
    ErrorOnFull,
    /// Streaming flow control with credit window
    FlowControl { credit_window: usize },
}

/// Message priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl MessagePriority {
    pub fn name(&self) -> &'static str {
        match self {
            MessagePriority::Low => "low",
            MessagePriority::Normal => "normal",
            MessagePriority::High => "high",
            MessagePriority::Critical => "critical",
        }
    }
}

/// Heartbeat configuration.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Interval between heartbeats
    pub interval: Duration,
    /// Timeout after which a peer is considered dead
    pub timeout: Duration,
    /// Alert if ping RTT exceeds this
    pub latency_warn_us: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            timeout: Duration::from_secs(15),
            latency_warn_us: 10_000,
        }
    }
}

/// Reconnection policy.
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
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

impl Default for ReconnectConfig {
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

/// Authentication configuration.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Handshake timeout
    pub handshake_timeout: Duration,
    /// Replay window size (number of sequence numbers tracked)
    pub replay_window_size: usize,
    /// Key rotation interval
    pub key_rotation_interval: Duration,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            handshake_timeout: Duration::from_millis(200),
            replay_window_size: 1024,
            key_rotation_interval: Duration::from_secs(3600),
        }
    }
}

/// Rate limiting configuration.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Default messages per second per sender
    pub default_messages_per_second: f64,
    /// Plugin messages per second limit
    pub plugin_messages_per_second: f64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            default_messages_per_second: 1000.0,
            plugin_messages_per_second: 100.0,
        }
    }
}
