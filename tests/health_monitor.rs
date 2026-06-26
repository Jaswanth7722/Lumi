//! Integration tests for the health monitor.

use lumi_runtime::health::HealthMonitor;

#[tokio::test]
async fn test_empty_monitor_returns_unknown() {
    let monitor = HealthMonitor::new();
    let health = monitor.overall_health().await;
    assert_eq!(health.status, lumi_runtime::service::HealthStatus::Unknown);
}

#[tokio::test]
async fn test_not_degraded_by_default() {
    let monitor = HealthMonitor::new();
    assert!(!monitor.is_degraded().await);
}

#[tokio::test]
async fn test_aggregate_score_available() {
    let monitor = HealthMonitor::new();
    let health = monitor.overall_health().await;
    assert!(health.score >= 0.0);
}
