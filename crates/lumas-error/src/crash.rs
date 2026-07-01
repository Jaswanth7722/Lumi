//! # Crash Report
//!
//! Represents a complete crash report with all diagnostic information.
//! Written atomically to disk using temp-file + rename pattern.
//!
//! # Thread Safety
//! CrashReport is Send + Sync and can be safely shared across threads.

use crate::error::LumasError;
use crate::stacktrace::StackTrace;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// The type of crash that occurred.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CrashType {
    /// A Rust panic occurred.
    Panic {
        /// The panic message.
        message: String,
    },
    /// A fatal LumasError was generated.
    FatalError,
    /// Out-of-memory kill.
    OomKill,
    /// OS signal received.
    SignalReceived {
        /// The signal number.
        signal: i32,
    },
    /// Watchdog timeout.
    Watchdog {
        /// The component that timed out.
        component: String,
        /// The timeout duration in seconds.
        timeout_secs: u64,
    },
}

/// Information about a panic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanicInfo {
    /// The panic message.
    pub message: String,
    /// Source file where the panic occurred.
    pub file: Option<String>,
    /// Source line where the panic occurred.
    pub line: Option<u32>,
}

/// Snapshot of a thread at crash time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSnapshot {
    /// Thread ID.
    pub thread_id: u64,
    /// Thread name.
    pub name: Option<String>,
    /// Whether this is an async task.
    pub is_async: bool,
    /// Stack trace for this thread.
    pub stack_trace: Option<String>,
}

/// Memory usage snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnapshot {
    /// Resident set size in bytes.
    pub rss_bytes: u64,
    /// Virtual memory size in bytes.
    pub vms_bytes: u64,
    /// Heap usage in bytes.
    pub heap_bytes: u64,
}

/// Information about a loaded module/library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    /// Module name.
    pub name: String,
    /// Module version.
    pub version: String,
}

/// State of a service at crash time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    /// Service name.
    pub name: String,
    /// Whether the service was running.
    pub running: bool,
}

/// State of a task at crash time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    /// Task ID (String since task IDs are opaque).
    pub id: String,
    /// Task description.
    pub description: Option<String>,
}

/// A log entry for inclusion in crash reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log message.
    pub message: String,
    /// Log level.
    pub level: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Environment snapshot at crash time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSnapshot {
    /// OS name.
    pub os: String,
    /// OS version.
    pub os_version: String,
    /// CPU architecture.
    pub arch: String,
    /// Number of CPU cores.
    pub num_cpus: usize,
    /// Total system memory in bytes.
    pub total_memory_bytes: u64,
    /// Lumas data directory.
    pub data_dir: String,
    /// GPU info if available.
    pub gpu: Option<String>,
}

impl EnvironmentSnapshot {
    /// Capture the current environment.
    pub fn capture() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            os_version: String::new(), // Would need platform-specific API
            arch: std::env::consts::ARCH.to_string(),
            num_cpus: num_cpus::get(),
            total_memory_bytes: 0, // Would need platform-specific API
            data_dir: String::new(),
            gpu: None,
        }
    }
}

/// Sanitized config snapshot (secrets redacted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedConfig {
    /// A JSON blob of the config with secrets redacted.
    pub config_json: String,
}

/// A complete, atomic crash report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    /// Unique crash report ID.
    pub id: Uuid,
    /// When the crash occurred.
    pub timestamp: DateTime<Utc>,
    /// The type of crash.
    pub crash_type: CrashType,
    /// The originating error, if applicable.
    pub error: Option<LumasError>,
    /// Panic information, if applicable.
    pub panic_info: Option<PanicInfo>,
    /// Stack trace.
    pub stack_trace: StackTrace,
    /// Thread dump.
    pub thread_dump: Vec<ThreadSnapshot>,
    /// Memory snapshot.
    pub memory: MemorySnapshot,
    /// Loaded modules.
    pub loaded_modules: Vec<ModuleInfo>,
    /// Active services.
    pub active_services: Vec<ServiceState>,
    /// Active tasks.
    pub active_tasks: Vec<TaskState>,
    /// Recent log entries (bounded to 200).
    pub recent_logs: Vec<LogEntry>,
    /// Sanitized config snapshot.
    pub config_snapshot: SanitizedConfig,
    /// Environment snapshot.
    pub environment: EnvironmentSnapshot,
    /// Lumas version.
    pub lumi_version: String,
    /// Rust compiler version.
    pub rust_version: &'static str,
}

impl CrashReport {
    /// Create a new crash report with the given crash type.
    pub fn new(crash_type: CrashType) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            crash_type,
            error: None,
            panic_info: None,
            stack_trace: StackTrace::capture(),
            thread_dump: Vec::new(),
            memory: MemorySnapshot {
                rss_bytes: 0,
                vms_bytes: 0,
                heap_bytes: 0,
            },
            loaded_modules: Vec::new(),
            active_services: Vec::new(),
            active_tasks: Vec::new(),
            recent_logs: Vec::new(),
            config_snapshot: SanitizedConfig {
                config_json: String::new(),
            },
            environment: EnvironmentSnapshot::capture(),
            lumi_version: env!("CARGO_PKG_VERSION").to_string(),
            rust_version: rustc_version(),
        }
    }

    /// Set the originating error.
    pub fn with_error(mut self, error: LumasError) -> Self {
        self.error = Some(error);
        self
    }

    /// Set panic info.
    pub fn with_panic_info(mut self, info: PanicInfo) -> Self {
        self.panic_info = Some(info);
        self
    }

    /// Add a thread snapshot.
    pub fn with_thread(mut self, thread: ThreadSnapshot) -> Self {
        self.thread_dump.push(thread);
        self
    }

    /// Set memory snapshot.
    pub fn with_memory(mut self, memory: MemorySnapshot) -> Self {
        self.memory = memory;
        self
    }

    /// Write the crash report atomically to the given directory.
    ///
    /// Uses a temp file + rename pattern to ensure atomic writes.
    /// If the write fails mid-way, no partial crash report is left behind.
    ///
    /// # Errors
    /// Returns an IO error if the write fails.
    pub fn write_to_dir(&self, dir: &std::path::Path) -> Result<PathBuf, std::io::Error> {
        std::fs::create_dir_all(dir)?;

        let filename = format!("crash_{}.json", self.id);
        let final_path = dir.join(&filename);

        // Write to a temp file first, then rename (atomic on most filesystems)
        let temp_path = dir.join(format!("{}.tmp", self.id));
        {
            let file = std::fs::File::create(&temp_path)?;
            serde_json::to_writer_pretty(file, self)?;
        }
        std::fs::rename(&temp_path, &final_path)?;

        // Also update the crash index
        let index_path = dir.join("crash_index.json");
        update_crash_index(&index_path, &self)?;

        Ok(final_path)
    }

    /// Write a fallback minimalist crash report when allocation may be unsafe.
    ///
    /// This is the last-resort path called from the panic handler when the
    /// process may be in an unsafe state. It writes a minimal JSON line
    /// without allocating for the full CrashReport struct.
    pub fn write_fallback(id: Uuid, message: &str) -> Result<PathBuf, std::io::Error> {
        let data_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let crash_dir = data_dir.join("crashes");
        std::fs::create_dir_all(&crash_dir)?;

        let filename = format!("crash_{}.fallback.json", id);
        let path = crash_dir.join(&filename);

        let fallback = serde_json::json!({
            "id": id.to_string(),
            "timestamp": Utc::now().to_rfc3339(),
            "crash_type": "Panic",
            "panic_message": message,
            "fallback": true,
        });

        // Write directly (no temp file — we're in an emergency path)
        let file = std::fs::File::create(&path)?;
        serde_json::to_writer(file, &fallback)?;

        Ok(path)
    }
}

/// Update the crash index file with a new entry.
fn update_crash_index(path: &std::path::Path, report: &CrashReport) -> Result<(), std::io::Error> {
    let mut index: Vec<serde_json::Value> = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    index.push(serde_json::json!({
        "id": report.id.to_string(),
        "timestamp": report.timestamp.to_rfc3339(),
        "crash_type": format!("{:?}", report.crash_type),
        "lumi_version": report.lumi_version,
    }));

    // Keep only the last 50 entries
    while index.len() > 50 {
        index.remove(0);
    }

    let temp_path = path.with_extension("tmp");
    let file = std::fs::File::create(&temp_path)?;
    serde_json::to_writer_pretty(file, &index)?;
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

/// Return the Rust compiler version string.
fn rustc_version() -> &'static str {
    option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_crash_report_creation() {
        let report = CrashReport::new(CrashType::FatalError);
        assert_eq!(report.crash_type.to_string(), "fatal_error");
        assert!(report.error.is_none());
    }

    #[test]
    fn test_crash_report_atomic_write() {
        let dir = tempdir().unwrap();
        let report = CrashReport::new(CrashType::Panic {
            message: "test panic".into(),
        });
        let path = report.write_to_dir(dir.path()).unwrap();
        assert!(path.exists());

        // Verify it's valid JSON
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["crash_type"], "panic");
    }

    #[test]
    fn test_crash_report_fallback_write() {
        let id = Uuid::new_v4();
        let path = CrashReport::write_fallback(id, "emergency panic").unwrap();
        assert!(path.exists());

        // Clean up
        let _ = std::fs::remove_file(&path);
    }
}

impl std::fmt::Display for CrashType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CrashType::Panic { .. } => write!(f, "panic"),
            CrashType::FatalError => write!(f, "fatal_error"),
            CrashType::OomKill => write!(f, "oom_kill"),
            CrashType::SignalReceived { .. } => write!(f, "signal"),
            CrashType::Watchdog { .. } => write!(f, "watchdog"),
        }
    }
}
