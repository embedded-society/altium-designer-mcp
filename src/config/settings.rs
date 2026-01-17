//! Configuration structures for deserialisation.
//!
//! These structures map directly to the JSON configuration file format.

use std::path::PathBuf;

use serde::Deserialize;

use crate::error::ConfigError;

/// Root configuration structure.
///
/// This is the top-level structure that matches the JSON config file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Optional JSON schema reference (ignored during parsing).
    #[serde(rename = "$schema", default)]
    _schema: Option<String>,

    /// Optional comment field (ignored during parsing).
    #[serde(rename = "_comment", default)]
    _comment: Option<String>,

    /// Allowed paths for library operations.
    /// Only files within these directories can be accessed.
    #[serde(default)]
    pub allowed_paths: Vec<PathBuf>,

    /// Logging settings.
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl Config {
    /// Validates the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any validation checks fail.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate log level
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.logging.level.to_lowercase().as_str()) {
            return Err(ConfigError::ValidationError {
                message: format!(
                    "Invalid log level '{}'. Must be one of: trace, debug, info, warn, error",
                    self.logging.level
                ),
            });
        }
        Ok(())
    }
}

/// Logging configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error).
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
        }
    }
}

fn default_log_level() -> String {
    "warn".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let json = r"{}";
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn parse_full_config() {
        let json = r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "_comment": "Test config",
            "allowed_paths": ["/path/to/libraries", "/another/path"],
            "logging": {
                "level": "debug"
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_ok());
        assert_eq!(
            config.allowed_paths,
            vec![
                PathBuf::from("/path/to/libraries"),
                PathBuf::from("/another/path")
            ]
        );
        assert_eq!(config.logging.level, "debug");
    }

    #[test]
    fn logging_config_defaults() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, "warn");
    }

    #[test]
    fn reject_invalid_log_level() {
        let json = r#"{
            "logging": {
                "level": "invalid"
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn reject_unknown_fields() {
        let json = r#"{
            "unknown_field": "value"
        }"#;

        let result: Result<Config, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}
