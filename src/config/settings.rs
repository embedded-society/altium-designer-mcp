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

    /// Rate-limiting settings for destructive (file-mutating) operations.
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
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

        // Validate rate limiting: a zero burst would block every operation,
        // and a non-finite/negative refill rate is nonsensical.
        if self.rate_limit.max_burst == 0 {
            return Err(ConfigError::ValidationError {
                message: "rate_limit.max_burst must be greater than 0".to_string(),
            });
        }
        if !self.rate_limit.refill_per_sec.is_finite() || self.rate_limit.refill_per_sec < 0.0 {
            return Err(ConfigError::ValidationError {
                message: "rate_limit.refill_per_sec must be a finite, non-negative number"
                    .to_string(),
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

    /// Optional path to an append-only JSON-lines audit log. When set, every
    /// destructive operation is recorded. When unset, no audit log is written.
    #[serde(default)]
    pub audit_log_path: Option<PathBuf>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            audit_log_path: None,
        }
    }
}

fn default_log_level() -> String {
    "warn".to_string()
}

/// Rate-limiting configuration for destructive (file-mutating) operations.
///
/// Uses a token bucket: up to `max_burst` mutating operations may run in a
/// burst, refilling at `refill_per_sec` tokens per second. Read-only tools are
/// never rate limited.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitConfig {
    /// Maximum number of mutating operations allowed in a burst.
    #[serde(default = "default_max_burst")]
    pub max_burst: u64,

    /// Sustained mutating operations allowed per second (token refill rate).
    /// A value of `0` permits a single burst with no refill.
    #[serde(default = "default_refill_per_sec")]
    pub refill_per_sec: f64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_burst: default_max_burst(),
            refill_per_sec: default_refill_per_sec(),
        }
    }
}

const fn default_max_burst() -> u64 {
    120
}

const fn default_refill_per_sec() -> f64 {
    30.0
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

    #[test]
    fn rate_limit_defaults() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_burst, 120);
        assert!((config.refill_per_sec - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_rate_limit_config() {
        let json = r#"{
            "rate_limit": {
                "max_burst": 50,
                "refill_per_sec": 10.0
            }
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_ok());
        assert_eq!(config.rate_limit.max_burst, 50);
        assert!((config.rate_limit.refill_per_sec - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn reject_zero_burst() {
        let json = r#"{ "rate_limit": { "max_burst": 0 } }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn reject_negative_refill() {
        let json = r#"{ "rate_limit": { "refill_per_sec": -1.0 } }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn allow_zero_refill_burst_once() {
        // refill_per_sec == 0 is valid: a single burst with no refill.
        let json = r#"{ "rate_limit": { "max_burst": 5, "refill_per_sec": 0.0 } }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn shipped_example_config_parses_and_validates() {
        // Guards config/example-config.json against drift from the Config schema
        // (deny_unknown_fields would otherwise let the docs and code diverge).
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/config/example-config.json");
        let text = std::fs::read_to_string(path).expect("read example config");
        let config: Config = serde_json::from_str(&text).expect("parse example config");
        config.validate().expect("validate example config");
    }
}
