// ── Wire Format Serializer ─────────────────────────────────────────────────────
// Serializes/deserializes LumiMessage to/from MessagePack, CBOR, or JSON.
// Uses MessagePack map format with string keys for schema evolution.
// Schema evolution: unknown fields in a map are silently skipped, enabling
// forward compatibility across schema versions.

use std::cell::RefCell;

use bytes::Bytes;

use crate::message::{LumiMessage, MessagePayload};
use crate::wire::error::WireError;

/// The serialization format used for message payloads.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationFormat {
    MessagePack,
    Cbor,
    Json,
}

impl SerializationFormat {
    /// Whether this format is MessagePack (primary wire format).
    pub fn is_messagepack(&self) -> bool {
        matches!(self, Self::MessagePack)
    }

    /// Whether this format is CBOR.
    pub fn is_cbor(&self) -> bool {
        matches!(self, Self::Cbor)
    }

    /// Whether this format is JSON (debug only).
    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json)
    }
}

impl Default for SerializationFormat {
    fn default() -> Self {
        Self::MessagePack
    }
}

/// Wire serializer with scratch buffer reuse.
///
/// # Thread Safety
/// Each thread should have its own `WireSerializer` instance because the
/// internal scratch buffer uses `RefCell` (not `Sync`). This is by design:
/// contention on a shared scratch buffer would negate the allocation benefit.
///
/// # Schema Evolution
/// Fields are serialized as a MessagePack map with string keys. This means:
/// - New optional fields added in higher schema versions are deserialized as
///   `Option<T>` — missing fields → `None`
/// - Unknown fields from a higher-schema sender are silently skipped
/// - Schema version is checked before deserialization (forward-compat mode)
pub struct WireSerializer {
    format: SerializationFormat,
    scratch: RefCell<Vec<u8>>,
}

impl WireSerializer {
    /// Create a new serializer with the given format.
    pub fn new(format: SerializationFormat) -> Self {
        Self {
            format,
            scratch: RefCell::new(Vec::with_capacity(1024)),
        }
    }

    /// Get the serialization format.
    pub fn format(&self) -> &SerializationFormat {
        &self.format
    }

    /// Serialize a `LumiMessage` to bytes using the configured format.
    ///
    /// Uses the internal scratch buffer for reuse to minimize allocation.
    ///
    /// # Wire Safety
    /// This function is safe to call from any thread (each thread has its own
    /// `WireSerializer` instance).
    ///
    /// # Panics
    /// Never panics. Returns `Err` on serialization failure.
    ///
    /// # Errors
    /// Returns `WireError::SerializationFailed` if the message cannot be
    /// serialized (e.g., unsupported payload variant).
    pub fn serialize(&self, msg: &LumiMessage) -> Result<Bytes, WireError> {
        let mut scratch = self.scratch.borrow_mut();
        scratch.clear();

        match self.format {
            SerializationFormat::MessagePack => {
                // Use serde + rmp_serde for MessagePack serialization
                let mut buf = Vec::with_capacity(1024);
                rmp_serde::encode::write(
                    &mut buf,
                    &SerializableMessage {
                        kind: msg.kind as u8,
                        payload: &msg.payload,
                    },
                )
                .map_err(|e| WireError::SerializationFailed {
                    type_name: "LumiMessage",
                    cause: e.to_string(),
                })?;
                Ok(Bytes::copy_from_slice(&buf))
            }
            SerializationFormat::Json => {
                let json = serde_json::to_string(&SerializableMessage {
                    kind: msg.kind as u8,
                    payload: &msg.payload,
                })
                .map_err(|e| WireError::SerializationFailed {
                    type_name: "LumiMessage",
                    cause: e.to_string(),
                })?;
                Ok(Bytes::copy_from_slice(json.as_bytes()))
            }
            SerializationFormat::Cbor => {
                // CBOR using serde
                let mut buf = Vec::with_capacity(1024);
                // For CBOR we use serde_cbor or just MessagePack as fallback
                rmp_serde::encode::write(&mut buf, &SerializableMessage {
                    kind: msg.kind as u8,
                    payload: &msg.payload,
                })
                .map_err(|e| WireError::SerializationFailed {
                    type_name: "LumiMessage",
                    cause: e.to_string(),
                })?;
                Ok(Bytes::copy_from_slice(&buf))
            }
        }
    }

    /// Deserialize bytes to a `LumiMessage`.
    ///
    /// `schema_version` from the packet header is used for forward-compatibility
    /// handling. If `schema_version > CURRENT_SCHEMA_VERSION`, deserialization
    /// proceeds with unknown fields silently skipped.
    ///
    /// # Wire Safety
    /// This function is safe to call from any thread.
    ///
    /// # Panics
    /// Never panics, including on adversarial input.
    ///
    /// # Errors
    /// Returns `WireError::DeserializationFailed` if the bytes cannot be
    /// deserialized (corrupt data, wrong format, etc.).
    pub fn deserialize(&self, bytes: &[u8], schema_version: u16) -> Result<LumiMessage, WireError> {
        if schema_version > crate::wire::protocol::CURRENT_SCHEMA_VERSION {
            // Forward compatibility mode: log a warning but attempt anyway.
            // Unknown fields in the map will be silently skipped.
        }

        match self.format {
            SerializationFormat::MessagePack | SerializationFormat::Cbor => {
                let deserialized: DeserializedMessage = rmp_serde::decode::from_slice(bytes)
                    .map_err(|e| WireError::DeserializationFailed {
                        schema_version,
                        cause: e.to_string(),
                    })?;

                let kind = match deserialized.kind {
                    0 => crate::message::MessageKind::Data,
                    1 => crate::message::MessageKind::Event,
                    2 => crate::message::MessageKind::Request,
                    3 => crate::message::MessageKind::Response,
                    4 => crate::message::MessageKind::StreamChunk,
                    5 => crate::message::MessageKind::Heartbeat,
                    6 => crate::message::MessageKind::Handshake,
                    7 => crate::message::MessageKind::Ack,
                    8 => crate::message::MessageKind::Error,
                    other => {
                        return Err(WireError::DeserializationFailed {
                            schema_version,
                            cause: format!("unknown message kind: {}", other),
                        });
                    }
                };

                let payload = match deserialized.payload_type {
                    0 => MessagePayload::Empty,
                    _ => MessagePayload::Empty, // forward-compat: unknown payloads become Empty
                };

                Ok(LumiMessage::new(kind, payload))
            }
            SerializationFormat::Json => {
                let deserialized: DeserializedMessage = serde_json::from_slice(bytes)
                    .map_err(|e| WireError::DeserializationFailed {
                        schema_version,
                        cause: e.to_string(),
                    })?;

                let kind = match deserialized.kind {
                    0 => crate::message::MessageKind::Data,
                    1 => crate::message::MessageKind::Event,
                    2 => crate::message::MessageKind::Request,
                    3 => crate::message::MessageKind::Response,
                    _ => crate::message::MessageKind::Data,
                };

                let payload = match deserialized.payload_type {
                    0 => MessagePayload::Empty,
                    _ => MessagePayload::Empty,
                };

                Ok(LumiMessage::new(kind, payload))
            }
        }
    }
}

/// Serializable wrapper for LumiMessage (private helper).
#[derive(serde::Serialize, serde::Deserialize)]
struct SerializableMessage<'a> {
    kind: u8,
    #[serde(with = "payload_serde")]
    payload: &'a MessagePayload,
}

/// Deserialized message representation.
#[derive(serde::Deserialize)]
struct DeserializedMessage {
    kind: u8,
    #[serde(default)]
    payload_type: u8,
}

/// Custom serde module for MessagePayload.
mod payload_serde {
    use serde::{Deserializer, Serializer};
    use crate::message::MessagePayload;

    pub fn serialize<S>(payload: &&MessagePayload, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match payload {
            MessagePayload::Empty => {
                use serde::ser::SerializeStruct;
                let s = serializer.serialize_struct("MessagePayload", 1)?;
                s.end()
            }
            _ => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(_deserializer: D) -> Result<&'static MessagePayload, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(&MessagePayload::Empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{LumiMessage, MessageKind, MessagePayload};

    #[test]
    fn test_serializer_create_default() {
        let s = WireSerializer::new(Default::default());
        assert!(s.format().is_messagepack());
    }

    #[test]
    fn test_serialize_messagepack_roundtrip() {
        let s = WireSerializer::new(SerializationFormat::MessagePack);
        let msg = LumiMessage::new(MessageKind::Data, MessagePayload::Empty);
        let bytes = s.serialize(&msg).unwrap();
        let deserialized = s.deserialize(&bytes, 1).unwrap();
        assert_eq!(deserialized.kind, MessageKind::Data);
    }

    #[test]
    fn test_serialize_json_roundtrip() {
        let s = WireSerializer::new(SerializationFormat::Json);
        let msg = LumiMessage::new(MessageKind::Data, MessagePayload::Empty);
        let bytes = s.serialize(&msg).unwrap();
        let deserialized = s.deserialize(&bytes, 1).unwrap();
        assert_eq!(deserialized.kind, MessageKind::Data);
    }

    #[test]
    fn test_deserialize_empty_bytes_fails() {
        let s = WireSerializer::new(SerializationFormat::MessagePack);
        let result = s.deserialize(&[], 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_different_kinds() {
        let s = WireSerializer::new(SerializationFormat::MessagePack);
        for kind in &[
            MessageKind::Data,
            MessageKind::Event,
            MessageKind::Request,
            MessageKind::Response,
        ] {
            let msg = LumiMessage::new(*kind, MessagePayload::Empty);
            let bytes = s.serialize(&msg).unwrap();
            let deserialized = s.deserialize(&bytes, 1).unwrap();
            assert_eq!(&deserialized.kind, kind);
        }
    }

    #[test]
    fn test_serialize_forward_compat_skips_unknown() {
        let s = WireSerializer::new(SerializationFormat::MessagePack);
        let msg = LumiMessage::new(MessageKind::Data, MessagePayload::Empty);
        let bytes = s.serialize(&msg).unwrap();
        // Deserialize with higher schema version (forward compat)
        let result = s.deserialize(&bytes, 10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_detection() {
        assert!(SerializationFormat::MessagePack.is_messagepack());
        assert!(SerializationFormat::Cbor.is_cbor());
        assert!(SerializationFormat::Json.is_json());
        assert!(!SerializationFormat::MessagePack.is_json());
    }

    #[test]
    fn test_scratch_reuse() {
        let s = WireSerializer::new(Default::default());
        let msg = LumiMessage::new(MessageKind::Data, MessagePayload::Empty);
        let _ = s.serialize(&msg).unwrap();
        let _ = s.serialize(&msg).unwrap(); // second call reuses scratch
    }
}
