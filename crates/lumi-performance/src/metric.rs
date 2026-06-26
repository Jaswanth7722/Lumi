//! # Metric Type System
//!
//! Core trait hierarchy for all metric types. Enforces naming schema, registration,
//! and provides a snapshot mechanism for lock-free point-in-time reads.
//!
//! # Thread Safety
//! All metric types implement `Send + Sync + 'static`.

use serde::Serialize;
use std::sync::Arc;

/// Unique identifier for a metric — an interned, validated string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct MetricName(Arc<str>);

impl MetricName {
    /// Create a new metric name, validating the naming schema.
    ///
    /// Schema: `lumi.{subsystem}.{operation}.{unit}`
    ///
    /// # Errors
    /// Returns `MetricNameInvalid` if the name doesn't match the schema.
    pub fn new(name: &'static str) -> Result<Self, crate::PerformanceError> {
        if name.starts_with("lumi.") && name.chars().filter(|&c| c == '.').count() >= 2 {
            Ok(Self(Arc::from(name)))
        } else {
            Err(crate::PerformanceError::MetricNameInvalid {
                name: name.to_string(),
                reason: "must follow lumi.{subsystem}.{operation}.{unit} schema",
            })
        }
    }

    /// Create a metric name without validation (for internal use).
    pub(crate) fn from_str(name: &str) -> Self {
        Self(Arc::from(name.to_owned()))
    }

    /// Get the name as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the subsystem portion of the name.
    pub fn subsystem(&self) -> &str {
        self.0
            .strip_prefix("lumi.")
            .and_then(|s| s.split('.').next())
            .unwrap_or("unknown")
    }
}

impl std::fmt::Display for MetricName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for MetricName {
    fn from(s: &str) -> Self {
        Self::from_str(s)
    }
}

/// The kind of metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MetricKind {
    /// Monotonically increasing counter.
    Counter,
    /// Up/down gauge.
    Gauge,
    /// Value distribution histogram.
    Histogram,
    /// Duration timer.
    Timer,
    /// Rate meter (events/second).
    RateMeter,
}

/// Unit of measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MetricUnit {
    /// Count (no unit).
    Count,
    /// Microseconds.
    Microseconds,
    /// Milliseconds.
    Milliseconds,
    /// Bytes.
    Bytes,
    /// Percentage (0.0–100.0).
    Percent,
    /// Frames per second.
    Fps,
    /// Degrees Celsius.
    Celsius,
    /// Custom unit string.
    Custom(&'static str),
}

impl MetricUnit {
    /// Get the unit as a string for metric naming.
    pub fn as_str(&self) -> &'static str {
        match self {
            MetricUnit::Count => "count",
            MetricUnit::Microseconds => "microseconds",
            MetricUnit::Milliseconds => "milliseconds",
            MetricUnit::Bytes => "bytes",
            MetricUnit::Percent => "percent",
            MetricUnit::Fps => "fps",
            MetricUnit::Celsius => "celsius",
            MetricUnit::Custom(s) => s,
        }
    }
}

impl std::fmt::Display for MetricUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A tag (key-value pair) attached to a metric.
#[derive(Debug, Clone, Serialize)]
pub struct Tag {
    /// Tag key.
    pub key: &'static str,
    /// Tag value.
    pub value: String,
}

impl Tag {
    /// Create a new tag.
    pub fn new(key: &'static str, value: impl Into<String>) -> Self {
        Self {
            key,
            value: value.into(),
        }
    }
}

/// A snapshot of a metric's value at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct MetricSnapshot {
    /// Metric name.
    pub name: String,
    /// Metric kind.
    pub kind: MetricKind,
    /// Unit.
    pub unit: MetricUnit,
    /// Current value (for counters/gauges).
    pub value: f64,
    /// Histogram-specific snapshot.
    pub histogram: Option<HistogramBucketSnapshot>,
    /// Tags.
    pub tags: Vec<Tag>,
}

/// Snapshot of histogram buckets at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct HistogramBucketSnapshot {
    /// Number of recorded values.
    pub count: u64,
    /// Minimum value.
    pub min: u64,
    /// Maximum value.
    pub max: u64,
    /// Mean value.
    pub mean: f64,
    /// Standard deviation.
    pub stddev: f64,
    /// P50 (median).
    pub p50: u64,
    /// P90.
    pub p90: u64,
    /// P95.
    pub p95: u64,
    /// P99.
    pub p99: u64,
    /// P99.9.
    pub p999: u64,
}

impl Default for HistogramBucketSnapshot {
    fn default() -> Self {
        Self {
            count: 0,
            min: 0,
            max: 0,
            mean: 0.0,
            stddev: 0.0,
            p50: 0,
            p90: 0,
            p95: 0,
            p99: 0,
            p999: 0,
        }
    }
}

/// Implemented by all metric types.
pub trait Metric: Send + Sync + 'static {
    /// The metric name.
    fn name(&self) -> &MetricName;
    /// The metric kind.
    fn kind(&self) -> MetricKind;
    /// Human-readable description.
    fn description(&self) -> &'static str;
    /// The unit of measurement.
    fn unit(&self) -> MetricUnit;
    /// Tags attached to this metric.
    fn tags(&self) -> &[Tag];
    /// Snapshot of the current value.
    fn snapshot(&self) -> MetricSnapshot;
    /// Reset the metric to its initial state.
    fn reset(&self);
}

/// A label for categorizing metric values (e.g., plugin ID, provider name).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct MetricLabel(Arc<str>);

impl MetricLabel {
    /// Create a new label. Returns error if label is empty.
    pub fn new(label: &str) -> Result<Self, crate::PerformanceError> {
        if label.is_empty() {
            Err(crate::PerformanceError::MetricNameInvalid {
                name: label.to_string(),
                reason: "label must not be empty",
            })
        } else {
            Ok(Self(Arc::from(label.to_owned())))
        }
    }

    /// Get the label as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MetricLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
