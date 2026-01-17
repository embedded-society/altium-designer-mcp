//! stdio transport for MCP server.
//!
//! This module implements the stdio transport as specified by MCP:
//!
//! - Messages are UTF-8 encoded JSON-RPC
//! - Messages are delimited by newlines
//! - Messages must not contain embedded newlines
//! - stdin: receives messages from client
//! - stdout: sends messages to client
//! - stderr: may be used for logging (not MCP messages)
//!
//! # Thread Safety
//!
//! The transport uses async I/O with Tokio. Reading and writing are
//! handled through separate tasks to allow concurrent operation.

use std::io;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::mcp::protocol::{JsonRpcError, JsonRpcResponse, OutgoingNotification};

/// A stdio-based MCP transport.
///
/// Handles reading JSON-RPC messages from stdin and writing responses to stdout.
pub struct StdioTransport {
    /// Buffered reader for stdin.
    reader: BufReader<tokio::io::Stdin>,
    /// Handle for stdout.
    writer: tokio::io::Stdout,
}

impl StdioTransport {
    /// Creates a new stdio transport.
    #[must_use]
    pub fn new() -> Self {
        Self {
            reader: BufReader::new(tokio::io::stdin()),
            writer: tokio::io::stdout(),
        }
    }

    /// Reads the next message line from stdin.
    ///
    /// Returns `None` if stdin is closed (EOF).
    ///
    /// # Errors
    ///
    /// Returns an error if reading from stdin fails.
    pub async fn read_line(&mut self) -> io::Result<Option<String>> {
        let mut line = String::new();
        let bytes_read = self.reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            // EOF - stdin closed
            return Ok(None);
        }

        // Remove the trailing newline
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }

        Ok(Some(line))
    }

    /// Writes a JSON-RPC response to stdout.
    ///
    /// The response is serialised to JSON and terminated with a newline.
    ///
    /// # Errors
    ///
    /// Returns an error if serialisation or writing fails.
    pub async fn write_response(&mut self, response: &JsonRpcResponse) -> io::Result<()> {
        let json = serde_json::to_string(response)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.write_raw(&json).await
    }

    /// Writes a JSON-RPC error to stdout.
    ///
    /// # Errors
    ///
    /// Returns an error if serialisation or writing fails.
    pub async fn write_error(&mut self, error: &JsonRpcError) -> io::Result<()> {
        let json = serde_json::to_string(error)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.write_raw(&json).await
    }

    /// Writes a JSON-RPC notification to stdout.
    ///
    /// Used for sending progress updates and other server-initiated messages.
    ///
    /// # Errors
    ///
    /// Returns an error if serialisation or writing fails.
    pub async fn write_notification(
        &mut self,
        notification: &OutgoingNotification,
    ) -> io::Result<()> {
        let json = serde_json::to_string(notification)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.write_raw(&json).await
    }

    /// Writes a raw JSON string to stdout with newline termination.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    async fn write_raw(&mut self, json: &str) -> io::Result<()> {
        // MCP spec: messages must not contain embedded newlines
        debug_assert!(
            !json.contains('\n'),
            "JSON message must not contain embedded newlines"
        );

        self.writer.write_all(json.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;

        Ok(())
    }

    /// Writes an arbitrary JSON value to stdout.
    ///
    /// Used for sending messages that don't fit the standard response types.
    ///
    /// # Errors
    ///
    /// Returns an error if serialisation or writing fails.
    pub async fn write_json(&mut self, value: &serde_json::Value) -> io::Result<()> {
        let json = serde_json::to_string(value)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.write_raw(&json).await
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::protocol::RequestId;

    #[test]
    fn transport_default() {
        // Just ensure Default is implemented and doesn't panic
        let _transport = StdioTransport::default();
    }

    #[tokio::test]
    async fn serialise_response_no_newlines() {
        // Verify our JSON serialisation doesn't produce embedded newlines
        let response = JsonRpcResponse::success(
            RequestId::Number(1),
            serde_json::json!({
                "message": "hello world",
                "nested": {"key": "value"}
            }),
        );

        let json = serde_json::to_string(&response).unwrap();
        assert!(
            !json.contains('\n'),
            "Serialised JSON should not contain newlines"
        );
    }

    #[tokio::test]
    async fn serialise_error_no_newlines() {
        let error = JsonRpcError::method_not_found(RequestId::Number(1), "test/method");

        let json = serde_json::to_string(&error).unwrap();
        assert!(
            !json.contains('\n'),
            "Serialised JSON should not contain newlines"
        );
    }
}
