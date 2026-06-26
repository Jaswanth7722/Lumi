//! # Log Sinks
//!
//! A log sink receives formatted byte buffers and writes them to a destination.
//! Sinks are called from the pipeline worker task, never from the application hot path.

pub mod console;
pub mod file;
pub mod memory;
pub mod rotating;

use crate::error::LogError;
use crate::filter::FilterChain;
use crate::formatter::Formatter;
use crate::record::LogRecord;
use async_trait::async_trait;
use std::sync::Arc;

/// A log sink receives formatted byte buffers and writes them to a destination.
///
/// Sinks are called from the pipeline worker task, never from the application
/// hot path. Sink writes are allowed to be async. A sink that fails must
/// return `LogError::SinkWriteFailed` — it must not panic.
#[async_trait]
pub trait Sink: Send + Sync {
    /// Unique name for this sink.
    fn name(&self) -> &'static str;

    /// Write a single formatted record. Called after formatting and redaction.
    async fn write(&self, record: &LogRecord, formatted: &[u8]) -> Result<(), LogError>;

    /// Flush all buffered writes to durable storage.
    async fn flush(&self) -> Result<(), LogError>;

    /// Graceful shutdown: flush remaining records, close file handles.
    async fn shutdown(&self) -> Result<(), LogError>;

    /// Returns the formatter this sink uses.
    fn formatter(&self) -> &dyn Formatter;

    /// Returns the filter chain specific to this sink (in addition to global filters).
    fn filter(&self) -> &FilterChain;
}

/// A type-erased, cloneable handle to a registered sink.
/// Clone is O(1) — just an Arc clone.
#[derive(Clone)]
pub struct SinkHandle(Arc<dyn Sink>);

impl SinkHandle {
    /// Create a new sink handle.
    pub fn new(sink: impl Sink + 'static) -> Self {
        Self(Arc::new(sink))
    }

    /// Create a new sink handle from an Arc.
    pub fn from_arc(sink: Arc<dyn Sink>) -> Self {
        Self(sink)
    }

    /// Get a reference to the inner sink.
    pub fn inner(&self) -> &dyn Sink {
        self.0.as_ref()
    }
}

impl std::ops::Deref for SinkHandle {
    type Target = dyn Sink;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}
