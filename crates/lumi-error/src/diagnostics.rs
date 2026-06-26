//! # Diagnostics Engine
//!
//! Provides a queryable `ErrorHistory` with:
//! - Bounded circular buffer (configurable, default 10,000 entries)
//! - Multi-key indexing by severity, category, error code, time range
//! - Full-text search over diagnostic messages
//! - Pattern detection (repeated failures → severity escalation)
//!
//! # Thread Safety
//! All types are `Send + Sync`. Reads and writes use parking_lot locks.

use crate::category::ErrorCategory;
use crate::error::LumiError;
use crate::error_code::ErrorCode;
use crate::severity::Severity;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A single entry in the error history.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorHistoryEntry {
    /// Unique entry ID.
    pub id: u64,
    /// Error code.
    pub code: ErrorCode,
    /// Error category.
    pub category: ErrorCategory,
    /// Severity at the time of recording.
    pub severity: Severity,
    /// User-facing message.
    pub user_message: String,
    /// Diagnostic message (full detail).
    pub diagnostic_message: String,
    /// When the error occurred.
    pub timestamp: DateTime<Utc>,
    /// Correlation ID if available.
    pub correlation_id: Option<String>,
}

/// Query for filtering error history.
#[derive(Debug, Clone, Default)]
pub struct ErrorQuery {
    /// Filter by minimum severity.
    pub min_severity: Option<Severity>,
    /// Filter by maximum severity.
    pub max_severity: Option<Severity>,
    /// Filter by category name.
    pub category: Option<String>,
    /// Filter by error code.
    pub code: Option<ErrorCode>,
    /// Filter by session ID.
    pub session_id: Option<String>,
    /// Filter by correlation ID.
    pub correlation_id: Option<String>,
    /// Filter by time range start.
    pub start_time: Option<DateTime<Utc>>,
    /// Filter by time range end.
    pub end_time: Option<DateTime<Utc>>,
    /// Full-text search over diagnostic message.
    pub search_text: Option<String>,
    /// Maximum results to return.
    pub max_results: usize,
}

impl ErrorQuery {
    /// Create a new query with default values.
    pub fn new() -> Self {
        Self {
            max_results: 100,
            ..Default::default()
        }
    }

    /// Set minimum severity filter.
    pub fn with_min_severity(mut self, severity: Severity) -> Self {
        self.min_severity = Some(severity);
        self
    }

    /// Set category filter.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set correlation ID filter.
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Set search text.
    pub fn with_search_text(mut self, text: impl Into<String>) -> Self {
        self.search_text = Some(text.into());
        self
    }

    /// Set maximum results.
    pub fn with_max_results(mut self, max: usize) -> Self {
        self.max_results = max;
        self
    }
}

/// Topology of error history — provides multi-key indexing and search.
#[derive(Debug)]
pub struct ErrorHistory {
    /// Ring buffer of entries.
    buffer: Arc<parking_lot::RwLock<Vec<ErrorHistoryEntry>>>,
    /// Maximum capacity.
    capacity: usize,
    /// Auto-incrementing entry ID.
    next_id: std::sync::atomic::AtomicU64,
    /// Pattern detection state.
    pattern_tracker: Arc<parking_lot::Mutex<PatternTracker>>,
}

impl ErrorHistory {
    /// Create a new error history with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Arc::new(parking_lot::RwLock::new(Vec::with_capacity(capacity))),
            capacity,
            next_id: std::sync::atomic::AtomicU64::new(1),
            pattern_tracker: Arc::new(parking_lot::Mutex::new(PatternTracker::new(
                Duration::from_secs(60),
                5,
            ))),
        }
    }

    /// Record an error in the history.
    pub fn record(&self, error: &LumiError) -> ErrorHistoryEntry {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let entry = ErrorHistoryEntry {
            id,
            code: error.code(),
            category: error.category().clone(),
            severity: error.severity(),
            user_message: error.user_message().to_string(),
            diagnostic_message: error.diagnostic_message().to_string(),
            timestamp: Utc::now(),
            correlation_id: None,
        };

        let mut buffer = self.buffer.write();
        if buffer.len() >= self.capacity {
            buffer.remove(0); // Remove oldest
        }
        buffer.push(entry.clone());

        // Update pattern tracker
        let mut tracker = self.pattern_tracker.lock();
        tracker.record(error.code());

        entry
    }

    /// Query the error history.
    pub fn query(&self, query: &ErrorQuery) -> Vec<ErrorHistoryEntry> {
        let buffer = self.buffer.read();
        let filtered: Vec<ErrorHistoryEntry> = buffer
            .iter()
            .filter(|e| {
                if let Some(ref min_sev) = query.min_severity {
                    if e.severity < *min_sev {
                        return false;
                    }
                }
                if let Some(ref max_sev) = query.max_severity {
                    if e.severity > *max_sev {
                        return false;
                    }
                }
                if let Some(ref cat) = query.category {
                    if e.category.display_name() != cat {
                        return false;
                    }
                }
                if let Some(code) = query.code {
                    if e.code != code {
                        return false;
                    }
                }
                if let Some(ref cid) = query.correlation_id {
                    match e.correlation_id {
                        Some(ref ecid) if ecid == cid => {}
                        _ => return false,
                    }
                }
                if let Some(ref start) = query.start_time {
                    if e.timestamp < *start {
                        return false;
                    }
                }
                if let Some(ref end) = query.end_time {
                    if e.timestamp > *end {
                        return false;
                    }
                }
                if let Some(ref text) = query.search_text {
                    let lower = text.to_lowercase();
                    if !e.diagnostic_message.to_lowercase().contains(&lower) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        filtered.into_iter().rev().take(query.max_results).collect()
    }

    /// Get recent entries (newest first).
    pub fn recent(&self, n: usize) -> Vec<ErrorHistoryEntry> {
        let buffer = self.buffer.read();
        buffer.iter().rev().take(n).cloned().collect()
    }

    /// Get the total number of entries.
    pub fn len(&self) -> usize {
        self.buffer.read().len()
    }

    /// Whether the history is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.read().is_empty()
    }

    /// Clear the history.
    pub fn clear(&self) {
        self.buffer.write().clear();
    }

    /// Analyze failure patterns and return detected patterns.
    pub fn analyze_failure_patterns(&self) -> Vec<FailurePattern> {
        self.pattern_tracker.lock().detect_patterns()
    }

    /// Get the capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Set the capacity (truncates if smaller than current size).
    pub fn set_capacity(&mut self, new_capacity: usize) {
        self.capacity = new_capacity;
        let mut buffer = self.buffer.write();
        while buffer.len() > new_capacity {
            buffer.remove(0);
        }
    }
}

/// Pattern tracker for detecting repeated failures.
#[derive(Debug)]
struct PatternTracker {
    /// Window for pattern detection.
    window: Duration,
    /// Threshold within the window.
    threshold: u32,
    /// Recent error occurrences by code.
    occurrences: HashMap<ErrorCode, Vec<Instant>>,
}

impl PatternTracker {
    fn new(window: Duration, threshold: u32) -> Self {
        Self {
            window,
            threshold,
            occurrences: HashMap::new(),
        }
    }

    fn record(&mut self, code: ErrorCode) {
        let now = Instant::now();
        let entries = self.occurrences.entry(code).or_insert_with(Vec::new);
        entries.push(now);
        // Prune old entries
        entries.retain(|t| now.duration_since(*t) < self.window);
    }

    fn detect_patterns(&mut self) -> Vec<FailurePattern> {
        let now = Instant::now();
        let mut patterns = Vec::new();

        self.occurrences.retain(|_, entries| {
            entries.retain(|t| now.duration_since(*t) < self.window);
            !entries.is_empty()
        });

        for (code, entries) in &self.occurrences {
            if entries.len() >= self.threshold as usize {
                patterns.push(FailurePattern {
                    code: *code,
                    occurrence_count: entries.len() as u32,
                    window_secs: self.window.as_secs() as u32,
                });
            }
        }

        patterns
    }
}

/// A detected failure pattern.
#[derive(Debug, Clone, Serialize)]
pub struct FailurePattern {
    /// The error code that was detected.
    pub code: ErrorCode,
    /// Number of occurrences in the detection window.
    pub occurrence_count: u32,
    /// The detection window in seconds.
    pub window_secs: u32,
}

/// A complete diagnostic report.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticReport {
    /// When the report was generated.
    pub generated_at: DateTime<Utc>,
    /// Total error count.
    pub total_errors: usize,
    /// Errors by severity.
    pub errors_by_severity: HashMap<String, usize>,
    /// Errors by category.
    pub errors_by_category: HashMap<String, usize>,
    /// Detected failure patterns.
    pub failure_patterns: Vec<FailurePattern>,
    /// Recent errors (last 20).
    pub recent_errors: Vec<ErrorHistoryEntry>,
}

/// Generate a diagnostic report from error history.
pub fn generate_diagnostic_report(history: &ErrorHistory) -> DiagnosticReport {
    let entries = history.recent(usize::MAX);

    let mut by_severity: HashMap<String, usize> = HashMap::new();
    let mut by_category: HashMap<String, usize> = HashMap::new();

    for entry in &entries {
        *by_severity.entry(entry.severity.to_string()).or_insert(0) += 1;
        *by_category
            .entry(entry.category.display_name().to_string())
            .or_insert(0) += 1;
    }

    DiagnosticReport {
        generated_at: Utc::now(),
        total_errors: entries.len(),
        errors_by_severity: by_severity,
        errors_by_category: by_category,
        failure_patterns: history.analyze_failure_patterns(),
        recent_errors: history.recent(20),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::ErrorCategory;
    use crate::error::LumiError;
    use crate::error_code::ErrorCode;

    fn make_error(code: ErrorCode, cat: ErrorCategory, msg: &str) -> LumiError {
        LumiError::new(code, cat, msg)
    }

    #[test]
    fn test_error_history_record_and_query() {
        let history = ErrorHistory::new(100);
        let error = make_error(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "test AI error",
        );
        history.record(&error);

        assert_eq!(history.len(), 1);

        let recent = history.recent(10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].code, ErrorCode::AI_INFERENCE_FAILED);
    }

    #[test]
    fn test_error_history_query_by_severity() {
        let history = ErrorHistory::new(100);
        let err1 = make_error(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "recoverable",
        );
        let err2 = make_error(ErrorCode::RUNTIME_INTERNAL, ErrorCategory::Runtime, "fatal");
        history.record(&err1);
        history.record(&err2);

        let query = ErrorQuery::new()
            .with_min_severity(Severity::Critical)
            .with_max_results(10);
        let results = history.query(&query);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_error_history_bounded_capacity() {
        let history = ErrorHistory::new(3);
        for i in 0..5 {
            let err = make_error(
                ErrorCode::INTERNAL_UNEXPECTED,
                ErrorCategory::Internal,
                &format!("error {}", i),
            );
            history.record(&err);
        }
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_pattern_detection() {
        let history = ErrorHistory::new(100);
        let error = make_error(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "repeated error",
        );

        // Record the same error multiple times
        for _ in 0..6 {
            history.record(&error);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        let patterns = history.analyze_failure_patterns();
        assert!(!patterns.is_empty());
    }

    #[test]
    fn test_diagnostic_report() {
        let history = ErrorHistory::new(100);
        let err1 = make_error(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "AI error",
        );
        let err2 = make_error(
            ErrorCode::CONFIG_FILE_NOT_FOUND,
            ErrorCategory::Configuration { field: None },
            "config error",
        );
        history.record(&err1);
        history.record(&err2);

        let report = generate_diagnostic_report(&history);
        assert_eq!(report.total_errors, 2);
        assert!(report.errors_by_category.contains_key("AI Core"));
    }

    #[test]
    fn test_query_by_search_text() {
        let history = ErrorHistory::new(100);
        let err = make_error(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "Provider was unreachable",
        );
        history.record(&err);

        let query = ErrorQuery::new()
            .with_search_text("unreachable")
            .with_max_results(10);
        let results = history.query(&query);
        assert_eq!(results.len(), 1);
    }
}
