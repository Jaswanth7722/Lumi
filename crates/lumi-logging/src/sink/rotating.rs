//! # Rotating File Sink
//!
//! File sink with automatic rotation by size and/or time.

use crate::error::LogError;
use crate::filter::FilterChain;
use crate::formatter::Formatter;
use crate::formatter::json::JsonFormatter;
use crate::level::ArcLogLevel;
use crate::record::LogRecord;
use crate::rotation::RotationPolicy;
use crate::sink::Sink;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

/// State of the rotating file sink.
struct RotatingState {
    /// Buffered writer for the current file.
    file: std::io::BufWriter<std::fs::File>,
    /// Current file path (symlink/pointer to latest).
    current_path: PathBuf,
    /// Current file size in bytes.
    current_size_bytes: u64,
    /// Record count in current file.
    record_count: u64,
    /// When the current file was opened.
    opened_at: DateTime<Utc>,
    /// Count of rotated files.
    rotation_count: u32,
}

/// File sink with automatic rotation and compression.
pub struct RotatingFileSink {
    /// Base path for log files (e.g., "logs/lumi.log").
    base_path: PathBuf,
    /// Rotation policy.
    rotation: RotationPolicy,
    /// JSON formatter.
    formatter: JsonFormatter,
    /// Sink-local filter chain.
    filter: FilterChain,
    /// Current state.
    state: Arc<parking_lot::Mutex<RotatingState>>,
    /// Maximum number of rotated files to retain.
    max_files: u32,
    /// Whether to compress rotated files.
    compress: bool,
}

impl RotatingFileSink {
    /// Create a new rotating file sink.
    pub fn new(
        base_path: PathBuf,
        rotation: RotationPolicy,
        global_level: Arc<ArcLogLevel>,
        max_files: u32,
        compress: bool,
    ) -> Result<Self, LogError> {
        // Create parent directories
        if let Some(parent) = base_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| LogError::SinkWriteFailed {
                sink: format!("rotating:{}", base_path.display()),
                source: e,
            })?;
        }

        // Open initial file
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&base_path)
            .map_err(|e| LogError::SinkWriteFailed {
                sink: format!("rotating:{}", base_path.display()),
                source: e,
            })?;

        let metadata = file.metadata().ok();
        let current_size = metadata.map(|m| m.len()).unwrap_or(0);

        let state = RotatingState {
            file: std::io::BufWriter::with_capacity(64 * 1024, file),
            current_path: base_path.clone(),
            current_size_bytes: current_size,
            record_count: 0,
            opened_at: Utc::now(),
            rotation_count: 0,
        };

        Ok(Self {
            base_path,
            rotation,
            formatter: JsonFormatter::new(),
            filter: FilterChain::new(global_level),
            state: Arc::new(parking_lot::Mutex::new(state)),
            max_files,
            compress,
        })
    }

    /// Perform rotation: close current file, rename, compress, open new.
    fn rotate(&self, state: &mut RotatingState) -> Result<(), LogError> {
        // Flush and close current file
        state.file.flush().map_err(|e| LogError::SinkWriteFailed {
            sink: format!("rotating:{}", self.base_path.display()),
            source: e,
        })?;

        // Generate timestamped name
        let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%S");
        let rotated_name = format!(
            "{}-{}.log",
            self.base_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("lumi"),
            timestamp
        );
        let rotated_path = self.base_path.with_file_name(&rotated_name);

        // Rename current file to timestamped name
        std::fs::rename(&self.base_path, &rotated_path).map_err(|e| LogError::RotationFailed {
            path: self.base_path.clone(),
            reason: format!("rename failed: {e}"),
        })?;

        // Open new file at base path
        let new_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.base_path)
            .map_err(|e| LogError::SinkWriteFailed {
                sink: format!("rotating:{}", self.base_path.display()),
                source: e,
            })?;

        // Compress rotated file (in current thread for simplicity)
        if self.compress {
            let gz_path = rotated_path.with_extension("log.gz");
            if let Err(e) = Self::compress_file(&rotated_path, &gz_path) {
                // Non-fatal: log error, keep uncompressed
                tracing::warn!("Failed to compress rotated log file: {e}");
            } else {
                // Delete uncompressed file after successful compression
                let _ = std::fs::remove_file(&rotated_path);
            }
        }

        // Enforce max_files limit
        Self::enforce_max_files(&self.base_path, self.max_files);

        // Update state
        state.file = std::io::BufWriter::with_capacity(64 * 1024, new_file);
        state.current_size_bytes = 0;
        state.record_count = 0;
        state.opened_at = Utc::now();
        state.rotation_count += 1;

        Ok(())
    }

    /// Compress a file with gzip.
    fn compress_file(source: &PathBuf, dest: &PathBuf) -> Result<(), LogError> {
        use std::io::Read;
        let mut input = std::fs::File::open(source).map_err(|e| LogError::CompressionFailed {
            path: source.clone(),
            source: e,
        })?;
        let output = std::fs::File::create(dest).map_err(|e| LogError::CompressionFailed {
            path: dest.clone(),
            source: e,
        })?;
        let mut encoder = flate2::write::GzEncoder::new(output, flate2::Compression::Default);
        let mut buf = Vec::new();
        input
            .read_to_end(&mut buf)
            .map_err(|e| LogError::CompressionFailed {
                path: source.clone(),
                source: e,
            })?;
        encoder
            .write_all(&buf)
            .map_err(|e| LogError::CompressionFailed {
                path: dest.clone(),
                source: e,
            })?;
        encoder.finish().map_err(|e| LogError::CompressionFailed {
            path: dest.clone(),
            source: e,
        })?;
        Ok(())
    }

    /// Remove old rotated files beyond the max count.
    fn enforce_max_files(base_path: &PathBuf, max_files: u32) {
        let dir = base_path.parent().unwrap_or(std::path::Path::new("."));
        let stem = base_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("lumi");

        let mut files: Vec<_> = std::fs::read_dir(dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with(stem) && s != stem)
                    .unwrap_or(false)
            })
            .collect();

        // Sort by modified time (oldest first)
        files.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

        // Remove oldest files beyond max_files
        while files.len() > max_files as usize {
            if let Some(oldest) = files.first() {
                let _ = std::fs::remove_file(oldest.path());
                files.remove(0);
            }
        }
    }
}

#[async_trait]
impl Sink for RotatingFileSink {
    fn name(&self) -> &'static str {
        "rotating_file"
    }

    async fn write(&self, record: &LogRecord, _formatted: &[u8]) -> Result<(), LogError> {
        let mut buf = Vec::with_capacity(1024);
        self.formatter.format(record, &mut buf)?;

        let mut state = self.state.lock();

        // Check rotation triggers
        let elapsed = (Utc::now() - state.opened_at).to_std().unwrap_or_default();
        let should_rotate = self
            .rotation
            .should_rotate_by_size(state.current_size_bytes)
            || self.rotation.should_rotate_by_time(elapsed);

        if should_rotate {
            self.rotate(&mut state)?;
        }

        use std::io::Write;
        state
            .file
            .write_all(&buf)
            .map_err(|e| LogError::SinkWriteFailed {
                sink: format!("rotating:{}", self.base_path.display()),
                source: e,
            })?;

        state.current_size_bytes += buf.len() as u64;
        state.record_count += 1;

        Ok(())
    }

    async fn flush(&self) -> Result<(), LogError> {
        let mut state = self.state.lock();
        state.file.flush().map_err(|e| LogError::SinkWriteFailed {
            sink: format!("rotating:{}", self.base_path.display()),
            source: e,
        })?;
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), LogError> {
        self.flush().await?;
        let mut state = self.state.lock();
        state.file.flush().map_err(|e| LogError::SinkWriteFailed {
            sink: format!("rotating:{}", self.base_path.display()),
            source: e,
        })?;
        Ok(())
    }

    fn formatter(&self) -> &dyn Formatter {
        &self.formatter
    }

    fn filter(&self) -> &FilterChain {
        &self.filter
    }
}
