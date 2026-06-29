// ── Wire Version Negotiation ──────────────────────────────────────────────────
// Negotiates mutually supported protocol versions between two Lumi processes.
// Uses downgrade-on-disconnect semantics: always select the highest version
// that both sides support, falling back to the lowest common denominator.

use std::fmt;
use std::ops::RangeInclusive;

use crate::wire::compression::CompressionType;
use crate::wire::encryption::EncryptionType;
use crate::wire::error::WireError;
use crate::wire::protocol::*;

/// Capabilities declared by one side of a wire connection.
#[derive(Debug, Clone)]
pub struct WireCapabilities {
    /// Range of major wire versions supported (inclusive).
    pub wire_version_range: RangeInclusive<u8>,
    /// Range of header versions supported (inclusive).
    pub header_version_range: RangeInclusive<u8>,
    /// Highest schema version supported for MessagePack serialization.
    pub schema_version: u16,
    /// Supported compression type discriminants.
    pub supported_compression: Vec<u8>,
    /// Supported encryption type discriminants.
    pub supported_encryption: Vec<u8>,
}

impl Default for WireCapabilities {
    fn default() -> Self {
        Self {
            wire_version_range: SUPPORTED_WIRE_VERSIONS.clone(),
            header_version_range: SUPPORTED_HEADER_VERSIONS.clone(),
            schema_version: CURRENT_SCHEMA_VERSION,
            supported_compression: vec![CompressionType::None as u8, CompressionType::Zstd as u8],
            supported_encryption: vec![EncryptionType::None as u8, EncryptionType::ChaCha20Poly1305 as u8],
        }
    }
}

/// The mutually agreed version after successful negotiation.
#[derive(Debug, Clone)]
pub struct NegotiatedVersion {
    /// Selected wire major version.
    pub wire_version: u8,
    /// Selected header version.
    pub header_version: u8,
    /// Selected schema version (the minimum of ours and theirs).
    pub schema_version: u16,
    /// Compression types both sides support.
    pub compression_supported: Vec<CompressionType>,
    /// Encryption types both sides support.
    pub encryption_supported: Vec<EncryptionType>,
}

impl NegotiatedVersion {
    /// Whether the given compression type is supported by both sides.
    pub fn supports_compression(&self, ct: CompressionType) -> bool {
        self.compression_supported.contains(&ct)
    }

    /// Whether the given encryption type is supported by both sides.
    pub fn supports_encryption(&self, et: EncryptionType) -> bool {
        self.encryption_supported.contains(&et)
    }
}

impl fmt::Display for NegotiatedVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "wire v{}.{} / header v{} / schema v{} ({} compression types, {} encryption types)",
            self.wire_version,
            0, // minor not tracked in negotiation
            self.header_version,
            self.schema_version,
            self.compression_supported.len(),
            self.encryption_supported.len(),
        )
    }
}

/// The version negotiator: given our capabilities and the peer's, selects the
/// highest mutually supported version combination.
pub struct VersionNegotiator;

impl VersionNegotiator {
    /// Negotiate wire protocol versions between us and a peer.
    ///
    /// Returns the highest mutually supported version combination, or
    /// `WireError::IncompatibleVersions` if no common ground exists.
    ///
    /// # Wire Safety
    /// This function is safe to call from any thread. It performs no I/O and
    /// does not allocate beyond the returned `NegotiatedVersion`.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `WireError::IncompatibleVersions` if the wire or header
    /// version ranges have no overlap.
    pub fn negotiate(
        ours: &WireCapabilities,
        theirs: &WireCapabilities,
    ) -> Result<NegotiatedVersion, WireError> {
        // Find the highest wire version both sides support
        let wire_version = Self::find_highest_common(
            ours.wire_version_range.clone(),
            theirs.wire_version_range.clone(),
        )
        .ok_or_else(|| {
            let ours_str = format!("{}", ours.wire_version_range);
            let theirs_str = format!("{}", theirs.wire_version_range);
            WireError::IncompatibleVersions {
                ours: format!("wire {}", ours_str),
                theirs: format!("wire {}", theirs_str),
            }
        })?;

        // Find the highest header version both sides support
        let header_version = Self::find_highest_common(
            ours.header_version_range.clone(),
            theirs.header_version_range.clone(),
        )
        .ok_or_else(|| {
            let ours_str = format!("{}", ours.header_version_range);
            let theirs_str = format!("{}", theirs.header_version_range);
            WireError::IncompatibleVersions {
                ours: format!("header {}", ours_str),
                theirs: format!("header {}", theirs_str),
            }
        })?;

        // Schema version: use the minimum (ours is usually the current version)
        let schema_version = ours.schema_version.min(theirs.schema_version);

        // Intersect compression and encryption support
        let compression_supported = Self::intersect_compression(
            &ours.supported_compression,
            &theirs.supported_compression,
        );

        let encryption_supported = Self::intersect_encryption(
            &ours.supported_encryption,
            &theirs.supported_encryption,
        );

        Ok(NegotiatedVersion {
            wire_version,
            header_version,
            schema_version,
            compression_supported,
            encryption_supported,
        })
    }

    /// Quick compatibility check: do both sides support the given wire version?
    ///
    /// # Panics
    /// Never panics.
    pub fn is_compatible(our_wire_version: u8, their_wire_version: u8) -> bool {
        our_wire_version == their_wire_version
    }

    /// Find the highest common value in two inclusive ranges.
    fn find_highest_common(ours: RangeInclusive<u8>, theirs: RangeInclusive<u8>) -> Option<u8> {
        let our_max = *ours.end();
        let their_max = *theirs.end();
        let our_min = *ours.start();
        let their_min = *theirs.start();

        let common_max = our_max.min(their_max);
        let common_min = our_min.max(their_min);

        if common_min <= common_max {
            Some(common_max) // highest mutually supported
        } else {
            None
        }
    }

    /// Intersect two lists of compression type discriminants.
    fn intersect_compression(ours: &[u8], theirs: &[u8]) -> Vec<CompressionType> {
        let theirs_set: Vec<u8> = theirs.to_vec();
        ours.iter()
            .filter(|c| theirs_set.contains(c))
            .filter_map(|&c| match c {
                0 => Some(CompressionType::None),
                1 => Some(CompressionType::Zstd),
                _ => None,
            })
            .collect()
    }

    /// Intersect two lists of encryption type discriminants.
    fn intersect_encryption(ours: &[u8], theirs: &[u8]) -> Vec<EncryptionType> {
        let theirs_set: Vec<u8> = theirs.to_vec();
        ours.iter()
            .filter(|c| theirs_set.contains(c))
            .filter_map(|&c| match c {
                0 => Some(EncryptionType::None),
                1 => Some(EncryptionType::ChaCha20Poly1305),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn our_caps() -> WireCapabilities {
        WireCapabilities::default()
    }

    #[test]
    fn test_compatible_negotiation() {
        let result = VersionNegotiator::negotiate(&our_caps(), &our_caps());
        assert!(result.is_ok());
        let nv = result.unwrap();
        assert_eq!(nv.wire_version, 1);
        assert_eq!(nv.header_version, 1);
        assert_eq!(nv.schema_version, 1);
    }

    #[test]
    fn test_incompatible_wire_version() {
        let mut theirs = our_caps();
        theirs.wire_version_range = 2..=2;
        let result = VersionNegotiator::negotiate(&our_caps(), &theirs);
        assert!(matches!(result, Err(WireError::IncompatibleVersions { .. })));
    }

    #[test]
    fn test_incompatible_header_version() {
        let mut theirs = our_caps();
        theirs.header_version_range = 99..=99;
        let result = VersionNegotiator::negotiate(&our_caps(), &theirs);
        assert!(matches!(result, Err(WireError::IncompatibleVersions { .. })));
    }

    #[test]
    fn test_higher_schema_version_is_backward_compat() {
        let mut theirs = our_caps();
        theirs.schema_version = 5;
        let nv = VersionNegotiator::negotiate(&our_caps(), &theirs).unwrap();
        assert_eq!(nv.schema_version, 1); // ours is lower, use ours
    }

    #[test]
    fn test_lower_schema_version_is_forward_compat() {
        let mut theirs = our_caps();
        theirs.schema_version = 0;
        let nv = VersionNegotiator::negotiate(&our_caps(), &theirs).unwrap();
        assert_eq!(nv.schema_version, 0); // theirs is lower, use theirs
    }

    #[test]
    fn test_negotiate_higher_wire_version_overlap() {
        let mut theirs = our_caps();
        theirs.wire_version_range = 0..=1; // supports 0 and 1
        let nv = VersionNegotiator::negotiate(&our_caps(), &theirs).unwrap();
        assert_eq!(nv.wire_version, 1); // highest common
    }

    #[test]
    fn test_negotiate_lower_header_version() {
        let mut theirs = our_caps();
        theirs.header_version_range = 0..=1;
        let nv = VersionNegotiator::negotiate(&our_caps(), &theirs).unwrap();
        assert_eq!(nv.header_version, 1); // highest common
    }

    #[test]
    fn test_compression_intersection() {
        let mut theirs = our_caps();
        theirs.supported_compression = vec![0]; // only None
        let nv = VersionNegotiator::negotiate(&our_caps(), &theirs).unwrap();
        assert!(nv.supports_compression(CompressionType::None));
        assert!(!nv.supports_compression(CompressionType::Zstd));
    }

    #[test]
    fn test_encryption_intersection() {
        let mut theirs = our_caps();
        theirs.supported_encryption = vec![0]; // only None
        let nv = VersionNegotiator::negotiate(&our_caps(), &theirs).unwrap();
        assert!(nv.supports_encryption(EncryptionType::None));
        assert!(!nv.supports_encryption(EncryptionType::ChaCha20Poly1305));
    }

    #[test]
    fn test_is_compatible() {
        assert!(VersionNegotiator::is_compatible(1, 1));
        assert!(!VersionNegotiator::is_compatible(1, 2));
        assert!(!VersionNegotiator::is_compatible(2, 1));
    }

    #[test]
    fn test_negotiated_version_display() {
        let nv = VersionNegotiator::negotiate(&our_caps(), &our_caps()).unwrap();
        let display = format!("{}", nv);
        assert!(display.contains("wire v1"));
        assert!(display.contains("schema v1"));
    }

    #[test]
    fn test_wire_capabilities_default() {
        let caps = WireCapabilities::default();
        assert!(caps.wire_version_range.contains(&1));
        assert!(caps.header_version_range.contains(&1));
        assert_eq!(caps.schema_version, 1);
        assert_eq!(caps.supported_compression.len(), 2);
        assert_eq!(caps.supported_encryption.len(), 2);
    }

    #[test]
    fn test_all_zero_remote_is_incompatible() {
        let mut theirs = our_caps();
        theirs.wire_version_range = 0..=0;
        let result = VersionNegotiator::negotiate(&our_caps(), &theirs);
        assert!(result.is_err());
    }
}
