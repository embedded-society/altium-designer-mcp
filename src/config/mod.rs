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

pub use settings::{Config, LoggingConfig};

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

    #[test]
    fn load_config_missing_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.json");
        let err = load_config(Some(&missing)).unwrap_err();
        assert!(matches!(err, ConfigError::NotFound { .. }));
    }

    #[test]
    fn load_config_directory_is_read_error() {
        // A directory exists() but cannot be read as a string — exercises the
        // ReadError branch distinct from NotFound.
        let dir = tempfile::tempdir().unwrap();
        let err = load_config(Some(dir.path())).unwrap_err();
        assert!(matches!(err, ConfigError::ReadError { .. }));
    }

    #[test]
    fn load_config_malformed_json_is_parse_error() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "{ this is not json ").unwrap();
        let err = load_config(Some(file.path())).unwrap_err();
        assert!(matches!(err, ConfigError::ParseError { .. }));
    }

    #[test]
    fn load_config_invalid_log_level_is_validation_error() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), r#"{"logging":{"level":"chatty"}}"#).unwrap();
        let err = load_config(Some(file.path())).unwrap_err();
        assert!(matches!(err, ConfigError::ValidationError { .. }));
        assert!(err.to_string().to_lowercase().contains("log level"));
    }

    #[test]
    fn load_config_valid_file_parses_and_validates() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            file.path(),
            r#"{"allowed_paths":["/tmp/libs"],"logging":{"level":"warn"}}"#,
        )
        .unwrap();
        let config = load_config(Some(file.path())).expect("valid config loads");
        assert_eq!(config.allowed_paths.len(), 1);
        assert_eq!(config.logging.level, "warn");
    }

    #[test]
    fn shipped_example_config_loads() {
        // Drift guard: the documented example must always parse and validate
        // against the current schema.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("config")
            .join("example-config.json");
        load_config(Some(&path)).expect("shipped example-config.json must load");
    }
}
