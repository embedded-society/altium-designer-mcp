//! MCP server implementation for Altium Designer library management.
//!
//! This module implements the MCP server lifecycle:
//!
//! 1. **Initialisation**: Capability negotiation and version agreement
//! 2. **Operation**: Handling tool calls and other requests
//! 3. **Shutdown**: Graceful connection termination
//!
//! # Architecture
//!
//! This server provides low-level file I/O and primitive placement tools.
//! The AI handles the intelligence (IPC calculations, style decisions, etc.).

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
    #[serde(rename = "listChanged", skip_serializing_if = "is_false")]
    pub list_changed: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde requires &T signature
const fn is_false(b: &bool) -> bool {
    !*b
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
    #[serde(skip_serializing_if = "is_false")]
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
    /// Allowed paths for library operations.
    allowed_paths: Vec<PathBuf>,
}

impl McpServer {
    /// Creates a new MCP server with the given allowed paths.
    #[must_use]
    pub fn new(allowed_paths: Vec<PathBuf>) -> Self {
        Self {
            state: ServerState::AwaitingInit,
            transport: StdioTransport::new(),
            protocol_version: None,
            allowed_paths,
        }
    }

    /// Returns the current server state.
    #[must_use]
    pub const fn state(&self) -> ServerState {
        self.state
    }

    /// Validates that a path is within one of the allowed paths.
    ///
    /// Returns `Ok(())` if the path is allowed, or an error message if not.
    fn validate_path(&self, filepath: &str) -> Result<(), String> {
        use std::path::Path;

        // If no allowed paths are configured, allow all paths (backwards compatibility)
        if self.allowed_paths.is_empty() {
            return Ok(());
        }

        let path = Path::new(filepath);

        // Try to canonicalize the path. If it doesn't exist yet (for write operations),
        // canonicalize the parent directory and append the filename.
        let canonical_path = if path.exists() {
            path.canonicalize()
                .map_err(|e| format!("Failed to resolve path: {e}"))?
        } else {
            // For new files, check the parent directory
            let parent = path
                .parent()
                .ok_or_else(|| "Invalid path: no parent directory".to_string())?;
            let filename = path
                .file_name()
                .ok_or_else(|| "Invalid path: no filename".to_string())?;
            let canonical_parent = parent
                .canonicalize()
                .map_err(|e| format!("Failed to resolve parent directory: {e}"))?;
            canonical_parent.join(filename)
        };

        // Check if the path is within any of the allowed paths
        for allowed in &self.allowed_paths {
            let Ok(canonical_allowed) = allowed.canonicalize() else {
                continue; // Skip non-existent allowed paths
            };

            if canonical_path.starts_with(&canonical_allowed) {
                return Ok(());
            }
        }

        // Path is not within any allowed path - return error without exposing internal paths
        Err("Access denied: path is outside the configured allowed directories".to_string())
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
            // Library I/O tools
            "read_pcblib" => self.call_read_pcblib(&params.arguments),
            "write_pcblib" => self.call_write_pcblib(&params.arguments),
            "read_schlib" => self.call_read_schlib(&params.arguments),
            "write_schlib" => self.call_write_schlib(&params.arguments),
            "list_components" => self.call_list_components(&params.arguments),
            "extract_style" => self.call_extract_style(&params.arguments),
            // Library management tools
            "delete_component" => self.call_delete_component(&params.arguments),
            "validate_library" => self.call_validate_library(&params.arguments),
            "export_library" => self.call_export_library(&params.arguments),
            "diff_libraries" => self.call_diff_libraries(&params.arguments),
            // Unknown tool
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
    ///
    /// These are low-level file I/O and primitive placement tools.
    /// The AI handles IPC calculations and design decisions.
    #[allow(clippy::too_many_lines)]
    fn get_tool_definitions() -> Vec<ToolDefinition> {
        vec![
            // === Library Reading ===
            ToolDefinition {
                name: "read_pcblib".to_string(),
                description: Some(
                    "Read an Altium .PcbLib file and return its contents including footprints \
                     with their primitives (pads, tracks, arcs, regions, text). Returns structured \
                     data that can be used to understand existing footprint styles. \
                     All coordinates and dimensions are in millimetres (mm). \
                     For large libraries, use component_name to fetch specific footprints, \
                     or use limit/offset for pagination."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Optional: fetch only this specific footprint by name"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Optional: maximum number of footprints to return (default: all)"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Optional: skip first N footprints (default: 0)"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "read_schlib".to_string(),
                description: Some(
                    "Read an Altium .SchLib file and return its contents including symbols \
                     with their primitives (pins, rectangles, lines, text). \
                     Coordinates are in schematic units (10 units = 1 grid square, not mm). \
                     For large libraries, use component_name to fetch specific symbols, \
                     or use limit/offset for pagination."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .SchLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Optional: fetch only this specific symbol by name"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Optional: maximum number of symbols to return (default: all)"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Optional: skip first N symbols (default: 0)"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "list_components".to_string(),
                description: Some(
                    "List all component/footprint names in an Altium library file (.PcbLib or .SchLib)."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the library file"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            // === Style Extraction ===
            ToolDefinition {
                name: "extract_style".to_string(),
                description: Some(
                    "Extract style information from an existing Altium library file. Returns \
                     statistics about track widths, colors, pin lengths, layer usage, and other \
                     styling parameters. Use this to learn from existing libraries and create \
                     consistent new components."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib or .SchLib file"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            // === Library Writing ===
            ToolDefinition {
                name: "write_pcblib".to_string(),
                description: Some(
                    "Write footprints to an Altium .PcbLib file. Each footprint is defined by \
                     its primitives: pads (with position, size, shape, layer), tracks, arcs, \
                     regions, and text. The AI is responsible for calculating correct positions \
                     and sizes based on IPC-7351B or other standards. \
                     All coordinates and dimensions must be in millimetres (mm)."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib file to create/modify"
                        },
                        "footprints": {
                            "type": "array",
                            "description": "Array of footprint definitions",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": {
                                        "type": "string",
                                        "description": "Footprint name (e.g., 'RESC1608X55N')"
                                    },
                                    "description": {
                                        "type": "string",
                                        "description": "Footprint description"
                                    },
                                    "pads": {
                                        "type": "array",
                                        "description": "Pad definitions",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "designator": { "type": "string" },
                                                "x": { "type": "number", "description": "X position in mm" },
                                                "y": { "type": "number", "description": "Y position in mm" },
                                                "width": { "type": "number", "description": "Pad width in mm" },
                                                "height": { "type": "number", "description": "Pad height in mm" },
                                                "shape": {
                                                    "type": "string",
                                                    "enum": ["rectangle", "rounded_rectangle", "round", "oval"],
                                                    "description": "Pad shape (round/circle are equivalent)"
                                                },
                                                "layer": { "type": "string", "description": "Layer name (default: multi-layer for SMD)" },
                                                "hole_size": { "type": "number", "description": "Hole diameter for through-hole pads (mm)" }
                                            },
                                            "required": ["designator", "x", "y", "width", "height"]
                                        }
                                    },
                                    "tracks": {
                                        "type": "array",
                                        "description": "Track/line definitions for silkscreen, assembly, etc.",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "x1": { "type": "number" },
                                                "y1": { "type": "number" },
                                                "x2": { "type": "number" },
                                                "y2": { "type": "number" },
                                                "width": { "type": "number", "description": "Line width in mm" },
                                                "layer": { "type": "string", "description": "Layer name (e.g., 'Top Overlay', 'Mechanical 1')" }
                                            },
                                            "required": ["x1", "y1", "x2", "y2", "width", "layer"]
                                        }
                                    },
                                    "arcs": {
                                        "type": "array",
                                        "description": "Arc definitions",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "x": { "type": "number", "description": "Center X" },
                                                "y": { "type": "number", "description": "Center Y" },
                                                "radius": { "type": "number" },
                                                "start_angle": { "type": "number", "description": "Start angle in degrees" },
                                                "end_angle": { "type": "number", "description": "End angle in degrees" },
                                                "width": { "type": "number", "description": "Line width in mm" },
                                                "layer": { "type": "string" }
                                            },
                                            "required": ["x", "y", "radius", "start_angle", "end_angle", "width", "layer"]
                                        }
                                    },
                                    "regions": {
                                        "type": "array",
                                        "description": "Filled region definitions (courtyard, etc.)",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "vertices": {
                                                    "type": "array",
                                                    "items": {
                                                        "type": "object",
                                                        "properties": {
                                                            "x": { "type": "number" },
                                                            "y": { "type": "number" }
                                                        }
                                                    }
                                                },
                                                "layer": { "type": "string" }
                                            },
                                            "required": ["vertices", "layer"]
                                        }
                                    },
                                    "text": {
                                        "type": "array",
                                        "description": "Text/string definitions",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "x": { "type": "number" },
                                                "y": { "type": "number" },
                                                "text": { "type": "string" },
                                                "height": { "type": "number", "description": "Text height in mm" },
                                                "layer": { "type": "string" },
                                                "rotation": { "type": "number", "description": "Rotation in degrees" }
                                            },
                                            "required": ["x", "y", "text", "height", "layer"]
                                        }
                                    },
                                    "step_model": {
                                        "type": "object",
                                        "description": "Optional STEP 3D model attachment",
                                        "properties": {
                                            "filepath": { "type": "string", "description": "Path to .step file" },
                                            "x_offset": { "type": "number" },
                                            "y_offset": { "type": "number" },
                                            "z_offset": { "type": "number" },
                                            "rotation": { "type": "number", "description": "Z rotation in degrees" }
                                        },
                                        "required": ["filepath"]
                                    }
                                },
                                "required": ["name", "pads"]
                            }
                        },
                        "append": {
                            "type": "boolean",
                            "description": "If true, append to existing file; if false, create new file"
                        }
                    },
                    "required": ["filepath", "footprints"]
                }),
            },
            ToolDefinition {
                name: "write_schlib".to_string(),
                description: Some(
                    "Write schematic symbols to an Altium .SchLib file. Each symbol is defined by \
                     its primitives: pins, rectangles, lines, polylines, arcs, ellipses, and labels. \
                     Coordinates must be in schematic units (10 units = 1 grid square, not mm)."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .SchLib file to create/modify"
                        },
                        "symbols": {
                            "type": "array",
                            "description": "Array of symbol definitions",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "description": { "type": "string" },
                                    "designator_prefix": { "type": "string", "description": "e.g., 'R' for resistors, 'U' for ICs" },
                                    "pins": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "designator": { "type": "string" },
                                                "name": { "type": "string" },
                                                "x": { "type": "number" },
                                                "y": { "type": "number" },
                                                "length": { "type": "number" },
                                                "orientation": { "type": "string", "enum": ["left", "right", "up", "down"] },
                                                "electrical_type": { "type": "string", "enum": ["input", "output", "bidirectional", "passive", "power"] }
                                            },
                                            "required": ["designator", "name", "x", "y", "length", "orientation"]
                                        }
                                    },
                                    "rectangles": { "type": "array" },
                                    "lines": { "type": "array" },
                                    "text": { "type": "array" }
                                },
                                "required": ["name", "pins"]
                            }
                        },
                        "append": { "type": "boolean" }
                    },
                    "required": ["filepath", "symbols"]
                }),
            },
            // === Library Management ===
            ToolDefinition {
                name: "delete_component".to_string(),
                description: Some(
                    "Delete one or more components from an Altium library file (.PcbLib or .SchLib). \
                     The file type is auto-detected from the extension. Returns status for each \
                     component: deleted, not_found, or error."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib or .SchLib file"
                        },
                        "component_names": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Names of components to delete"
                        }
                    },
                    "required": ["filepath", "component_names"]
                }),
            },
            ToolDefinition {
                name: "validate_library".to_string(),
                description: Some(
                    "Validate an Altium library file for common issues. Checks for: empty components \
                     (no pads/pins), duplicate designators, invalid coordinates, zero-size primitives, \
                     and other integrity problems. Returns a list of warnings and errors."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib or .SchLib file"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "export_library".to_string(),
                description: Some(
                    "Export an Altium library to JSON or CSV format for version control, backup, \
                     or external processing. JSON includes full component data; CSV provides a \
                     summary table of component names and basic info."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib or .SchLib file"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["json", "csv"],
                            "description": "Export format: 'json' for full data, 'csv' for summary table"
                        }
                    },
                    "required": ["filepath", "format"]
                }),
            },
            ToolDefinition {
                name: "diff_libraries".to_string(),
                description: Some(
                    "Compare two Altium library files and report differences. Shows added, removed, \
                     and modified components. Both files must be the same type (.PcbLib or .SchLib)."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath_a": {
                            "type": "string",
                            "description": "Path to the first (base/old) library file"
                        },
                        "filepath_b": {
                            "type": "string",
                            "description": "Path to the second (new/changed) library file"
                        }
                    },
                    "required": ["filepath_a", "filepath_b"]
                }),
            },
        ]
    }

    // ==================== Tool Handlers ====================

    /// Reads a `PcbLib` file and returns its contents.
    /// Supports pagination via limit/offset and filtering by `component_name`.
    #[allow(clippy::cast_possible_truncation)]
    fn call_read_pcblib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::PcbLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional pagination/filter parameters
        let component_name = arguments.get("component_name").and_then(Value::as_str);
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);
        let offset = arguments
            .get("offset")
            .and_then(Value::as_u64)
            .map_or(0, |v| v as usize);

        match PcbLib::read(filepath) {
            Ok(library) => {
                let total_count = library.len();

                // Apply filtering and pagination
                let footprints: Vec<_> = library
                    .footprints()
                    .filter(|fp| {
                        // If component_name specified, only include matching
                        component_name.map_or(true, |name| fp.name == name)
                    })
                    .skip(offset)
                    .take(limit.unwrap_or(usize::MAX))
                    .map(|fp| {
                        json!({
                            "name": fp.name,
                            "description": fp.description,
                            "pads": fp.pads,
                            "tracks": fp.tracks,
                            "arcs": fp.arcs,
                            "regions": fp.regions,
                            "text": fp.text,
                            "model_3d": fp.model_3d,
                        })
                    })
                    .collect();

                let returned_count = footprints.len();
                let has_more = if component_name.is_some() {
                    false // Single component fetch, no pagination
                } else {
                    offset + returned_count < total_count
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "total_count": total_count,
                    "returned_count": returned_count,
                    "offset": offset,
                    "has_more": has_more,
                    "footprints": footprints,
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Writes footprints to a `PcbLib` file.
    #[allow(clippy::too_many_lines)]
    fn call_write_pcblib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::pcblib::{Footprint, Model3D, PcbLib};

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(footprints_json) = arguments.get("footprints").and_then(Value::as_array) else {
            return ToolCallResult::error("Missing required parameter: footprints");
        };

        // Collect and validate footprint names for duplicates
        let new_names: Vec<&str> = footprints_json
            .iter()
            .filter_map(|fp| fp.get("name").and_then(Value::as_str))
            .collect();

        // Check for duplicates within the new footprints
        {
            let mut seen = std::collections::HashSet::new();
            for name in &new_names {
                if !seen.insert(*name) {
                    return ToolCallResult::error(format!(
                        "Duplicate footprint name in request: '{name}'"
                    ));
                }
            }
        }

        // Validate footprint names
        // OLE storage names are limited to 31 characters and cannot contain certain chars
        #[allow(clippy::items_after_statements)]
        const MAX_OLE_NAME_LEN: usize = 31;
        #[allow(clippy::items_after_statements)]
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
        for name in &new_names {
            if name.is_empty() {
                return ToolCallResult::error("Footprint name cannot be empty");
            }
            if name.len() > MAX_OLE_NAME_LEN {
                return ToolCallResult::error(format!(
                    "Footprint name '{name}' is too long ({} bytes). \
                     Maximum length is {MAX_OLE_NAME_LEN} bytes due to OLE storage format limitations.",
                    name.len(),
                ));
            }
            if let Some(c) = name.chars().find(|c| INVALID_CHARS.contains(c)) {
                return ToolCallResult::error(format!(
                    "Footprint name '{name}' contains invalid character '{c}'. \
                     Names cannot contain: / \\ : * ? \" < > |",
                ));
            }
        }

        let append = arguments
            .get("append")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // If append mode and file exists, read existing library; otherwise create new
        let mut library = if append && std::path::Path::new(filepath).exists() {
            match PcbLib::read(filepath) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to read existing library for append: {e}"
                    ));
                }
            }
        } else {
            PcbLib::new()
        };

        // Check for duplicates with existing footprints in append mode
        if append {
            let existing_names: std::collections::HashSet<_> =
                library.names().into_iter().collect();
            for name in &new_names {
                if existing_names.contains(*name) {
                    return ToolCallResult::error(format!(
                        "Footprint '{name}' already exists in the library"
                    ));
                }
            }
        }

        for fp_json in footprints_json {
            let name = fp_json
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unnamed");
            let mut footprint = Footprint::new(name);

            if let Some(desc) = fp_json.get("description").and_then(Value::as_str) {
                footprint.description = desc.to_string();
            }

            // Parse pads
            if let Some(pads) = fp_json.get("pads").and_then(Value::as_array) {
                for (i, pad_json) in pads.iter().enumerate() {
                    match Self::parse_pad(pad_json) {
                        Ok(pad) => footprint.add_pad(pad),
                        Err(e) => {
                            return ToolCallResult::error(format!(
                                "Footprint '{name}' pad {i}: {e}"
                            ))
                        }
                    }
                }
            }

            // Parse tracks
            if let Some(tracks) = fp_json.get("tracks").and_then(Value::as_array) {
                for (i, track_json) in tracks.iter().enumerate() {
                    match Self::parse_track(track_json) {
                        Ok(track) => footprint.add_track(track),
                        Err(e) => {
                            return ToolCallResult::error(format!(
                                "Footprint '{name}' track {i}: {e}"
                            ))
                        }
                    }
                }
            }

            // Parse arcs
            if let Some(arcs) = fp_json.get("arcs").and_then(Value::as_array) {
                for (i, arc_json) in arcs.iter().enumerate() {
                    match Self::parse_arc(arc_json) {
                        Ok(arc) => footprint.add_arc(arc),
                        Err(e) => {
                            return ToolCallResult::error(format!(
                                "Footprint '{name}' arc {i}: {e}"
                            ))
                        }
                    }
                }
            }

            // Parse regions
            if let Some(regions) = fp_json.get("regions").and_then(Value::as_array) {
                for region_json in regions {
                    if let Some(region) = Self::parse_region(region_json) {
                        footprint.add_region(region);
                    }
                }
            }

            // Parse text
            if let Some(texts) = fp_json.get("text").and_then(Value::as_array) {
                for text_json in texts {
                    if let Some(text) = Self::parse_text(text_json) {
                        footprint.add_text(text);
                    }
                }
            }

            // Parse 3D model
            if let Some(model_json) = fp_json.get("step_model") {
                if let Some(model_path) = model_json.get("filepath").and_then(Value::as_str) {
                    footprint.model_3d = Some(Model3D {
                        filepath: model_path.to_string(),
                        x_offset: model_json
                            .get("x_offset")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        y_offset: model_json
                            .get("y_offset")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        z_offset: model_json
                            .get("z_offset")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        rotation: model_json
                            .get("rotation")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                    });
                }
            }

            // Validate coordinates before adding
            if let Err(e) = Self::validate_footprint_coordinates(&footprint) {
                return ToolCallResult::error(e);
            }

            library.add(footprint);
        }

        match library.write(filepath) {
            Ok(()) => {
                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "footprint_count": library.len(),
                    "footprint_names": library.names(),
                });
                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Reads a `SchLib` file and returns its contents.
    /// Supports pagination via limit/offset and filtering by `component_name`.
    #[allow(clippy::cast_possible_truncation)]
    fn call_read_schlib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::SchLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional pagination/filter parameters
        let component_name = arguments.get("component_name").and_then(Value::as_str);
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);
        let offset = arguments
            .get("offset")
            .and_then(Value::as_u64)
            .map_or(0, |v| v as usize);

        match SchLib::open(filepath) {
            Ok(library) => {
                let total_count = library.len();

                // Apply filtering and pagination
                let symbols: Vec<_> = library
                    .iter()
                    .filter(|(name, _)| {
                        // If component_name specified, only include matching
                        component_name.map_or(true, |filter| *name == filter)
                    })
                    .skip(offset)
                    .take(limit.unwrap_or(usize::MAX))
                    .map(|(name, symbol)| {
                        json!({
                            "name": name,
                            "description": symbol.description,
                            "designator": symbol.designator,
                            "part_count": symbol.part_count,
                            "pins": symbol.pins,
                            "rectangles": symbol.rectangles,
                            "lines": symbol.lines,
                            "polylines": symbol.polylines,
                            "arcs": symbol.arcs,
                            "ellipses": symbol.ellipses,
                            "labels": symbol.labels,
                            "parameters": symbol.parameters,
                            "footprints": symbol.footprints,
                        })
                    })
                    .collect();

                let returned_count = symbols.len();
                let has_more = if component_name.is_some() {
                    false // Single component fetch, no pagination
                } else {
                    offset + returned_count < total_count
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "total_count": total_count,
                    "returned_count": returned_count,
                    "offset": offset,
                    "has_more": has_more,
                    "symbols": symbols,
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Writes symbols to a `SchLib` file.
    #[allow(clippy::too_many_lines)]
    fn call_write_schlib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::schlib::{FootprintModel, SchLib, Symbol};

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(symbols_json) = arguments.get("symbols").and_then(Value::as_array) else {
            return ToolCallResult::error("Missing required parameter: symbols");
        };

        // Collect and validate symbol names
        let new_names: Vec<&str> = symbols_json
            .iter()
            .filter_map(|sym| sym.get("name").and_then(Value::as_str))
            .collect();

        // Check for duplicates within the new symbols
        {
            let mut seen = std::collections::HashSet::new();
            for name in &new_names {
                if !seen.insert(*name) {
                    return ToolCallResult::error(format!(
                        "Duplicate symbol name in request: '{name}'"
                    ));
                }
            }
        }

        // Validate symbol names
        // OLE storage names are limited to 31 characters and cannot contain certain chars
        #[allow(clippy::items_after_statements)]
        const MAX_OLE_NAME_LEN: usize = 31;
        #[allow(clippy::items_after_statements)]
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
        for name in &new_names {
            if name.is_empty() {
                return ToolCallResult::error("Symbol name cannot be empty");
            }
            if name.len() > MAX_OLE_NAME_LEN {
                return ToolCallResult::error(format!(
                    "Symbol name '{name}' is too long ({} bytes). \
                     Maximum length is {MAX_OLE_NAME_LEN} bytes due to OLE storage format limitations.",
                    name.len(),
                ));
            }
            if let Some(c) = name.chars().find(|c| INVALID_CHARS.contains(c)) {
                return ToolCallResult::error(format!(
                    "Symbol name '{name}' contains invalid character '{c}'. \
                     Names cannot contain: / \\ : * ? \" < > |",
                ));
            }
        }

        let append = arguments
            .get("append")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // If append mode and file exists, read existing library; otherwise create new
        let mut library = if append && std::path::Path::new(filepath).exists() {
            match SchLib::open(filepath) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to read existing library for append: {e}"
                    ));
                }
            }
        } else {
            SchLib::new()
        };

        // Check for duplicates with existing symbols in append mode
        if append {
            for name in &new_names {
                if library.get(name).is_some() {
                    return ToolCallResult::error(format!(
                        "Symbol '{name}' already exists in the library"
                    ));
                }
            }
        }

        for sym_json in symbols_json {
            let name = sym_json
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unnamed");
            let mut symbol = Symbol::new(name);

            if let Some(desc) = sym_json.get("description").and_then(Value::as_str) {
                symbol.description = desc.to_string();
            }

            if let Some(desig) = sym_json.get("designator_prefix").and_then(Value::as_str) {
                symbol.designator = format!("{desig}?");
            }

            // Parse pins
            if let Some(pins) = sym_json.get("pins").and_then(Value::as_array) {
                for pin_json in pins {
                    if let Some(pin) = Self::parse_schlib_pin(pin_json) {
                        symbol.add_pin(pin);
                    }
                }
            }

            // Parse rectangles
            if let Some(rects) = sym_json.get("rectangles").and_then(Value::as_array) {
                for rect_json in rects {
                    if let Some(rect) = Self::parse_schlib_rectangle(rect_json) {
                        symbol.add_rectangle(rect);
                    }
                }
            }

            // Parse lines
            if let Some(lines) = sym_json.get("lines").and_then(Value::as_array) {
                for line_json in lines {
                    if let Some(line) = Self::parse_schlib_line(line_json) {
                        symbol.add_line(line);
                    }
                }
            }

            // Parse polylines
            if let Some(polylines) = sym_json.get("polylines").and_then(Value::as_array) {
                for polyline_json in polylines {
                    if let Some(polyline) = Self::parse_schlib_polyline(polyline_json) {
                        symbol.add_polyline(polyline);
                    }
                }
            }

            // Parse arcs
            if let Some(arcs) = sym_json.get("arcs").and_then(Value::as_array) {
                for arc_json in arcs {
                    if let Some(arc) = Self::parse_schlib_arc(arc_json) {
                        symbol.add_arc(arc);
                    }
                }
            }

            // Parse ellipses
            if let Some(ellipses) = sym_json.get("ellipses").and_then(Value::as_array) {
                for ellipse_json in ellipses {
                    if let Some(ellipse) = Self::parse_schlib_ellipse(ellipse_json) {
                        symbol.add_ellipse(ellipse);
                    }
                }
            }

            // Parse parameters
            if let Some(params) = sym_json.get("parameters").and_then(Value::as_array) {
                for param_json in params {
                    if let Some(param) = Self::parse_schlib_parameter(param_json) {
                        symbol.add_parameter(param);
                    }
                }
            }

            // Parse footprint references
            if let Some(footprints) = sym_json.get("footprints").and_then(Value::as_array) {
                for fp_json in footprints {
                    if let Some(fp_name) = fp_json.get("name").and_then(Value::as_str) {
                        let mut fp = FootprintModel::new(fp_name);
                        if let Some(desc) = fp_json.get("description").and_then(Value::as_str) {
                            fp.description = desc.to_string();
                        }
                        symbol.add_footprint(fp);
                    }
                }
            }

            // Validate coordinates before adding
            if let Err(e) = Self::validate_symbol_coordinates(&symbol) {
                return ToolCallResult::error(e);
            }

            library.add_symbol(symbol);
        }

        match library.save(filepath) {
            Ok(()) => {
                let symbol_names: Vec<_> = library.iter().map(|(name, _)| name.clone()).collect();
                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "symbol_count": library.len(),
                    "symbol_names": symbol_names,
                });
                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Lists component names in a library file.
    fn call_list_components(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::{PcbLib, SchLib};

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Try to determine file type from extension
        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => match PcbLib::read(filepath) {
                Ok(library) => {
                    let result = json!({
                        "status": "success",
                        "filepath": filepath,
                        "file_type": "PcbLib",
                        "component_count": library.len(),
                        "components": library.names(),
                    });
                    ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
                }
                Err(e) => {
                    let result = json!({
                        "status": "error",
                        "filepath": filepath,
                        "error": e.to_string(),
                    });
                    ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
                }
            },
            Some("schlib") => match SchLib::open(filepath) {
                Ok(library) => {
                    let symbol_names: Vec<_> =
                        library.iter().map(|(name, _)| name.clone()).collect();
                    let result = json!({
                        "status": "success",
                        "filepath": filepath,
                        "file_type": "SchLib",
                        "component_count": library.len(),
                        "components": symbol_names,
                    });
                    ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
                }
                Err(e) => {
                    let result = json!({
                        "status": "error",
                        "filepath": filepath,
                        "error": e.to_string(),
                    });
                    ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
                }
            },
            _ => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": "Unknown file type. Expected .PcbLib or .SchLib extension.",
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Extracts style information from a library file.
    fn call_extract_style(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => Self::extract_pcblib_style(filepath),
            Some("schlib") => Self::extract_schlib_style(filepath),
            _ => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": "Unknown file type. Expected .PcbLib or .SchLib extension.",
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Extracts style from a `PcbLib` file.
    fn extract_pcblib_style(filepath: &str) -> ToolCallResult {
        use crate::altium::PcbLib;
        use std::collections::HashMap;

        match PcbLib::read(filepath) {
            Ok(library) => {
                // Track widths by layer
                let mut track_widths: HashMap<String, Vec<f64>> = HashMap::new();
                // Pad shapes count
                let mut pad_shapes: HashMap<String, usize> = HashMap::new();
                // Text heights
                let mut text_heights: Vec<f64> = Vec::new();
                // Layers used
                let mut layers_used: HashMap<String, usize> = HashMap::new();

                for fp in library.footprints() {
                    // Analyze tracks
                    for track in &fp.tracks {
                        let layer_name = track.layer.as_str().to_string();
                        track_widths
                            .entry(layer_name.clone())
                            .or_default()
                            .push(track.width);
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }

                    // Analyze pads
                    for pad in &fp.pads {
                        let shape_name = format!("{:?}", pad.shape);
                        *pad_shapes.entry(shape_name).or_insert(0) += 1;
                        let layer_name = pad.layer.as_str().to_string();
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }

                    // Analyze text
                    for text in &fp.text {
                        text_heights.push(text.height);
                        let layer_name = text.layer.as_str().to_string();
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }

                    // Analyze regions
                    for region in &fp.regions {
                        let layer_name = region.layer.as_str().to_string();
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }
                }

                // Calculate statistics for track widths
                #[allow(clippy::cast_precision_loss)]
                let track_width_stats: HashMap<String, Value> = track_widths
                    .into_iter()
                    .map(|(layer, widths)| {
                        let min = widths.iter().copied().fold(f64::INFINITY, f64::min);
                        let max = widths.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                        let avg = widths.iter().sum::<f64>() / widths.len() as f64;
                        let most_common = Self::most_common_f64(&widths);
                        (
                            layer,
                            json!({
                                "min_mm": min,
                                "max_mm": max,
                                "avg_mm": avg,
                                "most_common_mm": most_common,
                                "count": widths.len()
                            }),
                        )
                    })
                    .collect();

                // Calculate text height stats
                let text_height_stats = if text_heights.is_empty() {
                    json!(null)
                } else {
                    let min = text_heights.iter().copied().fold(f64::INFINITY, f64::min);
                    let max = text_heights
                        .iter()
                        .copied()
                        .fold(f64::NEG_INFINITY, f64::max);
                    let most_common = Self::most_common_f64(&text_heights);
                    json!({
                        "min_mm": min,
                        "max_mm": max,
                        "most_common_mm": most_common,
                        "count": text_heights.len()
                    })
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "file_type": "PcbLib",
                    "footprint_count": library.len(),
                    "style": {
                        "track_widths_by_layer": track_width_stats,
                        "pad_shapes": pad_shapes,
                        "text_heights": text_height_stats,
                        "layers_used": layers_used
                    }
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Extracts style from a `SchLib` file.
    fn extract_schlib_style(filepath: &str) -> ToolCallResult {
        use crate::altium::SchLib;
        use std::collections::HashMap;

        match SchLib::open(filepath) {
            Ok(library) => {
                // Line widths
                let mut line_widths: Vec<u8> = Vec::new();
                // Pin lengths
                let mut pin_lengths: Vec<i32> = Vec::new();
                // Colours used
                let mut line_colors: HashMap<String, usize> = HashMap::new();
                let mut fill_colors: HashMap<String, usize> = HashMap::new();
                // Rectangle stats
                let mut rect_filled_count = 0usize;
                let mut rect_unfilled_count = 0usize;

                for (_name, symbol) in library.iter() {
                    // Analyze pins
                    for pin in &symbol.pins {
                        pin_lengths.push(pin.length);
                    }

                    // Analyze rectangles
                    for rect in &symbol.rectangles {
                        line_widths.push(rect.line_width);
                        let line_color = format!("#{:06X}", rect.line_color);
                        let fill_color = format!("#{:06X}", rect.fill_color);
                        *line_colors.entry(line_color).or_insert(0) += 1;
                        *fill_colors.entry(fill_color).or_insert(0) += 1;
                        if rect.filled {
                            rect_filled_count += 1;
                        } else {
                            rect_unfilled_count += 1;
                        }
                    }

                    // Analyze lines
                    for line in &symbol.lines {
                        line_widths.push(line.line_width);
                        let color = format!("#{:06X}", line.color);
                        *line_colors.entry(color).or_insert(0) += 1;
                    }
                }

                // Calculate stats
                let pin_length_stats = if pin_lengths.is_empty() {
                    json!(null)
                } else {
                    let min = *pin_lengths.iter().min().unwrap();
                    let max = *pin_lengths.iter().max().unwrap();
                    let most_common = Self::most_common(&pin_lengths);
                    json!({
                        "min_units": min,
                        "max_units": max,
                        "most_common_units": most_common,
                        "count": pin_lengths.len()
                    })
                };

                let line_width_stats = if line_widths.is_empty() {
                    json!(null)
                } else {
                    let min = *line_widths.iter().min().unwrap();
                    let max = *line_widths.iter().max().unwrap();
                    let most_common = Self::most_common(&line_widths);
                    json!({
                        "min": min,
                        "max": max,
                        "most_common": most_common,
                        "count": line_widths.len()
                    })
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "file_type": "SchLib",
                    "symbol_count": library.len(),
                    "style": {
                        "pin_lengths": pin_length_stats,
                        "line_widths": line_width_stats,
                        "line_colors": line_colors,
                        "fill_colors": fill_colors,
                        "rectangles": {
                            "filled_count": rect_filled_count,
                            "unfilled_count": rect_unfilled_count
                        }
                    }
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Finds the most common value in a slice of hashable, copyable values.
    ///
    /// Returns the default value if the slice is empty.
    fn most_common<T>(values: &[T]) -> T
    where
        T: std::hash::Hash + Eq + Copy + Default,
    {
        use std::collections::HashMap;
        let mut counts: HashMap<T, usize> = HashMap::new();
        for &v in values {
            *counts.entry(v).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map_or_else(T::default, |(key, _)| key)
    }

    /// Finds the most common value in a slice of f64, rounded to 2 decimal places.
    ///
    /// Since f64 doesn't implement Hash/Eq, values are quantized to centesimal
    /// precision (0.01) for grouping purposes.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    fn most_common_f64(values: &[f64]) -> f64 {
        use std::collections::HashMap;
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for &v in values {
            // Round to 2 decimal places for grouping
            let key = (v * 100.0).round() as i64;
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map_or(0.0, |(key, _)| key as f64 / 100.0)
    }

    // ==================== Coordinate Validation ====================

    /// Maximum coordinate value in mm that can be safely converted to Altium internal units.
    /// Internal units use i32: max value ~5456 mm (`i32::MAX` / 393700.7874).
    /// We use 5000 mm (~200 inches) as a conservative limit.
    const MAX_COORDINATE_MM: f64 = 5000.0;

    /// Validates that a coordinate is within the safe range for Altium internal units.
    fn validate_coordinate(value: f64, field_name: &str) -> Result<(), String> {
        if !value.is_finite() {
            return Err(format!(
                "{field_name} must be a finite number, got: {value}"
            ));
        }
        if value.abs() > Self::MAX_COORDINATE_MM {
            return Err(format!(
                "{field_name} value {value} mm exceeds the maximum safe range of ±{} mm",
                Self::MAX_COORDINATE_MM
            ));
        }
        Ok(())
    }

    /// Validates all coordinates in a footprint before writing.
    fn validate_footprint_coordinates(
        footprint: &crate::altium::pcblib::Footprint,
    ) -> Result<(), String> {
        let name = &footprint.name;

        for (i, pad) in footprint.pads.iter().enumerate() {
            Self::validate_coordinate(pad.x, &format!("Footprint '{name}' pad {i} x"))?;
            Self::validate_coordinate(pad.y, &format!("Footprint '{name}' pad {i} y"))?;
            Self::validate_coordinate(pad.width, &format!("Footprint '{name}' pad {i} width"))?;
            Self::validate_coordinate(pad.height, &format!("Footprint '{name}' pad {i} height"))?;
            if let Some(hole) = pad.hole_size {
                Self::validate_coordinate(hole, &format!("Footprint '{name}' pad {i} hole_size"))?;
            }
        }

        for (i, track) in footprint.tracks.iter().enumerate() {
            Self::validate_coordinate(track.x1, &format!("Footprint '{name}' track {i} x1"))?;
            Self::validate_coordinate(track.y1, &format!("Footprint '{name}' track {i} y1"))?;
            Self::validate_coordinate(track.x2, &format!("Footprint '{name}' track {i} x2"))?;
            Self::validate_coordinate(track.y2, &format!("Footprint '{name}' track {i} y2"))?;
            Self::validate_coordinate(track.width, &format!("Footprint '{name}' track {i} width"))?;
        }

        for (i, arc) in footprint.arcs.iter().enumerate() {
            Self::validate_coordinate(arc.x, &format!("Footprint '{name}' arc {i} x"))?;
            Self::validate_coordinate(arc.y, &format!("Footprint '{name}' arc {i} y"))?;
            Self::validate_coordinate(arc.radius, &format!("Footprint '{name}' arc {i} radius"))?;
            Self::validate_coordinate(arc.width, &format!("Footprint '{name}' arc {i} width"))?;
        }

        for (i, region) in footprint.regions.iter().enumerate() {
            for (j, vertex) in region.vertices.iter().enumerate() {
                Self::validate_coordinate(
                    vertex.x,
                    &format!("Footprint '{name}' region {i} vertex {j} x"),
                )?;
                Self::validate_coordinate(
                    vertex.y,
                    &format!("Footprint '{name}' region {i} vertex {j} y"),
                )?;
            }
        }

        for (i, text) in footprint.text.iter().enumerate() {
            Self::validate_coordinate(text.x, &format!("Footprint '{name}' text {i} x"))?;
            Self::validate_coordinate(text.y, &format!("Footprint '{name}' text {i} y"))?;
            Self::validate_coordinate(text.height, &format!("Footprint '{name}' text {i} height"))?;
        }

        Ok(())
    }

    /// Maximum coordinate value for `SchLib` (uses i16 internally).
    /// `i16::MAX` = 32767, but we use 32000 as a conservative limit.
    const MAX_SCHLIB_COORDINATE: i32 = 32000;

    /// Validates that a `SchLib` coordinate is within the safe range for i16.
    fn validate_schlib_coordinate(value: i32, field_name: &str) -> Result<(), String> {
        if value.abs() > Self::MAX_SCHLIB_COORDINATE {
            return Err(format!(
                "{field_name} value {value} exceeds the maximum safe range of ±{} units",
                Self::MAX_SCHLIB_COORDINATE
            ));
        }
        Ok(())
    }

    /// Validates all coordinates in a symbol before writing.
    fn validate_symbol_coordinates(symbol: &crate::altium::schlib::Symbol) -> Result<(), String> {
        let name = &symbol.name;

        for (i, pin) in symbol.pins.iter().enumerate() {
            Self::validate_schlib_coordinate(pin.x, &format!("Symbol '{name}' pin {i} x"))?;
            Self::validate_schlib_coordinate(pin.y, &format!("Symbol '{name}' pin {i} y"))?;
            Self::validate_schlib_coordinate(
                pin.length,
                &format!("Symbol '{name}' pin {i} length"),
            )?;
        }

        for (i, rect) in symbol.rectangles.iter().enumerate() {
            Self::validate_schlib_coordinate(
                rect.x1,
                &format!("Symbol '{name}' rectangle {i} x1"),
            )?;
            Self::validate_schlib_coordinate(
                rect.y1,
                &format!("Symbol '{name}' rectangle {i} y1"),
            )?;
            Self::validate_schlib_coordinate(
                rect.x2,
                &format!("Symbol '{name}' rectangle {i} x2"),
            )?;
            Self::validate_schlib_coordinate(
                rect.y2,
                &format!("Symbol '{name}' rectangle {i} y2"),
            )?;
        }

        for (i, line) in symbol.lines.iter().enumerate() {
            Self::validate_schlib_coordinate(line.x1, &format!("Symbol '{name}' line {i} x1"))?;
            Self::validate_schlib_coordinate(line.y1, &format!("Symbol '{name}' line {i} y1"))?;
            Self::validate_schlib_coordinate(line.x2, &format!("Symbol '{name}' line {i} x2"))?;
            Self::validate_schlib_coordinate(line.y2, &format!("Symbol '{name}' line {i} y2"))?;
        }

        for (i, polyline) in symbol.polylines.iter().enumerate() {
            for (j, &(x, y)) in polyline.points.iter().enumerate() {
                Self::validate_schlib_coordinate(
                    x,
                    &format!("Symbol '{name}' polyline {i} point {j} x"),
                )?;
                Self::validate_schlib_coordinate(
                    y,
                    &format!("Symbol '{name}' polyline {i} point {j} y"),
                )?;
            }
        }

        for (i, arc) in symbol.arcs.iter().enumerate() {
            Self::validate_schlib_coordinate(arc.x, &format!("Symbol '{name}' arc {i} x"))?;
            Self::validate_schlib_coordinate(arc.y, &format!("Symbol '{name}' arc {i} y"))?;
            Self::validate_schlib_coordinate(
                arc.radius,
                &format!("Symbol '{name}' arc {i} radius"),
            )?;
        }

        for (i, ellipse) in symbol.ellipses.iter().enumerate() {
            Self::validate_schlib_coordinate(ellipse.x, &format!("Symbol '{name}' ellipse {i} x"))?;
            Self::validate_schlib_coordinate(ellipse.y, &format!("Symbol '{name}' ellipse {i} y"))?;
            Self::validate_schlib_coordinate(
                ellipse.radius_x,
                &format!("Symbol '{name}' ellipse {i} radius_x"),
            )?;
            Self::validate_schlib_coordinate(
                ellipse.radius_y,
                &format!("Symbol '{name}' ellipse {i} radius_y"),
            )?;
        }

        for (i, label) in symbol.labels.iter().enumerate() {
            Self::validate_schlib_coordinate(label.x, &format!("Symbol '{name}' label {i} x"))?;
            Self::validate_schlib_coordinate(label.y, &format!("Symbol '{name}' label {i} y"))?;
        }

        Ok(())
    }

    // ==================== Primitive Parsing Helpers ====================

    /// Parses a pad from JSON.
    #[allow(clippy::too_many_lines)] // Pad has many fields requiring individual parsing
    fn parse_pad(json: &Value) -> Result<crate::altium::pcblib::Pad, String> {
        use crate::altium::pcblib::{Layer, Pad, PadShape, PadStackMode, PcbFlags};

        let designator = json
            .get("designator")
            .and_then(Value::as_str)
            .ok_or("Pad missing required field 'designator'")?;

        // Validate designator is not empty
        if designator.trim().is_empty() {
            return Err("Pad designator cannot be empty".to_string());
        }

        let x = json
            .get("x")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'x'")?;
        let y = json
            .get("y")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'y'")?;
        let width = json
            .get("width")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'width'")?;
        let height = json
            .get("height")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'height'")?;

        // Validate pad dimensions are positive
        if width <= 0.0 {
            return Err(format!(
                "Pad '{designator}' has invalid width {width}. Width must be greater than 0."
            ));
        }
        if height <= 0.0 {
            return Err(format!(
                "Pad '{designator}' has invalid height {height}. Height must be greater than 0."
            ));
        }

        let shape_str = json
            .get("shape")
            .and_then(Value::as_str)
            .unwrap_or("rounded_rectangle");
        let shape = match shape_str {
            "rectangle" => PadShape::Rectangle,
            "round" | "circle" => PadShape::Round,
            "oval" => PadShape::Oval,
            "rounded_rectangle" => PadShape::RoundedRectangle,
            _ => {
                return Err(format!(
                    "Pad '{designator}' has invalid shape '{shape_str}'. \
                     Valid shapes are: rectangle, round, circle, oval, rounded_rectangle"
                ))
            }
        };

        let layer_str = json.get("layer").and_then(Value::as_str);
        let layer = match layer_str {
            Some(s) => Layer::parse(s).ok_or_else(|| {
                format!(
                    "Pad '{designator}' has invalid layer '{s}'. \
                     Valid layers include: Top Layer, Bottom Layer, Multi-Layer, Top Overlay, etc."
                )
            })?,
            None => Layer::MultiLayer, // Default for pads is Multi-Layer
        };

        let hole_size = json.get("hole_size").and_then(Value::as_f64);
        let rotation = json.get("rotation").and_then(Value::as_f64).unwrap_or(0.0);

        // Parse optional hole shape
        let hole_shape = json
            .get("hole_shape")
            .and_then(Value::as_str)
            .map(|s| match s.to_lowercase().as_str() {
                "square" => crate::altium::pcblib::HoleShape::Square,
                "slot" => crate::altium::pcblib::HoleShape::Slot,
                _ => crate::altium::pcblib::HoleShape::Round,
            })
            .unwrap_or_default();

        // Parse optional mask expansion values
        let paste_mask_expansion = json.get("paste_mask_expansion").and_then(Value::as_f64);
        let solder_mask_expansion = json.get("solder_mask_expansion").and_then(Value::as_f64);
        let paste_mask_expansion_manual = json
            .get("paste_mask_expansion_manual")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let solder_mask_expansion_manual = json
            .get("solder_mask_expansion_manual")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Parse optional corner radius
        let corner_radius_percent = json
            .get("corner_radius_percent")
            .and_then(Value::as_u64)
            .and_then(|v| u8::try_from(v).ok())
            .filter(|&v| v <= 100);

        Ok(Pad {
            designator: designator.to_string(),
            x,
            y,
            width,
            height,
            shape,
            layer,
            hole_size,
            hole_shape,
            rotation,
            paste_mask_expansion,
            solder_mask_expansion,
            paste_mask_expansion_manual,
            solder_mask_expansion_manual,
            corner_radius_percent,
            stack_mode: PadStackMode::Simple,
            per_layer_sizes: None,
            per_layer_shapes: None,
            per_layer_corner_radii: None,
            per_layer_offsets: None,
            flags: PcbFlags::empty(),
            unique_id: None,
        })
    }

    /// Parses a track from JSON.
    fn parse_track(json: &Value) -> Result<crate::altium::pcblib::Track, String> {
        use crate::altium::pcblib::{Layer, Track};

        let x1 = json
            .get("x1")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'x1'")?;
        let y1 = json
            .get("y1")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'y1'")?;
        let x2 = json
            .get("x2")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'x2'")?;
        let y2 = json
            .get("y2")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'y2'")?;
        let width = json
            .get("width")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'width'")?;

        let layer_str = json.get("layer").and_then(Value::as_str);
        let layer = match layer_str {
            Some(s) => Layer::parse(s).ok_or_else(|| {
                format!(
                    "Track has invalid layer '{s}'. \
                     Valid layers include: Top Layer, Bottom Layer, Top Overlay, Top Assembly, etc."
                )
            })?,
            None => Layer::TopOverlay, // Default for tracks is Top Overlay
        };

        Ok(Track::new(x1, y1, x2, y2, width, layer))
    }

    /// Parses an arc from JSON.
    fn parse_arc(json: &Value) -> Result<crate::altium::pcblib::Arc, String> {
        use crate::altium::pcblib::{Arc, Layer, PcbFlags};

        let x = json
            .get("x")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'x'")?;
        let y = json
            .get("y")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'y'")?;
        let radius = json
            .get("radius")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'radius'")?;
        let start_angle = json
            .get("start_angle")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'start_angle'")?;
        let end_angle = json
            .get("end_angle")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'end_angle'")?;
        let width = json
            .get("width")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'width'")?;

        let layer_str = json.get("layer").and_then(Value::as_str);
        let layer = match layer_str {
            Some(s) => Layer::parse(s).ok_or_else(|| {
                format!(
                    "Arc has invalid layer '{s}'. \
                     Valid layers include: Top Layer, Bottom Layer, Top Overlay, Top Assembly, etc."
                )
            })?,
            None => Layer::TopOverlay, // Default for arcs is Top Overlay
        };

        Ok(Arc {
            x,
            y,
            radius,
            start_angle,
            end_angle,
            width,
            layer,
            flags: PcbFlags::empty(),
            unique_id: None,
        })
    }

    /// Parses a region from JSON.
    fn parse_region(json: &Value) -> Option<crate::altium::pcblib::Region> {
        use crate::altium::pcblib::{Layer, PcbFlags, Region, Vertex};

        let vertices_json = json.get("vertices").and_then(Value::as_array)?;
        let layer = json
            .get("layer")
            .and_then(Value::as_str)
            .and_then(Layer::parse)
            .unwrap_or(Layer::Mechanical15);

        let vertices: Vec<Vertex> = vertices_json
            .iter()
            .filter_map(|v| {
                let x = v.get("x").and_then(Value::as_f64)?;
                let y = v.get("y").and_then(Value::as_f64)?;
                Some(Vertex { x, y })
            })
            .collect();

        if vertices.len() < 3 {
            return None; // Need at least 3 vertices for a polygon
        }

        Some(Region {
            vertices,
            layer,
            flags: PcbFlags::empty(),
            unique_id: None,
        })
    }

    /// Parses text from JSON.
    fn parse_text(json: &Value) -> Option<crate::altium::pcblib::Text> {
        use crate::altium::pcblib::{Layer, PcbFlags, Text, TextJustification, TextKind};

        let x = json.get("x").and_then(Value::as_f64)?;
        let y = json.get("y").and_then(Value::as_f64)?;
        let text = json.get("text").and_then(Value::as_str)?;
        let height = json.get("height").and_then(Value::as_f64)?;
        let layer = json
            .get("layer")
            .and_then(Value::as_str)
            .and_then(Layer::parse)
            .unwrap_or(Layer::TopOverlay);
        let rotation = json.get("rotation").and_then(Value::as_f64).unwrap_or(0.0);

        Some(Text {
            x,
            y,
            text: text.to_string(),
            height,
            layer,
            rotation,
            kind: TextKind::Stroke,
            stroke_font: None,
            justification: TextJustification::MiddleCenter,
            flags: PcbFlags::empty(),
            unique_id: None,
        })
    }

    // ==================== SchLib Primitive Parsing Helpers ====================

    /// Parses a schematic pin from JSON.
    #[allow(clippy::cast_possible_truncation)]
    fn parse_schlib_pin(json: &Value) -> Option<crate::altium::schlib::Pin> {
        use crate::altium::schlib::{Pin, PinElectricalType, PinOrientation};

        let designator = json.get("designator").and_then(Value::as_str)?;
        let name = json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(designator);
        let x = json.get("x").and_then(Value::as_i64)? as i32;
        let y = json.get("y").and_then(Value::as_i64)? as i32;
        let length = json.get("length").and_then(Value::as_i64).unwrap_or(10) as i32;

        let orientation =
            json.get("orientation")
                .and_then(Value::as_str)
                .map_or(PinOrientation::Right, |s| match s.to_lowercase().as_str() {
                    "left" => PinOrientation::Left,
                    "up" => PinOrientation::Up,
                    "down" => PinOrientation::Down,
                    _ => PinOrientation::Right,
                });

        let electrical_type = json.get("electrical_type").and_then(Value::as_str).map_or(
            PinElectricalType::Passive,
            |s| match s.to_lowercase().as_str() {
                "input" => PinElectricalType::Input,
                "output" => PinElectricalType::Output,
                "bidirectional" | "io" | "input_output" => PinElectricalType::Bidirectional,
                "power" => PinElectricalType::Power,
                "open_collector" => PinElectricalType::OpenCollector,
                "open_emitter" => PinElectricalType::OpenEmitter,
                "hiz" | "hi_z" | "tristate" => PinElectricalType::HiZ,
                _ => PinElectricalType::Passive,
            },
        );

        let hidden = json.get("hidden").and_then(Value::as_bool).unwrap_or(false);
        let show_name = json
            .get("show_name")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let show_designator = json
            .get("show_designator")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Pin {
            name: name.to_string(),
            designator: designator.to_string(),
            x,
            y,
            length,
            orientation,
            electrical_type,
            hidden,
            show_name,
            show_designator,
            description: String::new(),
            owner_part_id,
        })
    }

    /// Parses a schematic rectangle from JSON.
    #[allow(clippy::cast_possible_truncation)]
    fn parse_schlib_rectangle(json: &Value) -> Option<crate::altium::schlib::Rectangle> {
        use crate::altium::schlib::Rectangle;

        let x1 = json.get("x1").and_then(Value::as_i64)? as i32;
        let y1 = json.get("y1").and_then(Value::as_i64)? as i32;
        let x2 = json.get("x2").and_then(Value::as_i64)? as i32;
        let y2 = json.get("y2").and_then(Value::as_i64)? as i32;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xFF_FF_B0) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Rectangle {
            x1,
            y1,
            x2,
            y2,
            line_width,
            line_color,
            fill_color,
            filled,
            owner_part_id,
        })
    }

    /// Parses a schematic line from JSON.
    #[allow(clippy::cast_possible_truncation)]
    fn parse_schlib_line(json: &Value) -> Option<crate::altium::schlib::Line> {
        use crate::altium::schlib::Line;

        let x1 = json.get("x1").and_then(Value::as_i64)? as i32;
        let y1 = json.get("y1").and_then(Value::as_i64)? as i32;
        let x2 = json.get("x2").and_then(Value::as_i64)? as i32;
        let y2 = json.get("y2").and_then(Value::as_i64)? as i32;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Line {
            x1,
            y1,
            x2,
            y2,
            line_width,
            color,
            owner_part_id,
        })
    }

    /// Parses a schematic parameter from JSON.
    #[allow(clippy::cast_possible_truncation)]
    fn parse_schlib_parameter(json: &Value) -> Option<crate::altium::schlib::Parameter> {
        use crate::altium::schlib::Parameter;

        let name = json.get("name").and_then(Value::as_str)?;
        let value = json
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("*")
            .to_string();

        let x = json.get("x").and_then(Value::as_i64).unwrap_or(0) as i32;
        let y = json.get("y").and_then(Value::as_i64).unwrap_or(0) as i32;
        let font_id = json.get("font_id").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x80_00_00) as u32;
        let hidden = json.get("hidden").and_then(Value::as_bool).unwrap_or(false);
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Parameter {
            name: name.to_string(),
            value,
            x,
            y,
            font_id,
            color,
            hidden,
            owner_part_id,
        })
    }

    /// Parses a schematic polyline from JSON.
    #[allow(clippy::cast_possible_truncation)]
    fn parse_schlib_polyline(json: &Value) -> Option<crate::altium::schlib::Polyline> {
        use crate::altium::schlib::Polyline;

        let points_json = json.get("points").and_then(Value::as_array)?;
        let points: Vec<(i32, i32)> = points_json
            .iter()
            .filter_map(|p| {
                let x = p.get("x").and_then(Value::as_i64)? as i32;
                let y = p.get("y").and_then(Value::as_i64)? as i32;
                Some((x, y))
            })
            .collect();

        if points.len() < 2 {
            return None; // Need at least 2 points for a polyline
        }

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Polyline {
            points,
            line_width,
            color,
            owner_part_id,
        })
    }

    /// Parses a schematic arc from JSON.
    #[allow(clippy::cast_possible_truncation)]
    fn parse_schlib_arc(json: &Value) -> Option<crate::altium::schlib::Arc> {
        use crate::altium::schlib::Arc;

        let x = json.get("x").and_then(Value::as_i64)? as i32;
        let y = json.get("y").and_then(Value::as_i64)? as i32;
        let radius = json.get("radius").and_then(Value::as_i64)? as i32;
        let start_angle = json
            .get("start_angle")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let end_angle = json
            .get("end_angle")
            .and_then(Value::as_f64)
            .unwrap_or(360.0);
        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Arc {
            x,
            y,
            radius,
            start_angle,
            end_angle,
            line_width,
            color,
            owner_part_id,
        })
    }

    /// Parses a schematic ellipse from JSON.
    #[allow(clippy::cast_possible_truncation)]
    fn parse_schlib_ellipse(json: &Value) -> Option<crate::altium::schlib::Ellipse> {
        use crate::altium::schlib::Ellipse;

        let x = json.get("x").and_then(Value::as_i64)? as i32;
        let y = json.get("y").and_then(Value::as_i64)? as i32;
        let radius_x = json.get("radius_x").and_then(Value::as_i64)? as i32;
        let radius_y = json.get("radius_y").and_then(Value::as_i64)? as i32;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xFF_FF_B0) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Ellipse {
            x,
            y,
            radius_x,
            radius_y,
            line_width,
            line_color,
            fill_color,
            filled,
            owner_part_id,
        })
    }

    // ==================== Library Management Tools ====================

    /// Deletes one or more components from a library file.
    ///
    /// Supports both `.PcbLib` and `.SchLib` files. The file type is auto-detected
    /// from the extension. Returns per-component status (`deleted`, `not_found`, or `error`).
    fn call_delete_component(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(component_names) = arguments.get("component_names").and_then(Value::as_array)
        else {
            return ToolCallResult::error("Missing required parameter: component_names");
        };

        let names: Vec<&str> = component_names.iter().filter_map(Value::as_str).collect();

        if names.is_empty() {
            return ToolCallResult::error("component_names array is empty or contains no strings");
        }

        // Determine file type from extension
        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => Self::delete_from_pcblib(filepath, &names),
            Some("schlib") => Self::delete_from_schlib(filepath, &names),
            _ => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": "Unknown file type. Expected .PcbLib or .SchLib extension.",
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Deletes components from a `PcbLib` file.
    fn delete_from_pcblib(filepath: &str, names: &[&str]) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read the library
        let mut library = match PcbLib::read(filepath) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        let original_count = library.len();
        let mut results: Vec<Value> = Vec::with_capacity(names.len());
        let mut deleted_count = 0;

        // Remove each component
        for name in names {
            if library.remove(name).is_some() {
                results.push(json!({
                    "name": name,
                    "status": "deleted"
                }));
                deleted_count += 1;
            } else {
                results.push(json!({
                    "name": name,
                    "status": "not_found"
                }));
            }
        }

        // Only write if something was deleted
        if deleted_count > 0 {
            if let Err(e) = library.write(filepath) {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": format!("Failed to write library: {e}"),
                    "results": results,
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        }

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "PcbLib",
            "original_count": original_count,
            "deleted_count": deleted_count,
            "remaining_count": library.len(),
            "results": results,
        });
        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Deletes components from a `SchLib` file.
    fn delete_from_schlib(filepath: &str, names: &[&str]) -> ToolCallResult {
        use crate::altium::SchLib;

        // Read the library
        let mut library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        let original_count = library.len();
        let mut results: Vec<Value> = Vec::with_capacity(names.len());
        let mut deleted_count = 0;

        // Remove each component
        for name in names {
            if library.remove(name).is_some() {
                results.push(json!({
                    "name": name,
                    "status": "deleted"
                }));
                deleted_count += 1;
            } else {
                results.push(json!({
                    "name": name,
                    "status": "not_found"
                }));
            }
        }

        // Only write if something was deleted
        if deleted_count > 0 {
            if let Err(e) = library.save(filepath) {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": format!("Failed to write library: {e}"),
                    "results": results,
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        }

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "SchLib",
            "original_count": original_count,
            "deleted_count": deleted_count,
            "remaining_count": library.len(),
            "results": results,
        });
        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    // ==================== Library Validation Tools ====================

    /// Validates an Altium library file for common issues.
    ///
    /// Checks for empty components, duplicate designators, invalid coordinates,
    /// zero-size primitives, and other integrity problems.
    fn call_validate_library(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Determine file type from extension
        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => Self::validate_pcblib(filepath),
            Some("schlib") => Self::validate_schlib(filepath),
            _ => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": "Unknown file type. Expected .PcbLib or .SchLib extension.",
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Validates a `PcbLib` file.
    #[allow(clippy::too_many_lines)]
    fn validate_pcblib(filepath: &str) -> ToolCallResult {
        use crate::altium::PcbLib;
        use std::collections::HashSet;

        // Read the library
        let library = match PcbLib::read(filepath) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        let mut issues: Vec<Value> = Vec::new();
        let component_count = library.len();

        // Check for empty library
        if component_count == 0 {
            issues.push(json!({
                "severity": "warning",
                "component": null,
                "issue": "Library is empty (no footprints)"
            }));
        }

        // Validate each footprint
        for fp in library.footprints() {
            let name = &fp.name;

            // Check for empty name
            if name.is_empty() {
                issues.push(json!({
                    "severity": "error",
                    "component": name,
                    "issue": "Footprint has empty name"
                }));
            }

            // Check for no pads
            if fp.pads.is_empty() {
                issues.push(json!({
                    "severity": "warning",
                    "component": name,
                    "issue": "Footprint has no pads"
                }));
            }

            // Check for duplicate pad designators
            let mut seen_designators: HashSet<&str> = HashSet::new();
            for pad in &fp.pads {
                if !seen_designators.insert(&pad.designator) {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Duplicate pad designator: '{}'", pad.designator)
                    }));
                }

                // Check for empty designator
                if pad.designator.is_empty() {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": "Pad has empty designator"
                    }));
                }

                // Check for zero or negative dimensions
                if pad.width <= 0.0 || pad.height <= 0.0 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Pad '{}' has invalid dimensions (width: {}, height: {})",
                            pad.designator, pad.width, pad.height)
                    }));
                }

                // Check for invalid coordinates
                if !pad.x.is_finite() || !pad.y.is_finite() {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Pad '{}' has invalid coordinates (x: {}, y: {})",
                            pad.designator, pad.x, pad.y)
                    }));
                }
            }

            // Check tracks for invalid values
            for (i, track) in fp.tracks.iter().enumerate() {
                if track.width <= 0.0 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Track {} has invalid width: {}", i, track.width)
                    }));
                }
                if !track.x1.is_finite()
                    || !track.y1.is_finite()
                    || !track.x2.is_finite()
                    || !track.y2.is_finite()
                {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Track {} has invalid coordinates", i)
                    }));
                }
            }

            // Check arcs for invalid values
            for (i, arc) in fp.arcs.iter().enumerate() {
                if arc.radius <= 0.0 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Arc {} has invalid radius: {}", i, arc.radius)
                    }));
                }
                if !arc.x.is_finite() || !arc.y.is_finite() {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Arc {} has invalid centre coordinates", i)
                    }));
                }
            }

            // Check regions for minimum vertices
            for (i, region) in fp.regions.iter().enumerate() {
                if region.vertices.len() < 3 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Region {} has fewer than 3 vertices", i)
                    }));
                }
            }
        }

        let error_count = issues.iter().filter(|i| i["severity"] == "error").count();
        let warning_count = issues.iter().filter(|i| i["severity"] == "warning").count();

        let result = json!({
            "status": if error_count > 0 { "invalid" } else if warning_count > 0 { "warnings" } else { "valid" },
            "filepath": filepath,
            "file_type": "PcbLib",
            "component_count": component_count,
            "error_count": error_count,
            "warning_count": warning_count,
            "issues": issues,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Validates a `SchLib` file.
    fn validate_schlib(filepath: &str) -> ToolCallResult {
        use crate::altium::SchLib;
        use std::collections::HashSet;

        // Read the library
        let library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        let mut issues: Vec<Value> = Vec::new();
        let component_count = library.len();

        // Check for empty library
        if component_count == 0 {
            issues.push(json!({
                "severity": "warning",
                "component": null,
                "issue": "Library is empty (no symbols)"
            }));
        }

        // Validate each symbol
        for (name, symbol) in library.iter() {
            // Check for empty name
            if name.is_empty() {
                issues.push(json!({
                    "severity": "error",
                    "component": name,
                    "issue": "Symbol has empty name"
                }));
            }

            // Check for no pins
            if symbol.pins.is_empty() {
                issues.push(json!({
                    "severity": "warning",
                    "component": name,
                    "issue": "Symbol has no pins"
                }));
            }

            // Check for duplicate pin designators
            let mut seen_designators: HashSet<&str> = HashSet::new();
            for pin in &symbol.pins {
                if !seen_designators.insert(&pin.designator) {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Duplicate pin designator: '{}'", pin.designator)
                    }));
                }

                // Check for empty designator
                if pin.designator.is_empty() {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": "Pin has empty designator"
                    }));
                }

                // Check for zero or negative pin length
                if pin.length <= 0 {
                    issues.push(json!({
                        "severity": "warning",
                        "component": name,
                        "issue": format!("Pin '{}' has zero or negative length: {}",
                            pin.designator, pin.length)
                    }));
                }
            }

            // Check rectangles for inverted corners
            for (i, rect) in symbol.rectangles.iter().enumerate() {
                if rect.x1 > rect.x2 || rect.y1 > rect.y2 {
                    issues.push(json!({
                        "severity": "warning",
                        "component": name,
                        "issue": format!("Rectangle {} has inverted corners (x1={}, y1={}, x2={}, y2={})",
                            i, rect.x1, rect.y1, rect.x2, rect.y2)
                    }));
                }
            }

            // Check for symbols with no body (no rectangles, lines, or other graphics)
            let has_body = !symbol.rectangles.is_empty()
                || !symbol.lines.is_empty()
                || !symbol.polylines.is_empty()
                || !symbol.arcs.is_empty()
                || !symbol.ellipses.is_empty();

            if !has_body && !symbol.pins.is_empty() {
                issues.push(json!({
                    "severity": "warning",
                    "component": name,
                    "issue": "Symbol has pins but no body graphics (rectangles, lines, etc.)"
                }));
            }
        }

        let error_count = issues.iter().filter(|i| i["severity"] == "error").count();
        let warning_count = issues.iter().filter(|i| i["severity"] == "warning").count();

        let result = json!({
            "status": if error_count > 0 { "invalid" } else if warning_count > 0 { "warnings" } else { "valid" },
            "filepath": filepath,
            "file_type": "SchLib",
            "component_count": component_count,
            "error_count": error_count,
            "warning_count": warning_count,
            "issues": issues,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    // ==================== Library Export Tools ====================

    /// Exports an Altium library to JSON or CSV format.
    fn call_export_library(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(format) = arguments.get("format").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: format");
        };

        let format_lower = format.to_lowercase();
        if format_lower != "json" && format_lower != "csv" {
            return ToolCallResult::error("Invalid format. Expected 'json' or 'csv'.");
        }

        // Determine file type from extension
        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => Self::export_pcblib(filepath, &format_lower),
            Some("schlib") => Self::export_schlib(filepath, &format_lower),
            _ => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": "Unknown file type. Expected .PcbLib or .SchLib extension.",
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Exports a `PcbLib` file to JSON or CSV.
    #[allow(clippy::too_many_lines)]
    fn export_pcblib(filepath: &str, format: &str) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read the library
        let library = match PcbLib::read(filepath) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        if format == "json" {
            // Full JSON export
            let footprints: Vec<Value> = library
                .footprints()
                .map(|fp| {
                    json!({
                        "name": fp.name,
                        "description": fp.description,
                        "pads": fp.pads,
                        "tracks": fp.tracks,
                        "arcs": fp.arcs,
                        "regions": fp.regions,
                        "text": fp.text,
                        "model_3d": fp.model_3d,
                    })
                })
                .collect();

            let result = json!({
                "status": "success",
                "filepath": filepath,
                "file_type": "PcbLib",
                "format": "json",
                "component_count": library.len(),
                "footprints": footprints,
            });

            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
        } else {
            // CSV export - summary table
            let mut csv_lines: Vec<String> = Vec::new();
            csv_lines.push("name,description,pad_count,track_count,arc_count,region_count,text_count,has_3d_model".to_string());

            for fp in library.footprints() {
                let description = fp.description.replace(',', ";").replace('\n', " ");
                let has_model = if fp.model_3d.is_some() { "yes" } else { "no" };
                csv_lines.push(format!(
                    "{},{},{},{},{},{},{},{}",
                    fp.name,
                    description,
                    fp.pads.len(),
                    fp.tracks.len(),
                    fp.arcs.len(),
                    fp.regions.len(),
                    fp.text.len(),
                    has_model
                ));
            }

            let csv_content = csv_lines.join("\n");

            let result = json!({
                "status": "success",
                "filepath": filepath,
                "file_type": "PcbLib",
                "format": "csv",
                "component_count": library.len(),
                "csv": csv_content,
            });

            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
        }
    }

    /// Exports a `SchLib` file to JSON or CSV.
    fn export_schlib(filepath: &str, format: &str) -> ToolCallResult {
        use crate::altium::SchLib;

        // Read the library
        let library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        if format == "json" {
            // Full JSON export
            let symbols: Vec<Value> = library
                .iter()
                .map(|(name, symbol)| {
                    json!({
                        "name": name,
                        "description": symbol.description,
                        "designator": symbol.designator,
                        "pins": symbol.pins,
                        "rectangles": symbol.rectangles,
                        "lines": symbol.lines,
                        "polylines": symbol.polylines,
                        "arcs": symbol.arcs,
                        "ellipses": symbol.ellipses,
                        "labels": symbol.labels,
                        "parameters": symbol.parameters,
                        "footprints": symbol.footprints,
                    })
                })
                .collect();

            let result = json!({
                "status": "success",
                "filepath": filepath,
                "file_type": "SchLib",
                "format": "json",
                "component_count": library.len(),
                "symbols": symbols,
            });

            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
        } else {
            // CSV export - summary table
            let mut csv_lines: Vec<String> = Vec::new();
            csv_lines.push(
                "name,description,designator,pin_count,rectangle_count,line_count,footprint_count"
                    .to_string(),
            );

            for (name, symbol) in library.iter() {
                let description = symbol.description.replace(',', ";").replace('\n', " ");
                csv_lines.push(format!(
                    "{},{},{},{},{},{},{}",
                    name,
                    description,
                    symbol.designator,
                    symbol.pins.len(),
                    symbol.rectangles.len(),
                    symbol.lines.len(),
                    symbol.footprints.len()
                ));
            }

            let csv_content = csv_lines.join("\n");

            let result = json!({
                "status": "success",
                "filepath": filepath,
                "file_type": "SchLib",
                "format": "csv",
                "component_count": library.len(),
                "csv": csv_content,
            });

            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
        }
    }

    // ==================== Library Diff Tools ====================

    /// Compares two Altium library files and reports differences.
    fn call_diff_libraries(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath_a) = arguments.get("filepath_a").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath_a");
        };

        let Some(filepath_b) = arguments.get("filepath_b").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath_b");
        };

        // Validate both paths
        if let Err(e) = self.validate_path(filepath_a) {
            return ToolCallResult::error(e);
        }
        if let Err(e) = self.validate_path(filepath_b) {
            return ToolCallResult::error(e);
        }

        // Determine file types from extensions
        let path_a = std::path::Path::new(filepath_a);
        let path_b = std::path::Path::new(filepath_b);

        let ext_a = path_a
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);
        let ext_b = path_b
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        // Ensure both files are the same type
        if ext_a != ext_b {
            let result = json!({
                "status": "error",
                "error": format!("File types must match. Got '{}' and '{}'.",
                    ext_a.as_deref().unwrap_or("unknown"),
                    ext_b.as_deref().unwrap_or("unknown"))
            });
            return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
        }

        match ext_a.as_deref() {
            Some("pcblib") => Self::diff_pcblibs(filepath_a, filepath_b),
            Some("schlib") => Self::diff_schlibs(filepath_a, filepath_b),
            _ => {
                let result = json!({
                    "status": "error",
                    "error": "Unknown file type. Expected .PcbLib or .SchLib extension.",
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Compares two `PcbLib` files.
    #[allow(clippy::too_many_lines)]
    fn diff_pcblibs(filepath_a: &str, filepath_b: &str) -> ToolCallResult {
        use crate::altium::PcbLib;
        use std::collections::HashSet;

        // Read both libraries
        let lib_a = match PcbLib::read(filepath_a) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "error": format!("Failed to read '{}': {}", filepath_a, e),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        let lib_b = match PcbLib::read(filepath_b) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "error": format!("Failed to read '{}': {}", filepath_b, e),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        // Get component names from both libraries
        let names_a: HashSet<String> = lib_a.footprints().map(|f| f.name.clone()).collect();
        let names_b: HashSet<String> = lib_b.footprints().map(|f| f.name.clone()).collect();

        // Find added, removed, and common components
        let added: Vec<&str> = names_b.difference(&names_a).map(String::as_str).collect();
        let removed: Vec<&str> = names_a.difference(&names_b).map(String::as_str).collect();
        let common: Vec<&str> = names_a.intersection(&names_b).map(String::as_str).collect();

        // Check for modifications in common components
        let mut modified: Vec<Value> = Vec::new();

        for name in &common {
            let fp_a = lib_a.get(name).unwrap();
            let fp_b = lib_b.get(name).unwrap();

            let mut changes: Vec<String> = Vec::new();

            // Compare descriptions
            if fp_a.description != fp_b.description {
                changes.push(format!(
                    "description: '{}' -> '{}'",
                    fp_a.description, fp_b.description
                ));
            }

            // Compare primitive counts
            if fp_a.pads.len() != fp_b.pads.len() {
                changes.push(format!(
                    "pad_count: {} -> {}",
                    fp_a.pads.len(),
                    fp_b.pads.len()
                ));
            }
            if fp_a.tracks.len() != fp_b.tracks.len() {
                changes.push(format!(
                    "track_count: {} -> {}",
                    fp_a.tracks.len(),
                    fp_b.tracks.len()
                ));
            }
            if fp_a.arcs.len() != fp_b.arcs.len() {
                changes.push(format!(
                    "arc_count: {} -> {}",
                    fp_a.arcs.len(),
                    fp_b.arcs.len()
                ));
            }
            if fp_a.regions.len() != fp_b.regions.len() {
                changes.push(format!(
                    "region_count: {} -> {}",
                    fp_a.regions.len(),
                    fp_b.regions.len()
                ));
            }
            if fp_a.text.len() != fp_b.text.len() {
                changes.push(format!(
                    "text_count: {} -> {}",
                    fp_a.text.len(),
                    fp_b.text.len()
                ));
            }

            // Compare 3D model presence
            let has_model_a = fp_a.model_3d.is_some();
            let has_model_b = fp_b.model_3d.is_some();
            if has_model_a != has_model_b {
                changes.push(format!(
                    "3d_model: {} -> {}",
                    if has_model_a { "yes" } else { "no" },
                    if has_model_b { "yes" } else { "no" }
                ));
            }

            if !changes.is_empty() {
                modified.push(json!({
                    "name": name,
                    "changes": changes,
                }));
            }
        }

        let result = json!({
            "status": "success",
            "file_type": "PcbLib",
            "filepath_a": filepath_a,
            "filepath_b": filepath_b,
            "summary": {
                "components_in_a": lib_a.len(),
                "components_in_b": lib_b.len(),
                "added_count": added.len(),
                "removed_count": removed.len(),
                "modified_count": modified.len(),
                "unchanged_count": common.len() - modified.len(),
            },
            "added": added,
            "removed": removed,
            "modified": modified,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Compares two `SchLib` files.
    #[allow(clippy::too_many_lines)]
    fn diff_schlibs(filepath_a: &str, filepath_b: &str) -> ToolCallResult {
        use crate::altium::SchLib;
        use std::collections::HashSet;

        // Read both libraries
        let lib_a = match SchLib::open(filepath_a) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "error": format!("Failed to read '{}': {}", filepath_a, e),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        let lib_b = match SchLib::open(filepath_b) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "error": format!("Failed to read '{}': {}", filepath_b, e),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        // Get component names from both libraries
        let names_a: HashSet<String> = lib_a.iter().map(|(name, _)| name.clone()).collect();
        let names_b: HashSet<String> = lib_b.iter().map(|(name, _)| name.clone()).collect();

        // Find added, removed, and common components
        let added: Vec<&str> = names_b.difference(&names_a).map(String::as_str).collect();
        let removed: Vec<&str> = names_a.difference(&names_b).map(String::as_str).collect();
        let common: Vec<&str> = names_a.intersection(&names_b).map(String::as_str).collect();

        // Check for modifications in common components
        let mut modified: Vec<Value> = Vec::new();

        for name in &common {
            let sym_a = lib_a.get(name).unwrap();
            let sym_b = lib_b.get(name).unwrap();

            let mut changes: Vec<String> = Vec::new();

            // Compare descriptions
            if sym_a.description != sym_b.description {
                changes.push(format!(
                    "description: '{}' -> '{}'",
                    sym_a.description, sym_b.description
                ));
            }

            // Compare designators
            if sym_a.designator != sym_b.designator {
                changes.push(format!(
                    "designator: '{}' -> '{}'",
                    sym_a.designator, sym_b.designator
                ));
            }

            // Compare primitive counts
            if sym_a.pins.len() != sym_b.pins.len() {
                changes.push(format!(
                    "pin_count: {} -> {}",
                    sym_a.pins.len(),
                    sym_b.pins.len()
                ));
            }
            if sym_a.rectangles.len() != sym_b.rectangles.len() {
                changes.push(format!(
                    "rectangle_count: {} -> {}",
                    sym_a.rectangles.len(),
                    sym_b.rectangles.len()
                ));
            }
            if sym_a.lines.len() != sym_b.lines.len() {
                changes.push(format!(
                    "line_count: {} -> {}",
                    sym_a.lines.len(),
                    sym_b.lines.len()
                ));
            }
            if sym_a.polylines.len() != sym_b.polylines.len() {
                changes.push(format!(
                    "polyline_count: {} -> {}",
                    sym_a.polylines.len(),
                    sym_b.polylines.len()
                ));
            }
            if sym_a.arcs.len() != sym_b.arcs.len() {
                changes.push(format!(
                    "arc_count: {} -> {}",
                    sym_a.arcs.len(),
                    sym_b.arcs.len()
                ));
            }
            if sym_a.footprints.len() != sym_b.footprints.len() {
                changes.push(format!(
                    "footprint_count: {} -> {}",
                    sym_a.footprints.len(),
                    sym_b.footprints.len()
                ));
            }

            if !changes.is_empty() {
                modified.push(json!({
                    "name": name,
                    "changes": changes,
                }));
            }
        }

        let result = json!({
            "status": "success",
            "file_type": "SchLib",
            "filepath_a": filepath_a,
            "filepath_b": filepath_b,
            "summary": {
                "components_in_a": lib_a.len(),
                "components_in_b": lib_b.len(),
                "added_count": added.len(),
                "removed_count": removed.len(),
                "modified_count": modified.len(),
                "unchanged_count": common.len() - modified.len(),
            },
            "added": added,
            "removed": removed,
            "modified": modified,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_initial_state() {
        let server = McpServer::new(vec![PathBuf::from(".")]);
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

    #[test]
    fn most_common_generic() {
        // Test with i32
        let values_i32 = [1, 2, 2, 3, 2, 1];
        assert_eq!(McpServer::most_common(&values_i32), 2);

        // Test with u8
        let values_u8: [u8; 5] = [5, 5, 3, 5, 3];
        assert_eq!(McpServer::most_common(&values_u8), 5);

        // Test with empty slice - should return default
        let empty: [i32; 0] = [];
        assert_eq!(McpServer::most_common(&empty), 0);
    }

    #[test]
    fn most_common_f64_rounding() {
        // Values that are close should be grouped together
        let values = [1.001, 1.002, 1.009, 2.0];
        // All three ~1.0 values round to 1.00, so 1.0 should be most common
        assert!((McpServer::most_common_f64(&values) - 1.0).abs() < 0.01);

        // Empty slice should return 0.0
        let empty: [f64; 0] = [];
        assert!((McpServer::most_common_f64(&empty) - 0.0).abs() < f64::EPSILON);
    }
}
