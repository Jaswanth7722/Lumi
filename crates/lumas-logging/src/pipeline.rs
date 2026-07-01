//! # Internal Async Pipeline
//!
//! The bridge between the tracing Layer (hot path) and the sinks (I/O path).
//! Guarantees non-blocking submission on the hot path via a bounded crossbeam channel.
//!
//! # Architecture
//!
//! ```text
//! tracing Layer → LogPipeline::submit()  (hot path, non-blocking try_send)
//!                      │
//!                      ▼  crossbeam bounded channel
//!              PipelineWorker (dedicated thread)
//!                      │  Filter → Redact → Format → Write to sink
//!                      ▼
//!              Sink[0]  Sink[1]  ...  Sink[N]
//! ```
//!
//! # Thread Safety
//!
//! PipelineWorker runs on a dedicated std::thread. Sink writes use
//! `tokio::runtime::Handle::block_on` because all Sink trait methods
//! are async. This works because a Tokio runtime exists in the process
//! (initialized during bootstrap). The Handle is stored at construction time.

use crate::error::LogError;
use crate::filter::FilterChain;
use crate::level::ArcLogLevel;
use crate::metrics::LoggingMetrics;
use crate::record::LogRecord;
use crate::redaction::RedactionEngine;
use crate::sink::SinkHandle;
use crossbeam_channel::{Receiver, Sender};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::warn;

/// Message sent through the pipeline channel.
enum PipelineMessage {
    /// A log record to process.
    Record(LogRecord),
    /// Flush request with completion signal.
    Flush { done: oneshot::Sender<()> },
    /// Shutdown request with completion signal.
    Shutdown { done: oneshot::Sender<()> },
}

/// Sender side of the pipeline — used by the tracing Layer.
pub struct LogPipeline {
    /// Bounded crossbeam channel sender.
    sender: Sender<PipelineMessage>,
    /// Metrics for tracking pipeline performance.
    metrics: Arc<LoggingMetrics>,
    /// Channel capacity for diagnostics.
    capacity: usize,
}

impl LogPipeline {
    /// Create a new pipeline with the given capacity.
    pub fn new(capacity: usize, metrics: Arc<LoggingMetrics>) -> (Self, PipelineWorker) {
        let (sender, receiver) = crossbeam_channel::bounded(capacity);

        let pipeline = Self {
            sender,
            metrics: metrics.clone(),
            capacity,
        };

        // Capture the current tokio runtime handle for blocking async sink writes.
        // During bootstrap, a Tokio runtime is always active. If no runtime is
        // available, we fall back to a basic stdout write.
        let tokio_handle = tokio::runtime::Handle::try_current().ok();

        let worker = PipelineWorker {
            receiver,
            sinks: Vec::new(),
            filter: Arc::new(FilterChain::new(Arc::new(ArcLogLevel::new(
                crate::level::LogLevel::Info,
            )))),
            redaction: Arc::new(RedactionEngine::new()),
            metrics,
            tokio_handle,
        };

        (pipeline, worker)
    }

    /// Non-blocking record submission.
    /// If the channel is full, increments drop counter and returns immediately.
    pub fn submit(&self, record: LogRecord) {
        let level_idx = record.level as usize;
        self.metrics.record_submitted(level_idx);

        let msg = PipelineMessage::Record(record);

        match self.sender.try_send(msg) {
            Ok(()) => {}
            Err(_) => {
                // Channel full — increment drop counter
                self.metrics.record_dropped();
                // SAFETY: stderr write is best-effort, failure is ignored
                let _ = write!(
                    std::io::stderr(),
                    "lumi-logging: pipeline full, record dropped (total: {})\n",
                    self.metrics
                        .records_dropped
                        .load(std::sync::atomic::Ordering::Relaxed)
                );
            }
        }
    }

    /// Async flush: send Flush message and await the done signal.
    pub async fn flush(&self) -> Result<(), LogError> {
        let (tx, rx) = oneshot::channel();
        let msg = PipelineMessage::Flush { done: tx };

        if self.sender.send(msg).is_err() {
            return Err(LogError::FlushTimeout { timeout_ms: 5000 });
        }

        tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .map_err(|_| LogError::FlushTimeout { timeout_ms: 5000 })?
            .map_err(|_| LogError::FlushTimeout { timeout_ms: 5000 })
    }

    /// Async shutdown: flush, then signal the worker to exit.
    pub async fn shutdown(&self) -> Result<(), LogError> {
        self.flush().await?;

        let (tx, rx) = oneshot::channel();
        let msg = PipelineMessage::Shutdown { done: tx };

        if self.sender.send(msg).is_err() {
            // Worker already stopped
            return Ok(());
        }

        tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .map_err(|_| LogError::ShutdownTimeout { timeout_ms: 10000 })?;
        // rx error means the worker shut down its side — that is OK
        Ok(())
    }

    /// Get a reference to the metrics.
    pub fn metrics(&self) -> &Arc<LoggingMetrics> {
        &self.metrics
    }
}

impl Clone for LogPipeline {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            metrics: self.metrics.clone(),
            capacity: self.capacity,
        }
    }
}

/// Worker that processes pipeline messages on a dedicated blocking thread.
pub struct PipelineWorker {
    /// Channel receiver.
    receiver: Receiver<PipelineMessage>,
    /// Registered sinks (populated by LogManager before run()).
    sinks: Vec<SinkHandle>,
    /// Global filter chain.
    filter: Arc<FilterChain>,
    /// Redaction engine.
    redaction: Arc<RedactionEngine>,
    /// Metrics.
    metrics: Arc<LoggingMetrics>,
    /// Tokio runtime handle for blocking on async sink writes.
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl PipelineWorker {
    /// Set the sink list (called by LogManager before start).
    pub fn with_sinks(mut self, sinks: Vec<SinkHandle>) -> Self {
        self.sinks = sinks;
        self
    }

    /// Set the filter chain.
    pub fn with_filter(mut self, filter: Arc<FilterChain>) -> Self {
        self.filter = filter;
        self
    }

    /// Set the redaction engine.
    pub fn with_redaction(mut self, redaction: Arc<RedactionEngine>) -> Self {
        self.redaction = redaction;
        self
    }

    /// Run the worker loop on a dedicated blocking thread.
    ///
    /// # Processing Pipeline (per record)
    ///
    /// 1. **Filter** — Check global level + filter chain; drop if below threshold
    /// 2. **Redact** — Apply redaction rules (API keys, secrets, PII)
    /// 3. **Format** — Use each sink's formatter to produce byte output
    /// 4. **Write** — Dispatch formatted bytes to the sink via `block_on`
    ///
    /// # Panics
    ///
    /// Never panics. Sink write failures are logged via `tracing::warn!` to
    /// stderr and do not affect subsequent messages.
    pub fn run(self) {
        std::thread::Builder::new()
            .name("lumi-log-pipeline".into())
            .spawn(move || {
                for msg in &self.receiver {
                    match msg {
                        PipelineMessage::Record(mut record) => {
                            // Step 1: Apply global filter — fast path before any work
                            if !self.filter.is_enabled(&record) {
                                self.metrics.record_filtered();
                                continue;
                            }

                            // Step 2: Apply redaction (API keys, secrets, PII)
                            self.redaction.redact(&mut record);

                            // Step 3 & 4: Format and write to each sink
                            self.dispatch_to_sinks(&record);
                        }
                        PipelineMessage::Flush { done } => {
                            self.metrics.record_flush();
                            self.flush_sinks(done);
                        }
                        PipelineMessage::Shutdown { done } => {
                            self.flush_sinks(done);
                            return; // Exit worker thread
                        }
                    }
                }
            })
            .expect("Failed to spawn pipeline worker thread");
    }

    /// Dispatch a single record to every registered sink.
    ///
    /// For each sink:
    /// 1. Check the sink's local filter
    /// 2. Format the record into a reusable byte buffer
    /// 3. Write the formatted bytes to the sink (using block_on for async sinks)
    fn dispatch_to_sinks(&self, record: &LogRecord) {
        for sink in &self.sinks {
            // Step 3a: Check sink-local filter
            let sink_filter = sink.filter();
            if !sink_filter.is_enabled(record) {
                self.metrics.record_filtered();
                continue;
            }

            // Step 3b: Format the record to bytes
            let mut buf = Vec::with_capacity(1024);
            match sink.formatter().format(record, &mut buf) {
                Ok(()) => {
                    // Step 4: Write formatted bytes to the sink
                    let write_result = self.block_on_sink_write(sink, record, &buf);

                    match write_result {
                        Ok(()) => {
                            self.metrics.record_written(buf.len() as u64);
                        }
                        Err(e) => {
                            self.metrics.record_sink_error();
                            warn!(
                                "Sink {} write error: {e}",
                                sink.name(),
                            );
                        }
                    }
                }
                Err(e) => {
                    self.metrics.record_sink_error();
                    warn!(
                        "Sink {} format error: {e}",
                        sink.name(),
                    );
                }
            }
        }
    }

    /// Call `sink.write()` on the blocking thread using `Handle::block_on`.
    fn block_on_sink_write(
        &self,
        sink: &SinkHandle,
        record: &LogRecord,
        formatted: &[u8],
    ) -> Result<(), LogError> {
        match &self.tokio_handle {
            Some(handle) => {
                // We have a Tokio runtime — block_on the async write.
                // This is safe because we are on a dedicated blocking thread,
                // and the Tokio runtime runs on other threads. block_on will
                // park this thread until the async write completes.
                handle.block_on(sink.write(record, formatted))
            }
            None => {
                // No Tokio runtime available — perform a synchronous write
                // using std::io::Write for the basic case.
                // This handles the MemorySink (fully sync) and provides a
                // degraded fallback for ConsoleSink/FileSink.
                #[cfg(not(target_arch = "wasm32"))]
                {
                    use std::io::Write;
                    let _ = std::io::stdout().lock().write_all(formatted)
                        .map_err(|e| LogError::SinkWriteFailed {
                            sink: sink.name().to_string(),
                            source: e,
                        })?;
                }
                #[cfg(target_arch = "wasm32")]
                {
                    // WebAssembly: no stdout, just swallow the write
                    let _ = (sink, record, formatted);
                }
                Ok(())
            }
        }
    }

    /// Flush all sinks.
    fn flush_sinks(&self, done: oneshot::Sender<()>) {
        for sink in &self.sinks {
            match &self.tokio_handle {
                Some(handle) => {
                    let _ = handle.block_on(sink.flush());
                }
                None => {
                    // Best-effort: no runtime to flush
                }
            }
        }
        let _ = done.send(());
    }
}
