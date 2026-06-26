//! # Memory Sink
//!
//! In-memory circular buffer of recent log records.
//! Used by the diagnostics system and test assertions.

use crate::error::LogError;
use crate::filter::FilterChain;
use crate::formatter::Formatter;
use crate::formatter::pretty::PrettyFormatter;
use crate::level::ArcLogLevel;
use crate::record::LogRecord;
use crate::sink::Sink;
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// In-memory circular buffer of recent log records.
pub struct MemorySink {
    /// Ring buffer of records.
    buffer: Arc<parking_lot::RwLock<VecDeque<LogRecord>>>,
    /// Maximum capacity.
    capacity: usize,
    /// Pretty formatter (used for compatibility).
    formatter: PrettyFormatter,
    /// Sink-local filter.
    filter: FilterChain,
    /// Count of dropped records.
    dropped: AtomicU64,
}

impl MemorySink {
    /// Create a new memory sink with the given capacity.
    pub fn new(capacity: usize, global_level: Arc<ArcLogLevel>) -> Self {
        Self {
            buffer: Arc::new(parking_lot::RwLock::new(VecDeque::with_capacity(capacity))),
            capacity,
            formatter: PrettyFormatter::new(false),
            filter: FilterChain::new(global_level),
            dropped: AtomicU64::new(0),
        }
    }

    /// Returns a snapshot of all records currently in the buffer.
    pub fn records(&self) -> Vec<LogRecord> {
        self.buffer.read().iter().cloned().collect()
    }

    /// Returns records matching a filter predicate.
    pub fn search(&self, predicate: impl Fn(&LogRecord) -> bool) -> Vec<LogRecord> {
        self.buffer
            .read()
            .iter()
            .filter(|r| predicate(r))
            .cloned()
            .collect()
    }

    /// Returns the most recent `n` records.
    pub fn tail(&self, n: usize) -> Vec<LogRecord> {
        let guard = self.buffer.read();
        let len = guard.len();
        guard.iter().skip(len.saturating_sub(n)).cloned().collect()
    }

    /// Clears the buffer (used between tests).
    pub fn clear(&self) {
        self.buffer.write().clear();
    }

    /// Count of records dropped due to buffer overflow.
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl Sink for MemorySink {
    fn name(&self) -> &'static str {
        "memory"
    }

    async fn write(&self, record: &LogRecord, _formatted: &[u8]) -> Result<(), LogError> {
        let mut guard = self.buffer.write();
        if guard.len() >= self.capacity {
            guard.pop_front();
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        guard.push_back(record.clone());
        Ok(())
    }

    async fn flush(&self) -> Result<(), LogError> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), LogError> {
        Ok(())
    }

    fn formatter(&self) -> &dyn Formatter {
        &self.formatter
    }

    fn filter(&self) -> &FilterChain {
        &self.filter
    }
}
