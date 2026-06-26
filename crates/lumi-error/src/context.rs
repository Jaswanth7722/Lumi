//! # Error Context System
//!
//! Captures environment, location, causal chain, and thread/process information
//! for every error without requiring manual instrumentation at call sites.

use chrono::{DateTime, Utc};
use std::fmt;
use std::time::SystemTime;
use uuid::Uuid;

/// Source code location where an error was created or propagated.
#[derive(Debug, Clone)]
pub struct SourceLocation {
    /// Source file name.
    pub file: &'static str,
    /// Line number.
    pub line: u32,
    /// Column number.
    pub column: u32,
    /// Function name.
    pub function: &'static str,
}

impl SourceLocation {
    /// Create a source location at compile time.
    pub const fn from_parts(file: &'static str, line: u32, column: u32) -> Self {
        Self {
            file,
            line,
            column,
            function: "",
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// Thread information at the time of error creation.
#[derive(Debug, Clone)]
pub struct ThreadInfo {
    /// Thread ID (OS-level).
    pub id: u64,
    /// Thread name, if set.
    pub name: Option<String>,
    /// Whether this is an async task.
    pub is_async_task: bool,
}

impl ThreadInfo {
    /// Capture the current thread info.
    pub fn current() -> Self {
        let id = std::thread::current().id();
        Self {
            id: format!("{id:?}").parse().unwrap_or(0),
            name: std::thread::current().name().map(String::from),
            is_async_task: false,
        }
    }
}

/// Process-level information.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Process ID.
    pub pid: u32,
    /// Process uptime in seconds.
    pub uptime_secs: u64,
}

impl ProcessInfo {
    /// Capture the current process info.
    pub fn current() -> Self {
        Self {
            pid: std::process::id(),
            uptime_secs: 0,
        }
    }
}

/// Bounded causal chain (max depth = 16) to prevent allocation runaway.
#[derive(Debug, Clone)]
pub struct CauseChain {
    /// Ordered list of error summaries, newest first.
    entries: Vec<CauseEntry>,
    /// Maximum depth.
    max_depth: usize,
}

/// A single entry in the causal chain.
#[derive(Debug, Clone)]
pub struct CauseEntry {
    /// Human-readable error description.
    pub message: String,
    /// Error code, if known.
    pub code: Option<u32>,
    /// Category name.
    pub category: String,
    /// Timestamp of this cause.
    pub timestamp: DateTime<Utc>,
}

impl CauseChain {
    /// Create a new bounded cause chain.
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(4),
            max_depth: 16,
        }
    }

    /// Add a cause to the chain.
    pub fn push(&mut self, entry: CauseEntry) {
        if self.entries.len() < self.max_depth {
            self.entries.push(entry);
        }
    }

    /// Get all entries (newest first).
    pub fn entries(&self) -> &[CauseEntry] {
        &self.entries
    }

    /// Number of entries in the chain.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for CauseChain {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CauseChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, entry) in self.entries.iter().enumerate() {
            if i > 0 {
                write!(f, "\n  caused by: ")?;
            }
            write!(f, "{}", entry.message)?;
            if let Some(code) = entry.code {
                write!(f, " (code: {code})")?;
            }
        }
        Ok(())
    }
}

/// Full error context captured at the error creation site.
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Source code location.
    pub location: SourceLocation,
    /// Thread information.
    pub thread: ThreadInfo,
    /// When the error occurred.
    pub timestamp: SystemTime,
    /// Timing in ISO 8601 format.
    pub timestamp_iso: DateTime<Utc>,
    /// Process information.
    pub process: ProcessInfo,
    /// Subsystem name.
    pub subsystem: String,
    /// What the user was doing when the error occurred.
    pub user_action: Option<String>,
    /// Causal chain (newest first).
    pub cause_chain: CauseChain,
}

impl ErrorContext {
    /// Capture the current context at the error site.
    #[allow(unused_variables)]
    pub fn capture(location: SourceLocation, subsystem: &str) -> Self {
        Self {
            location,
            thread: ThreadInfo::current(),
            timestamp: SystemTime::now(),
            timestamp_iso: Utc::now(),
            process: ProcessInfo::current(),
            subsystem: subsystem.to_string(),
            user_action: None,
            cause_chain: CauseChain::new(),
        }
    }

    /// Attach user action context.
    pub fn with_user_action(mut self, action: impl Into<String>) -> Self {
        self.user_action = Some(action.into());
        self
    }

    /// Add a cause to the chain.
    pub fn with_cause(mut self, message: impl Into<String>) -> Self {
        self.cause_chain.push(CauseEntry {
            message: message.into(),
            code: None,
            category: String::new(),
            timestamp: Utc::now(),
        });
        self
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "at {} [{}] pid={} subsystem={}",
            self.location,
            self.timestamp_iso.format("%Y-%m-%dT%H:%M:%S.%3fZ"),
            self.process.pid,
            self.subsystem,
        )?;
        if let Some(ref action) = self.user_action {
            write!(f, " user_action=\"{action}\"")?;
        }
        if !self.cause_chain.is_empty() {
            write!(f, "\n{}", self.cause_chain)?;
        }
        Ok(())
    }
}
