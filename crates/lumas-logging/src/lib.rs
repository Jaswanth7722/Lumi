//! # Lumas Logging System
//!
//! The single, authoritative sink for all diagnostic output across every subsystem.
//!
//! # Architecture
//!
//! ```text
//! Subsystems (tracing::info! / tracing::error! / etc.)
//!     │
//!     ▼
//! LumiTracingLayer (tracing_subscriber::Layer)
//!     │  converts events → LogRecord, applies LogContext
//!     ▼
//! LogPipeline (crossbeam bounded channel)
//!     │  non-blocking try_send, drops on full
//!     ▼
//! PipelineWorker (dedicated thread)
//!     │  Filter → Redact → Format → Dispatch
//!     ▼
//! Sink[0]  Sink[1]  ...  Sink[N]
//! ```
//!
//! # Thread Safety
//!
//! All public types are `Send + Sync`. The hot path (record submission) is
//! allocation-minimal and never blocks the caller.
//!
//! # WORKSPACE AUDIT
//!
//! Existing logging-related infrastructure found:
//! - lumas-runtime/src/event.rs: Event trait (`pub trait Event: Send + Sync + Clone + fmt::Debug + 'static`)
//!   with existing ConfigLoaded, ConfigReloaded, LifecycleTransitioned event types
//! - lumas-runtime/src/metrics.rs: MetricsRegistry with Counter, Gauge, Histogram, Timer
//! - lumas-config/src/secret.rs: Secret<T> newtype with redacted Debug/Display/Serialize
//! - lumas-config/src/schema/logging.rs: LoggingConfig struct (with level, format, file_logging, etc.)
//! - lumi-common/src/logging.rs: Basic LogEntry/LogLevel types (will be superseded)
//! - lumi-core/src/logging.rs: Basic LogManager (will be superseded)
//!
//! Design decisions:
//! - Uses `tracing` as the instrumentation layer; subsystems use tracing macros
//! - Single global subscriber installed once by LogManager::install()
//! - Bounded crossbeam channel for non-blocking hot path
//! - AtomicLogLevel for lock-free runtime level changes
//! - Tokio task-local LogContext for automatic propagation

pub mod config;
pub mod context;
pub mod diagnostics;
pub mod error;
pub mod event;
pub mod filter;
pub mod formatter;
pub mod level;
pub mod manager;
pub mod metrics;
pub mod pipeline;
pub mod record;
pub mod redaction;
pub mod rotation;
pub mod sink;

// Convenience re-exports
pub use config::{ConsoleStream, LoggingConfig};
pub use context::LogContext;
pub use diagnostics::{CrashReport, DiagnosticsQuery, LogDiagnostics};
pub use error::LogError;
pub use filter::{Filter, FilterChain};
pub use formatter::{Formatter, json::JsonFormatter, pretty::PrettyFormatter};
pub use level::{AtomicLogLevel, LogLevel};
pub use manager::LogManager;
pub use metrics::LoggingMetrics;
pub use record::{FieldValue, LogRecord};
pub use redaction::RedactionEngine;
pub use rotation::RotationPolicy;
pub use sink::Sink;
pub use sink::SinkHandle;
