//! `SchLib` parameter/footprint management tools. Split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

impl McpServer {
    /// Manages parameters in a `SchLib` symbol.
    #[allow(clippy::too_many_lines, clippy::option_if_let_else)]
    pub(crate) fn call_manage_schlib_parameters(&self, arguments: &Value) -> ToolCallResult {
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
                let Some(symbol) = library.get_mut(component_name) else {
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

                let mut result = match operation {
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
                                // De-hardcoded core fields (omit-when-default on
                                // write, so leaving them unset stays byte-identical).
                                if let Some(ros) = arguments
                                    .get("read_only_state")
                                    .and_then(Value::as_u64)
                                    .and_then(|v| u8::try_from(v).ok())
                                {
                                    p.read_only_state = ros;
                                }
                                if let Some(pt) = arguments
                                    .get("param_type")
                                    .and_then(Value::as_u64)
                                    .and_then(|v| u8::try_from(v).ok())
                                {
                                    p.param_type = pt;
                                }
                                if let Some(uid) =
                                    arguments.get("unique_id").and_then(Value::as_str)
                                {
                                    p.unique_id = Some(uid.to_string());
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
                        if let Some(ros) = arguments
                            .get("read_only_state")
                            .and_then(Value::as_u64)
                            .and_then(|v| u8::try_from(v).ok())
                        {
                            param.read_only_state = ros;
                        }
                        if let Some(pt) = arguments
                            .get("param_type")
                            .and_then(Value::as_u64)
                            .and_then(|v| u8::try_from(v).ok())
                        {
                            param.param_type = pt;
                        }
                        if let Some(uid) = arguments.get("unique_id").and_then(Value::as_str) {
                            param.unique_id = Some(uid.to_string());
                        }
                        if let Some(x) = arguments.get("x").and_then(Value::as_f64) {
                            if let Err(e) = Self::validate_schlib_coordinate(x, "parameter x") {
                                return ToolCallResult::error(e);
                            }
                            param.x = x;
                        }
                        if let Some(y) = arguments.get("y").and_then(Value::as_f64) {
                            if let Err(e) = Self::validate_schlib_coordinate(y, "parameter y") {
                                return ToolCallResult::error(e);
                            }
                            param.y = y;
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

                if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
                    return resp;
                }

                // Run post-write validation
                if let Some(validation) = Self::post_write_validation_schlib(filepath) {
                    result["validation"] = validation;
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
    pub(crate) fn call_manage_schlib_footprints(&self, arguments: &Value) -> ToolCallResult {
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
                            "library_path": f.library_path,
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
                let Some(symbol) = library.get_mut(component_name) else {
                    return ToolCallResult::error(format!(
                        "Symbol '{component_name}' not found in library"
                    ));
                };

                let mut result = if operation == "add" {
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

                    // Apply optional PcbLib path so Altium can resolve the
                    // footprint (written as ModelDatafile0). Without it the model
                    // links by name only and shows "footprint not found" unless
                    // the library is installed/in the project.
                    if let Some(lib_path) = arguments.get("library_path").and_then(Value::as_str) {
                        footprint.library_path = Some(lib_path.to_string());
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

                if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
                    return resp;
                }

                // Run post-write validation
                if let Some(validation) = Self::post_write_validation_schlib(filepath) {
                    result["validation"] = validation;
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

    use crate::altium::SchLib;
    use crate::mcp::tools::test_support::{
        create_test_schlib, create_test_server, get_result_text, parse_result_json, test_temp_dir,
    };
    use serde_json::json;

    // ==================== manage_schlib_parameters ====================

    #[test]
    fn parameters_missing_required_arguments() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let result = server.call_manage_schlib_parameters(&json!({}));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: filepath"
        );

        let result = server.call_manage_schlib_parameters(&json!({ "filepath": "x.SchLib" }));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: component_name"
        );

        let result = server.call_manage_schlib_parameters(
            &json!({ "filepath": "x.SchLib", "component_name": "RESISTOR" }),
        );
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: operation"
        );
    }

    #[test]
    fn parameters_rejects_non_schlib_extension() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Lib.PcbLib");

        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "RESISTOR",
            "operation": "list",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("only supports SchLib files"));
    }

    #[test]
    fn parameters_rejects_path_outside_allowed() {
        let dir = test_temp_dir();
        let other = test_temp_dir();
        let server = create_test_server(dir.path());
        let outside = other.path().join("Out.SchLib");
        create_test_schlib(&outside);

        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": outside.to_string_lossy(),
            "component_name": "RESISTOR",
            "operation": "list",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Access denied"));
    }

    #[test]
    fn parameters_add_get_list_set_delete_round_trip() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Params.SchLib");
        create_test_schlib(&path);
        let filepath = path.to_string_lossy().to_string();

        // Add a parameter with optional placement.
        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "add",
            "parameter_name": "Tolerance",
            "value": "1%",
            "hidden": true,
            "x": 5.0,
            "y": -10.0,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["operation"], "add");
        assert_eq!(parsed["parameter"]["name"], "Tolerance");
        assert_eq!(parsed["parameter"]["value"], "1%");

        // Get it back (case-insensitive) — proves the write persisted.
        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "get",
            "parameter_name": "tolerance",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["parameter"]["name"], "Tolerance");
        assert_eq!(parsed["parameter"]["value"], "1%");
        assert_eq!(parsed["parameter"]["hidden"], true);
        assert_eq!(parsed["parameter"]["x"], 5.0);
        assert_eq!(parsed["parameter"]["y"], -10.0);

        // List includes it.
        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "list",
        }));
        assert!(!result.is_error);
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["operation"], "list");
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["parameters"][0]["name"], "Tolerance");

        // Set updates value and visibility.
        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "set",
            "parameter_name": "Tolerance",
            "value": "5%",
            "hidden": false,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["parameter"]["value"], "5%");
        assert_eq!(parsed["parameter"]["hidden"], false);

        // Verify on disk via the library layer (stable fields).
        let lib = SchLib::open(&path).unwrap();
        let sym = lib.get("RESISTOR").unwrap();
        assert_eq!(sym.parameters.len(), 1);
        assert_eq!(sym.parameters[0].name, "Tolerance");
        assert_eq!(sym.parameters[0].value, "5%");
        assert!(!sym.parameters[0].hidden);

        // Delete removes it.
        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "delete",
            "parameter_name": "Tolerance",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["deleted_parameter"], "Tolerance");

        let lib = SchLib::open(&path).unwrap();
        assert!(lib.get("RESISTOR").unwrap().parameters.is_empty());
    }

    #[test]
    fn parameters_error_paths() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Err.SchLib");
        create_test_schlib(&path);
        let filepath = path.to_string_lossy().to_string();

        // Unknown symbol.
        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": filepath,
            "component_name": "NOPE",
            "operation": "list",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Symbol 'NOPE' not found"));

        // Get a parameter that does not exist.
        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "get",
            "parameter_name": "Voltage",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Parameter 'Voltage' not found"));

        // Set a parameter that does not exist.
        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "set",
            "parameter_name": "Voltage",
            "value": "50V",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Use 'add' operation"));

        // Delete a parameter that does not exist.
        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "delete",
            "parameter_name": "Voltage",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Parameter 'Voltage' not found"));

        // Unknown operation.
        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "rename",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unknown operation: rename"));

        // Add without value.
        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "add",
            "parameter_name": "Voltage",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Missing required parameter: value"));
    }

    #[test]
    fn parameters_add_duplicate_is_rejected() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Dup.SchLib");
        create_test_schlib(&path);
        let filepath = path.to_string_lossy().to_string();

        let add = json!({
            "filepath": filepath,
            "component_name": "CAPACITOR",
            "operation": "add",
            "parameter_name": "Voltage",
            "value": "50V",
        });
        let result = server.call_manage_schlib_parameters(&add);
        assert!(!result.is_error, "{}", get_result_text(&result));

        let result = server.call_manage_schlib_parameters(&add);
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("already exists"));
        assert!(get_result_text(&result).contains("Use 'set' operation"));
    }

    #[test]
    fn parameters_add_rejects_out_of_range_coordinate() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Range.SchLib");
        create_test_schlib(&path);

        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "RESISTOR",
            "operation": "add",
            "parameter_name": "Voltage",
            "value": "50V",
            "x": 999_999.0,
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("parameter x"));
    }

    // ==================== manage_schlib_footprints ====================

    #[test]
    fn footprints_list_add_remove_round_trip() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Fps.SchLib");
        create_test_schlib(&path);
        let filepath = path.to_string_lossy().to_string();

        // Initially empty.
        let result = server.call_manage_schlib_footprints(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "list",
        }));
        assert!(!result.is_error);
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["count"], 0);

        // Add a footprint link with description and library path.
        let result = server.call_manage_schlib_footprints(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "add",
            "footprint_name": "CHIP_0402",
            "description": "0402 body",
            "library_path": "Resistors.PcbLib",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["footprint"], "CHIP_0402");

        // List reflects the persisted link with its metadata.
        let result = server.call_manage_schlib_footprints(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "list",
        }));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["footprints"][0]["name"], "CHIP_0402");
        assert_eq!(parsed["footprints"][0]["description"], "0402 body");
        assert_eq!(parsed["footprints"][0]["library_path"], "Resistors.PcbLib");

        // Duplicate add is rejected.
        let result = server.call_manage_schlib_footprints(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "add",
            "footprint_name": "chip_0402",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("already linked"));

        // Remove it (case-insensitive).
        let result = server.call_manage_schlib_footprints(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "remove",
            "footprint_name": "chip_0402",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["removed_footprint"], "chip_0402");

        let lib = SchLib::open(&path).unwrap();
        assert!(lib.get("RESISTOR").unwrap().footprints.is_empty());
    }

    #[test]
    fn footprints_error_paths() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("FpErr.SchLib");
        create_test_schlib(&path);
        let filepath = path.to_string_lossy().to_string();

        // Missing operation.
        let result = server.call_manage_schlib_footprints(
            &json!({ "filepath": filepath, "component_name": "RESISTOR" }),
        );
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: operation"
        );

        // Missing footprint_name for add.
        let result = server.call_manage_schlib_footprints(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "add",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Missing required parameter: footprint_name"));

        // Removing a link that does not exist.
        let result = server.call_manage_schlib_footprints(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "remove",
            "footprint_name": "MISSING_FP",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Footprint 'MISSING_FP' not found"));

        // Unknown symbol.
        let result = server.call_manage_schlib_footprints(&json!({
            "filepath": filepath,
            "component_name": "NOPE",
            "operation": "list",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Symbol 'NOPE' not found"));

        // Unknown operation.
        let result = server.call_manage_schlib_footprints(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "swap",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unknown operation: swap"));
    }
}
