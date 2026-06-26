//! # Runtime Profiler
//!
//! Feature-gated (`#[cfg(feature = "profiler")]`) CPU profiler for
//! development/debugging builds. Does not affect binary size or runtime
//! behavior when the feature is disabled.
//!
//! # Thread Safety
//! `Profiler` uses internal synchronization.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Profiler modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfilerMode {
    /// Statistical stack sampling.
    CpuSampling,
    /// Explicit span recording (always available via tracing).
    Instrumentation,
    /// Heap allocation tracking.
    AllocationTracking,
}

/// Profile output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileFormat {
    /// SVG flamegraph via inferno.
    Flamegraph,
    /// Google pprof format.
    Pprof,
    /// SpeedScope JSON format.
    SpeedScope,
    /// Custom JSON for Lumi's diagnostics dashboard.
    Lumi,
}

/// Runtime profiler for CPU sampling and instrumentation.
///
/// Only available when the `profiler` feature is enabled.
pub struct Profiler {
    /// Profiler mode.
    mode: ProfilerMode,
    /// Output format.
    output_format: ProfileFormat,
    /// Output path.
    output_path: PathBuf,
    /// Sampling rate in Hz.
    sampling_rate_hz: u32,
    /// Whether the profiler is currently running.
    running: AtomicBool,
}

impl Profiler {
    /// Create a new profiler.
    pub fn new(
        mode: ProfilerMode,
        output_format: ProfileFormat,
        output_path: PathBuf,
        sampling_rate_hz: u32,
    ) -> Self {
        Self {
            mode,
            output_format,
            output_path,
            sampling_rate_hz,
            running: AtomicBool::new(false),
        }
    }

    /// Start the profiler.
    ///
    /// # Errors
    /// Returns `ProfilerAlreadyRunning` if the profiler is already running.
    pub fn start(&self) -> Result<ProfilerSession, crate::PerformanceError> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(crate::PerformanceError::ProfilerAlreadyRunning);
        }

        Ok(ProfilerSession { profiler: self })
    }

    /// Get the profiler mode.
    pub fn mode(&self) -> ProfilerMode {
        self.mode
    }

    /// Get the output format.
    pub fn output_format(&self) -> ProfileFormat {
        self.output_format
    }

    /// Get whether the profiler is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Get the sampling rate.
    pub fn sampling_rate_hz(&self) -> u32 {
        self.sampling_rate_hz
    }

    fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

impl std::fmt::Debug for Profiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profiler")
            .field("mode", &self.mode)
            .field("output_format", &self.output_format)
            .field("running", &self.running)
            .finish()
    }
}

/// A profiler session that stops profiling on drop.
pub struct ProfilerSession<'a> {
    profiler: &'a Profiler,
}

impl<'a> Drop for ProfilerSession<'a> {
    fn drop(&mut self) {
        self.profiler.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiler_start_stop() {
        let profiler = Profiler::new(
            ProfilerMode::Instrumentation,
            ProfileFormat::Lumi,
            PathBuf::from("/tmp/profile"),
            99,
        );
        assert!(!profiler.is_running());

        {
            let _session = profiler.start().unwrap();
            assert!(profiler.is_running());
        }
        assert!(!profiler.is_running());
    }

    #[test]
    fn test_profiler_already_running() {
        let profiler = Profiler::new(
            ProfilerMode::Instrumentation,
            ProfileFormat::Lumi,
            PathBuf::from("/tmp/profile"),
            99,
        );
        let _session = profiler.start().unwrap();
        assert!(profiler.start().is_err());
    }
}
