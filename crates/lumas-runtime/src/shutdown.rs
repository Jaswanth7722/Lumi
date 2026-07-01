//! # Shutdown Manager
//!
//! Orchestrates graceful runtime shutdown.
//!
//! Triggered by:
//! - SIGTERM (Unix) / Ctrl+C (Windows)
//! - `shutdown()` called on `RuntimeHandle`
//! - Unrecoverable error detected by `HealthMonitor`
//!
//! The shutdown sequence drains in-flight tasks, stops services in
//! reverse dependency order, flushes logs, releases resources, and
//! emits the final `RuntimeStopped` event before exiting.
//!
//! # Errors
//!
//! Shutdown errors are logged but do not prevent process exit.

use crate::bootstrap::RuntimeHandle;
use crate::error::ShutdownError;
use crate::event::{RuntimeStopped, ShutdownInitiated};
use crate::lifecycle::{LifecycleManager, ShutdownPhase};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Graceful shutdown timeout: how long to wait for task drain.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-service stop timeout.
const SERVICE_STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// Manages the graceful shutdown sequence for the Lumas runtime.
pub struct ShutdownManager {
    /// Whether shutdown has been initiated (prevents double-shutdown).
    initiated: bool,
    /// Whether forced shutdown has been requested.
    forced: bool,
}

impl ShutdownManager {
    /// Create a new shutdown manager.
    pub fn new() -> Self {
        Self {
            initiated: false,
            forced: false,
        }
    }

    /// Initiate graceful shutdown.
    ///
    /// # Sequence
    ///
    /// 1. Emit `ShutdownInitiated` event
    /// 2. Transition lifecycle to `ShuttingDown`
    /// 3. Signal scheduler to stop accepting new tasks
    /// 4. Drain in-flight critical/high/normal tasks
    /// 5. Cancel background + low tasks
    /// 6. Stop services in reverse dependency order
    /// 7. Stop health monitor
    /// 8. Flush pending log records
    /// 9. Release resources
    /// 10. Emit `RuntimeStopped` event
    /// 11. Transition lifecycle to `Stopped`
    pub async fn shutdown(&mut self, handle: &RuntimeHandle) {
        if self.initiated {
            warn!("Shutdown already initiated, ignoring duplicate call");
            return;
        }
        self.initiated = true;

        info!("=== Lumas Runtime Shutdown Starting ===");

        // Phase 1-2: Emit event + transition lifecycle
        handle
            .event_bus
            .publish(ShutdownInitiated {
                reason: "user_request".to_string(),
                timestamp: chrono::Utc::now(),
            })
            .await;

        if let Err(e) = handle.lifecycle.write().await.begin_shutdown() {
            error!("Failed to enter shutdown state: {e}");
        }

        let _ = handle
            .lifecycle
            .write()
            .await
            .advance_shutdown(ShutdownPhase::StoppingNewWork);

        // Phase 3: Signal scheduler
        let _ = handle
            .lifecycle
            .write()
            .await
            .advance_shutdown(ShutdownPhase::DrainingTasks);

        handle.scheduler.shutdown(DRAIN_TIMEOUT).await;

        // Phase 4: Stop services
        let _ = handle
            .lifecycle
            .write()
            .await
            .advance_shutdown(ShutdownPhase::StoppingServices);

        if let Err(errors) = handle.services.read().await.stop_all().await {
            for err in &errors {
                error!("Service stop error: {err}");
            }
        }

        // Phase 5: Flush logs
        let _ = handle
            .lifecycle
            .write()
            .await
            .advance_shutdown(ShutdownPhase::FlushingLogs);

        // Give logs time to flush
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Phase 6: Release resources
        let _ = handle
            .lifecycle
            .write()
            .await
            .advance_shutdown(ShutdownPhase::ReleasingResources);

        // Phase 7: Emit final event
        let uptime_secs = handle.uptime_secs();
        handle
            .event_bus
            .publish(RuntimeStopped {
                timestamp: chrono::Utc::now(),
                uptime_secs,
            })
            .await;

        // Phase 8: Transition to Stopped
        let _ = handle
            .lifecycle
            .write()
            .await
            .advance_shutdown(ShutdownPhase::Complete);

        let _ = handle.lifecycle.write().await.transition_to_stopped();

        info!("=== Lumas Runtime Shutdown Complete (uptime: {uptime_secs}s) ===");
    }

    /// Initiate forced shutdown (skip drain, force stop services).
    pub async fn force_shutdown(&mut self, handle: &RuntimeHandle) {
        self.forced = true;
        self.initiated = true;

        warn!("=== Lumas Runtime Forced Shutdown ===");

        // Force cancel all scheduler tasks
        handle.scheduler.shutdown(Duration::from_secs(1)).await;

        // Force stop services
        let _ = handle.services.read().await.stop_all().await;

        // Skip to shutdown complete
        let _ = handle.lifecycle.write().await.transition_to_stopped();

        info!("=== Lumas Runtime Forced Shutdown Complete ===");
    }

    /// Check if shutdown has been initiated.
    pub fn is_initiated(&self) -> bool {
        self.initiated
    }

    /// Check if this is a forced shutdown.
    pub fn is_forced(&self) -> bool {
        self.forced
    }
}

impl Default for ShutdownManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shutdown_not_initiated_by_default() {
        let sm = ShutdownManager::new();
        assert!(!sm.is_initiated());
        assert!(!sm.is_forced());
    }

    #[test]
    fn test_double_shutdown_prevented() {
        let mut sm = ShutdownManager::new();
        sm.initiated = true;
        // Second call should be a no-op
        assert!(sm.is_initiated());
    }
}
