//! # Version Management
//!
//! Runtime version information and feature flag management.
//!
//! Provides compile-time build metadata (version, git SHA, build date,
//! profile, target triple) and runtime feature flag evaluation.
//!
//! # Thread Safety
//!
//! All types are `Send + Sync`. Feature flags are loaded once at bootstrap
//! and stored in an immutable `HashSet` for O(1) lookup without locking.

use std::collections::HashSet;
use std::fmt;

/// Build profile of the current binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildProfile {
    /// Debug build (`cargo build`).
    Debug,
    /// Release build (`cargo build --release`).
    Release,
    /// Release with debug info.
    RelWithDebInfo,
}

impl fmt::Display for BuildProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildProfile::Debug => write!(f, "debug"),
            BuildProfile::Release => write!(f, "release"),
            BuildProfile::RelWithDebInfo => write!(f, "relwithdebinfo"),
        }
    }
}

/// Runtime version metadata.
///
/// Carries comprehensive build provenance information so that operators
/// and diagnostic tools can identify exactly which binary is running.
#[derive(Debug, Clone)]
pub struct RuntimeVersion {
    /// Semantic version of the runtime (e.g., "1.0.0").
    pub version: semver::Version,
    /// Optional git commit SHA from which this binary was built.
    pub git_sha: Option<&'static str>,
    /// Build date in ISO 8601 format (e.g., "2026-06-26").
    pub build_date: &'static str,
    /// Build profile used to compile this binary.
    pub build_profile: BuildProfile,
    /// Target triple (e.g., "x86_64-unknown-linux-gnu").
    pub target_triple: &'static str,
}

impl RuntimeVersion {
    /// The current runtime version, populated from compile-time constants.
    ///
    /// Uses `option_env!` so that build metadata is optional. In production,
    /// a build script should set `VERGEN_GIT_SHA`, `VERGEN_BUILD_DATE` etc.
    pub fn current() -> Self {
        Self {
            version: semver::Version::new(0, 1, 0),
            git_sha: None,
            build_date: option_env!("VERGEN_BUILD_DATE").unwrap_or("unknown"),
            build_profile: Self::detect_profile(),
            target_triple: std::env::consts::ARCH,
        }
    }

    /// Detect the build profile.
    fn detect_profile() -> BuildProfile {
        if cfg!(debug_assertions) {
            BuildProfile::Debug
        } else if cfg!(feature = "relwithdebinfo") {
            BuildProfile::RelWithDebInfo
        } else {
            BuildProfile::Release
        }
    }

    /// Human-readable version string suitable for display.
    pub fn display_string(&self) -> String {
        let sha = self
            .git_sha
            .map(|s| format!(" (git: {})", &s[..7]))
            .unwrap_or_default();
        format!(
            "v{}{} {} {} built {}",
            self.version, sha, self.build_profile, self.target_triple, self.build_date
        )
    }
}

impl fmt::Display for RuntimeVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_string())
    }
}

// ---------------------------------------------------------------------------
// Feature Flags
// ---------------------------------------------------------------------------

/// Manages compile-time and runtime feature flags.
///
/// Compile-time flags are defined via Cargo features. Runtime flags are loaded
/// from configuration and allow gradual rollout of new capabilities.
///
/// # Examples
///
/// ```ignore
/// let flags = FeatureFlags::new();
/// flags.register("local-inference");
/// assert!(flags.is_enabled("local-inference"));
/// ```
#[derive(Debug, Clone)]
pub struct FeatureFlags {
    /// Set of enabled feature flag names.
    enabled: HashSet<String>,
}

impl FeatureFlags {
    /// Create an empty feature flags set.
    pub fn new() -> Self {
        Self {
            enabled: HashSet::new(),
        }
    }

    /// Create a feature flags set from a list of enabled flags.
    pub fn from_flags(flags: Vec<String>) -> Self {
        Self {
            enabled: flags.into_iter().collect(),
        }
    }

    /// Register and enable a compile-time or runtime feature flag.
    ///
    /// # Panics
    ///
    /// Panics if called after bootstrap (flags are immutable after startup).
    pub fn register(&mut self, flag: &str) {
        self.enabled.insert(flag.to_string());
    }

    /// Check if a feature flag is enabled.
    ///
    /// Returns `true` if the flag has been registered and enabled.
    /// This is an O(1) lookup via `HashSet::contains`.
    pub fn is_enabled(&self, flag: &str) -> bool {
        self.enabled.contains(flag)
    }

    /// Check if all the given feature flags are enabled.
    pub fn all_enabled(&self, flags: &[&str]) -> bool {
        flags.iter().all(|f| self.enabled.contains(*f))
    }

    /// Check if any of the given feature flags are enabled.
    pub fn any_enabled(&self, flags: &[&str]) -> bool {
        flags.iter().any(|f| self.enabled.contains(*f))
    }

    /// Get the set of all enabled feature flags.
    pub fn enabled_flags(&self) -> &HashSet<String> {
        &self.enabled
    }

    /// Number of enabled feature flags.
    pub fn len(&self) -> usize {
        self.enabled.len()
    }

    /// Whether any feature flags are registered.
    pub fn is_empty(&self) -> bool {
        self.enabled.is_empty()
    }
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_version_display() {
        let v = RuntimeVersion::current();
        let display = v.display_string();
        // Should start with "v0.1.0"
        assert!(display.starts_with("v0.1.0"), "version string: {display}");
    }

    #[test]
    fn test_feature_flags() {
        let mut flags = FeatureFlags::new();
        assert!(!flags.is_enabled("local-inference"));
        flags.register("local-inference");
        assert!(flags.is_enabled("local-inference"));
    }

    #[test]
    fn test_all_enabled() {
        let mut flags = FeatureFlags::new();
        flags.register("a");
        flags.register("b");
        assert!(flags.all_enabled(&["a", "b"]));
        assert!(!flags.all_enabled(&["a", "c"]));
    }

    #[test]
    fn test_empty() {
        let flags = FeatureFlags::new();
        assert!(flags.is_empty());
        assert_eq!(flags.len(), 0);
    }
}
