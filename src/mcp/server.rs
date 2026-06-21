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
use crate::security::{AuditEvent, AuditLogger, AuditOutcome, RateLimiter};

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
    ///
    /// The message is routed through [`crate::util::redact_absolute_paths`] as a
    /// defence-in-depth choke-point, so no error returned to the client can
    /// disclose an absolute filesystem path even if a call site forgot to
    /// sanitise one.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text {
                text: crate::util::redact_absolute_paths(&message.into()),
            }],
            is_error: true,
        }
    }

    /// Builds a sanitised structured error from a [`crate::altium::AltiumError`].
    ///
    /// The error's `Display` is already path-sanitised (file names only, never
    /// full paths — see [`crate::altium::error::sanitise_path_for_client`]), so
    /// routing every `AltiumError` through this single choke-point keeps the
    /// JSON error shape consistent and leak-proof rather than re-deriving it at
    /// each call site.
    #[must_use]
    pub fn from_altium(operation: impl Into<String>, err: &crate::altium::AltiumError) -> Self {
        Self::error_with_context(ErrorContext::new(operation, err.to_string()))
    }

    /// Creates a structured error with context.
    ///
    /// Returns a JSON-formatted error with operation context for better debugging.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // owned ErrorContext is the builder-style API
    pub fn error_with_context(context: ErrorContext) -> Self {
        // Redact absolute paths from every client-facing field (defence in depth).
        let message = crate::util::redact_absolute_paths(&context.message);
        let filepath = context
            .filepath
            .as_deref()
            .map(crate::util::redact_absolute_paths);
        let details = context
            .details
            .as_deref()
            .map(crate::util::redact_absolute_paths);
        let result = json!({
            "status": "error",
            "operation": context.operation,
            "error": message,
            "filepath": filepath,
            "component": context.component,
            "details": details,
        });
        Self {
            content: vec![ToolContent::Text {
                text: serde_json::to_string_pretty(&result).unwrap_or(message),
            }],
            is_error: true,
        }
    }
}

/// Context for structured error reporting.
#[derive(Debug, Default)]
pub struct ErrorContext {
    /// The operation being performed (e.g., `write_pcblib`, `delete_component`).
    pub operation: String,
    /// The error message.
    pub message: String,
    /// The file path being operated on (if applicable).
    pub filepath: Option<String>,
    /// The component name being processed (if applicable).
    pub component: Option<String>,
    /// Additional details about what was happening.
    pub details: Option<String>,
}

impl ErrorContext {
    /// Creates a new error context for an operation.
    #[must_use]
    pub fn new(operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            message: message.into(),
            ..Default::default()
        }
    }

    /// Sets the filepath for this error context.
    #[must_use]
    pub fn with_filepath(mut self, filepath: impl Into<String>) -> Self {
        self.filepath = Some(filepath.into());
        self
    }

    /// Sets the component name for this error context.
    #[must_use]
    pub fn with_component(mut self, component: impl Into<String>) -> Self {
        self.component = Some(component.into());
        self
    }

    /// Sets additional details for this error context.
    #[must_use]
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
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
    /// Rate limiter for destructive (file-mutating) operations.
    rate_limiter: RateLimiter,
    /// Optional append-only audit log for destructive operations.
    audit_logger: Option<AuditLogger>,
}

impl McpServer {
    /// Creates a new MCP server with the given allowed paths.
    ///
    /// The rate limiter defaults to unlimited; production wires a configured
    /// limiter via [`McpServer::with_rate_limiter`].
    #[must_use]
    pub fn new(allowed_paths: Vec<PathBuf>) -> Self {
        Self {
            state: ServerState::AwaitingInit,
            transport: StdioTransport::new(),
            protocol_version: None,
            allowed_paths,
            rate_limiter: RateLimiter::unlimited(),
            audit_logger: None,
        }
    }

    /// Installs a configured rate limiter for destructive operations.
    ///
    /// The default constructor uses an unlimited limiter (suitable for tests);
    /// production wires a limiter built from the user's configuration.
    ///
    /// Deliberately not a `const fn`: the assignment drops the previous
    /// `RateLimiter`, and `std::sync::Mutex`'s destructor is non-trivial on
    /// some targets (e.g. macOS), so const-evaluating the drop fails there
    /// with E0493. The `missing_const_for_fn` lint only observes the
    /// futex-based targets where it *would* be const, so suppress it.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_rate_limiter(mut self, rate_limiter: RateLimiter) -> Self {
        self.rate_limiter = rate_limiter;
        self
    }

    /// Installs an append-only audit logger for destructive operations.
    #[must_use]
    pub fn with_audit_logger(mut self, audit_logger: Option<AuditLogger>) -> Self {
        self.audit_logger = audit_logger;
        self
    }

    /// Returns `true` if the named tool mutates a library file on disk.
    ///
    /// Only these destructive operations are rate limited; read-only tools
    /// (reads, listings, diffs, renders, validation) are never throttled.
    fn is_mutating_tool(name: &str) -> bool {
        matches!(
            name,
            "write_pcblib"
                | "write_schlib"
                | "delete_component"
                | "import_library"
                | "batch_update"
                | "copy_component"
                | "rename_component"
                | "copy_component_cross_library"
                | "merge_libraries"
                | "reorder_components"
                | "update_component"
                | "manage_schlib_parameters"
                | "manage_schlib_footprints"
                | "repair_library"
                | "restore_backup"
                | "bulk_rename"
                | "update_pad"
                | "update_primitive"
        )
    }

    /// Returns the current server state.
    #[must_use]
    pub const fn state(&self) -> ServerState {
        self.state
    }

    /// Validates that a path is within one of the allowed paths.
    ///
    /// Returns `Ok(())` if the path is allowed, or an error message if not.
    pub(crate) fn validate_path(&self, filepath: &str) -> Result<(), String> {
        use std::path::Path;

        // Fail closed: with no configured allowed paths, deny everything rather
        // than granting access to the entire filesystem. The CLI substitutes
        // ["."] when the config omits allowed_paths (see main.rs), so in
        // practice this branch only fires for a server built with an empty list.
        if self.allowed_paths.is_empty() {
            return Err("Access denied: no allowed directories are configured".to_string());
        }

        let path = Path::new(filepath);

        // Only ever surface the file name to the client, never the full
        // (possibly canonicalised) path or the raw OS error text.
        let name = crate::altium::error::sanitise_path_for_client(path);

        // Try to canonicalize the path. If it doesn't exist yet (for write operations),
        // canonicalize the parent directory and append the filename.
        let canonical_path = if path.exists() {
            path.canonicalize()
                .map_err(|_| format!("Failed to resolve path '{name}'"))?
        } else {
            // For new files, check the parent directory
            let parent = path.parent().ok_or_else(|| {
                format!("Invalid path '{name}': cannot create a file at the filesystem root")
            })?;
            let filename = path
                .file_name()
                .ok_or_else(|| format!("Invalid path '{name}': no filename specified"))?;
            let canonical_parent = parent.canonicalize().map_err(|_| {
                format!("Parent directory of '{name}' does not exist or is inaccessible")
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

    /// Maximum number of timestamped backups to retain per file.
    const MAX_BACKUPS: usize = 5;

    /// Creates a timestamped backup of an existing file before modification.
    ///
    /// Copies `filepath` to `filepath.YYYYMMDD_HHMMSS.bak`, keeping up to
    /// `MAX_BACKUPS` recent backups per file. Older backups are automatically
    /// cleaned up to prevent unbounded disk usage.
    ///
    /// If the source file does not exist (new file creation), this is a no-op.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(backup_path))` if a backup was created, `Ok(None)` if
    /// the source file did not exist, or an error message if the backup failed.
    pub(crate) fn create_backup(filepath: &str) -> Result<Option<String>, String> {
        use std::path::Path;

        let path = Path::new(filepath);

        // No backup needed for new files
        if !path.exists() {
            return Ok(None);
        }

        // Generate timestamped backup filename
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let backup_path = format!("{filepath}.{timestamp}.bak");

        std::fs::copy(path, &backup_path).map_err(|e| {
            format!(
                "Failed to create backup of '{}': {e}",
                path.file_name()
                    .map_or_else(|| "file".to_string(), |n| n.to_string_lossy().into_owned())
            )
        })?;

        tracing::debug!(
            source = %filepath,
            backup = %backup_path,
            "Created timestamped backup before destructive operation"
        );

        // Clean up old backups, keeping only the most recent MAX_BACKUPS
        Self::cleanup_old_backups(filepath);

        Ok(Some(backup_path))
    }

    /// Removes old backup files, keeping only the most recent `MAX_BACKUPS`.
    pub(crate) fn cleanup_old_backups(filepath: &str) {
        use std::path::Path;

        let path = Path::new(filepath);
        let Some(parent) = path.parent() else {
            return;
        };
        let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
            return;
        };

        // Pattern: filename.YYYYMMDD_HHMMSS.bak
        let prefix = format!("{filename}.");
        let suffix = ".bak";

        // Collect all matching backup files
        let mut backups: Vec<_> = match std::fs::read_dir(parent) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with(&prefix)
                        && name.ends_with(suffix)
                        && name != format!("{filename}.bak")
                    {
                        // Extract timestamp part for sorting
                        let timestamp_part = &name[prefix.len()..name.len() - suffix.len()];
                        // Validate it looks like a timestamp (YYYYMMDD_HHMMSS = 15 chars)
                        if timestamp_part.len() == 15 {
                            return Some((entry.path(), name));
                        }
                    }
                    None
                })
                .collect(),
            Err(_) => return,
        };

        // Sort by filename (timestamp) descending (newest first)
        backups.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove backups beyond MAX_BACKUPS
        for (backup_path, _) in backups.into_iter().skip(Self::MAX_BACKUPS) {
            if let Err(e) = std::fs::remove_file(&backup_path) {
                tracing::warn!(
                    path = %backup_path.display(),
                    error = %e,
                    "Failed to remove old backup"
                );
            } else {
                tracing::debug!(
                    path = %backup_path.display(),
                    "Removed old backup (exceeded MAX_BACKUPS)"
                );
            }
        }
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

        // Throttle mutating operations so a runaway AI loop cannot thrash the
        // disk with repeated full-file rewrites + backups. Reads are unmetered.
        let result = if Self::is_mutating_tool(params.name.as_str())
            && !self.rate_limiter.try_acquire()
        {
            tracing::warn!(
                tool = %params.name,
                "Rate limit exceeded; rejecting mutating operation"
            );
            ToolCallResult::error(
                "Rate limit exceeded: too many write operations in a short period. \
                 Please slow down and retry.",
            )
        } else {
            match params.name.as_str() {
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
                "reorder_components" => self.call_reorder_components(&params.arguments),
                "update_component" => self.call_update_component(&params.arguments),
                "search_components" => self.call_search_components(&params.arguments),
                "get_component" => self.call_get_component(&params.arguments),
                "component_exists" => self.call_component_exists(&params.arguments),
                "render_footprint" => self.call_render_footprint(&params.arguments),
                "render_symbol" => self.call_render_symbol(&params.arguments),
                "manage_schlib_parameters" => self.call_manage_schlib_parameters(&params.arguments),
                "manage_schlib_footprints" => self.call_manage_schlib_footprints(&params.arguments),
                "compare_components" => self.call_compare_components(&params.arguments),
                "repair_library" => self.call_repair_library(&params.arguments),
                "list_backups" => self.call_list_backups(&params.arguments),
                "restore_backup" => self.call_restore_backup(&params.arguments),
                "bulk_rename" => self.call_bulk_rename(&params.arguments),
                "update_pad" => self.call_update_pad(&params.arguments),
                "update_primitive" => self.call_update_primitive(&params.arguments),
                // Unknown tool
                _ => ToolCallResult::error(format!("Unknown tool: {}", params.name)),
            }
        };

        // Audit destructive operations at the dispatch chokepoint (best-effort;
        // never fails the call). Reads are not audited.
        if let Some(logger) = &self.audit_logger {
            if Self::is_mutating_tool(params.name.as_str()) {
                let filepath = params
                    .arguments
                    .get("filepath")
                    .or_else(|| params.arguments.get("target_filepath"))
                    .or_else(|| params.arguments.get("output_path"))
                    .and_then(Value::as_str)
                    .map(|p| {
                        crate::altium::error::sanitise_path_for_client(std::path::Path::new(p))
                    });
                let outcome = if result.is_error {
                    AuditOutcome::Error
                } else {
                    AuditOutcome::Success
                };
                logger.record(&AuditEvent::new(params.name, outcome, filepath));
            }
        }

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::altium::pcblib::{Footprint, Pad, PcbLib};
    use crate::altium::schlib::{Pin, PinOrientation, Rectangle, SchLib, Symbol};
    use tempfile::TempDir;

    /// Creates a temporary directory inside `.tmp/` for test isolation.
    /// The directory is automatically cleaned up when the returned `TempDir` is dropped.
    ///
    /// Uses an absolute path to avoid issues with parallel test execution.
    fn test_temp_dir() -> TempDir {
        let cwd = std::env::current_dir().expect("Failed to get current directory");
        let tmp_root = cwd.join(".tmp");
        std::fs::create_dir_all(&tmp_root).expect("Failed to create .tmp directory");
        tempfile::tempdir_in(&tmp_root).expect("Failed to create temp dir")
    }

    /// Helper to create a server with a temp directory as allowed path.
    fn create_test_server(temp_path: &std::path::Path) -> McpServer {
        McpServer::new(vec![temp_path.to_path_buf()])
    }

    /// Helper to create a test `PcbLib` with sample footprints.
    fn create_test_pcblib(path: &std::path::Path) {
        let mut lib = PcbLib::new();

        let mut fp1 = Footprint::new("CHIP_0402");
        fp1.description = "0402 chip resistor".to_string();
        fp1.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        fp1.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
        lib.add(fp1);

        let mut fp2 = Footprint::new("CHIP_0603");
        fp2.description = "0603 chip resistor".to_string();
        fp2.add_pad(Pad::smd("1", -0.8, 0.0, 0.8, 0.8));
        fp2.add_pad(Pad::smd("2", 0.8, 0.0, 0.8, 0.8));
        lib.add(fp2);

        lib.save(path).expect("Failed to create test PcbLib");
    }

    /// Helper to create a test `SchLib` with sample symbols.
    fn create_test_schlib(path: &std::path::Path) {
        let mut lib = SchLib::new();

        let mut sym1 = Symbol::new("RESISTOR");
        sym1.description = "Generic resistor".to_string();
        sym1.designator = "R?".to_string();
        sym1.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
        sym1.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
        sym1.add_rectangle(Rectangle::new(-10, -5, 10, 5));
        lib.add(sym1);

        let mut sym2 = Symbol::new("CAPACITOR");
        sym2.description = "Generic capacitor".to_string();
        sym2.designator = "C?".to_string();
        sym2.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
        sym2.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
        lib.add(sym2);

        lib.save(path).expect("Failed to create test SchLib");
    }

    /// Helper to extract text from a tool result.
    fn get_result_text(result: &ToolCallResult) -> &str {
        match &result.content[0] {
            ToolContent::Text { text } => text,
        }
    }

    #[test]
    fn server_initial_state() {
        let server = McpServer::new(vec![PathBuf::from(".")]);
        assert_eq!(server.state(), ServerState::AwaitingInit);
    }

    #[test]
    fn is_mutating_tool_classification() {
        for t in [
            "write_pcblib",
            "write_schlib",
            "delete_component",
            "import_library",
            "batch_update",
            "merge_libraries",
            "update_pad",
            "update_primitive",
            "bulk_rename",
            "restore_backup",
            "rename_component",
        ] {
            assert!(McpServer::is_mutating_tool(t), "{t} should be mutating");
        }

        for t in [
            "read_pcblib",
            "read_schlib",
            "list_components",
            "get_component",
            "diff_libraries",
            "search_components",
            "render_footprint",
            "validate_library",
            "list_backups",
            "compare_components",
        ] {
            assert!(
                !McpServer::is_mutating_tool(t),
                "{t} should not be mutating"
            );
        }
    }

    #[test]
    fn rate_limit_blocks_excess_mutating_calls_but_not_reads() {
        let dir = test_temp_dir();
        let mut server = McpServer::new(vec![dir.path().to_path_buf()])
            .with_rate_limiter(RateLimiter::new(2, 0.0)); // burst 2, no refill
        server.state = ServerState::Running;

        let mutating_req = |id: i64| JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(id),
            method: "tools/call".to_string(),
            params: Some(json!({ "name": "write_pcblib", "arguments": {} })),
        };

        // The first two mutating calls pass the gate (they then fail in-handler
        // on missing args, which is a normal tool error, not a rate-limit block).
        for id in 1..=2 {
            let resp = server.handle_tools_call(&mutating_req(id)).unwrap();
            assert!(
                !resp.result.to_string().contains("Rate limit exceeded"),
                "call {id} should not be rate limited"
            );
        }

        // The third mutating call is blocked by the exhausted bucket.
        let resp = server.handle_tools_call(&mutating_req(3)).unwrap();
        assert!(resp.result.to_string().contains("Rate limit exceeded"));

        // Reads are never throttled, even with the bucket exhausted.
        let read_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(4),
            method: "tools/call".to_string(),
            params: Some(json!({ "name": "list_components", "arguments": {} })),
        };
        let resp = server.handle_tools_call(&read_req).unwrap();
        assert!(!resp.result.to_string().contains("Rate limit exceeded"));
    }

    #[test]
    fn validate_coordinate_accepts_in_range_and_boundary() {
        assert!(McpServer::validate_coordinate(0.0, "x").is_ok());
        // Boundary is inclusive (the check is `abs() > MAX`, strict).
        assert!(McpServer::validate_coordinate(5000.0, "x").is_ok());
        assert!(McpServer::validate_coordinate(-5000.0, "x").is_ok());
    }

    #[test]
    fn validate_coordinate_rejects_out_of_range_and_non_finite() {
        assert!(McpServer::validate_coordinate(5000.001, "x").is_err());
        assert!(McpServer::validate_coordinate(-5000.001, "x").is_err());
        assert!(McpServer::validate_coordinate(f64::NAN, "x").is_err());
        assert!(McpServer::validate_coordinate(f64::INFINITY, "x").is_err());
        assert!(McpServer::validate_coordinate(f64::NEG_INFINITY, "x").is_err());
    }

    #[test]
    fn validate_schlib_coordinate_boundary() {
        assert!(McpServer::validate_schlib_coordinate(32000, "x").is_ok());
        assert!(McpServer::validate_schlib_coordinate(-32000, "x").is_ok());
        assert!(McpServer::validate_schlib_coordinate(32001, "x").is_err());
        assert!(McpServer::validate_schlib_coordinate(-32001, "x").is_err());
    }

    #[test]
    fn from_altium_produces_sanitised_structured_error() {
        use crate::altium::AltiumError;

        let dir = "/private/secret/dir";
        let err = AltiumError::file_write(
            std::path::PathBuf::from(format!("{dir}/Lib.pcblib.tmp")),
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        );
        let result = ToolCallResult::from_altium("write_pcblib", &err);
        assert!(result.is_error);

        let text = get_result_text(&result);
        assert!(
            !text.contains(dir),
            "structured error leaked directory: {text}"
        );
        assert!(text.contains("write_pcblib"), "missing operation: {text}");
        assert!(text.contains("Lib.pcblib.tmp"), "missing file name: {text}");
    }

    #[test]
    fn error_constructor_redacts_absolute_paths() {
        // Defence in depth: even a hand-built error message that interpolates an
        // absolute path must not disclose the directory to the client.
        let result = ToolCallResult::error("Failed at /home/user/private/Lib.PcbLib while reading");
        assert!(result.is_error);
        let text = get_result_text(&result);
        assert!(
            !text.contains("/home/user/private"),
            "leaked directory: {text}"
        );
        assert!(text.contains("Lib.PcbLib"), "lost file name: {text}");

        // Plain messages are unchanged.
        let plain = ToolCallResult::error("Missing required parameter: filepath");
        assert_eq!(
            get_result_text(&plain),
            "Missing required parameter: filepath"
        );
    }

    #[test]
    fn validate_path_empty_allowlist_denies_all() {
        // Fail-closed: a server with no configured allowed paths denies every
        // path rather than granting whole-filesystem access.
        let server = McpServer::new(vec![]);
        let result = server.validate_path("anything.PcbLib");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Access denied"));
    }

    #[test]
    fn validate_path_accepts_path_inside_allowed() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let inside = dir.path().join("new.PcbLib");
        assert!(server.validate_path(&inside.to_string_lossy()).is_ok());
    }

    #[test]
    fn validate_path_rejects_traversal_outside_allowed() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        // Escape the allowed directory via `..`; the parent canonicalises but
        // the resulting path is outside the allowlist.
        let escaping = dir.path().join("..").join("..").join("escaped.PcbLib");
        let result = server.validate_path(&escaping.to_string_lossy());
        assert!(result.is_err());

        // The denial must not leak the allowed directory or the rejected
        // absolute path — only the generic message.
        let msg = result.unwrap_err();
        assert!(msg.contains("Access denied"), "msg: {msg}");
        let allowed = dir.path().to_string_lossy().into_owned();
        assert!(!msg.contains(&allowed), "denial leaked allowed path: {msg}");
    }

    /// Property-based partition tests for the coordinate validators. The
    /// proptest prelude is glob-imported here, isolated from the surrounding
    /// (very large) test module.
    mod coordinate_proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn pcb_coordinate_in_range_always_accepts(v in -5000.0f64..=5000.0) {
                prop_assert!(McpServer::validate_coordinate(v, "x").is_ok());
            }

            #[test]
            fn pcb_coordinate_out_of_range_always_rejects(v in 5000.001f64..1.0e9) {
                prop_assert!(McpServer::validate_coordinate(v, "x").is_err());
                prop_assert!(McpServer::validate_coordinate(-v, "x").is_err());
            }

            #[test]
            fn schlib_coordinate_in_range_always_accepts(v in -32000i32..=32000) {
                prop_assert!(McpServer::validate_schlib_coordinate(v, "x").is_ok());
            }

            #[test]
            fn schlib_coordinate_out_of_range_always_rejects(v in 32001i32..i32::MAX) {
                prop_assert!(McpServer::validate_schlib_coordinate(v, "x").is_err());
                prop_assert!(McpServer::validate_schlib_coordinate(-v, "x").is_err());
            }
        }
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

    // =========================================================================
    // list_components Tool Tests
    // =========================================================================

    #[test]
    fn list_components_pcblib_success() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({ "filepath": lib_path.to_string_lossy() });

        let result = server.call_list_components(&args);
        assert!(!result.is_error, "Expected success, got error");

        let text = get_result_text(&result);
        let parsed: Value = serde_json::from_str(text).expect("Invalid JSON");

        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["file_type"], "PcbLib");
        assert_eq!(parsed["total_count"], 2);
        assert_eq!(parsed["returned_count"], 2);
        assert_eq!(parsed["offset"], 0);
        assert_eq!(parsed["has_more"], false);

        let components = parsed["components"].as_array().unwrap();
        assert!(components.contains(&json!("CHIP_0402")));
        assert!(components.contains(&json!("CHIP_0603")));
    }

    #[test]
    fn list_components_schlib_success() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("test.SchLib");
        create_test_schlib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({ "filepath": lib_path.to_string_lossy() });

        let result = server.call_list_components(&args);
        assert!(!result.is_error, "Expected success, got error");

        let text = get_result_text(&result);
        let parsed: Value = serde_json::from_str(text).expect("Invalid JSON");

        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["file_type"], "SchLib");
        assert_eq!(parsed["total_count"], 2);
        assert_eq!(parsed["returned_count"], 2);
        assert_eq!(parsed["offset"], 0);
        assert_eq!(parsed["has_more"], false);

        let components = parsed["components"].as_array().unwrap();
        assert!(components.contains(&json!("RESISTOR")));
        assert!(components.contains(&json!("CAPACITOR")));
    }

    #[test]
    fn list_components_missing_filepath() {
        let server = McpServer::new(vec![PathBuf::from(".")]);
        let args = json!({});

        let result = server.call_list_components(&args);
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Missing required parameter"));
    }

    #[test]
    fn list_components_file_not_found() {
        let temp = test_temp_dir();
        let server = create_test_server(temp.path());
        let args = json!({ "filepath": temp.path().join("nonexistent.PcbLib").to_string_lossy() });

        let result = server.call_list_components(&args);
        assert!(result.is_error);
    }

    // =========================================================================
    // get_component Tool Tests
    // =========================================================================

    #[test]
    fn get_component_pcblib_found() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "component_name": "CHIP_0402"
        });

        let result = server.call_get_component(&args);
        assert!(!result.is_error, "Expected success, got error");

        let text = get_result_text(&result);
        let parsed: Value = serde_json::from_str(text).expect("Invalid JSON");

        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["component"]["name"], "CHIP_0402");
        assert_eq!(parsed["component"]["description"], "0402 chip resistor");
    }

    #[test]
    fn get_component_pcblib_not_found() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "component_name": "NONEXISTENT"
        });

        let result = server.call_get_component(&args);
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("not found"));
    }

    #[test]
    fn get_component_schlib_found() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("test.SchLib");
        create_test_schlib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "component_name": "RESISTOR"
        });

        let result = server.call_get_component(&args);
        assert!(!result.is_error, "Expected success, got error");

        let text = get_result_text(&result);
        let parsed: Value = serde_json::from_str(text).expect("Invalid JSON");

        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["component"]["name"], "RESISTOR");
    }

    // =========================================================================
    // search_components Tool Tests
    // =========================================================================

    #[test]
    fn search_components_glob_pattern() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepaths": [lib_path.to_string_lossy()],
            "pattern": "CHIP_*"
        });

        let result = server.call_search_components(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        let text = get_result_text(&result);
        let parsed: Value = serde_json::from_str(text).expect("Invalid JSON");

        assert_eq!(parsed["status"], "success");
        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn search_components_regex_pattern() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepaths": [lib_path.to_string_lossy()],
            "pattern": ".*0402$",
            "pattern_type": "regex"
        });

        let result = server.call_search_components(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        let text = get_result_text(&result);
        let parsed: Value = serde_json::from_str(text).expect("Invalid JSON");

        assert_eq!(parsed["status"], "success");
        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["name"], "CHIP_0402");
    }

    #[test]
    fn search_components_no_matches() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepaths": [lib_path.to_string_lossy()],
            "pattern": "NONEXISTENT_*"
        });

        let result = server.call_search_components(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        let text = get_result_text(&result);
        let parsed: Value = serde_json::from_str(text).expect("Invalid JSON");

        assert_eq!(parsed["status"], "success");
        let matches = parsed["matches"].as_array().unwrap();
        assert!(matches.is_empty());
    }

    // =========================================================================
    // write_pcblib Tool Tests
    // =========================================================================

    #[test]
    fn write_pcblib_create_new() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("new_lib.PcbLib");

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "footprints": [{
                "name": "TEST_FP",
                "description": "Test footprint",
                "pads": [
                    {"designator": "1", "x": -0.5, "y": 0.0, "width": 0.6, "height": 0.5}
                ]
            }]
        });

        let result = server.call_write_pcblib(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        // Verify file was created
        assert!(lib_path.exists());

        // Verify content
        let lib = PcbLib::open(&lib_path).expect("Failed to read created library");
        assert_eq!(lib.len(), 1);
        assert!(lib.get("TEST_FP").is_some());
    }

    #[test]
    fn write_pcblib_rejects_embed_source_outside_allowlist() {
        // GAP A regression: an embedded step_model.filepath is read from disk at
        // save time (prepare_3d_models_for_writing -> std::fs::read). A caller
        // must not be able to embed a file from outside the configured
        // allow-list (arbitrary file read / exfiltration into the library).
        let allowed = test_temp_dir();
        let outside = test_temp_dir(); // a different dir, NOT on the allow-list
        let secret = outside.path().join("secret.step");
        std::fs::write(&secret, b"TOP SECRET").expect("write secret");

        let server = create_test_server(allowed.path());
        let lib_path = allowed.path().join("out.PcbLib");
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "footprints": [{
                "name": "FP1",
                "pads": [{"designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0}],
                "step_model": {"filepath": secret.to_string_lossy(), "embed": true}
            }]
        });

        let result = server.call_write_pcblib(&args);
        assert!(
            result.is_error,
            "embedding a file outside the allow-list must be rejected"
        );
        let msg = get_result_text(&result).to_lowercase();
        assert!(
            msg.contains("denied") || msg.contains("outside"),
            "expected an access-denied error, got: {msg}"
        );
        // No library should have been written, and the secret never surfaces.
        assert!(
            !lib_path.exists(),
            "library must not be written on rejection"
        );
        assert!(!msg.contains("top secret"));
    }

    #[test]
    fn extract_all_step_models_sanitises_malicious_model_name() {
        // GAP B regression: model.name comes from inside a (caller-supplied)
        // library. A crafted name must not escape the output directory via
        // Path::join with ".." or an absolute path.
        use crate::altium::pcblib::EmbeddedModel;

        let temp = test_temp_dir();
        let server = create_test_server(temp.path());
        let out_dir = temp.path().join("out");

        let models_owned = [
            EmbeddedModel::new("{A}", "../ESCAPED.step", b"DATA".to_vec()),
            EmbeddedModel::new("{B}", "..", b"DATA".to_vec()),
        ];
        let models: Vec<&EmbeddedModel> = models_owned.iter().collect();

        let result =
            server.extract_all_step_models("lib.PcbLib", Some(&out_dir.to_string_lossy()), &models);
        assert!(
            !result.is_error,
            "extract_all reports per-model errors as partial success, not a hard error"
        );

        // "../ESCAPED.step" must be reduced to a bare filename inside out_dir,
        // never written to out_dir's parent; ".." has no file component (skipped).
        assert!(
            !temp.path().join("ESCAPED.step").exists(),
            "model name escaped the output directory"
        );
        assert!(
            out_dir.join("ESCAPED.step").exists(),
            "sanitised model should be written inside out_dir"
        );
    }

    #[test]
    fn write_pcblib_append_mode() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("append_test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "footprints": [{
                "name": "NEW_FP",
                "pads": [{"designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0}]
            }],
            "append": true
        });

        let result = server.call_write_pcblib(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        // Verify original + new footprints exist
        let lib = PcbLib::open(&lib_path).expect("Failed to read library");
        assert_eq!(lib.len(), 3);
        assert!(lib.get("CHIP_0402").is_some());
        assert!(lib.get("CHIP_0603").is_some());
        assert!(lib.get("NEW_FP").is_some());
    }

    // =========================================================================
    // write_schlib Tool Tests
    // =========================================================================

    #[test]
    fn write_schlib_create_new() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("new_lib.SchLib");

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "symbols": [{
                "name": "TEST_SYM",
                "description": "Test symbol",
                "designator": "U?",
                "pins": [
                    {"name": "VCC", "designator": "1", "x": -40, "y": 0, "length": 20, "orientation": "Right"}
                ],
                "rectangles": [
                    {"x1": -30, "y1": -20, "x2": 30, "y2": 20}
                ]
            }]
        });

        let result = server.call_write_schlib(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        // Verify file was created
        assert!(lib_path.exists());

        // Verify content
        let lib = SchLib::open(&lib_path).expect("Failed to read created library");
        assert_eq!(lib.len(), 1);
        assert!(lib.get("TEST_SYM").is_some());
    }

    #[test]
    fn write_schlib_append_mode() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("append_test.SchLib");
        create_test_schlib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "symbols": [{
                "name": "NEW_SYM",
                "designator": "X?",
                "pins": [],
                "rectangles": [{"x1": -10, "y1": -10, "x2": 10, "y2": 10}]
            }],
            "append": true
        });

        let result = server.call_write_schlib(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        // Verify original + new symbols exist
        let lib = SchLib::open(&lib_path).expect("Failed to read library");
        assert_eq!(lib.len(), 3);
        assert!(lib.get("RESISTOR").is_some());
        assert!(lib.get("CAPACITOR").is_some());
        assert!(lib.get("NEW_SYM").is_some());
    }

    // =========================================================================
    // delete_component Tool Tests
    // =========================================================================

    #[test]
    fn delete_component_pcblib() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("delete_test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "component_names": ["CHIP_0402"]
        });

        let result = server.call_delete_component(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        // Verify component was deleted
        let lib = PcbLib::open(&lib_path).expect("Failed to read library");
        assert_eq!(lib.len(), 1);
        assert!(lib.get("CHIP_0402").is_none());
        assert!(lib.get("CHIP_0603").is_some());
    }

    #[test]
    fn delete_component_not_found() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("delete_test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "component_names": ["NONEXISTENT"]
        });

        let result = server.call_delete_component(&args);
        // The tool returns success but with results showing "not_found" status
        let text = get_result_text(&result);
        let parsed: Value = serde_json::from_str(text).expect("Invalid JSON");
        let results = parsed["results"]
            .as_array()
            .expect("Should have results array");
        assert!(!results.is_empty(), "Should have results");
        assert_eq!(results[0]["status"], "not_found");
    }

    // =========================================================================
    // rename_component Tool Tests
    // =========================================================================

    #[test]
    fn rename_component_pcblib() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("rename_test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "old_name": "CHIP_0402",
            "new_name": "RES_0402"
        });

        let result = server.call_rename_component(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        // Verify rename
        let lib = PcbLib::open(&lib_path).expect("Failed to read library");
        assert_eq!(lib.len(), 2);
        assert!(lib.get("CHIP_0402").is_none());
        assert!(lib.get("RES_0402").is_some());
        assert!(lib.get("CHIP_0603").is_some());
    }

    #[test]
    fn rename_component_not_found() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("rename_test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "old_name": "NONEXISTENT",
            "new_name": "NEW_NAME"
        });

        let result = server.call_rename_component(&args);
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("not found"));
    }

    // =========================================================================
    // copy_component_cross_library Tool Tests
    // =========================================================================

    #[test]
    fn copy_component_cross_library_pcblib() {
        let temp = test_temp_dir();
        let source_path = temp.path().join("source.PcbLib");
        let target_path = temp.path().join("target.PcbLib");
        create_test_pcblib(&source_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "source_filepath": source_path.to_string_lossy(),
            "target_filepath": target_path.to_string_lossy(),
            "component_name": "CHIP_0402"
        });

        let result = server.call_copy_component_cross_library(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        // Verify copy
        let target_lib = PcbLib::open(&target_path).expect("Failed to read target library");
        assert_eq!(target_lib.len(), 1);
        assert!(target_lib.get("CHIP_0402").is_some());

        // Verify source unchanged
        let source_lib = PcbLib::open(&source_path).expect("Failed to read source library");
        assert_eq!(source_lib.len(), 2);
    }

    #[test]
    fn copy_component_cross_library_with_rename() {
        let temp = test_temp_dir();
        let source_path = temp.path().join("source.PcbLib");
        let target_path = temp.path().join("target.PcbLib");
        create_test_pcblib(&source_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "source_filepath": source_path.to_string_lossy(),
            "target_filepath": target_path.to_string_lossy(),
            "component_name": "CHIP_0402",
            "new_name": "COPIED_0402"
        });

        let result = server.call_copy_component_cross_library(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        // Verify copy with new name
        let target_lib = PcbLib::open(&target_path).expect("Failed to read target library");
        assert_eq!(target_lib.len(), 1);
        assert!(target_lib.get("CHIP_0402").is_none());
        assert!(target_lib.get("COPIED_0402").is_some());
    }

    // =========================================================================
    // render_footprint Tool Tests
    // =========================================================================

    #[test]
    fn render_footprint_ascii() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("render_test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "component_name": "CHIP_0402"
        });

        let result = server.call_render_footprint(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        let text = get_result_text(&result);
        // Should contain ASCII art representation
        assert!(text.contains("CHIP_0402"), "Should contain footprint name");
    }

    // =========================================================================
    // render_symbol Tool Tests
    // =========================================================================

    #[test]
    fn render_symbol_ascii() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("render_test.SchLib");
        create_test_schlib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "component_name": "RESISTOR"
        });

        let result = server.call_render_symbol(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        let text = get_result_text(&result);
        // Should contain ASCII art representation
        assert!(text.contains("RESISTOR"), "Should contain symbol name");
    }

    #[test]
    fn render_footprint_multidigit_designators() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("multidigit.PcbLib");

        // Create footprint with multi-digit pad designators
        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("BGA_100");
        // Add pads with various designator lengths
        fp.add_pad(Pad::smd("1", -2.0, 2.0, 0.5, 0.5));
        fp.add_pad(Pad::smd("10", -1.0, 2.0, 0.5, 0.5));
        fp.add_pad(Pad::smd("100", 0.0, 2.0, 0.5, 0.5));
        fp.add_pad(Pad::smd("A01", 1.0, 2.0, 0.5, 0.5));
        fp.add_pad(Pad::smd("AA01", 2.0, 2.0, 0.5, 0.5));
        lib.add(fp);
        lib.save(&lib_path).expect("Failed to create test PcbLib");

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "component_name": "BGA_100",
            "scale": 4.0
        });

        let result = server.call_render_footprint(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        let text = get_result_text(&result);
        // Verify full designators are shown (not truncated)
        assert!(text.contains("10"), "Should show full '10' designator");
        assert!(text.contains("100"), "Should show full '100' designator");
        assert!(text.contains("A01"), "Should show full 'A01' designator");
        assert!(text.contains("AA01"), "Should show full 'AA01' designator");
    }

    #[test]
    fn render_symbol_multidigit_designators() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("multidigit.SchLib");

        // Create symbol with multi-digit pin designators
        let mut lib = SchLib::new();
        let mut sym = Symbol::new("IC_100PIN");
        sym.designator = "U?".to_string();
        // Add pins with various designator lengths
        sym.add_pin(Pin::new("1", "PIN1", -40, 30, 10, PinOrientation::Right));
        sym.add_pin(Pin::new("10", "PIN10", -40, 20, 10, PinOrientation::Right));
        sym.add_pin(Pin::new(
            "100",
            "PIN100",
            -40,
            10,
            10,
            PinOrientation::Right,
        ));
        sym.add_pin(Pin::new("VCC", "VCC", -40, 0, 10, PinOrientation::Right));
        sym.add_pin(Pin::new("GND", "GND", -40, -10, 10, PinOrientation::Right));
        sym.add_rectangle(Rectangle::new(-30, -20, 30, 40));
        lib.add(sym);
        lib.save(&lib_path).expect("Failed to create test SchLib");

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "component_name": "IC_100PIN",
            "scale": 1.5
        });

        let result = server.call_render_symbol(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        let text = get_result_text(&result);
        // Verify full designators are shown (not truncated to single char)
        assert!(text.contains("10"), "Should show full '10' designator");
        assert!(text.contains("100"), "Should show full '100' designator");
        assert!(text.contains("VCC"), "Should show full 'VCC' designator");
        assert!(text.contains("GND"), "Should show full 'GND' designator");
    }

    // =========================================================================
    // Error Path Tests
    // =========================================================================

    #[test]
    fn error_path_outside_allowed_directories() {
        let temp = test_temp_dir();
        let other_temp = test_temp_dir();
        let outside_path = other_temp.path().join("outside.PcbLib");

        // Create a file outside the allowed directory
        create_test_pcblib(&outside_path);

        let server = create_test_server(temp.path());
        let args = json!({ "filepath": outside_path.to_string_lossy() });
        let result = server.call_list_components(&args);

        assert!(result.is_error);
        assert!(
            get_result_text(&result).contains("Access denied")
                || get_result_text(&result).contains("outside"),
            "Expected access denied error, got: {}",
            get_result_text(&result)
        );
    }

    #[test]
    fn error_unsupported_file_extension() {
        let temp = test_temp_dir();
        let bad_path = temp.path().join("test.txt");
        std::fs::write(&bad_path, "not a library").expect("Failed to write file");

        let server = create_test_server(temp.path());
        let args = json!({ "filepath": bad_path.to_string_lossy() });

        let result = server.call_list_components(&args);
        assert!(result.is_error);
        // The error message mentions the supported extensions
        let text = get_result_text(&result);
        assert!(
            text.contains("Unsupported") || text.contains("PcbLib") || text.contains("SchLib"),
            "Expected unsupported file type error, got: {text}"
        );
    }

    // =========================================================================
    // Backup Functionality Tests
    // =========================================================================

    #[test]
    fn backup_created_on_destructive_operation() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("backup_test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "component_names": ["CHIP_0402"]
        });

        // Delete a component (destructive operation)
        let result = server.call_delete_component(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        // Check that a backup was created (format: {filename}.{timestamp}.bak)
        let backup_pattern = format!("{}.*.bak", lib_path.file_name().unwrap().to_string_lossy());
        let backups: Vec<_> = std::fs::read_dir(temp.path())
            .expect("Failed to read temp dir")
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("backup_test.PcbLib.")
                    && e.file_name().to_string_lossy().ends_with(".bak")
            })
            .collect();
        assert!(
            !backups.is_empty(),
            "At least one backup should exist, pattern: {backup_pattern}"
        );
    }

    // =========================================================================
    // repair_library Tool Tests
    // =========================================================================

    #[test]
    fn repair_library_dry_run() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("repair_test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "dry_run": true
        });

        let result = server.call_repair_library(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        let text = get_result_text(&result);
        assert!(
            text.contains("dry_run"),
            "Response should indicate dry run mode"
        );
    }

    #[test]
    fn repair_library_no_orphans() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("clean_lib.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "dry_run": false
        });

        let result = server.call_repair_library(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        let text = get_result_text(&result);
        // A clean library should have 0 orphaned references removed
        assert!(
            text.contains("total_removed") || text.contains('0'),
            "Response should show removal count"
        );
    }

    #[test]
    fn repair_library_unsupported_schlib() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("repair_test.SchLib");
        create_test_schlib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy()
        });

        let result = server.call_repair_library(&args);
        assert!(
            result.is_error,
            "SchLib repair should fail (not yet supported)"
        );
        assert!(
            get_result_text(&result).contains("not yet supported")
                || get_result_text(&result).contains("PcbLib"),
            "Error should mention SchLib not supported"
        );
    }

    // =========================================================================
    // bulk_rename Tool Tests
    // =========================================================================

    #[test]
    fn bulk_rename_dry_run_glob() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("rename_test.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "pattern": "CHIP_*",
            "replacement": "RES_",
            "pattern_type": "glob",
            "dry_run": true
        });

        let result = server.call_bulk_rename(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        let text = get_result_text(&result);
        assert!(
            text.contains("dry_run"),
            "Response should indicate dry run mode"
        );
        // Should show preview of renames
        assert!(
            text.contains("CHIP_0402") || text.contains("CHIP_0603"),
            "Should preview matching components"
        );
    }

    #[test]
    fn bulk_rename_regex_with_capture() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("rename_regex.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "pattern": "^CHIP_(.*)$",
            "replacement": "RES_$1",
            "pattern_type": "regex",
            "dry_run": false
        });

        let result = server.call_bulk_rename(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        // Verify components were renamed
        let list_args = json!({ "filepath": lib_path.to_string_lossy() });
        let list_result = server.call_list_components(&list_args);
        let list_text = get_result_text(&list_result);

        assert!(
            list_text.contains("RES_0402") || list_text.contains("RES_0603"),
            "Components should be renamed: {list_text}"
        );
        assert!(
            !list_text.contains("CHIP_0402"),
            "Old names should not exist: {list_text}"
        );
    }

    #[test]
    fn bulk_rename_no_matches() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("no_match.PcbLib");
        create_test_pcblib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "pattern": "NONEXISTENT_*",
            "replacement": "NEW_",
            "pattern_type": "glob",
            "dry_run": false
        });

        let result = server.call_bulk_rename(&args);
        assert!(
            !result.is_error,
            "Expected success even with no matches, got: {}",
            get_result_text(&result)
        );

        let text = get_result_text(&result);
        // Should indicate no renames performed
        assert!(
            text.contains("renamed") || text.contains("[]"),
            "Response should indicate results"
        );
    }

    #[test]
    fn bulk_rename_schlib() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("rename_schlib.SchLib");
        create_test_schlib(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "pattern": "^(.*)$",
            "replacement": "SYM_$1",
            "pattern_type": "regex",
            "dry_run": false
        });

        let result = server.call_bulk_rename(&args);
        assert!(
            !result.is_error,
            "Expected success, got: {}",
            get_result_text(&result)
        );

        // Verify components were renamed
        let list_args = json!({ "filepath": lib_path.to_string_lossy() });
        let list_result = server.call_list_components(&list_args);
        let list_text = get_result_text(&list_result);

        assert!(
            list_text.contains("SYM_RESISTOR") || list_text.contains("SYM_CAPACITOR"),
            "Symbols should be renamed: {list_text}"
        );
    }
}
