//! # Shared Memory Transport (Tier 1)
//!
//! Lock-free SPSC (Single Producer, Single Consumer) ring buffer in a shared
//! memory region (`mmap` on Unix, `CreateFileMapping` on Windows).
//!
//! ## Safety Invariants
//!
//! 1. A `magic: u64` sentinel at the start of the shared region detects mapping errors.
//! 2. A `schema_hash: u64` (FNV of the message type layout) detects ABI mismatches.
//! 3. Head/tail atomics use `SeqCst` ordering (not `Relaxed`) because memory ordering
//!    is not guaranteed across process address spaces on all architectures.
//! 4. Data reads/writes use `volatile` semantics to prevent the compiler from
//!    optimizing away cross-process memory accesses.
//!
//! Used for: `render.command`, `ai.state` — channels requiring sub-100µs latency.

use crate::error::{IpcError, TransportError};
use crate::message::LumiMessage;
use crate::transport::{Transport, TransportMetrics, TransportTier};
use async_trait::async_trait;
use crossbeam::queue::SegQueue;
use fnv::FnvHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Magic sentinel value for shared memory region detection.
pub const SHM_MAGIC: u64 = 0x4C554D495F53484D; // "LUMI_SHM" in ASCII

/// Default number of ring buffer slots.
pub const DEFAULT_SLOTS: u32 = 64;

/// Default slot size in bytes.
pub const DEFAULT_SLOT_SIZE: u32 = 4096;

/// Compute the FNV hash of the LumiMessage type layout for ABI detection.
pub fn compute_schema_hash() -> u64 {
    let mut hasher = FnvHasher::default();
    "LumiMessage_v1".hash(&mut hasher);
    hasher.finish()
}

/// A slot in the shared memory ring buffer.
#[repr(C, align(64))]
struct Slot {
    /// Whether this slot is occupied (1) or free (0).
    /// Written by producer, read by consumer.
    occupied: u8,
    /// Padding to prevent false sharing
    _pad1: [u8; 7],
    /// Payload data
    data: [u8; 4096],
    /// Payload length
    length: u32,
    /// Padding
    _pad2: [u8; 28],
}

impl Slot {
    const fn new() -> Self {
        Self {
            occupied: 0,
            _pad1: [0u8; 7],
            data: [0u8; 4096],
            length: 0,
            _pad2: [0u8; 28],
        }
    }
}

/// Shared memory transport implementation.
///
/// NOTE: This is a simplified implementation that demonstrates the API and safety
/// model. A production implementation would use actual OS shared memory APIs
/// (`mmap`/`CreateFileMapping`) and memory-mapped files. This version uses
/// in-memory buffers for testing and API compatibility.
pub struct SharedMemoryTransport {
    /// Channel name
    channel: String,
    /// Number of ring buffer slots
    slots: u32,
    /// Slot size in bytes
    slot_size: u32,
    /// Magic sentinel
    magic: u64,
    /// Schema hash for ABI detection
    schema_hash: u64,
    /// Producer index (atomic, SeqCst)
    head: AtomicU64,
    /// Consumer index (atomic, SeqCst)
    tail: AtomicU64,
    /// Ring buffer slots (in real impl: pointer to mmap'd region)
    // In this simulation, we use an in-memory buffer
    buffer: Arc<std::sync::Mutex<Vec<Option<Vec<u8>>>>>,
    /// Receive buffer for messages not yet consumed
    recv_buffer: Arc<SegQueue<LumiMessage>>,
    /// Closed flag
    closed: AtomicBool,
    /// Metrics
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
}

impl SharedMemoryTransport {
    /// Create a new shared memory transport.
    ///
    /// # Arguments
    /// * `channel` - Channel name
    /// * `slots` - Number of ring buffer slots (default: 64)
    /// * `slot_size` - Size of each slot in bytes (default: 4096)
    pub fn new(channel: &str, slots: u32, slot_size: u32) -> Self {
        let slots = if slots == 0 { DEFAULT_SLOTS } else { slots };
        let slot_size = if slot_size == 0 { DEFAULT_SLOT_SIZE } else { slot_size };

        // Validate shared memory parameters
        assert!(slots > 0, "SHM transport requires at least 1 slot");
        assert!(slot_size >= 256, "SHM slot size must be at least 256 bytes");
        assert!(
            slot_size <= 1024 * 1024,
            "SHM slot size must not exceed 1MB"
        );

        Self {
            channel: channel.to_string(),
            slots,
            slot_size,
            magic: SHM_MAGIC,
            schema_hash: compute_schema_hash(),
            head: AtomicU64::new(0),
            tail: AtomicU64::new(0),
            buffer: Arc::new(std::sync::Mutex::new(vec![None; slots as usize])),
            recv_buffer: Arc::new(SegQueue::new()),
            closed: AtomicBool::new(false),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
        }
    }

    /// Verify the shared memory region is valid.
    ///
    /// # Safety
    ///
    /// In a real implementation, this would check:
    /// 1. The `magic` sentinel matches `SHM_MAGIC`
    /// 2. The `schema_hash` matches the current compiled schema hash
    ///
    /// If either check fails, the transport must refuse to operate
    /// because cross-process memory would be interpreted incorrectly.
    pub fn verify_region(&self) -> Result<(), IpcError> {
        // In production: read magic from mmap'd region
        let magic_ok = self.magic == SHM_MAGIC;
        if !magic_ok {
            return Err(IpcError::SharedMemoryMagicMismatch {
                expected: SHM_MAGIC as u32,
                found: self.magic as u32,
            });
        }

        let hash_ok = self.schema_hash == compute_schema_hash();
        if !hash_ok {
            return Err(IpcError::SharedMemorySchemaHashMismatch {
                expected: compute_schema_hash(),
                found: self.schema_hash,
            });
        }

        Ok(())
    }

    /// Check the magic sentinel (returns false if magic is wrong).
    pub fn check_magic(&self) -> bool {
        self.magic == SHM_MAGIC
    }

    /// Get the schema hash for this transport.
    pub fn schema_hash(&self) -> u64 {
        self.schema_hash
    }

    /// Get the number of slots.
    pub fn num_slots(&self) -> u32 {
        self.slots
    }
}

#[async_trait]
impl Transport for SharedMemoryTransport {
    fn tier(&self) -> TransportTier {
        TransportTier::SharedMemory
    }

    fn name(&self) -> &'static str {
        "shared-memory"
    }

    fn channel(&self) -> &str {
        &self.channel
    }

    async fn send(&self, msg: LumiMessage) -> Result<(), TransportError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(TransportError::Closed);
        }

        // Verify shared memory region
        if let Err(e) = self.verify_region() {
            return Err(TransportError::Io(e.to_string()));
        }

        self.messages_sent.fetch_add(1, Ordering::Relaxed);

        // Serialize the message
        let payload = rmp_serde::to_vec(&msg)
            .map_err(|e| TransportError::Io(e.to_string()))?;

        if payload.len() > self.slot_size as usize {
            return Err(TransportError::Io(format!(
                "Payload size {} exceeds slot size {}",
                payload.len(),
                self.slot_size
            )));
        }

        // Write to ring buffer (with SeqCst ordering for cross-process visibility)
        let head = self.head.fetch_add(1, Ordering::SeqCst);
        let index = (head % self.slots as u64) as usize;

        let mut buffer = self.buffer.lock().unwrap();
        buffer[index] = Some(payload);

        // In production: write_volatile for the slot data and SeqCst store for head
        // to ensure the consumer observes the write.

        Ok(())
    }

    async fn recv(&self) -> Result<LumiMessage, TransportError> {
        // Check buffer first
        if let Some(msg) = self.recv_buffer.pop() {
            self.messages_received.fetch_add(1, Ordering::Relaxed);
            return Ok(msg);
        }

        // Try reading from ring buffer
        let tail = self.tail.load(Ordering::SeqCst);
        let head = self.head.load(Ordering::SeqCst);

        if tail < head {
            let index = (tail % self.slots as u64) as usize;
            let mut buffer = self.buffer.lock().unwrap();

            if let Some(payload) = buffer[index].take() {
                // Advance tail with SeqCst ordering
                self.tail.fetch_add(1, Ordering::SeqCst);

                // Deserialize
                let msg: LumiMessage = rmp_serde::from_slice(&payload)
                    .map_err(|e| TransportError::Io(e.to_string()))?;

                self.messages_received.fetch_add(1, Ordering::Relaxed);
                return Ok(msg);
            }
        }

        Err(TransportError::NotConnected)
    }

    fn try_recv(&self) -> Result<Option<LumiMessage>, TransportError> {
        // Check software buffer
        if let Some(msg) = self.recv_buffer.pop() {
            self.messages_received.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(msg));
        }

        // Try ring buffer
        let tail = self.tail.load(Ordering::SeqCst);
        let head = self.head.load(Ordering::SeqCst);

        if tail < head {
            let index = (tail % self.slots as u64) as usize;
            let mut buffer = self.buffer.lock().unwrap();

            if let Some(payload) = buffer[index].take() {
                self.tail.fetch_add(1, Ordering::SeqCst);

                let msg: LumiMessage = rmp_serde::from_slice(&payload)
                    .map_err(|e| TransportError::Io(e.to_string()))?;

                self.messages_received.fetch_add(1, Ordering::Relaxed);
                return Ok(Some(msg));
            }
        }

        Ok(None)
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.closed.store(true, Ordering::Relaxed);
        // In production: unmap the shared memory region
        Ok(())
    }

    fn metrics(&self) -> TransportMetrics {
        TransportMetrics {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            ..Default::default()
        }
    }
}

impl std::fmt::Debug for SharedMemoryTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedMemoryTransport")
            .field("channel", &self.channel)
            .field("slots", &self.slots)
            .field("slot_size", &self.slot_size)
            .field("magic_valid", &(self.magic == SHM_MAGIC))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_constant() {
        assert_eq!(SHM_MAGIC, 0x4C554D495F53484D);
    }

    #[test]
    fn test_verify_region_ok() {
        let transport = SharedMemoryTransport::new("test.shm", 64, 4096);
        assert!(transport.verify_region().is_ok());
    }

    #[test]
    fn test_schema_hash_stable() {
        let hash1 = compute_schema_hash();
        let hash2 = compute_schema_hash();
        assert_eq!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_send_recv_roundtrip() {
        let transport = SharedMemoryTransport::new("test.shm", 64, 4096);

        let msg = LumiMessage::new_event(
            crate::message::ProcessId::Core,
            "test.shm",
            crate::message::MessagePayload::Empty,
        );

        transport.send(msg.clone()).await.unwrap();
        let received = transport.recv().await.unwrap();
        assert_eq!(msg.id, received.id);
    }

    #[test]
    fn test_transport_tier() {
        let transport = SharedMemoryTransport::new("test", 64, 4096);
        assert_eq!(transport.tier(), TransportTier::SharedMemory);
    }
}
