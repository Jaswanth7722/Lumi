//! # Header Encoding and Decoding
//!
//! Provides `Header` — the parsed, owned representation of a packet header.
//! Produced from raw bytes by `Header::parse()`; written to bytes by `Header::write()`.
//!
//! All parsing uses only safe array indexing with explicit bounds checks.
//! No `unsafe` pointer arithmetic is used.

use crate::wire::error::WireError;
use crate::wire::protocol::*;
use std::fmt;

/// The parsed, owned representation of a packet header.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    // Fixed prefix (always valid)
    pub wire_version: u8,
    pub header_version: u8,
    pub flags: Flags,
    pub total_length: u32,
    pub payload_length: u32,

    // Version-dependent fields (v1+)
    pub message_id: u128,
    pub correlation_id: u128,
    pub session_id: u64,
    pub sender_id: u64,
    pub receiver_id: u64,
    pub timestamp_us: u64,
    pub message_kind: u8,
    pub priority: u8,
    pub compression_type: u8,
    pub encryption_type: u8,
    pub schema_version: u16,
    pub fragment_index: u16,
    pub fragment_total: u16,
    pub fragment_id: u16,
    pub checksum: u32,
}

impl Header {
    /// Parse a header from the first `HEADER_V1_SIZE` bytes of a frame.
    ///
    /// # Errors
    /// - `TruncatedHeader` if the buffer is too short
    /// - `InvalidMagic` if the magic bytes don't match
    /// - `UnsupportedWireVersion` if the wire version is not supported
    /// - `UnsupportedHeaderVersion` if the header version is not supported
    ///
    /// # Panics
    /// Never panics, including on adversarial input.
    pub fn parse(bytes: &[u8]) -> Result<Self, WireError> {
        // Check minimum length
        if bytes.len() < HEADER_V1_SIZE {
            return Err(WireError::TruncatedHeader {
                available: bytes.len(),
                needed: HEADER_V1_SIZE,
            });
        }

        // 1. Validate magic (fastest rejection)
        let magic = u32::from_le_bytes(
            bytes.get(OFFSET_MAGIC..OFFSET_MAGIC + 4)
                .ok_or(WireError::TruncatedHeader {
                    available: bytes.len(),
                    needed: OFFSET_MAGIC + 4,
                })?
                .try_into()
                .map_err(|_| WireError::InvalidMagic {
                    found: 0,
                    expected: WIRE_MAGIC,
                })?,
        );

        if magic != WIRE_MAGIC {
            return Err(WireError::InvalidMagic {
                found: magic,
                expected: WIRE_MAGIC,
            });
        }

        // 2. Check wire version
        let wire_version = *bytes.get(OFFSET_WIRE_VERSION)
            .ok_or(WireError::TruncatedHeader {
                available: bytes.len(),
                needed: OFFSET_WIRE_VERSION + 1,
            })?;

        if !SUPPORTED_WIRE_VERSIONS.contains(&wire_version) {
            return Err(WireError::UnsupportedWireVersion {
                found: wire_version,
                supported: format!("{}-{}", SUPPORTED_WIRE_VERSIONS.start(), SUPPORTED_WIRE_VERSIONS.end()),
            });
        }

        // 3. Check header version
        let header_version = *bytes.get(OFFSET_HEADER_VERSION)
            .ok_or(WireError::TruncatedHeader {
                available: bytes.len(),
                needed: OFFSET_HEADER_VERSION + 1,
            })?;

        if !SUPPORTED_HEADER_VERSIONS.contains(&header_version) {
            return Err(WireError::UnsupportedHeaderVersion {
                found: header_version,
                supported: format!("{}-{}", SUPPORTED_HEADER_VERSIONS.start(), SUPPORTED_HEADER_VERSIONS.end()),
            });
        }

        // 4. Parse total length
        let total_length = u32::from_le_bytes(
            bytes.get(OFFSET_TOTAL_LENGTH..OFFSET_TOTAL_LENGTH + 4)
                .ok_or(WireError::TruncatedHeader {
                    available: bytes.len(),
                    needed: OFFSET_TOTAL_LENGTH + 4,
                })?
                .try_into().unwrap(),
        );

        if total_length > MAX_FRAME_SIZE as u32 {
            return Err(WireError::OversizedFrame {
                size: total_length,
                limit: MAX_FRAME_SIZE as u32,
            });
        }

        // 5. Parse payload length
        let payload_length = u32::from_le_bytes(
            bytes.get(OFFSET_PAYLOAD_LENGTH..OFFSET_PAYLOAD_LENGTH + 4)
                .ok_or(WireError::TruncatedHeader {
                    available: bytes.len(),
                    needed: OFFSET_PAYLOAD_LENGTH + 4,
                })?
                .try_into().unwrap(),
        );

        if payload_length > total_length.saturating_sub(HEADER_V1_SIZE as u32) {
            return Err(WireError::PayloadExceedsFrame {
                payload: payload_length,
                total: total_length,
            });
        }

        // 6. Parse flags
        let flags = u16::from_le_bytes(
            bytes.get(OFFSET_FLAGS..OFFSET_FLAGS + 2)
                .ok_or(WireError::TruncatedHeader {
                    available: bytes.len(),
                    needed: OFFSET_FLAGS + 2,
                })?
                .try_into().unwrap(),
        );

        let flags = Flags(flags);
        if !flags.reserved_bits_clear() {
            return Err(WireError::ReservedFlagsSet { flags: flags.0 });
        }

        // ── header_version=1 fields ──────────────────────────────────────────
        if header_version == 1 {
            let message_id = u128::from_le_bytes(
                bytes.get(OFFSET_MESSAGE_ID..OFFSET_MESSAGE_ID + 16)
                    .ok_or(WireError::TruncatedHeader {
                        available: bytes.len(),
                        needed: OFFSET_MESSAGE_ID + 16,
                    })?
                    .try_into().unwrap(),
            );

            if message_id == 0 {
                return Err(WireError::MissingMessageId);
            }

            let correlation_id = u128::from_le_bytes(
                bytes.get(OFFSET_CORRELATION_ID..OFFSET_CORRELATION_ID + 16)
                    .ok_or(WireError::TruncatedHeader {
                        available: bytes.len(),
                        needed: OFFSET_CORRELATION_ID + 16,
                    })?
                    .try_into().unwrap(),
            );

            let session_id = u64::from_le_bytes(
                bytes.get(OFFSET_SESSION_ID..OFFSET_SESSION_ID + 8)
                    .ok_or(WireError::TruncatedHeader {
                        available: bytes.len(),
                        needed: OFFSET_SESSION_ID + 8,
                    })?
                    .try_into().unwrap(),
            );

            let sender_id = u64::from_le_bytes(
                bytes.get(OFFSET_SENDER_ID..OFFSET_SENDER_ID + 8)
                    .ok_or(WireError::TruncatedHeader {
                        available: bytes.len(),
                        needed: OFFSET_SENDER_ID + 8,
                    })?
                    .try_into().unwrap(),
            );

            let receiver_id = u64::from_le_bytes(
                bytes.get(OFFSET_RECEIVER_ID..OFFSET_RECEIVER_ID + 8)
                    .ok_or(WireError::TruncatedHeader {
                        available: bytes.len(),
                        needed: OFFSET_RECEIVER_ID + 8,
                    })?
                    .try_into().unwrap(),
            );

            let timestamp_us = u64::from_le_bytes(
                bytes.get(OFFSET_TIMESTAMP_US..OFFSET_TIMESTAMP_US + 8)
                    .ok_or(WireError::TruncatedHeader {
                        available: bytes.len(),
                        needed: OFFSET_TIMESTAMP_US + 8,
                    })?
                    .try_into().unwrap(),
            );

            let message_kind = *bytes.get(OFFSET_MESSAGE_KIND)
                .ok_or(WireError::TruncatedHeader {
                    available: bytes.len(),
                    needed: OFFSET_MESSAGE_KIND + 1,
                })?;

            let priority = *bytes.get(OFFSET_PRIORITY)
                .ok_or(WireError::TruncatedHeader {
                    available: bytes.len(),
                    needed: OFFSET_PRIORITY + 1,
                })?;

            let compression_type = *bytes.get(OFFSET_COMPRESSION)
                .ok_or(WireError::TruncatedHeader {
                    available: bytes.len(),
                    needed: OFFSET_COMPRESSION + 1,
                })?;

            let encryption_type = *bytes.get(OFFSET_ENCRYPTION)
                .ok_or(WireError::TruncatedHeader {
                    available: bytes.len(),
                    needed: OFFSET_ENCRYPTION + 1,
                })?;

            let schema_version = u16::from_le_bytes(
                bytes.get(OFFSET_SCHEMA_VERSION..OFFSET_SCHEMA_VERSION + 2)
                    .ok_or(WireError::TruncatedHeader {
                        available: bytes.len(),
                        needed: OFFSET_SCHEMA_VERSION + 2,
                    })?
                    .try_into().unwrap(),
            );

            let fragment_index = u16::from_le_bytes(
                bytes.get(OFFSET_FRAGMENT_INDEX..OFFSET_FRAGMENT_INDEX + 2)
                    .ok_or(WireError::TruncatedHeader {
                        available: bytes.len(),
                        needed: OFFSET_FRAGMENT_INDEX + 2,
                    })?
                    .try_into().unwrap(),
            );

            let fragment_total = u16::from_le_bytes(
                bytes.get(OFFSET_FRAGMENT_TOTAL..OFFSET_FRAGMENT_TOTAL + 2)
                    .ok_or(WireError::TruncatedHeader {
                        available: bytes.len(),
                        needed: OFFSET_FRAGMENT_TOTAL + 2,
                    })?
                    .try_into().unwrap(),
            );

            let fragment_id = u16::from_le_bytes(
                bytes.get(OFFSET_FRAGMENT_ID..OFFSET_FRAGMENT_ID + 2)
                    .ok_or(WireError::TruncatedHeader {
                        available: bytes.len(),
                        needed: OFFSET_FRAGMENT_ID + 2,
                    })?
                    .try_into().unwrap(),
            );

            let checksum = u32::from_le_bytes(
                bytes.get(OFFSET_CHECKSUM..OFFSET_CHECKSUM + 4)
                    .ok_or(WireError::TruncatedHeader {
                        available: bytes.len(),
                        needed: OFFSET_CHECKSUM + 4,
                    })?
                    .try_into().unwrap(),
            );

            Ok(Self {
                wire_version,
                header_version,
                flags,
                total_length,
                payload_length,
                message_id,
                correlation_id,
                session_id,
                sender_id,
                receiver_id,
                timestamp_us,
                message_kind,
                priority,
                compression_type,
                encryption_type,
                schema_version,
                fragment_index,
                fragment_total,
                fragment_id,
                checksum,
            })
        } else {
            Err(WireError::UnsupportedHeaderVersion {
                found: header_version,
                supported: format!("{}-{}", SUPPORTED_HEADER_VERSIONS.start(), SUPPORTED_HEADER_VERSIONS.end()),
            })
        }
    }

    /// Write the header into `dst` (must be exactly `HEADER_V1_SIZE` bytes).
    /// Sets the checksum field to zero; caller must fill it after computing.
    ///
    /// # Panics
    /// Never panics. Returns `Err` if `dst` is too short.
    pub fn write(&self, dst: &mut [u8]) -> Result<(), WireError> {
        if dst.len() < HEADER_V1_SIZE {
            return Err(WireError::TruncatedHeader {
                available: dst.len(),
                needed: HEADER_V1_SIZE,
            });
        }

        // Fixed prefix
        dst[OFFSET_MAGIC..OFFSET_MAGIC + 4].copy_from_slice(&WIRE_MAGIC.to_le_bytes());
        dst[OFFSET_WIRE_VERSION] = self.wire_version;
        dst[OFFSET_HEADER_VERSION] = self.header_version;
        dst[OFFSET_FLAGS..OFFSET_FLAGS + 2].copy_from_slice(&self.flags.0.to_le_bytes());
        dst[OFFSET_TOTAL_LENGTH..OFFSET_TOTAL_LENGTH + 4].copy_from_slice(&self.total_length.to_le_bytes());
        dst[OFFSET_PAYLOAD_LENGTH..OFFSET_PAYLOAD_LENGTH + 4].copy_from_slice(&self.payload_length.to_le_bytes());

        // Version 1 fields
        dst[OFFSET_MESSAGE_ID..OFFSET_MESSAGE_ID + 16].copy_from_slice(&self.message_id.to_le_bytes());
        dst[OFFSET_CORRELATION_ID..OFFSET_CORRELATION_ID + 16].copy_from_slice(&self.correlation_id.to_le_bytes());
        dst[OFFSET_SESSION_ID..OFFSET_SESSION_ID + 8].copy_from_slice(&self.session_id.to_le_bytes());
        dst[OFFSET_SENDER_ID..OFFSET_SENDER_ID + 8].copy_from_slice(&self.sender_id.to_le_bytes());
        dst[OFFSET_RECEIVER_ID..OFFSET_RECEIVER_ID + 8].copy_from_slice(&self.receiver_id.to_le_bytes());
        dst[OFFSET_TIMESTAMP_US..OFFSET_TIMESTAMP_US + 8].copy_from_slice(&self.timestamp_us.to_le_bytes());
        dst[OFFSET_MESSAGE_KIND] = self.message_kind;
        dst[OFFSET_PRIORITY] = self.priority;
        dst[OFFSET_COMPRESSION] = self.compression_type;
        dst[OFFSET_ENCRYPTION] = self.encryption_type;
        dst[OFFSET_SCHEMA_VERSION..OFFSET_SCHEMA_VERSION + 2].copy_from_slice(&self.schema_version.to_le_bytes());
        dst[OFFSET_FRAGMENT_INDEX..OFFSET_FRAGMENT_INDEX + 2].copy_from_slice(&self.fragment_index.to_le_bytes());
        dst[OFFSET_FRAGMENT_TOTAL..OFFSET_FRAGMENT_TOTAL + 2].copy_from_slice(&self.fragment_total.to_le_bytes());
        dst[OFFSET_FRAGMENT_ID..OFFSET_FRAGMENT_ID + 2].copy_from_slice(&self.fragment_id.to_le_bytes());
        // Checksum is zero during computation
        dst[OFFSET_CHECKSUM..OFFSET_CHECKSUM + 4].copy_from_slice(&0u32.to_le_bytes());
        // Reserved bytes
        dst[OFFSET_RESERVED..OFFSET_RESERVED + 8].copy_from_slice(&0u64.to_le_bytes());

        Ok(())
    }

    /// Verify the checksum covers this header (bytes 0..OFFSET_CHECKSUM) plus `payload`.
    pub fn verify_checksum(&self, raw_header: &[u8], payload: &[u8]) -> Result<(), WireError> {
        let expected = self.checksum;
        // Compute checksum over bytes 0..OFFSET_CHECKSUM (92 bytes) with checksum field zeroed
        let header_prefix = &raw_header[0..OFFSET_CHECKSUM];
        let actual = super::checksum::ChecksumEngine::compute(header_prefix, payload);

        if expected != actual {
            return Err(WireError::ChecksumMismatch { expected, actual });
        }
        Ok(())
    }

    /// Check if the message is fragmented.
    pub fn is_fragmented(&self) -> bool {
        self.flags.is_fragmented()
    }

    /// Check if the message is compressed.
    pub fn is_compressed(&self) -> bool {
        self.flags.is_compressed()
    }

    /// Check if the message is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.flags.is_encrypted()
    }

    /// Create a builder-style header for encoding.
    pub fn builder() -> HeaderBuilder {
        HeaderBuilder::new()
    }
}

/// Builder for constructing Headers with a fluent API.
pub struct HeaderBuilder {
    wire_version: u8,
    header_version: u8,
    flags: Flags,
    total_length: u32,
    payload_length: u32,
    message_id: u128,
    correlation_id: u128,
    session_id: u64,
    sender_id: u64,
    receiver_id: u64,
    timestamp_us: u64,
    message_kind: u8,
    priority: u8,
    compression_type: u8,
    encryption_type: u8,
    schema_version: u16,
    fragment_index: u16,
    fragment_total: u16,
    fragment_id: u16,
}

impl HeaderBuilder {
    pub fn new() -> Self {
        Self {
            wire_version: WIRE_VERSION_MAJOR,
            header_version: HEADER_VERSION,
            flags: Flags(0),
            total_length: HEADER_V1_SIZE as u32,
            payload_length: 0,
            message_id: 0,
            correlation_id: 0,
            session_id: 0,
            sender_id: 0,
            receiver_id: 0,
            timestamp_us: 0,
            message_kind: 0,
            priority: 0,
            compression_type: 0,
            encryption_type: 0,
            schema_version: CURRENT_SCHEMA_VERSION,
            fragment_index: 0,
            fragment_total: 1,
            fragment_id: 0,
        }
    }

    pub fn message_id(mut self, id: u128) -> Self { self.message_id = id; self }
    pub fn correlation_id(mut self, id: u128) -> Self { self.correlation_id = id; self }
    pub fn session_id(mut self, id: u64) -> Self { self.session_id = id; self }
    pub fn sender(mut self, id: u64) -> Self { self.sender_id = id; self }
    pub fn receiver(mut self, id: u64) -> Self { self.receiver_id = id; self }
    pub fn timestamp(mut self, ts: u64) -> Self { self.timestamp_us = ts; self }
    pub fn kind(mut self, kind: u8) -> Self { self.message_kind = kind; self }
    pub fn priority(mut self, p: u8) -> Self { self.priority = p; self }
    pub fn compression(mut self, c: u8) -> Self { self.compression_type = c; self }
    pub fn encryption(mut self, e: u8) -> Self { self.encryption_type = e; self }
    pub fn schema_version(mut self, v: u16) -> Self { self.schema_version = v; self }
    pub fn fragment(mut self, index: u16, total: u16, id: u16) -> Self {
        self.fragment_index = index;
        self.fragment_total = total;
        self.fragment_id = id;
        if total > 1 { self.flags = self.flags.with_fragmented(); }
        self
    }
    pub fn stream(mut self) -> Self { self.flags = self.flags.with_stream(); self }
    pub fn requires_ack(mut self) -> Self { self.flags = self.flags.with_requires_ack(); self }

    pub fn payload_length(mut self, len: u32) -> Self {
        self.payload_length = len;
        self.total_length = HEADER_V1_SIZE as u32 + len;
        self
    }

    pub fn build(self) -> Header {
        Header {
            wire_version: self.wire_version,
            header_version: self.header_version,
            flags: self.flags,
            total_length: self.total_length,
            payload_length: self.payload_length,
            message_id: self.message_id,
            correlation_id: self.correlation_id,
            session_id: self.session_id,
            sender_id: self.sender_id,
            receiver_id: self.receiver_id,
            timestamp_us: self.timestamp_us,
            message_kind: self.message_kind,
            priority: self.priority,
            compression_type: self.compression_type,
            encryption_type: self.encryption_type,
            schema_version: self.schema_version,
            fragment_index: self.fragment_index,
            fragment_total: self.fragment_total,
            fragment_id: self.fragment_id,
            checksum: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

/// Wire frame flags as a bit field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flags(pub u16);

impl Flags {
    pub const fn empty() -> Self { Flags(0) }
    pub const fn all() -> Self { Flags(0x1F) }

    pub fn is_compressed(self) -> bool { self.0 & FLAG_COMPRESSED != 0 }
    pub fn is_encrypted(self) -> bool { self.0 & FLAG_ENCRYPTED != 0 }
    pub fn is_fragmented(self) -> bool { self.0 & FLAG_FRAGMENTED != 0 }
    pub fn is_stream(self) -> bool { self.0 & FLAG_STREAM != 0 }
    pub fn requires_ack(self) -> bool { self.0 & FLAG_REQUIRES_ACK != 0 }
    pub fn reserved_bits_clear(self) -> bool { self.0 & FLAGS_RESERVED_MASK == 0 }

    pub fn with_compressed(mut self) -> Self { self.0 |= FLAG_COMPRESSED; self }
    pub fn with_encrypted(mut self) -> Self { self.0 |= FLAG_ENCRYPTED; self }
    pub fn with_fragmented(mut self) -> Self { self.0 |= FLAG_FRAGMENTED; self }
    pub fn with_stream(mut self) -> Self { self.0 |= FLAG_STREAM; self }
    pub fn with_requires_ack(mut self) -> Self { self.0 |= FLAG_REQUIRES_ACK; self }
}

impl fmt::Display for Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#06x}", self.0)
    }
}
