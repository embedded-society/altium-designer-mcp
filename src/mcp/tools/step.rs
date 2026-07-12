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
        // Output names already claimed in this extraction, lower-cased because
        // Windows file systems are case-insensitive. Colliding names get a
        // `-1`, `-2`, ... suffix instead of silently overwriting each other.
        let mut used_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        for model in models {
            // model.name comes from inside the (caller-supplied) library. Reduce
            // it to a bare filename so a crafted name cannot use an absolute path
            // or ".." segments to escape out_dir via Path::join, sanitise the
            // Windows-invalid characters write_pcblib also rejects (a raw `:`
            // would write an NTFS alternate data stream), then re-validate the
            // resolved path against the allow-list (defence in depth).
            let Some(safe_name) = std::path::Path::new(&model.name)
                .file_name()
                .and_then(|n| crate::util::sanitise_file_name(&n.to_string_lossy()))
            else {
                errors.push(json!({
                    "id": model.id,
                    "name": model.name,
                    "error": "model name has no usable file component; skipped",
                }));
                continue;
            };
            let unique_name = Self::deduplicate_file_name(&safe_name, &mut used_names);
            let output_file = out_dir.join(&unique_name);
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

    /// Returns `name` if unclaimed, otherwise the first `stem-N.ext`
    /// (`N` = 1, 2, ...) not yet in `used`, and records the result. Claims are
    /// tracked lower-cased because Windows file systems are case-insensitive —
    /// two models named `A.step` and `a.step` must not overwrite each other.
    fn deduplicate_file_name(name: &str, used: &mut std::collections::HashSet<String>) -> String {
        if used.insert(name.to_lowercase()) {
            return name.to_string();
        }
        let path = std::path::Path::new(name);
        let stem = path
            .file_stem()
            .map_or_else(|| name.to_string(), |s| s.to_string_lossy().into_owned());
        let ext = path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        for n in 1.. {
            let candidate = format!("{stem}-{n}{ext}");
            if used.insert(candidate.to_lowercase()) {
                return candidate;
            }
        }
        unreachable!("some suffix is always free")
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

        // Extract or return the models. `output_path` is ALWAYS a directory in
        // this mode — previously it meant a file path for exactly one match
        // and a directory for several, so the same call wrote to different
        // places depending on how many models the footprint happened to
        // reference. The footprint decides the match count, not the caller,
        // so the meaning of the argument must not depend on it.
        output_path.map_or_else(
            || {
                if matching_models.len() == 1 {
                    // Single model, no output path - return it inline as base64
                    Self::extract_model_output(filepath, None, matching_models[0])
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
            },
            |out_dir| self.extract_all_step_models(filepath, Some(out_dir), &matching_models),
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::altium::pcblib::{ComponentBody, EmbeddedModel, Footprint, Pad, PcbLib};
    use crate::mcp::tools::test_support::{
        create_test_pcblib, create_test_server, get_result_text, parse_result_json, test_temp_dir,
    };
    use std::collections::HashSet;

    #[test]
    fn deduplicate_file_name_appends_suffix_before_extension() {
        let mut used = HashSet::new();
        assert_eq!(
            McpServer::deduplicate_file_name("model.step", &mut used),
            "model.step"
        );
        assert_eq!(
            McpServer::deduplicate_file_name("model.step", &mut used),
            "model-1.step"
        );
        assert_eq!(
            McpServer::deduplicate_file_name("model.step", &mut used),
            "model-2.step"
        );
        // Case-insensitive: on Windows `MODEL.STEP` would overwrite
        // `model.step`, and `MODEL-1`/`MODEL-2` are claimed too, so the first
        // free suffix is -3.
        assert_eq!(
            McpServer::deduplicate_file_name("MODEL.STEP", &mut used),
            "MODEL-3.STEP"
        );
        // No extension: suffix goes at the end.
        assert_eq!(McpServer::deduplicate_file_name("raw", &mut used), "raw");
        assert_eq!(McpServer::deduplicate_file_name("raw", &mut used), "raw-1");
    }

    #[test]
    fn extract_all_sanitises_and_deduplicates_model_names() {
        // Probe scenario: two models sharing one name must both survive (no
        // silent overwrite), and a name with a colon must not reach the file
        // system raw (on NTFS `foo:bar.step` writes an alternate data stream).
        let temp = test_temp_dir();
        let server = McpServer::new(vec![temp.path().to_path_buf()]);
        let out_dir = temp.path().join("out");

        let m1 = EmbeddedModel::new("{ID-1}", "dup.step", b"first".to_vec());
        let m2 = EmbeddedModel::new("{ID-2}", "dup.step", b"second".to_vec());
        let m3 = EmbeddedModel::new("{ID-3}", "foo:bar.step", b"third".to_vec());
        let models = [&m1, &m2, &m3];

        let result =
            server.extract_all_step_models("lib.PcbLib", Some(&out_dir.to_string_lossy()), &models);
        assert!(!result.is_error, "extraction succeeds");

        assert_eq!(
            std::fs::read(out_dir.join("dup.step")).expect("first file"),
            b"first"
        );
        assert_eq!(
            std::fs::read(out_dir.join("dup-1.step")).expect("deduplicated second file"),
            b"second"
        );
        assert_eq!(
            std::fs::read(out_dir.join("foo_bar.step")).expect("sanitised third file"),
            b"third",
            "the colon must be replaced, not written as an NTFS stream"
        );
        // Exactly three files — nothing overwritten, no stray `foo` stub from
        // an alternate-data-stream write.
        let count = std::fs::read_dir(&out_dir).expect("read out dir").count();
        assert_eq!(count, 3, "all three models extracted as separate files");
    }

    const MODEL_A_ID: &str = "{11111111-1111-1111-1111-111111111111}";
    const MODEL_B_ID: &str = "{22222222-2222-2222-2222-222222222222}";
    const MODEL_A_DATA: &[u8] = b"ISO-10303-21; model A";
    const MODEL_B_DATA: &[u8] = b"ISO-10303-21; model B";

    /// Builds a library with two embedded models; `QFN16` references model A.
    fn create_model_pcblib(path: &std::path::Path) {
        let mut lib = PcbLib::new();

        let mut fp = Footprint::new("QFN16");
        fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.3, 0.8));
        fp.add_component_body(ComponentBody::new(MODEL_A_ID, "modelA.step"));
        lib.add(fp);

        let mut plain = Footprint::new("PLAIN");
        plain.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
        lib.add(plain);

        lib.add_model(EmbeddedModel::new(
            MODEL_A_ID,
            "modelA.step",
            MODEL_A_DATA.to_vec(),
        ));
        lib.add_model(EmbeddedModel::new(
            MODEL_B_ID,
            "modelB.step",
            MODEL_B_DATA.to_vec(),
        ));
        lib.save(path).expect("Failed to create model PcbLib");
    }

    #[test]
    fn extract_step_model_list_mode_paginates() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Models.PcbLib");
        create_model_pcblib(&path);

        let result = server.call_extract_step_model(&json!({
            "filepath": path.to_string_lossy(),
            "mode": "list",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "list");
        assert_eq!(parsed["total_count"], 2);
        assert_eq!(parsed["returned_count"], 2);
        assert_eq!(parsed["has_more"], false);
        let models = parsed["models"].as_array().unwrap();
        assert!(models.iter().any(|m| m["name"] == "modelA.step"));

        // Pagination with limit 1: one entry and more remaining.
        let result = server.call_extract_step_model(&json!({
            "filepath": path.to_string_lossy(),
            "mode": "list",
            "limit": 1,
        }));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["returned_count"], 1);
        assert_eq!(parsed["has_more"], true);
    }

    #[test]
    fn extract_step_model_by_name_returns_base64() {
        use base64::Engine as _;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("ByName.PcbLib");
        create_model_pcblib(&path);

        let result = server.call_extract_step_model(&json!({
            "filepath": path.to_string_lossy(),
            "model": "modelA.step",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["model_name"], "modelA.step");
        assert_eq!(parsed["encoding"], "base64");
        assert_eq!(
            parsed["size_bytes"].as_u64().unwrap(),
            MODEL_A_DATA.len() as u64
        );
        let decoded = BASE64_STANDARD
            .decode(parsed["data"].as_str().unwrap())
            .expect("valid base64");
        assert_eq!(decoded, MODEL_A_DATA);
    }

    #[test]
    fn extract_step_model_writes_to_output_path() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("ToFile.PcbLib");
        create_model_pcblib(&path);
        let out = dir.path().join("out.step");

        // Look up by GUID without braces to exercise the trimmed-id match.
        let result = server.call_extract_step_model(&json!({
            "filepath": path.to_string_lossy(),
            "model": "22222222-2222-2222-2222-222222222222",
            "output_path": out.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["model_name"], "modelB.step");
        assert_eq!(std::fs::read(&out).unwrap(), MODEL_B_DATA);
    }

    #[test]
    fn extract_step_model_extract_all_writes_every_model() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("All.PcbLib");
        create_model_pcblib(&path);
        let out_dir = dir.path().join("exported");

        let result = server.call_extract_step_model(&json!({
            "filepath": path.to_string_lossy(),
            "mode": "extract_all",
            "output_path": out_dir.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["total_models"], 2);
        assert_eq!(parsed["extracted_count"], 2);
        assert_eq!(parsed["error_count"], 0);
        assert_eq!(
            std::fs::read(out_dir.join("modelA.step")).unwrap(),
            MODEL_A_DATA
        );
        assert_eq!(
            std::fs::read(out_dir.join("modelB.step")).unwrap(),
            MODEL_B_DATA
        );
    }

    #[test]
    fn extract_step_model_extract_all_requires_output_path() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("AllErr.PcbLib");
        create_model_pcblib(&path);

        let result = server.call_extract_step_model(&json!({
            "filepath": path.to_string_lossy(),
            "mode": "extract_all",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("output_path"));
    }

    #[test]
    fn extract_step_model_by_footprint_returns_referenced_model() {
        use base64::Engine as _;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("ByFp.PcbLib");
        create_model_pcblib(&path);

        // Single referenced model without output_path → inline base64.
        let result = server.call_extract_step_model(&json!({
            "filepath": path.to_string_lossy(),
            "mode": "extract_by_footprint",
            "footprint_name": "QFN16",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["model_name"], "modelA.step");
        let decoded = BASE64_STANDARD
            .decode(parsed["data"].as_str().unwrap())
            .expect("valid base64");
        assert_eq!(decoded, MODEL_A_DATA);
    }

    #[test]
    fn extract_step_model_by_footprint_error_paths() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("ByFpErr.PcbLib");
        create_model_pcblib(&path);
        let filepath = path.to_string_lossy().to_string();

        // Mode requires footprint_name.
        let result = server.call_extract_step_model(&json!({
            "filepath": filepath,
            "mode": "extract_by_footprint",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("footprint_name"));

        // Unknown footprint lists the available ones.
        let result = server.call_extract_step_model(&json!({
            "filepath": filepath,
            "mode": "extract_by_footprint",
            "footprint_name": "GHOST",
        }));
        assert!(result.is_error);
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "error");
        assert!(parsed["available_footprints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "QFN16"));

        // Footprint with no 3D references.
        let result = server.call_extract_step_model(&json!({
            "filepath": filepath,
            "mode": "extract_by_footprint",
            "footprint_name": "PLAIN",
        }));
        assert!(result.is_error);
        let parsed = parse_result_json(&result);
        assert_eq!(
            parsed["error"],
            "No 3D model references found for this footprint"
        );
    }

    #[test]
    fn extract_step_model_unknown_identifier_lists_models() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Unknown.PcbLib");
        create_model_pcblib(&path);

        let result = server.call_extract_step_model(&json!({
            "filepath": path.to_string_lossy(),
            "model": "nonsense.step",
        }));
        assert!(result.is_error);
        let parsed = parse_result_json(&result);
        assert!(parsed["error"]
            .as_str()
            .unwrap()
            .contains("Model 'nonsense.step' not found"));
        assert_eq!(parsed["available_models"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn extract_step_model_no_embedded_models_is_an_error() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("NoModels.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_extract_step_model(&json!({
            "filepath": path.to_string_lossy(),
        }));
        assert!(result.is_error);
        let parsed = parse_result_json(&result);
        assert_eq!(
            parsed["error"],
            "No embedded 3D models found in this library."
        );
        assert_eq!(
            parsed["note"],
            "No 3D model references of any kind found in this library."
        );
    }

    #[test]
    fn extract_step_model_missing_filepath() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let result = server.call_extract_step_model(&json!({}));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: filepath"
        );
    }

    // ==================== extract deep paths ====================

    mod extract_deep {
        use super::*;

        /// A library whose footprint references a body model id with no matching
        /// embedded model (dangling reference; `library.models()` stays empty).
        fn lib_with_dangling_body(path: &std::path::Path, model_id: &str) {
            let mut lib = PcbLib::new();
            let mut fp = Footprint::new("QFN16");
            fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.3, 0.8));
            fp.add_component_body(ComponentBody::new(model_id, "ghost.step"));
            lib.add(fp);
            lib.save(path).unwrap();
        }

        #[test]
        fn extract_corrupt_library_is_error() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Corrupt.PcbLib");
            std::fs::write(&path, b"not a compound file").unwrap();
            let r = server.call_extract_step_model(&json!({ "filepath": path.to_string_lossy() }));
            assert!(r.is_error);
            assert_eq!(parse_result_json(&r)["status"], "error");
        }

        #[test]
        fn extract_reports_component_body_and_external_references() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Refs.PcbLib");
            lib_with_dangling_body(&path, "{DEAD0000-0000-0000-0000-000000000000}");

            let r = server.call_extract_step_model(&json!({ "filepath": path.to_string_lossy() }));
            assert!(r.is_error);
            let p = parse_result_json(&r);
            assert_eq!(p["error"], "No embedded 3D models found in this library.");
            assert!(p["component_body_references"].is_array());
            assert!(p["external_model_references"].is_array());
            assert!(p["note"]
                .as_str()
                .unwrap_or("")
                .contains("Component bodies reference"));
        }

        #[test]
        fn extract_all_validate_path_push_error() {
            let temp = test_temp_dir();
            let server = create_test_server(temp.path());
            let outside = test_temp_dir(); // not in the allow-list
            let out = outside.path().join("out");
            let out_s = out.to_string_lossy();

            let m = EmbeddedModel::new(
                "{ID000000-0000-0000-0000-000000000000}",
                "m.step",
                b"x".to_vec(),
            );
            let models = [&m];
            let r = server.extract_all_step_models("lib.PcbLib", Some(out_s.as_ref()), &models);
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "partial");
            assert_eq!(p["extracted_count"], 0);
            assert_eq!(p["error_count"], 1);
        }

        #[test]
        fn extract_all_fs_write_error_is_partial() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Models.PcbLib");
            create_model_pcblib(&path); // modelA.step + modelB.step
            let out_dir = dir.path().join("out");
            // A directory sitting where modelA.step should be written -> write fails.
            std::fs::create_dir_all(out_dir.join("modelA.step")).unwrap();

            let r = server.call_extract_step_model(&json!({
                "filepath": path.to_string_lossy(),
                "mode": "extract_all",
                "output_path": out_dir.to_string_lossy(),
            }));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "partial");
            assert!(p["error_count"].as_u64().unwrap() >= 1);
        }

        #[test]
        fn extract_by_footprint_referenced_ids_not_found() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Mismatch.PcbLib");
            let mut lib = PcbLib::new();
            let mut fp = Footprint::new("QFN16");
            fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.3, 0.8));
            fp.add_component_body(ComponentBody::new(
                "{NONEXIST-0000-0000-0000-000000000000}",
                "ghost.step",
            ));
            lib.add(fp);
            lib.add_model(EmbeddedModel::new(
                "{REAL0000-0000-0000-0000-000000000000}",
                "real.step",
                b"data".to_vec(),
            ));
            lib.save(&path).unwrap();

            let r = server.call_extract_step_model(&json!({
                "filepath": path.to_string_lossy(),
                "mode": "extract_by_footprint",
                "footprint_name": "QFN16",
            }));
            assert!(r.is_error);
            assert_eq!(
                parse_result_json(&r)["error"],
                "Referenced model IDs not found in embedded models"
            );
        }

        #[test]
        fn extract_by_footprint_multiple_models_lists() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Multi.PcbLib");
            let mut lib = PcbLib::new();
            let mut fp = Footprint::new("QFN16");
            fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.3, 0.8));
            fp.add_component_body(ComponentBody::new(MODEL_A_ID, "modelA.step"));
            fp.add_component_body(ComponentBody::new(MODEL_B_ID, "modelB.step"));
            lib.add(fp);
            lib.add_model(EmbeddedModel::new(
                MODEL_A_ID,
                "modelA.step",
                MODEL_A_DATA.to_vec(),
            ));
            lib.add_model(EmbeddedModel::new(
                MODEL_B_ID,
                "modelB.step",
                MODEL_B_DATA.to_vec(),
            ));
            lib.save(&path).unwrap();

            let r = server.call_extract_step_model(&json!({
                "filepath": path.to_string_lossy(),
                "mode": "extract_by_footprint",
                "footprint_name": "QFN16",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "list");
            assert_eq!(p["model_count"], 2);
        }

        #[test]
        fn extract_by_footprint_with_output_extracts() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("ByFp.PcbLib");
            create_model_pcblib(&path); // QFN16 -> modelA.step
            let out_dir = dir.path().join("fp_export");

            let r = server.call_extract_step_model(&json!({
                "filepath": path.to_string_lossy(),
                "mode": "extract_by_footprint",
                "footprint_name": "QFN16",
                "output_path": out_dir.to_string_lossy(),
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            assert_eq!(parse_result_json(&r)["status"], "success");
            assert_eq!(
                std::fs::read(out_dir.join("modelA.step")).unwrap(),
                MODEL_A_DATA
            );
        }

        #[test]
        fn extract_model_output_write_error() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Single.PcbLib");
            create_model_pcblib(&path);
            let out_as_dir = dir.path().join("outdir");
            std::fs::create_dir(&out_as_dir).unwrap(); // write target is a directory

            let r = server.call_extract_step_model(&json!({
                "filepath": path.to_string_lossy(),
                "model": "modelA.step",
                "output_path": out_as_dir.to_string_lossy(),
            }));
            assert!(r.is_error);
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "error");
            assert!(p["error"]
                .as_str()
                .unwrap_or("")
                .contains("Failed to write file"));
        }
    }
}
