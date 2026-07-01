//! # Performance System Errors
//!
//! Uses lumi-error integration patterns for typed, structured error reporting.

use thiserror::Error;

/// Performance monitoring system errors.
#[derive(Debug, Error)]
pub enum PerformanceError {
    /// A collector timed out during collection.
    #[error("Collector '{collector_id}' timed out after {timeout:?}")]
    CollectorTimeout {
        collector_id: String,
        timeout: std::time::Duration,
    },

    /// Metric name does not match required schema.
    #[error("Invalid metric name '{name}': {reason}")]
    MetricNameInvalid { name: String, reason: &'static str },

    /// Attempted to record against an unregistered label.
    #[error(
        "Unregistered label '{label}' for metric '{}'. Label must be registered first.",
        metric
    )]
    UnregisteredLabel { metric: String, label: String },

    /// Metrics export failed.
    #[error("Export failed for '{exporter}': {reason}")]
    ExportFailed {
        exporter: &'static str,
        reason: String,
    },

    /// Histogram value out of configured range.
    #[error("Histogram value {value} exceeds configured max {max} for '{metric}'")]
    HistogramRangeMismatch {
        metric: String,
        value: u64,
        max: u64,
    },

    /// Threshold evaluation encountered an error.
    #[error("Threshold evaluation error for '{threshold_id}': {reason}")]
    ThresholdEvaluationError {
        threshold_id: String,
        reason: String,
    },

    /// System sampler is unavailable on this platform.
    #[error("System sampler unavailable on {platform}: {resource}")]
    SystemSamplerUnavailable {
        platform: &'static str,
        resource: &'static str,
    },

    /// Profiler is already running.
    #[error("Profiler is already running")]
    ProfilerAlreadyRunning,

    /// Shutdown timed out.
    #[error("Shutdown timed out after {elapsed:?}")]
    ShutdownTimeout { elapsed: std::time::Duration },

    /// Generic internal error.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl PerformanceError {
    /// Whether this error is recoverable (system can continue).
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            PerformanceError::MetricNameInvalid { .. }
                | PerformanceError::UnregisteredLabel { .. }
                | PerformanceError::HistogramRangeMismatch { .. }
                | PerformanceError::SystemSamplerUnavailable { .. }
                | PerformanceError::ProfilerAlreadyRunning
                | PerformanceError::ThresholdEvaluationError { .. }
        )
    }

    /// Human-readable suggested action.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            PerformanceError::CollectorTimeout { .. } => {
                "Increase the collector timeout or reduce the collection workload."
            }
            PerformanceError::MetricNameInvalid { .. } => {
                "Use the naming schema: lumi.{subsystem}.{operation}.{unit}"
            }
            PerformanceError::UnregisteredLabel { .. } => {
                "Register labels with MetricRegistry before recording."
            }
            PerformanceError::ExportFailed { .. } => {
                "Check the export destination and credentials."
            }
            PerformanceError::HistogramRangeMismatch { .. } => {
                "Increase the histogram max value or check the recorded value."
            }
            PerformanceError::ThresholdEvaluationError { .. } => {
                "Check the threshold configuration for invalid values."
            }
            PerformanceError::SystemSamplerUnavailable { .. } => {
                "This resource is not available on the current platform."
            }
            PerformanceError::ProfilerAlreadyRunning => {
                "Stop the current profiler session before starting a new one."
            }
            PerformanceError::ShutdownTimeout { .. } => {
                "Increase the shutdown timeout or investigate slow collectors."
            }
            PerformanceError::Internal(_) => "Contact support with the error details.",
        }
    }
}

pub type PerformanceResult<T> = Result<T, PerformanceError>;
