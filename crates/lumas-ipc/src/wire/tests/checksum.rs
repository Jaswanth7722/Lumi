// ── Checksum Tests ─────────────────────────────────────────────────────────────
// Tests the ChecksumEngine: correct checksum on known input, single-bit-flip
// detection, all-zero payload, large payload consistency, verify roundtrip.
#![cfg(test)]

use crate::wire::checksum::ChecksumEngine;
use crate::wire::error::WireError;
use crate::wire::header::Header;
use crate::wire::protocol::*;

/// Create a minimal header for checksum testing.
fn make_test_header(header_bytes: &[u8]) -> Header {
    Header {
        wire_version: header_bytes[4],
        header_version: header_bytes[5],
        flags: crate::wire::header::Flags(u16::from_le_bytes([
            header_bytes[6],
            header_bytes[7],
        ])),
        total_length: u32::from_le_bytes(header_bytes[8..12].try_into().unwrap()),
        payload_length: u32::from_le_bytes(header_bytes[12..16].try_into().unwrap()),
        message_id: uuid::Uuid::from_bytes(header_bytes[16..32].try_into().unwrap()),
        correlation_id: uuid::Uuid::from_bytes(header_bytes[32..48].try_into().unwrap()),
        session_id: u64::from_le_bytes(header_bytes[48..56].try_into().unwrap()),
        sender_id: u64::from_le_bytes(header_bytes[56..64].try_into().unwrap()),
        receiver_id: u64::from_le_bytes(header_bytes[64..72].try_into().unwrap()),
        timestamp_us: u64::from_le_bytes(header_bytes[72..80].try_into().unwrap()),
        message_kind: header_bytes[80],
        priority: header_bytes[81],
        compression_type: header_bytes[82],
        encryption_type: header_bytes[83],
        schema_version: u16::from_le_bytes(header_bytes[84..86].try_into().unwrap()),
        fragment_index: u16::from_le_bytes(header_bytes[86..88].try_into().unwrap()),
        fragment_total: u16::from_le_bytes(header_bytes[88..90].try_into().unwrap()),
        fragment_id: u16::from_le_bytes(header_bytes[90..92].try_into().unwrap()),
        checksum: u32::from_le_bytes(header_bytes[92..96].try_into().unwrap()),
    }
}

/// Build a complete header bytes buffer with checksum populated.
fn build_checksummed_header() -> (Vec<u8>, Header) {
    let mut buf = vec![0u8; HEADER_V1_SIZE];
    buf[0..4].copy_from_slice(&WIRE_MAGIC.to_be_bytes());
    buf[4] = WIRE_VERSION_MAJOR;
    buf[5] = 1;
    buf[6..8].copy_from_slice(&0u16.to_le_bytes());
    buf[8..12].copy_from_slice(&(HEADER_V1_SIZE as u32).to_le_bytes());
    buf[12..16].copy_from_slice(&0u32.to_le_bytes());
    buf[16..32].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    buf[32..48].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    buf[48..56].copy_from_slice(&1u64.to_le_bytes());
    buf[56..64].copy_from_slice(&42u64.to_le_bytes());
    buf[64..72].copy_from_slice(&u64::MAX.to_le_bytes());
    buf[72..80].copy_from_slice(&1_000_000u64.to_le_bytes());
    buf[80] = 1;
    buf[81] = 0;
    buf[82] = 0;
    buf[83] = 0;
    buf[84..86].copy_from_slice(&1u16.to_le_bytes());
    buf[86..88].copy_from_slice(&0u16.to_le_bytes());
    buf[88..90].copy_from_slice(&1u16.to_le_bytes());
    buf[90..92].copy_from_slice(&0u16.to_le_bytes());
    // Compute checksum and fill it
    let checksum = ChecksumEngine::compute(&buf[..92], &[]);
    buf[92..96].copy_from_slice(&checksum.to_le_bytes());
    buf[96..104].copy_from_slice(&0u64.to_le_bytes());

    let header = make_test_header(&buf);
    (buf, header)
}

#[test]
fn test_checksum_known_input() {
    let (buf, header) = build_checksummed_header();
    let result = header.verify_checksum(&buf[..92], &[]);
    assert!(result.is_ok(), "Checksum should match on valid header");
}

#[test]
fn test_checksum_single_bit_flip() {
    let (mut buf, header) = build_checksummed_header();
    // Flip one bit in the header
    buf[48] ^= 0x01;
    let result = header.verify_checksum(&buf[..92], &[]);
    assert!(
        matches!(result, Err(WireError::ChecksumMismatch { .. })),
        "Single-bit flip should cause checksum mismatch"
    );
}

#[test]
fn test_checksum_all_zero_payload() {
    let (mut buf, _) = build_checksummed_header();
    let payload = [0u8; 64];
    let checksum = ChecksumEngine::compute(&buf[..92], &payload);
    buf[92..96].copy_from_slice(&checksum.to_le_bytes());
    let header = make_test_header(&buf);
    let result = header.verify_checksum(&buf[..92], &payload);
    assert!(result.is_ok(), "All-zero payload checksum should match");
}

#[test]
fn test_checksum_payload_bit_flip() {
    let (mut buf, _) = build_checksummed_header();
    let mut payload = [0xABu8; 64];
    let checksum = ChecksumEngine::compute(&buf[..92], &payload);
    buf[92..96].copy_from_slice(&checksum.to_le_bytes());
    let header = make_test_header(&buf);

    // Flip a bit in the payload
    payload[32] ^= 0x80;
    let result = header.verify_checksum(&buf[..92], &payload);
    assert!(
        matches!(result, Err(WireError::ChecksumMismatch { .. })),
        "Payload bit flip should cause checksum mismatch"
    );
}

#[test]
fn test_checksum_deterministic() {
    let header_bytes = [0xABu8; 92];
    let payload = [0xCDu8; 64];
    let c1 = ChecksumEngine::compute(&header_bytes, &payload);
    let c2 = ChecksumEngine::compute(&header_bytes, &payload);
    assert_eq!(c1, c2, "Checksum must be deterministic");
}

#[test]
fn test_checksum_different_inputs_different_checksums() {
    let h1 = [0xABu8; 92];
    let h2 = [0xABu8; 92];
    let p1 = [0x00u8; 64];
    let p2 = [0x01u8; 64];
    let c1 = ChecksumEngine::compute(&h1, &p1);
    let c2 = ChecksumEngine::compute(&h2, &p2);
    assert_ne!(c1, c2, "Different inputs should produce different checksums");
}

#[test]
fn test_checksum_large_payload() {
    let header_bytes = [0u8; 92];
    let payload = vec![0x42u8; 100_000];
    let checksum = ChecksumEngine::compute(&header_bytes, &payload);
    assert_ne!(checksum, 0, "Non-zero payload should have non-zero checksum");
}

#[test]
fn test_checksum_empty_payload() {
    let header_bytes = [0u8; 92];
    let checksum = ChecksumEngine::compute(&header_bytes, &[]);
    // Empty payload should still produce a deterministic checksum
    let checksum2 = ChecksumEngine::compute(&header_bytes, &[]);
    assert_eq!(checksum, checksum2);
}

#[test]
fn test_checksum_verify_failure_format() {
    let (buf, _) = build_checksummed_header();
    let mut bad_header = make_test_header(&buf);
    bad_header.checksum = 0xDEADBEEF;
    let result = bad_header.verify_checksum(&buf[..92], &[]);
    assert!(matches!(result, Err(WireError::ChecksumMismatch { .. })));
    if let Err(WireError::ChecksumMismatch { expected, actual }) = result {
        assert_eq!(expected, 0xDEADBEEF);
        assert_ne!(expected, actual);
    }
}

#[test]
fn test_checksum_different_header_prefix_lengths() {
    let short_header = [0u8; 10];
    let long_header = [0u8; 92];
    let payload = [0xFFu8; 32];
    let c_short = ChecksumEngine::compute(&short_header, &payload);
    let c_long = ChecksumEngine::compute(&long_header, &payload);
    assert_ne!(c_short, c_long, "Different header lengths affect checksum");
}
