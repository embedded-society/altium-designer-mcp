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

                // Create backup before destructive operation
                if let Err(e) = Self::create_backup(filepath) {
                    return ToolCallResult::error(e);
                }

                // Save the modified library
                if let Err(e) = library.save(filepath) {
                    return ToolCallResult::error(format!("Failed to save library: {e}"));
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

                // Create backup before destructive operation
                if let Err(e) = Self::create_backup(filepath) {
                    return ToolCallResult::error(e);
                }

                // Save the modified library
                if let Err(e) = library.save(filepath) {
                    return ToolCallResult::error(format!("Failed to save library: {e}"));
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
