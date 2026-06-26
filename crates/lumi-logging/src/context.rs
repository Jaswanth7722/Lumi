//! # Log Context
//!
//! Correlation identifiers that travel with every log record produced
//! within a logical operation (conversation turn, tool execution, plan step).
//!
//! Stored in a tokio task-local variable so it propagates automatically
//! across .await points and spawned subtasks without explicit threading.

use serde::Serialize;
use uuid::Uuid;

/// Correlation identifiers that travel with every log record.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LogContext {
    /// Top-level request/operation ID.
    pub correlation_id: Option<Uuid>,
    /// User session ID.
    pub session_id: Option<Uuid>,
    /// Active conversation turn ID.
    pub conversation_id: Option<Uuid>,
    /// Scheduler task ID.
    pub task_id: Option<Uuid>,
    /// Active plan ID.
    pub plan_id: Option<Uuid>,
    /// Active plugin ID (if inside plugin call).
    pub plugin_id: Option<String>,
    /// Active workspace panel ID.
    pub workspace_id: Option<Uuid>,
    /// Subsystem name (e.g., "ai_core", "voice", "memory").
    pub subsystem: Option<String>,
}

impl LogContext {
    /// Create a new builder for constructing a context fluently.
    pub fn builder() -> LogContextBuilder {
        LogContextBuilder::new()
    }

    /// Execute a future with this context active for the duration.
    /// Context is automatically available to all log records emitted
    /// within the future, including across .await points.
    pub async fn scope<F, T>(self, f: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        CURRENT_CONTEXT.scope(self, f).await
    }

    /// Returns the current context if one is active, or Default otherwise.
    pub fn current() -> Self {
        CURRENT_CONTEXT
            .try_with(|ctx| ctx.clone())
            .unwrap_or_default()
    }

    /// Merge another context into this one (non-None fields override).
    pub fn merge(&mut self, other: &LogContext) {
        if other.correlation_id.is_some() {
            self.correlation_id = other.correlation_id;
        }
        if other.session_id.is_some() {
            self.session_id = other.session_id;
        }
        if other.conversation_id.is_some() {
            self.conversation_id = other.conversation_id;
        }
        if other.task_id.is_some() {
            self.task_id = other.task_id;
        }
        if other.plan_id.is_some() {
            self.plan_id = other.plan_id;
        }
        if other.plugin_id.is_some() {
            self.plugin_id = other.plugin_id.clone();
        }
        if other.workspace_id.is_some() {
            self.workspace_id = other.workspace_id;
        }
        if other.subsystem.is_some() {
            self.subsystem = other.subsystem.clone();
        }
    }
}

// Task-local context storage — zero-cost when not set
tokio::task_local! {
    static CURRENT_CONTEXT: LogContext;
}

/// Builder for constructing a LogContext fluently.
#[derive(Debug, Default)]
pub struct LogContextBuilder {
    context: LogContext,
}

impl LogContextBuilder {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the LogContext.
    pub fn build(self) -> LogContext {
        self.context
    }

    /// Set the correlation ID.
    pub fn with_correlation_id(mut self, id: Uuid) -> Self {
        self.context.correlation_id = Some(id);
        self
    }

    /// Set the session ID.
    pub fn with_session_id(mut self, id: Uuid) -> Self {
        self.context.session_id = Some(id);
        self
    }

    /// Set the conversation ID.
    pub fn with_conversation_id(mut self, id: Uuid) -> Self {
        self.context.conversation_id = Some(id);
        self
    }

    /// Set the task ID.
    pub fn with_task_id(mut self, id: Uuid) -> Self {
        self.context.task_id = Some(id);
        self
    }

    /// Set the plan ID.
    pub fn with_plan_id(mut self, id: Uuid) -> Self {
        self.context.plan_id = Some(id);
        self
    }

    /// Set the plugin ID.
    pub fn with_plugin_id(mut self, id: String) -> Self {
        self.context.plugin_id = Some(id);
        self
    }

    /// Set the workspace ID.
    pub fn with_workspace_id(mut self, id: Uuid) -> Self {
        self.context.workspace_id = Some(id);
        self
    }

    /// Set the subsystem name.
    pub fn with_subsystem(mut self, subsystem: &str) -> Self {
        self.context.subsystem = Some(subsystem.to_string());
        self
    }
}

/// Convenience macro for wrapping an async block with a log context.
///
/// # Example
///
/// ```ignore
/// use lumi_logging::with_log_context;
/// let ctx = LogContext::builder().with_subsystem("ai_core").build();
/// with_log_context!(ctx, {
///     // All logs inside this block carry the context
///     tracing::info!("Starting inference");
/// });
/// ```
#[macro_export]
macro_rules! with_log_context {
    ($ctx:expr, $future:expr) => {
        $ctx.scope(async { $future }).await
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_builder() {
        let ctx = LogContext::builder()
            .with_correlation_id(Uuid::new_v4())
            .with_subsystem("test")
            .build();

        assert!(ctx.correlation_id.is_some());
        assert_eq!(ctx.subsystem, Some("test".into()));
    }

    #[tokio::test]
    async fn test_context_scope_propagation() {
        let id = Uuid::new_v4();
        let ctx = LogContext::builder().with_correlation_id(id).build();

        let result = ctx
            .scope(async {
                let current = LogContext::current();
                current.correlation_id
            })
            .await;

        assert_eq!(result, Some(id));
    }

    #[tokio::test]
    async fn test_context_current_defaults_when_not_set() {
        let ctx = LogContext::current();
        assert!(ctx.correlation_id.is_none());
    }
}
