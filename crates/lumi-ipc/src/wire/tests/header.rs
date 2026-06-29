// ── Header Tests ──────────────────────────────────────────────────────────────
// Tests the Header struct: parsing from raw bytes, writing to bytes,
// roundtrip verification, truncated/bad inputs, and checksum verification.
#![cfg(test)]

use std::collections::HashMap;

use uuid::Uuid;

use crate::wire::checksum::ChecksumEngine;
use crate::wire::error::WireError;
use crate::wire::header::{Flags, Header, HeaderBuilder};
use crate::wire::protocol::*;

/// Build a minimal valid header in raw bytes for testing.
fn build_valid_header_bytes() -> Vec<u8> {
    let mut buf = vec![0u8; HEADER_V1_SIZE];
    // Magic
    buf[0..4].copy_from_slice(&WIRE_MAGIC.to_be_bytes());
    // Wire version
    buf[4] = WIRE_VERSION_MAJOR;
    // Header version
    buf[5] = 1;
    // Flags
    buf[6..8].copy_from_slice(&0u16.to_le_bytes());
    // Total length
    buf[8..12].copy_from_slice(&(HEADER_V1_SIZE as u32).to_le_bytes());
    // Payload length
    buf[12..16].copy_from_slice(&0u32.to_le_bytes());
    // Message ID (UUID v7)
    let msg_id = Uuid::from_u128(0x12345678_1234_7123_8000_000000000000);
    buf[16..32].copy_from_slice(msg_id.as_bytes());
    // Correlation ID
    let corr_id = Uuid::from_u128(0x87654321_4321_7876_8000_000000000000);
    buf[32..48].copy_from_slice(corr_id.as_bytes());
    // Session ID
    buf[48..56].copy_from_slice(&1u64.to_le_bytes());
    // Sender ID
    buf[56..64].copy_from_slice(&42u64.to_le_bytes());
    // Receiver ID (broadcast)
    buf[64..72].copy_from_slice(&BROADCAST_RECEIVER.to_le_bytes());
    // Timestamp
    buf[72..80].copy_from_slice(&1_000_000u64.to_le_bytes());
    // Message kind
    buf[80] = 1;
    // Priority
    buf[81] = 0;
    // Compression type
    buf[82] = 0;
    // Encryption type
    buf[83] = 0;
    // Schema version
    buf[84..86].copy_from_slice(&1u16.to_le_bytes());
    // Fragment index
    buf[86..88].copy_from_slice(&0u16.to_le_bytes());
    // Fragment total
    buf[88..90].copy_from_slice(&1u16.to_le_bytes());
    // Fragment ID
    buf[90..92].copy_from_slice(&0u16.to_le_bytes());
    // Checksum (placeholder)
    buf[92..96].copy_from_slice(&0u32.to_le_bytes());
    // Reserved
    buf[96..104].copy_from_slice(&0u64.to_le_bytes());
    buf
}

#[test]
fn test_parse_valid_header() {
    let bytes = build_valid_header_bytes();
    let header = Header::parse(&bytes).unwrap();
    assert_eq!(header.wire_version, WIRE_VERSION_MAJOR);
    assert_eq!(header.header_version, 1);
    assert_eq!(header.flags.0, 0);
    assert_eq!(header.total_length, HEADER_V1_SIZE as u32);
    assert_eq!(header.payload_length, 0);
    assert_eq!(header.sender_id, 42);
    assert_eq!(header.receiver_id, BROADCAST_RECEIVER);
    assert_eq!(header.fragment_total, 1);
    assert_eq!(header.fragment_index, 0);
}

#[test]
fn test_parse_truncated_header() {
    let bytes = vec![0u8; 10];
    let err = Header::parse(&bytes).unwrap_err();
    assert!(matches!(err, WireError::TruncatedFrame { .. }));
}

#[test]
fn test_parse_bad_magic() {
    let mut bytes = build_valid_header_bytes();
    bytes[0] = 0x00; // corrupt magic
    let err = Header::parse(&bytes).unwrap_err();
    assert!(matches!(err, WireError::InvalidMagic { .. }));
}

#[test]
fn test_parse_unsupported_wire_version() {
    let mut bytes = build_valid_header_bytes();
    bytes[4] = 99; // unsupported version
    let err = Header::parse(&bytes).unwrap_err();
    assert!(matches!(err, WireError::UnsupportedVersion { .. }));
}

#[test]
fn test_parse_unsupported_header_version() {
    let mut bytes = build_valid_header_bytes();
    bytes[5] = 99; // unsupported header version
    let err = Header::parse(&bytes).unwrap_err();
    assert!(matches!(err, WireError::UnsupportedVersion { .. }));
}

#[test]
fn test_header_write_roundtrip() {
    let original_bytes = build_valid_header_bytes();
    let header = Header::parse(&original_bytes).unwrap();

    let mut written = vec![0u8; HEADER_V1_SIZE];
    header.write(&mut written).unwrap();

    // Written bytes should match original (except checksum is zeroed)
    assert_eq!(written.len(), original_bytes.len());
    assert_eq!(written[0..92], original_bytes[0..92]);
    // Checksum field is zero in written (caller fills it later)
    assert_eq!(&written[92..96], &[0u8; 4]);
    // Reserved is preserved
    assert_eq!(written[96..104], original_bytes[96..104]);
}

#[test]
fn test_parse_with_flags() {
    let mut bytes = build_valid_header_bytes();
    let flags: u16 = FLAG_COMPRESSED | FLAG_ENCRYPTED;
    bytes[6..8].copy_from_slice(&flags.to_le_bytes());

    let header = Header::parse(&bytes).unwrap();
    assert!(header.flags.is_compressed());
    assert!(header.flags.is_encrypted());
    assert!(!header.flags.is_stream());
}

#[test]
fn test_header_builder() {
    let msg_id = Uuid::new_v4();
    let corr_id = Uuid::new_v4();
    let header = HeaderBuilder::new(msg_id, 1, 42, 100)
        .with_correlation(corr_id)
        .with_session(7)
        .build(256, 100);

    assert_eq!(header.message_id, msg_id);
    assert_eq!(header.correlation_id, corr_id);
    assert_eq!(header.sender_id, 42);
    assert_eq!(header.receiver_id, 100);
    assert_eq!(header.session_id, 7);
    assert_eq!(header.total_length, 256);
    assert_eq!(header.payload_length, 100);
    assert_eq!(header.wire_version, WIRE_VERSION_MAJOR);
    assert_eq!(header.header_version, 1);
    assert_eq!(header.fragment_total, 1);
}

#[test]
fn test_flags_builder_methods() {
    let flags = Flags(0)
        .with_compressed()
        .with_encrypted();
    assert!(flags.is_compressed());
    assert!(flags.is_encrypted());
    assert!(!flags.is_fragmented());

    let flags = Flags(0)
        .with_fragmented()
        .with_stream();
    assert!(flags.is_fragmented());
    assert!(flags.is_stream());
}

#[test]
fn test_reserved_bits_check() {
    let flags = Flags(0);
    assert!(flags.reserved_bits_clear());

    let flags = Flags(0x0100); // set a reserved bit
    assert!(!flags.reserved_bits_clear());
}

#[test]
fn test_parse_with_priority() {
    let mut bytes = build_valid_header_bytes();
    bytes[81] = 3; // Critical priority

    let header = Header::parse(&bytes).unwrap();
    assert_eq!(header.priority, 3);
}

#[test]
fn test_verify_checksum_on_header() {
    let bytes = build_valid_header_bytes();
    let header = Header::parse(&bytes).unwrap();

    // Empty payload checksum
    let result = header.verify_checksum(&bytes[..92], &[]);
    assert!(result.is_err()); // checksum in header is 0, but computed won't match
}

#[test]
fn test_write_too_small_buffer() {
    let header = HeaderBuilder::new(Uuid::new_v4(), 1, 0, 0).build(HEADER_V1_SIZE as u32, 0);
    let mut buf = vec![0u8; 4];
    let err = header.write(&mut buf).unwrap_err();
    assert!(matches!(err, WireError::TruncatedFrame { .. }));
}
