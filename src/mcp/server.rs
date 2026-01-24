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

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
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

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if requires a predicate fn(&T) -> bool, so we must take &bool here
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
                .map_err(|e| format!("Failed to resolve path '{}': {e}", path.display()))?
        } else {
            // For new files, check the parent directory
            let parent = path.parent().ok_or_else(|| {
                format!(
                    "Invalid path '{}': no parent directory (cannot create file at root)",
                    path.display()
                )
            })?;
            let filename = path.file_name().ok_or_else(|| {
                format!("Invalid path '{}': no filename specified", path.display())
            })?;
            let canonical_parent = parent.canonicalize().map_err(|e| {
                format!(
                    "Parent directory '{}' does not exist or is inaccessible: {e}",
                    parent.display()
                )
            })?;
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
            "import_library" => self.call_import_library(&params.arguments),
            "extract_step_model" => self.call_extract_step_model(&params.arguments),
            "diff_libraries" => self.call_diff_libraries(&params.arguments),
            "batch_update" => self.call_batch_update(&params.arguments),
            "copy_component" => self.call_copy_component(&params.arguments),
            "rename_component" => self.call_rename_component(&params.arguments),
            "copy_component_cross_library" => {
                self.call_copy_component_cross_library(&params.arguments)
            }
            "merge_libraries" => self.call_merge_libraries(&params.arguments),
            "search_components" => self.call_search_components(&params.arguments),
            "render_footprint" => self.call_render_footprint(&params.arguments),
            "render_symbol" => self.call_render_symbol(&params.arguments),
            "manage_schlib_parameters" => self.call_manage_schlib_parameters(&params.arguments),
            "manage_schlib_footprints" => self.call_manage_schlib_footprints(&params.arguments),
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
                                    "part_count": { "type": "integer", "description": "Number of parts for multi-part symbols (e.g., 2 for dual op-amp). Default: 1" },
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
                                                "electrical_type": { "type": "string", "enum": ["input", "output", "bidirectional", "passive", "power"] },
                                                "owner_part_id": { "type": "integer", "description": "Part number this pin belongs to (1-based). Default: 1" }
                                            },
                                            "required": ["designator", "name", "x", "y", "length", "orientation"]
                                        }
                                    },
                                    "rectangles": { "type": "array" },
                                    "lines": { "type": "array" },
                                    "text": { "type": "array" },
                                    "parameters": {
                                        "type": "array",
                                        "description": "Symbol parameters (e.g., Value, Part Number)",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "name": { "type": "string", "description": "Parameter name (e.g., 'Value')" },
                                                "value": { "type": "string", "description": "Parameter value (e.g., '10k'). Default: '*'" },
                                                "x": { "type": "integer", "description": "X position. Default: 0" },
                                                "y": { "type": "integer", "description": "Y position. Default: 0" },
                                                "font_id": { "type": "integer", "description": "Font ID. Default: 1" },
                                                "color": { "type": "integer", "description": "BGR colour. Default: 0x800000 (dark red)" },
                                                "hidden": { "type": "boolean", "description": "Whether hidden. Default: false" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" }
                                            },
                                            "required": ["name"]
                                        }
                                    }
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
                name: "import_library".to_string(),
                description: Some(
                    "Import components from JSON data into an Altium library file. Accepts JSON \
                     in the format produced by export_library, enabling round-trip workflows. \
                     Auto-detects library type (PcbLib/SchLib) from the JSON data."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "output_path": {
                            "type": "string",
                            "description": "Path where the new library file will be created (.PcbLib or .SchLib)"
                        },
                        "json_data": {
                            "type": "object",
                            "description": "JSON data containing components to import. Should have 'file_type' (PcbLib/SchLib) and 'footprints' or 'symbols' array."
                        },
                        "append": {
                            "type": "boolean",
                            "description": "If true, append to existing library instead of overwriting. Default: false"
                        }
                    },
                    "required": ["output_path", "json_data"]
                }),
            },
            ToolDefinition {
                name: "extract_step_model".to_string(),
                description: Some(
                    "Extract embedded STEP 3D models from an Altium .PcbLib file. \
                     Models are stored compressed inside the library and this tool extracts \
                     them to standalone .step files. Use 'list' mode to see available models, \
                     or specify a model name/ID to extract a specific model."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib file containing embedded 3D models"
                        },
                        "output_path": {
                            "type": "string",
                            "description": "Path where the extracted .step file will be saved. If omitted, returns base64-encoded data."
                        },
                        "model": {
                            "type": "string",
                            "description": "Model name (e.g., 'RESC1005X04L.step') or GUID to extract. If omitted and only one model exists, extracts it automatically. If multiple models exist and no model specified, lists available models."
                        }
                    },
                    "required": ["filepath"]
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
            ToolDefinition {
                name: "batch_update".to_string(),
                description: Some(
                    "Perform batch updates across all components in an Altium library file. \
                     For PcbLib: update track widths, rename layers. \
                     For SchLib: update parameter values across symbols."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium library file (.PcbLib or .SchLib)"
                        },
                        "operation": {
                            "type": "string",
                            "enum": ["update_track_width", "rename_layer", "update_parameters"],
                            "description": "The batch operation to perform. PcbLib: update_track_width, rename_layer. SchLib: update_parameters."
                        },
                        "parameters": {
                            "type": "object",
                            "description": "Operation-specific parameters",
                            "properties": {
                                "from_width": {
                                    "type": "number",
                                    "description": "For update_track_width: the track width to match (in mm)"
                                },
                                "to_width": {
                                    "type": "number",
                                    "description": "For update_track_width: the new track width (in mm)"
                                },
                                "from_layer": {
                                    "type": "string",
                                    "description": "For rename_layer: the layer name to change from"
                                },
                                "to_layer": {
                                    "type": "string",
                                    "description": "For rename_layer: the layer name to change to"
                                },
                                "tolerance": {
                                    "type": "number",
                                    "description": "For update_track_width: matching tolerance (default: 0.001 mm)"
                                },
                                "param_name": {
                                    "type": "string",
                                    "description": "For update_parameters: parameter name to update (e.g., 'Value')"
                                },
                                "param_value": {
                                    "type": "string",
                                    "description": "For update_parameters: new value for the parameter"
                                },
                                "symbol_filter": {
                                    "type": "string",
                                    "description": "For update_parameters: regex pattern to filter symbol names (optional)"
                                },
                                "add_if_missing": {
                                    "type": "boolean",
                                    "description": "For update_parameters: add parameter if not present (default: false)"
                                }
                            }
                        }
                    },
                    "required": ["filepath", "operation", "parameters"]
                }),
            },
            ToolDefinition {
                name: "copy_component".to_string(),
                description: Some(
                    "Copy/duplicate a component within an Altium library file. Creates a new component \
                     with a different name but identical primitives. Useful for creating variants."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium library file (.PcbLib or .SchLib)"
                        },
                        "source_name": {
                            "type": "string",
                            "description": "Name of the component to copy"
                        },
                        "target_name": {
                            "type": "string",
                            "description": "Name for the new copied component"
                        },
                        "description": {
                            "type": "string",
                            "description": "Optional description for the new component (defaults to source description)"
                        }
                    },
                    "required": ["filepath", "source_name", "target_name"]
                }),
            },
            ToolDefinition {
                name: "rename_component".to_string(),
                description: Some(
                    "Rename a component within an Altium library file. This is an atomic operation \
                     that changes the component's name while preserving all primitives and properties. \
                     More efficient than copy + delete for simple renames."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium library file (.PcbLib or .SchLib)"
                        },
                        "old_name": {
                            "type": "string",
                            "description": "Current name of the component to rename"
                        },
                        "new_name": {
                            "type": "string",
                            "description": "New name for the component"
                        }
                    },
                    "required": ["filepath", "old_name", "new_name"]
                }),
            },
            ToolDefinition {
                name: "copy_component_cross_library".to_string(),
                description: Some(
                    "Copy a component from one Altium library to another. Both libraries must be \
                     the same type (PcbLib to PcbLib, or SchLib to SchLib). Useful for consolidating \
                     libraries or sharing components between projects."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "source_filepath": {
                            "type": "string",
                            "description": "Path to the source library file (.PcbLib or .SchLib)"
                        },
                        "target_filepath": {
                            "type": "string",
                            "description": "Path to the target library file (must be same type as source)"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the component to copy from the source library"
                        },
                        "new_name": {
                            "type": "string",
                            "description": "Optional new name for the component in the target library (defaults to original name)"
                        },
                        "description": {
                            "type": "string",
                            "description": "Optional new description for the component (defaults to original description)"
                        }
                    },
                    "required": ["source_filepath", "target_filepath", "component_name"]
                }),
            },
            ToolDefinition {
                name: "merge_libraries".to_string(),
                description: Some(
                    "Merge multiple Altium libraries into a single library. All source libraries must \
                     be the same type (all PcbLib or all SchLib). Components are copied from each \
                     source into the target library."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "source_filepaths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Array of paths to source library files (.PcbLib or .SchLib)"
                        },
                        "target_filepath": {
                            "type": "string",
                            "description": "Path to the target library file (will be created or appended to)"
                        },
                        "on_duplicate": {
                            "type": "string",
                            "enum": ["skip", "error", "rename"],
                            "description": "How to handle duplicate component names: 'skip' (ignore duplicates), 'error' (fail on duplicates), 'rename' (auto-rename with suffix). Default: 'error'"
                        }
                    },
                    "required": ["source_filepaths", "target_filepath"]
                }),
            },
            ToolDefinition {
                name: "search_components".to_string(),
                description: Some(
                    "Search for components across multiple Altium libraries using regex or glob patterns. \
                     Returns matching component names with their source library paths. Supports both \
                     `.PcbLib` (footprints) and `.SchLib` (symbols) files."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepaths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Array of library file paths to search (.PcbLib or .SchLib)"
                        },
                        "pattern": {
                            "type": "string",
                            "description": "Search pattern to match component names"
                        },
                        "pattern_type": {
                            "type": "string",
                            "enum": ["glob", "regex"],
                            "description": "Pattern type: 'glob' (wildcards like * and ?) or 'regex' (regular expressions). Default: 'glob'"
                        }
                    },
                    "required": ["filepaths", "pattern"]
                }),
            },
            ToolDefinition {
                name: "render_footprint".to_string(),
                description: Some(
                    "Render an ASCII art visualisation of a footprint from a PcbLib file. Shows pads, \
                     tracks, and other primitives in a simple text format for quick preview."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium PcbLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the footprint to render"
                        },
                        "scale": {
                            "type": "number",
                            "description": "Characters per mm (default: 2.0). Higher = more detail"
                        },
                        "max_width": {
                            "type": "integer",
                            "description": "Maximum width in characters (default: 80)"
                        },
                        "max_height": {
                            "type": "integer",
                            "description": "Maximum height in characters (default: 40)"
                        }
                    },
                    "required": ["filepath", "component_name"]
                }),
            },
            ToolDefinition {
                name: "render_symbol".to_string(),
                description: Some(
                    "Render an ASCII art visualisation of a schematic symbol from a SchLib file. \
                     Shows pins, rectangles, lines, and other primitives in a simple text format \
                     for quick preview. Coordinates are in schematic units (10 units = 1 grid)."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium SchLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the symbol to render"
                        },
                        "scale": {
                            "type": "number",
                            "description": "Characters per 10 schematic units (default: 1.0). Higher = more detail"
                        },
                        "max_width": {
                            "type": "integer",
                            "description": "Maximum width in characters (default: 80)"
                        },
                        "max_height": {
                            "type": "integer",
                            "description": "Maximum height in characters (default: 40)"
                        },
                        "part_id": {
                            "type": "integer",
                            "description": "Part ID for multi-part symbols (default: 1, shows all parts if 0)"
                        }
                    },
                    "required": ["filepath", "component_name"]
                }),
            },
            // manage_schlib_parameters - Manage symbol parameters (Value, Manufacturer, etc.)
            ToolDefinition {
                name: "manage_schlib_parameters".to_string(),
                description: Some(
                    "Manage component parameters in Altium SchLib files. Supports listing, \
                     getting, setting, adding, and deleting parameters like Value, Manufacturer, \
                     Part Number, etc."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium SchLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the symbol to manage parameters for"
                        },
                        "operation": {
                            "type": "string",
                            "enum": ["list", "get", "set", "add", "delete"],
                            "description": "Operation to perform: list (all parameters), get (single parameter), set (update value), add (new parameter), delete (remove parameter)"
                        },
                        "parameter_name": {
                            "type": "string",
                            "description": "Name of the parameter (required for get, set, add, delete)"
                        },
                        "value": {
                            "type": "string",
                            "description": "Parameter value (required for set, add)"
                        },
                        "hidden": {
                            "type": "boolean",
                            "description": "Whether the parameter is hidden (optional for set, add)"
                        },
                        "x": {
                            "type": "integer",
                            "description": "X position in schematic units (optional for add)"
                        },
                        "y": {
                            "type": "integer",
                            "description": "Y position in schematic units (optional for add)"
                        }
                    },
                    "required": ["filepath", "component_name", "operation"]
                }),
            },
            // manage_schlib_footprints - Manage footprint links in symbols
            ToolDefinition {
                name: "manage_schlib_footprints".to_string(),
                description: Some(
                    "Manage footprint links in Altium SchLib symbols. Supports listing, adding, \
                     and removing footprint references that link schematic symbols to PCB footprints."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium SchLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the symbol to manage footprints for"
                        },
                        "operation": {
                            "type": "string",
                            "enum": ["list", "add", "remove"],
                            "description": "Operation to perform: list (all footprints), add (new footprint link), remove (delete footprint link)"
                        },
                        "footprint_name": {
                            "type": "string",
                            "description": "Footprint name (required for add, remove)"
                        },
                        "description": {
                            "type": "string",
                            "description": "Footprint description (optional for add)"
                        }
                    },
                    "required": ["filepath", "component_name", "operation"]
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
        // Note: OLE storage names are limited to 31 characters, but the library layer
        // handles this by truncating storage names while preserving full names in PATTERN.
        #[allow(clippy::items_after_statements)]
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
        for name in &new_names {
            if name.is_empty() {
                return ToolCallResult::error("Footprint name cannot be empty");
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
        // Note: OLE storage names are limited to 31 characters, but the library layer
        // handles this by truncating storage names while preserving full names in LIBREFERENCE.
        #[allow(clippy::items_after_statements)]
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
        for name in &new_names {
            if name.is_empty() {
                return ToolCallResult::error("Symbol name cannot be empty");
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

            // Parse part_count for multi-part symbols (e.g., dual op-amp)
            if let Some(part_count) = sym_json.get("part_count").and_then(Value::as_u64) {
                #[allow(clippy::cast_possible_truncation)]
                {
                    symbol.part_count = part_count.clamp(1, 255) as u32;
                }
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

    // ==================== Library Import ====================

    /// Imports components from JSON data into an Altium library file.
    #[allow(clippy::too_many_lines)]
    fn call_import_library(&self, arguments: &Value) -> ToolCallResult {
        let Some(output_path) = arguments.get("output_path").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: output_path");
        };

        // Validate output path
        if let Err(e) = self.validate_path(output_path) {
            return ToolCallResult::error(e);
        }

        let Some(json_data) = arguments.get("json_data") else {
            return ToolCallResult::error("Missing required parameter: json_data");
        };

        let append = arguments
            .get("append")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Detect file type from JSON data or output path extension
        let file_type = json_data
            .get("file_type")
            .and_then(Value::as_str)
            .map(str::to_lowercase);

        let ext = std::path::Path::new(output_path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        // Determine library type - prefer JSON file_type, fall back to extension
        let library_type = match (file_type.as_deref(), ext.as_deref()) {
            (Some("pcblib"), _) | (None, Some("pcblib")) => "pcblib",
            (Some("schlib"), _) | (None, Some("schlib")) => "schlib",
            _ => {
                return ToolCallResult::error(
                    "Cannot determine library type. Provide 'file_type' in JSON or use .PcbLib/.SchLib extension.",
                );
            }
        };

        match library_type {
            "pcblib" => Self::import_pcblib(output_path, json_data, append),
            "schlib" => Self::import_schlib(output_path, json_data, append),
            _ => unreachable!(),
        }
    }

    /// Imports footprints from JSON into a `PcbLib` file.
    fn import_pcblib(output_path: &str, json_data: &Value, append: bool) -> ToolCallResult {
        use crate::altium::pcblib::{Footprint, PcbLib};

        // Get footprints array
        let Some(footprints_json) = json_data.get("footprints").and_then(Value::as_array) else {
            return ToolCallResult::error("JSON data must contain 'footprints' array");
        };

        // If append mode and file exists, read existing library; otherwise create new
        let mut library = if append && std::path::Path::new(output_path).exists() {
            match PcbLib::read(output_path) {
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

        let mut imported_count = 0;

        // Parse and add each footprint
        for (idx, fp_json) in footprints_json.iter().enumerate() {
            let name = fp_json
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unnamed");

            // Check for duplicate
            if library.get(name).is_some() {
                return ToolCallResult::error(format!(
                    "Component '{name}' already exists in library"
                ));
            }

            // Use write_pcblib parsing logic via serde
            match serde_json::from_value::<Footprint>(fp_json.clone()) {
                Ok(footprint) => {
                    library.add(footprint);
                    imported_count += 1;
                }
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to parse footprint {idx} ('{name}'): {e}"
                    ));
                }
            }
        }

        // Write the library
        if let Err(e) = library.write(output_path) {
            return ToolCallResult::error(format!("Failed to write library: {e}"));
        }

        let total_count = library.len();
        let result = json!({
            "status": "success",
            "output_path": output_path,
            "file_type": "PcbLib",
            "imported_count": imported_count,
            "total_count": total_count,
            "append": append,
            "message": if append {
                format!("Imported {imported_count} footprints (library now has {total_count} total)")
            } else {
                format!("Created library with {imported_count} footprints")
            },
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Imports symbols from JSON into a `SchLib` file.
    fn import_schlib(output_path: &str, json_data: &Value, append: bool) -> ToolCallResult {
        use crate::altium::schlib::Symbol;
        use crate::altium::SchLib;

        // Get symbols array
        let Some(symbols_json) = json_data.get("symbols").and_then(Value::as_array) else {
            return ToolCallResult::error("JSON data must contain 'symbols' array");
        };

        // If append mode and file exists, read existing library; otherwise create new
        let mut library = if append && std::path::Path::new(output_path).exists() {
            match SchLib::open(output_path) {
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

        let mut imported_count = 0;

        // Parse and add each symbol
        for (idx, sym_json) in symbols_json.iter().enumerate() {
            let name = sym_json
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unnamed");

            // Check for duplicate
            if library.get(name).is_some() {
                return ToolCallResult::error(format!(
                    "Component '{name}' already exists in library"
                ));
            }

            // Parse symbol via serde
            match serde_json::from_value::<Symbol>(sym_json.clone()) {
                Ok(symbol) => {
                    library.add_symbol(symbol);
                    imported_count += 1;
                }
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to parse symbol {idx} ('{name}'): {e}"
                    ));
                }
            }
        }

        // Write the library
        if let Err(e) = library.save(output_path) {
            return ToolCallResult::error(format!("Failed to write library: {e}"));
        }

        let total_count = library.len();
        let result = json!({
            "status": "success",
            "output_path": output_path,
            "file_type": "SchLib",
            "imported_count": imported_count,
            "total_count": total_count,
            "append": append,
            "message": if append {
                format!("Imported {imported_count} symbols (library now has {total_count} total)")
            } else {
                format!("Created library with {imported_count} symbols")
            },
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    // ==================== STEP Model Extraction ====================

    /// Extracts embedded STEP 3D models from a `PcbLib` file.
    #[allow(clippy::too_many_lines)]
    fn call_extract_step_model(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::PcbLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate library path
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Validate output path if provided
        let output_path = arguments.get("output_path").and_then(Value::as_str);
        if let Some(out_path) = output_path {
            if let Err(e) = self.validate_path(out_path) {
                return ToolCallResult::error(e);
            }
        }

        let model_identifier = arguments.get("model").and_then(Value::as_str);

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

        // Get all embedded models
        let models: Vec<_> = library.models().collect();

        if models.is_empty() {
            let result = json!({
                "status": "error",
                "filepath": filepath,
                "error": "No embedded 3D models found in this library.",
            });
            return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
        }

        // Find the model to extract
        let target_model = if let Some(identifier) = model_identifier {
            // Look up by name or GUID
            models
                .iter()
                .find(|m| {
                    m.name.eq_ignore_ascii_case(identifier)
                        || m.id.eq_ignore_ascii_case(identifier)
                        || m.id
                            .trim_matches(|c| c == '{' || c == '}')
                            .eq_ignore_ascii_case(identifier)
                })
                .copied()
        } else if models.len() == 1 {
            // Only one model, extract it
            Some(models[0])
        } else {
            // Multiple models, list them
            let model_list: Vec<Value> = models
                .iter()
                .map(|m| {
                    json!({
                        "id": m.id,
                        "name": m.name,
                        "size_bytes": m.data.len(),
                    })
                })
                .collect();

            let result = json!({
                "status": "list",
                "filepath": filepath,
                "message": "Multiple models found. Specify 'model' parameter with name or ID to extract.",
                "model_count": models.len(),
                "models": model_list,
            });
            return ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap());
        };

        let Some(model) = target_model else {
            let model_list: Vec<Value> = models
                .iter()
                .map(|m| {
                    json!({
                        "id": m.id,
                        "name": m.name,
                    })
                })
                .collect();

            let result = json!({
                "status": "error",
                "filepath": filepath,
                "error": format!("Model '{}' not found.", model_identifier.unwrap_or("")),
                "available_models": model_list,
            });
            return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
        };

        // Extract the model - write to file or return as base64
        Self::extract_model_output(filepath, output_path, model)
    }

    /// Helper to output extracted model data (to file or base64).
    fn extract_model_output(
        filepath: &str,
        output_path: Option<&str>,
        model: &crate::altium::pcblib::EmbeddedModel,
    ) -> ToolCallResult {
        output_path.map_or_else(
            || {
                // Return as base64
                let base64_data = BASE64_STANDARD.encode(&model.data);
                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "model_id": model.id,
                    "model_name": model.name,
                    "size_bytes": model.data.len(),
                    "encoding": "base64",
                    "data": base64_data,
                });
                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            },
            |out_path| {
                // Write to file
                match std::fs::write(out_path, &model.data) {
                    Ok(()) => {
                        let result = json!({
                            "status": "success",
                            "filepath": filepath,
                            "output_path": out_path,
                            "model_id": model.id,
                            "model_name": model.name,
                            "size_bytes": model.data.len(),
                            "message": format!("STEP model extracted to '{}'", out_path),
                        });
                        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
                    }
                    Err(e) => {
                        let result = json!({
                            "status": "error",
                            "filepath": filepath,
                            "output_path": out_path,
                            "error": format!("Failed to write file: {}", e),
                        });
                        ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
                    }
                }
            },
        )
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

    /// Performs batch updates across all components in a library file.
    fn call_batch_update(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(operation) = arguments.get("operation").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: operation");
        };

        let Some(parameters) = arguments.get("parameters") else {
            return ToolCallResult::error("Missing required parameter: parameters");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Detect file type
        let ext = std::path::Path::new(filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match ext.as_deref() {
            Some("pcblib") => Self::batch_update_pcblib(filepath, operation, parameters),
            Some("schlib") => Self::batch_update_schlib(filepath, operation, parameters),
            _ => ToolCallResult::error("batch_update only supports .PcbLib and .SchLib files"),
        }
    }

    /// Performs batch updates on a `PcbLib` file.
    fn batch_update_pcblib(filepath: &str, operation: &str, parameters: &Value) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read the library
        let mut library = match PcbLib::read(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Perform the operation
        match operation {
            "update_track_width" => {
                Self::batch_update_track_width(&mut library, parameters, filepath)
            }
            "rename_layer" => Self::batch_rename_layer(&mut library, parameters, filepath),
            _ => ToolCallResult::error(format!(
                "Unknown PcbLib operation: {operation}. Valid: update_track_width, rename_layer"
            )),
        }
    }

    /// Performs batch updates on a `SchLib` file.
    fn batch_update_schlib(filepath: &str, operation: &str, parameters: &Value) -> ToolCallResult {
        use crate::altium::schlib::SchLib;

        // Read the library
        let mut library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Perform the operation
        match operation {
            "update_parameters" => {
                Self::batch_update_schlib_parameters(&mut library, parameters, filepath)
            }
            _ => ToolCallResult::error(format!(
                "Unknown SchLib operation: {operation}. Valid: update_parameters"
            )),
        }
    }

    /// Updates parameters across all symbols in a `SchLib`.
    fn batch_update_schlib_parameters(
        library: &mut crate::altium::schlib::SchLib,
        parameters: &Value,
        filepath: &str,
    ) -> ToolCallResult {
        use crate::altium::schlib::Parameter;
        use regex::Regex;

        let Some(param_name) = parameters.get("param_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: param_name");
        };

        let Some(param_value) = parameters.get("param_value").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: param_value");
        };

        let add_if_missing = parameters
            .get("add_if_missing")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Compile symbol filter regex if provided
        let symbol_filter = parameters
            .get("symbol_filter")
            .and_then(Value::as_str)
            .map(Regex::new)
            .transpose();

        let symbol_filter = match symbol_filter {
            Ok(filter) => filter,
            Err(e) => {
                return ToolCallResult::error(format!("Invalid symbol_filter regex: {e}"));
            }
        };

        let mut updates = Vec::new();
        let mut symbols_updated = 0;
        let mut params_updated = 0;
        let mut params_added = 0;

        // Update parameters across all symbols
        for symbol in library.symbols_mut() {
            // Check symbol filter
            if let Some(ref filter) = symbol_filter {
                if !filter.is_match(&symbol.name) {
                    continue;
                }
            }

            let mut updated_in_symbol = false;
            let mut added_in_symbol = false;

            // Try to find and update existing parameter
            for param in &mut symbol.parameters {
                if param.name == param_name {
                    let old_value = param.value.clone();
                    param.value = param_value.to_string();
                    updates.push(json!({
                        "symbol": symbol.name,
                        "action": "updated",
                        "old_value": old_value,
                        "new_value": param_value
                    }));
                    params_updated += 1;
                    updated_in_symbol = true;
                    break;
                }
            }

            // Add parameter if not found and add_if_missing is true
            if !updated_in_symbol && add_if_missing {
                let param = Parameter::new(param_name, param_value);
                symbol.add_parameter(param);
                updates.push(json!({
                    "symbol": symbol.name,
                    "action": "added",
                    "new_value": param_value
                }));
                params_added += 1;
                added_in_symbol = true;
            }

            if updated_in_symbol || added_in_symbol {
                symbols_updated += 1;
            }
        }

        // Write back if any updates were made
        if symbols_updated > 0 {
            if let Err(e) = library.save(filepath) {
                return ToolCallResult::error(format!("Failed to write library: {e}"));
            }
        }

        let result = json!({
            "success": true,
            "filepath": filepath,
            "operation": "update_parameters",
            "param_name": param_name,
            "param_value": param_value,
            "summary": {
                "symbols_updated": symbols_updated,
                "parameters_updated": params_updated,
                "parameters_added": params_added,
                "total_symbols": library.len()
            },
            "updates": updates
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Updates track widths across all footprints in a library.
    fn batch_update_track_width(
        library: &mut crate::altium::PcbLib,
        parameters: &Value,
        filepath: &str,
    ) -> ToolCallResult {
        let Some(from_width) = parameters.get("from_width").and_then(Value::as_f64) else {
            return ToolCallResult::error(
                "Missing required parameter: parameters.from_width (number)",
            );
        };

        let Some(to_width) = parameters.get("to_width").and_then(Value::as_f64) else {
            return ToolCallResult::error(
                "Missing required parameter: parameters.to_width (number)",
            );
        };

        let tolerance = parameters
            .get("tolerance")
            .and_then(Value::as_f64)
            .unwrap_or(0.001);

        if to_width <= 0.0 {
            return ToolCallResult::error("to_width must be greater than 0");
        }

        let mut total_updated = 0usize;
        let mut footprints_updated = Vec::new();

        for fp in library.footprints_mut() {
            let mut fp_count = 0usize;

            for track in &mut fp.tracks {
                if (track.width - from_width).abs() <= tolerance {
                    track.width = to_width;
                    fp_count += 1;
                }
            }

            if fp_count > 0 {
                footprints_updated.push(json!({
                    "name": fp.name,
                    "tracks_updated": fp_count,
                }));
                total_updated += fp_count;
            }
        }

        // Write the updated library if any changes were made
        if total_updated > 0 {
            if let Err(e) = library.write(filepath) {
                return ToolCallResult::error(format!("Failed to write updated library: {e}"));
            }
        }

        let result = json!({
            "status": "success",
            "operation": "update_track_width",
            "filepath": filepath,
            "from_width": from_width,
            "to_width": to_width,
            "tolerance": tolerance,
            "total_tracks_updated": total_updated,
            "footprints_updated_count": footprints_updated.len(),
            "footprints_updated": footprints_updated,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Renames layers across all footprints in a library.
    fn batch_rename_layer(
        library: &mut crate::altium::PcbLib,
        parameters: &Value,
        filepath: &str,
    ) -> ToolCallResult {
        let Some(from_layer_str) = parameters.get("from_layer").and_then(Value::as_str) else {
            return ToolCallResult::error(
                "Missing required parameter: parameters.from_layer (string)",
            );
        };

        let Some(to_layer_str) = parameters.get("to_layer").and_then(Value::as_str) else {
            return ToolCallResult::error(
                "Missing required parameter: parameters.to_layer (string)",
            );
        };

        // Parse layer names (supports both "TopLayer" and "Top Layer" formats)
        let Some(from_layer) = Self::parse_layer_name(from_layer_str) else {
            return ToolCallResult::error(format!(
                "Invalid from_layer: '{from_layer_str}'. Use format like 'Top Layer', 'Bottom Layer', \
                 'Top Overlay', 'Mechanical 1', etc."
            ));
        };

        let Some(to_layer) = Self::parse_layer_name(to_layer_str) else {
            return ToolCallResult::error(format!(
                "Invalid to_layer: '{to_layer_str}'. Use format like 'Top Layer', 'Bottom Layer', \
                 'Top Overlay', 'Mechanical 1', etc."
            ));
        };

        let mut total_updated = 0usize;
        let mut footprints_updated = Vec::new();

        for fp in library.footprints_mut() {
            let mut fp_changes = json!({
                "name": fp.name,
                "tracks": 0,
                "arcs": 0,
                "regions": 0,
                "text": 0,
            });
            let mut fp_total = 0usize;

            // Update tracks
            for track in &mut fp.tracks {
                if track.layer == from_layer {
                    track.layer = to_layer;
                    fp_changes["tracks"] = json!(fp_changes["tracks"].as_u64().unwrap_or(0) + 1);
                    fp_total += 1;
                }
            }

            // Update arcs
            for arc in &mut fp.arcs {
                if arc.layer == from_layer {
                    arc.layer = to_layer;
                    fp_changes["arcs"] = json!(fp_changes["arcs"].as_u64().unwrap_or(0) + 1);
                    fp_total += 1;
                }
            }

            // Update regions
            for region in &mut fp.regions {
                if region.layer == from_layer {
                    region.layer = to_layer;
                    fp_changes["regions"] = json!(fp_changes["regions"].as_u64().unwrap_or(0) + 1);
                    fp_total += 1;
                }
            }

            // Update text
            for text in &mut fp.text {
                if text.layer == from_layer {
                    text.layer = to_layer;
                    fp_changes["text"] = json!(fp_changes["text"].as_u64().unwrap_or(0) + 1);
                    fp_total += 1;
                }
            }

            if fp_total > 0 {
                fp_changes["total"] = json!(fp_total);
                footprints_updated.push(fp_changes);
                total_updated += fp_total;
            }
        }

        // Write the updated library if any changes were made
        if total_updated > 0 {
            if let Err(e) = library.write(filepath) {
                return ToolCallResult::error(format!("Failed to write updated library: {e}"));
            }
        }

        let result = json!({
            "status": "success",
            "operation": "rename_layer",
            "filepath": filepath,
            "from_layer": from_layer.as_str(),
            "to_layer": to_layer.as_str(),
            "total_primitives_updated": total_updated,
            "footprints_updated_count": footprints_updated.len(),
            "footprints_updated": footprints_updated,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Parses a layer name string, supporting both camelCase and spaced formats.
    fn parse_layer_name(s: &str) -> Option<crate::altium::pcblib::Layer> {
        use crate::altium::pcblib::Layer;

        // First try direct parsing (handles "Top Layer" format)
        if let Some(layer) = Layer::parse(s) {
            return Some(layer);
        }

        // Convert camelCase to spaced format and try again
        let spaced = match s {
            "TopLayer" => "Top Layer",
            "BottomLayer" => "Bottom Layer",
            "TopOverlay" => "Top Overlay",
            "BottomOverlay" => "Bottom Overlay",
            "TopPaste" => "Top Paste",
            "BottomPaste" => "Bottom Paste",
            "TopSolder" => "Top Solder",
            "BottomSolder" => "Bottom Solder",
            "MultiLayer" => "Multi-Layer",
            "KeepOutLayer" | "KeepOut" => "Keep-Out Layer",
            s if s.starts_with("MidLayer") => {
                let num = s.strip_prefix("MidLayer")?;
                return Layer::parse(&format!("Mid-Layer {num}"));
            }
            s if s.starts_with("Mechanical") => {
                let num = s.strip_prefix("Mechanical")?;
                return Layer::parse(&format!("Mechanical {num}"));
            }
            s if s.starts_with("InternalPlane") => {
                let num = s.strip_prefix("InternalPlane")?;
                return Layer::parse(&format!("Internal Plane {num}"));
            }
            _ => return None,
        };

        Layer::parse(spaced)
    }

    /// Validates a component name.
    ///
    /// Note: OLE storage names are limited to 31 characters, but the library layer
    /// handles this by truncating storage names while preserving full names in
    /// the PATTERN/LIBREFERENCE fields.
    fn validate_ole_name(name: &str) -> Result<(), String> {
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

        if name.is_empty() {
            return Err("Component name cannot be empty".to_string());
        }
        if let Some(c) = name.chars().find(|c| INVALID_CHARS.contains(c)) {
            return Err(format!(
                "Component name '{name}' contains invalid character '{c}'. \
                 Names cannot contain: / \\ : * ? \" < > |",
            ));
        }
        Ok(())
    }

    /// Copies a component within an Altium library file.
    fn call_copy_component(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(source_name) = arguments.get("source_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: source_name");
        };

        let Some(target_name) = arguments.get("target_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: target_name");
        };

        let description = arguments.get("description").and_then(Value::as_str);

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Validate target name
        if let Err(e) = Self::validate_ole_name(target_name) {
            return ToolCallResult::error(e);
        }

        // Determine file type from extension
        let ext = std::path::Path::new(filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match ext.as_deref() {
            Some("pcblib") => {
                Self::copy_pcblib_component(filepath, source_name, target_name, description)
            }
            Some("schlib") => {
                Self::copy_schlib_component(filepath, source_name, target_name, description)
            }
            Some(ext) => ToolCallResult::error(format!(
                "Unsupported file type: .{ext}. Use .PcbLib or .SchLib"
            )),
            None => ToolCallResult::error("File has no extension. Use .PcbLib or .SchLib"),
        }
    }

    /// Copies a footprint within a `PcbLib` file.
    fn copy_pcblib_component(
        filepath: &str,
        source_name: &str,
        target_name: &str,
        description: Option<&str>,
    ) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read the library
        let mut library = match PcbLib::read(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Check if target already exists
        if library.get(target_name).is_some() {
            return ToolCallResult::error(format!(
                "Component '{target_name}' already exists in library"
            ));
        }

        // Find the source component
        let Some(source) = library.get(source_name) else {
            return ToolCallResult::error(format!(
                "Source component '{source_name}' not found in library"
            ));
        };

        // Clone the footprint with new name
        let mut new_footprint = source.clone();
        new_footprint.name = target_name.to_string();
        if let Some(desc) = description {
            new_footprint.description = desc.to_string();
        }

        // Add the new footprint
        library.add(new_footprint);

        // Write the updated library
        if let Err(e) = library.write(filepath) {
            return ToolCallResult::error(format!("Failed to write library: {e}"));
        }

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "PcbLib",
            "source_name": source_name,
            "target_name": target_name,
            "component_count": library.len(),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Copies a symbol within a `SchLib` file.
    fn copy_schlib_component(
        filepath: &str,
        source_name: &str,
        target_name: &str,
        description: Option<&str>,
    ) -> ToolCallResult {
        use crate::altium::SchLib;

        // Read the library
        let mut library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Check if target already exists
        if library.get(target_name).is_some() {
            return ToolCallResult::error(format!(
                "Component '{target_name}' already exists in library"
            ));
        }

        // Find the source component
        let Some(source) = library.get(source_name) else {
            return ToolCallResult::error(format!(
                "Source component '{source_name}' not found in library"
            ));
        };

        // Clone the symbol with new name
        let mut new_symbol = source.clone();
        new_symbol.name = target_name.to_string();
        if let Some(desc) = description {
            new_symbol.description = desc.to_string();
        }

        // Add the new symbol
        library.add_symbol(new_symbol);

        // Write the updated library
        if let Err(e) = library.save(filepath) {
            return ToolCallResult::error(format!("Failed to write library: {e}"));
        }

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "SchLib",
            "source_name": source_name,
            "target_name": target_name,
            "component_count": library.len(),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    // ==================== Component Rename ====================

    /// Renames a component within an Altium library file.
    fn call_rename_component(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(old_name) = arguments.get("old_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: old_name");
        };

        let Some(new_name) = arguments.get("new_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: new_name");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Validate new name
        if let Err(e) = Self::validate_ole_name(new_name) {
            return ToolCallResult::error(e);
        }

        // Check for no-op rename
        if old_name == new_name {
            return ToolCallResult::error("old_name and new_name are identical");
        }

        // Determine file type from extension
        let ext = std::path::Path::new(filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match ext.as_deref() {
            Some("pcblib") => Self::rename_pcblib_component(filepath, old_name, new_name),
            Some("schlib") => Self::rename_schlib_component(filepath, old_name, new_name),
            Some(ext) => ToolCallResult::error(format!(
                "Unsupported file type: .{ext}. Use .PcbLib or .SchLib"
            )),
            None => ToolCallResult::error("File has no extension. Use .PcbLib or .SchLib"),
        }
    }

    /// Renames a footprint within a `PcbLib` file.
    fn rename_pcblib_component(filepath: &str, old_name: &str, new_name: &str) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read the library
        let mut library = match PcbLib::read(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Check if new name already exists
        if library.get(new_name).is_some() {
            return ToolCallResult::error(format!(
                "Component '{new_name}' already exists in library"
            ));
        }

        // Find and remove the source component
        let Some(mut footprint) = library.remove(old_name) else {
            return ToolCallResult::error(format!("Component '{old_name}' not found in library"));
        };

        // Rename and add back
        footprint.name = new_name.to_string();
        library.add(footprint);

        // Write the updated library
        if let Err(e) = library.write(filepath) {
            return ToolCallResult::error(format!("Failed to write library: {e}"));
        }

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "PcbLib",
            "old_name": old_name,
            "new_name": new_name,
            "component_count": library.len(),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Renames a symbol within a `SchLib` file.
    fn rename_schlib_component(filepath: &str, old_name: &str, new_name: &str) -> ToolCallResult {
        use crate::altium::SchLib;

        // Read the library
        let mut library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Check if new name already exists
        if library.get(new_name).is_some() {
            return ToolCallResult::error(format!(
                "Component '{new_name}' already exists in library"
            ));
        }

        // Find and remove the source component
        let Some(mut symbol) = library.remove(old_name) else {
            return ToolCallResult::error(format!("Component '{old_name}' not found in library"));
        };

        // Rename and add back
        symbol.name = new_name.to_string();
        library.add_symbol(symbol);

        // Write the updated library
        if let Err(e) = library.save(filepath) {
            return ToolCallResult::error(format!("Failed to write library: {e}"));
        }

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "SchLib",
            "old_name": old_name,
            "new_name": new_name,
            "component_count": library.len(),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    // ==================== Cross-Library Copy ====================

    /// Copies a component from one Altium library to another.
    fn call_copy_component_cross_library(&self, arguments: &Value) -> ToolCallResult {
        let Some(source_filepath) = arguments.get("source_filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: source_filepath");
        };

        let Some(target_filepath) = arguments.get("target_filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: target_filepath");
        };

        let Some(component_name) = arguments.get("component_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_name");
        };

        let new_name = arguments.get("new_name").and_then(Value::as_str);
        let description = arguments.get("description").and_then(Value::as_str);

        // Validate paths are within allowed directories
        if let Err(e) = self.validate_path(source_filepath) {
            return ToolCallResult::error(e);
        }
        if let Err(e) = self.validate_path(target_filepath) {
            return ToolCallResult::error(e);
        }

        // Validate new name if provided
        let target_name = new_name.unwrap_or(component_name);
        if let Err(e) = Self::validate_ole_name(target_name) {
            return ToolCallResult::error(e);
        }

        // Determine file types from extensions
        let source_ext = std::path::Path::new(source_filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);
        let target_ext = std::path::Path::new(target_filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        // Check that both files have the same type
        if source_ext != target_ext {
            return ToolCallResult::error(format!(
                "Source and target libraries must be the same type. Source: {}, Target: {}",
                source_ext.as_deref().unwrap_or("unknown"),
                target_ext.as_deref().unwrap_or("unknown")
            ));
        }

        match source_ext.as_deref() {
            Some("pcblib") => Self::copy_pcblib_component_cross_library(
                source_filepath,
                target_filepath,
                component_name,
                target_name,
                description,
            ),
            Some("schlib") => Self::copy_schlib_component_cross_library(
                source_filepath,
                target_filepath,
                component_name,
                target_name,
                description,
            ),
            Some(ext) => ToolCallResult::error(format!(
                "Unsupported file type: .{ext}. Use .PcbLib or .SchLib"
            )),
            None => ToolCallResult::error("Files have no extension. Use .PcbLib or .SchLib"),
        }
    }

    /// Copies a footprint from one `PcbLib` to another.
    fn copy_pcblib_component_cross_library(
        source_filepath: &str,
        target_filepath: &str,
        component_name: &str,
        target_name: &str,
        description: Option<&str>,
    ) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read the source library
        let source_library = match PcbLib::read(source_filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read source library: {e}")),
        };

        // Find the source component
        let Some(source) = source_library.get(component_name) else {
            return ToolCallResult::error(format!(
                "Component '{component_name}' not found in source library"
            ));
        };

        // Clone the footprint
        let mut new_footprint = source.clone();
        new_footprint.name = target_name.to_string();
        if let Some(desc) = description {
            new_footprint.description = desc.to_string();
        }

        // Read or create the target library
        let mut target_library = if std::path::Path::new(target_filepath).exists() {
            match PcbLib::read(target_filepath) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error(format!("Failed to read target library: {e}"))
                }
            }
        } else {
            PcbLib::new()
        };

        // Check if target already exists
        if target_library.get(target_name).is_some() {
            return ToolCallResult::error(format!(
                "Component '{target_name}' already exists in target library"
            ));
        }

        // Add the footprint to target library
        target_library.add(new_footprint);

        // Write the target library
        if let Err(e) = target_library.write(target_filepath) {
            return ToolCallResult::error(format!("Failed to write target library: {e}"));
        }

        let result = json!({
            "status": "success",
            "source_filepath": source_filepath,
            "target_filepath": target_filepath,
            "file_type": "PcbLib",
            "component_name": component_name,
            "target_name": target_name,
            "target_component_count": target_library.len(),
            "message": format!(
                "Copied '{}' from '{}' to '{}'{}",
                component_name,
                source_filepath,
                target_filepath,
                if target_name == component_name {
                    String::new()
                } else {
                    format!(" as '{target_name}'")
                }
            ),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Copies a symbol from one `SchLib` to another.
    fn copy_schlib_component_cross_library(
        source_filepath: &str,
        target_filepath: &str,
        component_name: &str,
        target_name: &str,
        description: Option<&str>,
    ) -> ToolCallResult {
        use crate::altium::SchLib;

        // Read the source library
        let source_library = match SchLib::open(source_filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read source library: {e}")),
        };

        // Find the source component
        let Some(source) = source_library.get(component_name) else {
            return ToolCallResult::error(format!(
                "Component '{component_name}' not found in source library"
            ));
        };

        // Clone the symbol
        let mut new_symbol = source.clone();
        new_symbol.name = target_name.to_string();
        if let Some(desc) = description {
            new_symbol.description = desc.to_string();
        }

        // Read or create the target library
        let mut target_library = if std::path::Path::new(target_filepath).exists() {
            match SchLib::open(target_filepath) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error(format!("Failed to read target library: {e}"))
                }
            }
        } else {
            SchLib::new()
        };

        // Check if target already exists
        if target_library.get(target_name).is_some() {
            return ToolCallResult::error(format!(
                "Component '{target_name}' already exists in target library"
            ));
        }

        // Add the symbol to target library
        target_library.add_symbol(new_symbol);

        // Write the target library
        if let Err(e) = target_library.save(target_filepath) {
            return ToolCallResult::error(format!("Failed to write target library: {e}"));
        }

        let result = json!({
            "status": "success",
            "source_filepath": source_filepath,
            "target_filepath": target_filepath,
            "file_type": "SchLib",
            "component_name": component_name,
            "target_name": target_name,
            "target_component_count": target_library.len(),
            "message": format!(
                "Copied '{}' from '{}' to '{}'{}",
                component_name,
                source_filepath,
                target_filepath,
                if target_name == component_name {
                    String::new()
                } else {
                    format!(" as '{target_name}'")
                }
            ),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Merges multiple Altium libraries into a single library.
    fn call_merge_libraries(&self, arguments: &Value) -> ToolCallResult {
        let Some(source_filepaths) = arguments.get("source_filepaths").and_then(Value::as_array)
        else {
            return ToolCallResult::error("Missing required parameter: source_filepaths");
        };

        let source_paths: Vec<&str> = source_filepaths.iter().filter_map(Value::as_str).collect();

        if source_paths.is_empty() {
            return ToolCallResult::error("source_filepaths must contain at least one path");
        }

        let Some(target_filepath) = arguments.get("target_filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: target_filepath");
        };

        let on_duplicate = arguments
            .get("on_duplicate")
            .and_then(Value::as_str)
            .unwrap_or("error");

        // Validate on_duplicate parameter
        if !["skip", "error", "rename"].contains(&on_duplicate) {
            return ToolCallResult::error("on_duplicate must be one of: 'skip', 'error', 'rename'");
        }

        // Validate all paths
        for path in &source_paths {
            if let Err(e) = self.validate_path(path) {
                return ToolCallResult::error(e);
            }
        }
        if let Err(e) = self.validate_path(target_filepath) {
            return ToolCallResult::error(e);
        }

        // Determine file types from extensions
        let source_exts: Vec<Option<String>> = source_paths
            .iter()
            .map(|p| {
                std::path::Path::new(p)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_lowercase)
            })
            .collect();

        let target_ext = std::path::Path::new(target_filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        // Check that all files have the same type
        let first_ext = &source_exts[0];
        for (i, ext) in source_exts.iter().enumerate() {
            if ext != first_ext {
                return ToolCallResult::error(format!(
                    "All source libraries must be the same type. '{}' has type {:?}, but first source has type {:?}",
                    source_paths[i],
                    ext.as_deref().unwrap_or("unknown"),
                    first_ext.as_deref().unwrap_or("unknown")
                ));
            }
        }

        // Check target matches source type
        if target_ext != *first_ext {
            return ToolCallResult::error(format!(
                "Target library type must match source libraries. Sources: {:?}, Target: {:?}",
                first_ext.as_deref().unwrap_or("unknown"),
                target_ext.as_deref().unwrap_or("unknown")
            ));
        }

        match first_ext.as_deref() {
            Some("pcblib") => {
                Self::merge_pcblib_libraries(&source_paths, target_filepath, on_duplicate)
            }
            Some("schlib") => {
                Self::merge_schlib_libraries(&source_paths, target_filepath, on_duplicate)
            }
            Some(ext) => ToolCallResult::error(format!(
                "Unsupported file type: .{ext}. Use .PcbLib or .SchLib"
            )),
            None => ToolCallResult::error("Files have no extension. Use .PcbLib or .SchLib"),
        }
    }

    /// Merges multiple `PcbLib` files into one.
    fn merge_pcblib_libraries(
        source_paths: &[&str],
        target_filepath: &str,
        on_duplicate: &str,
    ) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read or create target library
        let mut target_library = if std::path::Path::new(target_filepath).exists() {
            match PcbLib::read(target_filepath) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error(format!("Failed to read target library: {e}"))
                }
            }
        } else {
            PcbLib::new()
        };

        let initial_count = target_library.len();
        let mut merged_count = 0;
        let mut skipped_count = 0;
        let mut renamed_count = 0;
        let mut source_details: Vec<Value> = Vec::new();

        for source_path in source_paths {
            let source_library = match PcbLib::read(source_path) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to read source library '{source_path}': {e}"
                    ))
                }
            };

            let mut source_merged = 0;
            let mut source_skipped = 0;
            let mut source_renamed = 0;

            for footprint in source_library.footprints() {
                let original_name = footprint.name.clone();
                let mut fp_to_add = footprint.clone();

                if target_library.get(&original_name).is_some() {
                    match on_duplicate {
                        "skip" => {
                            source_skipped += 1;
                            skipped_count += 1;
                            continue;
                        }
                        "error" => {
                            return ToolCallResult::error(format!(
                                "Duplicate component name '{original_name}' from '{source_path}'. Use on_duplicate: 'skip' or 'rename' to handle duplicates."
                            ));
                        }
                        "rename" => {
                            // Find a unique name
                            let mut counter = 1;
                            let mut new_name = format!("{original_name}_{counter}");
                            while target_library.get(&new_name).is_some() {
                                counter += 1;
                                new_name = format!("{original_name}_{counter}");
                            }
                            fp_to_add.name = new_name;
                            source_renamed += 1;
                            renamed_count += 1;
                        }
                        _ => unreachable!(),
                    }
                }

                target_library.add(fp_to_add);
                source_merged += 1;
                merged_count += 1;
            }

            source_details.push(json!({
                "source": source_path,
                "merged": source_merged,
                "skipped": source_skipped,
                "renamed": source_renamed,
            }));
        }

        // Write the merged library
        if let Err(e) = target_library.write(target_filepath) {
            return ToolCallResult::error(format!("Failed to write target library: {e}"));
        }

        let result = json!({
            "status": "success",
            "target_filepath": target_filepath,
            "file_type": "PcbLib",
            "sources_count": source_paths.len(),
            "initial_count": initial_count,
            "merged_count": merged_count,
            "skipped_count": skipped_count,
            "renamed_count": renamed_count,
            "final_count": target_library.len(),
            "sources": source_details,
            "message": format!(
                "Merged {} components from {} sources into '{}' (total: {})",
                merged_count,
                source_paths.len(),
                target_filepath,
                target_library.len()
            ),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Merges multiple `SchLib` files into one.
    fn merge_schlib_libraries(
        source_paths: &[&str],
        target_filepath: &str,
        on_duplicate: &str,
    ) -> ToolCallResult {
        use crate::altium::SchLib;

        // Read or create target library
        let mut target_library = if std::path::Path::new(target_filepath).exists() {
            match SchLib::open(target_filepath) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error(format!("Failed to read target library: {e}"))
                }
            }
        } else {
            SchLib::new()
        };

        let initial_count = target_library.len();
        let mut merged_count = 0;
        let mut skipped_count = 0;
        let mut renamed_count = 0;
        let mut source_details: Vec<Value> = Vec::new();

        for source_path in source_paths {
            let source_library = match SchLib::open(source_path) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to read source library '{source_path}': {e}"
                    ))
                }
            };

            let mut source_merged = 0;
            let mut source_skipped = 0;
            let mut source_renamed = 0;

            // Collect symbols to avoid borrowing issues
            let symbols: Vec<_> = source_library.iter().map(|(_, s)| s.clone()).collect();

            for symbol in symbols {
                let original_name = symbol.name.clone();
                let mut sym_to_add = symbol;

                if target_library.get(&original_name).is_some() {
                    match on_duplicate {
                        "skip" => {
                            source_skipped += 1;
                            skipped_count += 1;
                            continue;
                        }
                        "error" => {
                            return ToolCallResult::error(format!(
                                "Duplicate component name '{original_name}' from '{source_path}'. Use on_duplicate: 'skip' or 'rename' to handle duplicates."
                            ));
                        }
                        "rename" => {
                            // Find a unique name
                            let mut counter = 1;
                            let mut new_name = format!("{original_name}_{counter}");
                            while target_library.get(&new_name).is_some() {
                                counter += 1;
                                new_name = format!("{original_name}_{counter}");
                            }
                            sym_to_add.name = new_name;
                            source_renamed += 1;
                            renamed_count += 1;
                        }
                        _ => unreachable!(),
                    }
                }

                target_library.add_symbol(sym_to_add);
                source_merged += 1;
                merged_count += 1;
            }

            source_details.push(json!({
                "source": source_path,
                "merged": source_merged,
                "skipped": source_skipped,
                "renamed": source_renamed,
            }));
        }

        // Write the merged library
        if let Err(e) = target_library.save(target_filepath) {
            return ToolCallResult::error(format!("Failed to write target library: {e}"));
        }

        let result = json!({
            "status": "success",
            "target_filepath": target_filepath,
            "file_type": "SchLib",
            "sources_count": source_paths.len(),
            "initial_count": initial_count,
            "merged_count": merged_count,
            "skipped_count": skipped_count,
            "renamed_count": renamed_count,
            "final_count": target_library.len(),
            "sources": source_details,
            "message": format!(
                "Merged {} components from {} sources into '{}' (total: {})",
                merged_count,
                source_paths.len(),
                target_filepath,
                target_library.len()
            ),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Searches for components across multiple libraries using regex or glob patterns.
    fn call_search_components(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepaths) = arguments.get("filepaths").and_then(Value::as_array) else {
            return ToolCallResult::error("Missing required parameter: filepaths");
        };

        let paths: Vec<&str> = filepaths.iter().filter_map(Value::as_str).collect();

        if paths.is_empty() {
            return ToolCallResult::error("filepaths must contain at least one path");
        }

        let Some(pattern) = arguments.get("pattern").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: pattern");
        };

        let pattern_type = arguments
            .get("pattern_type")
            .and_then(Value::as_str)
            .unwrap_or("glob");

        if !["glob", "regex"].contains(&pattern_type) {
            return ToolCallResult::error("pattern_type must be one of: 'glob', 'regex'");
        }

        // Validate all paths
        for path in &paths {
            if let Err(e) = self.validate_path(path) {
                return ToolCallResult::error(e);
            }
        }

        // Convert glob to regex if needed
        let regex_pattern = if pattern_type == "glob" {
            Self::glob_to_regex(pattern)
        } else {
            pattern.to_string()
        };

        // Compile the regex
        let regex = match regex::Regex::new(&format!("(?i)^{regex_pattern}$")) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(format!("Invalid pattern: {e}")),
        };

        let mut matches: Vec<Value> = Vec::new();
        let mut searched_count = 0;
        let mut errors: Vec<String> = Vec::new();

        for path in &paths {
            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_lowercase);

            match ext.as_deref() {
                Some("pcblib") => match Self::search_pcblib(path, &regex) {
                    Ok((names, count)) => {
                        for name in names {
                            matches.push(json!({
                                "name": name,
                                "library": path,
                                "type": "PcbLib"
                            }));
                        }
                        searched_count += count;
                    }
                    Err(e) => errors.push(format!("{path}: {e}")),
                },
                Some("schlib") => match Self::search_schlib(path, &regex) {
                    Ok((names, count)) => {
                        for name in names {
                            matches.push(json!({
                                "name": name,
                                "library": path,
                                "type": "SchLib"
                            }));
                        }
                        searched_count += count;
                    }
                    Err(e) => errors.push(format!("{path}: {e}")),
                },
                Some(ext) => errors.push(format!("{path}: Unsupported file type '.{ext}'")),
                None => errors.push(format!("{path}: No file extension")),
            }
        }

        let result = json!({
            "status": if errors.is_empty() { "success" } else { "partial" },
            "pattern": pattern,
            "pattern_type": pattern_type,
            "libraries_searched": paths.len(),
            "components_searched": searched_count,
            "matches_found": matches.len(),
            "matches": matches,
            "errors": if errors.is_empty() { Value::Null } else { json!(errors) },
            "message": format!(
                "Found {} matches for '{}' across {} libraries ({} components searched)",
                matches.len(),
                pattern,
                paths.len(),
                searched_count
            ),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Converts a glob pattern to a regex pattern.
    fn glob_to_regex(glob: &str) -> String {
        let mut regex = String::with_capacity(glob.len() * 2);
        for c in glob.chars() {
            match c {
                '*' => regex.push_str(".*"),
                '?' => regex.push('.'),
                '.' | '+' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' => {
                    regex.push('\\');
                    regex.push(c);
                }
                _ => regex.push(c),
            }
        }
        regex
    }

    /// Searches a `PcbLib` for component names matching the regex.
    fn search_pcblib(path: &str, regex: &regex::Regex) -> Result<(Vec<String>, usize), String> {
        use crate::altium::PcbLib;

        let library = PcbLib::read(path).map_err(|e| format!("Failed to read: {e}"))?;
        let total = library.len();
        let matching: Vec<String> = library
            .footprints()
            .filter(|fp| regex.is_match(&fp.name))
            .map(|fp| fp.name.clone())
            .collect();

        Ok((matching, total))
    }

    /// Searches a `SchLib` for component names matching the regex.
    fn search_schlib(path: &str, regex: &regex::Regex) -> Result<(Vec<String>, usize), String> {
        use crate::altium::SchLib;

        let library = SchLib::open(path).map_err(|e| format!("Failed to read: {e}"))?;
        let total = library.len();
        let matching: Vec<String> = library
            .iter()
            .filter(|(name, _)| regex.is_match(name))
            .map(|(name, _)| name.clone())
            .collect();

        Ok((matching, total))
    }

    // ==================== Rendering Tools ====================

    /// Renders an ASCII art visualisation of a footprint.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn call_render_footprint(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::PcbLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(component_name) = arguments.get("component_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_name");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional parameters
        let scale = arguments
            .get("scale")
            .and_then(Value::as_f64)
            .unwrap_or(2.0);
        let max_width = arguments
            .get("max_width")
            .and_then(Value::as_u64)
            .unwrap_or(80) as usize;
        let max_height = arguments
            .get("max_height")
            .and_then(Value::as_u64)
            .unwrap_or(40) as usize;

        if scale <= 0.0 {
            return ToolCallResult::error("scale must be greater than 0");
        }

        // Read the library
        let library = match PcbLib::read(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Find the footprint
        let Some(footprint) = library.get(component_name) else {
            let available: Vec<_> = library.footprints().take(5).map(|f| &f.name).collect();
            let hint = if available.is_empty() {
                "Library is empty".to_string()
            } else {
                format!(
                    "Available footprints include: {}{}",
                    available
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    if library.len() > 5 {
                        format!(" (and {} more)", library.len() - 5)
                    } else {
                        String::new()
                    }
                )
            };
            return ToolCallResult::error(format!(
                "Footprint '{component_name}' not found in library. {hint}"
            ));
        };

        // Render the footprint
        let ascii_art = Self::render_footprint_ascii(footprint, scale, max_width, max_height);

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "component_name": component_name,
            "scale": scale,
            "render": ascii_art,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Renders a footprint as ASCII art.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::similar_names,
        clippy::too_many_lines,
        clippy::float_cmp,
        clippy::needless_range_loop
    )]
    fn render_footprint_ascii(
        footprint: &crate::altium::pcblib::Footprint,
        scale: f64,
        max_width: usize,
        max_height: usize,
    ) -> String {
        use std::fmt::Write;

        // Find bounding box
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);

        for pad in &footprint.pads {
            let half_w = pad.width / 2.0;
            let half_h = pad.height / 2.0;
            min_x = min_x.min(pad.x - half_w);
            max_x = max_x.max(pad.x + half_w);
            min_y = min_y.min(pad.y - half_h);
            max_y = max_y.max(pad.y + half_h);
        }

        for track in &footprint.tracks {
            min_x = min_x.min(track.x1.min(track.x2) - track.width / 2.0);
            max_x = max_x.max(track.x1.max(track.x2) + track.width / 2.0);
            min_y = min_y.min(track.y1.min(track.y2) - track.width / 2.0);
            max_y = max_y.max(track.y1.max(track.y2) + track.width / 2.0);
        }

        for arc in &footprint.arcs {
            min_x = min_x.min(arc.x - arc.radius);
            max_x = max_x.max(arc.x + arc.radius);
            min_y = min_y.min(arc.y - arc.radius);
            max_y = max_y.max(arc.y + arc.radius);
        }

        // Handle empty footprint
        if min_x == f64::MAX {
            return "Empty footprint (no primitives)".to_string();
        }

        // Add margin
        let margin = 0.5;
        min_x -= margin;
        max_x += margin;
        min_y -= margin;
        max_y += margin;

        // Calculate canvas size
        let width_mm = max_x - min_x;
        let height_mm = max_y - min_y;
        let mut canvas_width = (width_mm * scale).ceil() as usize;
        let mut canvas_height = (height_mm * scale).ceil() as usize;

        // Clamp to max dimensions
        if canvas_width > max_width {
            canvas_width = max_width;
        }
        if canvas_height > max_height {
            canvas_height = max_height;
        }

        // Ensure minimum size
        canvas_width = canvas_width.max(10);
        canvas_height = canvas_height.max(5);

        // Calculate actual scale after clamping
        let actual_scale_x = canvas_width as f64 / width_mm;
        let actual_scale_y = canvas_height as f64 / height_mm;

        // Create canvas (y is inverted for display)
        let mut canvas = vec![vec![' '; canvas_width]; canvas_height];

        // Helper to convert coordinates
        let to_canvas = |x: f64, y: f64| -> (usize, usize) {
            let cx = ((x - min_x) * actual_scale_x).round() as usize;
            let cy =
                canvas_height.saturating_sub(1) - ((y - min_y) * actual_scale_y).round() as usize;
            (cx.min(canvas_width - 1), cy.min(canvas_height - 1))
        };

        // Draw tracks (as lines)
        for track in &footprint.tracks {
            Self::draw_line(
                &mut canvas,
                to_canvas(track.x1, track.y1),
                to_canvas(track.x2, track.y2),
                '-',
            );
        }

        // Draw arcs (simplified as circles at centre)
        for arc in &footprint.arcs {
            let (cx, cy) = to_canvas(arc.x, arc.y);
            if cx < canvas_width && cy < canvas_height {
                canvas[cy][cx] = 'o';
            }
        }

        // Draw pads (as rectangles with designator)
        for pad in &footprint.pads {
            let half_w = pad.width / 2.0;
            let half_h = pad.height / 2.0;
            let (x1, y1) = to_canvas(pad.x - half_w, pad.y - half_h);
            let (x2, y2) = to_canvas(pad.x + half_w, pad.y + half_h);

            // Fill pad area
            let (min_cy, max_cy) = (y1.min(y2), y1.max(y2));
            let (min_cx, max_cx) = (x1.min(x2), x1.max(x2));

            for cy in min_cy..=max_cy {
                for cx in min_cx..=max_cx {
                    if cy < canvas_height && cx < canvas_width {
                        canvas[cy][cx] = '#';
                    }
                }
            }

            // Place designator at centre
            let (cx, cy) = to_canvas(pad.x, pad.y);
            if cx < canvas_width && cy < canvas_height {
                let designator_char = pad.designator.chars().next().unwrap_or('#');
                canvas[cy][cx] = designator_char;
            }
        }

        // Draw origin crosshair
        let (ox, oy) = to_canvas(0.0, 0.0);
        if ox < canvas_width && oy < canvas_height {
            canvas[oy][ox] = '+';
        }

        // Build output string
        let mut output = String::new();
        let _ = writeln!(
            output,
            "Footprint: {} ({:.2} x {:.2} mm)",
            footprint.name,
            width_mm - margin * 2.0,
            height_mm - margin * 2.0
        );
        let _ = writeln!(
            output,
            "Pads: {}, Tracks: {}, Arcs: {}",
            footprint.pads.len(),
            footprint.tracks.len(),
            footprint.arcs.len()
        );
        output.push_str(&"-".repeat(canvas_width + 2));
        output.push('\n');

        for row in &canvas {
            output.push('|');
            for &ch in row {
                output.push(ch);
            }
            output.push('|');
            output.push('\n');
        }

        output.push_str(&"-".repeat(canvas_width + 2));
        output.push('\n');
        output.push_str("Legend: # = pad, - = track, o = arc, + = origin\n");

        output
    }

    /// Draws a line on the canvas using Bresenham's algorithm.
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn draw_line(
        canvas: &mut [Vec<char>],
        (x0, y0): (usize, usize),
        (x1, y1): (usize, usize),
        ch: char,
    ) {
        let dx = (x1 as isize - x0 as isize).abs();
        let dy = (y1 as isize - y0 as isize).abs();
        let sx: isize = if x0 < x1 { 1 } else { -1 };
        let sy: isize = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;

        let mut x = x0 as isize;
        let mut y = y0 as isize;

        let height = canvas.len();
        let width = if height > 0 { canvas[0].len() } else { 0 };

        loop {
            if (x as usize) < width && (y as usize) < height {
                canvas[y as usize][x as usize] = ch;
            }

            if x == x1 as isize && y == y1 as isize {
                break;
            }

            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Renders an ASCII art visualisation of a schematic symbol.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn call_render_symbol(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::SchLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(component_name) = arguments.get("component_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_name");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional parameters
        let scale = arguments
            .get("scale")
            .and_then(Value::as_f64)
            .unwrap_or(1.0);
        let max_width = arguments
            .get("max_width")
            .and_then(Value::as_u64)
            .unwrap_or(80) as usize;
        let max_height = arguments
            .get("max_height")
            .and_then(Value::as_u64)
            .unwrap_or(40) as usize;
        let part_id = arguments
            .get("part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        if scale <= 0.0 {
            return ToolCallResult::error("scale must be greater than 0");
        }

        // Read the library
        let library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Find the symbol
        let Some(symbol) = library.get(component_name) else {
            let available: Vec<_> = library
                .iter()
                .take(5)
                .map(|(name, _)| name.as_str())
                .collect();
            let hint = if available.is_empty() {
                "Library is empty".to_string()
            } else {
                format!(
                    "Available symbols include: {}{}",
                    available.join(", "),
                    if library.len() > 5 {
                        format!(" (and {} more)", library.len() - 5)
                    } else {
                        String::new()
                    }
                )
            };
            return ToolCallResult::error(format!(
                "Symbol '{component_name}' not found in library. {hint}"
            ));
        };

        // Render the symbol
        let ascii_art = Self::render_symbol_ascii(symbol, scale, max_width, max_height, part_id);

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "component_name": component_name,
            "scale": scale,
            "part_id": part_id,
            "render": ascii_art,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Renders a schematic symbol as ASCII art.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::similar_names,
        clippy::too_many_lines,
        clippy::float_cmp,
        clippy::needless_range_loop
    )]
    fn render_symbol_ascii(
        symbol: &crate::altium::schlib::Symbol,
        scale: f64,
        max_width: usize,
        max_height: usize,
        part_id: i32,
    ) -> String {
        use crate::altium::schlib::PinOrientation;
        use std::fmt::Write;

        // Find bounding box (in schematic units)
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);

        // Helper to check if primitive belongs to requested part
        let matches_part = |owner_part_id: i32| -> bool {
            part_id == 0 || owner_part_id == part_id || owner_part_id == 0
        };

        // Calculate bounding box from pins (include pin length)
        for pin in &symbol.pins {
            if !matches_part(pin.owner_part_id) {
                continue;
            }
            let (px, py) = (pin.x, pin.y);
            let (end_x, end_y) = match pin.orientation {
                PinOrientation::Right => (px + pin.length, py),
                PinOrientation::Left => (px - pin.length, py),
                PinOrientation::Up => (px, py + pin.length),
                PinOrientation::Down => (px, py - pin.length),
            };
            min_x = min_x.min(px).min(end_x);
            max_x = max_x.max(px).max(end_x);
            min_y = min_y.min(py).min(end_y);
            max_y = max_y.max(py).max(end_y);
        }

        // Calculate bounding box from rectangles
        for rect in &symbol.rectangles {
            if !matches_part(rect.owner_part_id) {
                continue;
            }
            min_x = min_x.min(rect.x1).min(rect.x2);
            max_x = max_x.max(rect.x1).max(rect.x2);
            min_y = min_y.min(rect.y1).min(rect.y2);
            max_y = max_y.max(rect.y1).max(rect.y2);
        }

        // Calculate bounding box from lines
        for line in &symbol.lines {
            if !matches_part(line.owner_part_id) {
                continue;
            }
            min_x = min_x.min(line.x1).min(line.x2);
            max_x = max_x.max(line.x1).max(line.x2);
            min_y = min_y.min(line.y1).min(line.y2);
            max_y = max_y.max(line.y1).max(line.y2);
        }

        // Calculate bounding box from polylines
        for polyline in &symbol.polylines {
            if !matches_part(polyline.owner_part_id) {
                continue;
            }
            for &(x, y) in &polyline.points {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }

        // Calculate bounding box from arcs
        for arc in &symbol.arcs {
            if !matches_part(arc.owner_part_id) {
                continue;
            }
            min_x = min_x.min(arc.x - arc.radius);
            max_x = max_x.max(arc.x + arc.radius);
            min_y = min_y.min(arc.y - arc.radius);
            max_y = max_y.max(arc.y + arc.radius);
        }

        // Calculate bounding box from ellipses
        for ellipse in &symbol.ellipses {
            if !matches_part(ellipse.owner_part_id) {
                continue;
            }
            min_x = min_x.min(ellipse.x - ellipse.radius_x);
            max_x = max_x.max(ellipse.x + ellipse.radius_x);
            min_y = min_y.min(ellipse.y - ellipse.radius_y);
            max_y = max_y.max(ellipse.y + ellipse.radius_y);
        }

        // Handle empty symbol
        if min_x == i32::MAX {
            return "Empty symbol (no primitives)".to_string();
        }

        // Add margin (10 schematic units = 1 grid)
        let margin = 10;
        min_x -= margin;
        max_x += margin;
        min_y -= margin;
        max_y += margin;

        // Calculate canvas size (scale is chars per 10 schematic units)
        let width_units = f64::from(max_x - min_x);
        let height_units = f64::from(max_y - min_y);
        let mut canvas_width = ((width_units / 10.0) * scale).ceil() as usize;
        let mut canvas_height = ((height_units / 10.0) * scale).ceil() as usize;

        // Clamp to max dimensions
        canvas_width = canvas_width.clamp(10, max_width);
        canvas_height = canvas_height.clamp(5, max_height);

        // Calculate actual scale after clamping
        let actual_scale_x = canvas_width as f64 / width_units;
        let actual_scale_y = canvas_height as f64 / height_units;

        // Create canvas (y is inverted for display)
        let mut canvas = vec![vec![' '; canvas_width]; canvas_height];

        // Helper to convert schematic coordinates to canvas coordinates
        let to_canvas = |x: i32, y: i32| -> (usize, usize) {
            let cx = (f64::from(x - min_x) * actual_scale_x).round() as usize;
            let cy = canvas_height.saturating_sub(1)
                - (f64::from(y - min_y) * actual_scale_y).round() as usize;
            (cx.min(canvas_width - 1), cy.min(canvas_height - 1))
        };

        // Draw rectangles (as box outlines or filled)
        for rect in &symbol.rectangles {
            if !matches_part(rect.owner_part_id) {
                continue;
            }
            let (x1, y1) = to_canvas(rect.x1, rect.y1);
            let (x2, y2) = to_canvas(rect.x2, rect.y2);
            let (min_cx, max_cx) = (x1.min(x2), x1.max(x2));
            let (min_cy, max_cy) = (y1.min(y2), y1.max(y2));

            // Draw top and bottom edges
            for cx in min_cx..=max_cx {
                if cx < canvas_width {
                    if min_cy < canvas_height {
                        canvas[min_cy][cx] = '-';
                    }
                    if max_cy < canvas_height {
                        canvas[max_cy][cx] = '-';
                    }
                }
            }
            // Draw left and right edges
            for cy in min_cy..=max_cy {
                if cy < canvas_height {
                    if min_cx < canvas_width {
                        canvas[cy][min_cx] = '|';
                    }
                    if max_cx < canvas_width {
                        canvas[cy][max_cx] = '|';
                    }
                }
            }
            // Draw corners
            if min_cy < canvas_height && min_cx < canvas_width {
                canvas[min_cy][min_cx] = '+';
            }
            if min_cy < canvas_height && max_cx < canvas_width {
                canvas[min_cy][max_cx] = '+';
            }
            if max_cy < canvas_height && min_cx < canvas_width {
                canvas[max_cy][min_cx] = '+';
            }
            if max_cy < canvas_height && max_cx < canvas_width {
                canvas[max_cy][max_cx] = '+';
            }
        }

        // Draw lines
        for line in &symbol.lines {
            if !matches_part(line.owner_part_id) {
                continue;
            }
            Self::draw_line(
                &mut canvas,
                to_canvas(line.x1, line.y1),
                to_canvas(line.x2, line.y2),
                '-',
            );
        }

        // Draw polylines
        for polyline in &symbol.polylines {
            if !matches_part(polyline.owner_part_id) {
                continue;
            }
            for i in 0..polyline.points.len().saturating_sub(1) {
                let (x1, y1) = polyline.points[i];
                let (x2, y2) = polyline.points[i + 1];
                Self::draw_line(&mut canvas, to_canvas(x1, y1), to_canvas(x2, y2), '-');
            }
        }

        // Draw arcs (simplified as circles at centre)
        for arc in &symbol.arcs {
            if !matches_part(arc.owner_part_id) {
                continue;
            }
            let (cx, cy) = to_canvas(arc.x, arc.y);
            if cx < canvas_width && cy < canvas_height {
                canvas[cy][cx] = 'o';
            }
        }

        // Draw ellipses (simplified as circles at centre)
        for ellipse in &symbol.ellipses {
            if !matches_part(ellipse.owner_part_id) {
                continue;
            }
            let (cx, cy) = to_canvas(ellipse.x, ellipse.y);
            if cx < canvas_width && cy < canvas_height {
                canvas[cy][cx] = 'O';
            }
        }

        // Draw pins
        for pin in &symbol.pins {
            if !matches_part(pin.owner_part_id) {
                continue;
            }
            let (px, py) = (pin.x, pin.y);
            let (end_x, end_y) = match pin.orientation {
                PinOrientation::Right => (px + pin.length, py),
                PinOrientation::Left => (px - pin.length, py),
                PinOrientation::Up => (px, py + pin.length),
                PinOrientation::Down => (px, py - pin.length),
            };

            // Draw pin line
            Self::draw_line(&mut canvas, to_canvas(px, py), to_canvas(end_x, end_y), '~');

            // Draw connection point (at pin position, not end)
            let (cx, cy) = to_canvas(px, py);
            if cx < canvas_width && cy < canvas_height {
                // Use designator first char or '*'
                let pin_char = pin.designator.chars().next().unwrap_or('*');
                canvas[cy][cx] = pin_char;
            }
        }

        // Draw origin crosshair
        let (ox, oy) = to_canvas(0, 0);
        if ox < canvas_width && oy < canvas_height && canvas[oy][ox] == ' ' {
            canvas[oy][ox] = '+';
        }

        // Count primitives for summary
        let pin_count = symbol
            .pins
            .iter()
            .filter(|p| matches_part(p.owner_part_id))
            .count();
        let rect_count = symbol
            .rectangles
            .iter()
            .filter(|r| matches_part(r.owner_part_id))
            .count();
        let line_count = symbol
            .lines
            .iter()
            .filter(|l| matches_part(l.owner_part_id))
            .count();

        // Build output string
        let mut output = String::new();
        let _ = writeln!(
            output,
            "Symbol: {} (part {}/{})",
            symbol.name,
            if part_id == 0 { 1 } else { part_id },
            symbol.part_count
        );
        let _ = writeln!(
            output,
            "Pins: {pin_count}, Rectangles: {rect_count}, Lines: {line_count}"
        );
        output.push_str(&"-".repeat(canvas_width + 2));
        output.push('\n');

        for row in &canvas {
            output.push('|');
            for &ch in row {
                output.push(ch);
            }
            output.push('|');
            output.push('\n');
        }

        output.push_str(&"-".repeat(canvas_width + 2));
        output.push('\n');
        output.push_str("Legend: 1-9/* = pin, |-+ = rectangle, ~ = pin line, o = arc, O = ellipse, + = origin\n");

        output
    }

    /// Manages parameters in a `SchLib` symbol.
    #[allow(clippy::too_many_lines, clippy::option_if_let_else)]
    fn call_manage_schlib_parameters(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::schlib::{Parameter, SchLib};

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(component_name) = arguments.get("component_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_name");
        };

        let Some(operation) = arguments.get("operation").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: operation");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Validate file extension
        let ext = std::path::Path::new(filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        if ext.as_deref() != Some("schlib") {
            return ToolCallResult::error("manage_schlib_parameters only supports SchLib files");
        }

        // Handle read-only operations without loading the full library first
        match operation {
            "list" | "get" => {
                // Read the library
                let library = match SchLib::open(filepath) {
                    Ok(lib) => lib,
                    Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
                };

                // Find the symbol
                let Some(symbol) = library.get(component_name) else {
                    return ToolCallResult::error(format!(
                        "Symbol '{component_name}' not found in library"
                    ));
                };

                if operation == "list" {
                    // List all parameters
                    let params: Vec<_> = symbol
                        .parameters
                        .iter()
                        .map(|p| {
                            json!({
                                "name": p.name,
                                "value": p.value,
                                "hidden": p.hidden,
                                "x": p.x,
                                "y": p.y,
                            })
                        })
                        .collect();

                    let result = json!({
                        "status": "success",
                        "filepath": filepath,
                        "component_name": component_name,
                        "operation": "list",
                        "parameters": params,
                        "count": params.len(),
                    });

                    return ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap());
                }

                // Get single parameter
                let Some(parameter_name) = arguments.get("parameter_name").and_then(Value::as_str)
                else {
                    return ToolCallResult::error(
                        "Missing required parameter: parameter_name (required for get operation)",
                    );
                };

                let param = symbol
                    .parameters
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(parameter_name));

                match param {
                    Some(p) => {
                        let result = json!({
                            "status": "success",
                            "filepath": filepath,
                            "component_name": component_name,
                            "operation": "get",
                            "parameter": {
                                "name": p.name,
                                "value": p.value,
                                "hidden": p.hidden,
                                "x": p.x,
                                "y": p.y,
                            },
                        });
                        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
                    }
                    None => ToolCallResult::error(format!(
                        "Parameter '{parameter_name}' not found in symbol '{component_name}'"
                    )),
                }
            }

            "set" | "add" | "delete" => {
                // These operations require modifying the library
                let mut library = match SchLib::open(filepath) {
                    Ok(lib) => lib,
                    Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
                };

                // Find the symbol (mutable)
                let Some(symbol) = library.symbols.get_mut(component_name) else {
                    return ToolCallResult::error(format!(
                        "Symbol '{component_name}' not found in library"
                    ));
                };

                let Some(parameter_name) = arguments.get("parameter_name").and_then(Value::as_str)
                else {
                    return ToolCallResult::error(format!(
                        "Missing required parameter: parameter_name (required for {operation} operation)"
                    ));
                };

                let result = match operation {
                    "set" => {
                        // Find and update existing parameter
                        let param = symbol
                            .parameters
                            .iter_mut()
                            .find(|p| p.name.eq_ignore_ascii_case(parameter_name));

                        match param {
                            Some(p) => {
                                // Update value if provided
                                if let Some(value) = arguments.get("value").and_then(Value::as_str)
                                {
                                    p.value = value.to_string();
                                }
                                // Update hidden if provided
                                if let Some(hidden) =
                                    arguments.get("hidden").and_then(Value::as_bool)
                                {
                                    p.hidden = hidden;
                                }

                                json!({
                                    "status": "success",
                                    "filepath": filepath,
                                    "component_name": component_name,
                                    "operation": "set",
                                    "parameter": {
                                        "name": p.name.clone(),
                                        "value": p.value.clone(),
                                        "hidden": p.hidden,
                                    },
                                })
                            }
                            None => {
                                return ToolCallResult::error(format!(
                                    "Parameter '{parameter_name}' not found in symbol '{component_name}'. \
                                     Use 'add' operation to create a new parameter."
                                ));
                            }
                        }
                    }

                    "add" => {
                        // Check if parameter already exists
                        if symbol
                            .parameters
                            .iter()
                            .any(|p| p.name.eq_ignore_ascii_case(parameter_name))
                        {
                            return ToolCallResult::error(format!(
                                "Parameter '{parameter_name}' already exists in symbol '{component_name}'. \
                                 Use 'set' operation to update it."
                            ));
                        }

                        let Some(value) = arguments.get("value").and_then(Value::as_str) else {
                            return ToolCallResult::error(
                                "Missing required parameter: value (required for add operation)",
                            );
                        };

                        let mut param = Parameter::new(parameter_name, value);

                        // Apply optional properties
                        if let Some(hidden) = arguments.get("hidden").and_then(Value::as_bool) {
                            param.hidden = hidden;
                        }
                        if let Some(x) = arguments.get("x").and_then(Value::as_i64) {
                            #[allow(clippy::cast_possible_truncation)]
                            {
                                param.x = x as i32;
                            }
                        }
                        if let Some(y) = arguments.get("y").and_then(Value::as_i64) {
                            #[allow(clippy::cast_possible_truncation)]
                            {
                                param.y = y as i32;
                            }
                        }

                        symbol.add_parameter(param);

                        json!({
                            "status": "success",
                            "filepath": filepath,
                            "component_name": component_name,
                            "operation": "add",
                            "parameter": {
                                "name": parameter_name,
                                "value": value,
                            },
                        })
                    }

                    "delete" => {
                        // Find and remove parameter
                        let original_len = symbol.parameters.len();
                        symbol
                            .parameters
                            .retain(|p| !p.name.eq_ignore_ascii_case(parameter_name));

                        if symbol.parameters.len() == original_len {
                            return ToolCallResult::error(format!(
                                "Parameter '{parameter_name}' not found in symbol '{component_name}'"
                            ));
                        }

                        json!({
                            "status": "success",
                            "filepath": filepath,
                            "component_name": component_name,
                            "operation": "delete",
                            "deleted_parameter": parameter_name,
                        })
                    }

                    _ => unreachable!(),
                };

                // Save the modified library
                if let Err(e) = library.save(filepath) {
                    return ToolCallResult::error(format!("Failed to save library: {e}"));
                }

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }

            _ => ToolCallResult::error(format!(
                "Unknown operation: {operation}. Valid operations: list, get, set, add, delete"
            )),
        }
    }

    /// Manages footprint links in a `SchLib` symbol.
    #[allow(clippy::too_many_lines)]
    fn call_manage_schlib_footprints(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::schlib::{FootprintModel, SchLib};

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(component_name) = arguments.get("component_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_name");
        };

        let Some(operation) = arguments.get("operation").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: operation");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Validate file extension
        let ext = std::path::Path::new(filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        if ext.as_deref() != Some("schlib") {
            return ToolCallResult::error("manage_schlib_footprints only supports SchLib files");
        }

        match operation {
            "list" => {
                // Read the library
                let library = match SchLib::open(filepath) {
                    Ok(lib) => lib,
                    Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
                };

                // Find the symbol
                let Some(symbol) = library.get(component_name) else {
                    return ToolCallResult::error(format!(
                        "Symbol '{component_name}' not found in library"
                    ));
                };

                // List all footprints
                let footprints: Vec<_> = symbol
                    .footprints
                    .iter()
                    .map(|f| {
                        json!({
                            "name": f.name,
                            "description": f.description,
                        })
                    })
                    .collect();

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "component_name": component_name,
                    "operation": "list",
                    "footprints": footprints,
                    "count": footprints.len(),
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }

            "add" | "remove" => {
                let Some(footprint_name) = arguments.get("footprint_name").and_then(Value::as_str)
                else {
                    return ToolCallResult::error(format!(
                        "Missing required parameter: footprint_name (required for {operation} operation)"
                    ));
                };

                // Read the library
                let mut library = match SchLib::open(filepath) {
                    Ok(lib) => lib,
                    Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
                };

                // Find the symbol (mutable)
                let Some(symbol) = library.symbols.get_mut(component_name) else {
                    return ToolCallResult::error(format!(
                        "Symbol '{component_name}' not found in library"
                    ));
                };

                let result = if operation == "add" {
                    // Check if footprint already exists
                    if symbol
                        .footprints
                        .iter()
                        .any(|f| f.name.eq_ignore_ascii_case(footprint_name))
                    {
                        return ToolCallResult::error(format!(
                            "Footprint '{footprint_name}' already linked to symbol '{component_name}'"
                        ));
                    }

                    let mut footprint = FootprintModel::new(footprint_name);

                    // Apply optional description
                    if let Some(desc) = arguments.get("description").and_then(Value::as_str) {
                        footprint.description = desc.to_string();
                    }

                    symbol.add_footprint(footprint);

                    json!({
                        "status": "success",
                        "filepath": filepath,
                        "component_name": component_name,
                        "operation": "add",
                        "footprint": footprint_name,
                    })
                } else {
                    // Remove footprint
                    let original_len = symbol.footprints.len();
                    symbol
                        .footprints
                        .retain(|f| !f.name.eq_ignore_ascii_case(footprint_name));

                    if symbol.footprints.len() == original_len {
                        return ToolCallResult::error(format!(
                            "Footprint '{footprint_name}' not found in symbol '{component_name}'"
                        ));
                    }

                    json!({
                        "status": "success",
                        "filepath": filepath,
                        "component_name": component_name,
                        "operation": "remove",
                        "removed_footprint": footprint_name,
                    })
                };

                // Save the modified library
                if let Err(e) = library.save(filepath) {
                    return ToolCallResult::error(format!("Failed to save library: {e}"));
                }

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }

            _ => ToolCallResult::error(format!(
                "Unknown operation: {operation}. Valid operations: list, add, remove"
            )),
        }
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
