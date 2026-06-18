//! Error types for Altium file operations.

use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Result type for Altium operations.
pub type AltiumResult<T> = Result<T, AltiumError>;

/// Renders only the final component of a path for client-facing error
/// messages.
///
/// Internal directory structure and atomic-write temp paths (e.g.
/// `…/MyLib.pcblib.tmp`) must never be disclosed to the MCP client. The full
/// path remains available in the structured error field for `tracing` at
/// debug level. Falls back to `<file>` when there is no final component.
#[must_use]
pub fn sanitise_path_for_client(path: &Path) -> String {
    path.file_name().map_or_else(
        || "<file>".to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Errors that can occur during Altium file operations.
#[derive(Debug, Error)]
pub enum AltiumError {
    /// Failed to open or read the file.
    ///
    /// Display shows only the file name, never the full path, to avoid
    /// leaking internal directory structure to the client.
    #[error("Failed to read file: {}", sanitise_path_for_client(.path))]
    FileRead {
        /// Path to the file.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Failed to write the file.
    ///
    /// Display shows only the file name, never the full path (in particular
    /// not the internal atomic-write temp path), to avoid leaking internal
    /// details to the client.
    #[error("Failed to write file: {}", sanitise_path_for_client(.path))]
    FileWrite {
        /// Path to the file.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Invalid OLE compound document structure.
    #[error("Invalid OLE structure: {message}")]
    InvalidOle {
        /// Description of what's wrong.
        message: String,
    },

    /// Missing required stream in the OLE document.
    #[error("Missing stream: {stream_name}")]
    MissingStream {
        /// Name of the missing stream.
        stream_name: String,
    },

    /// Failed to parse binary data.
    #[error("Parse error at offset {offset}: {message}")]
    ParseError {
        /// Byte offset where the error occurred.
        offset: usize,
        /// Description of what's wrong.
        message: String,
    },

    /// Invalid parameter value.
    #[error("Invalid parameter '{name}': {message}")]
    InvalidParameter {
        /// Parameter name.
        name: String,
        /// Description of what's wrong.
        message: String,
    },

    /// Component not found in library.
    #[error("Component not found: {name}")]
    ComponentNotFound {
        /// Component name that was not found.
        name: String,
    },

    /// Unsupported file version.
    #[error("Unsupported file version: {version}")]
    UnsupportedVersion {
        /// Version string from the file.
        version: String,
    },

    /// Compression or decompression failed.
    #[error("Compression error: {message}")]
    CompressionError {
        /// Description of what went wrong.
        message: String,
        /// Underlying I/O error if available.
        #[source]
        source: Option<io::Error>,
    },

    /// Wrong file type (e.g., opened `PcbLib` as `SchLib` or vice versa).
    #[error("Wrong file type: expected {expected}, got {actual}")]
    WrongFileType {
        /// Expected file type.
        expected: String,
        /// Actual file type detected.
        actual: String,
    },
}

impl AltiumError {
    /// Creates a file read error.
    pub fn file_read(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::FileRead {
            path: path.into(),
            source,
        }
    }

    /// Creates a file write error.
    pub fn file_write(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::FileWrite {
            path: path.into(),
            source,
        }
    }

    /// Creates an invalid OLE error.
    pub fn invalid_ole(message: impl Into<String>) -> Self {
        Self::InvalidOle {
            message: message.into(),
        }
    }

    /// Creates a missing stream error.
    pub fn missing_stream(stream_name: impl Into<String>) -> Self {
        Self::MissingStream {
            stream_name: stream_name.into(),
        }
    }

    /// Creates a parse error.
    pub fn parse_error(offset: usize, message: impl Into<String>) -> Self {
        Self::ParseError {
            offset,
            message: message.into(),
        }
    }

    /// Creates a compression error.
    pub fn compression_error(message: impl Into<String>, source: Option<io::Error>) -> Self {
        Self::CompressionError {
            message: message.into(),
            source,
        }
    }

    /// Creates a wrong file type error.
    pub fn wrong_file_type(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self::WrongFileType {
            expected: expected.into(),
            actual: actual.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = AltiumError::missing_stream("Data");
        assert_eq!(err.to_string(), "Missing stream: Data");
    }

    #[test]
    fn wrong_file_type_error_display() {
        let err = AltiumError::wrong_file_type("PcbLib", "SchLib (Schematic Library)");
        assert_eq!(
            err.to_string(),
            "Wrong file type: expected PcbLib, got SchLib (Schematic Library)"
        );
    }

    #[test]
    fn sanitise_path_strips_directory() {
        assert_eq!(
            sanitise_path_for_client(Path::new("/some/internal/dir/c.PcbLib")),
            "c.PcbLib"
        );
        assert_eq!(sanitise_path_for_client(Path::new("c.PcbLib")), "c.PcbLib");
    }

    #[test]
    fn file_write_error_does_not_leak_directory() {
        // The path field carries the internal atomic-write temp path, but the
        // client-facing Display must only show the file name.
        let dir = "/secret/internal/dir";
        let err = AltiumError::file_write(
            PathBuf::from(format!("{dir}/MyLib.pcblib.tmp")),
            io::Error::new(io::ErrorKind::PermissionDenied, "permission denied"),
        );
        let msg = err.to_string();
        assert!(!msg.contains(dir), "error message leaked directory: {msg}");
        assert!(msg.contains("MyLib.pcblib.tmp"), "message: {msg}");
    }

    #[test]
    fn file_read_error_does_not_leak_directory() {
        let dir = "/home/user/private/libs";
        let err = AltiumError::file_read(
            PathBuf::from(format!("{dir}/Parts.SchLib")),
            io::Error::new(io::ErrorKind::NotFound, "not found"),
        );
        let msg = err.to_string();
        assert!(!msg.contains(dir), "error message leaked directory: {msg}");
        assert!(msg.contains("Parts.SchLib"), "message: {msg}");
    }
}
