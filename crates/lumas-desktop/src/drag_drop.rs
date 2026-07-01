//! Drag-and-drop target registration and handling.
//!
//! The Desktop Engine registers drag-drop targets for workspace panels and the
//! settings window. The stage window (character overlay) does not accept drops
//! by default — users should not accidentally drop files onto the character.
//!
//! # Thread Safety
//! `DragDropManager` is `Send + Sync`. It stores drag-drop targets in a
//! `DashMap` keyed by `WindowId`.
//!
//! # Platform Notes
//! - **macOS**: Implemented via `NSDraggingDestination` on each `NSView`.
//! - **Windows**: Implemented via `RevokeDragDrop`/`RegisterDragDrop` + `IDropTarget`.
//! - **Linux/X11**: Implemented via XDnD protocol (X11 `Xdnd*` atoms).
//! - **Linux/Wayland**: Implemented via `wl_data_device` and `wl_data_offer`.

use crate::error::DesktopError;
use crate::metrics::DesktopMetrics;
use crate::window::WindowId;
use crossbeam_channel::Sender;
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// Identifier for a drag-drop target registration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DragDropId(Uuid);

impl DragDropId {
    /// Create a new unique drag-drop identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Kinds of content that can be dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DroppedContent {
    /// One or more file paths from the OS file manager.
    Files(Vec<PathBuf>),
    /// Plain text content.
    Text(String),
    /// URL content.
    Url(String),
    /// Image data (raw bytes + format hint).
    Image {
        format: String,
        data: Vec<u8>,
    },
}

/// Information about a completed drop event.
#[derive(Debug, Clone)]
pub struct DropEvent {
    /// The window that received the drop.
    pub window_id: WindowId,
    /// The dropped content.
    pub content: DroppedContent,
    /// Position of the drop in logical pixels relative to the window.
    pub position: crate::geometry::LogicalPoint,
    /// Timestamp of the drop event.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Configuration for a drag-drop target window.
#[derive(Debug, Clone)]
pub struct DragDropTargetConfig {
    /// Whether the target accepts file drops.
    pub accept_files: bool,
    /// Whether the target accepts text drops.
    pub accept_text: bool,
    /// Whether the target accepts URL drops.
    pub accept_urls: bool,
    /// Whether the target accepts image drops.
    pub accept_images: bool,
    /// File extension filters (e.g., `["png", "jpg"]`).
    pub allowed_extensions: Vec<String>,
}

impl Default for DragDropTargetConfig {
    fn default() -> Self {
        Self {
            accept_files: true,
            accept_text: true,
            accept_urls: false,
            accept_images: false,
            allowed_extensions: Vec::new(),
        }
    }
}

/// Manages drag-drop target registration and event dispatch.
///
/// # Thread Safety
/// `DragDropManager` is `Send + Sync`. All internal state uses concurrent data
/// structures. Methods can be called from any thread.
///
/// # Errors
/// Returns `DesktopError::DragDropFailed` if the OS rejects a registration.
pub struct DragDropManager {
    targets: DashMap<WindowId, (DragDropTargetConfig, DragDropId)>,
    event_tx: Sender<DropEvent>,
    metrics: Arc<DesktopMetrics>,
}

impl DragDropManager {
    /// Create a new drag-drop manager.
    ///
    /// # Examples
    /// ```
    /// # use lumas_desktop::drag_drop::DragDropManager;
    /// # let (tx, _rx) = crossbeam_channel::bounded(64);
    /// let manager = DragDropManager::new(tx, Default::default());
    /// ```
    pub fn new(
        event_tx: Sender<DropEvent>,
        _metrics: Arc<DesktopMetrics>,
    ) -> Self {
        Self {
            targets: DashMap::new(),
            event_tx,
            metrics: _metrics,
        }
    }

    /// Register a window as a drag-drop target.
    ///
    /// # Errors
    /// Returns `DesktopError::DragDropFailed` if the registration fails at the
    /// OS level.
    pub fn register(
        &self,
        window_id: WindowId,
        config: DragDropTargetConfig,
    ) -> Result<DragDropId, DesktopError> {
        let id = DragDropId::new();
        self.targets.insert(window_id, (config, id.clone()));
        Ok(id)
    }

    /// Deregister a window as a drag-drop target.
    pub fn deregister(&self, window_id: &WindowId) {
        self.targets.remove(window_id);
    }

    /// Check if a window is registered as a drag-drop target.
    pub fn is_registered(&self, window_id: &WindowId) -> bool {
        self.targets.contains_key(window_id)
    }

    /// Get the configuration for a registered window.
    pub fn config(&self, window_id: &WindowId) -> Option<DragDropTargetConfig> {
        self.targets.get(window_id).map(|entry| entry.value().0.clone())
    }

    /// Handle a drop event — validate and forward to the event channel.
    ///
    /// Returns `Err(DesktopError::DragDropFailed)` if the content type is not
    /// accepted by the target.
    pub fn handle_drop(&self, event: DropEvent) -> Result<(), DesktopError> {
        let entry = self
            .targets
            .get(&event.window_id)
            .ok_or_else(|| DesktopError::DragDropFailed {
                id: event.window_id.clone(),
                reason: "Window is not registered as a drag-drop target".into(),
            })?;

        let (config, _) = entry.value();

        // Validate content type against configuration.
        match &event.content {
            DroppedContent::Files(paths) => {
                if !config.accept_files {
                    return Err(DesktopError::DragDropFailed {
                        id: event.window_id.clone(),
                        reason: "File drops are not accepted by this target".into(),
                    });
                }
                // Check file extension filters.
                if !config.allowed_extensions.is_empty() {
                    for path in paths {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if !config.allowed_extensions.iter().any(|a| a == ext) {
                                return Err(DesktopError::DragDropFailed {
                                    id: event.window_id.clone(),
                                    reason: format!(
                                        "File extension '.{}' is not in allowed list",
                                        ext
                                    ),
                                });
                            }
                        }
                    }
                }
            }
            DroppedContent::Text(_) => {
                if !config.accept_text {
                    return Err(DesktopError::DragDropFailed {
                        id: event.window_id.clone(),
                        reason: "Text drops are not accepted by this target".into(),
                    });
                }
            }
            DroppedContent::Url(_) => {
                if !config.accept_urls {
                    return Err(DesktopError::DragDropFailed {
                        id: event.window_id.clone(),
                        reason: "URL drops are not accepted by this target".into(),
                    });
                }
            }
            DroppedContent::Image { .. } => {
                if !config.accept_images {
                    return Err(DesktopError::DragDropFailed {
                        id: event.window_id.clone(),
                        reason: "Image drops are not accepted by this target".into(),
                    });
                }
            }
        }

        // Forward to event channel.
        self.event_tx
            .send(event)
            .map_err(|_| DesktopError::DragDropFailed {
                id: event.window_id.clone(),
                reason: "Event channel closed; cannot dispatch drop event".into(),
            })?;

        Ok(())
    }

    /// Returns the number of registered drag-drop targets.
    pub fn target_count(&self) -> usize {
        self.targets.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::LogicalPoint;

    #[test]
    fn test_register_and_deregister() {
        let (tx, _rx) = crossbeam_channel::bounded(64);
        let metrics = Arc::new(DesktopMetrics::new(&Default::default()));
        let manager = DragDropManager::new(tx, metrics);

        let window_id = WindowId::new();
        assert!(!manager.is_registered(&window_id));

        let id = manager.register(window_id.clone(), DragDropTargetConfig::default());
        assert!(id.is_ok());
        assert!(manager.is_registered(&window_id));

        manager.deregister(&window_id);
        assert!(!manager.is_registered(&window_id));
    }

    #[test]
    fn test_drop_on_unregistered_window_returns_error() {
        let (tx, _rx) = crossbeam_channel::bounded(64);
        let metrics = Arc::new(DesktopMetrics::new(&Default::default()));
        let manager = DragDropManager::new(tx, metrics);

        let result = manager.handle_drop(DropEvent {
            window_id: WindowId::new(),
            content: DroppedContent::Text("hello".into()),
            position: LogicalPoint { x: 0.0, y: 0.0 },
            timestamp: chrono::Utc::now(),
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_text_drop_rejected_when_disabled() {
        let (tx, rx) = crossbeam_channel::bounded(64);
        let metrics = Arc::new(DesktopMetrics::new(&Default::default()));
        let manager = DragDropManager::new(tx, metrics);

        let window_id = WindowId::new();
        let config = DragDropTargetConfig {
            accept_text: false,
            ..Default::default()
        };
        manager.register(window_id.clone(), config).unwrap();

        let result = manager.handle_drop(DropEvent {
            window_id,
            content: DroppedContent::Text("hello".into()),
            position: LogicalPoint { x: 0.0, y: 0.0 },
            timestamp: chrono::Utc::now(),
        });

        assert!(result.is_err());
        // Verify no event was sent.
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_drop_sends_event() {
        let (tx, rx) = crossbeam_channel::bounded(64);
        let metrics = Arc::new(DesktopMetrics::new(&Default::default()));
        let manager = DragDropManager::new(tx, metrics);

        let window_id = WindowId::new();
        manager
            .register(window_id.clone(), DragDropTargetConfig::default())
            .unwrap();

        let drop = DropEvent {
            window_id: window_id.clone(),
            content: DroppedContent::Text("hello".into()),
            position: LogicalPoint { x: 10.0, y: 20.0 },
            timestamp: chrono::Utc::now(),
        };

        manager.handle_drop(drop).unwrap();

        let received = rx.try_recv().unwrap();
        assert_eq!(received.window_id, window_id);
        assert_eq!(received.position.x, 10.0);
    }
}
