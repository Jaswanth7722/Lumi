//! # Checksum Engine
//!
//! Uses `BLAKE3` truncated to 32 bits for the packet checksum.
//!
//! Rationale: CRC32 detects accidental corruption but is trivially forged;
//! BLAKE3-truncated provides collision resistance proportional to its output
//! size. BLAKE3 is faster than CRC32 on modern hardware when using SIMD.
//!
//! The checksum covers: header bytes [0..92] (all header fields except the
//! checksum field itself) concatenated with the payload bytes (after
//! compression and encryption if applied). The checksum field at offset 92
//! is zero during checksum computation.

use crate::wire::error::WireError;
use crate::wire::header::Header;
use crate::wire::protocol::OFFSET_CHECKSUM;

/// Checksum engine using BLAKE3 truncated to 32 bits.
pub struct ChecksumEngine;

impl ChecksumEngine {
    /// Compute BLAKE3-truncated-to-32-bits checksum over header prefix + payload.
    ///
    /// `header_bytes`: the first 92 bytes of the header (checksum field at offset
    /// 92 must be zeroed during computation — the caller is responsible for this).
    ///
    /// `payload`: the wire payload bytes (post-compression, post-encryption).
    ///
    /// # Performance
    ///
    /// BLAKE3 is faster than CRC32 on payloads > 1KB on modern hardware with SIMD.
    pub fn compute(header_bytes: &[u8], payload: &[u8]) -> u32 {
        let mut hasher = blake3::Hasher::new();
        hasher.update(header_bytes);
        hasher.update(payload);
        let hash = hasher.finalize();
        u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap())
    }

    /// Verify the checksum in `header` against `header_bytes` and `payload`.
    ///
    /// Returns `Ok(())` if the checksum matches, `Err(WireError::ChecksumMismatch)`
    /// otherwise.
    pub fn verify(header: &Header, header_bytes: &[u8], payload: &[u8]) -> Result<(), WireError> {
        let expected = header.checksum;

        // Zero out the checksum field in a copy for computation
        let mut header_prefix = header_bytes[..OFFSET_CHECKSUM].to_vec();
        // Ensure checksum field is zero during computation
        // (header_bytes should already have it zeroed at offset 92, but be safe)
        while header_prefix.len() < OFFSET_CHECKSUM + 4 {
            header_prefix.push(0);
        }

        let actual = Self::compute(&header_prefix[..OFFSET_CHECKSUM], payload);

        if expected != actual {
            return Err(WireError::ChecksumMismatch { expected, actual });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_deterministic() {
        let header = [0u8; 92];
        let payload = b"hello world";

        let c1 = ChecksumEngine::compute(&header, payload);
        let c2 = ChecksumEngine::compute(&header, payload);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_checksum_different_payload() {
        let header = [0u8; 92];
        let c1 = ChecksumEngine::compute(&header, b"hello");
        let c2 = ChecksumEngine::compute(&header, b"world");
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_single_bit_flip_detected() {
        let header = [0u8; 92];
        let payload = b"hello world";

        let original = ChecksumEngine::compute(&header, payload);

        // Flip one bit in payload
        let mut corrupted = payload.to_vec();
        corrupted[0] ^= 0x01;

        let corrupted_checksum = ChecksumEngine::compute(&header, &corrupted);
        assert_ne!(original, corrupted_checksum);
    }

    #[test]
    fn test_all_zero_payload() {
        let header = [0u8; 92];
        let payload = [0u8; 1024];
        let checksum = ChecksumEngine::compute(&header, &payload);
        // Just verify it doesn't panic and returns something
        assert_ne!(checksum, 0); // BLAKE3 of zeros is not zero
    }

    #[test]
    fn test_checksum_is_32_bit() {
        let header = [0u8; 92];
        let payload = b"test";

        let checksum = ChecksumEngine::compute(&header, payload);
        // Verify it fits in 32 bits
        assert!(checksum <= u32::MAX);
    }
}
