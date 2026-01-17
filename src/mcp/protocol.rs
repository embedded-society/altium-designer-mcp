//! JSON-RPC 2.0 message types for MCP protocol.
//!
//! This module defines the core message types used in the Model Context Protocol.
//! All messages follow the JSON-RPC 2.0 specification with MCP-specific extensions.
//!
//! # Message Types
//!
//! - **Request**: A message expecting a response (has `id`)
//! - **Response**: A reply to a request (success or error)
//! - **Notification**: A one-way message (no `id`, no response expected)
//!
//! # MCP-Specific Constraints
//!
//! - Request IDs must be strings or integers (never `null`)
//! - Request IDs must be unique within a session

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The MCP protocol version this implementation supports.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Server name for capability negotiation.
pub const SERVER_NAME: &str = "altium-designer-mcp";

/// A JSON-RPC 2.0 request ID.
///
/// Per the MCP specification, IDs must be strings or integers, never `null`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// Numeric request ID.
    Number(i64),
    /// String request ID.
    String(String),
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::String(s) => write!(f, "{s}"),
        }
    }
}

/// A JSON-RPC 2.0 request message.
///
/// Requests expect a response from the server.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    /// Must be "2.0".
    pub jsonrpc: String,

    /// Unique request identifier.
    pub id: RequestId,

    /// The method to invoke.
    pub method: String,

    /// Optional parameters for the method.
    #[serde(default)]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    /// Validates that this is a well-formed JSON-RPC 2.0 request.
    ///
    /// Returns an error message if validation fails.
    #[must_use]
    pub fn validate(&self) -> Option<&'static str> {
        if self.jsonrpc != "2.0" {
            return Some("jsonrpc field must be \"2.0\"");
        }
        if self.method.is_empty() {
            return Some("method field cannot be empty");
        }
        None
    }
}

/// A JSON-RPC 2.0 notification message (incoming).
///
/// Notifications do not have an ID and do not expect a response.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcNotification {
    /// Must be "2.0".
    pub jsonrpc: String,

    /// The notification method.
    pub method: String,

    /// Optional parameters for the notification.
    #[serde(default)]
    pub params: Option<Value>,
}

/// An outgoing JSON-RPC 2.0 notification (server to client).
///
/// Used for sending progress updates and other notifications.
#[derive(Debug, Clone, Serialize)]
pub struct OutgoingNotification {
    /// Always "2.0".
    pub jsonrpc: &'static str,

    /// The notification method.
    pub method: String,

    /// Optional parameters for the notification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl OutgoingNotification {
    /// Creates a new outgoing notification.
    #[must_use]
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params,
        }
    }

    /// Creates a progress notification.
    #[must_use]
    pub fn progress(
        progress_token: &str,
        progress: u32,
        total: Option<u32>,
        message: Option<&str>,
    ) -> Self {
        let params = serde_json::json!({
            "progressToken": progress_token,
            "progress": progress,
            "total": total,
            "message": message,
        });
        Self::new("notifications/progress", Some(params))
    }
}

/// A successful JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    /// Always "2.0".
    pub jsonrpc: &'static str,

    /// The request ID this response corresponds to.
    pub id: RequestId,

    /// The result of the method call.
    pub result: Value,
}

impl JsonRpcResponse {
    /// Creates a new success response.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // Value is not const-compatible
    pub fn success(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result,
        }
    }
}

/// Standard JSON-RPC 2.0 error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Invalid JSON was received by the server.
    ParseError,
    /// The JSON sent is not a valid Request object.
    InvalidRequest,
    /// The method does not exist or is not available.
    MethodNotFound,
    /// Invalid method parameters.
    InvalidParams,
    /// Internal JSON-RPC error.
    InternalError,
    /// Server-defined error.
    ServerError(i32),
}

impl ErrorCode {
    /// Returns the numeric code for this error.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::InternalError => -32603,
            Self::ServerError(code) => code,
        }
    }

    /// Returns the default message for this error code.
    #[must_use]
    pub const fn default_message(self) -> &'static str {
        match self {
            Self::ParseError => "Parse error",
            Self::InvalidRequest => "Invalid Request",
            Self::MethodNotFound => "Method not found",
            Self::InvalidParams => "Invalid params",
            Self::InternalError => "Internal error",
            Self::ServerError(_) => "Server error",
        }
    }
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcErrorData {
    /// The error code.
    pub code: i32,

    /// A short description of the error.
    pub message: String,

    /// Additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcErrorData {
    /// Creates a new error from an error code.
    #[must_use]
    pub fn from_code(code: ErrorCode) -> Self {
        Self {
            code: code.code(),
            message: code.default_message().to_string(),
            data: None,
        }
    }

    /// Creates a new error with a custom message.
    #[must_use]
    pub fn with_message(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: code.code(),
            message: message.into(),
            data: None,
        }
    }

    /// Adds additional data to the error.
    #[must_use]
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// A JSON-RPC 2.0 error response.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    /// Always "2.0".
    pub jsonrpc: &'static str,

    /// The request ID this error corresponds to (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,

    /// The error details.
    pub error: JsonRpcErrorData,
}

impl JsonRpcError {
    /// Creates a new error response.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // JsonRpcErrorData contains String
    pub fn new(id: Option<RequestId>, error: JsonRpcErrorData) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            error,
        }
    }

    /// Creates a parse error response (ID cannot be determined).
    #[must_use]
    pub fn parse_error() -> Self {
        Self::new(None, JsonRpcErrorData::from_code(ErrorCode::ParseError))
    }

    /// Creates an invalid request error response.
    #[must_use]
    pub fn invalid_request(id: Option<RequestId>) -> Self {
        Self::new(id, JsonRpcErrorData::from_code(ErrorCode::InvalidRequest))
    }

    /// Creates a method not found error response.
    #[must_use]
    pub fn method_not_found(id: RequestId, method: &str) -> Self {
        Self::new(
            Some(id),
            JsonRpcErrorData::with_message(
                ErrorCode::MethodNotFound,
                format!("Method not found: {method}"),
            ),
        )
    }

    /// Creates an invalid params error response.
    #[must_use]
    pub fn invalid_params(id: RequestId, message: impl Into<String>) -> Self {
        Self::new(
            Some(id),
            JsonRpcErrorData::with_message(ErrorCode::InvalidParams, message),
        )
    }

    /// Creates an internal error response.
    #[must_use]
    pub fn internal_error(id: RequestId, message: impl Into<String>) -> Self {
        Self::new(
            Some(id),
            JsonRpcErrorData::with_message(ErrorCode::InternalError, message),
        )
    }
}

/// An incoming message that could be either a request or notification.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum IncomingMessage {
    /// A request expecting a response.
    Request(JsonRpcRequest),
    /// A notification (no response expected).
    Notification(JsonRpcNotification),
}

impl IncomingMessage {
    /// Returns the method name of this message.
    #[must_use]
    pub fn method(&self) -> &str {
        match self {
            Self::Request(req) => &req.method,
            Self::Notification(notif) => &notif.method,
        }
    }

    /// Returns the parameters of this message.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // Option::as_ref is not const
    pub fn params(&self) -> Option<&Value> {
        match self {
            Self::Request(req) => req.params.as_ref(),
            Self::Notification(notif) => notif.params.as_ref(),
        }
    }

    /// Returns the request ID if this is a request.
    #[must_use]
    pub const fn id(&self) -> Option<&RequestId> {
        match self {
            Self::Request(req) => Some(&req.id),
            Self::Notification(_) => None,
        }
    }
}

/// Parses a JSON string into an incoming message.
///
/// # Errors
///
/// Returns a `JsonRpcError` if the JSON is malformed or not a valid message.
pub fn parse_message(json: &str) -> Result<IncomingMessage, JsonRpcError> {
    // First, try to parse as generic JSON to check structure
    let value: Value = serde_json::from_str(json).map_err(|_| JsonRpcError::parse_error())?;

    // Check if it's an object
    let obj = value.as_object().ok_or_else(JsonRpcError::parse_error)?;

    // Check for jsonrpc field
    let jsonrpc = obj
        .get("jsonrpc")
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError::invalid_request(None))?;

    if jsonrpc != "2.0" {
        return Err(JsonRpcError::invalid_request(None));
    }

    // Check if this is a request (has id) or notification (no id)
    if obj.contains_key("id") {
        // This is a request
        let request: JsonRpcRequest =
            serde_json::from_value(value).map_err(|_| JsonRpcError::invalid_request(None))?;

        if request.validate().is_some() {
            return Err(JsonRpcError::invalid_request(Some(request.id)));
        }

        Ok(IncomingMessage::Request(request))
    } else {
        // This is a notification
        let notification: JsonRpcNotification =
            serde_json::from_value(value).map_err(|_| JsonRpcError::invalid_request(None))?;

        Ok(IncomingMessage::Notification(notification))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_request() {
        let json = r#"{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}"#;
        let msg = parse_message(json).unwrap();

        let IncomingMessage::Request(req) = msg else {
            panic!("Expected Request, got Notification");
        };
        assert_eq!(req.id, RequestId::Number(1));
        assert_eq!(req.method, "initialize");
    }

    #[test]
    fn parse_valid_notification() {
        let json = r#"{"jsonrpc": "2.0", "method": "notifications/initialized"}"#;
        let msg = parse_message(json).unwrap();

        let IncomingMessage::Notification(notif) = msg else {
            panic!("Expected Notification, got Request");
        };
        assert_eq!(notif.method, "notifications/initialized");
    }

    #[test]
    fn parse_string_id() {
        let json = r#"{"jsonrpc": "2.0", "id": "abc-123", "method": "test"}"#;
        let msg = parse_message(json).unwrap();

        let IncomingMessage::Request(req) = msg else {
            panic!("Expected Request, got Notification");
        };
        assert_eq!(req.id, RequestId::String("abc-123".to_string()));
    }

    #[test]
    fn parse_invalid_json() {
        let json = "not valid json";
        let err = parse_message(json).unwrap_err();
        assert_eq!(err.error.code, ErrorCode::ParseError.code());
    }

    #[test]
    fn parse_missing_jsonrpc() {
        let json = r#"{"id": 1, "method": "test"}"#;
        let err = parse_message(json).unwrap_err();
        assert_eq!(err.error.code, ErrorCode::InvalidRequest.code());
    }

    #[test]
    fn parse_wrong_jsonrpc_version() {
        let json = r#"{"jsonrpc": "1.0", "id": 1, "method": "test"}"#;
        let err = parse_message(json).unwrap_err();
        assert_eq!(err.error.code, ErrorCode::InvalidRequest.code());
    }

    #[test]
    fn serialise_success_response() {
        let response =
            JsonRpcResponse::success(RequestId::Number(1), serde_json::json!({"ok": true}));
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""id":1"#));
        assert!(json.contains(r#""result":{"ok":true}"#));
    }

    #[test]
    fn serialise_error_response() {
        let error = JsonRpcError::method_not_found(RequestId::Number(1), "unknown/method");
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""id":1"#));
        assert!(json.contains(r#""code":-32601"#));
        assert!(json.contains("unknown/method"));
    }

    #[test]
    fn request_id_display() {
        assert_eq!(format!("{}", RequestId::Number(42)), "42");
        assert_eq!(format!("{}", RequestId::String("abc".to_string())), "abc");
    }
}
