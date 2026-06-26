//! # Formatters
//!
//! Formatters convert `LogRecord` instances into formatted byte buffers
//! for different output destinations.

pub mod json;
pub mod pretty;

use crate::error::LogError;
use crate::record::LogRecord;

/// A formatter converts a LogRecord into a formatted byte buffer.
///
/// The buffer is reused across calls to minimize allocation.
/// Implementors must clear and refill the buffer, not append.
pub trait Formatter: Send + Sync {
    /// Unique name for this formatter.
    fn name(&self) -> &'static str;

    /// Format a single log record into a byte buffer.
    fn format(&self, record: &LogRecord, buf: &mut Vec<u8>) -> Result<(), LogError>;
}
