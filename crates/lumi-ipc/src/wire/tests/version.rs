// ── Version Tests ──────────────────────────────────────────────────────────────
// Tests the VersionNegotiator: compatible negotiation, incompatible rejection,
// forward-compatibility (their's is newer major), backward-compatibility.
#![cfg(test)]

use crate::wire::error::WireError;
use crate::wire::version::{NegotiatedVersion, VersionNegotiator};

#[test]
fn test_compatible_negotiation() {
    let negotiator = VersionNegotiator;
    let result = negotiator.negotiate(1, 1, 1);
    assert!(result.is_ok());
    let nv = result.unwrap();
    assert_eq!(nv.wire_version, 1);
    assert_eq!(nv.header_version, 1);
    assert_eq!(nv.schema_version, 1);
}

#[test]
fn test_incompatible_major_version() {
    let negotiator = VersionNegotiator;
    let result = negotiator.negotiate(2, 1, 1);
    assert!(
        matches!(result, Err(WireError::IncompatibleVersions { .. })),
        "Major version 2 should be incompatible: {:?}",
        result
    );
}

#[test]
fn test_incompatible_header_version() {
    let negotiator = VersionNegotiator;
    let result = negotiator.negotiate(1, 99, 1);
    assert!(
        matches!(result, Err(WireError::IncompatibleVersions { .. })),
        "Header version 99 should be incompatible: {:?}",
        result
    );
}

#[test]
fn test_negotiate_higher_schema_version() {
    // Higher schema version should be accepted (forward-compat)
    let negotiator = VersionNegotiator;
    let result = negotiator.negotiate(1, 1, 5);
    assert!(result.is_ok());
    let nv = result.unwrap();
    assert_eq!(nv.schema_version, 5);
}

#[test]
fn test_negotiate_lower_schema_version() {
    // Lower schema version should be accepted (backward-compat)
    let negotiator = VersionNegotiator;
    let result = negotiator.negotiate(1, 1, 0);
    assert!(result.is_ok());
    let nv = result.unwrap();
    assert_eq!(nv.schema_version, 0);
}

#[test]
fn test_negotiate_exact_match() {
    let negotiator = VersionNegotiator;
    let result = negotiator.negotiate(WIRE_VERSION_MAJOR, 1, 1).unwrap();
    assert_eq!(result.wire_version, WIRE_VERSION_MAJOR);
}

#[test]
fn test_negotiate_compression_types_default() {
    let negotiator = VersionNegotiator;
    let nv = negotiator.negotiate(1, 1, 1).unwrap();
    // Our compression types should include None and Zstd
    assert!(nv.compression_supported.iter().any(|c| c == &crate::wire::compression::CompressionType::None));
    assert!(nv.compression_supported.iter().any(|c| c == &crate::wire::compression::CompressionType::Zstd));
}

#[test]
fn test_negotiate_encryption_types_default() {
    let negotiator = VersionNegotiator;
    let nv = negotiator.negotiate(1, 1, 1).unwrap();
    // Our encryption types should include None and ChaCha20Poly1305
    assert!(nv.encryption_supported.iter().any(|e| e == &crate::wire::encryption::EncryptionType::None));
    assert!(nv.encryption_supported.iter().any(|e| e == &crate::wire::encryption::EncryptionType::ChaCha20Poly1305));
}

#[test]
fn test_negotiate_all_zero_remote() {
    let negotiator = VersionNegotiator;
    let result = negotiator.negotiate(0, 0, 0);
    assert!(result.is_err(), "Both zero should be rejected");
}

#[test]
fn test_negotiate_error_contains_details() {
    let negotiator = VersionNegotiator;
    let err = negotiator.negotiate(99, 1, 1).unwrap_err();
    if let WireError::IncompatibleVersions { ours, theirs } = &err {
        assert!(!ours.is_empty());
        assert_eq!(*theirs, "99.1.1");
    } else {
        panic!("Expected IncompatibleVersions, got {:?}", err);
    }
}

#[test]
fn test_forward_compatibility_warning() {
    // Our schema version is 1, their schema version is higher
    // This should succeed with a warning
    let negotiator = VersionNegotiator;
    let result = negotiator.negotiate(1, 1, 10);
    assert!(result.is_ok(), "Forward compatibility should work: {:?}", result);
}

#[test]
fn test_negotiated_version_debug() {
    let negotiator = VersionNegotiator;
    let nv = negotiator.negotiate(1, 1, 1).unwrap();
    let debug_str = format!("{:?}", nv);
    assert!(debug_str.contains("wire_version: 1"));
    assert!(debug_str.contains("schema_version: 1"));
}
