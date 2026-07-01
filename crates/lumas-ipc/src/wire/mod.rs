//! # Lumas Wire Protocol — Binary Framing, Serialization, Security
//!
//! This module is the **lowest layer of Lumi's communication stack**. Every byte
//! that crosses a process boundary passes through this code. It is solely
//! responsible for: getting bytes onto the wire correctly, getting them off the
//! wire safely, and refusing to process anything that is malformed, corrupted,
//! or version-incompatible.
//!
//! ## Non-Negotiable Properties
//!
//! 1. **Panic-free on all inputs** — including adversarially crafted inputs.
//! 2. **Zero silent corruption** — every packet carries integrity data verified
//!    before payload bytes are handed to the caller.
//! 3. **Indefinite backward compatibility** — a Lumas process compiled today must
//!    decode packets produced by a Lumas process compiled years from now, provided
//!    the major wire version matches.
//!
//! ## Packet Layout (header_version=1)
//!
//! The first 16 bytes are **fixed forever** at their current offsets:
//!
//! ```text
//! Offset  Size  Field             Notes
//! ------  ----  -----             -----
//! 0       4     magic             0x4C554D49 ("LUMI"). First check. Reject if wrong.
//! 4       1     wire_version      Major wire protocol version. Reject if unsupported.
//! 5       1     header_version    Minor header version. Indicates optional header fields.
//! 6       2     flags             Bit field (compressed, encrypted, fragmented, stream, ack).
//! 8       4     total_length      Total frame length in bytes INCLUDING this header. Max: 512KB.
//! 12      4     payload_length    Compressed+encrypted payload length. ≤ total_length - header_size.
//! ```
//!
//! After byte 16, the header is **version-dependent** (header_version=1):
//!
//! ```text
//! Offset  Size  Field (header_version=1)
//! ------  ----  -----
//! 16      16    message_id        UUID v7
//! 32      16    correlation_id    UUID v7
//! 48      8     session_id        u64
//! 56      8     sender_id         ProcessId as u64
//! 64      8     receiver_id       ProcessId as u64, or 0xFFFFFFFFFFFFFFFF for broadcast
//! 72      8     timestamp_us      Unix timestamp, microseconds
//! 80      1     message_kind      MessageKind discriminant
//! 81      1     priority          MessagePriority as u8
//! 82      1     compression_type  CompressionType discriminant
//! 83      1     encryption_type   EncryptionType discriminant
//! 84      2     schema_version    Message schema version
//! 86      2     fragment_index    0 for non-fragmented; fragment number for fragmented
//! 88      2     fragment_total    1 for non-fragmented; total fragments for fragmented
//! 90      2     fragment_id       0 for non-fragmented; unique reassembly ID
//! 92      4     checksum          BLAKE3 truncated to 4 bytes over [0..92] ++ payload
//! 96      8     reserved          Must be zero on send; ignored on recv
//! ------
//! 104     total  Header size for header_version=1
//! ```
//!
//! ## Wire Stack (Encode Direction)
//!
//! ```text
//! LumiMessage (typed) → Serializer → Fragmenter → Compressor → Encryptor → Header Builder → Frame Encoder → wire bytes
//! ```
//!
//! ## Wire Stack (Decode Direction)
//!
//! ```text
//! wire bytes → Frame Decoder → Header Parser + Validator → Decryptor → Decompressor → Fragment Reassembler → Deserializer → LumiMessage
//! ```
//!
//! ## Compatibility Commitment
//!
//! | What Requires Major Version Bump | What Does Not |
//! |---|---|
//! | Changing the fixed-prefix layout (bytes 0-15) | Adding new optional header fields in the version-dependent area |
//! | Removing a field from header v1 | Adding a new message kind |
//! | Changing the checksum algorithm | Deprecating a message kind (keep in enum, mark deprecated) |
//! | Changing the serialization format | Adding new envelope fields as `Option<T>` |
//! | Changing the wire version negotiation rules | Changing the compression level defaults |
//!
//! ## Fuzz Targets
//!
//! | Target | File | What It Fuzzes |
//! |---|---|---|
//! | `fuzz_frame_decode` | `fuzz/fuzz_targets/frame_decode.rs` | Random bytes → FrameDecoder |
//! | `fuzz_packet_parse` | `fuzz/fuzz_targets/packet_parse.rs` | Random bytes → Packet::parse() |
//! | `fuzz_reassembly` | `fuzz/fuzz_targets/reassembly.rs` | Random fragment sequences |
//! | `fuzz_decompression` | `fuzz/fuzz_targets/decompression.rs` | Random bytes as zstd input |

pub mod protocol;
pub mod error;
pub mod header;
pub mod frame;
pub mod checksum;
pub mod compression;
pub mod encryption;
pub mod version;
pub mod validator;
pub mod serializer;
pub mod packet;
pub mod fragmentation;
pub mod streaming;
pub mod metrics;
pub mod diagnostics;
pub mod codec;

pub use error::WireError;
pub use header::Header;
pub use packet::{ParsedPacket, OwnedPacket, PacketBuilder};
pub use frame::{RawFrame, LumiFramer};
pub use protocol::*;
pub use codec::WireCodec;
pub use checksum::ChecksumEngine;
pub use version::{VersionNegotiator, WireCapabilities};
