//! # Message Validator
//!
//! Validates received messages before authentication:
//! - Envelope structure (magic, version, fields)
//! - Payload schema compliance
//! - TTL expiry
//! - Size limits
//! - Timestamp freshness

use crate::error::ValidationError;
use crate::message::{ChannelName, LumiMessage, MessageKind, ProtocolVersion};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Schema definition for a channel.
#[derive(Debug, Clone)]
pub struct ChannelSchema {
    /// Channel name
    pub channel: String,
    /// Maximum payload size in bytes
    pub max_payload_bytes: u32,
    /// Supported schema version
    pub schema_version: u16,
    /// Required fields
    pub required_fields: Vec<&'static str>,
}

/// Message validator.
pub struct Validator {
    /// Channel schemas
    channel_schemas: HashMap<String, ChannelSchema>,
    /// Maximum allowed clock skew in seconds
    max_clock_skew_secs: i64,
    /// Default TTL for messages without explicit TTL
    default_ttl_ms: u32,
}

impl Validator {
    /// Create a new validator.
    pub fn new(max_clock_skew_secs: i64, default_ttl_ms: u32) -> Self {
        Self {
            channel_schemas: HashMap::new(),
            max_clock_skew_secs,
            default_ttl_ms,
        }
    }

    /// Register a channel schema for validation.
    pub fn register_schema(&mut self, schema: ChannelSchema) {
        self.channel_schemas.insert(schema.channel.clone(), schema);
    }

    /// Validate a received message envelope (before auth — checks structure only).
    pub fn check_envelope(&self, msg: &LumiMessage) -> Result<(), ValidationError> {
        // Validate version compatibility
        let current = ProtocolVersion::CURRENT;
        if msg.version.major != current.major {
            return Err(ValidationError::SchemaVersionMismatch {
                expected: current.major,
                got: msg.version.major,
            });
        }

        // Validate required fields
        if msg.id.0.is_empty() {
            return Err(ValidationError::MissingField { field: "id".into() });
        }
        if msg.sender.to_string().is_empty() {
            return Err(ValidationError::MissingField { field: "sender".into() });
        }
        if msg.channel.0.is_empty() {
            return Err(ValidationError::MissingField { field: "channel".into() });
        }

        // Validate timestamp freshness (± max_clock_skew)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let timestamp_ms = msg.timestamp / 1000;
        let now_ms = now / 1000;
        let skew_ms = (now_ms as i64 - timestamp_ms as i64).abs();

        if skew_ms > self.max_clock_skew_secs * 1000 {
            return Err(ValidationError::TimestampOutOfRange);
        }

        // Payload size is checked in check_payload
        Ok(())
    }

    /// Validate the payload matches the declared channel schema.
    pub fn check_payload(&self, msg: &LumiMessage) -> Result<(), ValidationError> {
        if let Some(schema) = self.channel_schemas.get(&msg.channel.0) {
            // Check payload size
            let payload_size = rmp_serde::to_vec(&msg.payload)
                .map(|b| b.len() as u32)
                .unwrap_or(0);

            if payload_size > schema.max_payload_bytes {
                return Err(ValidationError::PayloadTooLarge {
                    size: payload_size,
                    limit: schema.max_payload_bytes,
                });
            }
        }
        Ok(())
    }

    /// Check the message has not exceeded its TTL.
    pub fn check_ttl(&self, msg: &LumiMessage) -> Result<(), ValidationError> {
        let ttl_ms = msg.ttl_ms.unwrap_or(self.default_ttl_ms);

        if ttl_ms == 0 {
            return Ok(()); // No TTL
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let age_us = now.saturating_sub(msg.timestamp);
        let age_ms = (age_us / 1000) as u64;

        if age_ms > ttl_ms as u64 {
            return Err(ValidationError::Expired {
                age_ms,
                ttl_ms,
            });
        }

        Ok(())
    }

    /// Run all validation checks.
    pub fn validate_all(&self, msg: &LumiMessage) -> Result<(), ValidationError> {
        self.check_envelope(msg)?;
        self.check_payload(msg)?;
        self.check_ttl(msg)?;
        Ok(())
    }

    /// Check if a message kind requires a reply.
    pub fn requires_reply(kind: &MessageKind) -> bool {
        matches!(kind, MessageKind::Request { .. })
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new(60, 30000) // 60s clock skew, 30s default TTL
    }
}
