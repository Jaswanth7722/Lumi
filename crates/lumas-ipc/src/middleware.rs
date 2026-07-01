//! # Middleware Pipeline
//!
//! Middleware provides a plugin-style pipeline for processing messages
//! before they are routed (inbound) and after they are sent (outbound).
//!
//! Pipeline order:
//! 1. TracingMiddleware  (order: -20) — Span management
//! 2. MetricsMiddleware  (order: -10) — Latency/duration recording
//! 3. LoggingMiddleware  (order:   0) — Message logging
//! 4. ValidationMiddleware (order: 10) — Schema validation
//! 5. RateLimitMiddleware  (order: 20) — Rate check

use crate::error::MiddlewareError;
use crate::message::LumiMessage;
use async_trait::async_trait;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// The Next type in the middleware chain.
pub struct Next<'a> {
    remaining: &'a [Arc<dyn Middleware>],
    handler: Arc<dyn Fn(LumiMessage) -> Pin<Box<dyn Future<Output = Result<LumiMessage, MiddlewareError>> + Send>> + Send + Sync>,
}

impl<'a> Next<'a> {
    pub fn new(
        remaining: &'a [Arc<dyn Middleware>],
    ) -> Self {
        let remaining_len = remaining.len();
        Self {
            remaining,
            handler: Arc::new(move |msg: LumiMessage| {
                Box::pin(async move { Ok(msg) })
            }),
        }
    }

    /// Run the next middleware in the chain.
    pub async fn run(self, msg: LumiMessage) -> Result<LumiMessage, MiddlewareError> {
        if let Some((first, rest)) = self.remaining.split_first() {
            let next = Next {
                remaining: rest,
                handler: self.handler.clone(),
            };
            first.process_inbound(msg, next).await
        } else {
            (self.handler)(msg).await
        }
    }

    /// Run the next middleware in the outbound direction.
    pub async fn run_outbound(self, msg: LumiMessage) -> Result<LumiMessage, MiddlewareError> {
        if let Some((first, rest)) = self.remaining.split_first() {
            let next = Next {
                remaining: rest,
                handler: self.handler.clone(),
            };
            first.process_outbound(msg, next).await
        } else {
            (self.handler)(msg).await
        }
    }
}

/// Middleware trait for processing messages in the pipeline.
#[async_trait]
pub trait Middleware: Send + Sync + fmt::Debug + 'static {
    /// Middleware name for diagnostics.
    fn name(&self) -> &'static str;

    /// Pipeline order. Lower = earlier in the pipeline.
    /// Negative values execute before authentication.
    fn order(&self) -> i32;

    /// Process an inbound message before routing.
    async fn process_inbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError>;

    /// Process an outbound message after routing.
    async fn process_outbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError>;
}

// ---------------------------------------------------------------------------
// Built-in Middleware Implementations
// ---------------------------------------------------------------------------

/// Tracing middleware — extracts/injects trace context.
#[derive(Debug)]
pub struct TracingMiddleware;

#[async_trait]
impl Middleware for TracingMiddleware {
    fn name(&self) -> &'static str {
        "tracing"
    }

    fn order(&self) -> i32 {
        -20
    }

    async fn process_inbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError> {
        // Extract trace context from message metadata
        // In production: create/restore OpenTelemetry span
        let result = next.run(msg).await?;
        Ok(result)
    }

    async fn process_outbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError> {
        // Inject trace context into message metadata
        let result = next.run_outbound(msg).await?;
        Ok(result)
    }
}

/// Metrics middleware — records send/receive latency.
#[derive(Debug)]
pub struct MetricsMiddleware;

#[async_trait]
impl Middleware for MetricsMiddleware {
    fn name(&self) -> &'static str {
        "metrics"
    }

    fn order(&self) -> i32 {
        -10
    }

    async fn process_inbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError> {
        let start = std::time::Instant::now();
        let result = next.run(msg).await?;
        let latency_us = start.elapsed().as_micros() as u64;
        tracing::debug!("Middleware processing took {}µs", latency_us);
        Ok(result)
    }

    async fn process_outbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError> {
        let start = std::time::Instant::now();
        let result = next.run_outbound(msg).await?;
        let latency_us = start.elapsed().as_micros() as u64;
        tracing::debug!("Outbound processing took {}µs", latency_us);
        Ok(result)
    }
}

/// Logging middleware — logs all messages passing through.
#[derive(Debug)]
pub struct LoggingMiddleware;

#[async_trait]
impl Middleware for LoggingMiddleware {
    fn name(&self) -> &'static str {
        "logging"
    }

    fn order(&self) -> i32 {
        0
    }

    async fn process_inbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError> {
        tracing::debug!(
            "[IPC] Inbound: {} -> {} on {} (kind: {:?})",
            msg.sender,
            msg.channel.0,
            msg.channel.0,
            msg.kind,
        );
        next.run(msg).await
    }

    async fn process_outbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError> {
        tracing::debug!(
            "[IPC] Outbound: {} -> {} on {}",
            msg.sender,
            msg.channel.0,
            msg.channel.0,
        );
        next.run_outbound(msg).await
    }
}

/// Validation middleware — validates message schema.
#[derive(Debug)]
pub struct ValidationMiddleware;

#[async_trait]
impl Middleware for ValidationMiddleware {
    fn name(&self) -> &'static str {
        "validation"
    }

    fn order(&self) -> i32 {
        10
    }

    async fn process_inbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError> {
        // Basic validation
        if msg.id.0.is_empty() {
            return Err(MiddlewareError::Rejected {
                name: "validation",
                reason: "Message has empty ID".into(),
            });
        }
        next.run(msg).await
    }

    async fn process_outbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError> {
        next.run_outbound(msg).await
    }
}

/// Rate limiting middleware — checks sender rate.
#[derive(Debug)]
pub struct RateLimitMiddleware {
    max_per_second: f64,
}

impl RateLimitMiddleware {
    pub fn new(max_per_second: f64) -> Self {
        Self { max_per_second }
    }
}

#[async_trait]
impl Middleware for RateLimitMiddleware {
    fn name(&self) -> &'static str {
        "rate-limit"
    }

    fn order(&self) -> i32 {
        20
    }

    async fn process_inbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError> {
        // Rate limiting would be implemented here using a token bucket
        // For now, pass through
        next.run(msg).await
    }

    async fn process_outbound(
        &self,
        msg: LumiMessage,
        next: Next<'_>,
    ) -> Result<LumiMessage, MiddlewareError> {
        next.run_outbound(msg).await
    }
}

/// Middleware pipeline — ordered collection of middleware.
pub struct MiddlewarePipeline {
    middleware: Vec<Arc<dyn Middleware>>,
}

impl MiddlewarePipeline {
    /// Create a new middleware pipeline with built-in middleware.
    pub fn with_defaults() -> Self {
        let mut pipeline = Self::new();
        pipeline.add(Arc::new(TracingMiddleware));
        pipeline.add(Arc::new(MetricsMiddleware));
        pipeline.add(Arc::new(LoggingMiddleware));
        pipeline.add(Arc::new(ValidationMiddleware));
        pipeline.add(Arc::new(RateLimitMiddleware::new(1000.0)));
        pipeline.sort();
        pipeline
    }

    /// Create a new empty pipeline.
    pub fn new() -> Self {
        Self {
            middleware: Vec::new(),
        }
    }

    /// Add a middleware to the pipeline.
    pub fn add(&mut self, m: Arc<dyn Middleware>) {
        self.middleware.push(m);
    }

    /// Sort middleware by order.
    pub fn sort(&mut self) {
        self.middleware.sort_by_key(|m| m.order());
    }

    /// Process a message through the inbound pipeline.
    pub async fn process_inbound(
        &self,
        msg: LumiMessage,
    ) -> Result<LumiMessage, MiddlewareError> {
        let next = Next::new(&self.middleware);
        next.run(msg).await
    }

    /// Process a message through the outbound pipeline.
    pub async fn process_outbound(
        &self,
        msg: LumiMessage,
    ) -> Result<LumiMessage, MiddlewareError> {
        let mut reversed: Vec<_> = self.middleware.iter().cloned().rev().collect();
        let next = Next::new(&reversed);
        next.run_outbound(msg).await
    }

    /// Get the number of middleware in the pipeline.
    pub fn len(&self) -> usize {
        self.middleware.len()
    }

    /// Check if the pipeline is empty.
    pub fn is_empty(&self) -> bool {
        self.middleware.is_empty()
    }
}

impl Default for MiddlewarePipeline {
    fn default() -> Self {
        Self::with_defaults()
    }
}
