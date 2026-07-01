//! # Platform Config Paths
//!
//! Returns platform-appropriate configuration, data, models, logs, and plugins
//! directory paths for the Lumas platform.

use crate::error::ConfigError;
use std::path::{Path, PathBuf};

/// Returns the platform-appropriate configuration file path.
///
/// - macOS:   `~/Library/Application Support/Lumas/config.toml`
/// - Windows: `%APPDATA%\Lumas\config.toml`
/// - Linux:   `$XDG_CONFIG_HOME/lumas/config.toml` or `~/.config/lumas/config.toml`
///
/// # Errors
///
/// Returns `ConfigError::PlatformPathUnavailable` if the home directory
/// or `%APPDATA%` cannot be determined.
pub fn config_file_path() -> Result<PathBuf, ConfigError> {
    let base = config_dir()?;
    Ok(base.join("config.toml"))
}

/// Returns the platform-appropriate config directory.
///
/// - macOS:   `~/Library/Application Support/Lumas/`
/// - Windows: `%APPDATA%\Lumas\`
/// - Linux:   `$XDG_CONFIG_HOME/lumas/` or `~/.config/lumas/`
pub fn config_dir() -> Result<PathBuf, ConfigError> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").map_err(|_| ConfigError::PlatformPathUnavailable {
            reason: "HOME environment variable not set".into(),
        })?;
        Ok(PathBuf::from(home).join("Library/Application Support/Lumas"))
    }
    #[cfg(target_os = "windows")]
    {
        let appdata =
            std::env::var("APPDATA").map_err(|_| ConfigError::PlatformPathUnavailable {
                reason: "APPDATA environment variable not set".into(),
            })?;
        Ok(PathBuf::from(appdata).join("Lumas"))
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            Ok(PathBuf::from(xdg).join("lumas"))
        } else if let Ok(home) = std::env::var("HOME") {
            Ok(PathBuf::from(home).join(".config/lumas"))
        } else {
            Ok(PathBuf::from("/etc/lumas"))
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Ok(PathBuf::from("/tmp/lumas"))
    }
}

/// Returns the platform-appropriate user data directory.
///
/// - macOS:   `~/Library/Application Support/Lumas/`
/// - Windows: `%APPDATA%\Lumas\`
/// - Linux:   `$XDG_DATA_HOME/lumas/` or `~/.local/share/lumas/`
pub fn data_dir() -> Result<PathBuf, ConfigError> {
    #[cfg(target_os = "macos")]
    {
        // macOS uses same base as config
        config_dir()
    }
    #[cfg(target_os = "windows")]
    {
        config_dir()
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            Ok(PathBuf::from(xdg).join("lumas"))
        } else if let Ok(home) = std::env::var("HOME") {
            Ok(PathBuf::from(home).join(".local/share/lumas"))
        } else {
            Ok(PathBuf::from("/var/lib/lumas"))
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Ok(PathBuf::from("/tmp/lumas"))
    }
}

/// Returns the platform-appropriate models directory.
pub fn models_dir() -> Result<PathBuf, ConfigError> {
    Ok(data_dir()?.join("models"))
}

/// Returns the platform-appropriate logs directory.
pub fn logs_dir() -> Result<PathBuf, ConfigError> {
    Ok(data_dir()?.join("logs"))
}

/// Returns the platform-appropriate plugins directory.
pub fn plugins_dir() -> Result<PathBuf, ConfigError> {
    Ok(data_dir()?.join("plugins"))
}

/// Ensures the given directory exists, creating it (and parents) if absent.
///
/// # Errors
///
/// Returns `ConfigError::WriteFailed` if the directory cannot be created.
pub fn ensure_dir(path: &Path) -> Result<(), ConfigError> {
    std::fs::create_dir_all(path).map_err(|e| ConfigError::WriteFailed {
        path: path.to_path_buf(),
        source: e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_file_path_ends_with_config_toml() {
        let path = config_file_path().unwrap();
        assert!(path.ends_with("config.toml"));
    }

    #[test]
    fn test_data_dir_returns_non_empty() {
        let path = data_dir().unwrap();
        assert!(!path.as_os_str().is_empty());
    }

    #[test]
    fn test_ensure_dir_creates_and_succeeds() {
        let tmp = std::env::temp_dir().join(format!("lumas-test-{}", std::process::id()));
        let result = ensure_dir(&tmp);
        assert!(result.is_ok());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_models_dir_is_subdir_of_data() {
        let data = data_dir().unwrap();
        let models = models_dir().unwrap();
        assert!(models.starts_with(&data));
    }
}
