//! # lumi-performance — Centralized Performance Observability for Lumi
//!
//! Every subsystem reports its performance characteristics through this framework.
//! It is the single source of truth for all runtime performance data, threshold
//! alerting, profiling, and performance diagnostics across the platform.
//!
//! ## Observer Effect Guarantee
//!
//! The monitoring system must be self-effacing: it measures everything while
//! consuming negligible resources itself. Quantified overhead budgets:
//!
//! | Operation | Max Overhead | Measurement |
//! |-----------|--------------|-------------|
//! | `counter.increment()` | < 3 ns | criterion benchmark |
//! | `histogram.record(value)` | < 10 ns | criterion benchmark |
//! | `timer.start()` / `timer.stop()` | < 20 ns total | criterion benchmark |
//! | Background metric aggregation | < 0.1% CPU | Sampled resource test |
//! | Full metrics snapshot export | < 5 ms | criterion benchmark |
//!
//! ## Rt-Safe vs. Async Metrics
//!
//! The type system enforces separation between real-time-safe metrics (render/audio paths)
//! and async metrics (background subsystems):
//!
//! - `RtSafeCounter`, `RtSafeGauge`, `RtSafeHistogram` — no alloc, no lock, no I/O
//! - `AsyncCounter`, `AsyncGauge`, `Histogram` — full features, async-capable
//!
//! ## Quick Start
//!
//! ```ignore
//! use lumi_performance::prelude::*;
//!
//! let config = PerformanceConfig::default();
//! let manager = PerformanceManager::start(config).await.unwrap();
//!
//! let render_h = manager.subsystem_handle(SubsystemId::Render);
//! let frame_timer = render_h.timer("frame_time", MetricUnit::Microseconds);
//!
//! let _g = frame_timer.start();
//! // ... render frame ...
//! // guard drops here, recording elapsed time
//! ```

pub mod alert;
pub mod collector;
pub mod config;
pub mod counter;
pub mod dashboard;
pub mod diagnostics;
pub mod error;
pub mod event;
pub mod export;
pub mod gauge;
pub mod histogram;
pub mod integration;
pub mod manager;
pub mod metric;
#[cfg(feature = "profiler")]
pub mod profiler;
pub mod reporter;
pub mod resource;
pub mod sampler;
pub mod system;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod threshold;
pub mod timer;

// Core types
pub use alert::*;
pub use config::*;
pub use counter::*;
pub use error::*;
pub use gauge::*;
pub use histogram::*;
pub use manager::*;
pub use metric::*;
pub use sampler::*;
pub use threshold::*;
pub use timer::*;

/// Prelude module — import this to get the most commonly used types.
pub mod prelude {
    pub use crate::alert::{Alert, AlertEvent, AlertSeverity};
    pub use crate::config::PerformanceConfig;
    pub use crate::counter::{AsyncCounter, RtSafeCounter};
    pub use crate::error::PerformanceError;
    pub use crate::gauge::{AsyncGauge, RtSafeGauge};
    pub use crate::histogram::{HdrHistogram, RtSafeHistogram};
    pub use crate::manager::{PerformanceManager, PerformanceSnapshot, SubsystemId};
    pub use crate::metric::{Metric, MetricKind, MetricName, MetricSnapshot, MetricUnit};
    pub use crate::threshold::{Threshold, ThresholdCondition, ThresholdId};
    pub use crate::timer::Timer;
}
