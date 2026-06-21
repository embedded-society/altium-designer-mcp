//! STEP 3D model extraction tools, split from `server.rs`.

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

impl McpServer {
    // ==================== STEP Model Extraction ====================

    /// Extracts embedded STEP 3D models from a `PcbLib` file.
    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    pub(crate) fn call_extract_step_model(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::PcbLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate library path
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Get extraction mode (default: auto)
        let mode = arguments
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("auto");

        // Validate output path if provided
        let output_path = arguments.get("output_path").and_then(Value::as_str);
        if let Some(out_path) = output_path {
            if let Err(e) = self.validate_path(out_path) {
                return ToolCallResult::error(e);
            }
        }

        let model_identifier = arguments.get("model").and_then(Value::as_str);
        let footprint_name = arguments.get("footprint_name").and_then(Value::as_str);

        // Parse optional pagination parameters (for listing models)
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| usize::try_from(v).unwrap_or(usize::MAX));
        let offset = arguments
            .get("offset")
            .and_then(Value::as_u64)
            .map_or(0, |v| usize::try_from(v).unwrap_or(usize::MAX));

        // Read the library
        let library = match PcbLib::open(filepath) {
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
            // Check for external model references to provide better error context
            let external_refs: Vec<Value> = library
                .iter()
                .filter_map(|fp| {
                    fp.model_3d.as_ref().map(|m| {
                        json!({
                            "footprint": fp.name,
                            "filepath": m.filepath,
                        })
                    })
                })
                .collect();

            let component_body_refs: Vec<Value> = library
                .iter()
                .filter(|fp| !fp.component_bodies.is_empty())
                .map(|fp| {
                    json!({
                        "footprint": fp.name,
                        "body_count": fp.component_bodies.len(),
                        "model_ids": fp.component_bodies.iter().map(|cb| &cb.model_id).collect::<Vec<_>>(),
                    })
                })
                .collect();

            let mut result = json!({
                "status": "error",
                "filepath": filepath,
                "error": "No embedded 3D models found in this library.",
            });

            if !external_refs.is_empty() {
                result["note"] = json!("This library uses external STEP file references (not embedded). The model files are stored separately on disk.");
                result["external_model_references"] = json!(external_refs);
            }

            if !component_body_refs.is_empty() {
                result["component_body_references"] = json!(component_body_refs);
                result["note"] = json!("Component bodies reference model IDs, but the corresponding model data was not found in /Library/Models/. The models may have been removed or the library may be using external references.");
            }

            if external_refs.is_empty() && component_body_refs.is_empty() {
                result["note"] = json!("No 3D model references of any kind found in this library.");
            }

            return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
        }

        // Handle different modes
        match mode {
            "list" => {
                // Force list mode - always list models
                Self::list_step_models(filepath, &models, limit, offset)
            }
            "extract_all" => {
                // Extract all models to output directory
                self.extract_all_step_models(filepath, output_path, &models)
            }
            "extract_by_footprint" => {
                // Extract models used by a specific footprint
                let Some(fp_name) = footprint_name else {
                    return ToolCallResult::error(
                        "Missing required parameter 'footprint_name' for extract_by_footprint mode",
                    );
                };
                self.extract_step_by_footprint(filepath, output_path, &library, &models, fp_name)
            }
            _ => {
                // Default "auto" mode - original behaviour
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
                    // Multiple models, list them with pagination
                    return Self::list_step_models(filepath, &models, limit, offset);
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
        }
    }

    /// Lists embedded STEP models with pagination.
    pub(crate) fn list_step_models(
        filepath: &str,
        models: &[&crate::altium::pcblib::EmbeddedModel],
        limit: Option<usize>,
        offset: usize,
    ) -> ToolCallResult {
        let total_count = models.len();
        let model_list: Vec<Value> = models
            .iter()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .map(|m| {
                json!({
                    "id": m.id,
                    "name": m.name,
                    "size_bytes": m.data.len(),
                })
            })
            .collect();

        let returned_count = model_list.len();
        let has_more = offset + returned_count < total_count;

        let result = json!({
            "status": "list",
            "filepath": filepath,
            "total_count": total_count,
            "returned_count": returned_count,
            "offset": offset,
            "has_more": has_more,
            "models": model_list,
        });
        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Extracts all STEP models to an output directory.
    pub(crate) fn extract_all_step_models(
        &self,
        filepath: &str,
        output_path: Option<&str>,
        models: &[&crate::altium::pcblib::EmbeddedModel],
    ) -> ToolCallResult {
        let Some(output_dir) = output_path else {
            return ToolCallResult::error(
                "Missing required parameter 'output_path' (directory) for extract_all mode",
            );
        };

        // Create output directory if it doesn't exist
        let out_dir = std::path::Path::new(output_dir);
        if let Err(e) = std::fs::create_dir_all(out_dir) {
            return ToolCallResult::error(format!("Failed to create output directory: {e}"));
        }

        let mut extracted: Vec<Value> = Vec::new();
        let mut errors: Vec<Value> = Vec::new();

        for model in models {
            // model.name comes from inside the (caller-supplied) library. Reduce
            // it to a bare filename so a crafted name cannot use an absolute path
            // or ".." segments to escape out_dir via Path::join, then re-validate
            // the resolved path against the allow-list (defence in depth).
            let Some(safe_name) = std::path::Path::new(&model.name).file_name() else {
                errors.push(json!({
                    "id": model.id,
                    "name": model.name,
                    "error": "model name has no usable file component; skipped",
                }));
                continue;
            };
            let output_file = out_dir.join(safe_name);
            if let Err(e) = self.validate_path(&output_file.to_string_lossy()) {
                errors.push(json!({
                    "id": model.id,
                    "name": model.name,
                    "error": e,
                }));
                continue;
            }
            match std::fs::write(&output_file, &model.data) {
                Ok(()) => {
                    extracted.push(json!({
                        "id": model.id,
                        "name": model.name,
                        "output_path": output_file.to_string_lossy(),
                        "size_bytes": model.data.len(),
                    }));
                }
                Err(e) => {
                    errors.push(json!({
                        "id": model.id,
                        "name": model.name,
                        "error": e.to_string(),
                    }));
                }
            }
        }

        let result = json!({
            "status": if errors.is_empty() { "success" } else { "partial" },
            "filepath": filepath,
            "output_directory": output_dir,
            "total_models": models.len(),
            "extracted_count": extracted.len(),
            "error_count": errors.len(),
            "extracted": extracted,
            "errors": errors,
        });
        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Extracts STEP models used by a specific footprint.
    pub(crate) fn extract_step_by_footprint(
        &self,
        filepath: &str,
        output_path: Option<&str>,
        library: &crate::altium::PcbLib,
        models: &[&crate::altium::pcblib::EmbeddedModel],
        footprint_name: &str,
    ) -> ToolCallResult {
        // Find the footprint
        let Some(footprint) = library.get(footprint_name) else {
            let available: Vec<&str> = library.iter().map(|fp| fp.name.as_str()).collect();
            let result = json!({
                "status": "error",
                "filepath": filepath,
                "error": format!("Footprint '{}' not found", footprint_name),
                "available_footprints": available,
            });
            return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
        };

        // Collect model IDs used by this footprint
        let mut model_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        // From component bodies
        for cb in &footprint.component_bodies {
            model_ids.insert(cb.model_id.to_lowercase());
        }

        // From model_3d reference (if it points to an embedded model)
        if let Some(ref m3d) = footprint.model_3d {
            if !m3d.filepath.is_empty() {
                // Match by filename: m3d.filepath may carry path components,
                // whereas the embedded model name is a bare filename.
                let m3d_name = std::path::Path::new(&m3d.filepath)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(m3d.filepath.as_str());
                for model in models {
                    if model.name.eq_ignore_ascii_case(m3d_name) {
                        model_ids.insert(model.id.to_lowercase());
                    }
                }
            }
        }

        if model_ids.is_empty() {
            let result = json!({
                "status": "error",
                "filepath": filepath,
                "footprint": footprint_name,
                "error": "No 3D model references found for this footprint",
            });
            return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
        }

        // Find matching models
        let matching_models: Vec<&crate::altium::pcblib::EmbeddedModel> = models
            .iter()
            .filter(|m| {
                let id_lower = m.id.to_lowercase();
                let id_trimmed = id_lower.trim_matches(|c| c == '{' || c == '}');
                model_ids.contains(&id_lower)
                    || model_ids.contains(id_trimmed)
                    || model_ids
                        .iter()
                        .any(|mid| mid.trim_matches(|c| c == '{' || c == '}') == id_trimmed)
            })
            .copied()
            .collect();

        if matching_models.is_empty() {
            let result = json!({
                "status": "error",
                "filepath": filepath,
                "footprint": footprint_name,
                "error": "Referenced model IDs not found in embedded models",
                "referenced_ids": model_ids.iter().collect::<Vec<_>>(),
                "available_model_ids": models.iter().map(|m| &m.id).collect::<Vec<_>>(),
            });
            return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
        }

        // Extract or return the models
        if matching_models.len() == 1 {
            // Single model - use standard extraction
            Self::extract_model_output(filepath, output_path, matching_models[0])
        } else if let Some(out_dir) = output_path {
            // Multiple models - extract to directory
            self.extract_all_step_models(filepath, Some(out_dir), &matching_models)
        } else {
            // Multiple models, no output - return info
            let model_info: Vec<Value> = matching_models
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
                "footprint": footprint_name,
                "message": "Multiple models found for footprint. Specify 'output_path' to extract all.",
                "model_count": matching_models.len(),
                "models": model_info,
            });
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
        }
    }

    /// Helper to output extracted model data (to file or base64).
    pub(crate) fn extract_model_output(
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
}
