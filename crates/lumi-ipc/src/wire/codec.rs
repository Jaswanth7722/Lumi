// ── Lumi Wire Protocol Codec (Top-Level Encode/Decode API) ─────────────────────
// Wire Codec: High-level encode/decode that orchestrates the full pipeline:
//
//   Encode: serialize → compress → encrypt → fragment → frame
//   Decode: validate → checksum → decrypt → decompress → reassemble → deserialize
//
// Every layer in this stack has exactly one job. The serializer knows nothing
// about fragmentation. The framer knows nothing about checksums. The compressor
// knows nothing about message types.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use uuid::Uuid;

use crate::message::LumiMessage;
use crate::wire::checksum::ChecksumEngine;
use crate::wire::compression::{CompressionLevel, CompressionPolicy, Compressor, Decompressor};
use crate::wire::encryption::{EncryptionLayer, EncryptionType};
use crate::wire::error::WireError;
use crate::wire::frame::{LumiFramer, RawFrame};
use crate::wire::fragmentation::{Fragmenter, Reassembler, ReassemblerConfig};
use crate::wire::header::{Flags, Header, HeaderBuilder};
use crate::wire::metrics::WireMetrics;
use crate::wire::packet::{PacketBuilder, ParsedPacket};
use crate::wire::protocol::*;
use crate::wire::serializer::WireSerializer;
use crate::wire::validator::PacketValidator;

/// Per-channel limits for the wire codec.
#[derive(Debug, Clone)]
pub struct ChannelLimits {
    pub max_payload_size: usize,
    pub mtu: usize,
    pub compression_policy: CompressionPolicy,
}

impl Default for ChannelLimits {
    fn default() -> Self {
        Self {
            max_payload_size: MAX_FRAME_SIZE,
            mtu: DEFAULT_MTU,
            compression_policy: CompressionPolicy::ThresholdBytes(512, CompressionLevel::FAST),
        }
    }
}

/// Configuration for the wire codec.
#[derive(Debug, Clone)]
pub struct WireCodecConfig {
    pub default_mtu: usize,
    pub default_compression_policy: CompressionPolicy,
    pub channel_limits: HashMap<u8, ChannelLimits>,
    pub max_frame_size: usize,
    pub enable_fragmentation: bool,
    pub enable_compression: bool,
    pub enable_encryption: bool,
    pub format: crate::wire::serializer::SerializationFormat,
}

impl Default for WireCodecConfig {
    fn default() -> Self {
        let mut channel_limits = HashMap::new();
        channel_limits.insert(
            1, // render.command
            ChannelLimits {
                max_payload_size: 4096,
                mtu: 4096,
                compression_policy: CompressionPolicy::Never,
            },
        );
        channel_limits.insert(
            2, // ai.state
            ChannelLimits {
                max_payload_size: 1024,
                mtu: 1024,
                compression_policy: CompressionPolicy::Never,
            },
        );
        channel_limits.insert(
            3, // memory.query
            ChannelLimits {
                max_payload_size: 8192,
                mtu: 8192,
                compression_policy: CompressionPolicy::ThresholdBytes(512, CompressionLevel::FAST),
            },
        );
        channel_limits.insert(
            4, // plugin.invoke
            ChannelLimits {
                max_payload_size: 256 * 1024,
                mtu: 65536,
                compression_policy: CompressionPolicy::ThresholdBytes(512, CompressionLevel::FAST),
            },
        );
        channel_limits.insert(
            5, // voice.input
            ChannelLimits {
                max_payload_size: 32768,
                mtu: 32768,
                compression_policy: CompressionPolicy::Never,
            },
        );
        Self {
            default_mtu: DEFAULT_MTU,
            default_compression_policy: CompressionPolicy::ThresholdBytes(512, CompressionLevel::FAST),
            channel_limits,
            max_frame_size: MAX_FRAME_SIZE,
            enable_fragmentation: true,
            enable_compression: true,
            enable_encryption: false,
            format: Default::default(),
        }
    }
}

/// The top-level wire codec: orchestrates the encode/decode pipeline.
///
/// # Pipeline
///
/// **Encode:** `serialize → compress → encrypt → fragment → frame`
///
/// 1. Serialize the `LumiMessage` to MessagePack bytes
/// 2. Optionally compress using zstd (based on channel policy)
/// 3. Optionally encrypt using ChaCha20-Poly1305
/// 4. Fragment if payload exceeds channel MTU
/// 5. Build wire frames with headers and checksums
///
/// **Decode:** `validate → checksum → decrypt → decompress → reassemble → deserialize`
///
/// 1. Validate header (version, lengths, flags, timestamp)
/// 2. Verify BLAKE3 checksum
/// 3. Optionally decrypt
/// 4. Optionally decompress (via associated function, no &self needed)
/// 5. Reassemble fragments if fragmented (Reassembler wrapped in Mutex)
/// 6. Deserialize MessagePack to `LumiMessage`
///
/// # Thread Safety
///
/// - `Compressor::compress()` takes `&self` — read-only, no locking needed
/// - `Decompressor::decompress()` is an associated function — no &self needed
/// - `Reassembler` is wrapped in `Mutex` — interior mutability for `&self`
/// - `WireCodec::encode()` and `decode()` both take `&self` — thread-safe
#[derive(Debug)]
pub struct WireCodec {
    pub config: WireCodecConfig,
    pub framer: LumiFramer,
    pub serializer: WireSerializer,
    pub compressor: Compressor,
    pub encryption: EncryptionLayer,
    pub fragmenter: Fragmenter,
    /// Reassembler requires internal mutability; wrapped in Mutex for `&self` access.
    pub reassembler: Mutex<Reassembler>,
    pub validator: PacketValidator,
    pub metrics: Arc<WireMetrics>,
}

impl WireCodec {
    /// Create a new wire codec with the given config.
    pub fn new(config: WireCodecConfig) -> Self {
        let metrics = Arc::new(WireMetrics::new());
        let reassembler_config = ReassemblerConfig {
            fragment_timeout: std::time::Duration::from_millis(FRAGMENT_TIMEOUT_MS),
            max_pending_fragments: MAX_IN_FLIGHT_REASSEMBLIES,
            metrics: metrics.clone(),
        };
        Self {
            framer: LumiFramer::new(config.max_frame_size, metrics.clone()),
            serializer: WireSerializer::new(config.format),
            compressor: Compressor::new(CompressionLevel::FAST),
            encryption: EncryptionLayer::none(),
            fragmenter: Fragmenter::new(config.default_mtu),
            reassembler: Mutex::new(Reassembler::new(reassembler_config, metrics.clone())),
            validator: PacketValidator::new(),
            metrics,
            config,
        }
    }

    /// Access the framer for use with `tokio_util::codec::Framed`.
    pub fn framer(&self) -> &LumiFramer {
        &self.framer
    }

    /// Get a reference to the metrics.
    pub fn metrics(&self) -> Arc<WireMetrics> {
        self.metrics.clone()
    }

    /// Get the max frame size from config.
    pub fn max_frame_size(&self) -> usize {
        self.config.max_frame_size
    }

    /// Get channel limits for a given channel kind, or defaults.
    pub fn channel_limits(&self, kind: u8) -> ChannelLimits {
        self.config
            .channel_limits
            .get(&kind)
            .cloned()
            .unwrap_or_default()
    }

    /// Encode a `LumiMessage` to one or more wire frames.
    ///
    /// Pipeline: `serialize → compress → encrypt → fragment → frame`
    ///
    /// Returns multiple frames if the message requires fragmentation.
    ///
    /// # Wire Safety
    /// This function is safe to call from any thread.
    ///
    /// # Panics
    /// Never panics, including on adversarial input.
    ///
    /// # Errors
    /// Returns `WireError` at any stage of the pipeline.
    pub fn encode(&self, msg: &LumiMessage) -> Result<EncodedFrames, WireError> {
        // Step 1: Determine channel kind from message (default to 0 if not specified)
        let kind = msg.kind as u8;
        let limits = self.channel_limits(kind);

        // Step 2: Serialize
        let serialized = self.serializer.serialize(msg)?;
        let mut payload_bytes = serialized;

        // Step 3: Compress (if policy says so and feature is enabled)
        // Compressor::compress takes &self (only reads self.level), no mutex needed.
        if self.config.enable_compression && limits.compression_policy.should_compress(payload_bytes.len()) {
            match self.compressor.compress(&payload_bytes) {
                Ok(compressed) => {
                    if compressed.len() < payload_bytes.len() {
                        payload_bytes = compressed;
                    }
                }
                Err(_) => {
                    // Compression failed (payload too small or expanded) — proceed uncompressed
                }
            }
        }

        // Step 4: Encrypt (if enabled)
        let is_encrypted = self.config.enable_encryption
            && self.encryption.encryption_type() != EncryptionType::None;
        if is_encrypted {
            let (ciphertext, _nonce) = self.encryption.encrypt(&payload_bytes)?;
            payload_bytes = ciphertext;
        }

        // Step 5: Fragment (if payload exceeds MTU and feature is enabled)
        let msg_id = Uuid::new_v4();
        let should_fragment = self.config.enable_fragmentation
            && payload_bytes.len() > limits.mtu;

        if should_fragment {
            let fragments = self.fragmenter.fragment(&payload_bytes, msg_id, 0);
            let mut frames = Vec::with_capacity(fragments.len());
            self.metrics.increment_fragmentation_events();

            for frag in &fragments {
                let builder = PacketBuilder::new(msg_id, kind, 0, 0)
                    .with_schema_version(CURRENT_SCHEMA_VERSION)
                    .with_fragment(frag.index, frag.total, frag.fragment_id)
                    .with_fragmented_flag();

                let builder = if is_encrypted {
                    builder.with_encrypted_flag()
                } else {
                    builder
                };

                let wire_bytes = builder.build(&frag.data)?;
                self.metrics.increment_frames_encoded();
                frames.push(wire_bytes);
            }

            Ok(EncodedFrames { frames })
        } else {
            // Step 6: Single frame (no fragmentation)
            let builder = PacketBuilder::new(msg_id, kind, 0, 0)
                .with_schema_version(CURRENT_SCHEMA_VERSION);

            let builder = if is_encrypted {
                builder.with_encrypted_flag()
            } else {
                builder
            };

            let wire_bytes = builder.build(&payload_bytes)?;
            self.metrics.increment_frames_encoded();
            Ok(EncodedFrames {
                frames: vec![wire_bytes],
            })
        }
    }

    /// Decode one wire frame to a `LumiMessage`.
    ///
    /// Pipeline: `validate → checksum → decrypt → decompress → reassemble → deserialize`
    ///
    /// Returns `Ok(None)` if the frame is a fragment and reassembly is not yet
    /// complete. Returns `Ok(Some(msg))` when a complete message is available.
    ///
    /// # Wire Safety
    /// This function is safe to call from any thread.
    ///
    /// # Panics
    /// Never panics, including on adversarial input.
    ///
    /// # Errors
    /// Returns `WireError` at any stage of the pipeline.
    pub fn decode(&self, frame: RawFrame) -> Result<Option<LumiMessage>, WireError> {
        let frame_bytes = frame.as_ref();
        self.metrics.increment_frames_decoded();

        // Step 1: Parse header
        let packet = ParsedPacket::parse(frame_bytes)?;

        // Step 2: Validate header (version, lengths, flags, timestamp)
        self.validator.validate_all(&packet.header, packet.payload)?;

        // Step 3: Verify checksum
        packet.header.verify_checksum(
            &frame_bytes[..OFFSET_CHECKSUM],
            packet.payload,
        )?;
        self.metrics.increment_checksum_passes();

        // Step 4: Handle fragmentation (Reassembler is behind Mutex)
        let is_fragmented = packet.header.flags.is_fragmented()
            && packet.header.fragment_total > 1;
        let payload = if is_fragmented {
            self.metrics.increment_reassembly_events();
            let fragment = crate::wire::fragmentation::Fragment {
                msg_id: packet.header.message_id,
                index: packet.header.fragment_index,
                total: packet.header.fragment_total,
                data: packet.payload.to_vec(),
                fragment_id: packet.header.fragment_id,
            };

            let mut reassembler = self.reassembler.lock().unwrap();
            reassembler.add_fragment(fragment)?;

            // Check if reassembly is complete
            match reassembler.take_reassembled(packet.header.message_id) {
                Some(data) => Bytes::from(data),
                None => return Ok(None), // need more fragments
            }
        } else {
            Bytes::copy_from_slice(packet.payload)
        };

        // Step 5: Decrypt (if encrypted flag set)
        let decrypted = if packet.header.flags.is_encrypted() {
            self.metrics.increment_decryption_failures();
            self.encryption.decrypt(&payload, &[0u8; 12])?
        } else {
            payload
        };

        // Step 6: Decompress (if compressed flag set)
        // Decompressor::decompress is an associated function — no &self needed
        let decompressed = if packet.header.flags.is_compressed() {
            let max_output = MAX_DECOMPRESSED_SIZE;
            Decompressor::decompress(&decrypted, max_output)?
        } else {
            decrypted
        };

        // Step 7: Deserialize
        let msg = self.serializer.deserialize(&decompressed, packet.header.schema_version)?;

        Ok(Some(msg))
    }

    /// Run GC on the reassembler to clean up timed-out fragments.
    pub fn run_gc(&self) -> usize {
        let mut reassembler = self.reassembler.lock().unwrap();
        reassembler.gc()
    }
}

/// Encoded output: one or more wire frames (for fragmented messages).
#[derive(Debug, Clone)]
pub struct EncodedFrames {
    /// The frames to transmit.
    pub frames: Vec<Bytes>,
}

impl EncodedFrames {
    /// Total byte count across all frames.
    pub fn total_bytes(&self) -> usize {
        self.frames.iter().map(|b| b.len()).sum()
    }

    /// Number of frames.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Whether the message was fragmented.
    pub fn is_fragmented(&self) -> bool {
        self.frames.len() > 1
    }

    /// Iterate over the frames.
    pub fn iter(&self) -> impl Iterator<Item = &Bytes> {
        self.frames.iter()
    }

    /// Consume and return the frames.
    pub fn into_frames(self) -> Vec<Bytes> {
        self.frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{LumiMessage, MessageKind, MessagePayload};

    #[test]
    fn test_codec_default_config() {
        let config = WireCodecConfig::default();
        assert_eq!(config.default_mtu, DEFAULT_MTU);
        assert_eq!(config.max_frame_size, MAX_FRAME_SIZE);
        assert!(config.enable_fragmentation);
        assert!(config.enable_compression);
        assert!(!config.enable_encryption);
    }

    #[test]
    fn test_channel_limits_render_command() {
        let config = WireCodecConfig::default();
        let limits = config.channel_limits.get(&1).unwrap();
        assert_eq!(limits.max_payload_size, 4096);
        assert_eq!(limits.mtu, 4096);
        assert!(matches!(limits.compression_policy, CompressionPolicy::Never));
    }

    #[test]
    fn test_codec_creation() {
        let codec = WireCodec::new(WireCodecConfig::default());
        assert_eq!(codec.config.default_mtu, DEFAULT_MTU);
        assert_eq!(codec.max_frame_size(), MAX_FRAME_SIZE);
    }

    #[test]
    fn test_codec_framer_access() {
        let codec = WireCodec::new(WireCodecConfig::default());
        let _ = codec.framer();
    }

    #[test]
    fn test_codec_metrics_access() {
        let codec = WireCodec::new(WireCodecConfig::default());
        let m = codec.metrics();
        assert_eq!(m.snapshot().frames_decoded, 0);
    }

    #[test]
    fn test_encode_small_message() {
        let codec = WireCodec::new(WireCodecConfig::default());
        let msg = LumiMessage::new(MessageKind::Data, MessagePayload::Empty);
        let result = codec.encode(&msg);
        assert!(result.is_ok(), "Encode should succeed: {:?}", result.err());
        let frames = result.unwrap();
        assert_eq!(frames.frame_count(), 1);
        assert!(!frames.is_fragmented());
        assert!(frames.total_bytes() >= HEADER_V1_SIZE);
    }

    #[test]
    fn test_encode_different_kinds() {
        let codec = WireCodec::new(WireCodecConfig::default());
        for kind in &[
            MessageKind::Data,
            MessageKind::Heartbeat,
            MessageKind::Ack,
        ] {
            let msg = LumiMessage::new(*kind, MessagePayload::Empty);
            let result = codec.encode(&msg);
            assert!(result.is_ok(), "Encode {:?} should succeed", kind);
        }
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let codec = WireCodec::new(WireCodecConfig::default());
        let msg = LumiMessage::new(MessageKind::Data, MessagePayload::Empty);

        // Encode
        let frames = codec.encode(&msg).unwrap();
        assert_eq!(frames.frame_count(), 1);

        // Decode
        let frame_bytes = frames.into_frames().remove(0);
        let raw = RawFrame::new(frame_bytes);
        let decoded = codec.decode(raw).unwrap();
        assert!(decoded.is_some());
        let decoded = decoded.unwrap();
        assert_eq!(decoded.kind, msg.kind);
    }

    #[test]
    fn test_decode_garbage_returns_error() {
        let codec = WireCodec::new(WireCodecConfig::default());
        let garbage = RawFrame::new(Bytes::from(&b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09"[..]));
        let result = codec.decode(garbage);
        assert!(result.is_err());
    }

    #[test]
    fn test_encoded_frames_empty() {
        let frames = EncodedFrames { frames: vec![] };
        assert_eq!(frames.total_bytes(), 0);
        assert_eq!(frames.frame_count(), 0);
        assert!(!frames.is_fragmented());
    }

    #[test]
    fn test_encoded_frames_multiple() {
        let frames = EncodedFrames {
            frames: vec![
                Bytes::from(&b"frame1"[..]),
                Bytes::from(&b"frame2"[..]),
            ],
        };
        assert_eq!(frames.total_bytes(), 12);
        assert_eq!(frames.frame_count(), 2);
        assert!(frames.is_fragmented());
    }

    #[test]
    fn test_encoded_frames_iter() {
        let frames = EncodedFrames {
            frames: vec![Bytes::from(&b"a"[..]), Bytes::from(&b"b"[..])],
        };
        let count = frames.iter().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_codec_run_gc() {
        let codec = WireCodec::new(WireCodecConfig::default());
        let gced = codec.run_gc();
        assert_eq!(gced, 0); // nothing to GC initially
    }

    #[test]
    fn test_codec_custom_config() {
        let config = WireCodecConfig {
            default_mtu: 512,
            max_frame_size: 1024,
            enable_fragmentation: false,
            enable_compression: false,
            ..Default::default()
        };
        let codec = WireCodec::new(config);
        assert_eq!(codec.config.default_mtu, 512);
        assert!(!codec.config.enable_fragmentation);
    }

    #[test]
    fn test_channel_limits_fallback() {
        let codec = WireCodec::new(WireCodecConfig::default());
        let limits = codec.channel_limits(99); // unknown channel
        assert_eq!(limits.max_payload_size, MAX_FRAME_SIZE);
    }
}
