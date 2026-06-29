//! # Tokio Codec Implementation
//!
//! Implements `tokio_util::codec::Decoder` and `Encoder` for use with
//! `FramedRead`/`FramedWrite`, eliminating manual framing state machines.
//!
//! This is the primary interface for Tier 2 (socket) transport.

use crate::error::CodecError;
use crate::message::LumiMessage;
use crate::wire::{WireCodec, HEADER_SIZE};
use bytes::{BytesMut, BufMut};
use tokio_util::codec::{Decoder, Encoder};

/// Framed codec for Lumi wire protocol.
///
/// Wraps `WireCodec` for use with `tokio_util::codec::Framed`.
/// Implements length-delimited framing with magic byte validation.
pub struct LumiFramer {
    codec: WireCodec,
    max_frame_size: usize,
}

impl LumiFramer {
    /// Create a new LumiFramer with the given wire codec.
    pub fn new(codec: WireCodec) -> Self {
        Self {
            max_frame_size: (crate::wire::HEADER_SIZE + crate::wire::MAX_FRAME_PAYLOAD_BYTES as usize) as usize,
            codec,
        }
    }

    /// Create a LumiFramer with default settings.
    pub fn default() -> Self {
        Self::new(WireCodec::new())
    }
}

impl Decoder for LumiFramer {
    type Item = LumiMessage;
    type Error = CodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Need at least HEADER_SIZE bytes to read the length
        if src.len() < HEADER_SIZE {
            return Ok(None);
        }

        // Peek at the length field (bytes 8-12)
        let payload_len = u32::from_le_bytes(
            src[8..12].try_into().unwrap()
        ) as usize;

        let frame_size = HEADER_SIZE + payload_len;

        // Check if frame exceeds maximum size
        if frame_size > self.max_frame_size {
            return Err(CodecError::FrameTooLarge {
                size: frame_size,
                max: self.max_frame_size,
            });
        }

        // Wait for full frame
        if src.len() < frame_size {
            // Reserve more space
            src.reserve(frame_size - src.len());
            return Ok(None);
        }

        // Extract the frame
        let frame = src.split_to(frame_size);

        // Decode using WireCodec
        let msg = self.codec.decode(&frame)?;
        Ok(Some(msg))
    }
}

impl Encoder<LumiMessage> for LumiFramer {
    type Error = CodecError;

    fn encode(&mut self, item: LumiMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Encode using WireCodec
        let encoded = self.codec.encode(&item)?;

        // Write to output buffer
        dst.put_slice(&encoded);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{LumiMessage, MessagePayload, ProcessId};
    use bytes::BytesMut;

    #[test]
    fn test_encode_decode_roundtrip() {
        let mut framer = LumiFramer::default();

        let msg = LumiMessage::new_event(
            ProcessId::Core,
            "test.channel",
            MessagePayload::Empty,
        );

        // Encode
        let mut buf = BytesMut::new();
        framer.encode(msg.clone(), &mut buf).unwrap();

        // Decode
        let decoded = framer.decode(&mut buf).unwrap().unwrap();

        assert_eq!(msg.id, decoded.id);
        assert_eq!(msg.sender, decoded.sender);
        assert_eq!(msg.channel.0, decoded.channel.0);
    }

    #[test]
    fn test_decode_incomplete_header() {
        let mut framer = LumiFramer::default();
        let mut buf = BytesMut::new();

        // Write partial header (only 4 bytes)
        buf.extend_from_slice(b"LUM");
        let result = framer.decode(&mut buf).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_incomplete_payload() {
        let mut framer = LumiFramer::default();
        let mut buf = BytesMut::new();

        // Write header saying payload is 100 bytes but only write 50
        buf.extend_from_slice(&crate::wire::WIRE_MAGIC_BYTES);
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&100u32.to_le_bytes());
        buf.extend_from_slice(&vec![0u8; 50]);

        let result = framer.decode(&mut buf).unwrap();
        assert!(result.is_none());
    }
}
