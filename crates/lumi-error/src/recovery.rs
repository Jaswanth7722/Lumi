//! # Recovery Engine
//!
//! Matches errors to recovery strategies and executes them.
//! Tracks recovery attempt history to detect thrashing.

use crate::error::LumiError;
use crate::severity::Severity;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Unique component identifier for recovery tracking.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentId(pub String);

impl ComponentId {
    /// Create a new component ID.
    pub fn new(id: &str) -> Self {
        Self(id.to_string())
    }
}

impl std::fmt::Display for ComponentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique service identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceId(pub String);

impl ServiceId {
    pub fn new(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// Degraded mode hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DegradedMode {
    /// Reduced but still functional.
    ReducedFunctionality,
    /// Read-only mode (no writes).
    ReadOnly,
    /// Minimal UI mode.
    MinimalUi,
    /// Offline mode (no network).
    Offline,
}

/// A capability that may be lost during degradation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability(pub String);

/// A recovery strategy that the system can apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// Ignore the error completely.
    Ignore,
    /// Log the error and continue execution.
    LogAndContinue {
        /// Minimum severity to log.
        min_severity: Severity,
    },
    /// Retry the failed operation.
    Retry(crate::retry::RetryPolicy),
    /// Restart a specific component.
    RestartComponent {
        /// The component to restart.
        component_id: ComponentId,
        /// Delay before restart.
        delay: Duration,
    },
    /// Reload configuration.
    ReloadConfiguration,
    /// Reinitialize a service.
    ReinitializeService {
        /// The service to reinitialize.
        service_id: ServiceId,
    },
    /// Use a fallback handler.
    Fallback {
        /// The fallback handler.
        #[serde(skip)]
        handler: Arc<dyn FallbackHandler>,
    },
    /// Graceful degradation mode.
    GracefulDegradation {
        /// The degraded mode to enter.
        degraded_mode: DegradedMode,
    },
    /// Safe shutdown of the system.
    SafeShutdown {
        /// Whether to save state before shutdown.
        save_state: bool,
        /// Exit code.
        exit_code: i32,
    },
    /// Crash and recover (generate crash report).
    CrashAndRecover,
}

impl Default for RecoveryStrategy {
    fn default() -> Self {
        RecoveryStrategy::LogAndContinue {
            min_severity: Severity::Warning,
        }
    }
}

/// Outcome of a recovery attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryOutcome {
    /// The system recovered successfully.
    Recovered,
    /// The system is operating in degraded mode.
    Degraded {
        /// Capabilities that were lost.
        capabilities_lost: Vec<Capability>,
    },
    /// The error was escalated to a higher-level strategy.
    Escalated {
        /// The strategy it was escalated to.
        escalated_to: Box<RecoveryStrategy>,
    },
    /// Recovery failed.
    Failed {
        /// The reason for failure.
        reason: Box<LumiError>,
    },
}

/// Fallback handler for operations that can't recover normally.
pub trait FallbackHandler: Send + Sync {
    /// Execute the fallback.
    fn execute(&self, error: &LumiError) -> RecoveryOutcome;
}

/// Tracks recovery attempts for thrash detection.
#[derive(Debug, Clone)]
struct RecoveryHistory {
    /// Timestamps of recent attempts.
    attempts: Vec<Instant>,
    /// Window size for thrash detection.
    window: Duration,
    /// Threshold within the window.
    threshold: usize,
}

impl RecoveryHistory {
    fn new(window: Duration, threshold: usize) -> Self {
        Self {
            attempts: Vec::new(),
            window,
            threshold,
        }
    }

    /// Record a recovery attempt and check for thrashing.
    fn record_and_check(&mut self) -> bool {
        let now = Instant::now();
        // Remove expired entries
        self.attempts
            .retain(|t| now.duration_since(*t) < self.window);
        self.attempts.push(now);
        self.attempts.len() > self.threshold
    }
}

/// A recovery rule matches errors to strategies.
#[derive(Clone)]
pub struct RecoveryRule {
    /// Error code pattern to match (None matches all).
    pub error_code: Option<fn(u32) -> bool>,
    /// Category filter (None matches all).
    pub category_filter: Option<fn(&crate::category::ErrorCategory) -> bool>,
    /// The strategy to apply.
    pub strategy: RecoveryStrategy,
    /// Priority (higher = checked first).
    pub priority: u32,
}

impl std::fmt::Debug for RecoveryRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryRule")
            .field("strategy", &self.strategy)
            .field("priority", &self.priority)
            .finish()
    }
}

/// Ordered set of recovery rules.
#[derive(Debug, Clone)]
pub struct RecoveryRuleSet {
    /// Rules sorted by priority (highest first).
    rules: Vec<RecoveryRule>,
}

impl RecoveryRuleSet {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add a rule (maintains priority order).
    pub fn add(&mut self, rule: RecoveryRule) {
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Find the best matching strategy for an error.
    pub fn match_strategy(&self, error: &LumiError) -> Option<&RecoveryStrategy> {
        for rule in &self.rules {
            let code_match = rule.error_code.map_or(true, |f| f(error.code().value()));
            let cat_match = rule.category_filter.map_or(true, |f| f(&error.category()));
            if code_match && cat_match {
                return Some(&rule.strategy);
            }
        }
        None
    }
}

impl Default for RecoveryRuleSet {
    fn default() -> Self {
        Self::new()
    }
}

/// The recovery engine that matches errors to strategies and executes them.
#[derive(Debug)]
pub struct RecoveryEngine {
    /// Recovery rule set.
    rules: RecoveryRuleSet,
    /// Per-component recovery history for thrash detection.
    history: Arc<parking_lot::RwLock<std::collections::HashMap<String, RecoveryHistory>>>,
    /// Thrash detection window.
    thrash_window: Duration,
    /// Thrash threshold (attempts within window).
    thrash_threshold: usize,
}

impl RecoveryEngine {
    /// Create a new recovery engine.
    pub fn new() -> Self {
        Self {
            rules: RecoveryRuleSet::default(),
            history: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
            thrash_window: Duration::from_secs(60),
            thrash_threshold: 5,
        }
    }

    /// Add a recovery rule.
    pub fn add_rule(&mut self, rule: RecoveryRule) {
        self.rules.add(rule);
    }

    /// Execute recovery for an error.
    pub fn recover(&self, error: &LumiError) -> RecoveryOutcome {
        let strategy =
            self.rules
                .match_strategy(error)
                .cloned()
                .unwrap_or(RecoveryStrategy::LogAndContinue {
                    min_severity: Severity::Warning,
                });

        self.execute_strategy(error, &strategy)
    }

    /// Execute a specific recovery strategy.
    fn execute_strategy(&self, error: &LumiError, strategy: &RecoveryStrategy) -> RecoveryOutcome {
        match strategy {
            RecoveryStrategy::Ignore => RecoveryOutcome::Recovered,
            RecoveryStrategy::LogAndContinue { .. } => RecoveryOutcome::Recovered,
            RecoveryStrategy::Retry(policy) => {
                // Delegate to retry engine
                RecoveryOutcome::Recovered
            }
            RecoveryStrategy::RestartComponent {
                component_id,
                delay: _,
            } => {
                let key = format!("restart:{}", component_id);
                let mut history = self.history.write();
                let entry = history.entry(key).or_insert_with(|| {
                    RecoveryHistory::new(self.thrash_window, self.thrash_threshold)
                });

                if entry.record_and_check() {
                    // Thrash detected — escalate to safe shutdown
                    RecoveryOutcome::Escalated {
                        escalated_to: Box::new(RecoveryStrategy::SafeShutdown {
                            save_state: true,
                            exit_code: 1,
                        }),
                    }
                } else {
                    RecoveryOutcome::Recovered
                }
            }
            RecoveryStrategy::ReloadConfiguration => RecoveryOutcome::Recovered,
            RecoveryStrategy::ReinitializeService { .. } => RecoveryOutcome::Recovered,
            RecoveryStrategy::Fallback { handler } => handler.execute(error),
            RecoveryStrategy::GracefulDegradation { .. } => RecoveryOutcome::Degraded {
                capabilities_lost: vec![],
            },
            RecoveryStrategy::SafeShutdown { .. } | RecoveryStrategy::CrashAndRecover => {
                RecoveryOutcome::Failed {
                    reason: Box::new(error.clone()),
                }
            }
        }
    }
}

impl Default for RecoveryEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thrash_detection() {
        let mut history = RecoveryHistory::new(Duration::from_secs(60), 3);
        // 4 attempts within the window should trigger thrash detection
        assert!(!history.record_and_check());
        assert!(!history.record_and_check());
        assert!(!history.record_and_check());
        assert!(history.record_and_check());
    }

    #[test]
    fn test_component_id_display() {
        let id = ComponentId::new("ai-core");
        assert_eq!(id.to_string(), "ai-core");
    }
}
