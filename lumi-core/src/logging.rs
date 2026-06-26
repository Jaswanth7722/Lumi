//! # Logging and Telemetry — Structured Logging and Audit (Chapter 28)

use lumi_common::logging::{
    AuditEntry, AuditEventType, AuditOutcome, LogEntry, LogLevel, TelemetryConfig,
};
use std::collections::HashMap;
use tracing::debug;

/// Manages structured logging and audit events.
pub struct LoggingManager {
    process: String,
    audit_log: Vec<AuditEntry>,
    max_audit_entries: usize,
    telemetry_config: TelemetryConfig,
}

impl LoggingManager {
    pub fn new(process: &str) -> Self {
        Self {
            process: process.to_string(),
            audit_log: Vec::new(),
            max_audit_entries: 10000,
            telemetry_config: TelemetryConfig::new(),
        }
    }

    pub fn log(&self, level: LogLevel, module: &str, message: &str) -> LogEntry {
        let entry = LogEntry::new(level, &self.process, module, message);
        match level {
            LogLevel::Error => tracing::error!("[{}] {}", module, message),
            LogLevel::Warn => tracing::warn!("[{}] {}", module, message),
            LogLevel::Info => tracing::info!("[{}] {}", module, message),
            LogLevel::Debug => tracing::debug!("[{}] {}", module, message),
            LogLevel::Trace => tracing::trace!("[{}] {}", module, message),
        }
        entry
    }

    pub fn audit(&mut self, entry: AuditEntry) {
        if self.audit_log.len() >= self.max_audit_entries {
            self.audit_log.remove(0);
        }
        self.audit_log.push(entry);
    }

    pub fn audit_log(&self) -> &[AuditEntry] {
        &self.audit_log
    }

    pub fn set_telemetry_enabled(&mut self, enabled: bool) {
        self.telemetry_config.enabled = enabled;
    }

    pub fn telemetry_enabled(&self) -> bool {
        self.telemetry_config.enabled
    }

    pub fn record_telemetry(&self, event: &str, properties: HashMap<String, String>) {
        if !self.telemetry_config.enabled {
            return;
        }
        debug!("Telemetry: {} {:?}", event, properties);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_creation() {
        let manager = LoggingManager::new("test");
        let entry = manager.log(LogLevel::Info, "test_module", "test message");
        assert_eq!(entry.level, LogLevel::Info);
    }

    #[test]
    fn test_audit_log() {
        let mut manager = LoggingManager::new("test");
        manager.audit(AuditEntry::tool_executed(
            "fs.read_file",
            AuditOutcome::Success,
            Some(true),
        ));
        assert_eq!(manager.audit_log().len(), 1);
    }

    #[test]
    fn test_telemetry_disabled() {
        let manager = LoggingManager::new("test");
        assert!(!manager.telemetry_enabled());
    }
}
