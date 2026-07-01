//! # Wire Protocol Constants and Invariants
//!
//! These constants define the binary contract between all Lumas processes.
//! Changing any of these values requires bumping `WIRE_VERSION_MAJOR`.

use std::ops::RangeInclusive;


// ── Identity ──────────────────────────────────────────────────────────────────

/// Wire protocol magic: "LUMI" in ASCII, big-endian.
pub const WIRE_MAGIC: u32 = 0x4C554D49;

/// Magic bytes as a byte array.
pub const WIRE_MAGIC_BYTES: [u8; 4] = [0x4C, 0x55, 0x4D, 0x49];

// ── Versions ──────────────────────────────────────────────────────────────────

/// Current major wire version.
pub const WIRE_VERSION_MAJOR: u8 = 1;

/// Current minor wire version.
pub const WIRE_VERSION_MINOR: u8 = 0;

/// Current header version (v1).
pub const HEADER_VERSION: u8 = 1;

// ── Fixed-Position Prefix Offsets (bytes 0-15, never change) ─────────────────

pub const OFFSET_MAGIC: usize          = 0;
pub const OFFSET_WIRE_VERSION: usize   = 4;
pub const OFFSET_HEADER_VERSION: usize = 5;
pub const OFFSET_FLAGS: usize          = 6;
pub const OFFSET_TOTAL_LENGTH: usize   = 8;
pub const OFFSET_PAYLOAD_LENGTH: usize = 12;

/// Size of the fixed-position prefix that never moves across versions.
pub const FIXED_PREFIX_SIZE: usize = 16;

// ── Header v1 Field Offsets (header_version == 1) ────────────────────────────

pub const OFFSET_MESSAGE_ID: usize      = 16;
pub const OFFSET_CORRELATION_ID: usize  = 32;
pub const OFFSET_SESSION_ID: usize      = 48;
pub const OFFSET_SENDER_ID: usize       = 56;
pub const OFFSET_RECEIVER_ID: usize     = 64;
pub const OFFSET_TIMESTAMP_US: usize    = 72;
pub const OFFSET_MESSAGE_KIND: usize    = 80;
pub const OFFSET_PRIORITY: usize        = 81;
pub const OFFSET_COMPRESSION: usize     = 82;
pub const OFFSET_ENCRYPTION: usize      = 83;
pub const OFFSET_SCHEMA_VERSION: usize  = 84;
pub const OFFSET_FRAGMENT_INDEX: usize  = 86;
pub const OFFSET_FRAGMENT_TOTAL: usize  = 88;
pub const OFFSET_FRAGMENT_ID: usize     = 90;
pub const OFFSET_CHECKSUM: usize        = 92;
pub const OFFSET_RESERVED: usize        = 96;

/// Total size of header v1 (104 bytes).
pub const HEADER_V1_SIZE: usize = 104;

// ── Frame Size Limits ─────────────────────────────────────────────────────────

/// Minimum valid frame size = header size (no payload is allowed for some kinds).
pub const MIN_FRAME_SIZE: usize = HEADER_V1_SIZE;

/// Absolute maximum frame size: 512KB.
pub const MAX_FRAME_SIZE: usize = 512 * 1024;

/// Default MTU for fragmentation (when channel MTU is not specified).
pub const DEFAULT_MTU: usize = 4096;

/// Maximum payload size for the shared memory transport (slot size).
pub const SHM_SLOT_SIZE: usize = 4096;

// ── Special IDs ───────────────────────────────────────────────────────────────

/// Receiver ID value representing broadcast to all subscribers.
pub const BROADCAST_RECEIVER: u64 = u64::MAX;

/// Receiver ID value representing no specific target (internal use).
pub const NO_TARGET: u64 = 0;

/// Sender ID value for unauthenticated/anonymous messages.
pub const ANONYMOUS_SENDER: u64 = 0;

// ── Flags Bit Positions ───────────────────────────────────────────────────────

pub const FLAG_COMPRESSED: u16       = 1 << 0;
pub const FLAG_ENCRYPTED: u16        = 1 << 1;
pub const FLAG_FRAGMENTED: u16       = 1 << 2;
pub const FLAG_STREAM: u16           = 1 << 3;
pub const FLAG_REQUIRES_ACK: u16     = 1 << 4;

/// Reserved flags mask: upper byte must be zero on send.
pub const FLAGS_RESERVED_MASK: u16   = 0xFF00;

// ── Supported Version Ranges ──────────────────────────────────────────────────

/// Range of wire versions this build can interoperate with.
pub const SUPPORTED_WIRE_VERSIONS: RangeInclusive<u8> = 1..=1;

/// Range of header versions this build can parse.
pub const SUPPORTED_HEADER_VERSIONS: RangeInclusive<u8> = 1..=1;

/// Current message schema version for MessagePack serialization.
pub const CURRENT_SCHEMA_VERSION: u16 = 1;

// ── Fragment Timeout ──────────────────────────────────────────────────────────

/// Maximum time to wait for all fragments of a message before GC.
pub const FRAGMENT_TIMEOUT_MS: u64 = 30_000;

/// Maximum concurrent in-flight reassemblies per sender.
pub const MAX_IN_FLIGHT_REASSEMBLIES: usize = 256;

/// Maximum size of a reassembled message (512KB).
pub const MAX_REASSEMBLED_SIZE: usize = MAX_FRAME_SIZE;

// ── Compression Defaults ──────────────────────────────────────────────────────

/// Default compression threshold: compress if payload > 512 bytes.
pub const DEFAULT_COMPRESSION_THRESHOLD: usize = 512;

/// Default zstd compression level (1 = fastest).
pub const DEFAULT_COMPRESSION_LEVEL: i32 = 1;

/// Maximum decompressed size allowed (512KB, DoS protection).
pub const MAX_DECOMPRESSED_SIZE: usize = MAX_FRAME_SIZE;

// ── Timestamp Skew ────────────────────────────────────────────────────────────

/// Maximum allowed clock skew between sender and receiver (60 seconds).
pub const MAX_TIMESTAMP_SKEW_SECS: u64 = 60;
