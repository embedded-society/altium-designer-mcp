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
    JsonRpcRequest, JsonRpcResponse, RequestId, SERVER_NAME,
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

/// The MCP tool annotations: hints a client shows and reasons with before it
/// runs a tool.
///
/// `read_only_hint` and `destructive_hint` follow the same classification the
/// rate limiter and the audit log use (`is_mutating_tool`), so a client is told
/// exactly which tools change files; `open_world_hint` is `false` for every
/// tool, since each one touches only the library files it is given.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    /// Human-readable display name.
    pub title: String,
    /// Whether the tool changes nothing on disk.
    pub read_only_hint: bool,
    /// Whether the tool may overwrite or delete what is there (every mutating
    /// tool takes a backup first, but the file still changes).
    pub destructive_hint: bool,
    /// Whether the tool reaches beyond the local files it is given: never.
    pub open_world_hint: bool,
}

/// A tool definition for tools/list response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    /// Unique tool name.
    pub name: String,
    /// Human-readable display name (the MCP `title`, also carried in
    /// `annotations`); filled in by `annotate`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Behavioural hints for clients; filled in by `annotate`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: Value,
    /// Representative `tools/call` example, rendered into `docs/TOOLS.md` by the
    /// doc generator. Internal only — `#[serde(skip)]` keeps it off the
    /// `tools/list` wire response (it is not part of the MCP tool schema).
    #[serde(skip)]
    pub example: Option<Value>,
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
    /// Rate limiter for destructive (file-mutating) operations, shared by
    /// every session the HTTP transport serves so that opening another session
    /// is no way around it.
    rate_limiter: std::sync::Arc<RateLimiter>,
    /// Optional append-only audit log for destructive operations.
    audit_logger: Option<AuditLogger>,
}

/// The JSON types a schema `type` names, as `serde_json` sees a value. A
/// number with no fractional part is an `integer` as well as a `number`, as
/// JSON Schema has it.
fn value_has_schema_type(value: &Value, type_name: &str) -> bool {
    match type_name {
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        "integer" => value
            .as_f64()
            .is_some_and(|n| n.fract() == 0.0 && n.abs() <= EXACT_INTEGER_MAX),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        // A type this checker does not know is not a reason to refuse.
        _ => true,
    }
}

/// The largest magnitude a JSON number can carry as an exact integer
/// (2^53): a whole float beyond it has no exact integer to become.
const EXACT_INTEGER_MAX: f64 = 9_007_199_254_740_992.0;

/// Rewrites, in place, every whole float under a schema node typed
/// `integer` (alone or in a union) into a JSON integer, so a handler
/// reading the field with `as_u64` / `as_i64` sees `2` where the caller
/// sent `2.0` — the type check accepts both, as JSON Schema requires, and
/// the handlers must not disagree with it by reading the float as absent.
/// Descends the object `properties` and array `items` the schema describes.
fn canonicalise_integers(value: &mut Value, schema: &Value) {
    let integer_typed = match &schema["type"] {
        Value::String(t) => t == "integer",
        Value::Array(ts) => ts.iter().any(|t| t == "integer"),
        _ => false,
    };
    if integer_typed && value.is_f64() {
        if let Some(whole) = value
            .as_f64()
            .filter(|n| n.fract() == 0.0 && n.abs() <= EXACT_INTEGER_MAX)
        {
            #[allow(clippy::cast_possible_truncation)] // bounded by EXACT_INTEGER_MAX
            let integer = whole as i64;
            *value = Value::from(integer);
        }
    }
    match value {
        Value::Object(fields) => {
            if let Some(properties) = schema["properties"].as_object() {
                for (key, child) in fields.iter_mut() {
                    if let Some(child_schema) = properties.get(key) {
                        canonicalise_integers(child, child_schema);
                    }
                }
            }
        }
        Value::Array(elements) => {
            let items = &schema["items"];
            if items.is_object() {
                for element in elements.iter_mut() {
                    canonicalise_integers(element, items);
                }
            }
        }
        _ => {}
    }
}

/// Describes a value's JSON type for an error message.
const fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Checks `value` against `schema`'s `type` and, for a number, the
/// `minimum` / `maximum` the schema states, and recurses into the object
/// `properties` and array `items` the schema describes. Keys the schema does
/// not mention are left alone (the tools' own allow-lists refuse unknown keys
/// where that matters); `enum` and `required` are the parsers' to judge,
/// since several fields accept spellings the schema lists only by example.
/// `path` names the value in the error (`footprints[0].pads[1].width`).
fn check_value_against_schema(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    let allowed: Vec<&str> = match &schema["type"] {
        Value::String(t) => vec![t.as_str()],
        Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if !allowed.is_empty() && !allowed.iter().any(|t| value_has_schema_type(value, t)) {
        let expected = allowed.join(" or ");
        let shown = match value {
            Value::String(s) if s.chars().count() > 40 => {
                format!("\"{}…\"", s.chars().take(40).collect::<String>())
            }
            other => other.to_string(),
        };
        return Err(format!(
            "Argument '{path}' must be {} {expected}, got {} {shown}",
            article(&expected),
            json_type_name(value)
        ));
    }
    if let Some(number) = value.as_f64() {
        let bound = |key: &str| schema.get(key).and_then(Value::as_f64);
        match (bound("minimum"), bound("maximum")) {
            (Some(min), Some(max)) if number < min || number > max => {
                return Err(format!(
                    "Argument '{path}' must be between {min} and {max}, got {value}"
                ));
            }
            (Some(min), _) if number < min => {
                return Err(format!(
                    "Argument '{path}' must be at least {min}, got {value}"
                ));
            }
            (_, Some(max)) if number > max => {
                return Err(format!(
                    "Argument '{path}' must be at most {max}, got {value}"
                ));
            }
            _ => {}
        }
    }
    if let (Some(properties), Some(object)) = (schema["properties"].as_object(), value.as_object())
    {
        for (key, child) in object {
            if let Some(child_schema) = properties.get(key) {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                check_value_against_schema(child, child_schema, &child_path)?;
            }
        }
    }
    if let (Some(items), Some(array)) = (schema.get("items"), value.as_array()) {
        if items.is_object() {
            for (index, element) in array.iter().enumerate() {
                check_value_against_schema(element, items, &format!("{path}[{index}]"))?;
            }
        }
    }
    Ok(())
}

/// "a" or "an", for the expected type in a message.
fn article(noun: &str) -> &'static str {
    if noun.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

/// A library an in-place tool persists: checked for text its records cannot
/// hold, then written to its path (see [`McpServer::backup_then_save`]).
pub(crate) trait Persist {
    /// The first text field a record of this library cannot hold, as the
    /// message the tool reports, or `Ok(())`.
    ///
    /// # Errors
    ///
    /// The message naming the component and the offending field.
    fn check_record_text(&self) -> Result<(), String>;

    /// Writes the library to `path`.
    ///
    /// # Errors
    ///
    /// Whatever the writer refuses or the file system reports.
    fn persist(&mut self, path: &str) -> crate::altium::AltiumResult<()>;
}

/// A handler that already holds its library by mutable reference passes
/// that reference on.
impl<L: Persist> Persist for &mut L {
    fn check_record_text(&self) -> Result<(), String> {
        (**self).check_record_text()
    }

    fn persist(&mut self, path: &str) -> crate::altium::AltiumResult<()> {
        (**self).persist(path)
    }
}

impl Persist for crate::altium::PcbLib {
    fn check_record_text(&self) -> Result<(), String> {
        self.iter()
            .try_for_each(crate::altium::pcblib::Footprint::check_record_text)
    }

    fn persist(&mut self, path: &str) -> crate::altium::AltiumResult<()> {
        self.save(path)
    }
}

impl Persist for crate::altium::SchLib {
    fn check_record_text(&self) -> Result<(), String> {
        self.iter()
            .try_for_each(crate::altium::schlib::Symbol::check_record_text)
    }

    fn persist(&mut self, path: &str) -> crate::altium::AltiumResult<()> {
        self.save(path)
    }
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
            rate_limiter: std::sync::Arc::new(RateLimiter::unlimited()),
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
    pub fn with_rate_limiter(self, rate_limiter: RateLimiter) -> Self {
        self.with_shared_rate_limiter(std::sync::Arc::new(rate_limiter))
    }

    /// Installs a rate limiter shared with other servers — every HTTP session
    /// draws on the same one.
    #[must_use]
    pub fn with_shared_rate_limiter(mut self, rate_limiter: std::sync::Arc<RateLimiter>) -> Self {
        self.rate_limiter = rate_limiter;
        self
    }

    /// Installs an append-only audit logger for destructive operations.
    #[must_use]
    pub fn with_audit_logger(mut self, audit_logger: Option<AuditLogger>) -> Self {
        self.audit_logger = audit_logger;
        self
    }

    /// Every tool's input schema, from the same definitions `tools/list`
    /// serves and `docs/TOOLS.md` is generated from.
    fn tool_schemas() -> &'static std::collections::HashMap<String, Value> {
        static SCHEMAS: std::sync::OnceLock<std::collections::HashMap<String, Value>> =
            std::sync::OnceLock::new();
        SCHEMAS.get_or_init(|| {
            Self::get_tool_definitions()
                .into_iter()
                .map(|tool| (tool.name, tool.input_schema))
                .collect()
        })
    }

    /// Refuses a call whose arguments the tool's schema does not describe:
    /// an argument name the schema does not document — a typo (`dryrun`,
    /// `compnent_name`) every handler would otherwise ignore, silently taking
    /// the default — or a value of the wrong JSON type anywhere in the
    /// arguments, however deeply nested, which a handler would likewise
    /// ignore (`"filled": "true"` is not `true`; `"width": "1.5"` is not
    /// `1.5`). An unknown tool name is left to dispatch to report.
    fn check_tool_arguments(name: &str, arguments: &Value) -> Result<(), String> {
        let (Some(schema), Some(given)) = (Self::tool_schemas().get(name), arguments.as_object())
        else {
            return Ok(());
        };
        let known: Vec<&String> = schema["properties"]
            .as_object()
            .map(|props| props.keys().collect())
            .unwrap_or_default();
        if let Some(unknown) = given.keys().find(|key| !known.contains(key)) {
            let known: Vec<&str> = known.iter().map(|s| s.as_str()).collect();
            return Err(format!(
                "Unknown argument '{unknown}' for tool '{name}'. Accepted arguments are: {known:?}"
            ));
        }
        check_value_against_schema(arguments, schema, "")
            .map_err(|e| format!("{e} (tool '{name}')"))
    }

    /// Hands the handler the arguments in the form it reads: every whole
    /// float the schema types as an integer becomes one (see
    /// [`canonicalise_integers`]). Runs only after [`Self::check_tool_arguments`]
    /// has passed them.
    fn canonicalise_tool_arguments(name: &str, arguments: &mut Value) {
        if let Some(schema) = Self::tool_schemas().get(name) {
            canonicalise_integers(arguments, schema);
        }
    }

    /// Returns `true` if the named tool mutates a library file on disk.
    ///
    /// Only these destructive operations are rate limited; read-only tools
    /// (reads, listings, diffs, renders, validation) are never throttled.
    pub(crate) fn is_mutating_tool(name: &str) -> bool {
        matches!(
            name,
            "write_pcblib"
                | "write_schlib"
                | "write_libpkg"
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

        if filepath.is_empty() {
            return Err("Invalid path: no file path was given".to_string());
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

    /// Refuses text the library's records cannot hold, backs up `filepath`,
    /// then persists the library.
    ///
    /// Centralises the check → backup → save sequence shared by every
    /// mutating tool handler so create and update paths cannot drift. The
    /// check comes first: the writer would refuse the same text, but only
    /// after a backup had been made for a save that never happens. On
    /// failure it returns the tool-error response; the caller builds its own
    /// success result.
    pub(crate) fn backup_then_save(
        filepath: &str,
        library: &mut impl Persist,
    ) -> Result<(), ToolCallResult> {
        library.check_record_text().map_err(ToolCallResult::error)?;
        if let Err(e) = Self::create_backup(filepath) {
            return Err(ToolCallResult::error(e));
        }
        library
            .persist(filepath)
            .map_err(|e| ToolCallResult::error(format!("Failed to write library: {e}")))?;
        Ok(())
    }

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
        match self.dispatch_request(&req) {
            Ok(resp) => self.transport.write_response(&resp).await,
            Err(error) => self.transport.write_error(&error).await,
        }
    }

    /// Computes the response to a request, whatever transport carries it.
    fn dispatch_request(&mut self, req: &JsonRpcRequest) -> Result<JsonRpcResponse, JsonRpcError> {
        match req.method.as_str() {
            "initialize" => self.handle_initialize(req),
            "tools/list" => self.handle_tools_list(req),
            "tools/call" => self.handle_tools_call(req),
            "ping" => Ok(Self::handle_ping(req)),
            _ => Err(JsonRpcError::method_not_found(req.id.clone(), &req.method)),
        }
    }

    /// Handles one JSON-RPC message given as text and returns the reply as
    /// text: the response or error for a request (or for a message that does
    /// not parse), `None` for a notification. This is the Streamable HTTP
    /// transport's entry point; stdio goes through [`McpServer::run`].
    #[must_use]
    pub fn handle_message_text(&mut self, text: &str) -> Option<String> {
        use crate::mcp::protocol::parse_message;

        let reply = match parse_message(text) {
            Ok(IncomingMessage::Request(req)) => match self.dispatch_request(&req) {
                Ok(resp) => serde_json::to_string(&resp),
                Err(error) => serde_json::to_string(&error),
            },
            Ok(IncomingMessage::Notification(notif)) => {
                self.handle_notification(&notif);
                return None;
            }
            Err(error) => serde_json::to_string(&error),
        };
        Some(reply.unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to serialise a JSON-RPC reply");
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal error"}}"#
                .to_string()
        }))
    }

    /// The protocol version negotiated at `initialize`, if it has happened.
    #[must_use]
    pub fn negotiated_protocol_version(&self) -> Option<&str> {
        self.protocol_version.as_deref()
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

        let params: InitializeParams = req
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

        let negotiated_version =
            crate::mcp::protocol::negotiate_protocol_version(&params.protocol_version).to_string();

        self.protocol_version = Some(negotiated_version.clone());
        self.state = ServerState::Initialising;

        let result = json!({
            "protocolVersion": negotiated_version,
            "capabilities": ServerCapabilities::default(),
            "serverInfo": ServerInfo::default(),
            // MCP `instructions`: surfaced to the model by the client so an agent
            // learns the conventions (units, pin geometry, sandbox, build flow)
            // without reading the source. Embedded from docs/AGENT_GUIDE.md so
            // there is a single source of truth; per-tool detail stays in the
            // tools/list schema.
            "instructions": include_str!("../../docs/AGENT_GUIDE.md"),
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

    /// Runs one tool call with a panic safety net.
    ///
    /// A panic anywhere below a tool — a parser assertion, an index past the
    /// end, a third-party crate's debug check — must not take the server down
    /// mid-conversation: the client loses every later call until someone
    /// restarts the process. Caught here, it becomes an `isError` result like
    /// any other failure, and the session continues. The panic payload is
    /// logged server-side only; the client message carries no detail, because
    /// a payload may quote a path or file content.
    fn guard_panics(tool: &str, call: impl FnOnce() -> ToolCallResult) -> ToolCallResult {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)) {
            Ok(result) => result,
            Err(payload) => {
                let detail = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string panic payload");
                tracing::error!(tool = %tool, panic = %detail, "tool call panicked; server kept running");
                ToolCallResult::error(format!(
                    "Internal error while running '{tool}'. The server is still running; please report this with the arguments used."
                ))
            }
        }
    }

    /// Handles the tools/call request.
    fn handle_tools_call(&self, req: &JsonRpcRequest) -> Result<JsonRpcResponse, JsonRpcError> {
        self.require_running(&req.id)?;

        let mut params: ToolCallParams = req
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
        let checked = Self::check_tool_arguments(&params.name, &params.arguments)
            .map(|()| Self::canonicalise_tool_arguments(&params.name, &mut params.arguments));
        let result = if let Err(e) = checked {
            ToolCallResult::error(e)
        } else if Self::is_mutating_tool(params.name.as_str()) && !self.rate_limiter.try_acquire() {
            tracing::warn!(
                tool = %params.name,
                "Rate limit exceeded; rejecting mutating operation"
            );
            ToolCallResult::error(
                "Rate limit exceeded: too many write operations in a short period. \
                 Please slow down and retry.",
            )
        } else {
            Self::guard_panics(&params.name, || match params.name.as_str() {
                // Library I/O tools
                "read_pcblib" => self.call_read_pcblib(&params.arguments),
                "write_pcblib" => self.call_write_pcblib(&params.arguments),
                "read_schlib" => self.call_read_schlib(&params.arguments),
                "write_schlib" => self.call_write_schlib(&params.arguments),
                "write_libpkg" => self.call_write_libpkg(&params.arguments),
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
            })
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
    use crate::mcp::tools::test_support::{
        create_test_pcblib, create_test_schlib, create_test_server, get_result_text, test_temp_dir,
    };

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
            "write_libpkg",
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
        assert!(McpServer::validate_schlib_coordinate(32000.0, "x").is_ok());
        assert!(McpServer::validate_schlib_coordinate(-32000.0, "x").is_ok());
        assert!(McpServer::validate_schlib_coordinate(32001.0, "x").is_err());
        assert!(McpServer::validate_schlib_coordinate(-32001.0, "x").is_err());
        // Fractional in-range coordinate is accepted.
        assert!(McpServer::validate_schlib_coordinate(-28.995, "x").is_ok());
        // Non-finite coordinates must be rejected.
        assert!(McpServer::validate_schlib_coordinate(f64::NAN, "x").is_err());
        assert!(McpServer::validate_schlib_coordinate(f64::INFINITY, "x").is_err());
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
                prop_assert!(McpServer::validate_schlib_coordinate(f64::from(v), "x").is_ok());
            }

            #[test]
            fn schlib_coordinate_out_of_range_always_rejects(v in 32001i32..i32::MAX) {
                prop_assert!(McpServer::validate_schlib_coordinate(f64::from(v), "x").is_err());
                prop_assert!(McpServer::validate_schlib_coordinate(f64::from(-v), "x").is_err());
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
    // extract_step_model Tool Tests
    // =========================================================================

    /// Creates a test `PcbLib` with one footprint referencing one embedded
    /// STEP model.
    fn create_test_pcblib_with_model(path: &std::path::Path) {
        use crate::altium::pcblib::{ComponentBody, EmbeddedModel};

        const MODEL_ID: &str = "{11111111-2222-3333-4444-555555555555}";

        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("FP_3D");
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        fp.add_component_body(ComponentBody::new(MODEL_ID, "part.step"));
        lib.add(fp);
        lib.add_model(EmbeddedModel::new(
            MODEL_ID,
            "part.step",
            b"ISO-10303-21; test".to_vec(),
        ));
        lib.save(path).expect("save pcblib with model");
    }

    #[test]
    fn extract_by_footprint_output_path_is_a_directory_even_for_one_match() {
        // `output_path` is ALWAYS a directory here, whatever the match count.
        // An argument whose meaning switches between file and directory based
        // on how many models happen to match is data the caller does not
        // control.
        let temp = test_temp_dir();
        let lib_path = temp.path().join("with_model.PcbLib");
        create_test_pcblib_with_model(&lib_path);

        let out_dir = temp.path().join("models_out");
        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "mode": "extract_by_footprint",
            "footprint_name": "FP_3D",
            "output_path": out_dir.to_string_lossy(),
        });

        let result = server.call_extract_step_model(&args);
        assert!(!result.is_error, "expected success: {:?}", result.content);

        let extracted = out_dir.join("part.step");
        assert!(
            extracted.exists(),
            "single-match extract_by_footprint must treat output_path as a directory"
        );
        assert_eq!(
            std::fs::read(&extracted).expect("read extracted model"),
            b"ISO-10303-21; test"
        );
    }

    #[test]
    fn extract_by_footprint_without_output_returns_base64_for_single_match() {
        let temp = test_temp_dir();
        let lib_path = temp.path().join("with_model.PcbLib");
        create_test_pcblib_with_model(&lib_path);

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "mode": "extract_by_footprint",
            "footprint_name": "FP_3D",
        });

        let result = server.call_extract_step_model(&args);
        assert!(!result.is_error, "expected success: {:?}", result.content);
        let parsed: Value = serde_json::from_str(get_result_text(&result)).expect("valid JSON");
        assert_eq!(parsed["encoding"], "base64");
    }

    // =========================================================================
    // SchLib primitive-family completeness Tests
    // =========================================================================

    /// Builds a symbol carrying every primitive family a read-modify-write is
    /// at risk of dropping (pies, images, beziers, elliptical arcs, footprint
    /// links) plus a non-default part count.
    fn symbol_with_every_at_risk_family() -> Symbol {
        use crate::altium::schlib::{Bezier, EllipticalArc, FootprintModel, Image, Pie};

        let mut symbol = Symbol::new("FAMILIES");
        symbol.part_count = 2;
        symbol.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
        symbol.pies.push(Pie::new(0, 0, 5, 30.0, 210.0));
        symbol.images.push(Image::new(-5, -3, 5, 3, "logo.bmp"));
        symbol.beziers.push(Bezier::new(0, 0, 1, 2, 3, 2, 4, 0));
        symbol
            .elliptical_arcs
            .push(EllipticalArc::new(0, 0, 10, 5, 0, 90));
        symbol.footprints.push(FootprintModel::new("RESC1608X55N"));
        symbol
    }

    #[test]
    fn update_schlib_preserves_every_primitive_family() {
        // Regression: update_schlib_component rebuilt the symbol from JSON but
        // never parsed pies, images, beziers, elliptical_arcs, footprints or
        // part_count, so feeding get_component's own output back (the
        // documented read-modify-write flow) silently deleted them all.
        let temp = test_temp_dir();
        let lib_path = temp.path().join("families.SchLib");

        let symbol = symbol_with_every_at_risk_family();
        let mut lib = SchLib::new();
        lib.add(symbol.clone());
        lib.save(&lib_path).expect("save schlib");

        // Exactly what get_component returns: the serde-serialised symbol.
        let mut sym_json = serde_json::to_value(&symbol).expect("serialise symbol");
        sym_json["description"] = json!("after");

        let server = McpServer::new(vec![temp.path().to_path_buf()]);
        let result = server.update_schlib_component(
            &lib_path.to_string_lossy(),
            "FAMILIES",
            &sym_json,
            false,
        );
        assert!(!result.is_error, "expected success: {:?}", result.content);

        let reread = SchLib::open(&lib_path).expect("reopen");
        let updated = reread.get("FAMILIES").expect("symbol present");
        assert_eq!(updated.description, "after");
        assert_eq!(updated.part_count, 2, "multi-part count preserved");
        assert_eq!(updated.pins.len(), 1, "pin preserved");
        assert_eq!(updated.pies.len(), 1, "pie preserved");
        assert_eq!(updated.images.len(), 1, "image preserved");
        assert_eq!(updated.beziers.len(), 1, "bezier preserved");
        assert_eq!(updated.elliptical_arcs.len(), 1, "elliptical arc preserved");
        assert_eq!(updated.footprints.len(), 1, "footprint link preserved");
    }

    #[test]
    fn export_schlib_json_includes_every_family() {
        // Regression: the JSON export omitted pies, images, beziers,
        // elliptical_arcs, text and part_count, so the documented
        // export -> import round-trip silently dropped them.
        let temp = test_temp_dir();
        let lib_path = temp.path().join("families.SchLib");

        let mut lib = SchLib::new();
        lib.add(symbol_with_every_at_risk_family());
        lib.save(&lib_path).expect("save schlib");

        let server = create_test_server(temp.path());
        let args = json!({ "filepath": lib_path.to_string_lossy(), "format": "json" });
        let result = server.call_export_library(&args);
        assert!(!result.is_error, "expected success: {:?}", result.content);

        let parsed: Value = serde_json::from_str(get_result_text(&result)).expect("valid JSON");
        let symbol = &parsed["symbols"][0];
        // The export is the struct's own serde shape: every populated family
        // is present (an empty one is omitted, and import defaults it).
        for key in [
            "part_count",
            "pies",
            "images",
            "beziers",
            "elliptical_arcs",
            "footprints",
        ] {
            assert!(
                symbol.get(key).is_some(),
                "export omits '{key}' — import would silently drop it"
            );
        }
        assert_eq!(symbol["part_count"], 2);
        assert_eq!(symbol["pies"].as_array().map(Vec::len), Some(1));
        assert_eq!(symbol["images"].as_array().map(Vec::len), Some(1));
        assert!(symbol.get("text").is_none(), "an empty family is omitted");
    }

    #[test]
    fn symbol_validation_rejects_out_of_range_pie_and_image() {
        use crate::altium::schlib::{Image, Pie};

        // Pies and images must go through the same ±32000-unit coordinate
        // guard as every other family before a write.
        let mut with_pie = Symbol::new("BAD_PIE");
        with_pie.pies.push(Pie::new(100_000, 0, 5, 0.0, 90.0));
        assert!(
            McpServer::validate_symbol_coordinates(&with_pie).is_err(),
            "out-of-range pie must be rejected"
        );

        let mut with_image = Symbol::new("BAD_IMAGE");
        with_image
            .images
            .push(Image::new(0, 0, 100_000, 3, "x.bmp"));
        assert!(
            McpServer::validate_symbol_coordinates(&with_image).is_err(),
            "out-of-range image must be rejected"
        );
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
    fn write_schlib_accepts_arcs() {
        // Regression: the strict-deserialization allow-list for SchLib arcs was
        // copied from the (layer-based) PcbLib arc as `&["layer"]`, so every real
        // arc — which carries x/y/radius/angles — was rejected as an unknown field
        // and silently produced an arc-less symbol. The allow-list must accept the
        // documented SchLib arc fields.
        let temp = test_temp_dir();
        let lib_path = temp.path().join("arc_lib.SchLib");

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "symbols": [{
                "name": "ARC_SYM",
                "designator": "L?",
                "pins": [],
                "arcs": [
                    {"x": 0, "y": 0, "radius": 10, "start_angle": 0, "end_angle": 180,
                     "line_width": 1, "color": 128, "owner_part_id": 1}
                ]
            }]
        });

        let result = server.call_write_schlib(&args);
        assert!(
            !result.is_error,
            "arc input must be accepted, got: {}",
            get_result_text(&result)
        );

        // The arc must actually persist (not be silently dropped).
        let lib = SchLib::open(&lib_path).expect("Failed to read created library");
        let sym = lib.get("ARC_SYM").expect("symbol present");
        assert_eq!(sym.arcs.len(), 1, "arc written to the symbol");
        let arc = &sym.arcs[0];
        assert!((arc.radius - 10.0).abs() < 1e-9);
        assert!((arc.end_angle - 180.0).abs() < 1e-9);
    }

    #[test]
    fn initialize_sends_agent_instructions() {
        // The MCP `instructions` field is how a client teaches the model the
        // conventions; it must be sent and must be the embedded AGENT_GUIDE so
        // the doc stays the single source.
        let dir = test_temp_dir();
        let mut server = McpServer::new(vec![dir.path().to_path_buf()]);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(1),
            method: "initialize".to_string(),
            params: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0.0" }
            })),
        };
        let resp = server.handle_initialize(&req).expect("initialize ok");
        let instructions = resp
            .result
            .get("instructions")
            .and_then(|v| v.as_str())
            .expect("initialize result includes `instructions`");
        assert!(
            !instructions.trim().is_empty(),
            "instructions must be non-empty"
        );
        assert_eq!(
            instructions,
            include_str!("../../docs/AGENT_GUIDE.md"),
            "instructions must be the embedded AGENT_GUIDE.md"
        );
    }

    #[test]
    fn write_schlib_accepts_all_primitive_fields() {
        // Guards the strict-deserialization allow-lists against narrowing. The
        // unit suite otherwise never exercises these optional fields, so an
        // allow-list that rejects a shipped field (the #188/#189 regression the
        // integration suite caught) would still pass CI. Each primitive here
        // carries the full field set its `parse_*` reads.
        let temp = test_temp_dir();
        let lib_path = temp.path().join("all_prims.SchLib");
        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "symbols": [{
                "name": "ALL", "designator_prefix": "U",
                "pins": [{"designator": "1", "name": "P1", "x": 0, "y": 0,
                    "length": 10, "orientation": "right"}],
                "rectangles": [{"x1": -10, "y1": -10, "x2": 10, "y2": 10,
                    "line_width": 1, "line_color": 128, "fill_color": 11_599_871,
                    "filled": true, "owner_part_id": 1}],
                "round_rects": [{"x1": -5, "y1": -5, "x2": 5, "y2": 5,
                    "corner_x_radius": 1, "corner_y_radius": 1, "line_width": 1,
                    "line_color": 128, "fill_color": 11_599_871, "filled": true,
                    "owner_part_id": 1}],
                "lines": [{"x1": 0, "y1": 0, "x2": 10, "y2": 0, "line_width": 1,
                    "color": 128, "owner_part_id": 1}],
                "polylines": [{"points": [{"x": 0, "y": 0}, {"x": 5, "y": 5}],
                    "line_width": 1, "color": 128, "owner_part_id": 1}],
                "polygons": [{"points": [{"x": 0, "y": 0}, {"x": 5, "y": 0},
                    {"x": 5, "y": 5}], "line_width": 1, "line_color": 128,
                    "fill_color": 11_599_871, "filled": true, "owner_part_id": 1}],
                "arcs": [{"x": 0, "y": 0, "radius": 5, "start_angle": 0,
                    "end_angle": 360, "line_width": 1, "color": 128, "owner_part_id": 1}],
                "pies": [{"x": 0, "y": 0, "radius": 5, "start_angle": 45,
                    "end_angle": 135, "line_width": 1, "line_color": 128,
                    "fill_color": 11_599_871, "filled": true, "transparent": false,
                    "is_not_accessible": true, "owner_part_id": 1,
                    "graphically_locked": false, "disabled": false, "dimmed": false,
                    "owner_part_display_mode": 0}],
                "ellipses": [{"x": 0, "y": 0, "radius_x": 5, "radius_y": 3,
                    "line_width": 1, "line_color": 128, "fill_color": 11_599_871,
                    "filled": true, "owner_part_id": 1}],
                "text_frames": [{"x1": -10, "y1": -5, "x2": 10, "y2": 5,
                    "text": "Frame", "color": 128, "area_color": 11_599_871,
                    "text_color": 8_388_608, "text_margin": 0.2, "line_width": 1,
                    "line_style": 0, "transparent": false, "font_id": 1,
                    "orientation": 0, "alignment": 1, "is_solid": true,
                    "show_border": true, "word_wrap": true, "clip_to_rect": true,
                    "is_not_accessible": true, "owner_part_id": 1,
                    "graphically_locked": false, "disabled": false, "dimmed": false,
                    "owner_part_display_mode": 0}],
                "images": [{"x1": -5, "y1": -3, "x2": 5, "y2": 3, "line_width": 1,
                    "line_color": 128, "line_style": 0, "fill_color": 11_599_871,
                    "filled": false, "transparent": false, "show_border": false,
                    "keep_aspect": true, "embed_image": true,
                    "file_name": "C:\\img\\embed.bmp", "image_data": "Qk0AAQI=",
                    "is_not_accessible": true, "owner_part_id": 1,
                    "graphically_locked": false, "disabled": false, "dimmed": false,
                    "owner_part_display_mode": 0}],
                "beziers": [{"x1": -10, "y1": 0, "x2": -5, "y2": 8, "x3": 5, "y3": 8,
                    "x4": 10, "y4": 0, "line_width": 1, "color": 128,
                    "is_not_accessible": true, "owner_part_id": 1}],
                "elliptical_arcs": [{"x": 0, "y": 0, "radius": 10, "secondary_radius": 5,
                    "start_angle": 0, "end_angle": 90, "line_width": 1, "color": 128,
                    "fill_color": 0, "owner_part_id": 1}],
                "labels": [{"x": 0, "y": 0, "text": "L", "font_id": 1, "color": 128,
                    "rotation": 0, "is_mirrored": false, "is_hidden": false,
                    "justification": "bottom_left", "owner_part_id": 1}],
                "ieee_symbols": [{"x": 0, "y": 0, "symbol": 4, "scale_factor": 20, "rotation": 90,
                    "is_mirrored": true, "line_width": 1, "color": 128, "owner_part_id": 1,
                    "graphically_locked": true}],
                "parameters": [{"name": "Value", "value": "*", "x": 0, "y": 0,
                    "hidden": false, "font_id": 1, "color": 8_388_608, "owner_part_id": 1}],
                "footprints": [{"name": "FP", "description": "d",
                    "library_path": lib_path.to_string_lossy()}]
            }]
        });
        let result = server.call_write_schlib(&args);
        assert!(
            !result.is_error,
            "all schlib primitive fields must be accepted, got: {}",
            get_result_text(&result)
        );
    }

    #[test]
    fn write_schlib_authors_beziers_and_elliptical_arcs() {
        // Both families must be authorable, not merely read and preserved.
        // Author one of each and read them back through the real writer and
        // reader.
        let temp = test_temp_dir();
        let lib_path = temp.path().join("bez_earc.SchLib");
        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "symbols": [{
                "name": "CURVES",
                "beziers": [{"x1": -10, "y1": 0, "x2": -5, "y2": 8,
                    "x3": 5, "y3": 8, "x4": 10, "y4": 0}],
                "elliptical_arcs": [{"x": 0, "y": 0, "radius": 10,
                    "secondary_radius": 5, "start_angle": 0, "end_angle": 90}]
            }]
        });
        let result = server.call_write_schlib(&args);
        assert!(
            !result.is_error,
            "write failed: {}",
            get_result_text(&result)
        );

        let lib = SchLib::open(&lib_path).expect("reopen");
        let sym = lib.get("CURVES").expect("symbol present");
        assert_eq!(sym.beziers.len(), 1, "bezier authored");
        let bez = &sym.beziers[0];
        assert!(
            (bez.x1 + 10.0).abs() < 1e-9
                && (bez.y2 - 8.0).abs() < 1e-9
                && (bez.x4 - 10.0).abs() < 1e-9,
            "bezier control points round-trip, got ({}, {}) .. ({}, {})",
            bez.x1,
            bez.y1,
            bez.x4,
            bez.y4
        );
        assert_eq!(sym.elliptical_arcs.len(), 1, "elliptical arc authored");
        let ell = &sym.elliptical_arcs[0];
        assert!(
            (ell.radius - 10.0).abs() < 1e-9
                && (ell.secondary_radius - 5.0).abs() < 1e-9
                && (ell.end_angle - 90.0).abs() < 1e-9,
            "elliptical arc fields round-trip, got r={} r2={} end={}",
            ell.radius,
            ell.secondary_radius,
            ell.end_angle
        );
    }

    #[test]
    fn export_schlib_write_schlib_round_trip_preserves_symbol_header_fields() {
        // The five symbol header fields beyond part_count are emitted by
        // export_schlib and must survive a replay through write_schlib, or an
        // export -> write round-trip collapses e.g. a two-display-mode symbol
        // back to one.
        let temp = test_temp_dir();
        let src_path = temp.path().join("hdr_src.SchLib");
        let dst_path = temp.path().join("hdr_dst.SchLib");
        let server = create_test_server(temp.path());

        let mut symbol = Symbol::new("HDR");
        symbol.designator = "U?".to_string();
        symbol.display_mode_count = 2;
        symbol.current_part_id = 2;
        symbol.part_id_locked = true;
        symbol.source_library_name = "SrcLib".to_string();
        symbol.target_file_name = "Tgt.SchLib".to_string();
        let mut lib = SchLib::new();
        lib.add(symbol);
        lib.save(&src_path).expect("save source library");

        let export = McpServer::export_schlib(&src_path.to_string_lossy(), "json");
        assert!(!export.is_error, "export failed");
        let export_json: Value =
            serde_json::from_str(get_result_text(&export)).expect("export output is JSON");
        assert_eq!(
            export_json["symbols"][0]["display_mode_count"], 2,
            "export must emit display_mode_count"
        );

        let write_args = json!({
            "filepath": dst_path.to_string_lossy(),
            "symbols": export_json["symbols"],
        });
        let result = server.call_write_schlib(&write_args);
        assert!(
            !result.is_error,
            "exported symbols must replay into write_schlib, got: {}",
            get_result_text(&result)
        );

        let reread = SchLib::open(&dst_path).expect("reopen destination");
        let sym = reread.get("HDR").expect("symbol present");
        assert_eq!(sym.display_mode_count, 2, "display_mode_count preserved");
        assert_eq!(sym.current_part_id, 2, "current_part_id preserved");
        assert!(sym.part_id_locked, "part_id_locked preserved");
        assert_eq!(sym.source_library_name, "SrcLib");
        assert_eq!(sym.target_file_name, "Tgt.SchLib");
    }

    #[test]
    fn write_pcblib_accepts_all_primitive_fields() {
        // Companion to the SchLib guard: text (layer/stroke_width), region
        // (layer) and component_bodies must survive the footprint allow-list.
        let temp = test_temp_dir();
        let lib_path = temp.path().join("all_prims.PcbLib");
        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "footprints": [{
                "name": "FP",
                "pads": [{"designator": "1", "x": 0, "y": 0, "width": 1, "height": 1}],
                "regions": [{"layer": "Top Courtyard",
                    "vertices": [{"x": -1, "y": -1}, {"x": 1, "y": -1}, {"x": 1, "y": 1}]}],
                "text": [{"x": 0, "y": 2, "text": ".Designator", "height": 0.5,
                    "layer": "Top Overlay", "rotation": 0, "stroke_width": 0.1}],
                "component_bodies": [{"layer": "Top 3D Body", "overall_height": 1.0,
                    "outline": [{"x": -1, "y": -1}, {"x": 1, "y": -1}, {"x": 1, "y": 1},
                        {"x": -1, "y": 1}]}]
            }]
        });
        let result = server.call_write_pcblib(&args);
        assert!(
            !result.is_error,
            "all pcblib primitive fields must be accepted, got: {}",
            get_result_text(&result)
        );
    }

    #[test]
    fn read_pcblib_output_replays_into_write_pcblib() {
        // read_pcblib emits "model_3d" for every footprint (null when there is
        // no model, populated from the first ComponentBody otherwise); the
        // write allow-list must accept that spelling or a read result cannot
        // be replayed. FP_BODY exercises the non-null shape, FP_PLAIN the null.
        let temp = test_temp_dir();
        let src_path = temp.path().join("replay_src.PcbLib");
        let dst_path = temp.path().join("replay_dst.PcbLib");
        let server = create_test_server(temp.path());

        let write_args = json!({
            "filepath": src_path.to_string_lossy(),
            "footprints": [
                {
                    "name": "FP_BODY",
                    "pads": [{"designator": "1", "x": 0, "y": 0, "width": 1, "height": 1}],
                    "component_bodies": [{"layer": "Top 3D Body", "overall_height": 1.2,
                        "outline": [{"x": -1, "y": -1}, {"x": 1, "y": -1},
                            {"x": 1, "y": 1}, {"x": -1, "y": 1}]}]
                },
                {
                    "name": "FP_PLAIN",
                    "pads": [{"designator": "1", "x": 0, "y": 0, "width": 1, "height": 1}]
                }
            ]
        });
        let write_result = server.call_write_pcblib(&write_args);
        assert!(
            !write_result.is_error,
            "source write failed: {}",
            get_result_text(&write_result)
        );

        let read_result = server.call_read_pcblib(&json!({"filepath": src_path.to_string_lossy()}));
        assert!(!read_result.is_error, "read failed");
        let read_json: Value =
            serde_json::from_str(get_result_text(&read_result)).expect("read output is JSON");
        let footprints = read_json["footprints"].clone();
        assert!(
            footprints
                .as_array()
                .unwrap()
                .iter()
                .any(|fp| fp.get("model_3d").is_some_and(|m| !m.is_null())),
            "precondition: read output carries a non-null model_3d"
        );

        let replay_args = json!({
            "filepath": dst_path.to_string_lossy(),
            "footprints": footprints,
        });
        let replay_result = server.call_write_pcblib(&replay_args);
        assert!(
            !replay_result.is_error,
            "read_pcblib output must replay into write_pcblib without error, got: {}",
            get_result_text(&replay_result)
        );
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

    #[test]
    fn write_schlib_geometry_echo_covers_only_written_symbols() {
        // The geometry echo exists so the caller can verify the pins it just wrote.
        // Scoped to this call's symbols: walking the whole library makes an
        // `append: true` sequence re-echo every pre-existing symbol, growing
        // the response quadratically with the number of appends.
        let temp = test_temp_dir();
        let lib_path = temp.path().join("geom_scope.SchLib");
        create_test_schlib(&lib_path); // seeds RESISTOR + CAPACITOR

        let server = create_test_server(temp.path());
        let args = json!({
            "filepath": lib_path.to_string_lossy(),
            "symbols": [{
                "name": "APPENDED",
                "designator": "X?",
                "pins": [{"designator": "1", "name": "A", "x": -30, "y": 0,
                          "length": 10, "orientation": "left"}]
            }],
            "append": true
        });

        let result = server.call_write_schlib(&args);
        assert!(
            !result.is_error,
            "append must succeed, got: {}",
            get_result_text(&result)
        );

        let parsed: serde_json::Value =
            serde_json::from_str(get_result_text(&result)).expect("response is JSON");
        let geometry = parsed["geometry"].as_array().expect("geometry array");
        let names: Vec<&str> = geometry.iter().filter_map(|g| g["name"].as_str()).collect();

        assert_eq!(
            names,
            vec!["APPENDED"],
            "geometry must cover only the symbols written by this call"
        );
        // The library itself still grew — only the echo is scoped.
        assert_eq!(parsed["symbol_count"].as_u64(), Some(3));
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

    // ==================== dispatch, lifecycle and transport-loop body ====================

    mod dispatch_and_lifecycle {
        use super::*;

        /// A tool that panics answers its call with an error result and the
        /// server keeps running; the panic's own text stays in the log, never
        /// in the client-facing message. A tool that returns normally is
        /// passed through untouched.
        #[test]
        fn guard_panics_turns_a_panic_into_an_error_result() {
            // Silence the default hook's stderr noise for this one test.
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let result = McpServer::guard_panics("boom_tool", || {
                panic!("secret detail: C:/Users/someone/Library.PcbLib");
            });
            let payload_result = McpServer::guard_panics("boom_tool", || {
                std::panic::panic_any(42_u8);
            });
            std::panic::set_hook(previous);

            assert!(result.is_error);
            let text = match &result.content[0] {
                ToolContent::Text { text } => text.clone(),
            };
            assert!(
                text.contains("Internal error while running 'boom_tool'"),
                "{text}"
            );
            assert!(text.contains("still running"), "{text}");
            assert!(
                !text.contains("secret detail"),
                "panic text must not reach the client: {text}"
            );
            assert!(
                payload_result.is_error,
                "non-string payloads are guarded too"
            );

            let ok = McpServer::guard_panics("fine_tool", || ToolCallResult::text("fine"));
            assert!(!ok.is_error);
        }

        fn req(method: &str, params: Option<Value>) -> JsonRpcRequest {
            JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: RequestId::Number(1),
                method: method.to_string(),
                params,
            }
        }

        fn running_server(dir: &std::path::Path) -> McpServer {
            let mut s = McpServer::new(vec![dir.to_path_buf()])
                .with_rate_limiter(RateLimiter::new(1000, 0.0));
            s.state = ServerState::Running;
            s
        }

        /// Every tool name the dispatch match routes.
        const ALL_TOOLS: &[&str] = &[
            "read_pcblib",
            "write_pcblib",
            "read_schlib",
            "write_schlib",
            "write_libpkg",
            "list_components",
            "extract_style",
            "delete_component",
            "validate_library",
            "export_library",
            "import_library",
            "extract_step_model",
            "diff_libraries",
            "batch_update",
            "copy_component",
            "rename_component",
            "copy_component_cross_library",
            "merge_libraries",
            "reorder_components",
            "update_component",
            "search_components",
            "get_component",
            "component_exists",
            "render_footprint",
            "render_symbol",
            "manage_schlib_parameters",
            "manage_schlib_footprints",
            "compare_components",
            "repair_library",
            "list_backups",
            "restore_backup",
            "bulk_rename",
            "update_pad",
            "update_primitive",
        ];

        #[test]
        fn tools_call_dispatches_every_tool_name() {
            let dir = test_temp_dir();
            let server = running_server(dir.path());
            for name in ALL_TOOLS {
                let r = req("tools/call", Some(json!({ "name": name, "arguments": {} })));
                let resp = server.handle_tools_call(&r).expect("dispatch returns Ok");
                assert!(
                    resp.result.get("content").is_some(),
                    "{name} produced no content"
                );
            }
            // The unknown-tool arm.
            let r = req(
                "tools/call",
                Some(json!({ "name": "no_such_tool", "arguments": {} })),
            );
            let resp = server.handle_tools_call(&r).unwrap();
            assert!(resp.result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Unknown tool"));
        }

        /// Every tool refuses an argument its schema does not document, and
        /// every tool's own documented example passes the same check — so
        /// the schemas, the examples and the handlers cannot drift apart.
        #[test]
        fn tools_call_refuses_an_undocumented_argument_on_every_tool() {
            let dir = test_temp_dir();
            let server = running_server(dir.path());
            for tool in McpServer::get_tool_definitions() {
                let r = req(
                    "tools/call",
                    Some(
                        json!({ "name": tool.name, "arguments": { "filepath": "x", "dryrun": true } }),
                    ),
                );
                let resp = server.handle_tools_call(&r).unwrap();
                let text = resp.result["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .to_string();
                assert_eq!(resp.result["isError"], json!(true), "{}: {text}", tool.name);
                assert!(
                    text.contains("Unknown argument 'dryrun'") && text.contains(&tool.name),
                    "{}: {text}",
                    tool.name
                );

                let example = tool.example.expect("every tool documents an example");
                assert_eq!(example["name"], tool.name);
                assert!(
                    McpServer::check_tool_arguments(&tool.name, &example["arguments"]).is_ok(),
                    "{}: its own example carries an undocumented argument",
                    tool.name
                );
            }
            // Non-object arguments and unknown tools are left to dispatch.
            assert!(McpServer::check_tool_arguments("read_pcblib", &json!(null)).is_ok());
            assert!(McpServer::check_tool_arguments("no_such_tool", &json!({ "x": 1 })).is_ok());
        }

        /// A value of the wrong JSON type is refused wherever it sits, with
        /// its path: a handler reading it with `as_bool` / `as_f64` would
        /// otherwise get `None` and silently take the default.
        #[test]
        fn tools_call_refuses_a_value_of_the_wrong_type_wherever_it_is_nested() {
            let cases: [(&str, Value, &str); 7] = [
                (
                    "write_schlib",
                    json!({
                        "filepath": "x.SchLib",
                        "symbols": [{
                            "name": "S",
                            "rectangles": [{ "x1": 0, "y1": 0, "x2": 1, "y2": 1, "filled": "true" }],
                        }],
                    }),
                    "Argument 'symbols[0].rectangles[0].filled' must be a boolean, got string \"true\"",
                ),
                (
                    "write_pcblib",
                    json!({
                        "filepath": "x.PcbLib",
                        "footprints": [{
                            "name": "F",
                            "pads": [{ "designator": "1", "x": 0, "y": 0, "width": "1.5", "height": 1 }],
                        }],
                    }),
                    "Argument 'footprints[0].pads[0].width' must be a number, got string \"1.5\"",
                ),
                (
                    "update_pad",
                    json!({
                        "filepath": "x.PcbLib", "component_name": "F", "designator": "1",
                        "updates": { "width": "wide" },
                    }),
                    "Argument 'updates.width' must be a number, got string \"wide\"",
                ),
                (
                    "write_pcblib",
                    json!({ "filepath": "x.PcbLib", "footprints": "not a list" }),
                    "Argument 'footprints' must be an array, got string",
                ),
                (
                    "write_schlib",
                    json!({
                        "filepath": "x.SchLib",
                        "symbols": [{ "name": "S", "pins": [{ "designator": 1 }] }],
                    }),
                    "Argument 'symbols[0].pins[0].designator' must be a string, got number 1",
                ),
                (
                    "list_components",
                    json!({ "filepath": "x.PcbLib", "limit": 2.5 }),
                    "Argument 'limit' must be an integer, got number 2.5",
                ),
                (
                    "list_components",
                    json!({ "filepath": "x.PcbLib", "limit": 1e20 }),
                    "Argument 'limit' must be an integer, got number 1e+20",
                ),
            ];
            for (tool, arguments, expected) in cases {
                let err = McpServer::check_tool_arguments(tool, &arguments)
                    .expect_err(&format!("{tool}: {arguments} must be refused"));
                assert!(
                    err.contains(expected),
                    "{tool}: expected {expected:?}, got {err:?}"
                );
                assert!(
                    err.contains(tool),
                    "{tool}: the error names the tool: {err:?}"
                );
            }
        }

        /// What the check must not refuse: a whole number for an `integer`
        /// however it is written, a union type in either of its forms, a key
        /// the schema does not describe (the tools' allow-lists judge those),
        /// and a string that only looks long.
        #[test]
        fn a_whole_float_reaches_the_handler_as_the_integer_it_is() {
            // Under `integer` (alone or in a union) a whole float becomes an
            // integer; a fraction, a number-typed field and a float beyond
            // 2^53 are left as they are.
            let mut arguments = json!({
                "filepath": "x.PcbLib",
                "footprints": [{
                    "name": "F",
                    "pads": [{
                        "designator": "1", "x": 0, "y": 0, "width": 1.5, "height": 1,
                        "corner_radius_percent": 25.0, "net_index": 65535.0, "flags": 4.0,
                        "per_layer_corner_radii": [10.0, 20.0],
                    }],
                }],
            });
            McpServer::canonicalise_tool_arguments("write_pcblib", &mut arguments);
            let pad = &arguments["footprints"][0]["pads"][0];
            assert!(pad["corner_radius_percent"].is_i64(), "{pad}");
            assert_eq!(pad["corner_radius_percent"].as_u64(), Some(25));
            assert_eq!(pad["net_index"].as_u64(), Some(65535));
            assert_eq!(pad["flags"].as_u64(), Some(4), "union with integer");
            assert_eq!(
                pad["per_layer_corner_radii"][1].as_u64(),
                Some(20),
                "array items"
            );
            assert!(pad["width"].is_f64(), "a number-typed field is untouched");

            let mut arguments = json!({ "filepath": "x.PcbLib", "limit": 2.0, "offset": 1e300 });
            McpServer::canonicalise_tool_arguments("list_components", &mut arguments);
            assert_eq!(arguments["limit"].as_u64(), Some(2));
            assert!(
                arguments["offset"].is_f64(),
                "beyond 2^53 has no exact integer"
            );

            // End to end: a page of `1.0` pages by one.
            let dir = test_temp_dir();
            let path = dir.path().join("Page.PcbLib");
            create_test_pcblib(&path);
            let mut server = McpServer::new(vec![dir.path().to_path_buf()]);
            server.state = ServerState::Running;
            let r = req(
                "tools/call",
                Some(json!({
                    "name": "list_components",
                    "arguments": { "filepath": path.to_string_lossy(), "limit": 1.0 },
                })),
            );
            let response = server.handle_tools_call(&r).unwrap();
            let result = &response.result;
            assert_ne!(result["is_error"], true, "{result}");
            let text = result["content"][0]["text"]
                .as_str()
                .expect("a text result");
            let page: Value = serde_json::from_str(text).unwrap();
            assert_eq!(page["returned_count"], 1, "{page}");
            assert_eq!(page["has_more"], true, "{page}");
        }

        /// A number outside the `minimum` / `maximum` the schema states is
        /// refused by path — a floor alone, a ceiling alone, or both.
        #[test]
        fn tools_call_refuses_a_value_outside_the_range_the_schema_states() {
            let cases: [(&str, Value, &str); 5] = [
                (
                    "list_components",
                    json!({ "filepath": "x.PcbLib", "limit": 0 }),
                    "Argument 'limit' must be at least 1, got 0",
                ),
                (
                    "write_schlib",
                    json!({
                        "filepath": "x.SchLib",
                        "symbols": [{ "name": "S", "pins": [{ "designator": "1", "name": "P",
                            "x": 0, "y": 0, "length": 10, "orientation": "left",
                            "owner_part_id": -2 }] }],
                    }),
                    "Argument 'symbols[0].pins[0].owner_part_id' must be at least -1, got -2",
                ),
                (
                    "write_schlib",
                    json!({
                        "filepath": "x.SchLib",
                        "symbols": [{ "name": "S", "labels": [{ "x": 0, "y": 0, "text": "T",
                            "font_id": 0 }] }],
                    }),
                    "Argument 'symbols[0].labels[0].font_id' must be between 1 and 255, got 0",
                ),
                (
                    "write_pcblib",
                    json!({
                        "filepath": "x.PcbLib",
                        "footprints": [{ "name": "F", "pads": [{ "designator": "1", "x": 0, "y": 0,
                            "width": 1, "height": 1, "net_index": 70000 }] }],
                    }),
                    "Argument 'footprints[0].pads[0].net_index' must be between 0 and 65535, got 70000",
                ),
                (
                    "write_pcblib",
                    json!({
                        "filepath": "x.PcbLib",
                        "footprints": [{ "name": "F", "pads": [{ "designator": "1", "x": 0, "y": 0,
                            "width": 1, "height": 1, "flags": -1 }] }],
                    }),
                    "Argument 'footprints[0].pads[0].flags' must be at least 0, got -1",
                ),
            ];
            for (tool, arguments, expected) in cases {
                let err = McpServer::check_tool_arguments(tool, &arguments)
                    .expect_err(&format!("{tool}: {arguments} must be refused"));
                assert!(
                    err.contains(expected),
                    "{tool}: expected {expected:?}, got {err:?}"
                );
            }
        }

        #[test]
        fn tools_call_accepts_every_type_the_schema_allows() {
            let accepted: [(&str, Value); 5] = [
                (
                    "list_components",
                    json!({ "filepath": "x.PcbLib", "limit": 2.0 }),
                ),
                (
                    "write_pcblib",
                    json!({
                        "filepath": "x.PcbLib",
                        "footprints": [{
                            "name": "F",
                            "regions": [
                                { "layer": "Top Overlay", "vertices": [], "kind": "cutout" },
                                { "layer": "Top Overlay", "vertices": [], "kind": 4 },
                            ],
                            "pads": [{ "designator": "1", "x": 0, "y": 0, "width": 1, "height": 1,
                                       "flags": "LOCKED" },
                                     { "designator": "2", "x": 0, "y": 0, "width": 1, "height": 1,
                                       "flags": 4 }],
                        }],
                    }),
                ),
                (
                    "write_schlib",
                    json!({
                        "filepath": "x.SchLib",
                        "symbols": [{ "name": "S", "pins": [{ "designator": "1", "not_in_schema": "?" }] }],
                    }),
                ),
                (
                    "read_pcblib",
                    json!({ "filepath": "x.PcbLib", "component_name": "F" }),
                ),
                (
                    "write_pcblib",
                    json!({ "filepath": "x.PcbLib", "footprints": [{ "name": "F", "description": "d".repeat(500) }] }),
                ),
            ];
            for (tool, arguments) in accepted {
                assert!(
                    McpServer::check_tool_arguments(tool, &arguments).is_ok(),
                    "{tool}: {arguments} must pass the type check"
                );
            }
        }

        /// The schemas describe what the read tools emit: every golden
        /// library, read through `read_pcblib` / `read_schlib`, passes the
        /// type check when handed back to `write_pcblib` / `write_schlib`
        /// and `update_component`. A schema that mislabels a field's type
        /// would refuse a faithful read-modify-write here.
        #[test]
        fn every_golden_read_passes_the_type_check_as_write_arguments() {
            use crate::mcp::tools::test_support::parse_result_json;
            let samples = std::path::Path::new("scripts/samples")
                .canonicalize()
                .unwrap();
            let server = McpServer::new(vec![samples.clone()]);
            let mut checked = 0;
            for entry in std::fs::read_dir(&samples).unwrap().flatten() {
                let path = entry.path();
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let (read, list_key, write_tool, item_key) = match ext.as_str() {
                    "pcblib" => (
                        server.call_read_pcblib(&json!({ "filepath": path.to_string_lossy() })),
                        "footprints",
                        "write_pcblib",
                        "footprint",
                    ),
                    "schlib" => (
                        server.call_read_schlib(&json!({ "filepath": path.to_string_lossy() })),
                        "symbols",
                        "write_schlib",
                        "symbol",
                    ),
                    _ => continue,
                };
                assert!(
                    !read.is_error,
                    "{}: {}",
                    path.display(),
                    get_result_text(&read)
                );
                let components = parse_result_json(&read)[list_key].clone();
                let write_args =
                    json!({ "filepath": path.to_string_lossy(), list_key: components });
                McpServer::check_tool_arguments(write_tool, &write_args).unwrap_or_else(|e| {
                    panic!(
                        "{}: a faithful read must pass {write_tool}: {e}",
                        path.display()
                    )
                });
                for component in write_args[list_key].as_array().unwrap() {
                    let update_args = json!({
                        "filepath": path.to_string_lossy(),
                        "component_name": component["name"],
                        item_key: component,
                    });
                    McpServer::check_tool_arguments("update_component", &update_args)
                        .unwrap_or_else(|e| {
                            panic!(
                                "{}: {} must pass update_component: {e}",
                                path.display(),
                                component["name"]
                            )
                        });
                    checked += 1;
                }
            }
            assert!(
                checked > 100,
                "the golden samples were read: {checked} components"
            );
        }

        #[test]
        fn tools_call_missing_and_invalid_params_error() {
            let dir = test_temp_dir();
            let server = running_server(dir.path());
            assert!(server.handle_tools_call(&req("tools/call", None)).is_err());
            assert!(server
                .handle_tools_call(&req("tools/call", Some(json!("not an object"))))
                .is_err());
        }

        #[test]
        fn requests_before_running_are_rejected() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path()); // AwaitingInit
            assert!(server.handle_tools_list(&req("tools/list", None)).is_err());
            assert!(server
                .handle_tools_call(&req(
                    "tools/call",
                    Some(json!({ "name": "ping", "arguments": {} }))
                ))
                .is_err());
        }

        #[test]
        fn handle_initialize_success_and_error_paths() {
            let dir = test_temp_dir();

            // Success: AwaitingInit -> Initialising.
            let mut server = create_test_server(dir.path());
            let r = req(
                "initialize",
                Some(json!({ "protocolVersion": "2024-11-05", "capabilities": {} })),
            );
            let resp = server.handle_initialize(&r).expect("initialise ok");
            assert_eq!(resp.result["protocolVersion"], "2024-11-05");
            assert_eq!(server.state(), ServerState::Initialising);

            // Already initialised -> InvalidRequest.
            let mut running = running_server(dir.path());
            let err = running.handle_initialize(&r).unwrap_err();
            assert!(err.error.message.contains("already initialised"));

            // Missing params and invalid params.
            let mut fresh = create_test_server(dir.path());
            assert!(fresh.handle_initialize(&req("initialize", None)).is_err());
            let mut fresh2 = create_test_server(dir.path());
            assert!(fresh2
                .handle_initialize(&req("initialize", Some(json!("bad"))))
                .is_err());
        }

        #[tokio::test]
        async fn handle_transport_result_eof_shuts_down() {
            let dir = test_temp_dir();
            let mut server = create_test_server(dir.path());
            let shutdown = server.handle_transport_result(Ok(None)).await.unwrap();
            assert!(shutdown);
            assert_eq!(server.state(), ServerState::ShuttingDown);
        }

        #[tokio::test]
        async fn handle_transport_result_empty_line_continues() {
            let dir = test_temp_dir();
            let mut server = create_test_server(dir.path());
            let shutdown = server
                .handle_transport_result(Ok(Some("   ".to_string())))
                .await
                .unwrap();
            assert!(!shutdown);
        }

        #[tokio::test]
        async fn handle_transport_result_propagates_io_error() {
            let dir = test_temp_dir();
            let mut server = create_test_server(dir.path());
            let e = std::io::Error::other("boom");
            assert!(server.handle_transport_result(Err(e)).await.is_err());
        }

        #[tokio::test]
        async fn handle_transport_result_processes_ping_line() {
            let dir = test_temp_dir();
            let mut server = create_test_server(dir.path());
            let sink = server.transport.capture_output();
            let shutdown = server
                .handle_transport_result(Ok(Some(
                    r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#.to_string(),
                )))
                .await
                .unwrap();
            assert!(!shutdown);
            let out = String::from_utf8(sink.lock().unwrap().clone()).unwrap();
            assert!(out.contains("\"id\":1"));
        }

        #[tokio::test]
        async fn full_lifecycle_via_handle_line() {
            let dir = test_temp_dir();
            let mut server = create_test_server(dir.path());
            let sink = server.transport.capture_output();

            server
                .handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#)
                .await
                .unwrap();
            assert_eq!(server.state(), ServerState::Initialising);

            server
                .handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .await
                .unwrap();
            assert_eq!(server.state(), ServerState::Running);

            server
                .handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
                .await
                .unwrap();

            let out = String::from_utf8(sink.lock().unwrap().clone()).unwrap();
            assert!(out.contains("protocolVersion"));
            assert!(out.contains("read_pcblib"));
        }

        #[tokio::test]
        async fn handle_line_writes_parse_error_and_method_not_found() {
            let dir = test_temp_dir();
            let mut server = create_test_server(dir.path());
            let sink = server.transport.capture_output();

            server.handle_line("{not valid json").await.unwrap();
            server
                .handle_line(r#"{"jsonrpc":"2.0","id":9,"method":"bogus/method"}"#)
                .await
                .unwrap();

            let out = String::from_utf8(sink.lock().unwrap().clone()).unwrap();
            assert!(out.contains("-32700")); // parse error
            assert!(out.contains("-32601")); // method not found
        }

        #[test]
        fn audit_logger_records_mutating_call() {
            let dir = test_temp_dir();
            let audit_path = dir.path().join("audit.jsonl");
            let mut server = McpServer::new(vec![dir.path().to_path_buf()])
                .with_rate_limiter(RateLimiter::new(1000, 0.0))
                .with_audit_logger(Some(AuditLogger::new(audit_path.clone())));
            server.state = ServerState::Running;

            let r = req(
                "tools/call",
                Some(json!({ "name": "write_pcblib", "arguments": {} })),
            );
            let _ = server.handle_tools_call(&r).unwrap();

            let logged = std::fs::read_to_string(&audit_path).unwrap();
            assert!(logged.contains("write_pcblib"));
        }

        /// A successful mutating call is recorded with its outcome and the
        /// file it touched — the file name only, never the directory.
        #[test]
        fn audit_logger_records_a_successful_call_by_file_name() {
            let dir = test_temp_dir();
            let audit_path = dir.path().join("audit.jsonl");
            let lib = dir.path().join("Audited.PcbLib");
            let mut server = McpServer::new(vec![dir.path().to_path_buf()])
                .with_rate_limiter(RateLimiter::new(1000, 0.0))
                .with_audit_logger(Some(AuditLogger::new(audit_path.clone())));
            server.state = ServerState::Running;

            let r = req(
                "tools/call",
                Some(json!({
                    "name": "write_pcblib",
                    "arguments": {
                        "filepath": lib.to_string_lossy(),
                        "footprints": [{
                            "name": "A",
                            "pads": [{ "designator": "1", "x": 0, "y": 0, "width": 1, "height": 1 }],
                        }],
                    },
                })),
            );
            let _ = server.handle_tools_call(&r).unwrap();
            assert!(lib.exists(), "the call succeeded");

            // One JSON event per line; the last is this call's.
            let logged = std::fs::read_to_string(&audit_path).unwrap();
            let event: Value = serde_json::from_str(logged.lines().last().unwrap()).unwrap();
            assert_eq!(event["operation"], "write_pcblib");
            assert_eq!(event["outcome"], "success");
            assert_eq!(event["filepath"], "Audited.PcbLib");
        }

        #[test]
        fn error_context_builders_populate_fields() {
            let ctx = ErrorContext::new("write_pcblib", "boom")
                .with_filepath("/libs/x.PcbLib")
                .with_component("R1")
                .with_details("while saving");
            assert_eq!(ctx.operation, "write_pcblib");
            assert_eq!(ctx.filepath.as_deref(), Some("/libs/x.PcbLib"));
            assert_eq!(ctx.component.as_deref(), Some("R1"));
            assert_eq!(ctx.details.as_deref(), Some("while saving"));
        }

        #[test]
        fn cleanup_old_backups_keeps_only_the_most_recent() {
            let dir = test_temp_dir();
            let original = dir.path().join("Lib.PcbLib");
            std::fs::write(&original, b"x").unwrap();
            // Seven timestamped backups; cleanup keeps the newest MAX_BACKUPS (5).
            for n in 1..=7 {
                let bak = dir.path().join(format!("Lib.PcbLib.20260101_12000{n}.bak"));
                std::fs::write(&bak, b"x").unwrap();
            }
            McpServer::cleanup_old_backups(&original.to_string_lossy());

            let remaining = std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("bak"))
                .count();
            assert_eq!(remaining, 5);
        }

        /// What pruning leaves alone: a file whose middle part is not a
        /// timestamp (not a backup), a backup it cannot remove (reported,
        /// not fatal), and a path with no parent, no file name or a
        /// directory that cannot be read.
        #[test]
        fn cleanup_old_backups_skips_non_backups_and_survives_a_stubborn_one() {
            let dir = test_temp_dir();
            let original = dir.path().join("Lib.PcbLib");
            std::fs::write(&original, b"x").unwrap();
            let odd = dir.path().join("Lib.PcbLib.notatimestamp.bak");
            std::fs::write(&odd, b"x").unwrap();
            // The oldest "backup" is a directory, which remove_file refuses.
            let stubborn = dir.path().join("Lib.PcbLib.20200101_000000.bak");
            std::fs::create_dir(&stubborn).unwrap();
            for n in 1..=6 {
                let bak = dir.path().join(format!("Lib.PcbLib.20260101_12000{n}.bak"));
                std::fs::write(&bak, b"x").unwrap();
            }

            McpServer::cleanup_old_backups(&original.to_string_lossy());

            assert!(odd.exists(), "not a backup, so not pruned");
            assert!(
                stubborn.exists(),
                "could not be removed; reported, not fatal"
            );
            assert!(
                !dir.path().join("Lib.PcbLib.20260101_120001.bak").exists(),
                "the sixth-newest backup is pruned"
            );
            assert!(dir.path().join("Lib.PcbLib.20260101_120002.bak").exists());

            // Nothing to do, and nothing to fail on, for these.
            McpServer::cleanup_old_backups("");
            McpServer::cleanup_old_backups("Lib.PcbLib/..");
            McpServer::cleanup_old_backups(
                &dir.path()
                    .join("missing")
                    .join("Lib.PcbLib")
                    .to_string_lossy(),
            );
        }
    }

    // ==================== path gate and backup retention =====================

    mod path_and_backups {
        use crate::altium::pcblib::PcbLib;
        use crate::altium::schlib::SchLib;
        use crate::mcp::server::McpServer;
        use crate::mcp::tools::test_support::{create_test_schlib, get_result_text, test_temp_dir};

        #[test]
        fn a_configured_allowed_path_that_does_not_exist_is_skipped_not_trusted() {
            // An allow-list entry pointing at a directory that was never
            // created cannot be canonicalised. Skipping it must not open the
            // gate — a path is allowed only by matching an entry that resolves.
            let real = test_temp_dir();
            let server = McpServer::new(vec![
                real.path().join("never_created"),
                real.path().to_path_buf(),
            ]);

            // Inside the entry that does resolve: allowed.
            assert!(server
                .validate_path(&real.path().join("Lib.PcbLib").to_string_lossy())
                .is_ok());

            // With only the unresolvable entry configured, nothing is allowed.
            let blind = McpServer::new(vec![real.path().join("never_created")]);
            let err = blind
                .validate_path(&real.path().join("Lib.PcbLib").to_string_lossy())
                .expect_err("an unresolvable allow-list must not permit anything");
            assert!(err.contains("Access denied"), "{err}");
        }

        /// An empty file path is refused as what it is — an empty path has
        /// no parent to check, which is not "the filesystem root".
        #[test]
        fn an_empty_path_is_refused_as_empty() {
            let dir = test_temp_dir();
            let server = McpServer::new(vec![dir.path().to_path_buf()]);
            let err = server
                .validate_path("")
                .expect_err("an empty path must be refused");
            assert_eq!(err, "Invalid path: no file path was given");
        }

        #[test]
        fn validate_path_reports_a_missing_parent_without_leaking_it() {
            // Write targets are checked through their parent directory, and
            // the message names only the file — never the resolved path.
            let dir = test_temp_dir();
            let server = McpServer::new(vec![dir.path().to_path_buf()]);
            let missing = dir.path().join("no_such_dir").join("Lib.PcbLib");

            let err = server
                .validate_path(&missing.to_string_lossy())
                .expect_err("a missing parent directory must be refused");
            assert!(err.contains("Lib.PcbLib"), "{err}");
            assert!(
                !err.contains("no_such_dir"),
                "the message leaked the directory: {err}"
            );
        }

        #[test]
        fn backup_then_save_reports_a_failed_backup_before_saving() {
            // The backup runs first, so a failure there must stop the write
            // rather than let a save proceed with no recovery point. A
            // directory standing where the library should be exists, so a
            // backup is attempted, and copying a directory fails.
            let dir = test_temp_dir();
            let as_dir = dir.path().join("Blocked.PcbLib");
            std::fs::create_dir(&as_dir).unwrap();

            let mut library = PcbLib::new();
            let result = McpServer::backup_then_save(&as_dir.to_string_lossy(), &mut library);
            assert!(result.is_err(), "a failed backup must abort the save");
            assert!(
                std::fs::read_dir(&as_dir).unwrap().next().is_none(),
                "the save ran despite the backup failing"
            );
        }

        /// Text the records cannot hold is refused before any backup is made:
        /// the file the tool was about to touch keeps its one copy.
        #[test]
        fn backup_then_save_refuses_record_text_before_backing_up() {
            let dir = test_temp_dir();
            let lib = dir.path().join("Lib.SchLib");
            create_test_schlib(&lib);
            let mut library = SchLib::open(&lib).unwrap();
            library.get_mut("RESISTOR").unwrap().description = "A|B".to_string();

            let err = McpServer::backup_then_save(&lib.to_string_lossy(), &mut library)
                .expect_err("record text the format cannot hold must be refused");
            let text = get_result_text(&err);
            assert!(
                text.contains("Symbol 'RESISTOR' description contains '|'"),
                "{text}"
            );
            let backups = std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|x| x == "bak"))
                .count();
            assert_eq!(backups, 0, "no backup for a save that never happens");
            assert_eq!(
                SchLib::open(&lib)
                    .unwrap()
                    .get("RESISTOR")
                    .unwrap()
                    .description,
                "Generic resistor",
                "the file is untouched"
            );
        }

        #[test]
        fn backup_retention_keeps_the_newest_and_prunes_the_rest() {
            // Backups accumulate on every mutating call, so retention is what
            // stops a busy library filling the directory. Newest are kept.
            let dir = test_temp_dir();
            let lib = dir.path().join("Lib.PcbLib");
            std::fs::write(&lib, b"library").unwrap();

            // Seven stamped backups, oldest first, plus two near-misses that
            // are not retention's business: an unstamped `.bak` and a file
            // belonging to a different library.
            let stamps: Vec<String> = (1..=7).map(|i| format!("2026010{i}_010101")).collect();
            for stamp in &stamps {
                std::fs::write(dir.path().join(format!("Lib.PcbLib.{stamp}.bak")), b"old").unwrap();
            }
            std::fs::write(dir.path().join("Lib.PcbLib.bak"), b"unstamped").unwrap();
            std::fs::write(
                dir.path().join("Other.PcbLib.20260101_010101.bak"),
                b"other",
            )
            .unwrap();

            McpServer::cleanup_old_backups(&lib.to_string_lossy());

            let survivors: Vec<String> = std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| {
                    n.starts_with("Lib.PcbLib.")
                        && std::path::Path::new(n)
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("bak"))
                })
                .collect();

            // Five stamped survivors plus the unstamped one, which retention
            // deliberately does not touch.
            assert_eq!(survivors.len(), 6, "{survivors:?}");
            for stamp in &stamps[2..] {
                assert!(
                    survivors.iter().any(|n| n.contains(stamp.as_str())),
                    "newest backup {stamp} was pruned: {survivors:?}"
                );
            }
            for stamp in &stamps[..2] {
                assert!(
                    !survivors.iter().any(|n| n.contains(stamp.as_str())),
                    "oldest backup {stamp} survived: {survivors:?}"
                );
            }
            assert!(survivors.iter().any(|n| n == "Lib.PcbLib.bak"));
            assert!(dir.path().join("Other.PcbLib.20260101_010101.bak").exists());
        }

        #[test]
        fn creating_a_backup_of_a_missing_file_is_a_no_op() {
            // Every mutating tool calls this, including on a brand-new
            // library, so "nothing to back up" has to succeed quietly.
            let dir = test_temp_dir();
            let absent = dir.path().join("Nope.PcbLib");
            assert_eq!(
                McpServer::create_backup(&absent.to_string_lossy()),
                Ok(None)
            );

            let present = dir.path().join("Here.PcbLib");
            std::fs::write(&present, b"library").unwrap();
            let made = McpServer::create_backup(&present.to_string_lossy())
                .expect("an existing file should back up");
            let made = made.expect("a backup path should be reported");
            assert!(std::path::Path::new(&made).exists(), "{made}");
        }
    }

    /// The type check's own corners: a `null` type, a type it does not know
    /// (left to the parser), a union type, every JSON type named in the
    /// message, the message's 40-character cap on a long string, and
    /// `items` that is not a schema object (JSON Schema allows `true`),
    /// which checks nothing.
    #[test]
    fn check_value_against_schema_covers_every_schema_form() {
        let check = |value: Value, schema: Value| check_value_against_schema(&value, &schema, "v");

        assert!(check(json!(null), json!({ "type": "null" })).is_ok());
        assert!(check(json!("2026-08-30"), json!({ "type": "date" })).is_ok());

        let union = json!({ "type": ["string", "integer"] });
        assert!(check(json!("LOCKED"), union.clone()).is_ok());
        assert!(check(json!(4), union.clone()).is_ok());
        assert_eq!(
            check(json!(true), union).unwrap_err(),
            "Argument 'v' must be a string or integer, got boolean true"
        );

        for (value, name) in [
            (json!(null), "null"),
            (json!(false), "boolean"),
            (json!([1]), "array"),
            (json!({ "a": 1 }), "object"),
        ] {
            let err = check(value, json!({ "type": "number" })).unwrap_err();
            assert!(err.contains(&format!("got {name} ")), "{err}");
        }

        let err = check(json!("x".repeat(60)), json!({ "type": "number" })).unwrap_err();
        assert!(
            err.ends_with(&format!("got string \"{}…\"", "x".repeat(40))),
            "{err}"
        );

        assert!(check(json!([1, "two"]), json!({ "type": "array", "items": true })).is_ok());
    }

    /// Every integer-typed argument, however deep, states its floor — the
    /// dispatch check can only refuse an out-of-range value where the schema
    /// gives the range, and a negative under an unsigned field used to read
    /// as absent. A pin's position and a model checksum are signed and
    /// unbounded by design.
    #[test]
    fn every_integer_argument_states_its_floor() {
        fn walk(schema: &Value, path: &str, missing: &mut Vec<String>) {
            let integer_typed = match &schema["type"] {
                Value::String(t) => t == "integer",
                Value::Array(ts) => ts.iter().any(|t| t == "integer"),
                _ => false,
            };
            let key = path.rsplit(['.', '[']).next().unwrap_or(path);
            let unbounded = matches!(key, "x" | "y" | "model_checksum");
            if integer_typed && !unbounded && schema.get("minimum").is_none() {
                missing.push(path.to_string());
            }
            if let Some(properties) = schema["properties"].as_object() {
                for (name, child) in properties {
                    walk(child, &format!("{path}.{name}"), missing);
                }
            }
            if schema["items"].is_object() {
                walk(&schema["items"], &format!("{path}[]"), missing);
            }
        }
        let mut missing = Vec::new();
        for tool in McpServer::get_tool_definitions() {
            walk(&tool.input_schema, &tool.name, &mut missing);
        }
        assert!(
            missing.is_empty(),
            "integer arguments without a minimum: {missing:#?}"
        );
    }
}
