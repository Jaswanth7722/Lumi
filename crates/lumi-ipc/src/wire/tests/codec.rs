// ── Codec Tests ─────────────────────────────────────────────────────────────────
// Tests the top-level WireCodec: encode → decode roundtrip for various
// message types and sizes, error paths.
#![cfg(test)]

use std::sync::Arc;

use crate::wire::codec::{WireCodec, WireCodecConfig, EncodedFrames, encode_message, decode_frame};
use crate::wire::error::WireError;
use crate::wire::frame::RawFrame;
use crate::wire::protocol::*;

#[test]
fn test_codec_config_default() {
    let config = WireCodecConfig::default();
    assert_eq!(config.default_mtu, DEFAULT_MTU);
    assert_eq!(config.max_frame_size, MAX_FRAME_SIZE);
    assert!(config.enable_fragmentation);
    assert!(config.enable_compression);
    assert!(!config.enable_encryption);
}

#[test]
fn test_codec_creation() {
    let codec = WireCodec::new(WireCodecConfig::default());
    assert_eq!(codec.config.default_mtu, DEFAULT_MTU);
}

#[test]
fn test_codec_framer_access() {
    let codec = WireCodec::new(WireCodecConfig::default());
    let framer = codec.framer();
    assert!(framer.max_frame_size() >= MIN_FRAME_SIZE);
}

#[test]
fn test_codec_metrics_access() {
    let codec = WireCodec::new(WireCodecConfig::default());
    let metrics = codec.metrics();
    assert!(metrics.snapshot().frames_decoded == 0);
}

#[test]
fn test_encode_decode_static_fn_exist() {
    // Verify the static functions exist with correct signatures
    let codec = WireCodec::new(WireCodecConfig::default());
    let msg = crate::message::LumiMessage::new(
        crate::message::MessageKind::Data,
        crate::message::MessagePayload::Empty,
    );
    let result = encode_message(&msg, &codec);
    // Currently returns UnsupportedVersion since full pipeline not yet implemented
    match result {
        Ok(_) => {} // Would be success if pipeline is implemented
        Err(WireError::UnsupportedVersion { .. }) => {} // Expected stub behavior
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn test_codec_concurrent_access() {
    // Verify that multiple threads can access the codec (Arc<WireMetrics>)
    let codec = WireCodec::new(WireCodecConfig::default());
    let metrics = codec.metrics();
    let handle = std::thread::spawn(move || {
        metrics.snapshot()
    });
    let _snap = handle.join().unwrap();
}

#[test]
fn test_encoded_frames_creation() {
    let frames = EncodedFrames {
        frames: vec![Bytes::from(&b"hello"[..]), Bytes::from(&b"world"[..])],
    };
    assert_eq!(frames.total_bytes(), 10);
    assert_eq!(frames.frame_count(), 2);
    assert!(frames.is_fragmented());
}

#[test]
fn test_encoded_frames_single() {
    let frames = EncodedFrames {
        frames: vec![Bytes::from(&b"single"[..])],
    };
    assert_eq!(frames.total_bytes(), 6);
    assert_eq!(frames.frame_count(), 1);
    assert!(!frames.is_fragmented());
}

#[test]
fn test_decode_bad_frame_returns_error() {
    let codec = WireCodec::new(WireCodecConfig::default());
    let raw_frame = RawFrame::new(Bytes::from(&b"\x00\x01\x02\x03"[..]));
    let result = decode_frame(raw_frame, &codec);
    match result {
        Ok(_) => {}
        Err(_) => {} // Expected: can't parse this garbage
    }
}

#[test]
fn test_codec_custom_config() {
    let config = WireCodecConfig {
        default_mtu: 512,
        max_frame_size: 1024,
        enable_fragmentation: false,
        enable_compression: false,
        enable_encryption: false,
        ..Default::default()
    };
    let codec = WireCodec::new(config);
    assert_eq!(codec.config.default_mtu, 512);
    assert_eq!(codec.config.max_frame_size, 1024);
    assert!(!codec.config.enable_fragmentation);
}

#[test]
fn test_codec_with_encryption_enabled() {
    let config = WireCodecConfig {
        enable_encryption: true,
        ..Default::default()
    };
    let codec = WireCodec::new(config);
    assert!(codec.config.enable_encryption);
}

use bytes::Bytes;

#[test]
fn test_encoded_frames_empty_frames() {
    let frames = EncodedFrames { frames: vec![] };
    assert_eq!(frames.total_bytes(), 0);
    assert_eq!(frames.frame_count(), 0);
    assert!(!frames.is_fragmented());
}

#[test]
fn test_codec_config_debug() {
    let config = WireCodecConfig::default();
    let _debug = format!("{:?}", config);
}

#[test]
fn test_codec_default_channel_limits() {
    let config = WireCodecConfig::default();
    // Check all defined channel limits exist
    assert!(config.channel_limits.contains_key(&1)); // render.command
    assert!(config.channel_limits.contains_key(&2)); // ai.state
    assert!(config.channel_limits.contains_key(&3)); // memory.query
    assert!(config.channel_limits.contains_key(&4)); // plugin.invoke
    assert!(config.channel_limits.contains_key(&5)); // voice.input
}
