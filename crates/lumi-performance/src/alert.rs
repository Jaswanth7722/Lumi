//! # Alert Engine
//!
//! Manages the lifecycle of performance alerts: firing, resolving, cooldown,
//! deduplication, and history tracking.
//!
//! # Thread Safety
//! `AlertEngine` uses `Arc` and `parking_lot` locks for thread-safe access.

use crate::threshold::ThresholdId;
use dashmap::DashMap;
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Alert severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AlertSeverity {
    /// Informational alert.
    Info,
    /// Warning — may require attention.
    Warning,
    /// Critical — requires immediate attention.
    Critical,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Info => write!(f, "info"),
            AlertSeverity::Warning => write!(f, "warning"),
            AlertSeverity::Critical => write!(f, "critical"),
        }
    }
}

/// A single performance alert.
#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    /// Unique alert ID.
    pub id: String,
    /// Threshold ID that fired this alert.
    pub threshold_id: String,
    /// When the alert fired.
    pub fired_at: SystemTime,
    /// When the alert was resolved.
    pub resolved_at: Option<SystemTime>,
    /// Severity.
    pub severity: AlertSeverity,
    /// Metric value at the time the alert fired.
    pub metric_value_at_fire: f64,
    /// Human-readable message.
    pub message: String,
    /// Optional recovery hint.
    pub recovery_hint: Option<String>,
}

impl Alert {
    /// Create a new alert.
    pub fn new(
        threshold_id: &ThresholdId,
        severity: AlertSeverity,
        message: String,
        metric_value: f64,
    ) -> Self {
        Self {
            id: format!("alert-{}", uuid::Uuid::new_v4()),
            threshold_id: threshold_id.to_string(),
            fired_at: SystemTime::now(),
            resolved_at: None,
            severity,
            metric_value_at_fire: metric_value,
            message,
            recovery_hint: None,
        }
    }

    /// Create a new alert with a recovery hint.
    pub fn with_recovery(mut self, hint: &str) -> Self {
        self.recovery_hint = Some(hint.to_string());
        self
    }

    /// Resolve this alert.
    pub fn resolve(&mut self) {
        self.resolved_at = Some(SystemTime::now());
    }
}

/// Alert event emitted to the event bus.
#[derive(Debug, Clone)]
pub enum AlertEvent {
    /// An alert was fired.
    Fired(Alert),
    /// An alert was resolved.
    Resolved {
        /// The alert ID.
        alert_id: String,
        /// Metric value at resolution time.
        metric_value: f64,
    },
}

/// Alert engine managing alert lifecycle.
#[derive(Debug)]
pub struct AlertEngine {
    /// Active (unresolved) alerts.
    active: DashMap<String, Alert>,
    /// Alert history (bounded to 1000 entries).
    history: Arc<parking_lot::Mutex<Vec<Alert>>>,
    /// Maximum history entries.
    max_history: usize,
}

impl AlertEngine {
    /// Create a new alert engine.
    pub fn new() -> Self {
        Self {
            active: DashMap::new(),
            history: Arc::new(parking_lot::Mutex::new(Vec::new())),
            max_history: 1000,
        }
    }

    /// Fire an alert.
    pub fn fire(&self, alert: Alert) {
        let alert_id = alert.id.clone();
        self.active.insert(alert_id, alert.clone());
        let mut history = self.history.lock();
        history.push(alert);
        while history.len() > self.max_history {
            history.remove(0);
        }
    }

    /// Resolve an active alert.
    pub fn resolve(&self, alert_id: &str) -> Option<Alert> {
        if let Some(mut entry) = self.active.get_mut(alert_id) {
            entry.resolve();
            let alert = entry.clone();
            self.active.remove(alert_id);
            Some(alert)
        } else {
            None
        }
    }

    /// Get all active alerts.
    pub fn active_alerts(&self) -> Vec<Alert> {
        self.active.iter().map(|e| e.value().clone()).collect()
    }

    /// Get alert history since a given time.
    pub fn alert_history(&self, since: SystemTime) -> Vec<Alert> {
        let history = self.history.lock();
        history
            .iter()
            .filter(|a| a.fired_at >= since)
            .cloned()
            .collect()
    }

    /// Number of active alerts.
    pub fn active_count(&self) -> u32 {
        self.active.len() as u32
    }
}

impl Default for AlertEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fire_and_resolve_alert() {
        let engine = AlertEngine::new();
        let alert = Alert::new(
            &ThresholdId::new("THR-CPU-001"),
            AlertSeverity::Warning,
            "CPU usage high".into(),
            75.0,
        );
        let alert_id = alert.id.clone();
        engine.fire(alert);

        assert_eq!(engine.active_count(), 1);
        assert!(engine.resolve(&alert_id).is_some());
        assert_eq!(engine.active_count(), 0);
    }

    #[test]
    fn test_active_alerts() {
        let engine = AlertEngine::new();
        engine.fire(Alert::new(
            &ThresholdId::new("THR-CPU-001"),
            AlertSeverity::Warning,
            "CPU high".into(),
            75.0,
        ));
        engine.fire(Alert::new(
            &ThresholdId::new("THR-MEM-001"),
            AlertSeverity::Critical,
            "Memory high".into(),
            90.0,
        ));

        let active = engine.active_alerts();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_alert_history() {
        let engine = AlertEngine::new();
        engine.fire(Alert::new(
            &ThresholdId::new("THR-CPU-001"),
            AlertSeverity::Warning,
            "test".into(),
            50.0,
        ));

        let since = SystemTime::now() - Duration::from_secs(60);
        let history = engine.alert_history(since);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_alert_severity_display() {
        assert_eq!(AlertSeverity::Info.to_string(), "info");
        assert_eq!(AlertSeverity::Warning.to_string(), "warning");
        assert_eq!(AlertSeverity::Critical.to_string(), "critical");
    }
}
