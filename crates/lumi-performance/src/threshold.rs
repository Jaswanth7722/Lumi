//! # Threshold Engine
//!
//! Evaluates performance thresholds and triggers alerts when metrics
//! exceed configured bounds. Supports multiple condition types including
//! percentile-based and sustained-above detection.
//!
//! # Thread Safety
//! `ThresholdEngine` uses `DashMap` for concurrent access and `Arc` for sharing.

use crate::alert::{Alert, AlertSeverity};
use crate::metric::MetricName;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Unique threshold identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThresholdId(pub String);

impl ThresholdId {
    /// Create a new threshold ID.
    pub fn new(id: &str) -> Self {
        Self(id.to_string())
    }
}

impl std::fmt::Display for ThresholdId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Condition type for threshold evaluation.
#[derive(Debug, Clone)]
pub enum ThresholdCondition {
    /// Value is greater than the threshold.
    GreaterThan,
    /// Value is less than the threshold.
    LessThan,
    /// A percentile value exceeds the threshold.
    Percentile {
        /// The percentile (0.0–100.0).
        p: f64,
        /// The value to exceed.
        greater_than: f64,
    },
    /// Rate exceeds a threshold.
    RateExceeds {
        /// Events per second.
        per_second: f64,
    },
    /// Value must exceed for N seconds before alerting.
    SustainedAbove {
        /// Duration the value must exceed the threshold.
        duration: Duration,
    },
}

/// A single threshold definition.
#[derive(Debug, Clone)]
pub struct Threshold {
    /// Unique threshold ID.
    pub id: ThresholdId,
    /// The metric to evaluate.
    pub metric_name: MetricName,
    /// The condition type.
    pub condition: ThresholdCondition,
    /// Threshold value.
    pub value: f64,
    /// Optional rolling window (None = instantaneous).
    pub window: Option<Duration>,
    /// Severity when this threshold fires.
    pub severity: AlertSeverity,
    /// Minimum time between repeated alerts.
    pub cooldown: Duration,
    /// Message template (use {value} and {threshold} placeholders).
    pub message_template: &'static str,
    /// Optional recovery hint.
    pub recovery_hint: Option<&'static str>,
}

/// State tracking for a single threshold.
#[derive(Debug)]
struct ThresholdState {
    /// When the condition first became true (for sustained-above).
    sustained_since: Option<SystemTime>,
    /// When the last alert was fired (for cooldown).
    last_alert_at: Option<SystemTime>,
    /// Whether an alert is currently active.
    alert_active: bool,
}

/// Threshold evaluation engine.
#[derive(Debug)]
pub struct ThresholdEngine {
    thresholds: Vec<Threshold>,
    states: DashMap<String, ThresholdState>,
}

impl ThresholdEngine {
    /// Create a new threshold engine.
    pub fn new() -> Self {
        Self {
            thresholds: Vec::new(),
            states: DashMap::new(),
        }
    }

    /// Register a threshold.
    pub fn register(&mut self, threshold: Threshold) {
        let id = threshold.id.0.clone();
        self.states.insert(
            id,
            ThresholdState {
                sustained_since: None,
                last_alert_at: None,
                alert_active: false,
            },
        );
        self.thresholds.push(threshold);
    }

    /// Evaluate all registered thresholds against the given metric value.
    pub fn evaluate(&self, metric_name: &str, value: f64) -> Vec<Alert> {
        let now = SystemTime::now();
        let mut alerts = Vec::new();

        for threshold in &self.thresholds {
            if threshold.metric_name.as_str() != metric_name {
                continue;
            }

            let state_key = threshold.id.0.clone();
            let mut state = self.states.get_mut(&state_key);

            let triggered = match threshold.condition {
                ThresholdCondition::GreaterThan => value > threshold.value,
                ThresholdCondition::LessThan => value < threshold.value,
                ThresholdCondition::Percentile { p: _, greater_than } => value > greater_than,
                ThresholdCondition::RateExceeds { per_second } => value > per_second,
                ThresholdCondition::SustainedAbove { duration } => {
                    if value > threshold.value {
                        let sustained_since = state
                            .as_ref()
                            .and_then(|s| s.sustained_since)
                            .unwrap_or(now);
                        if let Some(ref mut s) = state.as_mut() {
                            if s.sustained_since.is_none() {
                                s.sustained_since = Some(now);
                            }
                        }
                        now.duration_since(sustained_since)
                            .unwrap_or(Duration::ZERO)
                            >= duration
                    } else {
                        if let Some(ref mut s) = state.as_mut() {
                            s.sustained_since = None;
                        }
                        false
                    }
                }
            };

            if triggered {
                let should_fire = match state.as_ref() {
                    Some(s) => {
                        let cooldown_ok = s
                            .last_alert_at
                            .map(|t| {
                                now.duration_since(t).unwrap_or(Duration::ZERO)
                                    >= threshold.cooldown
                            })
                            .unwrap_or(true);
                        cooldown_ok
                    }
                    None => true,
                };

                if should_fire {
                    let msg = threshold
                        .message_template
                        .replace("{value}", &format!("{:.1}", value))
                        .replace("{threshold}", &format!("{:.1}", threshold.value));

                    alerts.push(Alert::new(&threshold.id, threshold.severity, msg, value));

                    if let Some(ref mut s) = state.as_mut() {
                        s.last_alert_at = Some(now);
                        s.alert_active = true;
                    }
                }
            } else {
                if let Some(ref mut s) = state.as_mut() {
                    s.alert_active = false;
                    s.sustained_since = None;
                }
            }
        }

        alerts
    }

    /// Get all registered thresholds.
    pub fn thresholds(&self) -> &[Threshold] {
        &self.thresholds
    }
}

impl Default for ThresholdEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threshold_greater_than() {
        let mut engine = ThresholdEngine::new();
        engine.register(Threshold {
            id: ThresholdId::new("THR-CPU-001"),
            metric_name: MetricName::from_str("lumi.system.cpu.lumi_process_percent"),
            condition: ThresholdCondition::GreaterThan,
            value: 50.0,
            window: None,
            severity: AlertSeverity::Warning,
            cooldown: Duration::from_secs(10),
            message_template: "CPU {value}% exceeds {threshold}%",
            recovery_hint: None,
        });

        let alerts = engine.evaluate("lumi.system.cpu.lumi_process_percent", 75.0);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, AlertSeverity::Warning);
    }

    #[test]
    fn test_threshold_not_triggered() {
        let mut engine = ThresholdEngine::new();
        engine.register(Threshold {
            id: ThresholdId::new("THR-CPU-001"),
            metric_name: MetricName::from_str("lumi.system.cpu.lumi_process_percent"),
            condition: ThresholdCondition::GreaterThan,
            value: 50.0,
            window: None,
            severity: AlertSeverity::Warning,
            cooldown: Duration::from_secs(10),
            message_template: "CPU {value}% exceeds {threshold}%",
            recovery_hint: None,
        });

        let alerts = engine.evaluate("lumi.system.cpu.lumi_process_percent", 25.0);
        assert_eq!(alerts.len(), 0);
    }

    #[test]
    fn test_sustained_above() {
        let mut engine = ThresholdEngine::new();
        engine.register(Threshold {
            id: ThresholdId::new("THR-CPU-002"),
            metric_name: MetricName::from_str("lumi.system.cpu.lumi_process_percent"),
            condition: ThresholdCondition::SustainedAbove {
                duration: Duration::from_millis(10),
            },
            value: 30.0,
            window: None,
            severity: AlertSeverity::Critical,
            cooldown: Duration::from_secs(30),
            message_template: "CPU sustained at {value}%",
            recovery_hint: None,
        });

        // First evaluation starts the timer
        let alerts = engine.evaluate("lumi.system.cpu.lumi_process_percent", 50.0);
        assert_eq!(alerts.len(), 0); // Not yet sustained

        // After enough time
        std::thread::sleep(Duration::from_millis(15));
        let alerts = engine.evaluate("lumi.system.cpu.lumi_process_percent", 50.0);
        assert_eq!(alerts.len(), 1);
    }
}
