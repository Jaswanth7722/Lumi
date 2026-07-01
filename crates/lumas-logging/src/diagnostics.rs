//! # Diagnostics API
//!
//! Provides search, export, and crash report capabilities on the in-memory log buffer.

use crate::error::LogError;
use crate::level::LogLevel;
use crate::metrics::{LoggingMetrics, LoggingMetricsSnapshot};
use crate::record::LogRecord;
use crate::sink::memory::MemorySink;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// Diagnostics API for querying the in-memory log buffer.
pub struct LogDiagnostics {
    /// Reference to the memory sink.
    memory_sink: Arc<MemorySink>,
    /// Reference to the logging metrics.
    metrics: Arc<LoggingMetrics>,
}

impl LogDiagnostics {
    /// Create a new diagnostics handle.
    pub fn new(memory_sink: Arc<MemorySink>, metrics: Arc<LoggingMetrics>) -> Self {
        Self {
            memory_sink,
            metrics,
        }
    }

    /// Returns the most recent `n` log records across all levels.
    pub fn recent(&self, n: usize) -> Vec<LogRecord> {
        self.memory_sink.tail(n)
    }

    /// Search records in the memory buffer by multiple criteria.
    pub fn search(&self, query: DiagnosticsQuery) -> Vec<LogRecord> {
        self.memory_sink
            .search(|record| {
                if let Some(ref min_level) = query.level_min {
                    if record.level < *min_level {
                        return false;
                    }
                }
                if let Some(ref subsystem) = query.subsystem {
                    if record.context.subsystem.as_deref() != Some(subsystem) {
                        return false;
                    }
                }
                if let Some(ref correlation_id) = query.correlation_id {
                    if record.context.correlation_id != Some(*correlation_id) {
                        return false;
                    }
                }
                if let Some(ref msg_pattern) = query.message_contains {
                    if !record.message.contains(msg_pattern) {
                        return false;
                    }
                }
                if let Some(ref after) = query.after {
                    if record.timestamp < *after {
                        return false;
                    }
                }
                if let Some(ref before) = query.before {
                    if record.timestamp > *before {
                        return false;
                    }
                }
                true
            })
            .into_iter()
            .take(query.limit.unwrap_or(usize::MAX))
            .collect()
    }

    /// Export all records in the memory buffer to a JSON string.
    pub fn export_json(&self) -> Result<String, LogError> {
        serde_json::to_string_pretty(&self.memory_sink.records()).map_err(|e| {
            LogError::ExportFailed {
                reason: format!("JSON serialization failed: {e}"),
            }
        })
    }

    /// Export all records to a file path.
    pub async fn export_to_file(&self, path: &Path) -> Result<(), LogError> {
        let json = self.export_json()?;
        tokio::fs::write(path, &json)
            .await
            .map_err(|e| LogError::ExportFailed {
                reason: format!("File write failed: {e}"),
            })?;
        Ok(())
    }

    /// Collect a crash report: last 500 records + current metrics snapshot.
    pub fn crash_report(&self) -> CrashReport {
        CrashReport {
            generated_at: Utc::now(),
            recent_records: self.recent(500),
            metrics: self.metrics.snapshot(),
            lumi_version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
        }
    }

    /// Returns the current metrics snapshot.
    pub fn metrics(&self) -> LoggingMetricsSnapshot {
        self.metrics.snapshot()
    }
}

/// Query parameters for searching log records.
#[derive(Debug, Default)]
pub struct DiagnosticsQuery {
    /// Minimum log level.
    pub level_min: Option<LogLevel>,
    /// Subsystem name filter.
    pub subsystem: Option<String>,
    /// Correlation ID filter.
    pub correlation_id: Option<Uuid>,
    /// Message substring filter.
    pub message_contains: Option<String>,
    /// Only records after this timestamp.
    pub after: Option<DateTime<Utc>>,
    /// Only records before this timestamp.
    pub before: Option<DateTime<Utc>>,
    /// Maximum number of records to return.
    pub limit: Option<usize>,
}

/// A crash report containing recent records and metrics.
#[derive(Debug, Serialize)]
pub struct CrashReport {
    /// When the report was generated.
    pub generated_at: DateTime<Utc>,
    /// Recent log records (up to 500).
    pub recent_records: Vec<LogRecord>,
    /// Metrics snapshot.
    pub metrics: LoggingMetricsSnapshot,
    /// Lumas version string.
    pub lumi_version: String,
    /// Platform identifier.
    pub platform: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::LogLevel;
    use crate::record::LogRecord;

    #[test]
    fn test_crash_report_contains_fields() {
        let metrics = Arc::new(LoggingMetrics::new());
        let memory_sink = Arc::new(MemorySink::new(
            10000,
            Arc::new(crate::level::AtomicLogLevel::new(LogLevel::Trace)),
        ));
        let diagnostics = LogDiagnostics::new(memory_sink, metrics);

        let report = diagnostics.crash_report();
        assert!(report.generated_at <= Utc::now());
        assert_eq!(report.lumi_version, env!("CARGO_PKG_VERSION"));
    }
}
