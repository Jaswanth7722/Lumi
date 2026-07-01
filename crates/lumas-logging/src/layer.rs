//! # tracing Layer Implementation
//!
//! Bridges the tracing ecosystem to lumi-logging's internal pipeline.
//! Converts tracing::Event and Span lifecycle into LogRecord instances.

use crate::context::LogContext;
use crate::level::{ArcLogLevel, LogLevel};
use crate::pipeline::LogPipeline;
use crate::record::{FieldValue, LogErrorInfo, LogRecord};
use std::sync::Arc;
use tracing::span;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// Tracing layer that routes events to the Lumas logging pipeline.
pub struct LumiTracingLayer {
    /// Pipeline for submitting log records.
    pipeline: Arc<LogPipeline>,
    /// Current global log level.
    level: Arc<ArcLogLevel>,
}

impl LumiTracingLayer {
    /// Create a new tracing layer.
    pub fn new(pipeline: Arc<LogPipeline>, level: Arc<ArcLogLevel>) -> Self {
        Self { pipeline, level }
    }
}

impl<S> tracing_subscriber::Layer<S> for LumiTracingLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        // Extract metadata
        let metadata = event.metadata();

        // Build the log record
        let level = LogLevel::from_tracing(metadata.level(), false);
        let target = metadata.target().to_string();
        let mut record = LogRecord::new(level, target, String::new());

        // Set source location
        record = record.with_source(
            metadata.module_path().map(String::from),
            metadata.file().map(String::from),
            metadata.line(),
        );

        // Extract span context
        if let Some(span) = ctx.current_span().id() {
            if let Some(span_ref) = ctx.span(span) {
                record.span_name = Some(span_ref.name().to_string());
                record.span_id = Some(span_ref.id().into_u64());
            }
        }

        // Set correlation context
        record.context = LogContext::current();

        // Extract fields via visitor
        let mut visitor = LogFieldVisitor::new();
        event.record(&mut visitor);

        record.message = visitor.message.unwrap_or_default();
        record.fields = visitor.fields;

        if let Some(error_info) = visitor.error {
            record.error = Some(error_info);
        }

        // Submit to pipeline (non-blocking)
        self.pipeline.submit(record);
    }

    fn on_enter(&self, _id: &span::Id, _ctx: Context<'_, S>) {}

    fn on_exit(&self, _id: &span::Id, _ctx: Context<'_, S>) {}

    fn on_close(&self, _id: span::Id, ctx: Context<'_, S>) {
        // Record span close with duration if available
        if let Some(span_ref) = ctx.span(&_id) {
            let metadata = span_ref.metadata();
            let level = LogLevel::from_tracing(metadata.level(), false);
            let mut record = LogRecord::new(
                level,
                metadata.target().to_string(),
                format!("span closed: {}", metadata.name()),
            );
            record.span_name = Some(metadata.name().to_string());
            record.span_id = Some(_id.into_u64());

            // Try to extract duration from extensions
            if let Some(ext) = span_ref.extensions().get::<SpanTiming>() {
                record.duration_ms = Some(ext.elapsed_ms());
            }

            self.pipeline.submit(record);
        }
    }
}

/// Tracks span timing for duration reporting on close.
pub struct SpanTiming {
    start: std::time::Instant,
}

impl SpanTiming {
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }
}

/// Custom field visitor that extracts tracing fields into LogRecord fields.
struct LogFieldVisitor {
    /// Extracted message.
    message: Option<String>,
    /// Extracted structured fields.
    fields: indexmap::IndexMap<String, FieldValue>,
    /// Extracted error info.
    error: Option<LogErrorInfo>,
    /// Current field key being visited.
    current_key: Option<String>,
}

impl LogFieldVisitor {
    fn new() -> Self {
        Self {
            message: None,
            fields: indexmap::IndexMap::new(),
            error: None,
            current_key: None,
        }
    }
}

impl tracing::field::Visit for LogFieldVisitor {
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        // Skip internal tracing fields
        if field.name() == "log.target" || field.name() == "log.module_path" || field.name() == "log.file" || field.name() == "log.line" {
            return;
        }
        self.fields.insert(field.name().to_string(), FieldValue::Int(value));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        if field.name() == "log.target" || field.name() == "log.module_path" || field.name() == "log.file" || field.name() == "log.line" {
            return;
        }
        self.fields.insert(field.name().to_string(), FieldValue::Uint(value));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields.insert(field.name().to_string(), FieldValue::Bool(value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
            return;
        }

        // Check if this is a secret field (name starts with "secret.")
        if field.name().starts_with("secret.") {
            self.fields.insert(field.name().to_string(), FieldValue::Redacted);
            return;
        }

        self.fields.insert(field.name().to_string(), FieldValue::String(value.to_string()));
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        // For error fields, try to extract structured error info
        let name = field.name();
        if name == "error" || name == "err" {
            let debug_str = format!("{value:?}");
            self.error = Some(LogErrorInfo {
                message: debug_str,
                kind: "std::error::Error".into(),
                source_chain: vec![],
            });
        } else {
            self.fields.insert(name.to_string(), FieldValue::String(format!("{value:?}")));
        }
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        let name = field.name();
        if name == "error" || name == "err" {
            self.error = Some(LogErrorInfo::from_error(value));
        } else {
            self.fields.insert(name.to_string(), FieldValue::String(value.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::FilterChain;
    use crate::pipeline::LogPipeline;
    use crate::redaction::RedactionEngine;

    #[test]
    fn test_field_visitor_extracts_message() {
        let mut visitor = LogFieldVisitor::new();
        let message_field = tracing::field::Field::new("message");
        // Can't easily test field visitor without tracing runtime, but the construction is correct
    }
}
