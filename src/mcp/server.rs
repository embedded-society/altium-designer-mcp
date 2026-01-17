//! MCP server implementation for Altium Designer library management.
//!
//! This module implements the MCP server lifecycle:
//!
//! 1. **Initialisation**: Capability negotiation and version agreement
//! 2. **Operation**: Handling tool calls and other requests
//! 3. **Shutdown**: Graceful connection termination

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::mcp::protocol::{
    ErrorCode, IncomingMessage, JsonRpcError, JsonRpcErrorData, JsonRpcNotification,
    JsonRpcRequest, JsonRpcResponse, RequestId, MCP_PROTOCOL_VERSION, SERVER_NAME,
};
use crate::mcp::transport::StdioTransport;

/// Server state in the MCP lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    /// Waiting for initialize request.
    AwaitingInit,
    /// Initialize received, waiting for initialized notification.
    Initialising,
    /// Ready for normal operation.
    Running,
    /// Shutdown in progress.
    ShuttingDown,
}

/// Server capabilities advertised during initialisation.
#[derive(Debug, Clone, Serialize)]
pub struct ServerCapabilities {
    /// Tool-related capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolCapabilities>,
}

impl Default for ServerCapabilities {
    fn default() -> Self {
        Self {
            tools: Some(ToolCapabilities::default()),
        }
    }
}

/// Tool-specific capabilities.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ToolCapabilities {
    /// Whether the tool list can change during the session.
    #[serde(rename = "listChanged", skip_serializing_if = "std::ops::Not::not")]
    pub list_changed: bool,
}

/// Server information for initialisation response.
#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    /// Server name.
    pub name: String,
    /// Server version.
    pub version: String,
}

impl Default for ServerInfo {
    fn default() -> Self {
        Self {
            name: SERVER_NAME.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Client information received during initialisation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    /// Client name.
    pub name: String,
    /// Client version.
    #[serde(default)]
    pub version: Option<String>,
}

/// Parameters for the initialize request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// Protocol version requested by client.
    pub protocol_version: String,
    /// Client capabilities.
    #[serde(default)]
    pub capabilities: Value,
    /// Client information.
    #[serde(default)]
    pub client_info: Option<ClientInfo>,
}

/// A tool definition for tools/list response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    /// Unique tool name.
    pub name: String,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: Value,
}

/// Parameters for tools/call request.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallParams {
    /// Name of the tool to call.
    pub name: String,
    /// Arguments for the tool.
    #[serde(default)]
    pub arguments: Value,
}

/// Content item in a tool call response.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolContent {
    /// Text content.
    Text {
        /// The text content.
        text: String,
    },
}

/// Result of a tool call.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallResult {
    /// Content returned by the tool.
    pub content: Vec<ToolContent>,
    /// Whether the tool call resulted in an error.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

impl ToolCallResult {
    /// Creates a successful text result.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text { text: text.into() }],
            is_error: false,
        }
    }

    /// Creates an error text result.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text {
                text: message.into(),
            }],
            is_error: true,
        }
    }
}

/// The MCP server for Altium Designer library management.
pub struct McpServer {
    /// Current server state.
    state: ServerState,
    /// The transport layer.
    transport: StdioTransport,
    /// Negotiated protocol version (set after initialisation).
    protocol_version: Option<String>,
    /// Path to the component library directory.
    library_path: PathBuf,
}

impl McpServer {
    /// Creates a new MCP server with the given library path.
    #[must_use]
    pub fn new(library_path: PathBuf) -> Self {
        Self {
            state: ServerState::AwaitingInit,
            transport: StdioTransport::new(),
            protocol_version: None,
            library_path,
        }
    }

    /// Returns the current server state.
    #[must_use]
    pub const fn state(&self) -> ServerState {
        self.state
    }

    /// Runs the MCP server main loop with graceful shutdown handling.
    ///
    /// # Errors
    ///
    /// Returns an error if transport I/O fails.
    pub async fn run(&mut self) -> std::io::Result<()> {
        self.run_with_shutdown().await
    }

    /// Runs the main loop and handles shutdown.
    #[cfg(unix)]
    async fn run_with_shutdown(&mut self) -> std::io::Result<()> {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigint = signal(SignalKind::interrupt()).map_err(std::io::Error::other)?;
        let mut sigterm = signal(SignalKind::terminate()).map_err(std::io::Error::other)?;

        loop {
            tokio::select! {
                _ = sigint.recv() => {
                    tracing::info!("Received SIGINT, initiating graceful shutdown");
                    self.state = ServerState::ShuttingDown;
                    return Ok(());
                }

                _ = sigterm.recv() => {
                    tracing::info!("Received SIGTERM, initiating graceful shutdown");
                    self.state = ServerState::ShuttingDown;
                    return Ok(());
                }

                line_result = self.transport.read_line() => {
                    if self.handle_transport_result(line_result).await? {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Runs the main loop and handles shutdown.
    #[cfg(windows)]
    async fn run_with_shutdown(&mut self) -> std::io::Result<()> {
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);

        loop {
            tokio::select! {
                _ = &mut ctrl_c => {
                    tracing::info!("Received Ctrl+C, initiating graceful shutdown");
                    self.state = ServerState::ShuttingDown;
                    return Ok(());
                }

                line_result = self.transport.read_line() => {
                    if self.handle_transport_result(line_result).await? {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Handles the result from transport read.
    ///
    /// Returns `true` if the server should shut down.
    async fn handle_transport_result(
        &mut self,
        line_result: std::io::Result<Option<String>>,
    ) -> std::io::Result<bool> {
        let Some(line) = line_result? else {
            self.state = ServerState::ShuttingDown;
            return Ok(true);
        };

        if line.trim().is_empty() {
            return Ok(false);
        }

        self.handle_line(&line).await?;

        if self.state == ServerState::ShuttingDown {
            return Ok(true);
        }

        Ok(false)
    }

    /// Handles a single line of input.
    async fn handle_line(&mut self, line: &str) -> std::io::Result<()> {
        use crate::mcp::protocol::parse_message;

        match parse_message(line) {
            Ok(msg) => self.handle_message(msg).await,
            Err(error) => {
                self.transport.write_error(&error).await?;
                Ok(())
            }
        }
    }

    /// Handles a parsed incoming message.
    async fn handle_message(&mut self, msg: IncomingMessage) -> std::io::Result<()> {
        match msg {
            IncomingMessage::Request(req) => self.handle_request(req).await,
            IncomingMessage::Notification(ref notif) => {
                self.handle_notification(notif);
                Ok(())
            }
        }
    }

    /// Handles an incoming request.
    async fn handle_request(&mut self, req: JsonRpcRequest) -> std::io::Result<()> {
        let response = match req.method.as_str() {
            "initialize" => self.handle_initialize(&req),
            "tools/list" => self.handle_tools_list(&req),
            "tools/call" => self.handle_tools_call(&req),
            "ping" => Ok(Self::handle_ping(&req)),
            _ => Err(JsonRpcError::method_not_found(req.id.clone(), &req.method)),
        };

        match response {
            Ok(resp) => self.transport.write_response(&resp).await,
            Err(error) => self.transport.write_error(&error).await,
        }
    }

    /// Handles an incoming notification.
    fn handle_notification(&mut self, notif: &JsonRpcNotification) {
        if notif.method == "notifications/initialized" && self.state == ServerState::Initialising {
            self.state = ServerState::Running;
        }
    }

    /// Handles the initialize request.
    fn handle_initialize(&mut self, req: &JsonRpcRequest) -> Result<JsonRpcResponse, JsonRpcError> {
        if self.state != ServerState::AwaitingInit {
            return Err(JsonRpcError::new(
                Some(req.id.clone()),
                JsonRpcErrorData::with_message(
                    ErrorCode::InvalidRequest,
                    "Server already initialised",
                ),
            ));
        }

        let _params: InitializeParams = req
            .params
            .as_ref()
            .map(|p| serde_json::from_value(p.clone()))
            .transpose()
            .map_err(|e| {
                JsonRpcError::invalid_params(
                    req.id.clone(),
                    format!("Invalid initialize params: {e}"),
                )
            })?
            .ok_or_else(|| {
                JsonRpcError::invalid_params(req.id.clone(), "Missing initialize params")
            })?;

        let negotiated_version = MCP_PROTOCOL_VERSION.to_string();

        self.protocol_version = Some(negotiated_version.clone());
        self.state = ServerState::Initialising;

        let result = json!({
            "protocolVersion": negotiated_version,
            "capabilities": ServerCapabilities::default(),
            "serverInfo": ServerInfo::default(),
        });

        Ok(JsonRpcResponse::success(req.id.clone(), result))
    }

    /// Handles the tools/list request.
    fn handle_tools_list(&self, req: &JsonRpcRequest) -> Result<JsonRpcResponse, JsonRpcError> {
        self.require_running(&req.id)?;

        let tools = Self::get_tool_definitions();

        let result = json!({
            "tools": tools,
        });

        Ok(JsonRpcResponse::success(req.id.clone(), result))
    }

    /// Handles the tools/call request.
    fn handle_tools_call(&self, req: &JsonRpcRequest) -> Result<JsonRpcResponse, JsonRpcError> {
        self.require_running(&req.id)?;

        let params: ToolCallParams = req
            .params
            .as_ref()
            .map(|p| serde_json::from_value(p.clone()))
            .transpose()
            .map_err(|e| {
                JsonRpcError::invalid_params(
                    req.id.clone(),
                    format!("Invalid tool call params: {e}"),
                )
            })?
            .ok_or_else(|| {
                JsonRpcError::invalid_params(req.id.clone(), "Missing tool call params")
            })?;

        let result = match params.name.as_str() {
            "list_package_types" => self.call_list_package_types(),
            "calculate_footprint" => self.call_calculate_footprint(&params.arguments),
            "get_ipc_name" => self.call_get_ipc_name(&params.arguments),
            _ => ToolCallResult::error(format!("Unknown tool: {}", params.name)),
        };

        let result_value = serde_json::to_value(&result).map_err(|e| {
            tracing::error!(error = %e, "Failed to serialise tool call result");
            JsonRpcError::new(
                Some(req.id.clone()),
                JsonRpcErrorData::with_message(
                    ErrorCode::InternalError,
                    "Internal error: failed to serialise result",
                ),
            )
        })?;

        Ok(JsonRpcResponse::success(req.id.clone(), result_value))
    }

    /// Handles the ping request.
    fn handle_ping(req: &JsonRpcRequest) -> JsonRpcResponse {
        JsonRpcResponse::success(req.id.clone(), json!({}))
    }

    /// Ensures the server is in the Running state.
    fn require_running(&self, id: &RequestId) -> Result<(), JsonRpcError> {
        if self.state != ServerState::Running {
            return Err(JsonRpcError::new(
                Some(id.clone()),
                JsonRpcErrorData::with_message(ErrorCode::InvalidRequest, "Server not initialised"),
            ));
        }
        Ok(())
    }

    /// Returns the list of available tools.
    fn get_tool_definitions() -> Vec<ToolDefinition> {
        vec![
            // IPC-7351B Tools
            ToolDefinition {
                name: "list_package_types".to_string(),
                description: Some(
                    "List all supported IPC-7351B package types with descriptions. \
                     Use this to discover available package families before calculating footprints."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            ToolDefinition {
                name: "calculate_footprint".to_string(),
                description: Some(
                    "Calculate IPC-7351B compliant land pattern from package dimensions. \
                     Returns pad positions, sizes, courtyard, and silkscreen geometry."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "package_type": {
                            "type": "string",
                            "description": "Package type: CHIP, SOIC, QFP, QFN, BGA, SOT, MELF"
                        },
                        "body_length": {
                            "type": "number",
                            "description": "Component body length in mm"
                        },
                        "body_width": {
                            "type": "number",
                            "description": "Component body width in mm"
                        },
                        "lead_span": {
                            "type": "number",
                            "description": "Lead span (toe-to-toe) in mm"
                        },
                        "lead_width": {
                            "type": "number",
                            "description": "Lead width in mm"
                        },
                        "pitch": {
                            "type": "number",
                            "description": "Lead pitch in mm (for multi-pin packages)"
                        },
                        "pin_count": {
                            "type": "integer",
                            "description": "Total pin count"
                        },
                        "density": {
                            "type": "string",
                            "description": "Density level: M (Most), N (Nominal), L (Least). Default: N"
                        }
                    },
                    "required": ["package_type", "body_length", "body_width", "pin_count"]
                }),
            },
            ToolDefinition {
                name: "get_ipc_name".to_string(),
                description: Some(
                    "Generate IPC-7351B compliant component name from package parameters. \
                     Example: SOIC127P600X175-8N"
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "package_type": {
                            "type": "string",
                            "description": "Package type: CHIP, SOIC, QFP, QFN, BGA, SOT"
                        },
                        "pitch": {
                            "type": "number",
                            "description": "Lead pitch in mm"
                        },
                        "body_length": {
                            "type": "number",
                            "description": "Body length in mm"
                        },
                        "body_width": {
                            "type": "number",
                            "description": "Body width in mm"
                        },
                        "height": {
                            "type": "number",
                            "description": "Component height in mm"
                        },
                        "pin_count": {
                            "type": "integer",
                            "description": "Total pin count"
                        },
                        "density": {
                            "type": "string",
                            "description": "Density level: M, N, or L. Default: N"
                        }
                    },
                    "required": ["package_type", "body_length", "body_width", "pin_count"]
                }),
            },
        ]
    }

    // ==================== Tool Handlers ====================

    /// Lists supported IPC-7351B package types.
    fn call_list_package_types(&self) -> ToolCallResult {
        let package_types = json!({
            "package_types": [
                {
                    "type": "CHIP",
                    "description": "Chip resistors, capacitors, inductors (0201, 0402, 0603, 0805, 1206, etc.)",
                    "examples": ["0201", "0402", "0603", "0805", "1206", "1210", "2512"]
                },
                {
                    "type": "SOIC",
                    "description": "Small Outline Integrated Circuit",
                    "variants": ["SOIC", "SSOP", "TSSOP", "MSOP"]
                },
                {
                    "type": "QFP",
                    "description": "Quad Flat Package",
                    "variants": ["QFP", "LQFP", "TQFP"]
                },
                {
                    "type": "QFN",
                    "description": "Quad Flat No-Lead package",
                    "variants": ["QFN", "DFN", "SON", "VSON"]
                },
                {
                    "type": "BGA",
                    "description": "Ball Grid Array",
                    "variants": ["BGA", "CSP", "WLCSP"]
                },
                {
                    "type": "SOT",
                    "description": "Small Outline Transistor",
                    "variants": ["SOT-23", "SOT-223", "SOT-363", "SOT-23-5", "SOT-23-6"]
                },
                {
                    "type": "MELF",
                    "description": "Metal Electrode Leadless Face",
                    "examples": ["MiniMELF", "MELF", "SOD-80"]
                },
                {
                    "type": "SMA",
                    "description": "Surface Mount Assembly diodes",
                    "variants": ["SMA", "SMB", "SMC", "SOD-123", "SOD-323"]
                }
            ],
            "density_levels": {
                "M": "Most (largest pads, maximum solder fillet)",
                "N": "Nominal (standard density, recommended for most applications)",
                "L": "Least (smallest pads, minimum solder fillet, for high-density boards)"
            }
        });

        ToolCallResult::text(serde_json::to_string_pretty(&package_types).unwrap())
    }

    /// Calculates IPC-7351B compliant footprint (placeholder).
    fn call_calculate_footprint(&self, arguments: &Value) -> ToolCallResult {
        // TODO: Implement actual IPC-7351B calculations
        let package_type = arguments
            .get("package_type")
            .and_then(Value::as_str)
            .unwrap_or("CHIP");
        let body_length = arguments
            .get("body_length")
            .and_then(Value::as_f64)
            .unwrap_or(1.6);
        let body_width = arguments
            .get("body_width")
            .and_then(Value::as_f64)
            .unwrap_or(0.8);
        let pin_count = arguments
            .get("pin_count")
            .and_then(Value::as_u64)
            .unwrap_or(2);
        let density = arguments
            .get("density")
            .and_then(Value::as_str)
            .unwrap_or("N");

        let result = json!({
            "status": "placeholder",
            "message": "IPC-7351B calculation not yet implemented",
            "input": {
                "package_type": package_type,
                "body_length": body_length,
                "body_width": body_width,
                "pin_count": pin_count,
                "density": density
            },
            "library_path": self.library_path.display().to_string(),
            "note": "This is a placeholder response. Full IPC-7351B calculations will be implemented in future versions."
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Generates IPC-7351B compliant name (placeholder).
    fn call_get_ipc_name(&self, arguments: &Value) -> ToolCallResult {
        // TODO: Implement actual IPC naming convention
        let package_type = arguments
            .get("package_type")
            .and_then(Value::as_str)
            .unwrap_or("CHIP");
        let body_length = arguments
            .get("body_length")
            .and_then(Value::as_f64)
            .unwrap_or(1.6);
        let body_width = arguments
            .get("body_width")
            .and_then(Value::as_f64)
            .unwrap_or(0.8);
        let height = arguments
            .get("height")
            .and_then(Value::as_f64)
            .unwrap_or(0.55);
        let pin_count = arguments
            .get("pin_count")
            .and_then(Value::as_u64)
            .unwrap_or(2);
        let density = arguments
            .get("density")
            .and_then(Value::as_str)
            .unwrap_or("N");

        // Generate placeholder IPC name
        let ipc_name = format!(
            "{}{}X{}X{}-{}{}",
            package_type.to_uppercase(),
            (body_length * 100.0) as u32,
            (body_width * 100.0) as u32,
            (height * 100.0) as u32,
            pin_count,
            density.to_uppercase()
        );

        let result = json!({
            "ipc_name": ipc_name,
            "components": {
                "package_type": package_type,
                "body_length_mm": body_length,
                "body_width_mm": body_width,
                "height_mm": height,
                "pin_count": pin_count,
                "density_level": density
            },
            "note": "This is a simplified placeholder. Full IPC-7351B naming will be implemented."
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_initial_state() {
        let server = McpServer::new(PathBuf::from("."));
        assert_eq!(server.state(), ServerState::AwaitingInit);
    }

    #[test]
    fn tool_definitions_valid() {
        let tools = McpServer::get_tool_definitions();
        assert!(!tools.is_empty());

        for tool in &tools {
            assert!(!tool.name.is_empty());
            assert!(tool.input_schema.is_object());
        }
    }

    #[test]
    fn tool_call_result_text() {
        let result = ToolCallResult::text("Hello, world!");
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);

        match &result.content[0] {
            ToolContent::Text { text } => assert_eq!(text, "Hello, world!"),
        }
    }

    #[test]
    fn tool_call_result_error() {
        let result = ToolCallResult::error("Something went wrong");
        assert!(result.is_error);
        assert_eq!(result.content.len(), 1);

        match &result.content[0] {
            ToolContent::Text { text } => assert_eq!(text, "Something went wrong"),
        }
    }
}
