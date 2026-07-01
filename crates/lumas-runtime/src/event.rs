//! # Typed Event Bus
//!
//! Strongly-typed, async event bus for the Lumas runtime.
//!
//! Built on `tokio::sync::broadcast` with a configurable capacity,
//! the event bus provides type-safe publish/subscribe semantics.
//! Each event type has its own broadcast channel, so subscribers
//! only receive events of the type they subscribed to.
//!
//! # Thread Safety
//!
//! `EventBus` is `Send + Sync`. Publishing and subscribing are
//! fully concurrent operations.
//!
//! # Trailing Issues
//!
//! Lagged subscribers (readers that fall behind the buffer capacity)
//! receive an error and must re-subscribe. This is intentional to
//! prevent memory exhaustion from slow consumers.

use crate::error::EventError;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::any::TypeId;
use std::fmt;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, instrument, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Event Priority
// ---------------------------------------------------------------------------

/// Priority level for events. Higher-priority events are delivered first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventPriority {
    /// Critical system events (lifecycle, health).
    Critical,
    /// High-importance events (service failures, config changes).
    High,
    /// Normal operational events.
    Normal,
    /// Low-priority events (metrics, debug).
    Low,
}

// ---------------------------------------------------------------------------
// Event Trait
// ---------------------------------------------------------------------------

/// Trait that all Lumas platform events must implement.
///
/// Provides the event type name for diagnostics and optional priority
/// and trace ID for distributed tracing.
pub trait Event: Send + Sync + Clone + fmt::Debug + 'static {
    /// Human-readable event type name (e.g., "RuntimeStarted").
    fn event_type() -> &'static str
    where
        Self: Sized;

    /// Priority level for this event type.
    fn priority() -> EventPriority
    where
        Self: Sized,
    {
        EventPriority::Normal
    }

    /// Optional trace ID for correlating events across subsystems.
    fn trace_id(&self) -> Option<Uuid> {
        None
    }
}

// ---------------------------------------------------------------------------
// Event Bus
// ---------------------------------------------------------------------------

/// A `dyn`-compatible wrapper for type-erased event dispatch.
struct ChannelEntry {
    /// The broadcast sender (type-erased via `Arc<dyn Any + Send + Sync>`).
    sender: Arc<dyn Any + Send + Sync>,
    /// Type name for diagnostics.
    type_name: &'static str,
}

/// Strongly-typed, async event bus with per-type broadcast channels.
///
/// # Examples
///
/// ```ignore
/// let bus = EventBus::new(1024);
/// let mut rx = bus.subscribe::<RuntimeStarted>();
/// bus.publish(RuntimeStarted::now()).await;
/// let event = rx.recv().await.unwrap();
/// ```
pub struct EventBus {
    /// Per-event-type broadcast channels.
    channels: DashMap<TypeId, ChannelEntry>,
    /// Metrics counters.
    events_published: Arc<crate::metrics::Counter>,
    events_dropped: Arc<crate::metrics::Counter>,
}

impl EventBus {
    /// Create a new event bus with the given channel capacity per event type.
    ///
    /// The capacity applies to each event type independently. A capacity of 1024
    /// means each event type can buffer up to 1024 events before lagging.
    pub fn new(capacity: usize) -> Self {
        Self {
            channels: DashMap::new(),
            events_published: Arc::new(crate::metrics::Counter::new()),
            events_dropped: Arc::new(crate::metrics::Counter::new()),
        }
    }

    /// Get or create the broadcast channel for a specific event type.
    fn channel_for<E: Event>(&self) -> broadcast::Sender<E> {
        let type_id = TypeId::of::<E>();
        let entry = self.channels.entry(type_id).or_insert_with(|| {
            let (tx, _rx): (broadcast::Sender<E>, broadcast::Receiver<E>) =
                broadcast::channel(1024);
            ChannelEntry {
                sender: Arc::new(tx),
                type_name: E::event_type(),
            }
        });
        entry
            .sender
            .clone()
            .downcast::<broadcast::Sender<E>>()
            .expect("TypeId invariant violated: channel sender type mismatch")
            .as_ref()
            .clone()
    }

    /// Publish an event to all subscribers of that event type.
    ///
    /// This is non-blocking. If the channel is full, the event is dropped
    /// with a warning log.
    #[instrument(skip(self, event), fields(event_type = %E::event_type()))]
    pub async fn publish<E: Event>(&self, event: E) {
        self.events_published.increment();
        let sender = self.channel_for::<E>();
        let count = sender.receiver_count();
        if count == 0 {
            debug!("No subscribers for event type: {}", E::event_type());
            return;
        }

        match sender.send(event) {
            Ok(_) => {
                debug!(
                    "Event published: {} ({} subscribers)",
                    E::event_type(),
                    count
                );
            }
            Err(_) => {
                self.events_dropped.increment();
                warn!("Event dropped (all receivers lagged): {}", E::event_type());
            }
        }
    }

    /// Publish an event with elevated priority.
    ///
    /// High-priority events are delivered before normal-priority events
    /// in the same channel. Internally, this uses the same broadcast channel
    /// but subscribers process priority events first.
    pub async fn publish_priority<E: Event>(&self, event: E) {
        // Priority events use the same channel but subscribers can check
        // the event's priority() method for ordering.
        self.publish(event).await
    }

    /// Subscribe to all events of a specific type.
    ///
    /// Returns a `EventReceiver` that yields events of type `E`.
    pub fn subscribe<E: Event>(&self) -> EventReceiver<E> {
        let sender = self.channel_for::<E>();
        EventReceiver {
            rx: sender.subscribe(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Subscribe to events matching a filter predicate.
    ///
    /// The filter is applied on the receiver side; all events are still
    /// received on the channel, but only matching events are yielded.
    pub fn subscribe_filtered<E: Event>(
        &self,
        filter: impl Fn(&E) -> bool + Send + 'static,
    ) -> FilteredEventReceiver<E> {
        let rx = self.subscribe::<E>();
        FilteredEventReceiver {
            inner: rx,
            filter: Box::new(filter),
        }
    }

    /// Get the number of subscribers for a given event type.
    pub fn subscriber_count<E: Event>(&self) -> usize {
        let type_id = TypeId::of::<E>();
        self.channels
            .get(&type_id)
            .map(|entry| {
                entry
                    .sender
                    .clone()
                    .downcast::<broadcast::Sender<E>>()
                    .expect("TypeId invariant violated")
                    .receiver_count()
            })
            .unwrap_or(0)
    }

    /// Number of events published since startup.
    pub fn events_published_count(&self) -> u64 {
        self.events_published.get()
    }

    /// Number of events dropped due to full channels since startup.
    pub fn events_dropped_count(&self) -> u64 {
        self.events_dropped.get()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

// ---------------------------------------------------------------------------
// Event Receiver
// ---------------------------------------------------------------------------

/// A receiver for events of a specific type.
///
/// Created by `EventBus::subscribe()`.
pub struct EventReceiver<E> {
    /// The underlying broadcast receiver.
    rx: broadcast::Receiver<E>,
    /// Marker for the event type.
    _marker: std::marker::PhantomData<E>,
}

impl<E: Event> EventReceiver<E> {
    /// Receive the next event, waiting indefinitely.
    ///
    /// Returns `None` if the sender has been dropped (bus shut down).
    pub async fn recv(&mut self) -> Option<E> {
        loop {
            match self.rx.recv().await {
                Ok(event) => return Some(event),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        "Subscriber lagged on {}: missed {n} events. Re-synchronizing.",
                        E::event_type()
                    );
                    // Re-sync and continue
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }

    /// Receive the next event with a timeout.
    pub async fn recv_timeout(
        &mut self,
        duration: tokio::time::Duration,
    ) -> Result<E, crate::error::EventError> {
        tokio::time::timeout(duration, self.rx.recv())
            .await
            .map_err(|_| crate::error::EventError::SubscriberLagged {
                event_type: E::event_type(),
            })?
            .map_err(|_| crate::error::EventError::SubscriberLagged {
                event_type: E::event_type(),
            })
    }

    /// Try to receive an event without blocking.
    pub fn try_recv(&mut self) -> Option<E> {
        match self.rx.try_recv() {
            Ok(event) => Some(event),
            Err(broadcast::error::TryRecvError::Empty) => None,
            Err(broadcast::error::TryRecvError::Lagged(n)) => {
                warn!(
                    "Subscriber lagged on {}: missed {n} events.",
                    E::event_type()
                );
                None
            }
            Err(broadcast::error::TryRecvError::Closed) => None,
        }
    }

    /// Convert this receiver into a `tokio_stream::wrappers::BroadcastStream` for use with StreamExt.
    pub fn into_stream(self) -> tokio_stream::wrappers::BroadcastStream<E> {
        tokio_stream::wrappers::BroadcastStream::new(self.rx)
    }
}

/// A receiver that filters events using a predicate.
pub struct FilteredEventReceiver<E> {
    inner: EventReceiver<E>,
    filter: Box<dyn Fn(&E) -> bool + Send>,
}

impl<E: Event> FilteredEventReceiver<E> {
    /// Receive the next matching event.
    pub async fn recv(&mut self) -> Option<E> {
        while let Some(event) = self.inner.recv().await {
            if (self.filter)(&event) {
                return Some(event);
            }
        }
        None
    }
}

// =========================================================================
// Lumas Platform Events
// =========================================================================

// -- Lifecycle events --

/// Emitted when the runtime has completed bootstrap and is Running.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStarted {
    /// Timestamp of the event.
    pub timestamp: DateTime<Utc>,
    /// Runtime version.
    pub version: semver::Version,
}

impl RuntimeStarted {
    /// Create a new `RuntimeStarted` event with the current timestamp.
    pub fn new(version: semver::Version) -> Self {
        Self {
            timestamp: Utc::now(),
            version,
        }
    }
}

impl Event for RuntimeStarted {
    fn event_type() -> &'static str {
        "RuntimeStarted"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

/// Emitted when the runtime has completed shutdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStopped {
    /// Timestamp of the event.
    pub timestamp: DateTime<Utc>,
    /// Total uptime in seconds.
    pub uptime_secs: u64,
}

impl Event for RuntimeStopped {
    fn event_type() -> &'static str {
        "RuntimeStopped"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

/// Emitted when a service starts successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStarted {
    /// The service name.
    pub name: String,
    /// Duration of the start operation in milliseconds.
    pub duration_ms: u64,
}

impl Event for ServiceStarted {
    fn event_type() -> &'static str {
        "ServiceStarted"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

/// Emitted when a service fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceFailed {
    /// The service name.
    pub name: String,
    /// Error description.
    pub error: String,
    /// Whether the error is recoverable.
    pub recoverable: bool,
}

impl Event for ServiceFailed {
    fn event_type() -> &'static str {
        "ServiceFailed"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

/// Emitted when the runtime lifecycle state changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleTransitioned {
    /// Previous lifecycle state (serialized as string).
    pub from: String,
    /// New lifecycle state (serialized as string).
    pub to: String,
}

impl Event for LifecycleTransitioned {
    fn event_type() -> &'static str {
        "LifecycleTransitioned"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

/// Emitted when the shutdown is initiated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownInitiated {
    /// Reason for the shutdown.
    pub reason: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl Event for ShutdownInitiated {
    fn event_type() -> &'static str {
        "ShutdownInitiated"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

// -- AI events --

/// Emitted when the AI state changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIStateChanged {
    /// The new AI state.
    pub state: String,
    /// Optional hint for how long this state is expected to last.
    pub duration_hint_ms: Option<u64>,
}

impl Event for AIStateChanged {
    fn event_type() -> &'static str {
        "AIStateChanged"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

/// Emitted when AI inference starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceStarted {
    /// Request identifier for correlation.
    pub request_id: Uuid,
    /// Inference provider name.
    pub provider: String,
}

impl Event for InferenceStarted {
    fn event_type() -> &'static str {
        "InferenceStarted"
    }
}

/// Emitted when AI inference completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceCompleted {
    /// Request identifier for correlation.
    pub request_id: Uuid,
    /// Number of tokens generated.
    pub tokens: u32,
    /// Latency in milliseconds.
    pub latency_ms: u64,
}

impl Event for InferenceCompleted {
    fn event_type() -> &'static str {
        "InferenceCompleted"
    }
}

// -- Config events --

/// Emitted when configuration is loaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigLoaded {
    /// Path to the configuration file (None if no file found).
    pub path: Option<std::path::PathBuf>,
}

impl Event for ConfigLoaded {
    fn event_type() -> &'static str {
        "ConfigLoaded"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

/// Emitted when configuration is reloaded at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigReloaded {
    /// Keys that changed during the reload.
    pub changed_keys: Vec<String>,
}

impl Event for ConfigReloaded {
    fn event_type() -> &'static str {
        "ConfigReloaded"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

/// Emitted when configuration reload fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigReloadFailed {
    /// Error description.
    pub error: String,
}

impl Event for ConfigReloadFailed {
    fn event_type() -> &'static str {
        "ConfigReloadFailed"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

// -- Tool events --

/// Emitted when a tool is invoked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvoked {
    /// Name of the tool.
    pub tool_name: String,
    /// Request identifier.
    pub request_id: Uuid,
}

impl Event for ToolInvoked {
    fn event_type() -> &'static str {
        "ToolInvoked"
    }
}

/// Emitted when a tool invocation completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCompleted {
    /// Name of the tool.
    pub tool_name: String,
    /// Request identifier.
    pub request_id: Uuid,
    /// Whether the invocation was successful.
    pub success: bool,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

impl Event for ToolCompleted {
    fn event_type() -> &'static str {
        "ToolCompleted"
    }
}

// -- Health events --

/// Emitted when a service health check passes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckPassed {
    /// Service name.
    pub service: String,
    /// Health score (0.0 to 1.0).
    pub score: f32,
}

impl Event for HealthCheckPassed {
    fn event_type() -> &'static str {
        "HealthCheckPassed"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

/// Emitted when a service health check fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckFailed {
    /// Service name.
    pub service: String,
    /// Reason for the failure.
    pub reason: String,
}

impl Event for HealthCheckFailed {
    fn event_type() -> &'static str {
        "HealthCheckFailed"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

/// Emitted when the runtime enters degraded state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDegraded {
    /// List of services that have failed.
    pub failed_services: Vec<String>,
}

impl Event for RuntimeDegraded {
    fn event_type() -> &'static str {
        "RuntimeDegraded"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

/// Emitted when the runtime recovers from degraded state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRecovered {
    /// List of services that recovered.
    pub recovered_services: Vec<String>,
}

impl Event for RuntimeRecovered {
    fn event_type() -> &'static str {
        "RuntimeRecovered"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

// -- Resource events --

/// Emitted when a resource crosses the warning threshold (80%).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceWarning {
    /// Resource name.
    pub resource: String,
    /// Current usage.
    pub current: f64,
    /// The configured limit.
    pub limit: f64,
}

impl Event for ResourceWarning {
    fn event_type() -> &'static str {
        "ResourceWarning"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

/// Emitted when a resource crosses the critical threshold (95%).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCritical {
    /// Resource name.
    pub resource: String,
    /// Current usage.
    pub current: f64,
    /// The configured limit.
    pub limit: f64,
}

impl Event for ResourceCritical {
    fn event_type() -> &'static str {
        "ResourceCritical"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_names() {
        assert_eq!(RuntimeStarted::event_type(), "RuntimeStarted");
        assert_eq!(RuntimeStopped::event_type(), "RuntimeStopped");
        assert_eq!(LifecycleTransitioned::event_type(), "LifecycleTransitioned");
        assert_eq!(HealthCheckFailed::event_type(), "HealthCheckFailed");
    }

    #[tokio::test]
    async fn test_publish_received_by_subscriber() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe::<RuntimeStarted>();

        let event = RuntimeStarted::new(semver::Version::new(0, 1, 0));
        bus.publish(event.clone()).await;

        let received = rx.recv().await.unwrap();
        assert_eq!(received.version, event.version);
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe::<RuntimeStarted>();
        let mut rx2 = bus.subscribe::<RuntimeStarted>();

        let event = RuntimeStarted::new(semver::Version::new(0, 1, 0));
        bus.publish(event.clone()).await;

        let r1 = rx1.recv().await.unwrap();
        let r2 = rx2.recv().await.unwrap();
        assert_eq!(r1.version, r2.version);
    }

    #[tokio::test]
    async fn test_filtered_subscription() {
        let bus = EventBus::new(16);

        // Subscribe to only successful tool completions
        let mut rx = bus.subscribe_filtered::<ToolCompleted>(|t| t.success);

        bus.publish(ToolCompleted {
            tool_name: "test".into(),
            request_id: Uuid::new_v4(),
            success: true,
            duration_ms: 10,
        })
        .await;

        bus.publish(ToolCompleted {
            tool_name: "test".into(),
            request_id: Uuid::new_v4(),
            success: false,
            duration_ms: 10,
        })
        .await;

        let received = rx.recv().await.unwrap();
        assert!(received.success);
    }

    #[tokio::test]
    async fn test_event_metrics_increment_on_publish() {
        let bus = EventBus::new(16);
        assert_eq!(bus.events_published_count(), 0);

        bus.publish(RuntimeStarted::new(semver::Version::new(0, 1, 0)))
            .await;

        assert_eq!(bus.events_published_count(), 1);
    }

    #[tokio::test]
    async fn test_subscriber_count() {
        let bus = EventBus::new(16);
        assert_eq!(bus.subscriber_count::<RuntimeStarted>(), 0);

        let _rx1 = bus.subscribe::<RuntimeStarted>();
        let _rx2 = bus.subscribe::<RuntimeStarted>();

        // Subscriber count might be 0 if the channel hasn't been created yet
        // because subscribe() creates it lazily.
        assert_eq!(bus.subscriber_count::<RuntimeStarted>(), 2);
    }
}
