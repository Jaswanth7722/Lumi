//! # File Sink
//!
//! Buffered, async file logging with background flush.

use crate::error::LogError;
use crate::filter::FilterChain;
use crate::formatter::Formatter;
use crate::formatter::json::JsonFormatter;
use crate::level::ArcLogLevel;
use crate::record::LogRecord;
use crate::sink::Sink;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, BufWriter};

/// File log sink with buffered async writes.
pub struct FileSink {
    /// Path to the log file.
    path: PathBuf,
    /// Buffered writer wrapping the file.
    writer: Arc<tokio::sync::Mutex<Option<BufWriter<tokio::fs::File>>>>,
    /// JSON formatter.
    formatter: JsonFormatter,
    /// Sink-local filter chain.
    filter: FilterChain,
    /// Buffer size.
    buffer_size: usize,
}

impl FileSink {
    /// Create a new file sink.
    pub async fn new(path: PathBuf, global_level: Arc<ArcLogLevel>) -> Result<Self, LogError> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| LogError::SinkWriteFailed {
                    sink: format!("file:{}", path.display()),
                    source: e,
                })?;
        }

        // Open file with append and create
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .map_err(|e| LogError::SinkWriteFailed {
                sink: format!("file:{}", path.display()),
                source: e,
            })?;

        let writer = BufWriter::with_capacity(64 * 1024, file); // 64KB buffer

        Ok(Self {
            path,
            writer: Arc::new(tokio::sync::Mutex::new(Some(writer))),
            formatter: JsonFormatter::new(),
            filter: FilterChain::new(global_level),
            buffer_size: 64 * 1024,
        })
    }

    /// Get the file path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

#[async_trait]
impl Sink for FileSink {
    fn name(&self) -> &'static str {
        "file"
    }

    async fn write(&self, record: &LogRecord, _formatted: &[u8]) -> Result<(), LogError> {
        let mut buf = Vec::with_capacity(1024);
        self.formatter.format(record, &mut buf)?;

        let mut guard = self.writer.lock().await;
        if let Some(ref mut writer) = *guard {
            writer
                .write_all(&buf)
                .await
                .map_err(|e| LogError::SinkWriteFailed {
                    sink: format!("file:{}", self.path.display()),
                    source: e,
                })?;
        }

        Ok(())
    }

    async fn flush(&self) -> Result<(), LogError> {
        let mut guard = self.writer.lock().await;
        if let Some(ref mut writer) = *guard {
            writer
                .flush()
                .await
                .map_err(|e| LogError::SinkWriteFailed {
                    sink: format!("file:{}", self.path.display()),
                    source: e,
                })?;
        }
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), LogError> {
        self.flush().await?;
        let mut guard = self.writer.lock().await;
        if let Some(writer) = guard.take() {
            let file = writer.into_inner();
            file.sync_all()
                .await
                .map_err(|e| LogError::SinkWriteFailed {
                    sink: format!("file:{}", self.path.display()),
                    source: e,
                })?;
        }
        Ok(())
    }

    fn formatter(&self) -> &dyn Formatter {
        &self.formatter
    }

    fn filter(&self) -> &FilterChain {
        &self.filter
    }
}
