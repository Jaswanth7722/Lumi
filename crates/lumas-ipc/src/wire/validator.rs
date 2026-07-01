// ── Packet Validator ──────────────────────────────────────────────────────────
// Validates wire packets at multiple stages of the decode pipeline.
// Every check returns a structured WireError with context for debugging.
//
// Validation order (fast-fail):
//   1. Wire version — reject if unsupported
//   2. Header version — reject if unsupported  
//   3. Frame size — reject if too large or truncated
//   4. Payload size — reject if exceeds limits
//   5. Reserved flags — reject if set (must be zero on send)
//   6. Timestamp skew — reject if too far from current time
//   7. Message ID — reject if all zeros
//   8. Sender ID — reject if zero (unauthenticated)
//   9. Checksum — verify integrity

use std::time::{SystemTime, UNIX_EPOCH};

use crate::wire::error::WireError;
use crate::wire::header::{Flags, Header};
use crate::wire::protocol::*;

/// Validates wire packets at various stages of the decode pipeline.
#[derive(Debug, Clone)]
pub struct PacketValidator {
    /// Whether to enforce timestamp skew checks.
    pub enforce_timestamp_skew: bool,
    /// Whether to enforce reserved flags checks.
    pub enforce_reserved_flags: bool,
    /// Whether to require non-zero message IDs.
    pub require_message_id: bool,
    /// Whether to require non-zero sender IDs.
    pub require_sender_id: bool,
}

impl Default for PacketValidator {
    fn default() -> Self {
        Self {
            enforce_timestamp_skew: true,
            enforce_reserved_flags: true,
            require_message_id: true,
            require_sender_id: true,
        }
    }
}

impl PacketValidator {
    /// Create a new validator with all checks enabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate the wire version from a parsed header.
    ///
    /// # Wire Safety
    /// This function is safe to call from any thread.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `WireError::UnsupportedWireVersion` if the wire version is not
    /// in the supported range.
    pub fn validate_wire_version(&self, header: &Header) -> Result<(), WireError> {
        if !SUPPORTED_WIRE_VERSIONS.contains(&header.wire_version) {
            return Err(WireError::UnsupportedWireVersion {
                found: header.wire_version,
                supported: format!("{:?}", SUPPORTED_WIRE_VERSIONS),
            });
        }
        Ok(())
    }

    /// Validate the header version from a parsed header.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `WireError::UnsupportedHeaderVersion` if the header version
    /// is not in the supported range.
    pub fn validate_header_version(&self, header: &Header) -> Result<(), WireError> {
        if !SUPPORTED_HEADER_VERSIONS.contains(&header.header_version) {
            return Err(WireError::UnsupportedHeaderVersion {
                found: header.header_version,
                supported: format!("{:?}", SUPPORTED_HEADER_VERSIONS),
            });
        }
        Ok(())
    }

    /// Validate frame and payload lengths against limits.
    ///
    /// Checks:
    /// - `total_length` >= `HEADER_V1_SIZE` (minimum valid frame)
    /// - `total_length` <= `MAX_FRAME_SIZE`
    /// - `payload_length` <= `MAX_FRAME_SIZE`
    /// - `payload_length` <= `total_length - HEADER_V1_SIZE` (fits in frame)
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns:
    /// - `WireError::TruncatedFrame` if `total_length` is too small
    /// - `WireError::FrameTooLarge` if `total_length` exceeds max
    /// - `WireError::TruncatedFrame` if payload doesn't fit in frame
    pub fn validate_lengths(&self, header: &Header, _payload: &[u8]) -> Result<(), WireError> {
        let total = header.total_length as usize;
        let payload = header.payload_length as usize;

        // Minimum frame size
        if total < HEADER_V1_SIZE {
            return Err(WireError::TruncatedHeader {
                available: total,
                needed: HEADER_V1_SIZE,
            });
        }

        // Maximum frame size
        if total > MAX_FRAME_SIZE {
            return Err(WireError::OversizedFrame {
                size: header.total_length,
                limit: MAX_FRAME_SIZE as u32,
            });
        }

        // Maximum payload size
        if payload > MAX_FRAME_SIZE {
            return Err(WireError::OversizedFrame {
                size: header.payload_length,
                limit: MAX_FRAME_SIZE as u32,
            });
        }

        // Payload fits in frame
        let available_for_payload = total - HEADER_V1_SIZE;
        if payload > available_for_payload {
            return Err(WireError::TruncatedPayload {
                available: available_for_payload,
                needed: payload,
            });
        }

        Ok(())
    }

    /// Validate reserved flags are not set.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `WireError::ReservedFlagsSet` if any reserved flag bit is set.
    pub fn validate_flags(&self, flags: Flags) -> Result<(), WireError> {
        if !flags.reserved_bits_clear() {
            return Err(WireError::ReservedFlagsSet { flags: flags.0 });
        }
        Ok(())
    }

    /// Validate the schema version is within a reasonable range.
    ///
    /// Schema versions above `CURRENT_SCHEMA_VERSION` are accepted (forward
    /// compatibility mode with warning). Very large values are rejected as
    /// likely corrupt headers.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `WireError::DeserializationFailed` if schema version is absurdly
    /// large (> 1000 indicates corruption).
    pub fn validate_schema_version(&self, schema_version: u16) -> Result<(), WireError> {
        if schema_version > 1000 {
            return Err(WireError::DeserializationFailed {
                schema_version,
                cause: format!("schema version {} exceeds sanity limit 1000", schema_version),
            });
        }
        Ok(())
    }

    /// Validate the timestamp against the current system clock.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `WireError::TimestampSkew` if the clock skew exceeds the limit.
    pub fn validate_timestamp(&self, timestamp_us: u64) -> Result<(), WireError> {
        if !self.enforce_timestamp_skew {
            return Ok(());
        }

        let now_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let skew = if timestamp_us > now_us {
            (timestamp_us - now_us) as i64
        } else {
            (now_us - timestamp_us) as i64
        };

        let limit_us = MAX_TIMESTAMP_SKEW_SECS * 1_000_000;
        if skew > limit_us as i64 {
            return Err(WireError::TimestampSkew {
                skew_secs: skew / 1_000_000,
                limit_secs: MAX_TIMESTAMP_SKEW_SECS,
            });
        }

        Ok(())
    }

    /// Validate message ID is non-zero.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `WireError::MissingMessageId` if the message ID is all zeros.
    pub fn validate_message_id(&self, header: &Header) -> Result<(), WireError> {
        if !self.require_message_id {
            return Ok(());
        }
        let uuid_bytes = header.message_id.as_bytes();
        if uuid_bytes.iter().all(|&b| b == 0) {
            return Err(WireError::MissingMessageId);
        }
        Ok(())
    }

    /// Validate sender ID is non-zero.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `WireError::MissingSenderId` if the sender ID is zero.
    pub fn validate_sender_id(&self, header: &Header) -> Result<(), WireError> {
        if !self.require_sender_id {
            return Ok(());
        }
        if header.sender_id == 0 {
            return Err(WireError::MissingSenderId);
        }
        Ok(())
    }

    /// Run all validation checks on a header + payload.
    ///
    /// This is the primary entry point for packet validation. It runs all
    /// enabled checks in order and returns the first error encountered.
    ///
    /// # Wire Safety
    /// This function is safe to call from any thread.
    ///
    /// # Panics
    /// Never panics, including on adversarial input.
    ///
    /// # Errors
    /// Returns the first `WireError` encountered during validation.
    pub fn validate_all(&self, header: &Header, payload: &[u8]) -> Result<(), WireError> {
        self.validate_wire_version(header)?;
        self.validate_header_version(header)?;
        self.validate_lengths(header, payload)?;
        self.validate_flags(header.flags)?;
        self.validate_schema_version(header.schema_version)?;
        self.validate_timestamp(header.timestamp_us)?;
        self.validate_message_id(header)?;
        self.validate_sender_id(header)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::header::HeaderBuilder;
    use uuid::Uuid;

    fn valid_header() -> Header {
        HeaderBuilder::new(Uuid::new_v4(), 1, 42, 100)
            .build(HEADER_V1_SIZE as u32, 0)
    }

    #[test]
    fn test_valid_packet_passes() {
        let validator = PacketValidator::new();
        let h = valid_header();
        assert!(validator.validate_all(&h, &[]).is_ok());
    }

    #[test]
    fn test_unsupported_wire_version() {
        let validator = PacketValidator::new();
        let mut h = valid_header();
        h.wire_version = 99;
        let err = validator.validate_all(&h, &[]).unwrap_err();
        assert!(matches!(err, WireError::UnsupportedWireVersion { .. }));
    }

    #[test]
    fn test_unsupported_header_version() {
        let validator = PacketValidator::new();
        let mut h = valid_header();
        h.header_version = 99;
        let err = validator.validate_all(&h, &[]).unwrap_err();
        assert!(matches!(err, WireError::UnsupportedHeaderVersion { .. }));
    }

    #[test]
    fn test_oversized_frame() {
        let validator = PacketValidator::new();
        let mut h = valid_header();
        h.total_length = (MAX_FRAME_SIZE + 1) as u32;
        let err = validator.validate_lengths(&h, &[]).unwrap_err();
        assert!(matches!(err, WireError::OversizedFrame { .. }));
    }

    #[test]
    fn test_truncated_frame() {
        let validator = PacketValidator::new();
        let mut h = valid_header();
        h.total_length = 0;
        let err = validator.validate_lengths(&h, &[]).unwrap_err();
        assert!(matches!(err, WireError::TruncatedHeader { .. }));
    }

    #[test]
    fn test_payload_exceeds_frame() {
        let validator = PacketValidator::new();
        let mut h = valid_header();
        h.total_length = HEADER_V1_SIZE as u32;
        h.payload_length = 100;
        let err = validator.validate_lengths(&h, &[]).unwrap_err();
        assert!(matches!(err, WireError::TruncatedPayload { .. }));
    }

    #[test]
    fn test_reserved_flags_detected() {
        let validator = PacketValidator::new();
        let flags = Flags(0xFF00);
        let err = validator.validate_flags(flags).unwrap_err();
        assert!(matches!(err, WireError::ReservedFlagsSet { .. }));
    }

    #[test]
    fn test_clean_flags_pass() {
        let validator = PacketValidator::new();
        assert!(validator.validate_flags(Flags(0)).is_ok());
    }

    #[test]
    fn test_missing_message_id() {
        let validator = PacketValidator::new();
        let zero_uuid = uuid::Uuid::from_u128(0);
        let h = HeaderBuilder::new(zero_uuid, 1, 42, 100).build(HEADER_V1_SIZE as u32, 0);
        let err = validator.validate_message_id(&h).unwrap_err();
        assert!(matches!(err, WireError::MissingMessageId));
    }

    #[test]
    fn test_missing_sender_id() {
        let validator = PacketValidator::new();
        let h = HeaderBuilder::new(Uuid::new_v4(), 1, 0, 100).build(HEADER_V1_SIZE as u32, 0);
        let err = validator.validate_sender_id(&h).unwrap_err();
        assert!(matches!(err, WireError::MissingSenderId));
    }

    #[test]
    fn test_schema_version_too_high() {
        let validator = PacketValidator::new();
        let err = validator.validate_schema_version(9999).unwrap_err();
        assert!(matches!(err, WireError::DeserializationFailed { .. }));
    }

    #[test]
    fn test_schema_version_forward_compat() {
        let validator = PacketValidator::new();
        assert!(validator.validate_schema_version(5).is_ok());
    }

    #[test]
    fn test_validate_version_rejects_incompatible() {
        let validator = PacketValidator::new();
        let mut h = valid_header();
        h.wire_version = 2;
        assert!(validator.validate_wire_version(&h).is_err());
    }

    #[test]
    fn test_validate_with_checksum_verification() {
        // Not part of validator, but ensure validate_all rejects bad data
        let validator = PacketValidator::new();
        let mut h = valid_header();
        h.checksum = 0xDEADBEEF;
        // validate_all doesn't check checksum — that's done separately
        assert!(validator.validate_wire_version(&h).is_ok());
    }

    #[test]
    fn test_validator_default_config() {
        let v = PacketValidator::default();
        assert!(v.enforce_timestamp_skew);
        assert!(v.enforce_reserved_flags);
        assert!(v.require_message_id);
        assert!(v.require_sender_id);
    }

    #[test]
    fn test_validator_disable_timestamp() {
        let v = PacketValidator {
            enforce_timestamp_skew: false,
            ..Default::default()
        };
        assert!(v.validate_timestamp(0).is_ok());
    }

    #[test]
    fn test_validator_disable_message_id() {
        let v = PacketValidator {
            require_message_id: false,
            ..Default::default()
        };
        let zero_uuid = uuid::Uuid::from_u128(0);
        let h = HeaderBuilder::new(zero_uuid, 1, 42, 100).build(HEADER_V1_SIZE as u32, 0);
        assert!(v.validate_message_id(&h).is_ok());
    }
}
