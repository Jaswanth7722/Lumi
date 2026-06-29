// ── Frame Tests ────────────────────────────────────────────────────────────────
// Tests the LumiFramer state machine: complete frames, partial frames,
// two frames in one buffer, scanning recovery, oversized rejection, desync.
#![cfg(test)]

use std::sync::Arc;

use bytes::{BytesMut, BufMut};
use tokio_util::codec::Decoder;

use crate::wire::error::WireError;
use crate::wire::frame::LumiFramer;
use crate::wire::metrics::WireMetrics;
use crate::wire::protocol::*;

/// Build a minimal valid frame for testing
fn build_frame(payload_len: usize) -> BytesMut {
    let total_len = HEADER_V1_SIZE + payload_len;
    let mut buf = BytesMut::with_capacity(total_len);
    buf.put_u32(WIRE_MAGIC.to_be_bytes()); // magic
    buf.put_u8(WIRE_VERSION_MAJOR);        // wire_version
    buf.put_u8(1);                          // header_version
    buf.put_u16_le(0);                      // flags
    buf.put_u32_le(total_len as u32);       // total_length
    buf.put_u32_le(payload_len as u32);     // payload_length
    // Fill rest of header with zeros
    while buf.len() < HEADER_V1_SIZE {
        buf.put_u8(0);
    }
    // Add payload
    buf.extend(std::iter::repeat(0xAB).take(payload_len));
    buf
}

#[test]
fn test_decode_complete_frame() {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(MAX_FRAME_SIZE, metrics);
    let frame = build_frame(32);
    let raw = framer.decode(&mut frame.clone()).unwrap();
    assert!(raw.is_some());
    let raw = raw.unwrap();
    assert!(raw.as_ref().len() >= HEADER_V1_SIZE);
}

#[test]
fn test_decode_two_frames_in_one_buffer() {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(MAX_FRAME_SIZE, metrics);
    let mut buf = build_frame(16);
    buf.extend_from_slice(&build_frame(32));

    let raw1 = framer.decode(&mut buf).unwrap();
    assert!(raw1.is_some());
    let raw2 = framer.decode(&mut buf).unwrap();
    assert!(raw2.is_some());
    // Buffer should be empty now
    assert!(framer.decode(&mut buf).unwrap().is_none());
}

#[test]
fn test_decode_partial_frame_then_complete() {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(MAX_FRAME_SIZE, metrics);
    let mut buf = build_frame(64);
    let full_len = buf.len();

    // Split the buffer: give only first 20 bytes
    let mut partial = buf.split_to(20);
    let result = framer.decode(&mut partial).unwrap();
    assert!(result.is_none()); // not enough data

    // Give the rest
    let result = framer.decode(&mut buf).unwrap();
    assert!(result.is_some());
}

#[test]
fn test_oversized_frame_rejected() {
    let metrics = Arc::new(WireMetrics::new());
    // Set max frame size small
    let mut framer = LumiFramer::new(128, metrics);
    let buf = build_frame(200); // total will be > 128
    let result = framer.decode(&mut buf.clone()).unwrap();
    // Should either reject or enter scanning mode
    assert!(result.is_none() || result.is_err());
}

#[test]
fn test_garbage_then_valid_frame_scanning() {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(MAX_FRAME_SIZE, metrics);

    // Start with garbage
    let mut buf = BytesMut::from(&b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09"[..]);
    buf.extend_from_slice(&build_frame(16));
    buf.extend_from_slice(&build_frame(32));

    // First decode should trigger scanning and find the first frame
    let raw1 = framer.decode(&mut buf).unwrap();
    assert!(raw1.is_some());
    let raw1 = raw1.unwrap();
    assert!(raw1.as_ref().len() >= HEADER_V1_SIZE);

    // Second decode should get the next frame
    let raw2 = framer.decode(&mut buf).unwrap();
    assert!(raw2.is_some());
}

#[test]
fn test_empty_buffer_returns_none() {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(MAX_FRAME_SIZE, metrics);
    let mut buf = BytesMut::new();
    let result = framer.decode(&mut buf).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_single_byte_buffer_returns_none() {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(MAX_FRAME_SIZE, metrics);
    let mut buf = BytesMut::from(&b"\x4C"[..]); // 'L' from LUMI but not enough
    let result = framer.decode(&mut buf).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_zero_length_frame() {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(MAX_FRAME_SIZE, metrics);
    let buf = build_frame(0);
    let result = framer.decode(&mut buf.clone()).unwrap();
    assert!(result.is_some());
    let raw = result.unwrap();
    assert_eq!(raw.as_ref().len(), HEADER_V1_SIZE);
}

#[test]
fn test_frame_with_no_payload_debug_flag() {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(MAX_FRAME_SIZE, metrics);
    let buf = build_frame(0);
    let raw = framer.decode(&mut buf.clone()).unwrap().unwrap();
    assert!(!raw.as_ref().is_empty());
}

#[test]
fn test_recovery_after_truncated_frame() {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(MAX_FRAME_SIZE, metrics);

    // Valid frame
    let mut buf = build_frame(16);
    // Append garbage that looks like a truncated frame
    buf.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
    // Then another valid frame
    buf.extend_from_slice(&build_frame(8));

    // First: valid frame
    let r = framer.decode(&mut buf).unwrap();
    assert!(r.is_some());

    // Second: scanning should skip garbage and find the second frame
    let r = framer.decode(&mut buf).unwrap();
    assert!(r.is_some());
}

#[test]
fn test_max_frame_size_boundary() {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(1024, metrics);
    let buf = build_frame(1024 - HEADER_V1_SIZE); // exactly at limit
    let result = framer.decode(&mut buf.clone()).unwrap();
    assert!(result.is_some());

    let buf = build_frame(1024 - HEADER_V1_SIZE + 1); // just over
    let result = framer.decode(&mut buf.clone()).unwrap();
    assert!(result.is_none() || result.is_err());
}

#[test]
fn test_decode_respects_framer_metrics() {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(MAX_FRAME_SIZE, metrics.clone());
    let buf = build_frame(16);
    let _ = framer.decode(&mut buf.clone()).unwrap();
    assert!(metrics.snapshot().frames_decoded >= 1);
}
