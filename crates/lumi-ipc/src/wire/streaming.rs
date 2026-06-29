// ── Stream Framing ────────────────────────────────────────────────────────────
// Streaming support for continuous channels like `voice.input`.
// Stream-id-tagged chunks arrive in order with sequence numbers for ordering.
// Flow control uses a credit-based mechanism to prevent buffer overflow.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;

use crate::wire::error::WireError;
use crate::wire::protocol::MAX_FRAME_SIZE;

/// Maximum chunks buffered per stream before backpressure triggers.
const MAX_CHUNKS_PER_STREAM: usize = 8;

/// Unique identifier for a stream (e.g., voice.input session).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamId(u64);

impl StreamId {
    /// Create a new stream ID from a raw u64.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// The raw u64 representation.
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for StreamId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

/// A single chunk in a continuous stream (e.g., audio chunk from `voice.input`).
#[derive(Debug, Clone)]
pub struct StreamFrame {
    /// Which stream this chunk belongs to.
    pub stream_id: StreamId,
    /// Monotonic per-stream chunk counter.
    pub sequence: u32,
    /// Whether this is the final chunk in the stream.
    pub is_final: bool,
    /// The chunk payload.
    pub payload: Bytes,
}

impl StreamFrame {
    /// Create a new stream frame.
    pub fn new(stream_id: StreamId, sequence: u32, payload: Bytes) -> Self {
        Self {
            stream_id,
            sequence,
            is_final: false,
            payload,
        }
    }

    /// Mark this frame as the final one in the stream.
    pub fn with_final(mut self) -> Self {
        self.is_final = true;
        self
    }

    /// Estimated memory usage of this frame.
    pub fn memory_usage(&self) -> usize {
        8 + 4 + 1 + self.payload.len() + std::mem::size_of::<StreamFrame>()
    }
}

/// Buffer for ordered reassembly of stream chunks.
#[derive(Debug)]
struct StreamBuffer {
    /// Buffered chunks indexed by sequence number.
    chunks: HashMap<u32, StreamFrame>,
    /// The next expected sequence number.
    next_expected: u32,
    /// Whether the final chunk has been received.
    finalized: bool,
    /// Total bytes buffered.
    total_bytes: usize,
    /// When the last chunk was received (for timeout GC).
    last_activity: Instant,
}

impl StreamBuffer {
    fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            next_expected: 0,
            finalized: false,
            total_bytes: 0,
            last_activity: Instant::now(),
        }
    }

    fn add_chunk(&mut self, frame: StreamFrame) -> Result<Option<Vec<Bytes>>, WireError> {
        if self.finalized {
            return Err(WireError::StreamBufferFull {
                stream_id: frame.stream_id.as_u64(),
                buffer_chunks: self.chunks.len(),
            });
        }

        if self.chunks.len() >= MAX_CHUNKS_PER_STREAM {
            return Err(WireError::StreamBufferFull {
                stream_id: frame.stream_id.as_u64(),
                buffer_chunks: self.chunks.len(),
            });
        }

        self.last_activity = Instant::now();

        if frame.is_final {
            self.finalized = true;
        }

        // Insert the chunk
        let sequence = frame.sequence;
        self.total_bytes += frame.payload.len();
        self.chunks.insert(sequence, frame);

        // Drain contiguous chunks starting from next_expected
        let mut ordered = Vec::new();
        while let Some(chunk) = self.chunks.remove(&self.next_expected) {
            self.total_bytes = self.total_bytes.saturating_sub(chunk.payload.len());
            ordered.push(chunk.payload);
            self.next_expected += 1;
        }

        if ordered.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ordered))
        }
    }

    fn is_expired(&self, timeout: std::time::Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }
}

/// Credit-based flow controller for streams.
///
/// The receiver grants credits to the sender; the sender may only transmit
/// `credits` chunks before waiting for more credits. This prevents the
/// receiver's buffer from overflowing.
#[derive(Debug, Clone)]
pub struct FlowController {
    credits: Arc<AtomicU32>,
    max_credits: u32,
}

impl FlowController {
    /// Create a new flow controller with the given initial credit budget.
    pub fn new(initial_credits: u32) -> Self {
        Self {
            credits: Arc::new(AtomicU32::new(initial_credits)),
            max_credits: initial_credits,
        }
    }

    /// Try to acquire a credit to send one chunk.
    /// Returns `Ok(())` if credits remain, `Err` if no credits available.
    pub fn acquire_credit(&self) -> Result<(), WireError> {
        loop {
            let current = self.credits.load(Ordering::Acquire);
            if current == 0 {
                return Err(WireError::StreamBufferFull {
                    stream_id: 0,
                    buffer_chunks: 0,
                });
            }
            if self
                .credits
                .compare_exchange(current, current - 1, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    /// Return a credit (called by receiver after processing a chunk).
    pub fn return_credit(&self) {
        self.credits.fetch_add(1, Ordering::Release);
    }

    /// Grant additional credits to the sender.
    pub fn grant_credits(&self, additional: u32) {
        let new = self.credits.load(Ordering::Acquire).saturating_add(additional);
        let new = new.min(self.max_credits);
        self.credits.store(new, Ordering::Release);
    }

    /// Current number of available credits.
    pub fn available_credits(&self) -> u32 {
        self.credits.load(Ordering::Acquire)
    }
}

/// Manages multiple stream buffers, one per `StreamId`.
#[derive(Debug)]
pub struct StreamManager {
    buffers: HashMap<StreamId, StreamBuffer>,
    flow_controllers: HashMap<StreamId, FlowController>,
    timeout: std::time::Duration,
}

impl StreamManager {
    /// Create a new stream manager with the given inactivity timeout.
    pub fn new(timeout: std::time::Duration) -> Self {
        Self {
            buffers: HashMap::new(),
            flow_controllers: HashMap::new(),
            timeout,
        }
    }

    /// Register a new stream with a flow controller.
    pub fn register_stream(&mut self, stream_id: StreamId, initial_credits: u32) {
        self.buffers.entry(stream_id).or_insert_with(StreamBuffer::new);
        self.flow_controllers
            .entry(stream_id)
            .or_insert_with(|| FlowController::new(initial_credits));
    }

    /// Add a chunk to a stream buffer and retrieve ordered chunks.
    pub fn push_chunk(&mut self, frame: StreamFrame) -> Result<Option<Vec<Bytes>>, WireError> {
        let stream_id = frame.stream_id;
        let buffer = self
            .buffers
            .get_mut(&stream_id)
            .ok_or(WireError::StreamBufferFull {
                stream_id: stream_id.as_u64(),
                buffer_chunks: 0,
            })?;

        let result = buffer.add_chunk(frame)?;

        // Return a credit for the processed chunk
        if let Some(fc) = self.flow_controllers.get(&stream_id) {
            fc.return_credit();
        }

        Ok(result)
    }

    /// Get the flow controller for a stream.
    pub fn flow_controller(&self, stream_id: &StreamId) -> Option<&FlowController> {
        self.flow_controllers.get(stream_id)
    }

    /// Run garbage collection: remove timed-out stream buffers.
    /// Returns the number of removed streams.
    pub fn gc(&mut self) -> usize {
        let before = self.buffers.len();
        self.buffers.retain(|id, buf| !buf.is_expired(self.timeout));
        self.flow_controllers
            .retain(|id, _| self.buffers.contains_key(id));
        before - self.buffers.len()
    }

    /// Remove a completed or failed stream.
    pub fn remove_stream(&mut self, stream_id: &StreamId) {
        self.buffers.remove(stream_id);
        self.flow_controllers.remove(stream_id);
    }
}

/// A handle to an active stream for reading chunks.
#[derive(Debug)]
pub struct StreamHandle {
    stream_id: StreamId,
    next_sequence: u32,
    finalized: bool,
}

impl StreamHandle {
    /// Create a new stream handle.
    pub fn new(stream_id: StreamId) -> Self {
        Self {
            stream_id,
            next_sequence: 0,
            finalized: false,
        }
    }

    /// The stream ID this handle is for.
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    /// Whether the stream has been finalized (no more chunks expected).
    pub fn is_finalized(&self) -> bool {
        self.finalized
    }

    /// Mark the stream as finalized.
    pub fn set_finalized(&mut self) {
        self.finalized = true;
    }

    /// Get the next expected sequence number.
    pub fn next_sequence(&self) -> u32 {
        self.next_sequence
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_stream_id_creation() {
        let id = StreamId::new(42);
        assert_eq!(id.as_u64(), 42);
        assert_eq!(StreamId::from(100).as_u64(), 100);
    }

    #[test]
    fn test_stream_frame_creation() {
        let id = StreamId::new(1);
        let frame = StreamFrame::new(id, 0, Bytes::from(&b"audio data"[..]));
        assert_eq!(frame.stream_id, id);
        assert_eq!(frame.sequence, 0);
        assert!(!frame.is_final);
        assert_eq!(frame.payload.as_ref(), b"audio data");
    }

    #[test]
    fn test_stream_frame_with_final() {
        let id = StreamId::new(1);
        let frame = StreamFrame::new(id, 5, Bytes::new()).with_final();
        assert!(frame.is_final);
    }

    #[test]
    fn test_flow_controller_credit_acquire() {
        let fc = FlowController::new(3);
        assert!(fc.acquire_credit().is_ok());
        assert!(fc.acquire_credit().is_ok());
        assert!(fc.acquire_credit().is_ok());
        assert!(fc.acquire_credit().is_err()); // no credits left
    }

    #[test]
    fn test_flow_controller_return_credit() {
        let fc = FlowController::new(1);
        fc.acquire_credit().unwrap();
        assert!(fc.acquire_credit().is_err());
        fc.return_credit();
        assert!(fc.acquire_credit().is_ok()); // credit returned
    }

    #[test]
    fn test_flow_controller_grant_credits() {
        let fc = FlowController::new(1);
        fc.acquire_credit().unwrap();
        fc.grant_credits(5);
        assert_eq!(fc.available_credits(), 5);
    }

    #[test]
    fn test_flow_controller_max_credits() {
        let fc = FlowController::new(5);
        fc.grant_credits(100);
        assert_eq!(fc.available_credits(), 5); // capped at max
    }

    #[test]
    fn test_stream_buffer_in_order() {
        let mut buffer = StreamBuffer::new();
        let id = StreamId::new(1);

        let frame1 = StreamFrame::new(id, 0, Bytes::from(&b"chunk0"[..]));
        let result = buffer.add_chunk(frame1).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap()[0].as_ref(), b"chunk0");

        let frame2 = StreamFrame::new(id, 1, Bytes::from(&b"chunk1"[..]));
        let result = buffer.add_chunk(frame2).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap()[0].as_ref(), b"chunk1");
    }

    #[test]
    fn test_stream_buffer_out_of_order() {
        let mut buffer = StreamBuffer::new();
        let id = StreamId::new(1);

        // Send chunk 1 before chunk 0
        let frame1 = StreamFrame::new(id, 1, Bytes::from(&b"chunk1"[..]));
        let result = buffer.add_chunk(frame1).unwrap();
        assert!(result.is_none()); // can't emit yet

        let frame0 = StreamFrame::new(id, 0, Bytes::from(&b"chunk0"[..]));
        let result = buffer.add_chunk(frame0).unwrap();
        assert!(result.is_some());
        let chunks = result.unwrap();
        assert_eq!(chunks.len(), 2); // both chunks emitted in order
        assert_eq!(chunks[0].as_ref(), b"chunk0");
        assert_eq!(chunks[1].as_ref(), b"chunk1");
    }

    #[test]
    fn test_stream_buffer_max_chunks() {
        let mut buffer = StreamBuffer::new();
        let id = StreamId::new(1);

        for i in 0..MAX_CHUNKS_PER_STREAM {
            let frame = StreamFrame::new(id, i as u32, Bytes::from(&b"data"[..]));
            let result = buffer.add_chunk(frame);
            assert!(
                result.is_ok(),
                "Chunk {} should be accepted",
                i
            );
        }

        // Next chunk should be rejected
        let frame = StreamFrame::new(
            id,
            MAX_CHUNKS_PER_STREAM as u32,
            Bytes::from(&b"overflow"[..]),
        );
        let result = buffer.add_chunk(frame);
        assert!(result.is_err());
    }

    #[test]
    fn test_stream_manager_register() {
        let mut manager = StreamManager::new(Duration::from_secs(30));
        let id = StreamId::new(1);
        manager.register_stream(id, 100);
        assert!(manager.flow_controller(&id).is_some());
    }

    #[test]
    fn test_stream_manager_push_and_ordered() {
        let mut manager = StreamManager::new(Duration::from_secs(30));
        let id = StreamId::new(1);
        manager.register_stream(id, 100);

        let frame = StreamFrame::new(id, 0, Bytes::from(&b"hello"[..]));
        let result = manager.push_chunk(frame).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_stream_manager_gc() {
        let mut manager = StreamManager::new(Duration::from_secs(0)); // immediate timeout
        manager.register_stream(StreamId::new(1), 10);
        std::thread::sleep(Duration::from_millis(10));
        let removed = manager.gc();
        assert!(removed >= 1);
    }

    #[test]
    fn test_stream_handle() {
        let id = StreamId::new(1);
        let mut handle = StreamHandle::new(id);
        assert_eq!(handle.stream_id(), id);
        assert!(!handle.is_finalized());
        handle.set_finalized();
        assert!(handle.is_finalized());
        assert_eq!(handle.next_sequence(), 0);
    }

    #[test]
    fn test_stream_frame_memory_usage() {
        let id = StreamId::new(1);
        let frame = StreamFrame::new(id, 0, Bytes::from(&b"test"[..]));
        assert!(frame.memory_usage() > 0);
    }

    #[test]
    fn test_flow_controller_concurrent() {
        let fc = Arc::new(FlowController::new(1000));
        let mut handles = Vec::new();

        for _ in 0..10 {
            let fc = fc.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    while fc.acquire_credit().is_err() {
                        std::thread::yield_now();
                    }
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(fc.available_credits(), 0);
    }
}
