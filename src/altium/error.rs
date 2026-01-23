//! Error types for Altium file operations.

use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// Result type for Altium operations.
pub type AltiumResult<T> = Result<T, AltiumError>;

/// Errors that can occur during Altium file operations.
#[derive(Debug, Error)]
pub enum AltiumError {
    /// Failed to open or read the file.
    #[error("Failed to read file: {path}")]
    FileRead {
        /// Path to the file.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Failed to write the file.
    #[error("Failed to write file: {path}")]
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
}
