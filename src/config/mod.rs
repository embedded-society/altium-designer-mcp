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

/// Loads the configuration and grants access to `allow` directories on top of
/// whatever the file lists.
///
/// With an empty `allow` this is exactly [`load_config`]. With directories
/// given, a missing config file is no longer an error: the defaults stand in
/// for it, so `altium-designer-mcp --allow <dir>` runs without any file — the
/// path the Claude Desktop extension uses, where a directory picker supplies
/// the grants and nothing writes `config.json`. An explicitly given `path`
/// must still exist: naming a file that is not there stays an error rather
/// than silently running on defaults.
///
/// # Errors
///
/// Returns an error if the configuration file exists but cannot be read,
/// parsed or validated — or if `path` names a file that does not exist.
pub fn load_config_with_allow(
    path: Option<&Path>,
    allow: Vec<PathBuf>,
) -> Result<Config, ConfigError> {
    merge_allow(load_config(path), path.is_none(), allow)
}

/// The pure half of [`load_config_with_allow`]: grants `allow` on top of the
/// load result. `used_default_path` says the caller named no file, which is
/// the only case where a missing file is excused.
fn merge_allow(
    loaded: Result<Config, ConfigError>,
    used_default_path: bool,
    allow: Vec<PathBuf>,
) -> Result<Config, ConfigError> {
    if allow.is_empty() {
        return loaded;
    }
    let mut config = match loaded {
        Ok(config) => config,
        Err(ConfigError::NotFound { .. }) if used_default_path => Config::default(),
        Err(e) => return Err(e),
    };
    config.allowed_paths.extend(allow);
    Ok(config)
}

/// The system's Windows ANSI code page — the one an Altium on this machine
/// reads a `PcbLib`'s ANSI name fields through. `None` off Windows, or when it
/// cannot be read.
///
/// Read from the registry with `reg.exe` rather than `GetACP`: the crate
/// forbids unsafe code, and the registry value is the system page Altium's
/// process gets, whatever this process's own manifest says.
#[must_use]
pub fn system_ansi_code_page() -> Option<u32> {
    if !cfg!(windows) {
        return None;
    }
    // By absolute path, so a `reg.exe` planted on PATH is never run.
    let reg = PathBuf::from(std::env::var_os("SystemRoot")?)
        .join("System32")
        .join("reg.exe");
    let output = std::process::Command::new(reg)
        .args([
            "query",
            r"HKLM\SYSTEM\CurrentControlSet\Control\Nls\CodePage",
            "/v",
            "ACP",
        ])
        .output()
        .ok()?;
    parse_reg_acp(&String::from_utf8_lossy(&output.stdout))
}

/// Picks the code page out of `reg query … /v ACP` output, whose value line
/// reads `    ACP    REG_SZ    1250`.
fn parse_reg_acp(output: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next() == Some("ACP") && fields.next() == Some("REG_SZ"))
            .then(|| fields.next()?.parse().ok())
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_acp_output_yields_the_code_page() {
        let output = "\r\nHKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Nls\\CodePage\r\n    ACP    REG_SZ    936\r\n\r\n";
        assert_eq!(parse_reg_acp(output), Some(936));
        assert_eq!(
            parse_reg_acp("ERROR: The system was unable to find the key."),
            None
        );
        assert_eq!(parse_reg_acp("    ACP    REG_SZ    not-a-number"), None);
    }

    #[test]
    fn the_system_code_page_is_known_on_windows_only() {
        let page = system_ansi_code_page();
        if cfg!(windows) {
            assert!(page.is_some_and(|p| p > 0), "{page:?}");
        } else {
            assert_eq!(page, None);
        }
    }

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
    fn allow_alone_runs_on_defaults_without_any_config_file() {
        // The Claude Desktop extension path: no config file anywhere the
        // caller named, directories granted on the command line. Hermetic —
        // through the pure half, so the machine's real default config (which
        // load_config(None) would read) cannot leak in.
        let not_found = Err(ConfigError::NotFound {
            path: PathBuf::from("<default config path>"),
        });
        let config = merge_allow(not_found, true, vec![PathBuf::from("/tmp/libs")])
            .expect("--allow must not require a config file");
        assert_eq!(config.allowed_paths, vec![PathBuf::from("/tmp/libs")]);
        assert_eq!(config.logging.level, "warn");
        assert_eq!(config.rate_limit.max_burst, 120);
    }

    #[test]
    fn allow_extends_an_explicit_config_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), r#"{"allowed_paths":["/tmp/from-file"]}"#).unwrap();
        let config = load_config_with_allow(
            Some(file.path()),
            vec![PathBuf::from("/tmp/extra-a"), PathBuf::from("/tmp/extra-b")],
        )
        .expect("valid config with extra grants loads");
        assert_eq!(
            config.allowed_paths,
            vec![
                PathBuf::from("/tmp/from-file"),
                PathBuf::from("/tmp/extra-a"),
                PathBuf::from("/tmp/extra-b"),
            ]
        );
    }

    #[test]
    fn allow_does_not_excuse_a_named_config_file_that_is_missing() {
        // A caller who NAMED a file meant that file: running on defaults
        // instead would silently drop their logging and rate settings.
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.json");
        let err =
            load_config_with_allow(Some(&missing), vec![PathBuf::from("/tmp/libs")]).unwrap_err();
        assert!(matches!(err, ConfigError::NotFound { .. }));
    }

    #[test]
    fn allow_does_not_excuse_a_broken_config_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "{ not json ").unwrap();
        let err = load_config_with_allow(Some(file.path()), vec![PathBuf::from("/tmp/libs")])
            .unwrap_err();
        assert!(matches!(err, ConfigError::ParseError { .. }));
    }

    #[test]
    fn empty_allow_is_exactly_load_config() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.json");
        let err = load_config_with_allow(Some(&missing), Vec::new()).unwrap_err();
        assert!(matches!(err, ConfigError::NotFound { .. }));
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
