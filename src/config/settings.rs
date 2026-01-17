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

    /// Path to the component library directory.
    #[serde(default)]
    pub library_path: Option<PathBuf>,

    /// Path to the style guide JSON file.
    #[serde(default)]
    pub style_guide_path: Option<PathBuf>,

    /// IPC-7351B settings.
    #[serde(default)]
    pub ipc: IpcConfig,

    /// Style settings.
    #[serde(default)]
    pub style: StyleConfig,

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
        // Validate density level if specified
        if let Some(ref density) = self.ipc.default_density {
            let valid_densities = ["M", "N", "L"];
            if !valid_densities.contains(&density.as_str()) {
                return Err(ConfigError::ValidationError {
                    message: format!(
                        "Invalid IPC density level '{}'. Must be one of: M, N, L",
                        density
                    ),
                });
            }
        }
        Ok(())
    }
}

/// IPC-7351B configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IpcConfig {
    /// Default density level: "M" (Most), "N" (Nominal), "L" (Least).
    /// Default: "N"
    #[serde(default = "default_density")]
    pub default_density: Option<String>,

    /// Include thermal relief vias in QFN/DFN thermal pads.
    #[serde(default = "default_true")]
    pub thermal_vias: bool,

    /// Courtyard margin in mm (added around component outline).
    #[serde(default = "default_courtyard_margin")]
    pub courtyard_margin: f64,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            default_density: default_density(),
            thermal_vias: default_true(),
            courtyard_margin: default_courtyard_margin(),
        }
    }
}

fn default_density() -> Option<String> {
    Some("N".to_string())
}

fn default_courtyard_margin() -> f64 {
    0.25
}

const fn default_true() -> bool {
    true
}

/// Style configuration for generated components.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleConfig {
    /// Silkscreen line width in mm.
    #[serde(default = "default_silkscreen_width")]
    pub silkscreen_line_width: f64,

    /// Assembly drawing line width in mm.
    #[serde(default = "default_assembly_width")]
    pub assembly_line_width: f64,

    /// Pad corner radius as percentage (0-100).
    #[serde(default)]
    pub pad_corner_radius_percent: f64,

    /// Pin 1 marker style: "dot", "chamfer", "line".
    #[serde(default = "default_pin1_style")]
    pub pin1_marker_style: String,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            silkscreen_line_width: default_silkscreen_width(),
            assembly_line_width: default_assembly_width(),
            pad_corner_radius_percent: 0.0,
            pin1_marker_style: default_pin1_style(),
        }
    }
}

fn default_silkscreen_width() -> f64 {
    0.15
}

fn default_assembly_width() -> f64 {
    0.10
}

fn default_pin1_style() -> String {
    "dot".to_string()
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
            "library_path": "/path/to/libraries",
            "style_guide_path": "/path/to/style.json",
            "ipc": {
                "default_density": "N",
                "thermal_vias": true,
                "courtyard_margin": 0.25
            },
            "style": {
                "silkscreen_line_width": 0.15,
                "assembly_line_width": 0.10,
                "pad_corner_radius_percent": 25.0,
                "pin1_marker_style": "chamfer"
            },
            "logging": {
                "level": "debug"
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_ok());
        assert_eq!(
            config.library_path,
            Some(PathBuf::from("/path/to/libraries"))
        );
        assert_eq!(config.ipc.default_density, Some("N".to_string()));
        assert!(config.ipc.thermal_vias);
        assert!((config.ipc.courtyard_margin - 0.25).abs() < f64::EPSILON);
        assert!((config.style.silkscreen_line_width - 0.15).abs() < f64::EPSILON);
        assert_eq!(config.style.pin1_marker_style, "chamfer");
        assert_eq!(config.logging.level, "debug");
    }

    #[test]
    fn ipc_config_defaults() {
        let config = IpcConfig::default();
        assert_eq!(config.default_density, Some("N".to_string()));
        assert!(config.thermal_vias);
        assert!((config.courtyard_margin - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn style_config_defaults() {
        let config = StyleConfig::default();
        assert!((config.silkscreen_line_width - 0.15).abs() < f64::EPSILON);
        assert!((config.assembly_line_width - 0.10).abs() < f64::EPSILON);
        assert!((config.pad_corner_radius_percent - 0.0).abs() < f64::EPSILON);
        assert_eq!(config.pin1_marker_style, "dot");
    }

    #[test]
    fn logging_config_defaults() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, "warn");
    }

    #[test]
    fn reject_invalid_density() {
        let json = r#"{
            "ipc": {
                "default_density": "X"
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
