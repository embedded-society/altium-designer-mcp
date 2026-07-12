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

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::mcp::protocol::{JsonRpcError, JsonRpcResponse, OutgoingNotification};

/// Strips a single trailing `\n` and an optional preceding `\r` from a line.
fn strip_trailing_newline(line: &mut String) {
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
}

/// Reads one newline-delimited message from an async buffered reader.
///
/// Returns `None` on EOF. The trailing newline (and any preceding `\r`) is
/// stripped. There is intentionally no line-length cap: some tools carry
/// base64-encoded payloads (e.g. embedded STEP models via `write_pcblib`, or
/// `extract_step_model` output) inline on a single JSON-RPC line, so a message
/// may be multiple megabytes.
async fn read_message_line<R>(reader: &mut R) -> io::Result<Option<String>>
where
    R: AsyncBufRead + Unpin,
{
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        // EOF.
        return Ok(None);
    }
    strip_trailing_newline(&mut line);
    Ok(Some(line))
}

/// Writes a single JSON message line to an async writer, terminated by a
/// newline and flushed. Per the MCP spec a message must not contain embedded
/// newlines.
async fn write_message_line<W>(writer: &mut W, json: &str) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    debug_assert!(
        !json.contains('\n'),
        "JSON message must not contain embedded newlines"
    );
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

/// A stdio-based MCP transport.
///
/// Handles reading JSON-RPC messages from stdin and writing responses to stdout.
pub struct StdioTransport {
    /// Buffered reader for stdin.
    reader: BufReader<tokio::io::Stdin>,
    /// Handle for stdout.
    writer: tokio::io::Stdout,
    /// Under `cfg(test)`, redirects writes to an in-memory buffer instead of
    /// the real stdout so the write path can be asserted without a live pipe
    /// (and without the intermittent stdout-teardown hangs seen on Windows CI).
    /// Compiled out of non-test builds entirely.
    #[cfg(test)]
    test_sink: Option<std::sync::Arc<std::sync::Mutex<Vec<u8>>>>,
}

impl StdioTransport {
    /// Creates a new stdio transport.
    #[must_use]
    pub fn new() -> Self {
        Self {
            reader: BufReader::new(tokio::io::stdin()),
            writer: tokio::io::stdout(),
            #[cfg(test)]
            test_sink: None,
        }
    }

    /// Redirects subsequent writes into an in-memory buffer and returns a handle
    /// to it, so tests can assert on the exact framed bytes.
    #[cfg(test)]
    pub(crate) fn capture_output(&mut self) -> std::sync::Arc<std::sync::Mutex<Vec<u8>>> {
        let sink = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        self.test_sink = Some(std::sync::Arc::clone(&sink));
        sink
    }

    /// Reads the next message line from stdin.
    ///
    /// Returns `None` if stdin is closed (EOF).
    ///
    /// # Errors
    ///
    /// Returns an error if reading from stdin fails.
    pub async fn read_line(&mut self) -> io::Result<Option<String>> {
        read_message_line(&mut self.reader).await
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
        #[cfg(test)]
        if let Some(sink) = &self.test_sink {
            // Mirror write_message_line's framing (message + single '\n') without
            // holding the lock across an await point.
            let mut framed = json.as_bytes().to_vec();
            framed.push(b'\n');
            sink.lock()
                .expect("test sink mutex poisoned")
                .extend_from_slice(&framed);
            return Ok(());
        }
        write_message_line(&mut self.writer, json).await
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

    #[test]
    fn strip_trailing_newline_variants() {
        let mut s = "x\n".to_string();
        strip_trailing_newline(&mut s);
        assert_eq!(s, "x");

        let mut s = "x\r\n".to_string();
        strip_trailing_newline(&mut s);
        assert_eq!(s, "x");

        let mut s = "x".to_string();
        strip_trailing_newline(&mut s);
        assert_eq!(s, "x");

        let mut s = String::new();
        strip_trailing_newline(&mut s);
        assert_eq!(s, "");
    }

    #[tokio::test]
    async fn read_message_line_splits_and_strips() {
        use std::io::Cursor;

        let mut reader = Cursor::new(b"hello\r\nworld\n".to_vec());
        assert_eq!(
            read_message_line(&mut reader).await.unwrap().as_deref(),
            Some("hello")
        );
        assert_eq!(
            read_message_line(&mut reader).await.unwrap().as_deref(),
            Some("world")
        );
        // EOF.
        assert_eq!(read_message_line(&mut reader).await.unwrap(), None);
    }

    #[tokio::test]
    async fn write_message_line_appends_single_newline() {
        let mut buf: Vec<u8> = Vec::new();
        write_message_line(&mut buf, r#"{"a":1}"#).await.unwrap();
        assert_eq!(buf, b"{\"a\":1}\n");
    }

    /// Decodes the single framed line captured in a test sink.
    fn sink_json(sink: &std::sync::Arc<std::sync::Mutex<Vec<u8>>>) -> serde_json::Value {
        let bytes = sink.lock().unwrap().clone();
        let s = String::from_utf8(bytes).expect("utf-8 output");
        assert!(s.ends_with('\n'), "output must be newline-terminated");
        assert_eq!(s.matches('\n').count(), 1, "exactly one framed message");
        serde_json::from_str(s.trim_end()).expect("valid JSON line")
    }

    #[tokio::test]
    async fn write_response_frames_success_envelope() {
        let mut t = StdioTransport::new();
        let sink = t.capture_output();
        let resp = JsonRpcResponse::success(RequestId::Number(7), serde_json::json!({"ok": true}));
        t.write_response(&resp).await.unwrap();

        let v = sink_json(&sink);
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 7);
        assert_eq!(v["result"]["ok"], true);
    }

    #[tokio::test]
    async fn write_error_frames_error_object() {
        let mut t = StdioTransport::new();
        let sink = t.capture_output();
        let err = JsonRpcError::method_not_found(RequestId::Number(1), "nope/method");
        t.write_error(&err).await.unwrap();

        let v = sink_json(&sink);
        assert_eq!(v["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn write_notification_frames_method_and_params() {
        let mut t = StdioTransport::new();
        let sink = t.capture_output();
        let note = OutgoingNotification::new(
            "notifications/progress",
            Some(serde_json::json!({"progress": 42})),
        );
        t.write_notification(&note).await.unwrap();

        let v = sink_json(&sink);
        assert_eq!(v["method"], "notifications/progress");
        assert_eq!(v["params"]["progress"], 42);
        // Notifications carry no id.
        assert!(v.get("id").is_none());
    }

    #[tokio::test]
    async fn write_json_frames_arbitrary_value() {
        let mut t = StdioTransport::new();
        let sink = t.capture_output();
        t.write_json(&serde_json::json!({"custom": [1, 2, 3]}))
            .await
            .unwrap();

        let v = sink_json(&sink);
        assert_eq!(v["custom"], serde_json::json!([1, 2, 3]));
    }

    #[tokio::test]
    async fn multiple_writes_accumulate_in_order() {
        let mut t = StdioTransport::new();
        let sink = t.capture_output();
        t.write_json(&serde_json::json!({"n": 1})).await.unwrap();
        t.write_json(&serde_json::json!({"n": 2})).await.unwrap();

        let bytes = sink.lock().unwrap().clone();
        let s = String::from_utf8(bytes).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(lines[0]).unwrap()["n"],
            1
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(lines[1]).unwrap()["n"],
            2
        );
    }
}
