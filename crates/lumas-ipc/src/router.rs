//! # Message Router
//!
//! Routes messages to their destinations based on the channel's routing pattern.
//! Supports direct, multicast, topic, and broadcast routing with priority queuing.

use crate::connection::ConnectionManager;
use crate::error::{IpcError, IpcResult, RoutingError};
use crate::message::{LumiMessage, MessageKind, MessageTarget, ProcessId};
use crate::peer::PeerRegistry;
use crate::registry::ChannelRegistry;
use crate::transport::SharedTransport;
use crossbeam::queue::SegQueue;
use dashmap::DashMap;
use std::sync::Arc;

/// Route entry for a channel.
#[derive(Debug, Clone)]
pub enum RouteEntry {
    /// Direct routing to a specific process via a transport.
    Direct {
        target: ProcessId,
        transport: SharedTransport,
    },
    /// Multicast to multiple targets.
    Multicast {
        targets: Vec<ProcessId>,
    },
    /// Topic-based routing with pattern matching.
    Topic {
        pattern: String,
    },
    /// Broadcast to all subscribers.
    Broadcast,
}

/// Message router for dispatching messages to their destinations.
pub struct Router {
    /// Routes keyed by channel name
    routes: DashMap<String, Vec<RouteEntry>>,
    /// Priority queues: one per priority level (0=Low, 1=Normal, 2=High, 3=Critical)
    priority_queues: [SegQueue<(LumiMessage, String)>; 4],
    /// Channel registry
    channels: Arc<ChannelRegistry>,
    /// Peer registry
    peers: Arc<PeerRegistry>,
    /// Connection manager
    connections: Arc<ConnectionManager>,
}

impl Router {
    /// Create a new router.
    pub fn new(
        channels: Arc<ChannelRegistry>,
        peers: Arc<PeerRegistry>,
        connections: Arc<ConnectionManager>,
    ) -> Self {
        Self {
            routes: DashMap::new(),
            priority_queues: [
                SegQueue::new(),
                SegQueue::new(),
                SegQueue::new(),
                SegQueue::new(),
            ],
            channels,
            peers,
            connections,
        }
    }

    /// Register a route for a channel.
    pub fn register_route(&self, channel: impl Into<String>, route: RouteEntry) {
        let channel = channel.into();
        self.routes
            .entry(channel)
            .or_insert_with(Vec::new)
            .push(route);
    }

    /// Route a message to its destination(s).
    pub async fn route(&self, msg: LumiMessage) -> Result<usize, RoutingError> {
        let channel = msg.channel.0.clone();
        let target = msg.receiver.clone();
        let priority = msg.priority as usize;

        let routes = self.routes.get(&channel)
            .map(|r| r.clone())
            .unwrap_or_else(|| {
                // Default: use the channel's transport
                if let Some(entry) = self.channels.get(&channel) {
                    vec![RouteEntry::Direct {
                        target: ProcessId::Core,
                        transport: entry.transport.clone(),
                    }]
                } else {
                    vec![]
                }
            });

        if routes.is_empty() {
            return Err(RoutingError::NoRoute { channel: channel.clone() });
        }

        let mut delivery_count = 0;

        for route in &routes {
            match route {
                RouteEntry::Direct { target: _, transport } => {
                    // Critical priority: dispatch inline
                    if priority >= 3 {
                        if transport.send(msg.clone()).await.is_ok() {
                            delivery_count += 1;
                        }
                    } else {
                        // Other priorities: queue for dispatch
                        let queue_idx = priority.min(3);
                        self.priority_queues[queue_idx].push((msg.clone(), channel.clone()));
                        delivery_count += 1;
                    }
                }
                RouteEntry::Multicast { targets } => {
                    // Find transport for each target and send
                    for target_id in targets {
                        if let Some(entry) = self.channels.get(&channel) {
                            if entry.transport.send(msg.clone()).await.is_ok() {
                                delivery_count += 1;
                            }
                        }
                    }
                }
                RouteEntry::Broadcast => {
                    // Send to all subscribers
                    let subscribers = self.channels.subscribers(&channel);
                    for _sub in &subscribers {
                        if let Some(entry) = self.channels.get(&channel) {
                            if entry.transport.send(msg.clone()).await.is_ok() {
                                delivery_count += 1;
                            }
                        }
                    }
                }
                RouteEntry::Topic { pattern } => {
                    // Find routes matching the topic pattern
                    let matching_routes: Vec<String> = self.routes.iter()
                        .filter(|e| e.key().starts_with(pattern))
                        .map(|e| e.key().clone())
                        .collect();

                    for chan in &matching_routes {
                        if let Some(entry) = self.channels.get(chan) {
                            if entry.transport.send(msg.clone()).await.is_ok() {
                                delivery_count += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(delivery_count)
    }

    /// Process one message from the highest-priority non-empty queue.
    pub fn process_next(&self) -> Option<(LumiMessage, String)> {
        // Drain from highest priority to lowest
        for queue in (0..4).rev() {
            if let Some(msg) = self.priority_queues[queue].pop() {
                return Some(msg);
            }
        }
        None
    }

    /// Get the current queue depth for all priority levels.
    pub fn queue_depth(&self) -> [usize; 4] {
        [
            self.priority_queues[0].len(),
            self.priority_queues[1].len(),
            self.priority_queues[2].len(),
            self.priority_queues[3].len(),
        ]
    }

    /// Get the number of registered routes.
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
}
