//! # Heartbeat Engine
//!
//! Monitors peer health through periodic ping/pong exchanges.
//! Tracks round-trip time, detects unresponsive peers, and emits
//! events when peer health status changes.

use crate::connection::ConnectionId;
use crate::message::{LumiMessage, MessageId, MessageKind, MessagePayload, ProcessId};
use crate::event::{BusEvent, PeerHealthStatus};
use dashmap::DashMap;
use tokio::sync::broadcast;
use tracing::{debug, warn};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Heartbeat configuration.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Interval between heartbeat pings
    pub interval: Duration,
    /// Timeout after which a peer is considered dead
    pub timeout: Duration,
    /// Latency warning threshold
    pub latency_warn_us: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            timeout: Duration::from_secs(15),
            latency_warn_us: 10_000,
        }
    }
}

/// A pending ping awaiting a pong response.
#[derive(Debug)]
pub struct PendingPing {
    /// When the ping was sent
    pub sent_at: Instant,
    /// The ping message ID for correlation
    pub ping_id: MessageId,
    /// The peer we pinged
    pub peer: ProcessId,
}

/// Peer health tracking.
pub struct PeerHealth {
    /// Peer process ID
    peer: ProcessId,
    /// When we last received a pong
    last_pong_at: std::sync::Mutex<Option<Instant>>,
    /// Last measured RTT in microseconds
    last_rtt_us: std::sync::Mutex<Option<u64>>,
    /// Consecutive missed heartbeats
    consecutive_timeouts: std::sync::Mutex<u32>,
    /// Current health status
    status: std::sync::Mutex<PeerHealthStatus>,
}

impl PeerHealth {
    /// Create a new peer health tracker.
    pub fn new() -> Self {
        Self {
            peer: ProcessId::Core,
            last_pong_at: std::sync::Mutex::new(None),
            last_rtt_us: std::sync::Mutex::new(None),
            consecutive_timeouts: std::sync::Mutex::new(0),
            status: std::sync::Mutex::new(PeerHealthStatus::Healthy),
        }
    }

    /// Record a successful pong response.
    pub fn record_pong(&self, rtt_us: u64) {
        if let Ok(mut last) = self.last_pong_at.lock() {
            *last = Some(Instant::now());
        }
        if let Ok(mut rtt) = self.last_rtt_us.lock() {
            *rtt = Some(rtt_us);
        }
        if let Ok(mut timeouts) = self.consecutive_timeouts.lock() {
            *timeouts = 0;
        }
    }

    /// Record a missed heartbeat.
    pub fn record_timeout(&self) {
        if let Ok(mut timeouts) = self.consecutive_timeouts.lock() {
            *timeouts += 1;
        }
    }

    /// Get the number of consecutive timeouts.
    pub fn consecutive_timeouts(&self) -> u32 {
        self.consecutive_timeouts.lock().map(|t| *t).unwrap_or(0)
    }

    /// Get the current health status.
    pub fn status(&self) -> PeerHealthStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or(PeerHealthStatus::Healthy)
    }

    /// Get the last RTT in microseconds.
    pub fn last_rtt_us(&self) -> Option<u64> {
        self.last_rtt_us.lock().map(|r| *r).unwrap_or(None)
    }

    /// Set the health status.
    pub fn set_status(&self, status: PeerHealthStatus) {
        if let Ok(mut s) = self.status.lock() {
            *s = status;
        }
    }
}

impl std::fmt::Debug for PeerHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerHealth")
            .field("status", &self.status())
            .field("last_rtt_us", &self.last_rtt_us())
            .field("consecutive_timeouts", &self.consecutive_timeouts())
            .finish()
    }
}

/// Heartbeat engine for monitoring peer health.
pub struct HeartbeatEngine {
    config: HeartbeatConfig,
    pending_pings: DashMap<MessageId, PendingPing>,
    peer_health: DashMap<ProcessId, Arc<PeerHealth>>,
    event_tx: Option<broadcast::Sender<BusEvent>>,
    running: AtomicBool,
}

impl HeartbeatEngine {
    /// Create a new heartbeat engine.
    pub fn new(config: HeartbeatConfig) -> Self {
        Self {
            config,
            pending_pings: DashMap::new(),
            peer_health: DashMap::new(),
            event_tx: None,
            running: AtomicBool::new(true),
        }
    }

    /// Set the event bus sender for emitting health events.
    pub fn set_event_tx(&mut self, tx: broadcast::Sender<BusEvent>) {
        self.event_tx = Some(tx);
    }

    /// Register a peer for health monitoring.
    pub fn register_peer(&self, peer: ProcessId) -> Arc<PeerHealth> {
        let health = Arc::new(PeerHealth::new());
        self.peer_health.insert(peer, health.clone());
        health
    }

    /// Get the health tracker for a peer.
    pub fn get_health(&self, peer: &ProcessId) -> Option<Arc<PeerHealth>> {
        self.peer_health.get(peer).map(|h| h.clone())
    }

    /// Remove a peer from health monitoring.
    pub fn remove_peer(&self, peer: &ProcessId) {
        self.peer_health.remove(peer);
    }

    /// Record an incoming pong and compute RTT.
    pub fn record_pong(&self, ping_id: &MessageId, peer: &ProcessId) -> Option<u64> {
        if let Some((_, ping)) = self.pending_pings.remove(ping_id) {
            let rtt_us = ping.sent_at.elapsed().as_micros() as u64;

            if let Some(health) = self.peer_health.get(peer) {
                health.record_pong(rtt_us);

                // Check if latency exceeds warning threshold
                if rtt_us > self.config.latency_warn_us {
                    if let Some(ref tx) = self.event_tx {
                        let _ = tx.send(BusEvent::PeerDegraded {
                            peer: peer.clone(),
                            latency_us: rtt_us,
                        });
                    }
                }
            }

            Some(rtt_us)
        } else {
            None
        }
    }

    /// Check for timed-out peers.
    pub fn check_timeouts(&self) {
        let now = Instant::now();

        // Check all registered peers
        let mut dead_peers = Vec::new();

        for entry in self.peer_health.iter() {
            let peer = entry.key();
            let health = entry.value();

            // Check if we have pending pings for this peer that have timed out
            let timed_out: Vec<MessageId> = self.pending_pings
                .iter()
                .filter(|p| p.peer == *peer && p.sent_at.elapsed() > self.config.timeout)
                .map(|p| p.ping_id.clone())
                .collect();

            // Remove timed-out pings
            for ping_id in &timed_out {
                self.pending_pings.remove(ping_id);
            }

            if !timed_out.is_empty() {
                health.record_timeout();
                let timeouts = health.consecutive_timeouts();

                if timeouts >= 3 {
                    // Peer is dead
                    health.set_status(PeerHealthStatus::Dead);
                    dead_peers.push(peer.clone());
                }
            }
        }

        // Emit dead peer events
        if let Some(ref tx) = self.event_tx {
            for peer in &dead_peers {
                let _ = tx.send(BusEvent::PeerDead {
                    peer: peer.clone(),
                    last_seen: Instant::now(),
                });
            }
        }
    }

    /// Record a sent ping.
    pub fn record_ping(&self, ping_id: MessageId, peer: ProcessId) {
        self.pending_pings.insert(ping_id.clone(), PendingPing {
            sent_at: Instant::now(),
            ping_id,
            peer,
        });
    }

    /// Create a heartbeat ping message.
    pub fn create_ping(&self, sender: ProcessId, peer: ProcessId) -> LumiMessage {
        LumiMessage::builder()
            .sender(sender)
            .receiver(crate::message::MessageTarget::Process(peer))
            .channel("protocol.heartbeat")
            .kind(MessageKind::Heartbeat)
            .payload(MessagePayload::HeartbeatPing {
                sent_at: chrono::Utc::now().timestamp_micros() as u64,
            })
            .build()
            .expect("Failed to build heartbeat ping")
    }

    /// Check if a message is a heartbeat message.
    pub fn is_heartbeat(msg: &LumiMessage) -> bool {
        matches!(msg.kind, MessageKind::Heartbeat)
    }

    /// Shut down the heartbeat engine.
    pub fn shutdown(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
