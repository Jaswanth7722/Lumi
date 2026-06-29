// ── Packet Types and Zero-Copy Parser ─────────────────────────────────────────
// Packet types for the wire protocol. ParsedPacket borrows from the input buffer
// (zero-copy). PacketBuilder constructs wire bytes from components.

use bytes::{Bytes, BytesMut};
use uuid::Uuid;

use crate::wire::checksum::ChecksumEngine;
use crate::wire::error::WireError;
use crate::wire::header::{Flags, Header, HeaderBuilder};
use crate::wire::protocol::*;

/// A fully parsed and checksum-verified packet.
///
/// The `payload` field borrows directly from the original frame buffer — no copy
/// is performed. This is safe because `ParsedPacket<'buf>` cannot outlive the
/// buffer it borrows from (enforced by the borrow checker).
///
/// # Zero-Copy Guarantee
/// The `payload` field is `&'buf [u8]`, borrowing from the input buffer. The
/// caller decides whether to copy into owned storage. For shared memory
/// transport, the payload can remain in-place in the ring buffer during
/// deserialization.
#[derive(Debug)]
pub struct ParsedPacket<'buf> {
    pub header: Header,
    pub payload: &'buf [u8],
}

impl<'buf> ParsedPacket<'buf> {
    /// Parse a packet from a raw frame buffer.
    ///
    /// Steps:
    /// 1. Parse the header from the first `HEADER_V1_SIZE` bytes
    /// 2. Extract a zero-copy payload reference from the remaining bytes
    /// 3. Return the parsed packet
    ///
    /// Does NOT verify the checksum — call `verify_checksum` separately.
    ///
    /// # Wire Safety
    /// This function is safe to call from any thread.
    ///
    /// # Panics
    /// Never panics, including on adversarial input.
    ///
    /// # Errors
    /// Returns:
    /// - `WireError::TruncatedHeader` if the buffer is too short
    pub fn parse(frame: &'buf [u8]) -> Result<Self, WireError> {
        let header = Header::parse(frame)?;

        let payload_start = HEADER_V1_SIZE;
        let payload_end = header.total_length as usize;

        if frame.len() < payload_end {
            return Err(WireError::TruncatedPayload {
                available: frame.len().saturating_sub(payload_start),
                needed: payload_end - payload_start,
            });
        }

        let payload = &frame[payload_start..payload_end];

        Ok(ParsedPacket { header, payload })
    }

    /// Verify the checksum on this packet.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `WireError::ChecksumMismatch` if the checksum doesn't match.
    pub fn verify_checksum(&self, raw_header: &[u8]) -> Result<(), WireError> {
        self.header.verify_checksum(raw_header, self.payload)
    }

    /// Convert to an owned representation (copies the payload).
    pub fn into_owned(self) -> OwnedPacket {
        OwnedPacket {
            header: self.header,
            payload: Bytes::copy_from_slice(self.payload),
        }
    }
}

/// An owned packet with a copied payload buffer.
///
/// Use this when the packet needs to outlive the original frame buffer.
#[derive(Debug, Clone)]
pub struct OwnedPacket {
    pub header: Header,
    pub payload: Bytes,
}

impl OwnedPacket {
    /// Create a new owned packet from components.
    pub fn new(header: Header, payload: Bytes) -> Self {
        Self { header, payload }
    }

    /// Get the total wire size (header + payload).
    pub fn wire_size(&self) -> usize {
        HEADER_V1_SIZE + self.payload.len()
    }
}

/// Builder for constructing wire packets from components.
///
/// Handles header construction, checksum computation, and frame assembly.
///
/// # Example
/// ```rust,ignore
/// let packet = PacketBuilder::new(msg_id, kind, sender, receiver)
///     .with_correlation(corr_id)
///     .with_priority(2)
///     .build(payload_bytes)?;
/// ```
#[derive(Debug)]
pub struct PacketBuilder {
    header: HeaderBuilder,
}

impl PacketBuilder {
    /// Create a new packet builder with required fields.
    ///
    /// Required parameters:
    /// - `msg_id`: UUID v7 message identifier
    /// - `kind`: MessageKind discriminant
    /// - `sender_id`: Sending process ID
    /// - `receiver_id`: Receiving process ID (or `BROADCAST_RECEIVER`)
    pub fn new(msg_id: Uuid, kind: u8, sender_id: u64, receiver_id: u64) -> Self {
        Self {
            header: HeaderBuilder::new(msg_id, kind, sender_id, receiver_id),
        }
    }

    /// Set the correlation ID for request-response matching.
    pub fn with_correlation(mut self, id: Uuid) -> Self {
        self.header = self.header.with_correlation(id);
        self
    }

    /// Set the session ID.
    pub fn with_session(mut self, id: u64) -> Self {
        self.header = self.header.with_session(id);
        self
    }

    /// Set the message priority (0-3).
    pub fn with_priority(mut self, p: u8) -> Self {
        self.header = self.header.with_priority(p);
        self
    }

    /// Set the schema version for MessagePack serialization.
    pub fn with_schema_version(mut self, v: u16) -> Self {
        self.header = self.header.with_schema_version(v);
        self
    }

    /// Set the compression type.
    pub fn with_compression(mut self, c: u8) -> Self {
        self.header = self.header.with_compression(c);
        self
    }

    /// Set the encryption type.
    pub fn with_encryption(mut self, e: u8) -> Self {
        self.header = self.header.with_encryption(e);
        self
    }

    /// Set fragment metadata.
    pub fn with_fragment(mut self, index: u16, total: u16, frag_id: u16) -> Self {
        self.header = self.header.with_fragment(index, total, frag_id);
        self
    }

    /// Set the compressed flag.
    pub fn with_compressed_flag(mut self) -> Self {
        self.header = self.header.with_compressed_flag();
        self
    }

    /// Set the encrypted flag.
    pub fn with_encrypted_flag(mut self) -> Self {
        self.header = self.header.with_encrypted_flag();
        self
    }

    /// Set the fragmented flag.
    pub fn with_fragmented_flag(mut self) -> Self {
        self.header = self.header.with_fragmented_flag();
        self
    }

    /// Set the stream flag.
    pub fn with_stream_flag(mut self) -> Self {
        self.header = self.header.with_stream_flag();
        self
    }

    /// Set the requires-ack flag.
    pub fn with_requires_ack(mut self) -> Self {
        self.header = self.header.with_requires_ack();
        self
    }

    /// Finalize: compute checksum and produce wire bytes.
    ///
    /// `payload` is the already-compressed-and-encrypted payload bytes.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `WireError::OversizedFrame` if the total frame exceeds limits.
    pub fn build(self, payload: &[u8]) -> Result<Bytes, WireError> {
        let total_length = HEADER_V1_SIZE + payload.len();
        if total_length > MAX_FRAME_SIZE {
            return Err(WireError::OversizedFrame {
                size: total_length as u32,
                limit: MAX_FRAME_SIZE as u32,
            });
        }

        let header = self.header.build(total_length as u32, payload.len() as u32);

        let mut buf = BytesMut::with_capacity(total_length);
        buf.resize(HEADER_V1_SIZE, 0);
        header.write(&mut buf[..HEADER_V1_SIZE])?;

        // Compute and write checksum
        let checksum = ChecksumEngine::compute(&buf[..92], payload);
        buf[OFFSET_CHECKSUM..OFFSET_CHECKSUM + 4].copy_from_slice(&checksum.to_le_bytes());

        // Append payload
        buf.extend_from_slice(payload);

        Ok(buf.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_packet() {
        let mut raw = vec![0u8; HEADER_V1_SIZE + 16];
        raw[0..4].copy_from_slice(&WIRE_MAGIC.to_be_bytes());
        raw[4] = WIRE_VERSION_MAJOR;
        raw[5] = 1;
        raw[8..12].copy_from_slice(&((HEADER_V1_SIZE + 16) as u32).to_le_bytes());
        raw[12..16].copy_from_slice(&16u32.to_le_bytes());
        raw[16..32].copy_from_slice(Uuid::new_v4().as_bytes());
        raw[32..48].copy_from_slice(Uuid::new_v4().as_bytes());
        raw[48..56].copy_from_slice(&1u64.to_le_bytes());
        raw[56..64].copy_from_slice(&42u64.to_le_bytes());
        raw[64..72].copy_from_slice(&100u64.to_le_bytes());
        raw[72..80].copy_from_slice(&1_000_000u64.to_le_bytes());
        raw[80] = 1;
        // Add payload
        raw[HEADER_V1_SIZE..].copy_from_slice(b"Hello, Lumi!");

        let packet = ParsedPacket::parse(&raw).unwrap();
        assert_eq!(packet.header.sender_id, 42);
        assert_eq!(packet.header.receiver_id, 100);
        assert_eq!(packet.payload, b"Hello, Lumi!");
    }

    #[test]
    fn test_parse_truncated() {
        let raw = vec![0u8; HEADER_V1_SIZE]; // no payload but header claims payload
        let mut raw = raw;
        raw[0..4].copy_from_slice(&WIRE_MAGIC.to_be_bytes());
        raw[8..12].copy_from_slice(&((HEADER_V1_SIZE + 10) as u32).to_le_bytes());
        raw[12..16].copy_from_slice(&10u32.to_le_bytes());
        let err = ParsedPacket::parse(&raw).unwrap_err();
        assert!(matches!(err, WireError::TruncatedPayload { .. }));
    }

    #[test]
    fn test_packet_builder_roundtrip() {
        let msg_id = Uuid::new_v4();
        let payload = b"Hello, Lumi Wire Protocol!";

        let packet = PacketBuilder::new(msg_id, 1, 42, 100)
            .with_schema_version(1)
            .build(payload)
            .unwrap();

        // Parse back
        let parsed = ParsedPacket::parse(&packet).unwrap();
        assert_eq!(parsed.header.message_id, msg_id);
        assert_eq!(parsed.header.sender_id, 42);
        assert_eq!(parsed.header.receiver_id, 100);
        assert_eq!(parsed.header.total_length as usize, HEADER_V1_SIZE + payload.len());
        assert_eq!(parsed.payload, payload);
    }

    #[test]
    fn test_packet_builder_with_all_options() {
        let msg_id = Uuid::new_v4();
        let corr_id = Uuid::new_v4();
        let payload = b"test";

        let packet = PacketBuilder::new(msg_id, 2, 1, BROADCAST_RECEIVER)
            .with_correlation(corr_id)
            .with_session(99)
            .with_priority(3)
            .with_schema_version(2)
            .with_compression(0)
            .with_encryption(0)
            .with_requires_ack()
            .build(payload)
            .unwrap();

        let parsed = ParsedPacket::parse(&packet).unwrap();
        assert_eq!(parsed.header.correlation_id, corr_id);
        assert_eq!(parsed.header.session_id, 99);
        assert_eq!(parsed.header.priority, 3);
        assert_eq!(parsed.header.schema_version, 2);
        assert_eq!(parsed.header.receiver_id, BROADCAST_RECEIVER);
    }

    #[test]
    fn test_build_oversized_rejected() {
        let msg_id = Uuid::new_v4();
        let huge_payload = vec![0u8; MAX_FRAME_SIZE]; // header + payload > limit
        let result = PacketBuilder::new(msg_id, 1, 0, 0).build(&huge_payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_owned_packet_from_parsed() {
        // Build a valid packet, parse it, convert to owned
        let msg_id = Uuid::new_v4();
        let payload = b"test payload";
        let packet = PacketBuilder::new(msg_id, 1, 42, 100)
            .build(payload)
            .unwrap();
        let parsed = ParsedPacket::parse(&packet).unwrap();
        let owned = parsed.into_owned();
        assert_eq!(owned.payload.as_ref(), payload);
        assert_eq!(owned.header.message_id, msg_id);
        assert_eq!(owned.header.sender_id, 42);
    }

    #[test]
    fn test_owned_packet_wire_size() {
        let header = crate::wire::header::HeaderBuilder::new(Uuid::new_v4(), 1, 0, 0)
            .build(104, 32);
        let owned = OwnedPacket::new(header, Bytes::from(&b"Hello!"[..]));
        assert_eq!(owned.wire_size(), HEADER_V1_SIZE + 6);
    }

    #[test]
    fn test_packet_builder_with_fragment() {
        let msg_id = Uuid::new_v4();
        let payload = b"fragment data";

        let packet = PacketBuilder::new(msg_id, 1, 42, 100)
            .with_fragment(0, 3, 555)
            .with_fragmented_flag()
            .build(payload)
            .unwrap();

        let parsed = ParsedPacket::parse(&packet).unwrap();
        assert_eq!(parsed.header.fragment_index, 0);
        assert_eq!(parsed.header.fragment_total, 3);
        assert_eq!(parsed.header.fragment_id, 555);
        assert!(parsed.header.flags.is_fragmented());
    }
}
