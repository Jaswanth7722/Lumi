//! # Serialization Utilities
//!
//! Provides MessagePack and JSON serialization/deserialization for IPC messages.
//! MessagePack is the primary wire format; JSON is used for debugging and testing.

use crate::error::CodecError;
use crate::message::LumiMessage;

/// Serialize a LumiMessage to MessagePack bytes.
pub fn to_msgpack(msg: &LumiMessage) -> Result<Vec<u8>, CodecError> {
    rmp_serde::to_vec(msg)
        .map_err(|e| CodecError::Serialization(e.to_string()))
}

/// Deserialize a LumiMessage from MessagePack bytes.
pub fn from_msgpack(bytes: &[u8]) -> Result<LumiMessage, CodecError> {
    rmp_serde::from_slice(bytes)
        .map_err(|e| CodecError::Deserialization(e.to_string()))
}

/// Serialize a LumiMessage to a JSON string.
pub fn to_json(msg: &LumiMessage) -> Result<String, CodecError> {
    serde_json::to_string_pretty(msg)
        .map_err(|e| CodecError::Serialization(e.to_string()))
}

/// Deserialize a LumiMessage from a JSON string.
pub fn from_json(json: &str) -> Result<LumiMessage, CodecError> {
    serde_json::from_str(json)
        .map_err(|e| CodecError::Deserialization(e.to_string()))
}

/// Convert from the new LumiMessage format to the legacy format.
/// Used during migration from `lumas_common::ipc::LumiMessage`.
pub fn to_legacy(msg: &LumiMessage) -> lumas_common::ipc::LumiMessage {
    use lumas_common::ipc::{Channel, MessageType};
    use serde_json::Value;

    // Map ChannelName to legacy Channel
    let channel = match msg.channel.0.as_str() {
        "ai.state" => Channel::AiState,
        "render.command" => Channel::RenderCommand,
        "render.input" => Channel::RenderInput,
        "voice.input" => Channel::VoiceInput,
        "voice.output" => Channel::VoiceOutput,
        "memory.write" => Channel::MemoryWrite,
        "memory.query" => Channel::MemoryQuery,
        "desktop.event" => Channel::DesktopEvent,
        "plugin.capability" => Channel::PluginCapability,
        "plugin.invoke" => Channel::PluginInvoke,
        _ => Channel::ConfigOperation,
    };

    // Map MessageKind to legacy MessageType
    let msg_type = match &msg.kind {
        crate::message::MessageKind::Request { .. } => MessageType::Request,
        crate::message::MessageKind::Response { .. } => MessageType::Response,
        crate::message::MessageKind::Event
        | crate::message::MessageKind::Notification { .. } => MessageType::Event,
        crate::message::MessageKind::Command => MessageType::Request,
        _ => MessageType::Event,
    };

    let payload = serde_json::to_value(&msg.payload).unwrap_or(Value::Null);

    lumas_common::ipc::LumiMessage {
        id: msg.id.0.clone(),
        version: 1,
        source: msg.sender.clone(),
        target: match &msg.receiver {
            crate::message::MessageTarget::Process(p) => p.clone(),
            _ => lumas_common::ipc::ProcessId::Core,
        },
        channel,
        msg_type,
        payload,
        timestamp: chrono::Utc::now().timestamp_millis(),
        trace_id: None,
    }
}

/// Convert from legacy LumiMessage format to the new format.
pub fn from_legacy(msg: &lumas_common::ipc::LumiMessage) -> Result<LumiMessage, CodecError> {
    use crate::message::MessagePayload;

    let payload = MessagePayload::Empty;

    Ok(LumiMessage {
        id: crate::message::MessageId(msg.id.clone()),
        correlation_id: crate::message::CorrelationId::new(),
        conversation_id: crate::message::ConversationId::new(),
        session_id: crate::message::SessionId::new(),
        sender: msg.source.clone(),
        receiver: crate::message::MessageTarget::Process(msg.target.clone()),
        channel: crate::message::ChannelName(msg.channel.to_string()),
        timestamp: msg.timestamp as u64 * 1000,
        sequence: 0,
        version: crate::message::ProtocolVersion::CURRENT,
        kind: match msg.msg_type {
            lumas_common::ipc::MessageType::Request =>
                crate::message::MessageKind::Request {
                    reply_to: crate::message::ChannelName(msg.channel.to_string()),
                },
            lumas_common::ipc::MessageType::Response =>
                crate::message::MessageKind::Response {
                    request_id: crate::message::MessageId("".into()),
                    status: crate::message::ResponseStatus::Success,
                },
            lumas_common::ipc::MessageType::Error =>
                crate::message::MessageKind::Error {
                    error_code: "UNKNOWN".into(),
                    recoverable: false,
                },
            lumas_common::ipc::MessageType::Event =>
                crate::message::MessageKind::Event,
        },
        priority: 1,
        ttl_ms: Some(30000),
        auth: None,
        payload,
        metadata: crate::message::MessageMetadata::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{MessagePayload, ProcessId};

    #[test]
    fn test_msgpack_roundtrip() {
        let msg = LumiMessage::new_event(
            ProcessId::Core,
            "test",
            MessagePayload::Empty,
        );

        let bytes = to_msgpack(&msg).unwrap();
        let decoded = from_msgpack(&bytes).unwrap();

        assert_eq!(msg.id, decoded.id);
        assert_eq!(msg.sender, decoded.sender);
    }

    #[test]
    fn test_json_roundtrip() {
        let msg = LumiMessage::new_event(
            ProcessId::Core,
            "test",
            MessagePayload::Empty,
        );

        let json = to_json(&msg).unwrap();
        let decoded = from_json(&json).unwrap();

        assert_eq!(msg.id, decoded.id);
    }
}
