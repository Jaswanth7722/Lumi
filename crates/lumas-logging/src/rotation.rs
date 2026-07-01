//! # Rotation Policy
//!
//! Defines when and how log files are rotated.

use std::time::Duration;

/// Policy determining when a log file should be rotated.
#[derive(Debug, Clone)]
pub enum RotationPolicy {
    /// Rotate when the current file reaches the specified size in bytes.
    BySize {
        /// Maximum file size in bytes.
        max_bytes: u64,
    },
    /// Rotate at wall-clock intervals.
    ByTime {
        /// Interval between rotations.
        interval: Duration,
    },
    /// Rotate when either condition triggers (whichever comes first).
    Combined {
        /// Maximum file size in bytes.
        max_bytes: u64,
        /// Interval between rotations.
        interval: Duration,
    },
}

impl RotationPolicy {
    /// Check if rotation should trigger based on file size.
    pub fn should_rotate_by_size(&self, current_size: u64) -> bool {
        match self {
            RotationPolicy::BySize { max_bytes } => current_size >= *max_bytes,
            RotationPolicy::Combined { max_bytes, .. } => current_size >= *max_bytes,
            RotationPolicy::ByTime { .. } => false,
        }
    }

    /// Check if rotation should trigger based on time elapsed.
    pub fn should_rotate_by_time(&self, elapsed: Duration) -> bool {
        match self {
            RotationPolicy::ByTime { interval } => elapsed >= *interval,
            RotationPolicy::Combined { interval, .. } => elapsed >= *interval,
            RotationPolicy::BySize { .. } => false,
        }
    }
}

impl Default for RotationPolicy {
    fn default() -> Self {
        RotationPolicy::BySize {
            max_bytes: 50 * 1024 * 1024, // 50 MB
        }
    }
}
