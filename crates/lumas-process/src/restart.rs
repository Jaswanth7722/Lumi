//! # Restart Engine + Policies
//!
//! Configurable restart policies and the restart engine that applies them.
//!
//! The restart engine determines what action to take when a process crashes:
//! restart with backoff, give up permanently, await manual intervention, or
//! restart in safe mode. All policies operate within a sliding time window
//! to prevent rapid restart loops.
//!
//! # Thread Safety
//!
//! `RestartEngine` is `Send + Sync`. `RestartRecord` requires external
//! `Mutex` synchronization for mutation.
//!
//! # Design
//!
//! Restart policies are configured per-process in the `ProcessDescriptor`.
//! The sliding window counts restarts within `window_secs`. If the window
//! expires, the counter resets.

use crate::error::ProcessError;
use crate::id::ProcessId;
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// RestartPolicy
// ---------------------------------------------------------------------------

/// Configurable restart policy for a managed process.
///
/// Determines how the supervisor responds to a process crash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RestartPolicy {
    /// Never restart. Failure escalates immediately.
    Never,
    /// Restart immediately with no delay.
    Immediate {
        /// Maximum restarts within the sliding window.
        max_restarts: u32,
        /// Sliding window duration in seconds.
        window_secs: u64,
    },
    /// Restart with exponential backoff.
    ExponentialBackoff {
        /// Initial delay in milliseconds (default: 100).
        initial_delay_ms: u64,
        /// Backoff multiplier (default: 2.0).
        multiplier: f32,
        /// Maximum delay in milliseconds (default: 30_000).
        max_delay_ms: u64,
        /// Maximum restarts within the sliding window.
        max_restarts: u32,
        /// Sliding window duration in seconds.
        window_secs: u64,
        /// Jitter percentage (default: 10 = ±10%).
        jitter_percent: u8,
    },
    /// Restart with fixed linear backoff.
    LinearBackoff {
        /// Delay in milliseconds between restarts.
        delay_ms: u64,
        /// Maximum restarts within the sliding window.
        max_restarts: u32,
        /// Sliding window duration in seconds.
        window_secs: u64,
    },
    /// Never restart automatically. Await manual operator command.
    ManualRecovery,
    /// Restart in safe mode: start with minimal capabilities, no plugins.
    SafeMode {
        /// Maximum restarts within the sliding window.
        max_restarts: u32,
    },
}

impl RestartPolicy {
    /// Maximum restarts for this policy, if applicable.
    pub fn max_restarts(&self) -> Option<u32> {
        match self {
            RestartPolicy::Never => Some(0),
            RestartPolicy::Immediate { max_restarts, .. } => Some(*max_restarts),
            RestartPolicy::ExponentialBackoff { max_restarts, .. } => Some(*max_restarts),
            RestartPolicy::LinearBackoff { max_restarts, .. } => Some(*max_restarts),
            RestartPolicy::ManualRecovery => None,
            RestartPolicy::SafeMode { max_restarts, .. } => Some(*max_restarts),
        }
    }
}

// ---------------------------------------------------------------------------
// RestartRecord
// ---------------------------------------------------------------------------

/// Tracks restart history for a single process.
///
/// Used internally by the restart engine to enforce the sliding window.
/// The window resets after `window_secs` of inactivity.
#[derive(Debug, Clone)]
pub struct RestartRecord {
    /// Number of restarts within the current window.
    pub restart_count: u32,
    /// When the current window started.
    pub window_start: Instant,
    /// When the last restart occurred.
    pub last_restart: Option<Instant>,
    /// Exit code from the last failure.
    pub last_exit_code: Option<i32>,
}

impl RestartRecord {
    /// Create a new restart record starting fresh.
    pub fn new() -> Self {
        Self {
            restart_count: 0,
            window_start: Instant::now(),
            last_restart: None,
            last_exit_code: None,
        }
    }

    /// Returns `true` if another restart is permitted under the given max and window.
    pub fn can_restart(&mut self, max: u32, window_secs: u64) -> bool {
        let window_start = Instant::now() - Duration::from_secs(window_secs);
        if self.window_start < window_start {
            // Window has elapsed — reset count.
            self.restart_count = 0;
            self.window_start = Instant::now();
        }
        self.restart_count < max
    }

    /// Record a restart attempt (increment count, update timestamps).
    pub fn record_restart(&mut self, exit_code: Option<i32>) {
        self.restart_count += 1;
        self.last_restart = Some(Instant::now());
        self.last_exit_code = exit_code;
    }

    /// Reset the restart record (e.g., after a successful run).
    pub fn reset(&mut self) {
        self.restart_count = 0;
        self.window_start = Instant::now();
        self.last_restart = None;
        self.last_exit_code = None;
    }
}

impl Default for RestartRecord {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// RestartAction
// ---------------------------------------------------------------------------

/// The action the restart engine has decided to take.
#[derive(Debug, Clone)]
pub enum RestartAction {
    /// Restart after the specified delay.
    RestartAfter {
        /// Duration to wait before restarting.
        delay: Duration,
    },
    /// Max restarts exceeded — escalate to supervisor.
    GivingUp,
    /// Manual recovery policy — await operator command.
    AwaitManual,
    /// Restart in safe mode.
    RestartInSafeMode,
}

// ---------------------------------------------------------------------------
// RestartEngine
// ---------------------------------------------------------------------------

/// Determines and executes restart actions based on policy and history.
///
/// # Thread Safety
///
/// `RestartEngine` is `Send + Sync`. It is designed to be shared via `Arc`.
///
/// # Examples
///
/// ```ignore
/// let engine = RestartEngine::new();
/// let action = engine.next_action(&id, &policy, &mut record);
/// match action {
///     RestartAction::RestartAfter { delay } => { /* wait, then restart */ }
///     RestartAction::GivingUp => { /* escalate */ }
///     _ => {}
/// }
/// ```
pub struct RestartEngine;

impl RestartEngine {
    /// Create a new restart engine.
    pub fn new() -> Self {
        Self
    }

    /// Determine the next restart action given the current record and policy.
    ///
    /// # Parameters
    ///
    /// * `id` — The process ID (for error reporting).
    /// * `policy` — The configured restart policy.
    /// * `record` — The mutable restart history record.
    ///
    /// # Returns
    ///
    /// A `RestartAction` indicating what to do next.
    pub fn next_action(
        &self,
        id: &ProcessId,
        policy: &RestartPolicy,
        record: &mut RestartRecord,
    ) -> RestartAction {
        match *policy {
            RestartPolicy::Never => RestartAction::GivingUp,

            RestartPolicy::Immediate {
                max_restarts,
                window_secs,
            } => {
                if record.can_restart(max_restarts, window_secs) {
                    record.record_restart(record.last_exit_code);
                    RestartAction::RestartAfter {
                        delay: Duration::ZERO,
                    }
                } else {
                    RestartAction::GivingUp
                }
            }

            RestartPolicy::ExponentialBackoff {
                initial_delay_ms,
                multiplier,
                max_delay_ms,
                max_restarts,
                window_secs,
                jitter_percent,
            } => {
                if record.can_restart(max_restarts, window_secs) {
                    let base_delay =
                        initial_delay_ms as f32 * multiplier.powi(record.restart_count as i32);
                    let clamped = base_delay.min(max_delay_ms as f32);
                    let jitter_range = clamped * (jitter_percent as f32 / 100.0);
                    let jitter = rand::thread_rng().gen_range(-jitter_range..jitter_range);
                    let final_delay = (clamped + jitter).max(0.0) as u64;

                    record.record_restart(record.last_exit_code);
                    RestartAction::RestartAfter {
                        delay: Duration::from_millis(final_delay),
                    }
                } else {
                    RestartAction::GivingUp
                }
            }

            RestartPolicy::LinearBackoff {
                delay_ms,
                max_restarts,
                window_secs,
            } => {
                if record.can_restart(max_restarts, window_secs) {
                    record.record_restart(record.last_exit_code);
                    RestartAction::RestartAfter {
                        delay: Duration::from_millis(delay_ms),
                    }
                } else {
                    RestartAction::GivingUp
                }
            }

            RestartPolicy::ManualRecovery => RestartAction::AwaitManual,

            RestartPolicy::SafeMode {
                max_restarts: _,
            } => {
                record.record_restart(record.last_exit_code);
                RestartAction::RestartInSafeMode
            }
        }
    }
}

impl Default for RestartEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_id(name: &str) -> ProcessId {
        ProcessId::new(name)
    }

    #[test]
    fn test_immediate_policy_restarts_without_delay() {
        let engine = RestartEngine::new();
        let mut record = RestartRecord::new();
        let id = make_id("test");

        let action = engine.next_action(
            &id,
            &RestartPolicy::Immediate {
                max_restarts: 3,
                window_secs: 60,
            },
            &mut record,
        );

        match action {
            RestartAction::RestartAfter { delay } => {
                assert_eq!(delay, Duration::ZERO);
            }
            _ => panic!("Expected RestartAfter with zero delay"),
        }

        assert_eq!(record.restart_count, 1);
    }

    #[test]
    fn test_max_restarts_exceeded_transitions_to_giving_up() {
        let engine = RestartEngine::new();
        let mut record = RestartRecord::new();

        // Use a policy with 2 max restarts within a long window.
        let policy = RestartPolicy::Immediate {
            max_restarts: 2,
            window_secs: 3600,
        };
        let id = make_id("test");

        // First restart — should succeed.
        let a1 = engine.next_action(&id, &policy, &mut record);
        assert!(matches!(a1, RestartAction::RestartAfter { .. }));

        // Second restart — should succeed.
        let a2 = engine.next_action(&id, &policy, &mut record);
        assert!(matches!(a2, RestartAction::RestartAfter { .. }));

        // Third restart — should give up.
        let a3 = engine.next_action(&id, &policy, &mut record);
        assert!(matches!(a3, RestartAction::GivingUp));
    }

    #[test]
    fn test_exponential_backoff_increases_delay() {
        let engine = RestartEngine::new();
        let mut record = RestartRecord::new();
        let id = make_id("test");

        let policy = RestartPolicy::ExponentialBackoff {
            initial_delay_ms: 100,
            multiplier: 2.0,
            max_delay_ms: 30_000,
            max_restarts: 5,
            window_secs: 3600,
            jitter_percent: 0, // No jitter for deterministic test
        };

        // First restart delay should be ~100ms
        let a1 = engine.next_action(&id, &policy, &mut record);
        if let RestartAction::RestartAfter { delay } = a1 {
            assert!(delay >= Duration::from_millis(100) && delay <= Duration::from_millis(150));
        } else {
            panic!("Expected RestartAfter");
        }

        // Second restart delay should be ~200ms
        let a2 = engine.next_action(&id, &policy, &mut record);
        if let RestartAction::RestartAfter { delay } = a2 {
            assert!(delay >= Duration::from_millis(200) && delay <= Duration::from_millis(250));
        } else {
            panic!("Expected RestartAfter");
        }

        assert_eq!(record.restart_count, 2);
    }

    #[test]
    fn test_never_policy_gives_up_immediately() {
        let engine = RestartEngine::new();
        let mut record = RestartRecord::new();
        let id = make_id("test");

        let action = engine.next_action(&id, &RestartPolicy::Never, &mut record);
        assert!(matches!(action, RestartAction::GivingUp));
        assert_eq!(record.restart_count, 0); // Never increments
    }

    #[test]
    fn test_manual_recovery_awaits_command() {
        let engine = RestartEngine::new();
        let mut record = RestartRecord::new();
        let id = make_id("test");

        let action = engine.next_action(&id, &RestartPolicy::ManualRecovery, &mut record);
        assert!(matches!(action, RestartAction::AwaitManual));
    }

    #[test]
    fn test_restart_window_resets_after_expiry() {
        // Use a very short window so it expires immediately.
        let engine = RestartEngine::new();
        let mut record = RestartRecord::new();
        record.window_start = Instant::now() - Duration::from_secs(1);
        record.restart_count = 5; // Pretend we had 5 restarts in the old window.

        let id = make_id("test");
        let policy = RestartPolicy::Immediate {
            max_restarts: 3,
            window_secs: 0, // Window is already past
        };

        // Window expired, so restart count should reset and allow restart.
        let action = engine.next_action(&id, &policy, &mut record);
        assert!(matches!(action, RestartAction::RestartAfter { .. }));
        assert_eq!(record.restart_count, 1); // Reset to 1 after recording
    }

    #[test]
    fn test_jitter_is_applied() {
        let engine = RestartEngine::new();
        let mut record = RestartRecord::new();
        let id = make_id("test");

        let policy = RestartPolicy::ExponentialBackoff {
            initial_delay_ms: 1000,
            multiplier: 1.0,
            max_delay_ms: 2000,
            max_restarts: 5,
            window_secs: 3600,
            jitter_percent: 50, // ±50% jitter for high variance
        };

        let action = engine.next_action(&id, &policy, &mut record);
        match action {
            RestartAction::RestartAfter { delay } => {
                // With 50% jitter on 1000ms, delay should be 500-1500ms
                let ms = delay.as_millis() as u64;
                assert!(
                    ms >= 500 && ms <= 1500,
                    "Jittered delay {ms}ms out of expected range 500-1500ms"
                );
            }
            _ => panic!("Expected RestartAfter"),
        }
    }

    #[test]
    fn test_linear_backoff() {
        let engine = RestartEngine::new();
        let mut record = RestartRecord::new();
        let id = make_id("test");

        let policy = RestartPolicy::LinearBackoff {
            delay_ms: 500,
            max_restarts: 3,
            window_secs: 60,
        };

        let action = engine.next_action(&id, &policy, &mut record);
        match action {
            RestartAction::RestartAfter { delay } => {
                assert_eq!(delay, Duration::from_millis(500));
            }
            _ => panic!("Expected RestartAfter"),
        }
    }
}
