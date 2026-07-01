//! # Message Type System
//!
//! Defines the sealed, authenticated envelope for all IPC messages, plus all
//! typed payload structures for each Lumas platform channel.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Re-export ProcessId from lumi-common for convenience.
pub use lumas_common::ipc::ProcessId;

/// A UUID v7 message identifier (time-ordered for log correlation).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn new() -> Self {
        MessageId(uuid::Uuid::now_v7().to_string())
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Correlation ID linking request → response → follow-up messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    pub fn new() -> Self {
        CorrelationId(uuid::Uuid::now_v7().to_string())
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Conversation ID groups an entire interaction session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationId(pub String);

impl ConversationId {
    pub fn new() -> Self {
        ConversationId(uuid::Uuid::now_v7().to_string())
    }
}

impl Default for ConversationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Session ID for the current platform session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        SessionId(uuid::Uuid::now_v7().to_string())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Channel name as a domain-qualified string (e.g. "render.command", "ai.state").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelName(pub String);

impl ChannelName {
    pub fn new(name: impl Into<String>) -> Self {
        ChannelName(name.into())
    }
}

impl std::fmt::Display for ChannelName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for ChannelName {
    fn from(s: &str) -> Self {
        ChannelName(s.to_string())
    }
}

impl From<String> for ChannelName {
    fn from(s: String) -> Self {
        ChannelName(s)
    }
}

/// Protocol version for wire-level compatibility negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    pub const CURRENT: ProtocolVersion = ProtocolVersion { major: 1, minor: 0 };

    pub fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Check compatibility: this version can communicate with `other`.
    /// Compatible if major versions match and minor is within range.
    pub fn compatible_with(&self, other: &ProtocolVersion) -> bool {
        self.major == other.major
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Message sequence number for replay attack prevention.
static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_sequence() -> u64 {
    SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst)
}

// ---------------------------------------------------------------------------
// LumiMessage — the sealed, authenticated envelope
// ---------------------------------------------------------------------------

/// The immutable wire envelope for all Lumas IPC messages.
/// Created via `MessageBuilder`; fields are not publicly settable after construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LumiMessage {
    // Routing and identity (never encrypted)
    pub id: MessageId,
    pub correlation_id: CorrelationId,
    pub conversation_id: ConversationId,
    pub session_id: SessionId,
    pub sender: ProcessId,
    pub receiver: MessageTarget,
    pub channel: ChannelName,
    pub timestamp: u64, // Unix timestamp microseconds
    pub sequence: u64,  // monotonic per-sender, for replay prevention

    // Protocol metadata
    pub version: ProtocolVersion,
    pub kind: MessageKind,
    pub priority: u8, // 0=Low, 1=Normal, 2=High, 3=Critical
    pub ttl_ms: Option<u32>,

    // Authentication
    pub auth: Option<MessageAuth>,

    // Payload
    pub payload: MessagePayload,

    // Extensible metadata
    pub metadata: MessageMetadata,
}

impl LumiMessage {
    /// Create a new builder for constructing a LumiMessage.
    pub fn builder() -> MessageBuilder {
        MessageBuilder::new()
    }

    /// Create a new event message (fire-and-forget).
    pub fn new_event(
        sender: ProcessId,
        channel: impl Into<ChannelName>,
        payload: MessagePayload,
    ) -> Self {
        let channel = channel.into();
        Self {
            id: MessageId::new(),
            correlation_id: CorrelationId::new(),
            conversation_id: ConversationId::new(),
            session_id: SessionId::new(),
            sender: sender.clone(),
            receiver: MessageTarget::Broadcast,
            channel: channel.clone(),
            timestamp: chrono::Utc::now().timestamp_micros() as u64,
            sequence: next_sequence(),
            version: ProtocolVersion::CURRENT,
            kind: MessageKind::Event,
            priority: 1,
            ttl_ms: Some(30000),
            auth: None,
            payload,
            metadata: MessageMetadata::new(),
        }
    }

    /// Create a new command message.
    pub fn new_command(
        sender: ProcessId,
        receiver: ProcessId,
        channel: impl Into<ChannelName>,
        payload: MessagePayload,
    ) -> Self {
        let channel = channel.into();
        Self {
            id: MessageId::new(),
            correlation_id: CorrelationId::new(),
            conversation_id: ConversationId::new(),
            session_id: SessionId::new(),
            sender,
            receiver: MessageTarget::Process(receiver),
            channel,
            timestamp: chrono::Utc::now().timestamp_micros() as u64,
            sequence: next_sequence(),
            version: ProtocolVersion::CURRENT,
            kind: MessageKind::Command,
            priority: 2,
            ttl_ms: Some(10000),
            auth: None,
            payload,
            metadata: MessageMetadata::new(),
        }
    }

    /// Create a new request message.
    pub fn new_request(
        sender: ProcessId,
        receiver: ProcessId,
        channel: impl Into<ChannelName>,
        payload: MessagePayload,
    ) -> Self {
        let channel = channel.into();
        let reply_to = channel.clone();
        Self {
            id: MessageId::new(),
            correlation_id: CorrelationId::new(),
            conversation_id: ConversationId::new(),
            session_id: SessionId::new(),
            sender,
            receiver: MessageTarget::Process(receiver),
            channel,
            timestamp: chrono::Utc::now().timestamp_micros() as u64,
            sequence: next_sequence(),
            version: ProtocolVersion::CURRENT,
            kind: MessageKind::Request { reply_to },
            priority: 1,
            ttl_ms: Some(30000),
            auth: None,
            payload,
            metadata: MessageMetadata::new(),
        }
    }

    /// Create a response correlated to a request.
    pub fn new_response(
        request: &LumiMessage,
        payload: MessagePayload,
        status: ResponseStatus,
    ) -> Self {
        Self {
            id: MessageId::new(),
            correlation_id: request.correlation_id.clone(),
            conversation_id: request.conversation_id.clone(),
            session_id: request.session_id.clone(),
            sender: request.receiver.as_process().cloned()
                .unwrap_or_else(|| ProcessId::Core),
            receiver: MessageTarget::Process(request.sender.clone()),
            channel: request.channel.clone(),
            timestamp: chrono::Utc::now().timestamp_micros() as u64,
            sequence: next_sequence(),
            version: request.version,
            kind: MessageKind::Response { request_id: request.id.clone(), status },
            priority: 1,
            ttl_ms: request.ttl_ms,
            auth: None,
            payload,
            metadata: MessageMetadata::new(),
        }
    }

    /// Check if this message has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(ttl_ms) = self.ttl_ms {
            let now = chrono::Utc::now().timestamp_micros() as u64;
            let age_us = now.saturating_sub(self.timestamp);
            (age_us / 1000) > ttl_ms as u64
        } else {
            false
        }
    }
}

/// Builder for constructing LumiMessage with a fluent API.
#[derive(Debug, Clone)]
pub struct MessageBuilder {
    sender: Option<ProcessId>,
    receiver: Option<MessageTarget>,
    channel: Option<ChannelName>,
    kind: Option<MessageKind>,
    payload: Option<MessagePayload>,
    priority: u8,
    ttl_ms: Option<u32>,
    correlation_id: Option<CorrelationId>,
    conversation_id: Option<ConversationId>,
}

impl MessageBuilder {
    pub fn new() -> Self {
        Self {
            sender: None,
            receiver: None,
            channel: None,
            kind: None,
            payload: None,
            priority: 1,
            ttl_ms: Some(30000),
            correlation_id: None,
            conversation_id: None,
        }
    }

    pub fn sender(mut self, sender: ProcessId) -> Self {
        self.sender = Some(sender);
        self
    }

    pub fn receiver(mut self, receiver: MessageTarget) -> Self {
        self.receiver = Some(receiver);
        self
    }

    pub fn channel(mut self, channel: impl Into<ChannelName>) -> Self {
        self.channel = Some(channel.into());
        self
    }

    pub fn kind(mut self, kind: MessageKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn payload(mut self, payload: MessagePayload) -> Self {
        self.payload = Some(payload);
        self
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn ttl(mut self, ttl_ms: u32) -> Self {
        self.ttl_ms = Some(ttl_ms);
        self
    }

    pub fn build(self) -> Result<LumiMessage, String> {
        let sender = self.sender.ok_or("sender is required")?;
        let receiver = self.receiver.ok_or("receiver is required")?;
        let channel = self.channel.ok_or("channel is required")?;
        let kind = self.kind.ok_or("kind is required")?;
        let payload = self.payload.ok_or("payload is required")?;

        Ok(LumiMessage {
            id: MessageId::new(),
            correlation_id: self.correlation_id.unwrap_or_default(),
            conversation_id: self.conversation_id.unwrap_or_default(),
            session_id: SessionId::new(),
            sender,
            receiver,
            channel,
            timestamp: chrono::Utc::now().timestamp_micros() as u64,
            sequence: next_sequence(),
            version: ProtocolVersion::CURRENT,
            kind,
            priority: self.priority,
            ttl_ms: self.ttl_ms,
            auth: None,
            payload,
            metadata: MessageMetadata::new(),
        })
    }
}

impl Default for MessageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Message envelope types
// ---------------------------------------------------------------------------

/// Target of a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageTarget {
    /// Route to a specific process.
    Process(ProcessId),
    /// Route to all subscribers of the channel.
    Broadcast,
    /// Route to a topic pattern subscribers.
    Topic(String),
}

impl MessageTarget {
    pub fn as_process(&self) -> Option<&ProcessId> {
        match self {
            MessageTarget::Process(p) => Some(p),
            _ => None,
        }
    }
}

impl PartialEq for MessageTarget {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MessageTarget::Process(a), MessageTarget::Process(b)) => a == b,
            (MessageTarget::Broadcast, MessageTarget::Broadcast) => true,
            (MessageTarget::Topic(a), MessageTarget::Topic(b)) => a == b,
            _ => false,
        }
    }
}

/// Kind of message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageKind {
    /// A command (fire-and-forget, no response expected).
    Command,
    /// A request expecting a response.
    Request {
        reply_to: ChannelName,
    },
    /// A response to a prior request.
    Response {
        request_id: MessageId,
        status: ResponseStatus,
    },
    /// A fire-and-forget event notification.
    Event,
    /// A notification with a severity level.
    Notification {
        level: NotificationLevel,
    },
    /// A heartbeat ping or pong.
    Heartbeat,
    /// Open a streaming session.
    StreamOpen {
        stream_id: u64,
        total_chunks: Option<u32>,
    },
    /// A chunk in a streaming session.
    StreamChunk {
        stream_id: u64,
        chunk_index: u32,
        is_final: bool,
    },
    /// Close a streaming session.
    StreamClose {
        stream_id: u64,
        reason: StreamCloseReason,
    },
    /// An error response.
    Error {
        error_code: String,
        recoverable: bool,
    },
    /// Broadcast to all subscribers.
    Broadcast,
    // Protocol control
    HandshakeRequest,
    HandshakeResponse,
    Disconnect {
        reason: DisconnectReason,
    },
}

/// Response status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseStatus {
    Success,
    Error { code: String, message: String },
    NotFound,
    Timeout,
    Busy,
}

impl ResponseStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, ResponseStatus::Success)
    }
}

/// Notification severity level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
    Critical,
}

/// Reason for stream closure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamCloseReason {
    Completed,
    Cancelled,
    Error(String),
    Timeout,
}

/// Reason for disconnection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisconnectReason {
    Shutdown,
    Upgrade,
    ProtocolError(String),
    AuthFailure(String),
    HeartbeatTimeout,
}

// ---------------------------------------------------------------------------
// Message authentication
// ---------------------------------------------------------------------------

/// Authentication data attached to each message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAuth {
    pub process_token: ProcessToken,
    pub mac: [u8; 32],
    pub key_id: u64,
}

/// Process identity token — single-session proof of identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessToken(pub String);

// ---------------------------------------------------------------------------
// Message payload — the typed inner content
// ---------------------------------------------------------------------------

/// Typed, schema-versioned inner message payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePayload {
    // --- Channel: ai.state ---
    AiState(AiStatePayload),

    // --- Channel: render.command ---
    RenderCommand(RenderCommandPayload),
    RenderInput(RenderInputPayload),

    // --- Channel: voice.input / voice.output ---
    VoiceInput(VoiceInputPayload),
    VoiceOutput(VoiceOutputPayload),

    // --- Channel: memory.write / memory.query ---
    MemoryWrite(MemoryWritePayload),
    MemoryQuery(MemoryQueryPayload),
    MemoryQueryResult(MemoryQueryResultPayload),

    // --- Channel: plugin.capability ---
    PluginCapability(PluginCapabilityPayload),

    // --- Channel: plugin.invoke ---
    PluginInvoke(PluginInvokePayload),
    PluginInvokeResult(PluginInvokeResultPayload),

    // --- Channel: desktop.event ---
    DesktopEvent(DesktopEventPayload),

    // --- Protocol control ---
    Handshake(HandshakePayload),

    // Heartbeat
    HeartbeatPing { sent_at: u64 },
    HeartbeatPong { ping_id: MessageId, latency_us: u64 },

    // Extension point for future channels and plugins
    Extension { type_id: u32, schema_version: u16, data: Vec<u8> },

    /// Empty/no payload
    Empty,
}

// ---------------------------------------------------------------------------
// Typed payload structures
// ---------------------------------------------------------------------------

/// AI State change payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiStatePayload {
    pub state: String,
    pub previous_state: Option<String>,
    pub confidence: f64,
    pub metadata: HashMap<String, String>,
}

/// Render command payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCommandPayload {
    pub command: String,
    pub params: serde_json::Value,
    pub priority: u8,
}

/// Render input (user interaction) payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderInputPayload {
    pub input_type: String,
    pub data: serde_json::Value,
    pub position: Option<(f64, f64)>,
}

/// Voice input (from microphone) payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInputPayload {
    pub audio_data: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u8,
    pub is_final: bool,
    pub sequence: u32,
}

/// Voice output (TTS) payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceOutputPayload {
    pub text: String,
    pub voice: String,
    pub speed: f32,
    pub pitch: f32,
}

/// Memory write payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryWritePayload {
    pub key: String,
    pub value: serde_json::Value,
    pub namespace: String,
    pub ttl_seconds: Option<u64>,
}

/// Memory query payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQueryPayload {
    pub query: String,
    pub limit: u32,
    pub min_confidence: f64,
    pub namespace: Option<String>,
}

/// Memory query result payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQueryResultPayload {
    pub results: Vec<MemoryEntry>,
    pub query_time_us: u64,
}

/// A single memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub confidence: f64,
    pub created_at: i64,
    pub namespace: String,
}

/// Plugin capability registration payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapabilityPayload {
    pub plugin_id: String,
    pub plugin_name: String,
    pub version: String,
    pub tools: Vec<String>,
    pub required_capabilities: Vec<String>,
}

/// Plugin invocation payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInvokePayload {
    pub plugin_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub timeout_ms: u64,
    pub correlation_id: String,
}

/// Plugin invocation result payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInvokeResultPayload {
    pub correlation_id: String,
    pub success: bool,
    pub result: serde_json::Value,
    pub execution_time_us: u64,
    pub error: Option<String>,
}

/// Desktop event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopEventPayload {
    pub event_type: String,
    pub data: serde_json::Value,
    pub timestamp: i64,
}

/// Handshake payload for connection establishment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakePayload {
    pub process_id: ProcessId,
    pub capabilities: CapabilitySet,
    pub ephemeral_pk: Vec<u8>,
    pub nonce: Vec<u8>,
}

/// Capability set exchanged during handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySet {
    pub can_publish: Vec<String>,
    pub can_subscribe: Vec<String>,
    pub supported_message_versions: Vec<(String, u16)>,
    pub supported_wire_versions: (u16, u16),
    pub supports_compression: bool,
    pub supports_encryption: bool,
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self {
            can_publish: vec![],
            can_subscribe: vec![],
            supported_message_versions: vec![],
            supported_wire_versions: (1, 1),
            supports_compression: false,
            supports_encryption: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Message metadata
// ---------------------------------------------------------------------------

/// Extensible metadata for middleware (tracing spans, compression hints, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    inner: HashMap<String, MetadataValue>,
}

impl MessageMetadata {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: MetadataValue) {
        self.inner.insert(key.into(), value);
    }

    pub fn get(&self, key: &str) -> Option<&MetadataValue> {
        self.inner.get(key)
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for MessageMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// A value in the metadata map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetadataValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Bytes(Vec<u8>),
}

impl From<String> for MetadataValue {
    fn from(s: String) -> Self {
        MetadataValue::String(s)
    }
}

impl From<&str> for MetadataValue {
    fn from(s: &str) -> Self {
        MetadataValue::String(s.to_string())
    }
}

impl From<i64> for MetadataValue {
    fn from(i: i64) -> Self {
        MetadataValue::Int(i)
    }
}

impl From<f64> for MetadataValue {
    fn from(f: f64) -> Self {
        MetadataValue::Float(f)
    }
}

impl From<bool> for MetadataValue {
    fn from(b: bool) -> Self {
        MetadataValue::Bool(b)
    }
}

// ---------------------------------------------------------------------------
// Channel name constants
// ---------------------------------------------------------------------------

/// Well-known channel names in the Lumas platform.
pub mod channels {
    use super::ChannelName;

    pub fn ai_state() -> ChannelName { ChannelName("ai.state".into()) }
    pub fn ai_command() -> ChannelName { ChannelName("ai.command".into()) }
    pub fn state_event() -> ChannelName { ChannelName("state.event".into()) }
    pub fn render_command() -> ChannelName { ChannelName("render.command".into()) }
    pub fn render_input() -> ChannelName { ChannelName("render.input".into()) }
    pub fn voice_input() -> ChannelName { ChannelName("voice.input".into()) }
    pub fn voice_output() -> ChannelName { ChannelName("voice.output".into()) }
    pub fn memory_write() -> ChannelName { ChannelName("memory.write".into()) }
    pub fn memory_query() -> ChannelName { ChannelName("memory.query".into()) }
    pub fn config_operation() -> ChannelName { ChannelName("config.operation".into()) }
    pub fn desktop_event() -> ChannelName { ChannelName("desktop.event".into()) }
    pub fn plugin_capability() -> ChannelName { ChannelName("plugin.capability".into()) }
    pub fn plugin_invoke() -> ChannelName { ChannelName("plugin.invoke".into()) }
}
