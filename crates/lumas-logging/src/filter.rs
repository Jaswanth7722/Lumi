//! # Filter Chain
//!
//! Ordered chain of filters. A record must pass ALL filters (AND semantics).

use crate::level::{AtomicLogLevel, LogLevel};
use crate::record::LogRecord;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// A filter decides whether a log record should be processed.
pub trait Filter: Send + Sync {
    /// Unique name for this filter.
    fn name(&self) -> &'static str;
    /// Return true to keep the record, false to discard it.
    fn is_enabled(&self, record: &LogRecord) -> bool;
}

/// Ordered chain of filters. A record must pass ALL filters.
pub struct FilterChain {
    /// Registered filters.
    filters: Vec<Box<dyn Filter>>,
    /// Global minimum level (fast path check).
    global_level: Arc<AtomicLogLevel>,
}

impl FilterChain {
    /// Create a new filter chain with the given global level.
    pub fn new(global_level: Arc<AtomicLogLevel>) -> Self {
        Self {
            filters: Vec::new(),
            global_level,
        }
    }

    /// Fast path: check global level before evaluating any filter.
    pub fn is_enabled(&self, record: &LogRecord) -> bool {
        if !self.global_level.is_enabled(record.level) {
            return false;
        }
        self.filters.iter().all(|f| f.is_enabled(record))
    }

    /// Add a filter to the chain.
    pub fn add(&mut self, filter: Box<dyn Filter>) {
        self.filters.push(filter);
    }

    /// Remove a filter by name.
    pub fn remove(&mut self, name: &str) -> bool {
        let len_before = self.filters.len();
        self.filters.retain(|f| f.name() != name);
        self.filters.len() < len_before
    }

    /// Get a reference to the global level.
    pub fn global_level(&self) -> &Arc<AtomicLogLevel> {
        &self.global_level
    }
}

// ---------------------------------------------------------------------------
// Built-in Filters
// ---------------------------------------------------------------------------

/// Passes only records at or above the specified level.
pub struct LevelFilter {
    /// Minimum level to pass.
    pub min_level: LogLevel,
}

impl Filter for LevelFilter {
    fn name(&self) -> &'static str {
        "level_filter"
    }

    fn is_enabled(&self, record: &LogRecord) -> bool {
        record.level >= self.min_level
    }
}

/// Passes only records from specified subsystems.
pub struct SubsystemFilter {
    /// Set of allowed subsystems.
    pub allowed: Vec<String>,
}

impl Filter for SubsystemFilter {
    fn name(&self) -> &'static str {
        "subsystem_filter"
    }

    fn is_enabled(&self, record: &LogRecord) -> bool {
        match record.context.subsystem.as_ref() {
            Some(subsystem) => self.allowed.iter().any(|a| a == subsystem),
            None => true, // No subsystem set — allow through
        }
    }
}

/// Passes only records containing a specific correlation ID.
pub struct CorrelationFilter {
    /// Correlation ID to match.
    pub id: Uuid,
}

impl Filter for CorrelationFilter {
    fn name(&self) -> &'static str {
        "correlation_filter"
    }

    fn is_enabled(&self, record: &LogRecord) -> bool {
        record.context.correlation_id == Some(self.id)
    }
}

/// Passes records NOT matching a subsystem list (exclusion filter).
pub struct ExcludeSubsystemFilter {
    /// Subsystems to exclude.
    pub excluded: Vec<String>,
}

impl Filter for ExcludeSubsystemFilter {
    fn name(&self) -> &'static str {
        "exclude_subsystem_filter"
    }

    fn is_enabled(&self, record: &LogRecord) -> bool {
        match record.context.subsystem.as_ref() {
            Some(subsystem) => !self.excluded.iter().any(|e| e == subsystem),
            None => true,
        }
    }
}

/// Passes only records whose target matches a glob-like pattern.
pub struct TargetGlobFilter {
    /// Pattern to match against.
    pattern: String,
}

impl TargetGlobFilter {
    /// Create a new target glob filter.
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
        }
    }
}

impl Filter for TargetGlobFilter {
    fn name(&self) -> &'static str {
        "target_glob_filter"
    }

    fn is_enabled(&self, record: &LogRecord) -> bool {
        // Simple glob matching: * matches anything, otherwise exact or prefix match
        if self.pattern == "*" {
            return true;
        }
        if let Some(prefix) = self.pattern.strip_suffix('*') {
            record.target.starts_with(prefix)
        } else {
            record.target == self.pattern
        }
    }
}

/// Token bucket for rate limiting.
struct TokenBucket {
    /// Available tokens.
    tokens: f64,
    /// Maximum tokens (burst limit).
    max_tokens: f64,
    /// Refill rate (tokens per second).
    refill_rate: f64,
    /// Last refill timestamp.
    last_refill: Instant,
}

impl TokenBucket {
    fn new(max_per_second: u32) -> Self {
        Self {
            tokens: max_per_second as f64,
            max_tokens: max_per_second as f64,
            refill_rate: max_per_second as f64,
            last_refill: Instant::now(),
        }
    }

    /// Try to consume a token. Returns true if allowed.
    fn try_consume(&mut self) -> bool {
        // Refill based on elapsed time
        let now = Instant::now();
        let elapsed = now - self.last_refill;
        let refill = elapsed.as_secs_f64() * self.refill_rate;
        self.tokens = (self.tokens + refill).min(self.max_tokens);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Rate-limiting filter: passes at most `max_per_second` records per (level, target) pair.
pub struct RateLimitFilter {
    /// Maximum records per second per (level, target) pair.
    pub max_per_second: u32,
    /// Per-key token buckets.
    buckets: DashMap<(LogLevel, String), TokenBucket>,
}

impl RateLimitFilter {
    /// Create a new rate limit filter.
    pub fn new(max_per_second: u32) -> Self {
        Self {
            max_per_second,
            buckets: DashMap::new(),
        }
    }
}

impl Filter for RateLimitFilter {
    fn name(&self) -> &'static str {
        "rate_limit_filter"
    }

    fn is_enabled(&self, record: &LogRecord) -> bool {
        let key = (record.level, record.target.clone());
        self.buckets
            .entry(key)
            .or_insert_with(|| TokenBucket::new(self.max_per_second))
            .try_consume()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::LogContext;
    use crate::level::LogLevel;
    use crate::record::LogRecord;

    #[test]
    fn test_level_filter_passes_at_or_above() {
        let filter = LevelFilter {
            min_level: LogLevel::Warn,
        };
        let info = LogRecord::new(LogLevel::Info, "test".into(), "info".into());
        let warn = LogRecord::new(LogLevel::Warn, "test".into(), "warn".into());
        let error = LogRecord::new(LogLevel::Error, "test".into(), "error".into());

        assert!(!filter.is_enabled(&info));
        assert!(filter.is_enabled(&warn));
        assert!(filter.is_enabled(&error));
    }

    #[test]
    fn test_subsystem_filter() {
        let filter = SubsystemFilter {
            allowed: vec!["ai_core".into(), "voice".into()],
        };

        let mut voice = LogRecord::new(LogLevel::Info, "test".into(), "voice".into());
        voice.context.subsystem = Some("voice".into());
        assert!(filter.is_enabled(&voice));

        let mut render = LogRecord::new(LogLevel::Info, "test".into(), "render".into());
        render.context.subsystem = Some("rendering".into());
        assert!(!filter.is_enabled(&render));
    }

    #[test]
    fn test_target_glob_filter() {
        let filter = TargetGlobFilter::new("lumi_ai_core::*");
        let mut record = LogRecord::new(
            LogLevel::Info,
            "lumi_ai_core::inference".into(),
            "test".into(),
        );
        assert!(filter.is_enabled(&record));

        record.target = "lumas_render::pipeline".into();
        assert!(!filter.is_enabled(&record));
    }

    #[test]
    fn test_rate_limit_filter() {
        let filter = RateLimitFilter::new(10); // 10 per second
        let record = LogRecord::new(LogLevel::Info, "test".into(), "test".into());

        // Should allow up to 10
        for _ in 0..10 {
            assert!(filter.is_enabled(&record));
        }
        // May or may not allow more depending on timing (token bucket)
    }
}
