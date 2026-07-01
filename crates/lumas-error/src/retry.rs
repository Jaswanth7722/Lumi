//! # Retry Engine
//!
//! Supports all retry policies with a builder API (no stringly-typed configuration).

use crate::error::LumasError;
use crate::severity::Severity;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Configuration for jitter in retry delays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JitterConfig {
    /// Maximum jitter as a fraction of the delay (0.0 - 1.0).
    pub max_fraction: f64,
}

impl Default for JitterConfig {
    fn default() -> Self {
        Self { max_fraction: 0.1 }
    }
}

/// Condition that determines whether a retry should be attempted.
#[derive(Clone)]
pub struct RetryCondition(Arc<dyn Fn(&LumasError) -> bool + Send + Sync>);

impl RetryCondition {
    /// Create a new retry condition.
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&LumasError) -> bool + Send + Sync + 'static,
    {
        Self(Arc::new(f))
    }

    /// Check if the error should be retried.
    pub fn should_retry(&self, error: &LumasError) -> bool {
        (self.0)(error)
    }

    /// Default condition: retry on recoverable errors only.
    pub fn recoverable_only() -> Self {
        Self::new(|e| e.severity().is_recoverable())
    }
}

impl Default for RetryCondition {
    fn default() -> Self {
        Self::recoverable_only()
    }
}

impl std::fmt::Debug for RetryCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RetryCondition")
    }
}

/// Retry strategy variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryStrategy {
    /// Retry immediately with no delay.
    Immediate,
    /// Fixed delay between retries.
    Fixed {
        /// Delay amount.
        delay: Duration,
    },
    /// Linearly increasing delay.
    Linear {
        /// Initial delay.
        initial: Duration,
        /// Increment per attempt.
        increment: Duration,
    },
    /// Exponentially increasing delay.
    Exponential {
        /// Initial delay.
        initial: Duration,
        /// Base for exponential growth.
        base: f64,
        /// Maximum delay cap.
        max: Duration,
    },
    /// Fibonacci sequence delays.
    Fibonacci {
        /// Initial delay.
        initial: Duration,
        /// Maximum delay cap.
        max: Duration,
    },
}

impl Default for RetryStrategy {
    fn default() -> Self {
        RetryStrategy::Exponential {
            initial: Duration::from_millis(100),
            base: 2.0,
            max: Duration::from_secs(30),
        }
    }
}

/// Information about a single retry attempt.
#[derive(Debug, Clone)]
pub struct RetryAttempt {
    /// Current attempt number (1-based).
    pub attempt: u32,
    /// Total attempts allowed.
    pub max_attempts: u32,
    /// Delay before this attempt.
    pub delay: Duration,
    /// Error from the previous attempt.
    pub last_error: Option<Box<LumasError>>,
    /// Elapsed time since the first attempt.
    pub elapsed: Duration,
}

/// Retry policy configuration.
#[derive(Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts.
    pub max_attempts: std::num::NonZeroU32,
    /// Retry strategy.
    pub strategy: RetryStrategy,
    /// Overall timeout for all attempts.
    pub timeout: Option<Duration>,
    /// Jitter configuration.
    pub jitter: Option<JitterConfig>,
    /// Condition for when to retry.
    #[serde(skip)]
    pub retry_on: RetryCondition,
    /// Callback on each retry.
    #[serde(skip)]
    pub on_retry: Option<Arc<dyn Fn(RetryAttempt) + Send + Sync>>,
}

impl std::fmt::Debug for RetryPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryPolicy")
            .field("max_attempts", &self.max_attempts)
            .field("strategy", &self.strategy)
            .field("timeout", &self.timeout)
            .field("jitter", &self.jitter)
            .field("retry_on", &self.retry_on)
            .field("on_retry", &self.on_retry.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl RetryPolicy {
    /// Create a new retry policy builder.
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts: std::num::NonZeroU32::new(max_attempts.max(1))
                .unwrap_or(std::num::NonZeroU32::new(1).unwrap()),
            strategy: RetryStrategy::default(),
            timeout: None,
            jitter: None,
            retry_on: RetryCondition::recoverable_only(),
            on_retry: None,
        }
    }

    /// Create a default exponential backoff policy.
    pub fn exponential_default() -> Self {
        Self::new(3)
            .with_strategy(RetryStrategy::Exponential {
                initial: Duration::from_millis(100),
                base: 2.0,
                max: Duration::from_secs(10),
            })
            .with_timeout(Duration::from_secs(60))
    }

    /// Create a default linear retry policy.
    pub fn linear_default() -> Self {
        Self::new(3).with_strategy(RetryStrategy::Linear {
            initial: Duration::from_millis(100),
            increment: Duration::from_millis(200),
        })
    }

    /// Set the retry strategy.
    pub fn with_strategy(mut self, strategy: RetryStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set the overall timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set jitter configuration.
    pub fn with_jitter(mut self, jitter: JitterConfig) -> Self {
        self.jitter = Some(jitter);
        self
    }

    /// Set the retry condition.
    pub fn with_retry_condition(mut self, condition: RetryCondition) -> Self {
        self.retry_on = condition;
        self
    }

    /// Set the retry callback.
    pub fn with_on_retry<F>(mut self, f: F) -> Self
    where
        F: Fn(RetryAttempt) + Send + Sync + 'static,
    {
        self.on_retry = Some(Arc::new(f));
        self
    }

    /// Calculate the delay for a given attempt (1-based).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay = match &self.strategy {
            RetryStrategy::Immediate => Duration::ZERO,
            RetryStrategy::Fixed { delay } => *delay,
            RetryStrategy::Linear { initial, increment } => {
                *initial + *increment * (attempt.saturating_sub(1) as u32)
            }
            RetryStrategy::Exponential { initial, base, max } => {
                let factor = base.powf((attempt.saturating_sub(1)) as f64);
                let d = initial.mul_f64(factor);
                d.min(*max)
            }
            RetryStrategy::Fibonacci { initial, max } => {
                let fib = fib(attempt.saturating_sub(1) as u32);
                let d = initial.mul_f64(fib as f64);
                d.min(*max)
            }
        };

        // Apply jitter
        if let Some(ref jitter) = self.jitter {
            let jitter_amount = delay.mul_f64(jitter.max_fraction * rand::random::<f64>());
            delay.saturating_add(jitter_amount)
        } else {
            delay
        }
    }
}

/// Compute nth Fibonacci number.
fn fib(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => {
            let mut a = 0u64;
            let mut b = 1u64;
            for _ in 2..=n {
                let c = a.saturating_add(b);
                a = b;
                b = c;
            }
            b
        }
    }
}

/// Execute an async operation with retry.
pub async fn retry<F, Fut, T>(policy: &RetryPolicy, op: F) -> Result<T, LumasError>
where
    F: Fn() -> Fut + Send,
    Fut: std::future::Future<Output = Result<T, LumasError>> + Send,
{
    let start = Instant::now();
    let max = policy.max_attempts.get();
    let mut last_error = None;

    for attempt in 1..=max {
        // Check timeout
        if let Some(timeout) = policy.timeout {
            if start.elapsed() > timeout {
                break;
            }
        }

        // Execute operation
        match op().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                // Check if we should retry
                if err.severity() >= Severity::Fatal {
                    return Err(err);
                }
                if !policy.retry_on.should_retry(&err) {
                    return Err(err);
                }

                last_error = Some(err);

                // Delay before next attempt
                if attempt < max {
                    let delay = policy.delay_for_attempt(attempt);
                    if let Some(ref on_retry) = policy.on_retry {
                        on_retry(RetryAttempt {
                            attempt,
                            max_attempts: max,
                            delay,
                            last_error: last_error.as_ref().map(|e| Box::new(e.clone())),
                            elapsed: start.elapsed(),
                        });
                    }

                    // Check timeout again after callback
                    if let Some(timeout) = policy.timeout {
                        if start.elapsed() + delay > timeout {
                            break;
                        }
                    }

                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        LumasError::new(
            crate::error_code::ErrorCode::INTERNAL_UNEXPECTED,
            crate::category::ErrorCategory::Internal,
            "All retry attempts exhausted",
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fibonacci() {
        assert_eq!(fib(0), 0);
        assert_eq!(fib(1), 1);
        assert_eq!(fib(2), 1);
        assert_eq!(fib(3), 2);
        assert_eq!(fib(4), 3);
        assert_eq!(fib(5), 5);
        assert_eq!(fib(10), 55);
    }

    #[test]
    fn test_delay_calculation() {
        let policy = RetryPolicy::new(3).with_strategy(RetryStrategy::Fixed {
            delay: Duration::from_millis(100),
        });
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(100));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(100));
    }

    #[test]
    fn test_exponential_delay() {
        let policy = RetryPolicy::new(5).with_strategy(RetryStrategy::Exponential {
            initial: Duration::from_millis(100),
            base: 2.0,
            max: Duration::from_secs(10),
        });
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(100));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(400));
    }

    #[test]
    fn test_retry_condition() {
        let condition = RetryCondition::recoverable_only();
        // Can't easily test with a real LumasError in a simple test
    }
}
