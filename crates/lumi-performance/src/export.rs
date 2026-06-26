//! # Metrics Export Pipeline
//!
//! Exports performance snapshots to configured destinations.
//! Supports JSON file export (always available), Prometheus exposition format,
//! and OpenTelemetry OTLP export (feature-gated).
//!
//! # Thread Safety
//! Exporters are `Send + Sync`.

use crate::error::{PerformanceError, PerformanceResult};
use crate::manager::PerformanceSnapshot;
use async_trait::async_trait;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Metric exporter trait.
#[async_trait]
pub trait MetricExporter: Send + Sync {
    /// Exporter name.
    fn name(&self) -> &'static str;
    /// Export a snapshot.
    async fn export(&self, snapshot: &PerformanceSnapshot) -> PerformanceResult<()>;
}

/// JSON file exporter — writes `performance-{timestamp}.json` to a directory.
pub struct JsonFileExporter {
    /// Output directory.
    output_dir: PathBuf,
    /// Maximum number of files to retain.
    max_files: usize,
}

impl JsonFileExporter {
    /// Create a new JSON file exporter.
    pub fn new(output_dir: PathBuf, max_files: usize) -> Self {
        Self {
            output_dir,
            max_files,
        }
    }
}

#[async_trait]
impl MetricExporter for JsonFileExporter {
    fn name(&self) -> &'static str {
        "json_file"
    }

    async fn export(&self, snapshot: &PerformanceSnapshot) -> PerformanceResult<()> {
        // Ensure output directory exists
        tokio::fs::create_dir_all(&self.output_dir)
            .await
            .map_err(|e| PerformanceError::ExportFailed {
                exporter: "json_file",
                reason: e.to_string(),
            })?;

        // Generate filename with timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let filename = format!("performance-{}.json", timestamp);
        let path = self.output_dir.join(&filename);

        // Serialize snapshot to JSON
        let json =
            serde_json::to_string_pretty(snapshot).map_err(|e| PerformanceError::ExportFailed {
                exporter: "json_file",
                reason: e.to_string(),
            })?;

        // Write to temp file first, then rename (atomic write)
        let temp_path = self.output_dir.join(format!("{}.tmp", filename));
        tokio::fs::write(&temp_path, &json)
            .await
            .map_err(|e| PerformanceError::ExportFailed {
                exporter: "json_file",
                reason: e.to_string(),
            })?;
        tokio::fs::rename(&temp_path, &path)
            .await
            .map_err(|e| PerformanceError::ExportFailed {
                exporter: "json_file",
                reason: e.to_string(),
            })?;

        // Rotate old files
        self.rotate_old_files().await?;

        Ok(())
    }
}

impl JsonFileExporter {
    /// Remove old export files beyond the max limit.
    async fn rotate_old_files(&self) -> PerformanceResult<()> {
        let mut entries = tokio::fs::read_dir(&self.output_dir).await.map_err(|e| {
            PerformanceError::ExportFailed {
                exporter: "json_file",
                reason: e.to_string(),
            }
        })?;

        let mut files: Vec<PathBuf> = Vec::new();
        while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && !path.to_string_lossy().contains(".tmp")
            {
                files.push(path);
            }
        }

        // Sort by modified time and remove oldest beyond limit
        files.sort_by(|a, b| {
            std::fs::metadata(a)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
                .cmp(
                    &std::fs::metadata(b)
                        .and_then(|m| m.modified())
                        .unwrap_or(SystemTime::UNIX_EPOCH),
                )
        });

        while files.len() > self.max_files {
            if let Some(oldest) = files.first() {
                tokio::fs::remove_file(oldest).await.ok();
            }
            files.remove(0);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::PerformanceSnapshot;
    use std::time::Instant;

    #[test]
    fn test_exporter_name() {
        let exporter = JsonFileExporter::new(PathBuf::from("/tmp/metrics"), 48);
        assert_eq!(exporter.name(), "json_file");
    }

    #[tokio::test]
    async fn test_json_export() {
        let dir = tempfile::tempdir().unwrap();
        let exporter = JsonFileExporter::new(dir.path().to_path_buf(), 10);
        let snapshot = PerformanceSnapshot {
            timestamp: Instant::now(),
            metrics: vec![],
            active_alerts: vec![],
            cpu_percent: 25.0,
            memory_rss_mb: 256.0,
            fps: 60.0,
            uptime_seconds: 3600,
        };
        let result = exporter.export(&snapshot).await;
        assert!(result.is_ok());
    }
}
