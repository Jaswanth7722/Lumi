//! # IPC — Inter-Process Communication Types (Chapter 5)
//!
//! Defines the typed message bus infrastructure for the Lumas platform.
//! All inter-process communication uses MessagePack-serialized messages
//! over Unix domain sockets (macOS/Linux) or named pipes (Windows).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique process identifier within the Lumas platform.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProcessId {
    /// The main core process orchestrating all AI activity.
    Core,
    /// The GPU rendering process for the character and workspace.
    Render,
    /// The audio/voice process (wake word, STT, TTS).
    Voice,
    /// The persistent storage process (memory, config, cache).
    Storage,
    /// The plugin host process managing Wasm sandboxes.
    PluginHost,
    /// A specific plugin by name.
    Plugin(String),
}

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessId::Core => write!(f, "core"),
            ProcessId::Render => write!(f, "render"),
            ProcessId::Voice => write!(f, "voice"),
            ProcessId::Storage => write!(f, "storage"),
            ProcessId::PluginHost => write!(f, "plugin-host"),
            ProcessId::Plugin(name) => write!(f, "plugin:{name}"),
        }
    }
}

/// The type of an IPC message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    /// A request requiring a response.
    Request,
    /// A response to a prior request.
    Response,
    /// A fire-and-forget event notification.
    Event,
    /// An error response.
    Error,
}

/// A fully-typed IPC message on the Lumas platform bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LumiMessage {
    /// UUID v4 uniquely identifying this message.
    pub id: String,
    /// Protocol version (currently 1).
    pub version: u32,
    /// Sending process identifier.
    pub source: ProcessId,
    /// Receiving process or "broadcast" for all subscribers.
    pub target: ProcessId,
    /// Logical channel name.
    pub channel: Channel,
    /// Message type semantics.
    pub msg_type: MessageType,
    /// Channel-specific typed payload as raw JSON value.
    pub payload: serde_json::Value,
    /// Unix timestamp in milliseconds.
    pub timestamp: i64,
    /// Optional distributed trace ID for request/response correlation.
    pub trace_id: Option<String>,
}

/// Core IPC channels on the Lumas message bus.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Channel {
    // -- AI Core Channels --
    /// AI processing state changes (Core → Render).
    AiState,
    /// Internal AI orchestration commands (Core → Core).
    AiCommand,
    /// State events emitted from the State Machine (Core → all).
    StateEvent,

    // -- Render Channels --
    /// Behavior and animation commands (Core → Render).
    RenderCommand,
    /// User interactions with the character (Render → Core).
    RenderInput,

    // -- Voice Channels --
    /// Transcribed speech text (Voice → Core).
    VoiceInput,
    /// TTS generation requests (Core → Voice).
    VoiceOutput,

    // -- Memory/Storage Channels --
    /// Memory persistence requests (Core → Storage).
    MemoryWrite,
    /// Memory retrieval requests (Core → Storage).
    MemoryQuery,
    /// Config read/write requests (any → Storage).
    ConfigOperation,

    // -- Desktop Channels --
    /// Desktop awareness events (Core → Core internal).
    DesktopEvent,

    // -- Plugin Channels --
    /// Plugin capability registrations (PluginHost → Core).
    PluginCapability,
    /// Plugin tool execution requests (Core → PluginHost).
    PluginInvoke,
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Channel::AiState => write!(f, "ai.state"),
            Channel::AiCommand => write!(f, "ai.command"),
            Channel::StateEvent => write!(f, "state.event"),
            Channel::RenderCommand => write!(f, "render.command"),
            Channel::RenderInput => write!(f, "render.input"),
            Channel::VoiceInput => write!(f, "voice.input"),
            Channel::VoiceOutput => write!(f, "voice.output"),
            Channel::MemoryWrite => write!(f, "memory.write"),
            Channel::MemoryQuery => write!(f, "memory.query"),
            Channel::ConfigOperation => write!(f, "config.operation"),
            Channel::DesktopEvent => write!(f, "desktop.event"),
            Channel::PluginCapability => write!(f, "plugin.capability"),
            Channel::PluginInvoke => write!(f, "plugin.invoke"),
        }
    }
}

impl std::str::FromStr for Channel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ai.state" => Ok(Channel::AiState),
            "ai.command" => Ok(Channel::AiCommand),
            "state.event" => Ok(Channel::StateEvent),
            "render.command" => Ok(Channel::RenderCommand),
            "render.input" => Ok(Channel::RenderInput),
            "voice.input" => Ok(Channel::VoiceInput),
            "voice.output" => Ok(Channel::VoiceOutput),
            "memory.write" => Ok(Channel::MemoryWrite),
            "memory.query" => Ok(Channel::MemoryQuery),
            "config.operation" => Ok(Channel::ConfigOperation),
            "desktop.event" => Ok(Channel::DesktopEvent),
            "plugin.capability" => Ok(Channel::PluginCapability),
            "plugin.invoke" => Ok(Channel::PluginInvoke),
            other => Err(format!("Unknown channel: {other}")),
        }
    }
}

impl LumiMessage {
    /// Create a new request message.
    pub fn new_request(
        source: ProcessId,
        target: ProcessId,
        channel: Channel,
        payload: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            version: 1,
            source,
            target,
            channel,
            msg_type: MessageType::Request,
            payload: serde_json::to_value(payload)?,
            timestamp: chrono::Utc::now().timestamp_millis(),
            trace_id: None,
        })
    }

    /// Create a new response message correlated to a request.
    pub fn new_response(
        request: &LumiMessage,
        payload: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            version: 1,
            source: request.target.clone(),
            target: request.source.clone(),
            channel: request.channel.clone(),
            msg_type: MessageType::Response,
            payload: serde_json::to_value(payload)?,
            timestamp: chrono::Utc::now().timestamp_millis(),
            trace_id: request.trace_id.clone(),
        })
    }

    /// Create a new event message (fire-and-forget).
    pub fn new_event(
        source: ProcessId,
        channel: Channel,
        payload: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            version: 1,
            source,
            target: ProcessId::Core, // default target
            channel,
            msg_type: MessageType::Event,
            payload: serde_json::to_value(payload)?,
            timestamp: chrono::Utc::now().timestamp_millis(),
            trace_id: None,
        })
    }

    /// Create a new error message.
    pub fn new_error(request: &LumiMessage, error: impl ToString) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            version: 1,
            source: request.target.clone(),
            target: request.source.clone(),
            channel: request.channel.clone(),
            msg_type: MessageType::Error,
            payload: serde_json::json!({ "error": error.to_string() }),
            timestamp: chrono::Utc::now().timestamp_millis(),
            trace_id: request.trace_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_display_and_parse() {
        let channels = [
            (Channel::AiState, "ai.state"),
            (Channel::RenderCommand, "render.command"),
            (Channel::VoiceInput, "voice.input"),
            (Channel::MemoryWrite, "memory.write"),
            (Channel::PluginCapability, "plugin.capability"),
        ];
        for (channel, expected_str) in &channels {
            assert_eq!(&channel.to_string(), expected_str);
            assert_eq!(channel.to_string().parse::<Channel>(), Ok(channel.clone()));
        }
    }

    #[test]
    fn test_message_creation() {
        let msg = LumiMessage::new_request(
            ProcessId::Core,
            ProcessId::Render,
            Channel::RenderCommand,
            serde_json::json!({"animation": "walk"}),
        )
        .unwrap();

        assert_eq!(msg.source, ProcessId::Core);
        assert_eq!(msg.target, ProcessId::Render);
        assert_eq!(msg.channel, Channel::RenderCommand);
        assert_eq!(msg.msg_type, MessageType::Request);
        assert_eq!(msg.version, 1);
    }

    #[test]
    fn test_response_correlation() {
        let req = LumiMessage::new_request(
            ProcessId::Core,
            ProcessId::Storage,
            Channel::MemoryQuery,
            serde_json::json!({"query": "user preferences"}),
        )
        .unwrap();

        let res = LumiMessage::new_response(&req, serde_json::json!({"results": []})).unwrap();

        assert_eq!(res.source, ProcessId::Storage);
        assert_eq!(res.target, ProcessId::Core);
        assert_eq!(res.channel, Channel::MemoryQuery);
        assert_eq!(res.msg_type, MessageType::Response);
        assert_eq!(res.trace_id, req.trace_id);
    }
}
