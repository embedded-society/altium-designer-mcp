//! Error types for git-proxy-mcp.
//!
//! # Security Note
//!
//! Error messages are carefully crafted to NEVER include credentials.
//! All error variants that could potentially contain sensitive data
//! use generic descriptions instead of including the actual values.

use std::path::PathBuf;

use thiserror::Error;

/// Errors that can occur during configuration operations.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Configuration file could not be read.
    #[error("failed to read configuration file: {path}")]
    ReadError {
        /// Path to the configuration file.
        path: PathBuf,
        /// The underlying IO error.
        #[source]
        source: std::io::Error,
    },

    /// Configuration file could not be parsed.
    #[error("failed to parse configuration file: {path}")]
    ParseError {
        /// Path to the configuration file.
        path: PathBuf,
        /// The underlying JSON error.
        #[source]
        source: serde_json::Error,
    },

    /// Configuration file not found.
    #[error("configuration file not found: {path}")]
    NotFound {
        /// Path where the configuration file was expected.
        path: PathBuf,
    },

    /// Configuration validation failed.
    #[error("configuration validation failed: {message}")]
    ValidationError {
        /// Description of the validation failure.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_error_display() {
        let error = ConfigError::NotFound {
            path: PathBuf::from("/path/to/config.json"),
        };
        let msg = error.to_string();
        assert!(msg.contains("not found"));
        assert!(msg.contains("config.json"));
    }

    #[test]
    fn validation_error_display() {
        let error = ConfigError::ValidationError {
            message: "invalid setting".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.contains("invalid setting"));
    }
}
