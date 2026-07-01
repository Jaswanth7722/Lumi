//! # Lumas Configuration System
//!
//! Complete, production-ready configuration system for the Lumas platform.
//! Every subsystem reads configuration exclusively through this crate.
//!
//! # Architecture
//!
//! ```text
//! ConfigManager (public entry point)
//!   ├── ConfigCache (ArcSwap-based, lock-free reads)
//!   ├── ConfigLoader (7-stage loading pipeline)
//!   ├── ConfigWatcher (OS-native file watching + hot reload)
//!   ├── OverrideManager (runtime overrides)
//!   └── MigrationEngine (schema version migration)
//! ```
//!
//! # Thread Safety
//!
//! All public types are `Send + Sync`. Configuration is accessed from multiple
//! threads concurrently from the moment it is loaded.
//!
//! # WORKSPACE AUDIT
//!
//! This crate was built as the single source of truth for all configuration
//! in the Lumas platform. It replaces and extends the previous inline config
//! structs that were in lumi-common and lumas-runtime.
//!
//! Existing config-related items found during workspace audit:
//! - lumas-config/src/lib.rs (old): Had LumiConfig with flat config structs
//! - lumas-runtime/src/config.rs: Had ConfigLoader wrapping lumas-config
//! - lumas-runtime/src/event.rs: Had ConfigLoaded, ConfigReloaded events
//!
//! Design decisions:
//! - lumas-config does NOT depend on lumas-runtime to avoid circular deps
//! - Config events are standalone types; a ConfigEventPublisher trait
//!   bridges to lumas-runtime's event bus
//! - Secret<T> newtype prevents API key leakage through Debug/Display/Serialize

// Public modules
pub mod cache;
pub mod env;
pub mod error;
pub mod events;
pub mod loader;
pub mod manager;
pub mod migration;
pub mod override_;
pub mod platform;
pub mod resolver;
pub mod schema;
pub mod secret;
pub mod validator;
pub mod watcher;

// Re-export key public types at crate root for convenience
pub use cache::ConfigCache;
pub use error::{ConfigError, ValidationCategory, ValidationError};
pub use events::{ConfigEventPublisher, ConfigLoaded, ConfigReloadFailed, ConfigReloaded};
pub use loader::ConfigLoader;
pub use manager::ConfigManager;
pub use migration::{Migration, MigrationEngine, MigrationV0ToV1};
pub use override_::OverrideManager;
pub use platform::{
    config_dir, config_file_path, data_dir, ensure_dir, logs_dir, models_dir, plugins_dir,
};
pub use resolver::{merge_configs, ConfigSource, ResolvedConfig};
pub use schema::{
    AIConfig, AccessibilityConfig, AnimationConfig, CharacterConfig, CloudProvider,
    DiagnosticsConfig, FeatureFlags, GeneralConfig, IPCConfig, InferenceMode, LoggingConfig,
    LumiConfig, MemoryConfig, PerformanceConfig, PhysicsConfig, PluginConfig, PrivacyConfig,
    RenderingConfig, RuntimeConfig, STTModel, SecurityConfig, StorageConfig, UpdateConfig,
    VoiceConfig, WorkspaceConfig,
};
pub use secret::Secret;
pub use validator::{validate_config, Validate, ValidateWith};
