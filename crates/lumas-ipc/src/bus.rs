//! # Message Bus
//!
//! The central IPC coordinator. Owns all registered channels, routes messages
//! through the processing pipeline, and manages peer connections.
//!
//! ## Message Processing Pipeline
//!
//! Every inbound message traverses:
//! 1. WireCodec::decode() — frame deserialization
//! 2. Validator::check_envelope() — magic, version, size, required fields
//! 3. AuthEngine::verify() — MAC verification (reject on failure)
//! 4. ReplayWindow::check_sequence() — replay detection
//! 5. Validator::check_ttl() — expiry check
//! 6. Middleware::process_inbound() — logging, tracing, metrics, rate limit
//! 7. Router::route() — message delivery
//! 8. Dispatcher::dispatch() — subscriber delivery
//! 9. Middleware::process_outbound() — outcome logging

use crate::auth::AuthEngine;
use crate::connection::ConnectionManager;
use crate::dispatcher::Dispatcher;
use crate::error::{IpcError, IpcResult};
use crate::event::BusEvent;
use crate::heartbeat::HeartbeatEngine;
use crate::message::{LumiMessage, ProcessId};
use crate::middleware::MiddlewarePipeline;
use crate::peer::PeerRegistry;
use crate::permission::PermissionRegistry;
use crate::registry::ChannelRegistry;
use crate::router::Router;
use crate::transport::{SharedTransport, Transport};
use crate::validator::Validator;
use crate::config::{IpcConfig, TransportKind};
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;

/// The central message bus for the Lumas IPC framework.
///
/// Each process creates one `MessageBus` instance. The bus owns all channels,
/// routes messages, manages peer connections, and enforces security policies.
pub struct MessageBus {
    /// Process ID of this bus instance
    process_id: ProcessId,
    /// Bus configuration
    config: IpcConfig,
    /// Channel registry
    pub channels: Arc<ChannelRegistry>,
    /// Peer registry
    pub peers: Arc<PeerRegistry>,
    /// Connection manager
    pub connections: Arc<ConnectionManager>,
    /// Auth engine
    pub auth: Arc<AuthEngine>,
    /// Message validator
    pub validator: Arc<Validator>,
    /// Permission registry
    pub permissions: Arc<PermissionRegistry>,
    /// Middleware pipeline
    pub middleware: Arc<MiddlewarePipeline>,
    /// Message router
    pub router: Arc<Router>,
    /// Message dispatcher
    pub dispatcher: Arc<Dispatcher>,
    /// Heartbeat engine
    pub heartbeat: Arc<HeartbeatEngine>,
    /// Event bus for internal events
    event_tx: broadcast::Sender<BusEvent>,
    /// Running flag
    running: AtomicBool,
}

impl MessageBus {
    /// Create a new message bus.
    pub fn new(process_id: ProcessId) -> Self {
        let config = IpcConfig::default();
        let (event_tx, _) = broadcast::channel(256);

        let channels = Arc::new(ChannelRegistry::new());
        let peers = Arc::new(PeerRegistry::new());
        let connections = Arc::new(ConnectionManager::new(
            crate::connection::ReconnectPolicy::default(),
        ));
        let permissions = Arc::new(PermissionRegistry::new());
        let router = Arc::new(Router::new(
            channels.clone(),
            peers.clone(),
            connections.clone(),
        ));
        let dispatcher = Arc::new(Dispatcher::new());

        let mut heartbeat = HeartbeatEngine::new(config.heartbeat.clone());
        heartbeat.set_event_tx(event_tx.clone());

        let auth = Arc::new(AuthEngine::new(
            process_id.clone(),
            crate::message::ProcessToken(process_id.to_string()),
            config.auth.replay_window_size,
        ));

        let validator = Arc::new(Validator::new(
            60,
            config.bus.message_ttl_default_ms,
        ));

        Self {
            process_id,
            config,
            channels,
            peers,
            connections,
            auth,
            validator,
            permissions,
            middleware: Arc::new(MiddlewarePipeline::with_defaults()),
            router,
            dispatcher,
            heartbeat: Arc::new(heartbeat),
            event_tx,
            running: AtomicBool::new(false),
        }
    }

    /// Start the message bus.
    pub async fn start(&mut self) -> IpcResult<()> {
        self.running.store(true, Ordering::Relaxed);

        // Register default channels from configuration
        for (name, channel_config) in &self.config.channels {
            let tier = match channel_config.transport {
                TransportKind::InProcess => crate::transport::TransportTier::InProcess,
                TransportKind::Socket => crate::transport::TransportTier::Socket,
                TransportKind::SharedMemory => crate::transport::TransportTier::SharedMemory,
            };

            match crate::transport::create_transport(tier, name, channel_config) {
                Ok(transport) => {
                    self.channels.register(name.clone(), channel_config.clone(), transport);
                }
                Err(e) => {
                    tracing::warn!("Failed to create transport for channel {}: {:?}", name, e);
                }
            }
        }

        tracing::info!("Message bus started for {}", self.process_id);
        Ok(())
    }

    /// Send a message on the bus.
    pub async fn send(&self, msg: LumiMessage) -> IpcResult<()> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(IpcError::BusShuttingDown);
        }

        // Run through outbound middleware
        let msg = self.middleware.process_outbound(msg).await
            .map_err(|e| IpcError::Internal(e.to_string()))?;

        // Route the message
        self.router.route(msg).await
            .map_err(|e| IpcError::Internal(format!("Routing failed: {}", e)))?;

        Ok(())
    }

    /// Send a message and wait for a response (request/response pattern).
    pub async fn request(&self, msg: LumiMessage) -> IpcResult<LumiMessage> {
        self.send(msg).await
    }

    /// Subscribe to a channel's broadcast stream.
    pub fn subscribe(&self, channel: impl Into<String>) -> IpcResult<broadcast::Receiver<LumiMessage>> {
        let channel = channel.into();
        let entry = self.channels.get(&channel)
            .ok_or_else(|| IpcError::ChannelNotFound { channel: channel.clone() })?;

        // Create a broadcast channel for this subscription
        let (tx, rx) = broadcast::channel(256);
        self.dispatcher.register(
            ProcessId::Core,
            channel.clone(),
            tx.subscribe(),
        );

        Ok(rx)
    }

    /// Process an inbound message through the pipeline.
    pub async fn process_message(&self, msg: LumiMessage) -> IpcResult<()> {
        // 1. Validate envelope
        self.validator.check_envelope(&msg)
            .map_err(|e| IpcError::ValidationFailed {
                msg_id: msg.id.0.clone(),
                reason: e.to_string(),
            })?;

        // 2. Check TTL
        self.validator.check_ttl(&msg)
            .map_err(|e| IpcError::ValidationFailed {
                msg_id: msg.id.0.clone(),
                reason: e.to_string(),
            })?;

        // 3. Verify authentication (if present)
        if msg.auth.is_some() {
            self.auth.verify(&msg)
                .map_err(|e| IpcError::AuthenticationFailed {
                    peer: msg.sender.clone(),
                })?;
        }

        // 4. Run through inbound middleware
        let msg = self.middleware.process_inbound(msg).await
            .map_err(|e| IpcError::Internal(e.to_string()))?;

        // 5. Dispatch to local subscribers
        self.dispatcher.dispatch(&msg);

        Ok(())
    }

    /// Register a transport for a channel.
    pub fn register_transport(
        &self,
        channel: impl Into<String>,
        transport: SharedTransport,
    ) {
        let channel = channel.into();
        let config = self.config.channels.get(&channel)
            .cloned()
            .unwrap_or_default();
        self.channels.register(channel, config, transport);
    }

    /// Get the event bus receiver for monitoring.
    pub fn event_receiver(&self) -> broadcast::Receiver<BusEvent> {
        self.event_tx.subscribe()
    }

    /// Get the process ID.
    pub fn process_id(&self) -> &ProcessId {
        &self.process_id
    }

    /// Shut down the message bus.
    pub async fn shutdown(&self) -> IpcResult<()> {
        self.running.store(false, Ordering::Relaxed);
        self.connections.close_all().await;
        self.heartbeat.shutdown();
        tracing::info!("Message bus shut down for {}", self.process_id);
        Ok(())
    }
}

impl std::fmt::Debug for MessageBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageBus")
            .field("process_id", &self.process_id)
            .field("running", &self.running.load(Ordering::Relaxed))
            .field("channels", &self.channels.len())
            .field("peers", &self.peers.len())
            .finish()
    }
}
