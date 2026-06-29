//! # LogManager — Public Entry Point
//!
//! Constructed once during bootstrap by calling `LogManager::install()`.
//! After installation, subsystems access logging exclusively through
//! the tracing macros (tracing::info!, etc.) — they do not hold a
//! reference to LogManager directly.

use crate::config::{ConsoleStream, LoggingConfig};
use crate::error::LogError;
use crate::event::LoggingInitialized;
use crate::filter::{Filter, FilterChain};
use crate::level::{ArcLogLevel, LogLevel};
use crate::metrics::LoggingMetrics;
use crate::pipeline::LogPipeline;
use crate::redaction::RedactionRule;
use crate::sink::SinkHandle;
use crate::sink::memory::MemorySink;
use arc_swap::ArcSwap;
use dashmap::DashMap;
use lumi_runtime::event::EventBus;
use parking_lot::RwLock;
use std::sync::Arc;

static INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The public entry point for the entire logging system.
#[derive(Clone)]
pub struct LogManager {
    inner: Arc<LogManagerInner>,
}

struct LogManagerInner {
    /// Internal async pipeline.
    pipeline: LogPipeline,
    /// Global filter chain.
    filter: Arc<RwLock<FilterChain>>,
    /// Redaction engine.
    redaction: Arc<RwLock<super::redaction::RedactionEngine>>,
    /// Registered sinks.
    sinks: Arc<DashMap<String, SinkHandle>>,
    /// Logging metrics.
    metrics: Arc<LoggingMetrics>,
    /// In-memory sink for diagnostics.
    memory_sink: Arc<MemorySink>,
    /// Current config.
    config: Arc<ArcSwap<LoggingConfig>>,
    /// Event bus reference.
    event_bus: Arc<EventBus>,
    /// Global log level.
    global_level: Arc<ArcLogLevel>,
}

impl LogManager {
    /// Install the global tracing subscriber and start the pipeline worker.
    ///
    /// Must be called exactly once during bootstrap, before any subsystem starts.
    pub async fn install(
        config: LoggingConfig,
        event_bus: Arc<EventBus>,
    ) -> Result<Self, LogError> {
        if INSTALLED.swap(true, std::sync::atomic::Ordering::Acquire) {
            return Err(LogError::AlreadyInstalled);
        }

        let metrics = Arc::new(LoggingMetrics::new());
        let global_level = Arc::new(ArcLogLevel::new(config.level));
        let memory_sink_capacity = config.memory_sink_capacity;

        // Create pipeline
        let (pipeline, worker) =
            LogPipeline::new(config.pipeline_channel_capacity, metrics.clone());
        let pipeline = Arc::new(pipeline);

        // Create default memory sink
        let memory_sink = Arc::new(MemorySink::new(memory_sink_capacity, global_level.clone()));

        // Create filter chain
        let mut filter_chain = FilterChain::new(global_level.clone());

        // Configure default sinks
        let mut sinks = DashMap::new();

        // Console sink
        let console_sink = Box::new(crate::sink::console::ConsoleSink::new(
            config.console_colors,
            config.console_stream,
            global_level.clone(),
        ));
        sinks.insert("console".into(), SinkHandle::new(console_sink));

        // File sink (if enabled)
        if config.file_enabled {
            if config
                .file_path
                .as_ref()
                .map(|p| p.parent().is_some())
                .unwrap_or(false)
            {
                match crate::sink::file::FileSink::new(
                    config
                        .file_path
                        .clone()
                        .unwrap_or_else(|| PathBuf::from("lumi.log")),
                    global_level.clone(),
                )
                .await
                {
                    Ok(sink) => {
                        sinks.insert("file".into(), SinkHandle::new(sink));
                    }
                    Err(e) => {
                        // Non-fatal: warn and continue without file sink
                        tracing::warn!("Failed to create file sink: {e}");
                    }
                }
            }
        }

        let manager = Self {
            inner: Arc::new(LogManagerInner {
                pipeline: pipeline.as_ref().clone(),
                filter: Arc::new(RwLock::new(filter_chain)),
                redaction: Arc::new(RwLock::new(super::redaction::RedactionEngine::new())),
                sinks: Arc::new(sinks),
                metrics: metrics.clone(),
                memory_sink,
                config: Arc::new(ArcSwap::new(Arc::new(config))),
                event_bus: event_bus.clone(),
                global_level,
            }),
        };

        // Collect the constructed sinks and pass them to the pipeline worker
        let sink_handles: Vec<SinkHandle> = sinks.iter().map(|e| e.value().clone()).collect();
        let worker = worker.with_sinks(sink_handles);

        // Start pipeline worker
        worker.run();

        // Emit LoggingInitialized event
        let sink_names: Vec<String> = sinks.iter().map(|e| e.key().clone()).collect();
        event_bus
            .publish(LoggingInitialized {
                sinks: sink_names,
                level: manager.inner.global_level.load(),
                initialized_at: chrono::Utc::now(),
            })
            .await;

        Ok(manager)
    }

    /// Register a new sink at runtime.
    pub async fn add_sink(&self, sink: Box<dyn crate::sink::Sink>) -> Result<(), LogError> {
        let name = sink.name().to_string();
        if self.inner.sinks.contains_key(&name) {
            return Err(LogError::SinkAlreadyRegistered { name });
        }
        self.inner.sinks.insert(name, SinkHandle::new(sink));
        Ok(())
    }

    /// Remove a registered sink by name.
    pub async fn remove_sink(&self, name: &str) -> Result<(), LogError> {
        self.inner
            .sinks
            .remove(name)
            .ok_or_else(|| LogError::SinkNotFound {
                name: name.to_string(),
            })?;
        Ok(())
    }

    /// Change the global minimum log level at runtime.
    pub fn set_level(&self, level: LogLevel) {
        let old = self.inner.global_level.load();
        self.inner.global_level.store(level);
        // Emit event
        let event = crate::event::LogLevelChanged {
            old_level: old,
            new_level: level,
            changed_at: chrono::Utc::now(),
        };
        let event_bus = self.inner.event_bus.clone();
        tokio::spawn(async move {
            event_bus.publish(event).await;
        });
    }

    /// Add a filter to the global filter chain.
    pub fn add_filter(&self, filter: Box<dyn Filter>) {
        self.inner.filter.write().add(filter);
    }

    /// Remove a filter by name.
    pub fn remove_filter(&self, name: &str) -> bool {
        self.inner.filter.write().remove(name)
    }

    /// Add a redaction rule.
    pub fn add_redaction_rule(&self, rule: Box<dyn RedactionRule>) -> Result<(), LogError> {
        self.inner.redaction.write().register(rule)
    }

    /// Flush all sinks.
    pub async fn flush(&self) -> Result<(), LogError> {
        self.inner.pipeline.flush().await
    }

    /// Graceful shutdown: flush + stop pipeline worker + close all sinks.
    pub async fn shutdown(self) -> Result<(), LogError> {
        self.inner.pipeline.shutdown().await?;
        Ok(())
    }

    /// Returns a reference to the in-memory sink for diagnostics and tests.
    pub fn memory_sink(&self) -> Arc<MemorySink> {
        self.inner.memory_sink.clone()
    }

    /// Returns current logging metrics snapshot.
    pub fn metrics(&self) -> crate::metrics::LoggingMetricsSnapshot {
        self.inner.metrics.snapshot()
    }

    /// Apply a new LoggingConfig (from hot reload).
    pub async fn apply_config(&self, config: LoggingConfig) -> Result<(), LogError> {
        self.inner.config.store(Arc::new(config.clone()));
        self.inner.global_level.store(config.level);
        Ok(())
    }
}

use std::path::PathBuf;
