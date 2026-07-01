//! # Schema Migration Engine
//!
//! Transforms TOML config between schema versions. Migrations run in
//! version order and are chained automatically.

use crate::error::ConfigError;
use crate::schema::LumiConfig;

/// A migration transforms a TOML Value tree from one schema version to the next.
/// Migrations run in version order and are chained automatically.
pub trait Migration: Send + Sync {
    /// Source schema version.
    #[allow(clippy::wrong_self_convention)]
    fn from_version(&self) -> u32;
    /// Target schema version.
    fn to_version(&self) -> u32;
    /// Apply the migration to a raw TOML value tree.
    /// Must be idempotent: running it twice produces the same output.
    fn apply(&self, config: toml::Value) -> Result<toml::Value, ConfigError>;
    /// Human-readable description of what this migration does.
    fn description(&self) -> &'static str;
}

/// Engine that chains and runs migrations in order.
pub struct MigrationEngine {
    /// Registered migrations, sorted by version.
    migrations: Vec<Box<dyn Migration>>,
}

impl MigrationEngine {
    /// Create a new empty migration engine.
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    /// Register a migration. Migrations should be registered in version order.
    pub fn register(&mut self, migration: Box<dyn Migration>) {
        self.migrations.push(migration);
    }

    /// Apply all migrations from `from_version` up to `LumiConfig::CURRENT_SCHEMA_VERSION`.
    /// Returns the migrated TOML value and a log of applied migrations.
    pub fn migrate(
        &self,
        config: toml::Value,
        from_version: u32,
    ) -> Result<(toml::Value, Vec<String>), ConfigError> {
        let target = LumiConfig::CURRENT_SCHEMA_VERSION;
        if from_version > target {
            return Err(ConfigError::SchemaTooNew {
                found: from_version,
                max: target,
            });
        }

        let mut current = config;
        let mut log = Vec::new();
        let mut version = from_version;

        while version < target {
            let next_version = version + 1;
            let migrated = self
                .migrations
                .iter()
                .find(|m| m.from_version() == version && m.to_version() == next_version)
                .ok_or_else(|| ConfigError::MigrationFailed {
                    from: version,
                    to: next_version,
                    reason: format!("No migration registered from v{version} to v{next_version}"),
                })?;

            current = migrated.apply(current)?;
            log.push(format!(
                "v{} → v{}: {}",
                version,
                next_version,
                migrated.description()
            ));
            version = next_version;
        }

        Ok((current, log))
    }
}

impl Default for MigrationEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// V0 → V1 migration: initial schema.
///
/// This migration:
/// - Adds the `schema_version` field if missing
/// - Renames old `inference_mode` string field to match new enum format
pub struct MigrationV0ToV1;

impl Migration for MigrationV0ToV1 {
    fn from_version(&self) -> u32 {
        0
    }
    fn to_version(&self) -> u32 {
        1
    }

    fn apply(&self, mut config: toml::Value) -> Result<toml::Value, ConfigError> {
        // Ensure schema_version is set
        if let Some(table) = config.as_table_mut() {
            table.insert("schema_version".into(), toml::Value::Integer(1));

            // Migrate old ai.inference_mode values if present
            if let Some(ai) = table.get_mut("ai") {
                if let Some(ai_table) = ai.as_table_mut() {
                    if let Some(mode) = ai_table.get("inference_mode") {
                        if let Some(s) = mode.as_str() {
                            let new_mode = if s == "local" || s == "always_local" {
                                "always_local"
                            } else if s == "cloud" || s == "always_cloud" {
                                "always_cloud"
                            } else if s == "prefer_local" {
                                "prefer_local"
                            } else if s == "prefer_cloud" {
                                "prefer_cloud"
                            } else {
                                "prefer_local"
                            };
                            ai_table.insert(
                                "inference_mode".into(),
                                toml::Value::String(new_mode.into()),
                            );
                        }
                    }
                }
            }
        }

        Ok(config)
    }

    fn description(&self) -> &'static str {
        "Initial schema: adds schema_version field, normalizes inference_mode values"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_v0_to_v1_sets_schema_version() {
        let config_str = r#"
            [ai]
            inference_mode = "local"
        "#;
        let value: toml::Value = toml::from_str(config_str).unwrap();

        let engine = MigrationEngine::new();
        // We'd need to register the migration to test the chain
        let migration = MigrationV0ToV1;
        let result = migration.apply(value).unwrap();

        assert_eq!(
            result.get("schema_version").and_then(|v| v.as_integer()),
            Some(1)
        );
    }

    #[test]
    fn test_schema_too_new_returns_error() {
        let value = toml::Value::Table(toml::value::Table::new());
        let engine = MigrationEngine::new();
        let result = engine.migrate(value, 999);
        assert!(result.is_err());
        match result {
            Err(ConfigError::SchemaTooNew { found, .. }) => {
                assert_eq!(found, 999);
            }
            _ => panic!("Expected SchemaTooNew error"),
        }
    }
}
