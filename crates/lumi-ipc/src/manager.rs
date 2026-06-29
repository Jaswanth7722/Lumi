//! # IPC Manager
//!
//! High-level application integration for the IPC framework.
//! Wraps `MessageBus` with lifecycle management, configuration loading,
//! and integration with `lumi-state` and `lumi-performance`.

use crate::bus::MessageBus;
use crate::config::IpcConfig;
use crate::error::{IpcError, IpcResult};
use crate::message::{LumiMessage, ProcessId};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;

/// High-level IPC manager for application integration.
pub struct IpcManager {
    /// The message bus
    bus: Arc<MessageBus>,
    /// Configuration
    config: IpcConfig,
    /// Running flag
    running: AtomicBool,
}

impl IpcManager {
    /// Create a new IPC manager.
    pub fn new(process_id: ProcessId) -> Self {
        let bus = Arc::new(MessageBus::new(process_id));
        Self {
            bus,
            config: IpcConfig::default(),
            running: AtomicBool::new(false),
        }
    }

    /// Create a new IPC manager with custom configuration.
    pub fn with_config(process_id: ProcessId, config: IpcConfig) -> Self {
        let bus = Arc::new(MessageBus::new(process_id));
        Self {
            bus,
            config,
            running: AtomicBool::new(false),
        }
    }

    /// Start the IPC manager.
    pub async fn start(&mut self) -> IpcResult<()> {
        info!("Starting IPC manager for {}", self.bus.process_id());
        self.running.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Get a reference to the message bus.
    pub fn bus(&self) -> &Arc<MessageBus> {
        &self.bus
    }

    /// Send a message on the bus.
    pub async fn send(&self, msg: LumiMessage) -> IpcResult<()> {
        self.bus.send(msg).await
    }

    /// Send a request and wait for a response.
    pub async fn request(&self, msg: LumiMessage) -> IpcResult<LumiMessage> {
        self.bus.request(msg).await
    }

    /// Check if the manager is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Shut down the IPC manager.
    pub async fn shutdown(&self) -> IpcResult<()> {
        info!("Shutting down IPC manager for {}", self.bus.process_id());
        self.running.store(false, Ordering::Relaxed);
        self.bus.shutdown().await
    }
}

impl std::fmt::Debug for IpcManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpcManager")
            .field("bus", &self.bus)
            .field("running", &self.running.load(Ordering::Relaxed))
            .finish()
    }
}
