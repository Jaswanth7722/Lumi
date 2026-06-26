//! # Subsystem Collector Interface
//!
//! Each subsystem registers a `Collector` that reads and records its own metrics
//! on a configurable schedule.
//!
//! # Thread Safety
//! `Collector` implementations must be `Send + Sync + 'static`.

use crate::error::{PerformanceError, PerformanceResult};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

/// Subsystem identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubsystemId(pub String);

impl SubsystemId {
    /// Create a new subsystem ID.
    pub fn new(id: &str) -> Self {
        Self(id.to_string())
    }

    /// Get the ID as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SubsystemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Subsystem collector trait.
#[async_trait]
pub trait Collector: Send + Sync + 'static {
    /// Unique subsystem ID.
    fn id(&self) -> SubsystemId;
    /// Collection interval.
    fn collection_interval(&self) -> Duration;
    /// Called by the PerformanceManager on the collector's interval.
    async fn collect(
        &self,
        handle: &crate::manager::SubsystemMetricsHandle,
    ) -> PerformanceResult<()>;

    /// Called on graceful shutdown to flush pending measurements.
    async fn flush(&self) -> PerformanceResult<()>;
}

/// Render process collector — collects per-frame metrics.
pub struct RenderCollector;

#[async_trait]
impl Collector for RenderCollector {
    fn id(&self) -> SubsystemId {
        SubsystemId::new("render")
    }

    fn collection_interval(&self) -> Duration {
        Duration::from_millis(16) // ~60 FPS
    }

    async fn collect(
        &self,
        _handle: &crate::manager::SubsystemMetricsHandle,
    ) -> PerformanceResult<()> {
        Ok(())
    }

    async fn flush(&self) -> PerformanceResult<()> {
        Ok(())
    }
}

/// AI core collector — collects per-request inference metrics.
pub struct AiCollector;

#[async_trait]
impl Collector for AiCollector {
    fn id(&self) -> SubsystemId {
        SubsystemId::new("ai")
    }

    fn collection_interval(&self) -> Duration {
        Duration::from_millis(100)
    }

    async fn collect(
        &self,
        _handle: &crate::manager::SubsystemMetricsHandle,
    ) -> PerformanceResult<()> {
        Ok(())
    }

    async fn flush(&self) -> PerformanceResult<()> {
        Ok(())
    }
}

/// IPC collector — collects message throughput and latency metrics.
pub struct IpcCollector;

#[async_trait]
impl Collector for IpcCollector {
    fn id(&self) -> SubsystemId {
        SubsystemId::new("ipc")
    }

    fn collection_interval(&self) -> Duration {
        Duration::from_millis(100)
    }

    async fn collect(
        &self,
        _handle: &crate::manager::SubsystemMetricsHandle,
    ) -> PerformanceResult<()> {
        Ok(())
    }

    async fn flush(&self) -> PerformanceResult<()> {
        Ok(())
    }
}

/// Memory store collector — collects retrieval/write latency and cache metrics.
pub struct MemoryStoreCollector;

#[async_trait]
impl Collector for MemoryStoreCollector {
    fn id(&self) -> SubsystemId {
        SubsystemId::new("memory")
    }

    fn collection_interval(&self) -> Duration {
        Duration::from_secs(1)
    }

    async fn collect(&self) -> PerformanceResult<()> {
        Ok(())
    }

    async fn flush(&self) -> PerformanceResult<()> {
        Ok(())
    }
}

/// Voice collector — collects STT/TTS latency metrics.
pub struct VoiceCollector;

#[async_trait]
impl Collector for VoiceCollector {
    fn id(&self) -> SubsystemId {
        SubsystemId::new("voice")
    }

    fn collection_interval(&self) -> Duration {
        Duration::from_millis(100)
    }

    async fn collect(
        &self,
        _handle: &crate::manager::SubsystemMetricsHandle,
    ) -> PerformanceResult<()> {
        Ok(())
    }

    async fn flush(&self) -> PerformanceResult<()> {
        Ok(())
    }
}

/// Storage collector — collects read/write/query latency metrics.
pub struct StorageCollector;

#[async_trait]
impl Collector for StorageCollector {
    fn id(&self) -> SubsystemId {
        SubsystemId::new("storage")
    }

    fn collection_interval(&self) -> Duration {
        Duration::from_secs(1)
    }

    async fn collect(
        &self,
        _handle: &crate::manager::SubsystemMetricsHandle,
    ) -> PerformanceResult<()> {
        Ok(())
    }

    async fn flush(&self) -> PerformanceResult<()> {
        Ok(())
    }
}

/// Plugin collector — collects per-invocation tool execution metrics.
pub struct PluginCollector;

#[async_trait]
impl Collector for PluginCollector {
    fn id(&self) -> SubsystemId {
        SubsystemId::new("plugin")
    }

    fn collection_interval(&self) -> Duration {
        Duration::from_millis(100)
    }

    async fn collect(
        &self,
        _handle: &crate::manager::SubsystemMetricsHandle,
    ) -> PerformanceResult<()> {
        Ok(())
    }

    async fn flush(&self) -> PerformanceResult<()> {
        Ok(())
    }
}

/// System collector — collects CPU, memory, GPU at 1s intervals.
pub struct SystemCollector;

#[async_trait]
impl Collector for SystemCollector {
    fn id(&self) -> SubsystemId {
        SubsystemId::new("system")
    }

    fn collection_interval(&self) -> Duration {
        Duration::from_secs(1)
    }

    async fn collect(&self) -> PerformanceResult<()> {
        Ok(())
    }

    async fn flush(&self) -> PerformanceResult<()> {
        Ok(())
    }
}
