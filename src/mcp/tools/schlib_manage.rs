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
                    return ToolCallResult::error(super::component_not_found(
                        component_name,
                        &library.names(),
                    ));
                };

                if operation == "list" {
                    // Every parameter in the shape read_schlib and
                    // get_component use.
                    let params: Vec<Value> = symbol
                        .parameters
                        .iter()
                        .map(|p| serde_json::to_value(p).unwrap_or(Value::Null))
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
                            "parameter": serde_json::to_value(p).unwrap_or(Value::Null),
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
                let names = library.names();
                let Some(symbol) = library.get_mut(component_name) else {
                    return ToolCallResult::error(super::component_not_found(
                        component_name,
                        &names,
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
                                // A parameter is moved the same way it is placed.
                                if let Some(x) = arguments.get("x").and_then(Value::as_f64) {
                                    if let Err(e) =
                                        Self::validate_schlib_coordinate(x, "parameter x")
                                    {
                                        return ToolCallResult::error(e);
                                    }
                                    p.x = x;
                                }
                                if let Some(y) = arguments.get("y").and_then(Value::as_f64) {
                                    if let Err(e) =
                                        Self::validate_schlib_coordinate(y, "parameter y")
                                    {
                                        return ToolCallResult::error(e);
                                    }
                                    p.y = y;
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
                        // Every parameter of that name goes, last first so the
                        // earlier indices stay valid; the record order is kept
                        // in step so nothing else in the symbol moves.
                        let matches: Vec<usize> = symbol
                            .parameters
                            .iter()
                            .enumerate()
                            .filter(|(_, p)| p.name.eq_ignore_ascii_case(parameter_name))
                            .map(|(index, _)| index)
                            .collect();
                        if matches.is_empty() {
                            return ToolCallResult::error(format!(
                                "Parameter '{parameter_name}' not found in symbol '{component_name}'"
                            ));
                        }
                        for index in matches.into_iter().rev() {
                            symbol.remove_parameter(index);
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

                if let Err(resp) = Self::backup_then_save(filepath, &mut library) {
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
                    return ToolCallResult::error(super::component_not_found(
                        component_name,
                        &library.names(),
                    ));
                };

                // Every link in the shape read_schlib and get_component use.
                let footprints: Vec<Value> = symbol
                    .footprints
                    .iter()
                    .map(|f| serde_json::to_value(f).unwrap_or(Value::Null))
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
                let names = library.names();
                let Some(symbol) = library.get_mut(component_name) else {
                    return ToolCallResult::error(super::component_not_found(
                        component_name,
                        &names,
                    ));
                };

                let mut result = if operation == "add" {
                    // Check if footprint already exists (a simulation or 3D
                    // model of the same name is another kind of link)
                    if symbol
                        .footprints
                        .iter()
                        .any(|f| f.is_footprint() && f.name.eq_ignore_ascii_case(footprint_name))
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
                    // Remove the footprint, leaving any other kind of model
                    // link of the same name
                    let original_len = symbol.footprints.len();
                    symbol.footprints.retain(|f| {
                        !(f.is_footprint() && f.name.eq_ignore_ascii_case(footprint_name))
                    });

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

                if let Err(resp) = Self::backup_then_save(filepath, &mut library) {
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
        assert!(get_result_text(&result).contains("Component 'NOPE' not found in library"));

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
        // The whole link, as read_schlib reports it — not a three-field excerpt.
        assert_eq!(parsed["footprints"][0]["is_current"], true, "{parsed}");
        assert!(
            parsed["footprints"][0].get("unique_id").is_some(),
            "{parsed}"
        );

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
        assert!(get_result_text(&result).contains("Component 'NOPE' not found in library"));

        // Unknown operation.
        let result = server.call_manage_schlib_footprints(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "operation": "swap",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unknown operation: swap"));
    }

    // ==================== error paths ====================

    /// The branches the round-trip and `*_error_paths` tests above cannot reach:
    /// the corrupt-library arms, the argument checks that only the mutating
    /// operations perform, and the whole guard set on `manage_schlib_footprints`,
    /// which until now was only exercised through `manage_schlib_parameters`.
    mod error_paths {
        use super::*;
        use std::path::Path;

        /// Writes bytes that are not an OLE compound document, standing in for a
        /// truncated or transfer-mangled library file.
        fn write_corrupt_schlib(path: &Path) {
            std::fs::write(path, b"not an OLE compound document").expect("write corrupt file");
        }

        /// A server whose only allowed path holds a corrupt `.SchLib`.
        fn corrupt_library() -> (tempfile::TempDir, crate::mcp::server::McpServer, String) {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Corrupt.SchLib");
            write_corrupt_schlib(&path);
            let filepath = path.to_string_lossy().to_string();
            (dir, server, filepath)
        }

        #[test]
        fn parameters_report_a_corrupt_library_on_read() {
            // `list` and `get` open the library read-only; `set`, `add` and
            // `delete` open it again on a separate arm, so both need covering.
            let (_dir, server, filepath) = corrupt_library();

            for operation in ["list", "get"] {
                let result = server.call_manage_schlib_parameters(&json!({
                    "filepath": filepath,
                    "component_name": "RESISTOR",
                    "operation": operation,
                    "parameter_name": "Value",
                }));
                assert!(
                    result.is_error,
                    "{operation} must fail on a corrupt library"
                );
                assert!(
                    get_result_text(&result).contains("Failed to read library"),
                    "{operation} must name the read failure, got: {}",
                    get_result_text(&result)
                );
            }
        }

        #[test]
        fn parameters_report_a_corrupt_library_on_mutation() {
            let (_dir, server, filepath) = corrupt_library();

            for operation in ["set", "add", "delete"] {
                let result = server.call_manage_schlib_parameters(&json!({
                    "filepath": filepath,
                    "component_name": "RESISTOR",
                    "operation": operation,
                    "parameter_name": "Value",
                    "value": "1k",
                }));
                assert!(
                    result.is_error,
                    "{operation} must fail on a corrupt library"
                );
                assert!(
                    get_result_text(&result).contains("Failed to read library"),
                    "{operation} must name the read failure, got: {}",
                    get_result_text(&result)
                );
            }
        }

        #[test]
        fn parameters_get_requires_a_parameter_name() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("GetNoName.SchLib");
            create_test_schlib(&path);

            let result = server.call_manage_schlib_parameters(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "get",
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result)
                .contains("Missing required parameter: parameter_name (required for get"));
        }

        #[test]
        fn parameters_mutations_require_a_parameter_name() {
            // A separate check from the `get` one above, and it names the
            // operation in the message, so each arm is worth asserting.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("MutNoName.SchLib");
            create_test_schlib(&path);
            let filepath = path.to_string_lossy().to_string();

            for operation in ["set", "add", "delete"] {
                let result = server.call_manage_schlib_parameters(&json!({
                    "filepath": filepath,
                    "component_name": "RESISTOR",
                    "operation": operation,
                }));
                assert!(result.is_error, "{operation} must require parameter_name");
                assert!(
                    get_result_text(&result).contains(&format!(
                        "Missing required parameter: parameter_name (required for {operation}"
                    )),
                    "{operation} must name itself in the message, got: {}",
                    get_result_text(&result)
                );
            }
        }

        #[test]
        fn parameters_mutations_report_an_unknown_symbol() {
            // The read path resolves the symbol through `get`, the mutating path
            // through `get_mut` — a second, separately uncovered branch.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("MutNoSym.SchLib");
            create_test_schlib(&path);
            let filepath = path.to_string_lossy().to_string();

            for operation in ["set", "add", "delete"] {
                let result = server.call_manage_schlib_parameters(&json!({
                    "filepath": filepath,
                    "component_name": "NOPE",
                    "operation": operation,
                    "parameter_name": "Value",
                    "value": "1k",
                }));
                assert!(result.is_error, "{operation} must reject an unknown symbol");
                assert!(get_result_text(&result).contains("Component 'NOPE' not found in library"));
            }
        }

        #[test]
        fn parameters_add_rejects_an_out_of_range_y() {
            // The x guard is covered above; y is a separate branch.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("RangeY.SchLib");
            create_test_schlib(&path);

            let result = server.call_manage_schlib_parameters(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "add",
                "parameter_name": "Voltage",
                "value": "50V",
                "y": 999_999.0,
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("parameter y"));
        }

        #[test]
        fn footprints_require_filepath_and_component_name() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let result = server.call_manage_schlib_footprints(&json!({
                "component_name": "RESISTOR",
                "operation": "list",
            }));
            assert!(result.is_error);
            assert_eq!(
                get_result_text(&result),
                "Missing required parameter: filepath"
            );

            let result = server.call_manage_schlib_footprints(&json!({
                "filepath": dir.path().join("Any.SchLib").to_string_lossy(),
                "operation": "list",
            }));
            assert!(result.is_error);
            assert_eq!(
                get_result_text(&result),
                "Missing required parameter: component_name"
            );
        }

        #[test]
        fn footprints_reject_a_non_schlib_extension() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let result = server.call_manage_schlib_footprints(&json!({
                "filepath": dir.path().join("Lib.PcbLib").to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "list",
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("only supports SchLib files"));
        }

        #[test]
        fn footprints_reject_a_path_outside_the_allowed_roots() {
            let dir = test_temp_dir();
            let other = test_temp_dir();
            let server = create_test_server(dir.path());
            let outside = other.path().join("Out.SchLib");
            create_test_schlib(&outside);

            let result = server.call_manage_schlib_footprints(&json!({
                "filepath": outside.to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "list",
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("Access denied"));
        }

        #[test]
        fn footprints_report_a_corrupt_library() {
            // `list` and the `add`/`remove` pair open the library on separate arms.
            let (_dir, server, filepath) = corrupt_library();

            for (operation, name) in [("list", None), ("add", Some("FP")), ("remove", Some("FP"))] {
                let mut args = json!({
                    "filepath": filepath,
                    "component_name": "RESISTOR",
                    "operation": operation,
                });
                if let Some(name) = name {
                    args["footprint_name"] = json!(name);
                }
                let result = server.call_manage_schlib_footprints(&args);
                assert!(
                    result.is_error,
                    "{operation} must fail on a corrupt library"
                );
                assert!(
                    get_result_text(&result).contains("Failed to read library"),
                    "{operation} must name the read failure, got: {}",
                    get_result_text(&result)
                );
            }
        }

        #[test]
        fn footprints_mutations_report_an_unknown_symbol() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("FpNoSym.SchLib");
            create_test_schlib(&path);
            let filepath = path.to_string_lossy().to_string();

            for operation in ["add", "remove"] {
                let result = server.call_manage_schlib_footprints(&json!({
                    "filepath": filepath,
                    "component_name": "NOPE",
                    "operation": operation,
                    "footprint_name": "SOIC-8",
                }));
                assert!(result.is_error, "{operation} must reject an unknown symbol");
                assert!(get_result_text(&result).contains("Component 'NOPE' not found in library"));
            }
        }

        #[test]
        fn footprints_add_rejects_a_duplicate_link() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("FpDup.SchLib");
            create_test_schlib(&path);
            let filepath = path.to_string_lossy().to_string();

            let args = json!({
                "filepath": filepath,
                "component_name": "RESISTOR",
                "operation": "add",
                "footprint_name": "SOIC-8",
            });
            assert!(!server.call_manage_schlib_footprints(&args).is_error);

            let result = server.call_manage_schlib_footprints(&args);
            assert!(result.is_error, "the second add must be rejected");
            assert!(get_result_text(&result).contains("already linked"));
        }
    }

    // ==================== optional parameter properties ======================
    //
    // `add` and `set` each carry their own copy of the optional-property block.
    // Every one of these is omit-when-default on write, so an ignored argument
    // is silent: the call reports success and the property simply is not there.

    mod optional_properties {
        use crate::altium::SchLib;
        use crate::mcp::tools::test_support::{
            create_test_schlib, create_test_server, get_result_text, parse_result_json,
            test_temp_dir,
        };
        use serde_json::json;

        #[test]
        fn add_and_set_both_apply_the_optional_properties() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Params.SchLib");
            create_test_schlib(&path);

            let added = server.call_manage_schlib_parameters(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "add",
                "parameter_name": "Tolerance",
                "value": "1%",
                "hidden": true,
                "read_only_state": 1,
                "param_type": 2,
                "unique_id": "ABCDEFGH",
            }));
            assert!(!added.is_error, "{}", get_result_text(&added));

            let read = || {
                SchLib::open(&path)
                    .unwrap()
                    .get("RESISTOR")
                    .unwrap()
                    .parameters
                    .iter()
                    .find(|p| p.name == "Tolerance")
                    .expect("the parameter should be there")
                    .clone()
            };

            let param = read();
            assert!(param.hidden);
            assert_eq!(param.read_only_state, 1);
            assert_eq!(param.param_type, 2);
            assert_eq!(param.unique_id.as_deref(), Some("ABCDEFGH"));

            // `set` carries its own copy of the same block, so changing them on
            // an existing parameter has to work too.
            let updated = server.call_manage_schlib_parameters(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "set",
                "parameter_name": "Tolerance",
                "value": "5%",
                "hidden": false,
                "read_only_state": 0,
                "param_type": 1,
                "unique_id": "HGFEDCBA",
            }));
            assert!(!updated.is_error, "{}", get_result_text(&updated));

            let param = read();
            assert_eq!(param.value, "5%");
            assert!(!param.hidden);
            assert_eq!(param.param_type, 1);
            assert_eq!(param.unique_id.as_deref(), Some("HGFEDCBA"));
        }

        #[test]
        fn deleting_a_parameter_leaves_the_other_records_where_they_were() {
            // Stored interleaved — A, pin, B, pin — deleting A must not move
            // B in front of the first pin: the record order is kept in step.
            use crate::altium::schlib::{Parameter, Pin, PinOrientation, SchPrimitiveKind, Symbol};
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Order.SchLib");
            let mut symbol = Symbol::new("U1");
            symbol.add_parameter(Parameter::new("A", "1"));
            symbol.add_pin(Pin::new("1", "IN", -10, 0, 10, PinOrientation::Right));
            symbol.add_parameter(Parameter::new("B", "2"));
            symbol.add_pin(Pin::new("2", "OUT", -10, -10, 10, PinOrientation::Right));
            let mut lib = SchLib::new();
            lib.add(symbol);
            lib.save(&path).expect("save");

            let deleted = server.call_manage_schlib_parameters(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "U1",
                "operation": "delete",
                "parameter_name": "a",
            }));
            assert!(!deleted.is_error, "{}", get_result_text(&deleted));

            let lib = SchLib::open(&path).expect("reopen");
            let symbol = lib.get("U1").expect("symbol");
            assert_eq!(symbol.parameters.len(), 1);
            assert_eq!(symbol.parameters[0].name, "B");
            assert_eq!(
                symbol.primitive_order,
                vec![
                    SchPrimitiveKind::Pin,
                    SchPrimitiveKind::Parameter,
                    SchPrimitiveKind::Pin,
                ]
            );
        }

        #[test]
        fn set_moves_a_parameter_and_get_reports_the_whole_record() {
            // `x`/`y` are accepted by `add`; `set` takes them the same way
            // rather than reporting success and leaving the parameter where
            // it was. `get` and `list` report the parameter in the shape
            // read_schlib uses, not a five-field excerpt.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Move.SchLib");
            create_test_schlib(&path);
            let added = server.call_manage_schlib_parameters(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "add",
                "parameter_name": "Value",
                "value": "10k",
            }));
            assert!(!added.is_error, "{}", get_result_text(&added));

            let moved = server.call_manage_schlib_parameters(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "set",
                "parameter_name": "Value",
                "x": 15,
                "y": -25,
            }));
            assert!(!moved.is_error, "{}", get_result_text(&moved));
            let got = server.call_manage_schlib_parameters(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "get",
                "parameter_name": "Value",
            }));
            let param = parse_result_json(&got)["parameter"].clone();
            assert_eq!(param["x"], 15.0, "{param}");
            assert_eq!(param["y"], -25.0, "{param}");
            assert!(param.get("read_only_state").is_some(), "{param}");
            assert!(param.get("font_id").is_some(), "{param}");

            let listed = server.call_manage_schlib_parameters(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "list",
            }));
            let listed = parse_result_json(&listed);
            assert!(
                listed["parameters"]
                    .as_array()
                    .expect("array")
                    .iter()
                    .all(|p| p.get("font_id").is_some()),
                "{listed}"
            );

            for axis in ["x", "y"] {
                let refused = server.call_manage_schlib_parameters(&json!({
                    "filepath": path.to_string_lossy(),
                    "component_name": "RESISTOR",
                    "operation": "set",
                    "parameter_name": "Value",
                    axis: 1.0e12,
                }));
                assert!(refused.is_error, "{axis}");
                assert!(get_result_text(&refused).contains(&format!("parameter {axis}")));
            }
        }

        #[test]
        fn add_refuses_a_duplicate_and_set_refuses_a_missing_one() {
            // The two operations are not interchangeable, and each rejection
            // names the one the caller should have used.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Dup.SchLib");
            create_test_schlib(&path);

            let add = |name: &str, value: &str| {
                server.call_manage_schlib_parameters(&json!({
                    "filepath": path.to_string_lossy(),
                    "component_name": "RESISTOR",
                    "operation": "add",
                    "parameter_name": name,
                    "value": value,
                }))
            };

            assert!(!add("Tolerance", "1%").is_error);

            let duplicate = add("Tolerance", "2%");
            assert!(duplicate.is_error);
            assert!(
                get_result_text(&duplicate).contains("Use 'set'"),
                "{}",
                get_result_text(&duplicate)
            );

            let missing = server.call_manage_schlib_parameters(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "set",
                "parameter_name": "NoSuchParameter",
                "value": "x",
            }));
            assert!(missing.is_error);
            assert!(
                get_result_text(&missing).contains("Use 'add'"),
                "{}",
                get_result_text(&missing)
            );

            // `add` needs a value; there is nothing to default it to.
            let no_value = server.call_manage_schlib_parameters(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "operation": "add",
                "parameter_name": "Another",
            }));
            assert!(no_value.is_error);
        }

        #[test]
        fn a_parameter_coordinate_past_the_safe_range_is_refused() {
            // Same reasoning as the write path: an out-of-range schematic
            // coordinate saturates on save rather than failing loudly.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Far.SchLib");
            create_test_schlib(&path);

            for axis in ["x", "y"] {
                let mut args = json!({
                    "filepath": path.to_string_lossy(),
                    "component_name": "RESISTOR",
                    "operation": "add",
                    "parameter_name": format!("Far{axis}"),
                    "value": "1",
                });
                args[axis] = json!(99_999.0);
                let r = server.call_manage_schlib_parameters(&args);
                assert!(r.is_error, "{axis}: {}", get_result_text(&r));
                assert!(
                    get_result_text(&r).contains("exceeds the maximum safe range"),
                    "{}",
                    get_result_text(&r)
                );
            }
        }
    }

    /// A parameter value the record cannot hold is refused by field, with
    /// the file untouched and no backup made.
    #[test]
    fn set_refuses_a_pipe_in_the_value_before_any_backup() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Pipe.SchLib");
        create_test_schlib(&path);

        let result = server.call_manage_schlib_parameters(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "RESISTOR",
            "operation": "add",
            "parameter_name": "Tolerance",
            "value": "1|2",
        }));
        assert!(result.is_error);
        let text = get_result_text(&result);
        assert!(
            text.contains("Symbol 'RESISTOR' parameters[].value contains '|'"),
            "{text}"
        );
        let backups = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "bak"))
            .count();
        assert_eq!(backups, 0, "no backup for a save that never happens");
    }
}
