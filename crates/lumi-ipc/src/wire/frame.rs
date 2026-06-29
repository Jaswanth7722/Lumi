//! # Binary Framing
//!
//! Implements `LumiFramer` as a `tokio_util::codec::Codec` with a state machine:
//! `Scanning → ReadingPrefix → ReadingBody → emit RawFrame`
//!
//! The `Scanning` state is the partial-packet recovery mechanism — when the
//! decoder loses frame synchronization, it scans forward for the next magic.

use crate::wire::error::WireError;
use crate::wire::protocol::*;
use bytes::{BytesMut, BufMut};
use tokio_util::codec::{Decoder, Encoder};
use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicU64};

/// A raw validated frame — guaranteed to start with correct magic, have the
/// correct total_length bytes, and be within max_frame_size.
/// Not yet checksummed or header-parsed.
#[derive(Debug, Clone)]
pub struct RawFrame {
    pub bytes: BytesMut,
}

impl RawFrame {
    pub fn new(bytes: BytesMut) -> Self {
        Self { bytes }
    }

    /// Get the raw header bytes (first HEADER_V1_SIZE bytes).
    pub fn header_bytes(&self) -> &[u8] {
        &self.bytes[..HEADER_V1_SIZE.min(self.bytes.len())]
    }

    /// Get the payload bytes (after the header).
    pub fn payload_bytes(&self) -> &[u8] {
        if self.bytes.len() > HEADER_V1_SIZE {
            &self.bytes[HEADER_V1_SIZE..]
        } else {
            &[]
        }
    }

    /// Total frame length.
    pub fn total_length(&self) -> usize {
        self.bytes.len()
    }
}

/// Framer state machine.
enum FramerState {
    /// Scanning for the magic number in the byte stream (recovery state).
    Scanning { scanned: usize },
    /// Found magic; reading the 16-byte fixed prefix to get total_length.
    ReadingPrefix { prefix_buf: [u8; 16], prefix_read: usize },
    /// Reading the full frame body (header + payload) of known length.
    ReadingBody { total_length: usize, body_buf: BytesMut, body_read: usize },
}

/// Framer metrics for diagnostics.
pub struct FramerMetrics {
    pub frames_decoded: AtomicU64,
    pub bytes_decoded: AtomicU64,
    pub stream_desync_events: AtomicU64,
    pub oversized_frames: AtomicU64,
}

impl FramerMetrics {
    pub fn new() -> Self {
        Self {
            frames_decoded: AtomicU64::new(0),
            bytes_decoded: AtomicU64::new(0),
            stream_desync_events: AtomicU64::new(0),
            oversized_frames: AtomicU64::new(0),
        }
    }
}

/// Lumi frame decoder/encoder for use with `tokio_util::codec::Framed`.
pub struct LumiFramer {
    state: FramerState,
    max_frame_size: usize,
    bytes_since_last_valid_frame: usize,
    metrics: Arc<FramerMetrics>,
}

impl LumiFramer {
    /// Create a new LumiFramer.
    pub fn new(max_frame_size: usize) -> Self {
        Self {
            state: FramerState::Scanning { scanned: 0 },
            max_frame_size,
            bytes_since_last_valid_frame: 0,
            metrics: Arc::new(FramerMetrics::new()),
        }
    }

    /// Create a LumiFramer with default max frame size.
    pub fn default() -> Self {
        Self::new(MAX_FRAME_SIZE)
    }

    /// Get a reference to the framer metrics.
    pub fn metrics(&self) -> Arc<FramerMetrics> {
        self.metrics.clone()
    }

    /// Reset the framer state (e.g., after a successful reconnection).
    pub fn reset(&mut self) {
        self.state = FramerState::Scanning { scanned: 0 };
        self.bytes_since_last_valid_frame = 0;
    }
}

impl Decoder for LumiFramer {
    type Item = RawFrame;
    type Error = WireError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Take ownership of the current state, replacing it with a temporary Scanning.
        // We'll assign the final state back to self.state before returning.
        let mut state = std::mem::replace(&mut self.state, FramerState::Scanning { scanned: 0 });

        // Helper: persist the local state back to self.state and return Ok(None).
        // This is used when we don't have enough data yet.
        macro_rules! need_more_data {
            () => {
                self.state = state;
                return Ok(None);
            };
        }

        // Helper: persist and return an error.
        macro_rules! bail {
            ($err:expr) => {
                self.state = state;
                return Err($err);
            };
        }

        loop {
            match state {
                FramerState::Scanning { ref mut scanned } => {
                    // Reset the scanned counter for fresh scanning
                    if *scanned == 0 {
                        self.bytes_since_last_valid_frame = 0;
                    }

                    // Need at least 4 bytes to detect magic
                    if src.len() < 4 {
                        need_more_data!();
                    }

                    // Check if the first 4 bytes match magic
                    let mut magic_bytes = [0u8; 4];
                    magic_bytes.copy_from_slice(&src[0..4]);

                    if magic_bytes == WIRE_MAGIC_BYTES {
                        // Found magic! Transition to ReadingPrefix state.
                        // src is NOT advanced past the magic, so src[0..16] still contains
                        // the complete 16-byte fixed prefix at the correct offsets:
                        //   [0..4] = magic, [4..5] = wire_version, [5..6] = header_version,
                        //   [6..8] = flags, [8..12] = total_length, [12..16] = payload_length
                        self.bytes_since_last_valid_frame = 0;
                        state = FramerState::ReadingPrefix {
                            prefix_buf: [0u8; 16],
                            prefix_read: 0,
                        };
                        continue;
                    }

                    // No magic at current position — advance one byte and continue scanning
                    src.advance(1);
                    self.bytes_since_last_valid_frame += 1;

                    // Check for stream desync
                    if self.bytes_since_last_valid_frame > MAX_FRAME_SIZE {
                        self.metrics.stream_desync_events.fetch_add(1, Ordering::Relaxed);
                        bail!(WireError::StreamDesync {
                            bytes_lost: self.bytes_since_last_valid_frame,
                        });
                    }

                    // Stay in scanning state with incremented counter
                    *scanned += 1;
                    continue;
                }

                FramerState::ReadingPrefix {
                    ref mut prefix_buf,
                    ref mut prefix_read,
                } => {
                    // Need 16 bytes total for the fixed prefix
                    let needed = 16 - *prefix_read;
                    if src.len() < needed {
                        need_more_data!();
                    }

                    // Copy the remaining prefix bytes into the buffer
                    prefix_buf[*prefix_read..16].copy_from_slice(&src[0..needed]);
                    src.advance(needed);

                    // Parse total_length from the completed prefix (offset 8, 4 bytes LE)
                    let total_length = u32::from_le_bytes(
                        prefix_buf[OFFSET_TOTAL_LENGTH..OFFSET_TOTAL_LENGTH + 4]
                            .try_into()
                            .unwrap(),
                    ) as usize;

                    // Validate frame size
                    if total_length > self.max_frame_size {
                        self.metrics.oversized_frames.fetch_add(1, Ordering::Relaxed);
                        state = FramerState::Scanning { scanned: 0 };
                        continue;
                    }

                    // Create body buffer with the 16-byte prefix already written
                    let mut body_buf = BytesMut::with_capacity(total_length);
                    body_buf.extend_from_slice(&prefix_buf[..]);

                    if total_length == 16 {
                        // No payload — complete frame
                        self.metrics.frames_decoded.fetch_add(1, Ordering::Relaxed);
                        self.metrics.bytes_decoded.fetch_add(16, Ordering::Relaxed);
                        self.state = FramerState::Scanning { scanned: 0 };
                        return Ok(Some(RawFrame::new(body_buf)));
                    }

                    // Move to ReadingBody to collect the payload
                    state = FramerState::ReadingBody {
                        total_length,
                        body_buf,
                        body_read: 16,
                    };
                    // Fall through to the ReadingBody handler
                    continue;
                }

                FramerState::ReadingBody {
                    total_length,
                    mut body_buf,
                    body_read,
                } => {
                    let remaining = total_length - body_read;

                    if src.len() < remaining {
                        // Not enough data — copy what we have and wait
                        let to_copy = src.len();
                        body_buf.extend_from_slice(&src[..to_copy]);
                        src.advance(to_copy);
                        state = FramerState::ReadingBody {
                            total_length,
                            body_buf,
                            body_read: body_read + to_copy,
                        };
                        need_more_data!();
                    }

                    // Complete frame received
                    body_buf.extend_from_slice(&src[..remaining]);
                    src.advance(remaining);

                    self.metrics.frames_decoded.fetch_add(1, Ordering::Relaxed);
                    self.metrics.bytes_decoded.fetch_add(total_length as u64, Ordering::Relaxed);

                    self.state = FramerState::Scanning { scanned: 0 };
                    return Ok(Some(RawFrame::new(body_buf)));
                }
            }
        }
    }
}

impl Encoder<RawFrame> for LumiFramer {
    type Error = WireError;

    fn encode(&mut self, item: RawFrame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.put_slice(&item.bytes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_frame(payload: &[u8]) -> BytesMut {
        let total_len = HEADER_V1_SIZE + payload.len();
        let mut buf = BytesMut::with_capacity(total_len);

        // Magic (4 bytes)
        buf.extend_from_slice(&WIRE_MAGIC_BYTES);
        // Wire version + header version (2 bytes)
        buf.extend_from_slice(&[WIRE_VERSION_MAJOR, HEADER_VERSION]);
        // Flags (2 bytes) — zero
        buf.extend_from_slice(&0u16.to_le_bytes());
        // Total length (4 bytes)
        buf.extend_from_slice(&(total_len as u32).to_le_bytes());
        // Payload length (4 bytes)
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        // Rest of header (padding to HEADER_V1_SIZE)
        while buf.len() < HEADER_V1_SIZE {
            buf.extend_from_slice(&[0u8]);
        }
        // Overwrite message_id with non-zero (offset 16)
        buf[OFFSET_MESSAGE_ID] = 1;
        // Overwrite sender_id with non-zero (offset 56)
        buf[OFFSET_SENDER_ID] = 1;
        // Payload
        buf.extend_from_slice(payload);

        buf
    }

    #[test]
    fn test_decode_complete_frame() {
        let mut framer = LumiFramer::default();
        let mut buf = make_valid_frame(b"hello");
        let result = framer.decode(&mut buf).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_decode_partial_then_complete() {
        let mut framer = LumiFramer::default();
        let full = make_valid_frame(b"hello");

        // Feed first 10 bytes (partial header)
        let mut partial = full.slice(0..10);
        let result = framer.decode(&mut partial).unwrap();
        assert!(result.is_none(), "Should need more data");

        // Feed the rest
        let mut remaining = full.slice(10..);
        let result = framer.decode(&mut remaining).unwrap();
        assert!(result.is_some(), "Should complete the frame");
    }

    #[test]
    fn test_two_frames_in_one_buffer() {
        let mut framer = LumiFramer::default();
        let frame1 = make_valid_frame(b"frame1");
        let frame2 = make_valid_frame(b"frame2");

        let mut combined = BytesMut::new();
        combined.extend_from_slice(&frame1);
        combined.extend_from_slice(&frame2);

        // First decode should get frame1
        let result1 = framer.decode(&mut combined).unwrap();
        assert!(result1.is_some());

        // Second decode should get frame2
        let result2 = framer.decode(&mut combined).unwrap();
        assert!(result2.is_some());
    }

    #[test]
    fn test_invalid_magic_scanning() {
        let mut framer = LumiFramer::default();
        let mut buf = BytesMut::new();
        buf.extend_from_slice(b"GARBAGE");

        let result = framer.decode(&mut buf).unwrap();
        assert!(result.is_none());
        // Garbage should be consumed during scanning
        assert!(buf.is_empty());
    }

    #[test]
    fn test_garbage_then_valid_frame() {
        let mut framer = LumiFramer::default();
        let valid = make_valid_frame(b"ok");
        let mut buf = BytesMut::new();
        buf.extend_from_slice(b"JUNK");
        buf.extend_from_slice(&valid);

        // First call: scan through garbage (no frame yet)
        let result1 = framer.decode(&mut buf).unwrap();
        // The result depends on how much garbage was scanned — we may need multiple calls
        // Either way, eventually we should get the valid frame
        let _ = result1;

        // Keep decoding until we get the frame
        let mut found = false;
        for _ in 0..10 {
            if let Ok(Some(_)) = framer.decode(&mut buf) {
                found = true;
                break;
            }
            if buf.is_empty() {
                break;
            }
        }
        assert!(found, "Should eventually decode the valid frame after garbage");
    }

    #[test]
    fn test_encode_frame() {
        let mut framer = LumiFramer::default();
        let raw = RawFrame::new(BytesMut::from(&b"LUMI"[..]));

        let mut buf = BytesMut::new();
        framer.encode(raw, &mut buf).unwrap();
        assert_eq!(&buf[..], b"LUMI");
    }

    #[test]
    fn test_framer_reset() {
        let mut framer = LumiFramer::default();
        framer.reset();

        let mut buf = make_valid_frame(b"test");
        let result = framer.decode(&mut buf).unwrap();
        assert!(result.is_some());
    }
}
