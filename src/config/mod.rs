//! Configuration file loading and parsing.
//!
//! This module handles loading the configuration file from disk and parsing
//! it into validated, type-safe structures.
//!
//! # Configuration File Locations
//!
//! The configuration file is searched in the following order:
//!
//! 1. Path specified via `--config` CLI flag
//! 2. Default location:
//!    - **Linux/macOS:** `~/.altium-designer-mcp/config.json`
//!    - **Windows:** `%USERPROFILE%\.altium-designer-mcp\config.json`
//!
//! # Example Configuration
//!
//! See `config/example-config.json` for a complete example.

mod settings;

pub use settings::{Config, IpcConfig, LoggingConfig, StyleConfig};

use std::path::{Path, PathBuf};

use crate::error::ConfigError;

/// Returns the default configuration directory.
///
/// - **Linux/macOS:** `~/.altium-designer-mcp/`
/// - **Windows:** `%USERPROFILE%\.altium-designer-mcp\`
#[must_use]
pub fn default_config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|p| p.join(".altium-designer-mcp"))
}

/// Returns the platform-specific default configuration file path.
#[must_use]
pub fn default_config_path() -> Option<PathBuf> {
    default_config_dir().map(|p| p.join("config.json"))
}

/// Loads and parses the configuration file.
///
/// If `path` is `None`, uses the platform-specific default location.
///
/// # Errors
///
/// Returns an error if:
/// - The configuration file cannot be found
/// - The file cannot be read
/// - The JSON is malformed
/// - Required fields are missing or invalid
pub fn load_config(path: Option<&Path>) -> Result<Config, ConfigError> {
    let config_path = match path {
        Some(p) => p.to_path_buf(),
        None => default_config_path().ok_or_else(|| ConfigError::NotFound {
            path: PathBuf::from("<default config path>"),
        })?,
    };

    if !config_path.exists() {
        return Err(ConfigError::NotFound { path: config_path });
    }

    let contents = std::fs::read_to_string(&config_path).map_err(|e| ConfigError::ReadError {
        path: config_path.clone(),
        source: e,
    })?;

    let config: Config = serde_json::from_str(&contents).map_err(|e| ConfigError::ParseError {
        path: config_path.clone(),
        source: e,
    })?;

    // Validate the configuration
    config.validate()?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_dir_exists() {
        assert!(default_config_dir().is_some());
    }

    #[test]
    fn default_config_path_exists() {
        let path = default_config_path();
        assert!(path.is_some());
        assert!(path.unwrap().to_string_lossy().contains("config.json"));
    }
}
