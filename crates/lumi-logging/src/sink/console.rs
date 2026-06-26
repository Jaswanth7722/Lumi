//! # Console Sink
//!
//! Writes formatted log records to stdout or stderr.

use crate::config::ConsoleStream;
use crate::error::LogError;
use crate::filter::FilterChain;
use crate::formatter::Formatter;
use crate::formatter::pretty::PrettyFormatter;
use crate::level::ArcLogLevel;
use crate::record::LogRecord;
use crate::sink::Sink;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

/// Console log sink writing to stdout or stderr.
pub struct ConsoleSink {
    /// Pretty formatter for human-readable output.
    formatter: PrettyFormatter,
    /// Sink-local filter chain.
    filter: FilterChain,
    /// Which stream to write to.
    stream: ConsoleStream,
    /// Buffer for formatted output.
    buf: parking_lot::Mutex<Vec<u8>>,
}

impl ConsoleSink {
    /// Create a new console sink.
    pub fn new(use_colors: bool, stream: ConsoleStream, global_level: Arc<ArcLogLevel>) -> Self {
        Self {
            formatter: PrettyFormatter::new(use_colors),
            filter: FilterChain::new(global_level),
            stream,
            buf: parking_lot::Mutex::new(Vec::with_capacity(4096)),
        }
    }
}

#[async_trait]
impl Sink for ConsoleSink {
    fn name(&self) -> &'static str {
        "console"
    }

    async fn write(&self, record: &LogRecord, _formatted: &[u8]) -> Result<(), LogError> {
        let mut buf = self.buf.lock();
        self.formatter.format(record, &mut buf)?;

        let (stdout, stderr) = (tokio::io::stdout(), tokio::io::stderr());

        match self.stream {
            ConsoleStream::Stdout => {
                let mut out = stdout;
                out.write_all(&buf)
                    .await
                    .map_err(|e| LogError::SinkWriteFailed {
                        sink: "console".into(),
                        source: e,
                    })?;
            }
            ConsoleStream::Stderr => {
                let mut out = stderr;
                out.write_all(&buf)
                    .await
                    .map_err(|e| LogError::SinkWriteFailed {
                        sink: "console".into(),
                        source: e,
                    })?;
            }
            ConsoleStream::Auto => {
                if record.level >= crate::level::LogLevel::Warn {
                    let mut out = stderr;
                    out.write_all(&buf)
                        .await
                        .map_err(|e| LogError::SinkWriteFailed {
                            sink: "console".into(),
                            source: e,
                        })?;
                } else {
                    let mut out = stdout;
                    out.write_all(&buf)
                        .await
                        .map_err(|e| LogError::SinkWriteFailed {
                            sink: "console".into(),
                            source: e,
                        })?;
                }
            }
        }

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
