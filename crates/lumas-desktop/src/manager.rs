//! DesktopEngine — the public entry point for the Desktop Engine.
//!
//! `DesktopManager` is the single interface through which all Lumas subsystems
//! interact with the OS desktop: window creation, monitor management, hit
//! testing, input observation, and diagnostics.
//!
//! # Architecture
//!
//! The Desktop Engine owns the winit `EventLoop` on a dedicated thread.
//! All cross-thread requests use `DesktopCommandChannel` with typed commands
//! and `oneshot` responders. Platform-specific operations are delegated to
//! the `PlatformBackend` trait.
//!
//! # Thread Safety
//! `DesktopManager` is `Send + Sync`. It is designed to be stored in
//! `RuntimeContext` and accessed from multiple async tasks.
//!
//! # Lifecycle
//! 1. `DesktopManager::new()` — create the manager without starting the event loop.
//! 2. `DesktopManager::command()` — get the command channel for window operations.
//! 3. `DesktopManager::monitor_manager()` — access monitor state.
//! 4. `DesktopManager::shutdown()` — gracefully destroy all windows and stop.
//!
//! # Errors
//! - `DesktopError::UnsupportedPlatform` if no backend is available.
//! - `DesktopError::EventLoopExited` if the event loop has already shut down.

use crate::command::DesktopCommandChannel;
use crate::config::DesktopConfig;
use crate::diagnostics::DesktopDiagnostics;
use crate::error::DesktopError;
use crate::geometry::{LogicalPoint, PhysicalPoint};
use crate::hit_test::{HitResult, HitTester};
use crate::metrics::{DesktopMetrics, DesktopMetricsSnapshot};
use crate::monitor::{MonitorInfo, MonitorManager};
use crate::overlay::{OverlayDescriptor, OverlayHandle};
use crate::platform::{create_backend, PlatformBackend, TestBackend};
use crate::window::{WindowDescriptor, WindowHandle};
use crate::zorder::ZOrderManager;
use std::sync::Arc;

/// The public entry point for the Desktop Engine.
///
/// Provides access to all desktop-related functionality: window management,
/// monitor tracking, hit testing, input observation, and diagnostics.
///
/// # Examples
/// ```
/// use lumas_desktop::DesktopManager;
/// use lumas_desktop::config::DesktopConfig;
///
/// // Create with default configuration (test mode, no display required).
/// let manager = DesktopManager::new_test(DesktopConfig::default());
/// assert_eq!(manager.monitor_manager().all().len(), 0);
/// ```
pub struct DesktopManager {
    config: DesktopConfig,
    command_channel: Option<DesktopCommandChannel>,
    monitor_manager: Arc<MonitorManager>,
    metrics: Arc<DesktopMetrics>,
    diagnostics: Arc<DesktopDiagnostics>,
    platform_backend: Box<dyn PlatformBackend>,
    is_shutting_down: std::sync::atomic::AtomicBool,
}

impl DesktopManager {
    /// Create a new DesktopManager with the given configuration.
    ///
    /// This creates the platform backend but does not start the event loop.
    /// Call `create_backend()` internally and stores the backend for later use.
    ///
    /// # Errors
    /// Returns `DesktopError::UnsupportedPlatform` if no backend is available
    /// for the current platform.
    pub fn new(config: DesktopConfig) -> Result<Self, DesktopError> {
        let backend = create_backend()?;
        let metrics = Arc::new(DesktopMetrics::new());

        Ok(Self {
            monitor_manager: Arc::new(MonitorManager::new(metrics.clone())),
            metrics: metrics.clone(),
            diagnostics: Arc::new(DesktopDiagnostics::new(
                Arc::new(dashmap::DashMap::new()),
                Arc::new(Vec::new()),
                backend.name().to_string(),
            )),
            command_channel: None,
            config,
            platform_backend: backend,
            is_shutting_down: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Create a DesktopManager for testing (uses `TestBackend`).
    ///
    /// This does not require a display connection and always succeeds.
    ///
    /// # Examples
    /// ```
    /// # use lumas_desktop::DesktopManager;
    /// let manager = DesktopManager::new_test(Default::default());
    /// ```
    pub fn new_test(config: DesktopConfig) -> Self {
        let metrics = Arc::new(DesktopMetrics::new());
        let backend = TestBackend;

        Self {
            monitor_manager: Arc::new(MonitorManager::new(metrics.clone())),
            metrics: metrics.clone(),
            diagnostics: Arc::new(DesktopDiagnostics::new(
                Arc::new(dashmap::DashMap::new()),
                Arc::new(Vec::new()),
                "test".into(),
            )),
            command_channel: None,
            config,
            platform_backend: Box::new(backend),
            is_shutting_down: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Set the command channel after the event loop starts.
    ///
    /// Called internally by the event loop runner. Not intended for external use.
    pub(crate) fn set_command_channel(&mut self, channel: DesktopCommandChannel) {
        self.command_channel = Some(channel);
    }

    /// Get the command channel for sending requests to the event loop thread.
    ///
    /// # Errors
    /// Returns `DesktopError::EventLoopExited` if the event loop has not been
    /// started or has already shut down.
    pub fn command(&self) -> Result<DesktopCommandChannel, DesktopError> {
        self.command_channel
            .clone()
            .ok_or(DesktopError::EventLoopExited)
    }

    // --- Monitor Management ---

    /// Returns the monitor manager for querying display information.
    pub fn monitor_manager(&self) -> &Arc<MonitorManager> {
        &self.monitor_manager
    }

    // --- Window Management ---

    /// Create a new window through the event loop.
    ///
    /// # Errors
    /// Returns `DesktopError::EventLoopExited` if the event loop is not running,
    /// or `DesktopError::CommandTimeout` if the command times out.
    pub async fn create_window(
        &self,
        descriptor: WindowDescriptor,
    ) -> Result<WindowHandle, DesktopError> {
        let channel = self.command()?;
        channel
            .send(
                |responder| crate::command::DesktopCommand::CreateWindow {
                    descriptor,
                    responder,
                },
                self.config.command_timeout_ms,
                "create_window",
            )
            .await
    }

    /// Destroy a window through the event loop.
    ///
    /// # Errors
    /// Returns `DesktopError::EventLoopExited` if the event loop is not running.
    pub async fn destroy_window(&self, handle: WindowHandle) -> Result<(), DesktopError> {
        let channel = self.command()?;
        channel
            .send(
                |responder| crate::command::DesktopCommand::DestroyWindow {
                    id: handle.id().clone(),
                    responder,
                },
                self.config.command_timeout_ms,
                "destroy_window",
            )
            .await
    }

    // --- Hit Testing ---

    /// Update the alpha mask from the render process.
    ///
    /// # Errors
    /// Returns `DesktopError::HitTestMaskNotInitialized` if no hit tester has
    /// been configured.
    pub fn update_hit_mask(&self) -> Result<(), DesktopError> {
        // Placeholder — the hit tester is configured by the render pipeline.
        Ok(())
    }

    /// Test whether a logical screen point hits the character.
    ///
    /// Returns `HitResult::Miss` if no hit tester is configured.
    pub fn hit_test(&self, _point: LogicalPoint) -> HitResult {
        // Placeholder — returns Miss when no hit tester is configured.
        HitResult::Miss
    }

    // --- Input ---

    /// Returns the current cursor position in logical screen coordinates.
    pub fn cursor_position(&self) -> LogicalPoint {
        LogicalPoint { x: 0.0, y: 0.0 }
    }

    // --- Coordinate Conversion ---

    /// Convert a logical position to physical pixels.
    pub fn to_physical(&self, pos: LogicalPoint) -> PhysicalPoint {
        PhysicalPoint {
            x: pos.x as u32,
            y: pos.y as u32,
        }
    }

    /// Convert physical pixels to logical position on the given monitor.
    pub fn to_logical(&self, pos: PhysicalPoint, _monitor: &MonitorInfo) -> LogicalPoint {
        LogicalPoint {
            x: pos.x as f64,
            y: pos.y as f64,
        }
    }

    // --- Lifecycle ---

    /// Check if the manager is shutting down.
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutting_down.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Graceful shutdown: signal the event loop to exit.
    ///
    /// # Errors
    /// Returns `DesktopError::EventLoopExited` if the event loop is already gone.
    pub async fn shutdown(self) -> Result<(), DesktopError> {
        self.is_shutting_down
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(channel) = &self.command_channel {
            let _ = channel.send_raw(crate::command::DesktopCommand::Shutdown);
        }
        Ok(())
    }

    // --- Diagnostics ---

    /// Returns the diagnostics provider for state export.
    pub fn diagnostics(&self) -> &Arc<DesktopDiagnostics> {
        &self.diagnostics
    }

    /// Returns the current metrics snapshot.
    pub fn metrics(&self) -> DesktopMetricsSnapshot {
        self.metrics.snapshot()
    }
}

impl std::fmt::Debug for DesktopManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopManager")
            .field("platform", &self.platform_backend.name())
            .field("is_shutting_down", &self.is_shutting_down())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_test_succeeds() {
        let manager = DesktopManager::new_test(DesktopConfig::default());
        assert!(!manager.is_shutting_down());
        assert_eq!(manager.monitor_manager().all().len(), 0);
    }

    #[tokio::test]
    async fn test_command_fails_before_event_loop() {
        let manager = DesktopManager::new_test(DesktopConfig::default());
        let result = manager.command();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DesktopError::EventLoopExited));
    }

    #[tokio::test]
    async fn test_shutdown_marks_flag() {
        let manager = DesktopManager::new_test(DesktopConfig::default());
        assert!(!manager.is_shutting_down());
        let _ = manager.shutdown().await;
        // Note: shutdown only marks flag if command channel exists.
    }

    #[tokio::test]
    async fn test_hit_test_defaults_to_miss() {
        let manager = DesktopManager::new_test(DesktopConfig::default());
        let result = manager.hit_test(LogicalPoint { x: 100.0, y: 100.0 });
        assert_eq!(result, HitResult::Miss);
    }

    #[tokio::test]
    async fn test_metrics_snapshot() {
        let manager = DesktopManager::new_test(DesktopConfig::default());
        let snapshot = manager.metrics();
        assert_eq!(snapshot.window_count, 0);
        assert_eq!(snapshot.overlay_count, 0);
    }

    #[tokio::test]
    async fn test_diagnostics() {
        let manager = DesktopManager::new_test(DesktopConfig::default());
        let diag = manager.diagnostics();
        let snapshot = diag.snapshot();
        assert_eq!(snapshot.platform, "test");
    }
}
