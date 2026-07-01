//! # Wire Protocol Error Types
//!
//! All wire-level errors with error codes for integration with `lumi-error`.
//! Every error variant is documented with its severity and recovery strategy.

use std::fmt;

/// Wire protocol error.
#[derive(Debug, Clone)]
pub enum WireError {
    // ── Header / Magic ────────────────────────────────────────────────────────
    InvalidMagic { found: u32, expected: u32 },
    UnsupportedWireVersion { found: u8, supported: String },
    UnsupportedHeaderVersion { found: u8, supported: String },

    // ── Frame Size ────────────────────────────────────────────────────────────
    OversizedFrame { size: u32, limit: u32 },
    TruncatedHeader { available: usize, needed: usize },
    TruncatedPayload { available: usize, needed: usize },
    PayloadExceedsFrame { payload: u32, total: u32 },
    PayloadExceedsChannelLimit { kind: u8, size: u32, limit: u32 },

    // ── Flags / Fields ────────────────────────────────────────────────────────
    ReservedFlagsSet { flags: u16 },
    TimestampSkew { skew_secs: i64, limit_secs: u64 },
    MissingMessageId,
    MissingSenderId,
    InconsistentFragmentation { index: u16, total: u16 },

    // ── Integrity ─────────────────────────────────────────────────────────────
    ChecksumMismatch { expected: u32, actual: u32 },

    // ── Fragmentation ─────────────────────────────────────────────────────────
    DuplicateFragment { fragment_id: u16, index: u16 },
    ReassemblyTimeout { fragment_id: u16, received: u16, expected: u16 },
    ReassemblyTooLarge { size: usize, limit: usize },
    TooManyInFlightReassemblies { count: usize, limit: usize },

    // ── Serialization ─────────────────────────────────────────────────────────
    SerializationFailed { type_name: &'static str, cause: String },
    DeserializationFailed { schema_version: u16, cause: String },

    // ── Compression ───────────────────────────────────────────────────────────
    CompressionFailed { cause: String },
    DecompressionFailed { cause: String },
    DecompressionBombDetected { claimed_size: usize, limit: usize },

    // ── Encryption ────────────────────────────────────────────────────────────
    EncryptionFailed { cause: String },
    DecryptionFailed,
    NonceExhausted,

    // ── Stream / Connection ───────────────────────────────────────────────────
    StreamDesync { bytes_lost: usize },
    StreamBufferFull { stream_id: u64, buffer_chunks: usize },

    // ── Version Negotiation ───────────────────────────────────────────────────
    IncompatibleVersions { ours: String, theirs: String },

    // ── Unknown Types ─────────────────────────────────────────────────────────
    UnknownCompressionType { found: u8 },
    UnknownEncryptionType { found: u8 },
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::InvalidMagic { found, expected } => {
                write!(f, "Invalid magic: found {found:#010x}, expected {expected:#010x}")
            }
            WireError::UnsupportedWireVersion { found, supported } => {
                write!(f, "Unsupported wire version {found}, supported: {supported}")
            }
            WireError::UnsupportedHeaderVersion { found, supported } => {
                write!(f, "Unsupported header version {found}, supported: {supported}")
            }
            WireError::OversizedFrame { size, limit } => {
                write!(f, "Oversized frame: {size} bytes (max {limit})")
            }
            WireError::TruncatedHeader { available, needed } => {
                write!(f, "Truncated header: {available} bytes available, {needed} needed")
            }
            WireError::TruncatedPayload { available, needed } => {
                write!(f, "Truncated payload: {available} bytes available, {needed} needed")
            }
            WireError::PayloadExceedsFrame { payload, total } => {
                write!(f, "Payload length {payload} exceeds total frame length {total}")
            }
            WireError::PayloadExceedsChannelLimit { kind, size, limit } => {
                write!(f, "Payload {size} bytes exceeds channel {kind} limit {limit}")
            }
            WireError::ReservedFlagsSet { flags } => {
                write!(f, "Reserved flags set: {flags:#06x}")
            }
            WireError::TimestampSkew { skew_secs, limit_secs } => {
                write!(f, "Timestamp skew {skew_secs}s exceeds limit {limit_secs}s")
            }
            WireError::MissingMessageId => write!(f, "Missing message ID (all zeros)"),
            WireError::MissingSenderId => write!(f, "Missing sender ID (zero)"),
            WireError::InconsistentFragmentation { index, total } => {
                write!(f, "Inconsistent fragmentation: index {index} >= total {total}")
            }
            WireError::ChecksumMismatch { expected, actual } => {
                write!(f, "Checksum mismatch: expected {expected:#010x}, actual {actual:#010x}")
            }
            WireError::DuplicateFragment { fragment_id, index } => {
                write!(f, "Duplicate fragment {fragment_id} index {index}")
            }
            WireError::ReassemblyTimeout { fragment_id, received, expected } => {
                write!(f, "Reassembly timeout for fragment {fragment_id}: received {received}/{expected}")
            }
            WireError::ReassemblyTooLarge { size, limit } => {
                write!(f, "Reassembly {size} bytes exceeds limit {limit}")
            }
            WireError::TooManyInFlightReassemblies { count, limit } => {
                write!(f, "Too many in-flight reassemblies: {count} (max {limit})")
            }
            WireError::SerializationFailed { type_name, cause } => {
                write!(f, "Serialization of {type_name} failed: {cause}")
            }
            WireError::DeserializationFailed { schema_version, cause } => {
                write!(f, "Deserialization at schema v{schema_version} failed: {cause}")
            }
            WireError::CompressionFailed { cause } => {
                write!(f, "Compression failed: {cause}")
            }
            WireError::DecompressionFailed { cause } => {
                write!(f, "Decompression failed: {cause}")
            }
            WireError::DecompressionBombDetected { claimed_size, limit } => {
                write!(f, "Decompression bomb: claimed {claimed_size} bytes (max {limit})")
            }
            WireError::EncryptionFailed { cause } => {
                write!(f, "Encryption failed: {cause}")
            }
            WireError::DecryptionFailed => write!(f, "Decryption failed"),
            WireError::NonceExhausted => write!(f, "Nonce exhausted (2^64 encryptions)"),
            WireError::StreamDesync { bytes_lost } => {
                write!(f, "Stream desynchronized, lost {bytes_lost} bytes")
            }
            WireError::StreamBufferFull { stream_id, buffer_chunks } => {
                write!(f, "Stream {stream_id} buffer full ({buffer_chunks} chunks)")
            }
            WireError::IncompatibleVersions { ours, theirs } => {
                write!(f, "Incompatible wire versions: ours={ours}, theirs={theirs}")
            }
            WireError::UnknownCompressionType { found } => {
                write!(f, "Unknown compression type: {found}")
            }
            WireError::UnknownEncryptionType { found } => {
                write!(f, "Unknown encryption type: {found}")
            }
        }
    }
}

impl std::error::Error for WireError {}

/// Convert from crate-level error for convenience.
impl From<super::super::error::CodecError> for WireError {
    fn from(err: super::super::error::CodecError) -> Self {
        WireError::SerializationFailed {
            type_name: "codec",
            cause: err.to_string(),
        }
    }
}

impl From<std::io::Error> for WireError {
    fn from(err: std::io::Error) -> Self {
        WireError::CompressionFailed {
            cause: err.to_string(),
        }
    }
}
