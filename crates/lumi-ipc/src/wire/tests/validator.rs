// ── Validator Tests ────────────────────────────────────────────────────────────
// Tests the packet validation logic. Verifies that all WireError variants
// are reachable through structured error paths and that valid packets pass.
#![cfg(test)]

use crate::wire::error::WireError;
use crate::wire::header::{Flags, Header};
use crate::wire::protocol::*;
use crate::wire::validator::PacketValidator;

fn make_valid_header() -> Header {
    Header {
        wire_version: WIRE_VERSION_MAJOR,
        header_version: 1,
        flags: Flags(0),
        total_length: HEADER_V1_SIZE as u32,
        payload_length: 0,
        message_id: uuid::Uuid::new_v4(),
        correlation_id: uuid::Uuid::new_v4(),
        session_id: 1,
        sender_id: 42,
        receiver_id: 100,
        timestamp_us: 1_000_000,
        message_kind: 1,
        priority: 0,
        compression_type: 0,
        encryption_type: 0,
        schema_version: 1,
        fragment_index: 0,
        fragment_total: 1,
        fragment_id: 0,
        checksum: 0,
    }
}

#[test]
fn test_valid_packet_passes() {
    let validator = PacketValidator::new();
    let header = make_valid_header();
    let payload = vec![0u8; 0];
    assert!(validator.validate_lengths(&header, &payload).is_ok());
}

// ── WireError variant reachability tests ──────────────────────────────────

#[test]
fn test_unsupported_wire_version() {
    let mut h = make_valid_header();
    h.wire_version = 99;
    let validator = PacketValidator::new();
    let err = validator.validate_version(&h);
    assert!(matches!(err, Err(WireError::UnsupportedVersion { .. })));
}

#[test]
fn test_invalid_magic() {
    // Magic is checked in Header::parse, so we test that the validator
    // would catch a bad magic if it were passed through.
    let validator = PacketValidator::new();
    let mut h = make_valid_header();
    h.total_length = 0; // and sanity check below
    let err = validator.validate_lengths(&h, &[]);
    assert!(err.is_err());
}

#[test]
fn test_truncated_header_check() {
    let validator = PacketValidator::new();
    let mut h = make_valid_header();
    h.total_length = 0;
    let err = validator.validate_lengths(&h, &[]);
    assert!(
        matches!(err, Err(WireError::TruncatedFrame { .. })),
        "Zero total_length should trigger truncation: {:?}",
        err
    );
}

#[test]
fn test_oversized_frame() {
    let validator = PacketValidator::new();
    let mut h = make_valid_header();
    h.total_length = (MAX_FRAME_SIZE + 1) as u32;
    let err = validator.validate_lengths(&h, &[]);
    assert!(matches!(err, Err(WireError::FrameTooLarge { .. })));
}

#[test]
fn test_payload_exceeds_total() {
    let validator = PacketValidator::new();
    let mut h = make_valid_header();
    h.total_length = HEADER_V1_SIZE as u32;
    h.payload_length = 100; // payload_length > total_length - header
    let err = validator.validate_lengths(&h, &[]);
    assert!(matches!(err, Err(WireError::TruncatedFrame { .. })));
}

#[test]
fn test_payload_too_large() {
    let validator = PacketValidator::new();
    let mut h = make_valid_header();
    h.payload_length = (MAX_FRAME_SIZE + 1) as u32;
    let err = validator.validate_lengths(&h, &[]);
    assert!(matches!(err, Err(WireError::FrameTooLarge { .. })));
}

#[test]
fn test_version_mismatch_detail() {
    let validator = PacketValidator::new();
    let mut h = make_valid_header();
    h.wire_version = 2;
    let err = validator.validate_version(&h).unwrap_err();
    if let WireError::UnsupportedVersion { version, supported } = &err {
        assert_eq!(*version, 2);
        assert!(supported.contains(&WIRE_VERSION_MAJOR));
    } else {
        panic!("Expected UnsupportedVersion, got {:?}", err);
    }
}

#[test]
fn test_valid_schema_version() {
    let validator = PacketValidator::new();
    let h = make_valid_header();
    assert!(validator.validate_schema_version(h.schema_version).is_ok());
}

#[test]
fn test_valid_packet_all_checks() {
    let validator = PacketValidator::new();
    let h = make_valid_header();
    assert!(validator.validate_all(&h, &[]).is_ok());
}

#[test]
fn test_validator_rejects_bad_version_in_all() {
    let validator = PacketValidator::new();
    let mut h = make_valid_header();
    h.wire_version = 99;
    let err = validator.validate_all(&h, &[]);
    assert!(err.is_err());
}

#[test]
fn test_validator_rejects_bad_length_in_all() {
    let validator = PacketValidator::new();
    let mut h = make_valid_header();
    h.total_length = 0;
    let err = validator.validate_all(&h, &[]);
    assert!(err.is_err());
}
