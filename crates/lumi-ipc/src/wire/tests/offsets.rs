// ── Offset Layout Assertions (Compile-Time) ────────────────────────────────────
// Verified at compile time: if any OFFSET_* constant deviates from the spec,
// this file will fail to compile.
//
// WARNING: If any assertion here fails, the binary layout specification has been
// broken. Do NOT merely update these assertions — fix the protocol.rs constants
// to match the spec.
#![cfg(test)]

// Compile-time assertions for fixed-position prefix offsets (16 bytes)
const _: () = {
    assert!(crate::wire::protocol::OFFSET_MAGIC == 0);
    assert!(crate::wire::protocol::OFFSET_WIRE_VERSION == 4);
    assert!(crate::wire::protocol::OFFSET_HEADER_VERSION == 5);
    assert!(crate::wire::protocol::OFFSET_FLAGS == 6);
    assert!(crate::wire::protocol::OFFSET_TOTAL_LENGTH == 8);
    assert!(crate::wire::protocol::OFFSET_PAYLOAD_LENGTH == 12);
};

// Compile-time assertions for header v1 field offsets
const _: () = {
    assert!(crate::wire::protocol::OFFSET_MESSAGE_ID == 16);
    assert!(crate::wire::protocol::OFFSET_CORRELATION_ID == 32);
    assert!(crate::wire::protocol::OFFSET_SESSION_ID == 48);
    assert!(crate::wire::protocol::OFFSET_SENDER_ID == 56);
    assert!(crate::wire::protocol::OFFSET_RECEIVER_ID == 64);
    assert!(crate::wire::protocol::OFFSET_TIMESTAMP_US == 72);
    assert!(crate::wire::protocol::OFFSET_MESSAGE_KIND == 80);
    assert!(crate::wire::protocol::OFFSET_PRIORITY == 81);
    assert!(crate::wire::protocol::OFFSET_COMPRESSION == 82);
    assert!(crate::wire::protocol::OFFSET_ENCRYPTION == 83);
    assert!(crate::wire::protocol::OFFSET_SCHEMA_VERSION == 84);
    assert!(crate::wire::protocol::OFFSET_FRAGMENT_INDEX == 86);
    assert!(crate::wire::protocol::OFFSET_FRAGMENT_TOTAL == 88);
    assert!(crate::wire::protocol::OFFSET_FRAGMENT_ID == 90);
    assert!(crate::wire::protocol::OFFSET_CHECKSUM == 92);
    assert!(crate::wire::protocol::OFFSET_RESERVED == 96);
};

// Compile-time assertions for size constants
const _: () = {
    assert!(crate::wire::protocol::HEADER_V1_SIZE == 104);
    assert!(crate::wire::protocol::MIN_FRAME_SIZE == 104);
    assert!(crate::wire::protocol::MAX_FRAME_SIZE == 512 * 1024);
    assert!(crate::wire::protocol::BROADCAST_RECEIVER == u64::MAX);
};

// Compile-time assertions for magic value
const _: () = {
    assert!(crate::wire::protocol::WIRE_MAGIC == 0x4C554D49);
    assert!(crate::wire::protocol::WIRE_VERSION_MAJOR == 1);
    assert!(crate::wire::protocol::WIRE_VERSION_MINOR == 0);
};

// Compile-time assertions for flag bit positions
const _: () = {
    assert!(crate::wire::protocol::FLAG_COMPRESSED == 1 << 0);
    assert!(crate::wire::protocol::FLAG_ENCRYPTED == 1 << 1);
    assert!(crate::wire::protocol::FLAG_FRAGMENTED == 1 << 2);
    assert!(crate::wire::protocol::FLAG_STREAM == 1 << 3);
    assert!(crate::wire::protocol::FLAG_REQUIRES_ACK == 1 << 4);
    assert!(crate::wire::protocol::FLAGS_RESERVED_MASK == 0xFF00);
};

#[test]
fn verify_offset_constants_runtime() {
    // Double-check at runtime too (as a test, in addition to compile-time assertions)
    assert_eq!(crate::wire::protocol::OFFSET_MAGIC, 0);
    assert_eq!(crate::wire::protocol::OFFSET_WIRE_VERSION, 4);
    assert_eq!(crate::wire::protocol::OFFSET_TOTAL_LENGTH, 8);
    assert_eq!(crate::wire::protocol::HEADER_V1_SIZE, 104);
}

#[test]
fn verify_flag_constants_runtime() {
    assert_eq!(crate::wire::protocol::FLAG_COMPRESSED, 0x0001);
    assert_eq!(crate::wire::protocol::FLAGS_RESERVED_MASK, 0xFF00);
}

#[test]
fn verify_version_constants_runtime() {
    assert_eq!(crate::wire::protocol::WIRE_VERSION_MAJOR, 1);
    assert!(crate::wire::protocol::SUPPORTED_WIRE_VERSIONS.contains(&crate::wire::protocol::WIRE_VERSION_MAJOR));
}
