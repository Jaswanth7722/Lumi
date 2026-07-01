//! Integration tests for the typed event bus.

use lumas_runtime::event::*;
use uuid::Uuid;

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
async fn test_filtered_subscription_receives_matching_events_only() {
    let bus = EventBus::new(16);
    let mut rx = bus.subscribe_filtered::<ToolCompleted>(|t| t.success);

    bus.publish(ToolCompleted {
        tool_name: "success".into(),
        request_id: Uuid::new_v4(),
        success: true,
        duration_ms: 10,
    })
    .await;

    bus.publish(ToolCompleted {
        tool_name: "fail".into(),
        request_id: Uuid::new_v4(),
        success: false,
        duration_ms: 20,
    })
    .await;

    let received = rx.recv().await.unwrap();
    assert!(received.success);
    assert_eq!(received.tool_name, "success");
}

#[tokio::test]
async fn test_multiple_subscribers_all_receive_event() {
    let bus = EventBus::new(16);
    let mut rx1 = bus.subscribe::<RuntimeStarted>();
    let mut rx2 = bus.subscribe::<RuntimeStarted>();

    let event = RuntimeStarted::new(semver::Version::new(1, 0, 0));
    bus.publish(event.clone()).await;

    let r1 = rx1.recv().await.unwrap();
    let r2 = rx2.recv().await.unwrap();
    assert_eq!(r1.version, r2.version);
    assert_eq!(r1.timestamp.timestamp(), r2.timestamp.timestamp());
}

#[tokio::test]
async fn test_event_metrics_increment_on_publish() {
    let bus = EventBus::new(16);
    assert_eq!(bus.events_published_count(), 0);

    bus.publish(RuntimeStarted::new(semver::Version::new(0, 1, 0)))
        .await;

    assert_eq!(bus.events_published_count(), 1);

    bus.publish(RuntimeStarted::new(semver::Version::new(0, 1, 1)))
        .await;

    assert_eq!(bus.events_published_count(), 2);
}

#[tokio::test]
async fn test_subscriber_count_reflects_active_subscribers() {
    let bus = EventBus::new(16);
    assert_eq!(bus.subscriber_count::<RuntimeStarted>(), 0);

    let _rx1 = bus.subscribe::<RuntimeStarted>();
    // First subscribe creates the channel
    assert_eq!(bus.subscriber_count::<RuntimeStarted>(), 1);

    let _rx2 = bus.subscribe::<RuntimeStarted>();
    assert_eq!(bus.subscriber_count::<RuntimeStarted>(), 2);
}
