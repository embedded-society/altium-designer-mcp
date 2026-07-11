//! Component copy/rename/cross-copy/merge/reorder tools. Split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

impl McpServer {
    /// Copies a component within an Altium library file.
    pub(crate) fn call_copy_component(&self, arguments: &Value) -> ToolCallResult {
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
        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

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
            Some("pcblib") => Self::copy_pcblib_component(
                filepath,
                source_name,
                target_name,
                description,
                dry_run,
            ),
            Some("schlib") => Self::copy_schlib_component(
                filepath,
                source_name,
                target_name,
                description,
                dry_run,
            ),
            Some(ext) => ToolCallResult::error(format!(
                "Unsupported file type: .{ext}. Use .PcbLib or .SchLib"
            )),
            None => ToolCallResult::error("File has no extension. Use .PcbLib or .SchLib"),
        }
    }

    /// Copies a footprint within a `PcbLib` file.
    pub(crate) fn copy_pcblib_component(
        filepath: &str,
        source_name: &str,
        target_name: &str,
        description: Option<&str>,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read the library
        let mut library = match PcbLib::open(filepath) {
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

        // If dry_run, return what would happen without writing
        if dry_run {
            let result = json!({
                "status": "dry_run",
                "filepath": filepath,
                "file_type": "PcbLib",
                "source_name": source_name,
                "target_name": target_name,
                "component_count_after": library.len(),
                "dry_run": true,
                "message": format!("Would copy '{}' to '{}'", source_name, target_name),
            });
            return ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap());
        }

        if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
            return resp;
        }

        let mut result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "PcbLib",
            "source_name": source_name,
            "target_name": target_name,
            "component_count": library.len(),
            "dry_run": false,
        });

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_pcblib(filepath) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Copies a symbol within a `SchLib` file.
    pub(crate) fn copy_schlib_component(
        filepath: &str,
        source_name: &str,
        target_name: &str,
        description: Option<&str>,
        dry_run: bool,
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
        library.add(new_symbol);

        // If dry_run, return what would happen without writing
        if dry_run {
            let result = json!({
                "status": "dry_run",
                "filepath": filepath,
                "file_type": "SchLib",
                "source_name": source_name,
                "target_name": target_name,
                "component_count_after": library.len(),
                "dry_run": true,
                "message": format!("Would copy '{}' to '{}'", source_name, target_name),
            });
            return ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap());
        }

        if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
            return resp;
        }

        let mut result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "SchLib",
            "source_name": source_name,
            "target_name": target_name,
            "component_count": library.len(),
            "dry_run": false,
        });

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_schlib(filepath) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    // ==================== Component Rename ====================

    /// Renames a component within an Altium library file.
    pub(crate) fn call_rename_component(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(old_name) = arguments.get("old_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: old_name");
        };

        let Some(new_name) = arguments.get("new_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: new_name");
        };

        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

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
            Some("pcblib") => Self::rename_pcblib_component(filepath, old_name, new_name, dry_run),
            Some("schlib") => Self::rename_schlib_component(filepath, old_name, new_name, dry_run),
            Some(ext) => ToolCallResult::error(format!(
                "Unsupported file type: .{ext}. Use .PcbLib or .SchLib"
            )),
            None => ToolCallResult::error("File has no extension. Use .PcbLib or .SchLib"),
        }
    }

    /// Renames a footprint within a `PcbLib` file.
    pub(crate) fn rename_pcblib_component(
        filepath: &str,
        old_name: &str,
        new_name: &str,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read the library
        let mut library = match PcbLib::open(filepath) {
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

        // If dry_run, return what would happen without writing
        if dry_run {
            let result = json!({
                "status": "dry_run",
                "filepath": filepath,
                "file_type": "PcbLib",
                "old_name": old_name,
                "new_name": new_name,
                "dry_run": true,
                "message": format!("Would rename '{}' to '{}'", old_name, new_name),
            });
            return ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap());
        }

        if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
            return resp;
        }

        let mut result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "PcbLib",
            "old_name": old_name,
            "new_name": new_name,
            "component_count": library.len(),
            "dry_run": false,
        });

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_pcblib(filepath) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Renames a symbol within a `SchLib` file.
    pub(crate) fn rename_schlib_component(
        filepath: &str,
        old_name: &str,
        new_name: &str,
        dry_run: bool,
    ) -> ToolCallResult {
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
        library.add(symbol);

        // If dry_run, return what would happen without writing
        if dry_run {
            let result = json!({
                "status": "dry_run",
                "filepath": filepath,
                "file_type": "SchLib",
                "old_name": old_name,
                "new_name": new_name,
                "dry_run": true,
                "message": format!("Would rename '{}' to '{}'", old_name, new_name),
            });
            return ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap());
        }

        if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
            return resp;
        }

        let mut result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "SchLib",
            "old_name": old_name,
            "new_name": new_name,
            "component_count": library.len(),
            "dry_run": false,
        });

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_schlib(filepath) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    // ==================== Cross-Library Copy ====================

    /// Copies a component from one Altium library to another.
    pub(crate) fn call_copy_component_cross_library(&self, arguments: &Value) -> ToolCallResult {
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
        let ignore_missing_models = arguments
            .get("ignore_missing_models")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let preserve_external_paths = arguments
            .get("preserve_external_paths")
            .and_then(Value::as_bool)
            .unwrap_or(false);

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
                ignore_missing_models,
                preserve_external_paths,
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
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub(crate) fn copy_pcblib_component_cross_library(
        source_filepath: &str,
        target_filepath: &str,
        component_name: &str,
        target_name: &str,
        description: Option<&str>,
        ignore_missing_models: bool,
        preserve_external_paths: bool,
    ) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read the source library
        let source_library = match PcbLib::open(source_filepath) {
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

        // Handle model_3d reference - the STEP file path is relative to the source library
        // and may not be valid in the target location.
        let had_model_3d = new_footprint.model_3d.is_some();
        let preserved_model_3d = if preserve_external_paths {
            // Keep the external path - user explicitly requested this
            new_footprint.model_3d.is_some()
        } else {
            new_footprint.model_3d.take();
            false
        };

        // Collect embedded model IDs referenced by this footprint and check availability
        let mut embedded_model_ids: Vec<String> = Vec::new();
        let mut missing_model_ids: Vec<String> = Vec::new();

        for cb in &new_footprint.component_bodies {
            if cb.embedded {
                if source_library.get_model(&cb.model_id).is_some() {
                    embedded_model_ids.push(cb.model_id.clone());
                } else {
                    missing_model_ids.push(cb.model_id.clone());
                }
            }
        }

        // Handle missing models
        if !missing_model_ids.is_empty() {
            if ignore_missing_models {
                // Remove component bodies that reference missing models
                new_footprint
                    .component_bodies
                    .retain(|cb| !cb.embedded || !missing_model_ids.contains(&cb.model_id));
            } else {
                return ToolCallResult::error(format!(
                    "Component '{}' references missing embedded model(s): {}. \
                     Use ignore_missing_models=true to copy without the 3D model references.",
                    component_name,
                    missing_model_ids.join(", ")
                ));
            }
        }

        // Read or create the target library
        let mut target_library = if std::path::Path::new(target_filepath).exists() {
            match PcbLib::open(target_filepath) {
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

        // Copy embedded 3D models from source to target library
        let mut models_copied = 0;
        for model_id in &embedded_model_ids {
            // We already verified these exist above
            let model = source_library.get_model(model_id).unwrap();
            // Only add if not already present in target
            if target_library.get_model(model_id).is_none() {
                target_library.add_model(model.clone());
                models_copied += 1;
            }
        }

        // Add the footprint to target library
        target_library.add(new_footprint);

        if let Err(resp) =
            Self::backup_then_save(target_filepath, || target_library.save(target_filepath))
        {
            return resp;
        }

        let mut result = json!({
            "status": "success",
            "source_filepath": source_filepath,
            "target_filepath": target_filepath,
            "file_type": "PcbLib",
            "component_name": component_name,
            "target_name": target_name,
            "target_component_count": target_library.len(),
            "embedded_models_copied": models_copied,
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

        // Collect warnings
        let mut warnings: Vec<String> = Vec::new();
        // Only warn about external 3D model removal if the component had no embedded models.
        // If embedded models exist, the model_3d field was just a convenience reference
        // populated from ComponentBody during reading, not a true external reference.
        if had_model_3d && !preserved_model_3d && embedded_model_ids.is_empty() {
            warnings.push(
                "External 3D model reference was removed (STEP file path not portable across libraries)".to_string()
            );
        }
        if preserved_model_3d {
            warnings.push(
                "External 3D model path was preserved - verify the path is valid in the target location".to_string()
            );
        }
        if !missing_model_ids.is_empty() {
            warnings.push(format!(
                "Removed {} component body reference(s) with missing embedded model(s): {}",
                missing_model_ids.len(),
                missing_model_ids.join(", ")
            ));
        }
        if !warnings.is_empty() {
            result["warnings"] = json!(warnings);
        }
        result["preserve_external_paths"] = json!(preserve_external_paths);

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_pcblib(target_filepath) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Copies a symbol from one `SchLib` to another.
    pub(crate) fn copy_schlib_component_cross_library(
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
        target_library.add(new_symbol);

        if let Err(resp) =
            Self::backup_then_save(target_filepath, || target_library.save(target_filepath))
        {
            return resp;
        }

        let mut result = json!({
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

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_schlib(target_filepath) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Merges multiple Altium libraries into a single library.
    pub(crate) fn call_merge_libraries(&self, arguments: &Value) -> ToolCallResult {
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

        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

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
                Self::merge_pcblib_libraries(&source_paths, target_filepath, on_duplicate, dry_run)
            }
            Some("schlib") => {
                Self::merge_schlib_libraries(&source_paths, target_filepath, on_duplicate, dry_run)
            }
            Some(ext) => ToolCallResult::error(format!(
                "Unsupported file type: .{ext}. Use .PcbLib or .SchLib"
            )),
            None => ToolCallResult::error("Files have no extension. Use .PcbLib or .SchLib"),
        }
    }

    /// Merges multiple `PcbLib` files into one.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn merge_pcblib_libraries(
        source_paths: &[&str],
        target_filepath: &str,
        on_duplicate: &str,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read or create target library
        let mut target_library = if std::path::Path::new(target_filepath).exists() {
            match PcbLib::open(target_filepath) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error(format!("Failed to read target library: {e}"))
                }
            }
        } else {
            PcbLib::new()
        };

        // For dry_run, we track names that "would be" added to detect duplicates
        let mut simulated_names: std::collections::HashSet<String> =
            target_library.names().into_iter().collect();

        let initial_count = target_library.len();
        let mut merged_count = 0;
        let mut skipped_count = 0;
        let mut renamed_count = 0;
        let mut source_details: Vec<Value> = Vec::new();

        for source_path in source_paths {
            let source_library = match PcbLib::open(source_path) {
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

            for footprint in source_library.iter() {
                let original_name = footprint.name.clone();
                let mut fp_to_add = footprint.clone();

                let name_exists = if dry_run {
                    simulated_names.contains(&original_name)
                } else {
                    target_library.get(&original_name).is_some()
                };

                if name_exists {
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
                            while (dry_run && simulated_names.contains(&new_name))
                                || (!dry_run && target_library.get(&new_name).is_some())
                            {
                                counter += 1;
                                new_name = format!("{original_name}_{counter}");
                            }
                            fp_to_add.name.clone_from(&new_name);
                            if dry_run {
                                simulated_names.insert(new_name);
                            }
                            source_renamed += 1;
                            renamed_count += 1;
                        }
                        _ => unreachable!(),
                    }
                }

                if dry_run {
                    simulated_names.insert(fp_to_add.name.clone());
                } else {
                    target_library.add(fp_to_add);
                }
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

        // Only write if not dry-run
        if !dry_run {
            if let Err(resp) =
                Self::backup_then_save(target_filepath, || target_library.save(target_filepath))
            {
                return resp;
            }
        }

        let final_count = if dry_run {
            simulated_names.len()
        } else {
            target_library.len()
        };

        let mut result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "dry_run": dry_run,
            "target_filepath": target_filepath,
            "file_type": "PcbLib",
            "sources_count": source_paths.len(),
            "initial_count": initial_count,
            "merged_count": merged_count,
            "skipped_count": skipped_count,
            "renamed_count": renamed_count,
            "final_count": final_count,
            "sources": source_details,
            "message": format!(
                "{} {} components from {} sources into '{}' (total: {})",
                if dry_run { "Would merge" } else { "Merged" },
                merged_count,
                source_paths.len(),
                target_filepath,
                final_count
            ),
        });

        // Run post-write validation (only if actual changes were made)
        if merged_count > 0 && !dry_run {
            if let Some(validation) = Self::post_write_validation_pcblib(target_filepath) {
                result["validation"] = validation;
            }
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Merges multiple `SchLib` files into one.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn merge_schlib_libraries(
        source_paths: &[&str],
        target_filepath: &str,
        on_duplicate: &str,
        dry_run: bool,
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

        // For dry_run, we track names that "would be" added to detect duplicates
        let mut simulated_names: std::collections::HashSet<String> =
            target_library.iter().map(|s| s.name.clone()).collect();

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
            let symbols: Vec<_> = source_library.iter().cloned().collect();

            for symbol in symbols {
                let original_name = symbol.name.clone();
                let mut sym_to_add = symbol;

                let name_exists = if dry_run {
                    simulated_names.contains(&original_name)
                } else {
                    target_library.get(&original_name).is_some()
                };

                if name_exists {
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
                            while (dry_run && simulated_names.contains(&new_name))
                                || (!dry_run && target_library.get(&new_name).is_some())
                            {
                                counter += 1;
                                new_name = format!("{original_name}_{counter}");
                            }
                            sym_to_add.name.clone_from(&new_name);
                            if dry_run {
                                simulated_names.insert(new_name);
                            }
                            source_renamed += 1;
                            renamed_count += 1;
                        }
                        _ => unreachable!(),
                    }
                }

                if dry_run {
                    simulated_names.insert(sym_to_add.name.clone());
                } else {
                    target_library.add(sym_to_add);
                }
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

        // Only write if not dry-run
        if !dry_run {
            if let Err(resp) =
                Self::backup_then_save(target_filepath, || target_library.save(target_filepath))
            {
                return resp;
            }
        }

        let final_count = if dry_run {
            simulated_names.len()
        } else {
            target_library.len()
        };

        let mut result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "dry_run": dry_run,
            "target_filepath": target_filepath,
            "file_type": "SchLib",
            "sources_count": source_paths.len(),
            "initial_count": initial_count,
            "merged_count": merged_count,
            "skipped_count": skipped_count,
            "renamed_count": renamed_count,
            "final_count": final_count,
            "sources": source_details,
            "message": format!(
                "{} {} components from {} sources into '{}' (total: {})",
                if dry_run { "Would merge" } else { "Merged" },
                merged_count,
                source_paths.len(),
                target_filepath,
                final_count
            ),
        });

        // Run post-write validation (only if actual changes were made)
        if merged_count > 0 && !dry_run {
            if let Some(validation) = Self::post_write_validation_schlib(target_filepath) {
                result["validation"] = validation;
            }
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Reorders components in a `PcbLib` file.
    ///
    /// Components are reordered to match the specified order. Components not in the
    /// order list are placed at the end in their original relative order. `SchLib` files
    /// do not support reordering (unordered storage).
    pub(crate) fn call_reorder_components(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(component_order) = arguments.get("component_order").and_then(Value::as_array)
        else {
            return ToolCallResult::error("Missing required parameter: component_order");
        };

        let order: Vec<&str> = component_order.iter().filter_map(Value::as_str).collect();

        if order.is_empty() {
            return ToolCallResult::error("component_order array is empty or contains no strings");
        }

        // Determine file type from extension
        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => Self::reorder_pcblib(filepath, &order),
            Some("schlib") => Self::reorder_schlib(filepath, &order),
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

    /// Reorders components in a `PcbLib` file.
    pub(crate) fn reorder_pcblib(filepath: &str, order: &[&str]) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read the library
        let mut library = match PcbLib::open(filepath) {
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

        let original_order = library.names();
        let component_count = library.len();

        // Perform the reordering
        let new_order = library.reorder(order);

        // Create backup before destructive operation
        if let Err(e) = Self::create_backup(filepath) {
            return ToolCallResult::error(e);
        }

        // Write the library back
        if let Err(e) = library.save(filepath) {
            let result = json!({
                "status": "error",
                "filepath": filepath,
                "error": format!("Failed to write library: {e}"),
            });
            return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
        }

        // Determine which components were not in the requested order
        let requested_set: std::collections::HashSet<&str> = order.iter().copied().collect();
        let not_found: Vec<&str> = order
            .iter()
            .filter(|name| !original_order.contains(&(**name).to_string()))
            .copied()
            .collect();
        let not_requested: Vec<String> = original_order
            .iter()
            .filter(|name| !requested_set.contains(name.as_str()))
            .cloned()
            .collect();

        let mut result = json!({
            "status": "success",
            "filepath": filepath,
            "component_count": component_count,
            "original_order": original_order,
            "new_order": new_order,
            "not_in_library": not_found,
            "appended_at_end": not_requested,
            "message": format!(
                "Reordered {} components in '{}'{}{}",
                component_count,
                filepath,
                if not_found.is_empty() {
                    String::new()
                } else {
                    format!(" ({} requested names not found)", not_found.len())
                },
                if not_requested.is_empty() {
                    String::new()
                } else {
                    format!(" ({} components appended at end)", not_requested.len())
                }
            ),
        });

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_pcblib(filepath) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Reorders components in a `SchLib` file.
    pub(crate) fn reorder_schlib(filepath: &str, order: &[&str]) -> ToolCallResult {
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

        let original_order = library.names();
        let component_count = library.len();

        // Perform the reordering
        let new_order = library.reorder(order);

        // Create backup before destructive operation
        if let Err(e) = Self::create_backup(filepath) {
            return ToolCallResult::error(e);
        }

        // Write the library back
        if let Err(e) = library.save(filepath) {
            let result = json!({
                "status": "error",
                "filepath": filepath,
                "error": format!("Failed to write library: {e}"),
            });
            return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
        }

        // Determine which components were not in the requested order
        let requested_set: std::collections::HashSet<&str> = order.iter().copied().collect();
        let not_found: Vec<&str> = order
            .iter()
            .filter(|name| !original_order.contains(&(**name).to_string()))
            .copied()
            .collect();
        let not_requested: Vec<String> = original_order
            .iter()
            .filter(|name| !requested_set.contains(name.as_str()))
            .cloned()
            .collect();

        let mut result = json!({
            "status": "success",
            "filepath": filepath,
            "component_count": component_count,
            "original_order": original_order,
            "new_order": new_order,
            "not_in_library": not_found,
            "appended_at_end": not_requested,
            "message": format!(
                "Reordered {} components in '{}'{}{}",
                component_count,
                filepath,
                if not_found.is_empty() {
                    String::new()
                } else {
                    format!(" ({} requested names not found)", not_found.len())
                },
                if not_requested.is_empty() {
                    String::new()
                } else {
                    format!(" ({} components appended at end)", not_requested.len())
                }
            ),
        });

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_schlib(filepath) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }
}

#[cfg(test)]
mod tests {

    use crate::altium::pcblib::{ComponentBody, EmbeddedModel, Footprint, Pad, PcbLib};
    use crate::altium::SchLib;
    use crate::mcp::tools::test_support::{
        create_test_pcblib, create_test_schlib, create_test_server, get_result_text,
        parse_result_json, test_temp_dir,
    };
    use serde_json::json;

    // ==================== copy_component ====================

    #[test]
    fn copy_component_missing_and_invalid_arguments() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Copy.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_copy_component(&json!({}));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: filepath"
        );

        // Invalid OLE character in the target name.
        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "CHIP_0402",
            "target_name": "BAD:NAME",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("invalid character"));

        // Unsupported extension.
        let txt = dir.path().join("x.txt");
        let result = server.call_copy_component(&json!({
            "filepath": txt.to_string_lossy(),
            "source_name": "A",
            "target_name": "B",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unsupported file type"));
    }

    #[test]
    fn copy_component_pcblib_success_and_duplicate_rejection() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Copy.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "CHIP_0402",
            "target_name": "CHIP_0402_COPY",
            "description": "copied part",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["file_type"], "PcbLib");
        assert_eq!(parsed["source_name"], "CHIP_0402");
        assert_eq!(parsed["target_name"], "CHIP_0402_COPY");
        assert_eq!(parsed["component_count"], 3);

        // The copy persisted with the source's pads and the new description.
        let lib = PcbLib::open(&path).unwrap();
        let copy = lib.get("CHIP_0402_COPY").unwrap();
        assert_eq!(copy.description, "copied part");
        assert_eq!(copy.pads.len(), 2);
        assert_eq!(copy.pads[0].designator, "1");

        // Copying onto an existing name is rejected.
        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "CHIP_0402",
            "target_name": "CHIP_0603",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("already exists"));

        // Copying a component that does not exist is rejected.
        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "NOPE",
            "target_name": "NOPE_COPY",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Source component 'NOPE' not found"));
    }

    #[test]
    fn copy_component_pcblib_dry_run_leaves_file_untouched() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("CopyDry.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "CHIP_0402",
            "target_name": "CHIP_0402_COPY",
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "dry_run");
        assert_eq!(parsed["dry_run"], true);
        assert_eq!(parsed["component_count_after"], 3);

        let lib = PcbLib::open(&path).unwrap();
        assert_eq!(lib.len(), 2);
        assert!(lib.get("CHIP_0402_COPY").is_none());
    }

    #[test]
    fn copy_component_schlib_success() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Copy.SchLib");
        create_test_schlib(&path);

        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "RESISTOR",
            "target_name": "RESISTOR_PRECISION",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["file_type"], "SchLib");
        assert_eq!(parsed["component_count"], 3);

        let lib = SchLib::open(&path).unwrap();
        let copy = lib.get("RESISTOR_PRECISION").unwrap();
        assert_eq!(copy.pins.len(), 2);
        assert_eq!(copy.designator, "R?");
    }

    // ==================== rename_component ====================

    #[test]
    fn rename_component_pcblib_success() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Rename.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_rename_component(&json!({
            "filepath": path.to_string_lossy(),
            "old_name": "CHIP_0402",
            "new_name": "RES_0402",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["old_name"], "CHIP_0402");
        assert_eq!(parsed["new_name"], "RES_0402");
        assert_eq!(parsed["component_count"], 2);

        let lib = PcbLib::open(&path).unwrap();
        assert!(lib.get("CHIP_0402").is_none());
        let renamed = lib.get("RES_0402").unwrap();
        assert_eq!(renamed.pads.len(), 2);
        assert_eq!(renamed.description, "0402 chip resistor");
    }

    #[test]
    fn rename_component_error_paths() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("RenameErr.PcbLib");
        create_test_pcblib(&path);
        let filepath = path.to_string_lossy().to_string();

        // No-op rename is rejected up front.
        let result = server.call_rename_component(&json!({
            "filepath": filepath,
            "old_name": "CHIP_0402",
            "new_name": "CHIP_0402",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("identical"));

        // Renaming onto an existing component is rejected.
        let result = server.call_rename_component(&json!({
            "filepath": filepath,
            "old_name": "CHIP_0402",
            "new_name": "CHIP_0603",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("already exists"));

        // Renaming a missing component is rejected.
        let result = server.call_rename_component(&json!({
            "filepath": filepath,
            "old_name": "NOPE",
            "new_name": "NEW",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("'NOPE' not found"));
    }

    #[test]
    fn rename_component_schlib_dry_run() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Rename.SchLib");
        create_test_schlib(&path);

        let result = server.call_rename_component(&json!({
            "filepath": path.to_string_lossy(),
            "old_name": "RESISTOR",
            "new_name": "RES_GENERIC",
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "dry_run");
        assert_eq!(parsed["file_type"], "SchLib");

        // Nothing was written.
        let lib = SchLib::open(&path).unwrap();
        assert!(lib.get("RESISTOR").is_some());
        assert!(lib.get("RES_GENERIC").is_none());
    }

    #[test]
    fn rename_component_schlib_success() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("RenameOk.SchLib");
        create_test_schlib(&path);

        let result = server.call_rename_component(&json!({
            "filepath": path.to_string_lossy(),
            "old_name": "CAPACITOR",
            "new_name": "CAP_GENERIC",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");

        let lib = SchLib::open(&path).unwrap();
        assert!(lib.get("CAPACITOR").is_none());
        assert_eq!(lib.get("CAP_GENERIC").unwrap().designator, "C?");
    }

    // ==================== copy_component_cross_library ====================

    #[test]
    fn cross_library_copy_pcblib_creates_target() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let source = dir.path().join("Source.PcbLib");
        create_test_pcblib(&source);
        let target = dir.path().join("Target.PcbLib");

        let result = server.call_copy_component_cross_library(&json!({
            "source_filepath": source.to_string_lossy(),
            "target_filepath": target.to_string_lossy(),
            "component_name": "CHIP_0402",
            "new_name": "CHIP_0402_IMPORTED",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["target_name"], "CHIP_0402_IMPORTED");
        assert_eq!(parsed["target_component_count"], 1);
        assert_eq!(parsed["embedded_models_copied"], 0);

        let lib = PcbLib::open(&target).unwrap();
        assert_eq!(lib.get("CHIP_0402_IMPORTED").unwrap().pads.len(), 2);

        // The source is untouched.
        let src = PcbLib::open(&source).unwrap();
        assert_eq!(src.len(), 2);
    }

    #[test]
    fn cross_library_copy_pcblib_copies_embedded_models() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        // Source library with an embedded model referenced by a body.
        let model_id = "{11111111-2222-3333-4444-555555555555}";
        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("QFN16");
        fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.3, 0.8));
        fp.add_component_body(ComponentBody::new(model_id, "QFN16.step"));
        lib.add(fp);
        lib.add_model(EmbeddedModel::new(
            model_id,
            "QFN16.step",
            b"ISO-10303-21; test model".to_vec(),
        ));
        let source = dir.path().join("Models.PcbLib");
        lib.save(&source).unwrap();
        let target = dir.path().join("ModelsTarget.PcbLib");

        let result = server.call_copy_component_cross_library(&json!({
            "source_filepath": source.to_string_lossy(),
            "target_filepath": target.to_string_lossy(),
            "component_name": "QFN16",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["embedded_models_copied"], 1);

        let out = PcbLib::open(&target).unwrap();
        assert!(out.get_model(model_id).is_some());
        assert_eq!(out.get("QFN16").unwrap().component_bodies.len(), 1);
    }

    #[test]
    fn cross_library_copy_error_paths() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let source = dir.path().join("XSource.PcbLib");
        create_test_pcblib(&source);
        let sch = dir.path().join("XTarget.SchLib");
        create_test_schlib(&sch);

        // Mismatched library types.
        let result = server.call_copy_component_cross_library(&json!({
            "source_filepath": source.to_string_lossy(),
            "target_filepath": sch.to_string_lossy(),
            "component_name": "CHIP_0402",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("must be the same type"));

        // Missing component.
        let target = dir.path().join("XTarget.PcbLib");
        let result = server.call_copy_component_cross_library(&json!({
            "source_filepath": source.to_string_lossy(),
            "target_filepath": target.to_string_lossy(),
            "component_name": "NOPE",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("'NOPE' not found in source library"));
    }

    #[test]
    fn cross_library_copy_schlib_success() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let source = dir.path().join("SSource.SchLib");
        create_test_schlib(&source);
        let target = dir.path().join("STarget.SchLib");

        let result = server.call_copy_component_cross_library(&json!({
            "source_filepath": source.to_string_lossy(),
            "target_filepath": target.to_string_lossy(),
            "component_name": "RESISTOR",
            "description": "imported resistor",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["file_type"], "SchLib");
        assert_eq!(parsed["target_component_count"], 1);

        let lib = SchLib::open(&target).unwrap();
        assert_eq!(
            lib.get("RESISTOR").unwrap().description,
            "imported resistor"
        );
    }

    // ==================== merge_libraries ====================

    #[test]
    fn merge_libraries_validates_arguments() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let a = dir.path().join("MA.PcbLib");
        create_test_pcblib(&a);
        let target = dir.path().join("MT.PcbLib");

        let result = server.call_merge_libraries(&json!({}));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: source_filepaths"
        );

        let result = server.call_merge_libraries(&json!({ "source_filepaths": [] }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("at least one path"));

        let result = server.call_merge_libraries(&json!({
            "source_filepaths": [a.to_string_lossy()],
            "target_filepath": target.to_string_lossy(),
            "on_duplicate": "explode",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("on_duplicate must be one of"));
    }

    #[test]
    fn merge_libraries_pcblib_duplicate_handling() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        // Both sources contain CHIP_0402/CHIP_0603.
        let a = dir.path().join("MergeA.PcbLib");
        let b = dir.path().join("MergeB.PcbLib");
        create_test_pcblib(&a);
        create_test_pcblib(&b);
        let target = dir.path().join("Merged.PcbLib");

        // Default on_duplicate=error fails on the clash.
        let result = server.call_merge_libraries(&json!({
            "source_filepaths": [a.to_string_lossy(), b.to_string_lossy()],
            "target_filepath": target.to_string_lossy(),
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Duplicate component name"));
        assert!(!target.exists(), "failed merge must not create the target");

        // on_duplicate=rename merges everything with suffixed names.
        let result = server.call_merge_libraries(&json!({
            "source_filepaths": [a.to_string_lossy(), b.to_string_lossy()],
            "target_filepath": target.to_string_lossy(),
            "on_duplicate": "rename",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["merged_count"], 4);
        assert_eq!(parsed["renamed_count"], 2);
        assert_eq!(parsed["skipped_count"], 0);
        assert_eq!(parsed["final_count"], 4);
        assert_eq!(parsed["sources"][1]["renamed"], 2);

        let lib = PcbLib::open(&target).unwrap();
        assert_eq!(lib.len(), 4);
        assert!(lib.get("CHIP_0402").is_some());
        assert!(lib.get("CHIP_0402_1").is_some());
    }

    #[test]
    fn merge_libraries_pcblib_skip_and_dry_run() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let a = dir.path().join("SkipA.PcbLib");
        let b = dir.path().join("SkipB.PcbLib");
        create_test_pcblib(&a);
        create_test_pcblib(&b);
        let target = dir.path().join("Skipped.PcbLib");

        // Dry run reports the plan without creating the target file.
        let result = server.call_merge_libraries(&json!({
            "source_filepaths": [a.to_string_lossy(), b.to_string_lossy()],
            "target_filepath": target.to_string_lossy(),
            "on_duplicate": "skip",
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "dry_run");
        assert_eq!(parsed["merged_count"], 2);
        assert_eq!(parsed["skipped_count"], 2);
        assert_eq!(parsed["final_count"], 2);
        assert!(!target.exists(), "dry run must not create the target");

        // Real merge with skip keeps the first occurrence only.
        let result = server.call_merge_libraries(&json!({
            "source_filepaths": [a.to_string_lossy(), b.to_string_lossy()],
            "target_filepath": target.to_string_lossy(),
            "on_duplicate": "skip",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let lib = PcbLib::open(&target).unwrap();
        assert_eq!(lib.len(), 2);
    }

    #[test]
    fn merge_libraries_schlib_success() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let a = dir.path().join("SMergeA.SchLib");
        create_test_schlib(&a);
        let target = dir.path().join("SMerged.SchLib");

        let result = server.call_merge_libraries(&json!({
            "source_filepaths": [a.to_string_lossy()],
            "target_filepath": target.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["file_type"], "SchLib");
        assert_eq!(parsed["merged_count"], 2);
        assert_eq!(parsed["final_count"], 2);

        let lib = SchLib::open(&target).unwrap();
        assert!(lib.get("RESISTOR").is_some());
        assert!(lib.get("CAPACITOR").is_some());
    }

    // ==================== reorder_components ====================

    #[test]
    fn reorder_components_pcblib_success() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Reorder.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_reorder_components(&json!({
            "filepath": path.to_string_lossy(),
            "component_order": ["CHIP_0603", "CHIP_0402"],
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["component_count"], 2);
        assert_eq!(parsed["original_order"], json!(["CHIP_0402", "CHIP_0603"]));
        assert_eq!(parsed["new_order"], json!(["CHIP_0603", "CHIP_0402"]));
        assert_eq!(parsed["not_in_library"], json!([]));
        assert_eq!(parsed["appended_at_end"], json!([]));

        let lib = PcbLib::open(&path).unwrap();
        assert_eq!(lib.names(), vec!["CHIP_0603", "CHIP_0402"]);
    }

    #[test]
    fn reorder_components_schlib_reports_unknown_and_appended() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Reorder.SchLib");
        create_test_schlib(&path);

        // Request contains an unknown name and omits CAPACITOR.
        let result = server.call_reorder_components(&json!({
            "filepath": path.to_string_lossy(),
            "component_order": ["GHOST", "RESISTOR"],
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["not_in_library"], json!(["GHOST"]));
        assert_eq!(parsed["appended_at_end"], json!(["CAPACITOR"]));
        assert_eq!(parsed["new_order"], json!(["RESISTOR", "CAPACITOR"]));
    }

    #[test]
    fn reorder_components_rejects_bad_arguments() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("ReorderBad.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_reorder_components(&json!({
            "filepath": path.to_string_lossy(),
        }));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: component_order"
        );

        let result = server.call_reorder_components(&json!({
            "filepath": path.to_string_lossy(),
            "component_order": [],
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("empty"));
    }
}
