//! # Streaming Framework
//!
//! Provides credit-based flow control for streaming channels like `voice.input`.
//! Streams allow chunked message exchange with backpressure.

use crate::error::{IpcError, IpcResult};
use crate::message::{LumiMessage, MessageKind, MessagePayload};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::{mpsc, oneshot};

/// Stream direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDirection {
    Send,
    Receive,
}

/// Flow controller with credit-based backpressure.
pub struct FlowController {
    /// Available credit (send slots)
    credit: AtomicUsize,
    /// Maximum credit window size
    max_credit: usize,
    /// Channel for receiving additional credits from receiver
    credit_rx: std::sync::Mutex<mpsc::Receiver<usize>>,
    /// Channel for sending credit requests
    credit_tx: mpsc::Sender<usize>,
}

impl FlowController {
    /// Create a new flow controller with a credit window.
    pub fn new(initial_credit: usize) -> (Self, mpsc::Receiver<usize>) {
        let (credit_tx, incoming_credit_rx) = mpsc::channel(32);
        let (_, credit_rx_internal) = mpsc::channel(32);

        let controller = Self {
            credit: AtomicUsize::new(initial_credit),
            max_credit: initial_credit,
            credit_rx: std::sync::Mutex::new(credit_rx_internal),
            credit_tx,
        };

        (controller, incoming_credit_rx)
    }

    /// Try to acquire credit for sending one chunk.
    pub fn try_acquire(&self) -> bool {
        loop {
            let current = self.credit.load(Ordering::SeqCst);
            if current == 0 {
                return false;
            }
            if self.credit.compare_exchange(
                current,
                current - 1,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ).is_ok() {
                return true;
            }
        }
    }

    /// Return credit (called by receiver when it processes a chunk).
    pub fn return_credit(&self, n: usize) {
        self.credit.fetch_add(n, Ordering::SeqCst);
    }

    /// Get available credit.
    pub fn available(&self) -> usize {
        self.credit.load(Ordering::SeqCst)
    }

    /// Check if we have credit.
    pub fn has_credit(&self) -> bool {
        self.available() > 0
    }

    /// Get the max credit window.
    pub fn max_credit(&self) -> usize {
        self.max_credit
    }
}

/// Handle for an active stream.
pub struct StreamHandle {
    /// Stream ID
    pub id: u64,
    /// Channel name
    pub channel: String,
    /// Stream direction
    pub direction: StreamDirection,
    /// Flow controller for backpressure
    pub flow_control: FlowController,
    /// Channel for sending chunks to the receiver
    chunk_tx: mpsc::Sender<IpcResult<Vec<u8>>>,
    /// Whether the stream is closed
    closed: AtomicBool,
}

impl StreamHandle {
    /// Create a new stream handle.
    pub fn new(
        id: u64,
        channel: String,
        direction: StreamDirection,
        initial_credit: usize,
    ) -> (Self, mpsc::Receiver<IpcResult<Vec<u8>>>) {
        let (chunk_tx, chunk_rx) = mpsc::channel(64);
        let (flow_control, _) = FlowController::new(initial_credit);

        let handle = Self {
            id,
            channel,
            direction,
            flow_control,
            chunk_tx,
            closed: AtomicBool::new(false),
        };

        (handle, chunk_rx)
    }

    /// Send a chunk. Blocks if the credit window is exhausted (backpressure).
    pub async fn send_chunk(
        &self,
        data: Vec<u8>,
        is_final: bool,
    ) -> Result<(), IpcError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(IpcError::StreamAlreadyClosed { stream_id: self.id });
        }

        // Wait for credit
        while !self.flow_control.try_acquire() {
            tokio::task::yield_now().await;
        }

        self.chunk_tx
            .send(Ok(data))
            .await
            .map_err(|_| IpcError::Internal("Stream channel closed".into()))
    }

    /// Close the stream.
    pub async fn close(&self, _reason: &str) -> Result<(), IpcError> {
        self.closed.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Check if the stream is closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }
}

impl std::fmt::Debug for StreamHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamHandle")
            .field("id", &self.id)
            .field("channel", &self.channel)
            .field("direction", &self.direction)
            .field("closed", &self.closed.load(Ordering::Relaxed))
            .finish()
    }
}
