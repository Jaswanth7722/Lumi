//! # Config Error Hierarchy
//!
//! Complete, structured error types for the configuration system.
//! No stringly-typed errors. Every variant carries rich context.

use std::fmt;
use std::path::PathBuf;

/// Top-level configuration error enum.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Config file not found or unreadable.
    #[error("Config file not found at {path}: {source}")]
    FileNotFound {
        /// The path that was attempted.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// Config file parse error with location.
    #[error("Config file parse error at {path}:{line}:{col}: {message}")]
    ParseError {
        /// The file being parsed.
        path: PathBuf,
        /// Line number where the error occurred.
        line: usize,
        /// Column number where the error occurred.
        col: usize,
        /// Description of the parse failure.
        message: String,
    },

    /// Validation failed with multiple errors.
    #[error("Config validation failed with {count} error(s):\n{errors}")]
    ValidationFailed {
        /// Number of validation errors.
        count: usize,
        /// All validation error messages joined.
        errors: String,
    },

    /// Environment variable has an invalid value.
    #[error("Environment variable {var} has invalid value {value:?}: {reason}")]
    EnvVarInvalid {
        /// The environment variable name.
        var: String,
        /// The invalid value.
        value: String,
        /// Why the value is invalid.
        reason: String,
    },

    /// Config migration failed.
    #[error("Config migration from v{from} to v{to} failed: {reason}")]
    MigrationFailed {
        /// Source schema version.
        from: u32,
        /// Target schema version.
        to: u32,
        /// Why the migration failed.
        reason: String,
    },

    /// Hot reload failed.
    #[error("Config hot reload failed: {reason}")]
    ReloadFailed {
        /// Why the reload failed.
        reason: String,
    },

    /// Config backup failed.
    #[error("Config backup failed at {path}: {source}")]
    BackupFailed {
        /// The backup path.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// Config write failed.
    #[error("Config write failed at {path}: {source}")]
    WriteFailed {
        /// The write path.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// Schema version too new (file was written by a newer version).
    #[error("Schema version {found} is newer than maximum supported {max}")]
    SchemaTooNew {
        /// Version found in the file.
        found: u32,
        /// Maximum version supported by this runtime.
        max: u32,
    },

    /// Feature flag not defined.
    #[allow(dead_code)]
    #[error("Feature flag {flag} is not defined")]
    UnknownFeatureFlag {
        /// The flag name.
        flag: String,
    },

    /// Config override failed.
    #[error("Config override for key {key} failed: {reason}")]
    OverrideFailed {
        /// The config key path.
        key: String,
        /// Why the override failed.
        reason: String,
    },

    /// Platform config path unavailable.
    #[error("Platform config path unavailable: {reason}")]
    PlatformPathUnavailable {
        /// Why the path is unavailable.
        reason: String,
    },
}

impl ConfigError {
    /// Whether the system can continue with defaults after this error.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            ConfigError::UnknownFeatureFlag { .. }
                | ConfigError::OverrideFailed { .. }
                | ConfigError::EnvVarInvalid { .. }
        )
    }

    /// A human-readable suggestion for resolving this error.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            ConfigError::FileNotFound { .. } => {
                "Create a configuration file at the expected path or use defaults."
            }
            ConfigError::ParseError { .. } => {
                "Fix the TOML syntax error at the indicated location."
            }
            ConfigError::ValidationFailed { .. } => {
                "Review the validation errors and fix the configuration file."
            }
            ConfigError::EnvVarInvalid { .. } => {
                "Set the environment variable to a valid value matching the expected type."
            }
            ConfigError::MigrationFailed { .. } => {
                "The configuration migration failed. Revert to a backup or use a compatible version."
            }
            ConfigError::ReloadFailed { .. } => {
                "The new configuration was invalid. The previous configuration is still active."
            }
            ConfigError::BackupFailed { .. } => {
                "Ensure the backup directory is writable. The original file was not modified."
            }
            ConfigError::WriteFailed { .. } => {
                "Ensure the configuration directory is writable by the application."
            }
            ConfigError::SchemaTooNew { .. } => {
                "This config file was created by a newer version of Lumi. Upgrade the application."
            }
            ConfigError::UnknownFeatureFlag { .. } => {
                "Remove the unknown feature flag from the configuration."
            }
            ConfigError::OverrideFailed { .. } => {
                "Check the override key path and value type."
            }
            ConfigError::PlatformPathUnavailable { .. } => {
                "Set the HOME or APPDATA environment variable."
            }
        }
    }
}

/// A single validation error with rich context.
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Dotted field path (e.g., "ai.local_model_path").
    pub field_path: String,
    /// Category of validation failure.
    pub category: ValidationCategory,
    /// Human-readable error message.
    pub message: String,
    /// The expected value or range.
    pub expected: Option<String>,
    /// The actual value found.
    pub actual: Option<String>,
    /// Suggested fix.
    pub suggestion: Option<String>,
}

impl ValidationError {
    /// Create a new validation error.
    pub fn new(
        field_path: impl Into<String>,
        category: ValidationCategory,
        message: impl Into<String>,
    ) -> Self {
        Self {
            field_path: field_path.into(),
            category,
            message: message.into(),
            expected: None,
            actual: None,
            suggestion: None,
        }
    }

    /// Set the expected value.
    pub fn with_expected(mut self, expected: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self
    }

    /// Set the actual value.
    pub fn with_actual(mut self, actual: impl Into<String>) -> Self {
        self.actual = Some(actual.into());
        self
    }

    /// Set the suggestion.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Returns true if this error can be auto-corrected by applying the default.
    pub fn is_auto_correctable(&self) -> bool {
        matches!(
            self.category,
            ValidationCategory::OutOfRange | ValidationCategory::InvalidEnumVariant
        )
    }

    /// Returns the default value to apply if `is_auto_correctable()` is true.
    pub fn auto_correction(&self) -> Option<String> {
        self.expected.clone()
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field_path, self.message)?;
        if let Some(ref expected) = self.expected {
            write!(f, " (expected: {expected})")?;
        }
        if let Some(ref actual) = self.actual {
            write!(f, " (actual: {actual})")?;
        }
        Ok(())
    }
}

/// Category of validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationCategory {
    /// A required field is missing.
    RequiredFieldMissing,
    /// Value is outside the allowed range.
    OutOfRange,
    /// Value is not a valid enum variant.
    InvalidEnumVariant,
    /// Specified path does not exist.
    PathNotFound,
    /// Specified path is not a directory.
    PathNotDirectory,
    /// Specified path is not readable.
    PathNotReadable,
    /// Specified path is not writable.
    PathNotWritable,
    /// URL is not valid.
    InvalidUrl,
    /// Port number is invalid.
    InvalidPort,
    /// Cross-field constraint violation.
    IncompatibleOptions,
    /// Insufficient OS permissions.
    InsufficientPermission,
    /// Version incompatibility.
    VersionIncompatible,
}
