//! # Wire Format and Framing
//!
//! Binary wire format for Tier 2 transport (Unix socket / named pipe):
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        LUMI WIRE FRAME                         │
//! ├──────────┬──────────┬─────────┬──────────┬─────────────────────┤
//! │  Magic   │ Version  │  Flags  │  Length  │      Payload        │
//! │  4 bytes │  2 bytes │ 2 bytes │  4 bytes │    N bytes          │
//! ├──────────┴──────────┴─────────┴──────────┴─────────────────────┤
//! │ Magic:   0x4C554D49  ("LUMI" in ASCII)                        │
//! │ Version: Wire protocol version (not message schema version)    │
//! │ Flags:   bit 0 = compressed, bit 1 = encrypted, bits 2-15 = 0 │
//! │ Length:  Payload length in bytes, max 256KB enforced           │
//! │ Payload: MessagePack-serialized LumiMessage                    │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use crate::error::CodecError;
use crate::message::LumiMessage;
use bytes::{Bytes, BytesMut};

/// Wire protocol magic: "LUMI" in ASCII.
pub const WIRE_MAGIC: u32 = 0x4C554D49;
pub const WIRE_MAGIC_BYTES: [u8; 4] = [0x4C, 0x55, 0x4D, 0x49];

/// Maximum frame payload size: 256KB.
pub const MAX_FRAME_PAYLOAD_BYTES: u32 = 256 * 1024;

/// Wire protocol header size: 12 bytes.
pub const HEADER_SIZE: usize = 12;

/// Wire frame flags.
pub struct WireFlags;

impl WireFlags {
    pub const COMPRESSED: u16 = 0x0001;
    pub const ENCRYPTED: u16 = 0x0002;
}

/// A parsed wire frame with header metadata and payload.
#[derive(Debug, Clone)]
pub struct WireFrame {
    pub version: u16,
    pub flags: u16,
    pub payload: Bytes,
}

impl WireFrame {
    /// Create a new wire frame.
    pub fn new(version: u16, flags: u16, payload: Bytes) -> Self {
        Self { version, flags, payload }
    }

    /// Check if the frame is compressed.
    pub fn is_compressed(&self) -> bool {
        self.flags & WireFlags::COMPRESSED != 0
    }

    /// Check if the frame is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.flags & WireFlags::ENCRYPTED != 0
    }
}

// ---------------------------------------------------------------------------
// Wire codec
// ---------------------------------------------------------------------------

/// Wire protocol codec that handles encoding/decoding framed messages.
#[derive(Debug, Clone)]
pub struct WireCodec {
    compression: bool,
    encryption: bool,
}

impl WireCodec {
    /// Create a new wire codec.
    pub fn new() -> Self {
        Self {
            compression: false,
            encryption: false,
        }
    }

    /// Create a wire codec with compression and encryption support.
    pub fn with_features(compression: bool, encryption: bool) -> Self {
        Self { compression, encryption }
    }

    /// Encode a LumiMessage into a complete wire frame.
    pub fn encode(&self, msg: &LumiMessage) -> Result<Bytes, CodecError> {
        // Serialize the message to MessagePack
        let payload = rmp_serde::to_vec(msg)
            .map_err(|e| CodecError::Serialization(e.to_string()))?;

        let payload_len = payload.len() as u32;
        if payload_len > MAX_FRAME_PAYLOAD_BYTES {
            return Err(CodecError::FrameTooLarge {
                size: payload.len(),
                max: MAX_FRAME_PAYLOAD_BYTES as usize,
            });
        }

        let mut flags: u16 = 0;

        // Compress if enabled and payload > 1KB
        #[cfg(feature = "compression")]
        if self.compression && payload_len > 1024 {
            let compressed = zstd::encode_all(&payload[..], 3)
                .map_err(|e| CodecError::CompressionError(e.to_string()))?;
            if compressed.len() < payload.len() {
                flags |= WireFlags::COMPRESSED;
                return self.build_frame(1, flags, &compressed);
            }
        }

        self.build_frame(1, flags, &payload)
    }

    /// Build a complete wire frame from header + payload bytes.
    fn build_frame(&self, version: u16, flags: u16, payload: &[u8]) -> Result<Bytes, CodecError> {
        let payload_len = payload.len() as u32;
        let mut buf = BytesMut::with_capacity(HEADER_SIZE + payload.len());

        // Magic: 4 bytes
        buf.extend_from_slice(&WIRE_MAGIC_BYTES);

        // Version: 2 bytes (little-endian)
        buf.extend_from_slice(&version.to_le_bytes());

        // Flags: 2 bytes (little-endian)
        buf.extend_from_slice(&flags.to_le_bytes());

        // Length: 4 bytes (little-endian)
        buf.extend_from_slice(&payload_len.to_le_bytes());

        // Payload
        buf.extend_from_slice(payload);

        Ok(buf.freeze())
    }

    /// Decode a wire frame from a complete byte buffer.
    /// The buffer must contain at least HEADER_SIZE bytes.
    pub fn decode(&self, frame: &[u8]) -> Result<LumiMessage, CodecError> {
        if frame.len() < HEADER_SIZE {
            return Err(CodecError::BufferUnderflow {
                needed: HEADER_SIZE,
                available: frame.len(),
            });
        }

        // Validate magic
        let magic = u32::from_le_bytes(frame[0..4].try_into().unwrap());
        if magic != WIRE_MAGIC {
            return Err(CodecError::InvalidMagic {
                expected: WIRE_MAGIC,
                got: magic,
            });
        }

        // Parse version — only version 1 is currently supported
        let version = u16::from_le_bytes(frame[4..6].try_into().unwrap());
        if version != 1 {
            return Err(CodecError::UnsupportedVersion { version });
        }

        // Parse flags and length
        let flags = u16::from_le_bytes(frame[6..8].try_into().unwrap());
        let payload_len = u32::from_le_bytes(frame[8..12].try_into().unwrap()) as usize;

        if payload_len > MAX_FRAME_PAYLOAD_BYTES as usize {
            return Err(CodecError::FrameTooLarge {
                size: payload_len,
                max: MAX_FRAME_PAYLOAD_BYTES as usize,
            });
        }

        if frame.len() < HEADER_SIZE + payload_len {
            return Err(CodecError::BufferUnderflow {
                needed: HEADER_SIZE + payload_len,
                available: frame.len(),
            });
        }

        // Read payload bytes
        let payload = &frame[HEADER_SIZE..HEADER_SIZE + payload_len];

        // Decompress if flagged
        let data = if flags & WireFlags::COMPRESSED != 0 {
            #[cfg(feature = "compression")]
            {
                zstd::decode_all(payload)
                    .map_err(|e| CodecError::CompressionError(e.to_string()))?
            }
            #[cfg(not(feature = "compression"))]
            {
                return Err(CodecError::CompressionError(
                    "compression feature not enabled".into(),
                ));
            }
        } else {
            payload.to_vec()
        };

        // Decrypt if flagged (not yet implemented)
        #[allow(unused_variables)]
        let decrypted = if flags & WireFlags::ENCRYPTED != 0 {
            #[cfg(feature = "encryption")]
            {
                return Err(CodecError::EncryptionError(
                    "encryption not yet implemented".into(),
                ));
            }
            #[cfg(not(feature = "encryption"))]
            {
                return Err(CodecError::EncryptionError(
                    "encryption feature not enabled".into(),
                ));
            }
        } else {
            data.clone()
        };

        // Deserialize
        rmp_serde::from_slice(&data)
            .map_err(|e| CodecError::Deserialization(e.to_string()))
    }

    /// Get the frame size from a header without decoding the full frame.
    pub fn frame_size(header: &[u8]) -> Result<usize, CodecError> {
        if header.len() < HEADER_SIZE {
            return Err(CodecError::BufferUnderflow {
                needed: HEADER_SIZE,
                available: header.len(),
            });
        }

        let payload_len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        Ok(HEADER_SIZE + payload_len)
    }
}

impl Default for WireCodec {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_bytes() {
        assert_eq!(WIRE_MAGIC, 0x4C554D49);
        assert_eq!(&WIRE_MAGIC_BYTES, b"LUMI");
    }

    #[test]
    fn test_header_size_constant() {
        assert_eq!(HEADER_SIZE, 12);
    }

    #[test]
    fn test_frame_roundtrip() {
        let codec = WireCodec::new();
        let msg = LumiMessage::new_event(
            crate::message::ProcessId::Core,
            "test.channel",
            crate::message::MessagePayload::Empty,
        );

        let encoded = codec.encode(&msg).unwrap();
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(msg.id, decoded.id);
        assert_eq!(msg.sender, decoded.sender);
        assert_eq!(msg.channel.0, decoded.channel.0);
    }

    #[test]
    fn test_invalid_magic_rejected() {
        let codec = WireCodec::new();
        let bad_bytes = vec![0x00u8; HEADER_SIZE + 10];
        let result = codec.decode(&bad_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_frame_too_large_rejected() {
        let codec = WireCodec::new();
        // Build a header claiming payload is MAX + 1 bytes
        let mut header = vec![0u8; HEADER_SIZE];
        header[0..4].copy_from_slice(&WIRE_MAGIC_BYTES);
        header[4..6].copy_from_slice(&1u16.to_le_bytes());
        header[6..8].copy_from_slice(&0u16.to_le_bytes());
        header[8..12].copy_from_slice(&(MAX_FRAME_PAYLOAD_BYTES + 1).to_le_bytes());

        let result = codec.decode(&header);
        assert!(result.is_err());
    }

    #[test]
    fn test_max_payload_size() {
        assert_eq!(MAX_FRAME_PAYLOAD_BYTES, 262144);
    }
}
