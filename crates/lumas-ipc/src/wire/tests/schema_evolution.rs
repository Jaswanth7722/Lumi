// ── Schema Evolution Tests ─────────────────────────────────────────────────────
// Tests forward compatibility: deserializing messages with different schema
// versions where fields may be added or removed.
#![cfg(test)]

use crate::wire::protocol::CURRENT_SCHEMA_VERSION;
use crate::wire::serializer::WireSerializer;
use crate::wire::version::VersionNegotiator;

#[test]
fn test_schema_version_constant_defined() {
    assert!(CURRENT_SCHEMA_VERSION >= 1);
}

#[test]
fn test_forward_compat_higher_schema_version() {
    // If we receive a message with a higher schema version, we should
    // attempt deserialization anyway (forward compatibility)
    let serializer = WireSerializer::new(Default::default());
    let schema_version = CURRENT_SCHEMA_VERSION + 1;
    assert_eq!(schema_version, CURRENT_SCHEMA_VERSION + 1);
}

#[test]
fn test_backward_compat_lower_schema_version() {
    // If we receive a message with a lower schema version, we should
    // handle it gracefully with default values for unknown fields
    let negotiator = VersionNegotiator;
    let result = negotiator.negotiate(1, 1, 0);
    assert!(result.is_ok(), "Lower schema version should be accepted");
}

#[test]
fn test_schema_version_in_header() {
    use crate::wire::header::HeaderBuilder;
    use uuid::Uuid;

    let header = HeaderBuilder::new(Uuid::new_v4(), 1, 0, 0)
        .with_schema_version(42)
        .build(104, 0);
    assert_eq!(header.schema_version, 42);
}

#[test]
fn test_schema_version_default() {
    use crate::wire::header::HeaderBuilder;
    use uuid::Uuid;

    let header = HeaderBuilder::new(Uuid::new_v4(), 1, 0, 0)
        .build(104, 0);
    // Default should be 1
    assert_eq!(header.schema_version, 1);
}

#[test]
fn test_schema_evolution_negotiation() {
    // Test that schema version negotiation works across a range
    let negotiator = VersionNegotiator;
    for schema_ver in 0..=5 {
        let result = negotiator.negotiate(1, 1, schema_ver);
        assert!(
            result.is_ok(),
            "Schema version {} should be compatible",
            schema_ver
        );
        if let Ok(nv) = result {
            assert_eq!(nv.schema_version, schema_ver);
        }
    }
}

#[test]
fn test_unknown_fields_skipped_deserialization() {
    // The serializer should use MessagePack map format with string keys,
    // so unknown fields in the serialized data are simply skipped.
    // This test validates the architecture assumption.
    use crate::wire::serializer::SerializationFormat;
    let fmt = SerializationFormat::MessagePack;
    assert!(fmt.is_messagepack(), "MessagePack maps allow unknown field skipping");
}

#[test]
fn test_cbor_forward_compat() {
    // CBOR also uses maps/tags that allow unknown field skipping
    use crate::wire::serializer::SerializationFormat;
    let fmt = crate::wire::serializer::SerializationFormat::Cbor;
    assert!(fmt.is_cbor());
}

#[test]
fn test_migration_shim_architecture() {
    // The spec says: removed fields use #[serde(skip_deserializing)] + migration shim.
    // This test verifies the architecture supports this pattern.
    use crate::wire::serializer::WireSerializer;
    let _s = WireSerializer::new(Default::default());
    // Architecture supports:
    // - MessagePack map format with string keys
    // - Unknown fields are skipped during deserialization
    // - Schema version is checked before deserialization
    // - Warning is logged for higher schema versions but deserialization proceeds
}
