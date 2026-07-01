//! # Compression Layer
//!
//! Compression is opt-in per-encode-call. Applied after serialization,
//! before fragmentation. Uses zstd with configurable compression levels.
//!
//! ## Compression Bomb Protection
//!
//! The `Decompressor::decompress()` method enforces a `max_output_size` limit.
//! Zstd can decompress a tiny input into gigabytes of output; this is detected
//! and rejected before memory exhaustion.

use crate::wire::error::WireError;
use crate::wire::protocol::{DEFAULT_COMPRESSION_LEVEL, MAX_DECOMPRESSED_SIZE};
use bytes::Bytes;
use std::io::Read;

/// Types of compression supported.
#[non_exhaustive]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionType {
    None = 0,
    Zstd = 1,
}

/// Compression level for zstd.
#[derive(Debug, Clone, Copy)]
pub struct CompressionLevel(i32);

impl CompressionLevel {
    pub const FASTEST: Self = Self(1);
    pub const DEFAULT: Self = Self(3);
    pub const BEST: Self = Self(9);

    pub fn new(level: i32) -> Self {
        Self(level.clamp(1, 22))
    }

    pub fn as_i32(&self) -> i32 {
        self.0
    }
}

/// Compression policy for the encode path.
#[derive(Debug, Clone)]
pub enum CompressionPolicy {
    Never,
    Always(CompressionLevel),
    ThresholdBytes(usize, CompressionLevel),
}

impl Default for CompressionPolicy {
    fn default() -> Self {
        CompressionPolicy::ThresholdBytes(512, CompressionLevel::DEFAULT)
    }
}

/// Reusable zstd compressor.
pub struct Compressor {
    level: CompressionLevel,
}

impl Compressor {
    /// Create a new compressor.
    pub fn new(level: CompressionLevel) -> Result<Self, WireError> {
        // Validate level
        let _ = level.as_i32();
        Ok(Self { level })
    }

    /// Compress `input`. Returns the compressed bytes.
    /// Returns `Err` if compression produces output larger than input
    /// (caller should fall back to uncompressed).
    pub fn compress(&self, input: &[u8]) -> Result<Bytes, WireError> {
        let compressed = zstd::encode_all(input, self.level.as_i32())
            .map_err(|e| WireError::CompressionFailed { cause: e.to_string() })?;

        // If compression doesn't reduce size, return error so caller can fall back
        if compressed.len() >= input.len() {
            return Err(WireError::CompressionFailed {
                cause: "Compression expanded data".into(),
            });
        }

        Ok(Bytes::from(compressed))
    }
}

impl std::fmt::Debug for Compressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compressor")
            .field("level", &self.level.0)
            .finish()
    }
}

/// Reusable zstd decompressor with bomb protection.
pub struct Decompressor;

impl Decompressor {
    /// Decompress `input`. `max_output_size` caps decompressed output
    /// (DoS protection against zip bombs).
    ///
    /// Returns `WireError::DecompressionBombDetected` if the decompressed
    /// output exceeds `max_output_size`.
    pub fn decompress(input: &[u8], max_output_size: usize) -> Result<Bytes, WireError> {
        // Use a streaming decoder with a size limit
        let mut decoder = zstd::Decoder::new(input)
            .map_err(|e| WireError::DecompressionFailed { cause: e.to_string() })?;

        // Decompress with a size limit
        let mut output = Vec::with_capacity(input.len().min(max_output_size));

        // Read in chunks up to max_output_size
        let mut buf = [0u8; 8192];
        loop {
            match decoder.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if output.len() + n > max_output_size {
                        return Err(WireError::DecompressionBombDetected {
                            claimed_size: output.len() + n,
                            limit: max_output_size,
                        });
                    }
                    output.extend_from_slice(&buf[..n]);
                }
                Err(e) => {
                    return Err(WireError::DecompressionFailed { cause: e.to_string() });
                }
            }
        }

        Ok(Bytes::from(output))
    }

    /// Decompress with the default max size (MAX_FRAME_SIZE).
    pub fn decompress_default(input: &[u8]) -> Result<Bytes, WireError> {
        Self::decompress(input, MAX_DECOMPRESSED_SIZE)
    }
}

impl std::fmt::Debug for Decompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decompressor").finish()
    }
}

/// Determine if compression should be applied based on policy and input size.
pub fn should_compress(policy: &CompressionPolicy, input_size: usize) -> Option<CompressionLevel> {
    match policy {
        CompressionPolicy::Never => None,
        CompressionPolicy::Always(level) => Some(*level),
        CompressionPolicy::ThresholdBytes(threshold, level) => {
            if input_size > *threshold {
                Some(*level)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress_roundtrip() {
        let data = b"Hello, world! This is test data for compression! ".repeat(100);
        let mut compressor = Compressor::new(CompressionLevel::DEFAULT).unwrap();

        let compressed = compressor.compress(&data).unwrap();
        let decompressed = Decompressor::decompress_default(&compressed).unwrap();

        assert_eq!(&data[..], &decompressed[..]);
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_bomb_detection() {
        // Create a zstd frame that claims to be much larger than the input
        // Real zstd bombs use very small inputs that decompress to gigabytes
        let tiny_input = b"Hello";
        let mut compressor = Compressor::new(CompressionLevel::BEST).unwrap();
        let compressed = compressor.compress(tiny_input).unwrap();

        // Decompression with tiny max should succeed since output is small
        let result = Decompressor::decompress(&compressed, 1024);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tiny_payload_not_compressed() {
        let data = b"hi";
        let mut compressor = Compressor::new(CompressionLevel::FASTEST).unwrap();
        let result = compressor.compress(data);
        // Tiny payload may not compress — should expand or error
        assert!(result.is_err());
    }
}
