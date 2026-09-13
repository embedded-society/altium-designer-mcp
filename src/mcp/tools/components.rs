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
            _ => ToolCallResult::error(super::unsupported_file_type(filepath)),
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
        if let Some(existing) = library.get(target_name) {
            return ToolCallResult::error(Self::taken_name_error(
                format!("Component '{target_name}' already exists in library"),
                target_name,
                &existing.name,
            ));
        }

        // Find the source component
        let Some(source) = library.get(source_name) else {
            return ToolCallResult::error(super::component_not_found(
                source_name,
                &library.names(),
            ));
        };

        // Clone the footprint with new name. The copy lives beside its source,
        // so it gets the identity of a new component rather than sharing the
        // original's GUIDs and unique ids.
        let mut new_footprint = source.clone();
        new_footprint.name = target_name.to_string();
        new_footprint.storage_name = None;
        new_footprint.reset_identities();
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

        if let Err(resp) = Self::backup_then_save(filepath, &mut library) {
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
        if let Some(existing) = library.get(target_name) {
            return ToolCallResult::error(Self::taken_name_error(
                format!("Component '{target_name}' already exists in library"),
                target_name,
                &existing.name,
            ));
        }

        // Find the source component
        let Some(source) = library.get(source_name) else {
            return ToolCallResult::error(super::component_not_found(
                source_name,
                &library.names(),
            ));
        };

        // Clone the symbol with new name. The copy lives beside its source, so
        // it gets fresh unique ids rather than sharing the original's.
        let mut new_symbol = source.clone();
        new_symbol.name = target_name.to_string();
        new_symbol.storage_name = None;
        new_symbol.reset_identities();
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

        if let Err(resp) = Self::backup_then_save(filepath, &mut library) {
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
            _ => ToolCallResult::error(super::unsupported_file_type(filepath)),
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

        // Check if new name already exists. The component's own name in
        // another case resolves to itself and is a legitimate rename.
        if let Some(existing) = library
            .get(new_name)
            .filter(|c| !crate::altium::same_name(&c.name, old_name))
        {
            return ToolCallResult::error(Self::taken_name_error(
                format!("Component '{new_name}' already exists in library"),
                new_name,
                &existing.name,
            ));
        }

        // Renamed in place: the component keeps its position in the library.
        if !library.rename(old_name, new_name) {
            return ToolCallResult::error(super::component_not_found(old_name, &library.names()));
        }

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

        if let Err(resp) = Self::backup_then_save(filepath, &mut library) {
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

        // Check if new name already exists. The component's own name in
        // another case resolves to itself and is a legitimate rename.
        if let Some(existing) = library
            .get(new_name)
            .filter(|c| !crate::altium::same_name(&c.name, old_name))
        {
            return ToolCallResult::error(Self::taken_name_error(
                format!("Component '{new_name}' already exists in library"),
                new_name,
                &existing.name,
            ));
        }

        // Renamed in place: the symbol keeps its position in the library.
        if !library.rename(old_name, new_name) {
            return ToolCallResult::error(super::component_not_found(old_name, &library.names()));
        }

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

        if let Err(resp) = Self::backup_then_save(filepath, &mut library) {
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

    /// Whether two paths name the same file. Resolved through the filesystem
    /// so `./Lib.PcbLib` and an absolute spelling compare equal; a path that
    /// cannot be resolved (it does not exist yet) is compared textually.
    fn same_file(a: &str, b: &str) -> bool {
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => a == b,
        }
    }

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
        // Source and target being one file makes this a copy within a library,
        // which copy_component handles — and gives the copy fresh identities,
        // which the cross-library path (the same component in another file)
        // deliberately does not.
        if Self::same_file(source_filepath, target_filepath) {
            return ToolCallResult::error(
                "source_filepath and target_filepath are the same file; use copy_component to duplicate a component within a library",
            );
        }

        // Validate the new name if provided; without one the copy keeps the
        // source's name as its library spells it.
        if let Some(new_name) = new_name {
            if let Err(e) = Self::validate_ole_name(new_name) {
                return ToolCallResult::error(e);
            }
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
                new_name,
                description,
                ignore_missing_models,
                preserve_external_paths,
            ),
            Some("schlib") => Self::copy_schlib_component_cross_library(
                source_filepath,
                target_filepath,
                component_name,
                new_name,
                description,
            ),
            _ => ToolCallResult::error(super::unsupported_file_type(source_filepath)),
        }
    }

    /// Copies a footprint from one `PcbLib` to another.
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub(crate) fn copy_pcblib_component_cross_library(
        source_filepath: &str,
        target_filepath: &str,
        component_name: &str,
        new_name: Option<&str>,
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
            return ToolCallResult::error(super::component_not_found_in(
                component_name,
                "source library",
                &source_library.names(),
            ));
        };

        // Clone the footprint under the new name, or the source's own as its
        // library spells it — not as the caller happened to spell it.
        let target_name = new_name.unwrap_or(&source.name);
        let mut new_footprint = source.clone();
        new_footprint.name = target_name.to_string();
        // The copy lives in another library: its storage is derived there.
        new_footprint.storage_name = None;
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
                // Drop the bodies that reference missing models, keeping every
                // other primitive where it was.
                new_footprint.retain_component_bodies(|cb| {
                    !cb.embedded || !missing_model_ids.contains(&cb.model_id)
                });
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
        if let Some(existing) = target_library.get(target_name) {
            return ToolCallResult::error(Self::taken_name_error(
                format!("Component '{target_name}' already exists in target library"),
                target_name,
                &existing.name,
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

        if let Err(resp) = Self::backup_then_save(target_filepath, &mut target_library) {
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
                if new_name.is_none() {
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
        new_name: Option<&str>,
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
            return ToolCallResult::error(super::component_not_found_in(
                component_name,
                "source library",
                &source_library.names(),
            ));
        };

        // Clone the symbol under the new name, or the source's own as its
        // library spells it — not as the caller happened to spell it.
        let target_name = new_name.unwrap_or(&source.name);
        let mut new_symbol = source.clone();
        new_symbol.name = target_name.to_string();
        // The copy lives in another library: its storage is derived there.
        new_symbol.storage_name = None;
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
        if let Some(existing) = target_library.get(target_name) {
            return ToolCallResult::error(Self::taken_name_error(
                format!("Component '{target_name}' already exists in target library"),
                target_name,
                &existing.name,
            ));
        }

        // Add the symbol to target library
        target_library.add(new_symbol);

        if let Err(resp) = Self::backup_then_save(target_filepath, &mut target_library) {
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
                if new_name.is_none() {
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
        // Merging a library into itself duplicates every component — with
        // on_duplicate=rename, as identity-sharing twins. Nothing sensible
        // comes of it, so refuse.
        if let Some(path) = source_paths
            .iter()
            .find(|p| Self::same_file(p, target_filepath))
        {
            return ToolCallResult::error(format!(
                "Source library '{path}' is the target library; a library cannot be merged into itself"
            ));
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
            _ => ToolCallResult::error(super::unsupported_file_type(source_paths[0])),
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

        // For dry_run, we track names that "would be" added to detect
        // duplicates — case-folded, as the library resolves them.
        let mut simulated_names: std::collections::HashSet<String> = target_library
            .names()
            .iter()
            .map(|name| crate::altium::folded_name(name))
            .collect();

        let initial_count = target_library.len();
        let mut merged_count = 0;
        let mut skipped_count = 0;
        let mut renamed_count = 0;
        let mut models_copied = 0;
        // Model ids copied (or, in a dry run, that would be copied) so far,
        // so a model shared by several footprints is counted once.
        let mut copied_model_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut warnings: Vec<String> = Vec::new();
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
            let mut source_models_copied = 0;

            for footprint in source_library.iter() {
                let original_name = footprint.name.clone();
                let mut fp_to_add = footprint.clone();

                let name_exists = if dry_run {
                    simulated_names.contains(&crate::altium::folded_name(&original_name))
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
                            while (dry_run
                                && simulated_names.contains(&crate::altium::folded_name(&new_name)))
                                || (!dry_run && target_library.get(&new_name).is_some())
                            {
                                counter += 1;
                                new_name = format!("{original_name}_{counter}");
                            }
                            fp_to_add.name.clone_from(&new_name);
                            if dry_run {
                                simulated_names.insert(crate::altium::folded_name(&new_name));
                            }
                            source_renamed += 1;
                            renamed_count += 1;
                        }
                        _ => unreachable!(),
                    }
                }

                // A footprint's embedded 3D bodies reference model streams
                // that live in the source library, not in the footprint
                // record; carrying the bodies without their models leaves
                // dangling references Altium cannot render. Copy each model
                // once (the id is a GUID, so a model already in the target is
                // the same model), and report a body whose model is missing
                // from the source too rather than quietly inventing fidelity.
                for body in fp_to_add.component_bodies.iter().filter(|b| b.embedded) {
                    match source_library.get_model(&body.model_id) {
                        Some(model) => {
                            let already_present = target_library.get_model(&body.model_id).is_some()
                                || copied_model_ids.contains(&body.model_id);
                            if !already_present {
                                if !dry_run {
                                    target_library.add_model(model.clone());
                                }
                                copied_model_ids.insert(body.model_id.clone());
                                source_models_copied += 1;
                            }
                        }
                        None => warnings.push(format!(
                            "'{}' from '{source_path}' references embedded model {} which the source library does not contain; the body was merged as-is",
                            fp_to_add.name, body.model_id
                        )),
                    }
                }

                if dry_run {
                    simulated_names.insert(crate::altium::folded_name(&fp_to_add.name));
                } else {
                    target_library.add(fp_to_add);
                }
                source_merged += 1;
                merged_count += 1;
            }

            models_copied += source_models_copied;
            source_details.push(json!({
                "source": source_path,
                "merged": source_merged,
                "skipped": source_skipped,
                "renamed": source_renamed,
                "embedded_models_copied": source_models_copied,
            }));
        }

        // Only write if not dry-run
        if !dry_run {
            if let Err(resp) = Self::backup_then_save(target_filepath, &mut target_library) {
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
            "embedded_models_copied": models_copied,
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
        if !warnings.is_empty() {
            result["warnings"] = json!(warnings);
        }

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

        // For dry_run, we track names that "would be" added to detect
        // duplicates — case-folded, as the library resolves them.
        let mut simulated_names: std::collections::HashSet<String> = target_library
            .iter()
            .map(|s| crate::altium::folded_name(&s.name))
            .collect();

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
                    simulated_names.contains(&crate::altium::folded_name(&original_name))
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
                            while (dry_run
                                && simulated_names.contains(&crate::altium::folded_name(&new_name)))
                                || (!dry_run && target_library.get(&new_name).is_some())
                            {
                                counter += 1;
                                new_name = format!("{original_name}_{counter}");
                            }
                            sym_to_add.name.clone_from(&new_name);
                            if dry_run {
                                simulated_names.insert(crate::altium::folded_name(&new_name));
                            }
                            source_renamed += 1;
                            renamed_count += 1;
                        }
                        _ => unreachable!(),
                    }
                }

                if dry_run {
                    simulated_names.insert(crate::altium::folded_name(&sym_to_add.name));
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
            if let Err(resp) = Self::backup_then_save(target_filepath, &mut target_library) {
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
                    "error": super::unsupported_file_type(filepath),
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

        // Determine which components were not in the requested order; a
        // name resolves the way the library resolves it, regardless of case.
        let requested_set: std::collections::HashSet<String> = order
            .iter()
            .map(|name| crate::altium::folded_name(name))
            .collect();
        let original_set: std::collections::HashSet<String> = original_order
            .iter()
            .map(|name| crate::altium::folded_name(name))
            .collect();
        let not_found: Vec<&str> = order
            .iter()
            .filter(|name| !original_set.contains(&crate::altium::folded_name(name)))
            .copied()
            .collect();
        let not_requested: Vec<String> = original_order
            .iter()
            .filter(|name| !requested_set.contains(&crate::altium::folded_name(name)))
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

        // Determine which components were not in the requested order; a
        // name resolves the way the library resolves it, regardless of case.
        let requested_set: std::collections::HashSet<String> = order
            .iter()
            .map(|name| crate::altium::folded_name(name))
            .collect();
        let original_set: std::collections::HashSet<String> = original_order
            .iter()
            .map(|name| crate::altium::folded_name(name))
            .collect();
        let not_found: Vec<&str> = order
            .iter()
            .filter(|name| !original_set.contains(&crate::altium::folded_name(name)))
            .copied()
            .collect();
        let not_requested: Vec<String> = original_order
            .iter()
            .filter(|name| !requested_set.contains(&crate::altium::folded_name(name)))
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

        // A control character in the target name.
        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "CHIP_0402",
            "target_name": "BAD\tNAME",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("control character"));

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
        assert!(get_result_text(&result).contains("Component 'NOPE' not found in library"));
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

    /// A copy living beside its source is a new component: it must not share
    /// the original's footprint GUID, primitive GUIDs, unique ids or pad
    /// identity GUIDs — the writer mints fresh ones, and the original keeps
    /// its own.
    #[test]
    fn copy_component_pcblib_gives_the_copy_fresh_identities() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let mut fp = Footprint::new("ORIG");
        fp.guid = Some("{11111111-2222-3333-4444-555555555555}".to_string());
        let mut pad = Pad::smd("1", -0.5, 0.0, 0.6, 0.5);
        pad.guid = Some("{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}".to_string());
        pad.unique_id = Some("ORIGPAD1".to_string());
        pad.identity_guid = Some("{A5172B29-10E4-C726-929A-64E441352E67}".to_string());
        fp.add_pad(pad);
        let mut lib = PcbLib::new();
        lib.add(fp);
        let path = dir.path().join("Ident.PcbLib");
        lib.save(&path).unwrap();

        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "ORIG",
            "target_name": "COPY",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));

        let lib = PcbLib::open(&path).unwrap();
        let orig = lib.get("ORIG").unwrap();
        let copy = lib.get("COPY").unwrap();
        assert_eq!(
            orig.guid.as_deref(),
            Some("{11111111-2222-3333-4444-555555555555}"),
            "the original keeps its identity"
        );
        assert_eq!(orig.pads[0].unique_id.as_deref(), Some("ORIGPAD1"));
        assert!(
            copy.guid.is_none(),
            "the copy has no inherited footprint identity"
        );
        assert!(
            copy.pads[0].guid.is_none(),
            "no inherited primitive identity"
        );
        assert_ne!(copy.pads[0].unique_id, orig.pads[0].unique_id);
        assert_ne!(
            copy.pads[0].identity_guid, orig.pads[0].identity_guid,
            "fresh pad identity GUID minted on write"
        );
        assert!((copy.pads[0].width - 0.6).abs() < 1e-4, "geometry copied");
    }

    /// The copy is a new component: it does not inherit the source's storage
    /// name, so it lands under the storage its own name derives (#507).
    #[test]
    fn copy_component_pcblib_gives_the_copy_its_own_storage() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let mut fp = Footprint::new("EC6*5.4");
        fp.storage_name = Some("LEGACY_STORAGE".to_string());
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        let mut lib = PcbLib::new();
        lib.add(fp);
        let path = dir.path().join("Storage.PcbLib");
        lib.save(&path).unwrap();

        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "EC6*5.4",
            "target_name": "EC7*5.4",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));

        let cfb = cfb::open(&path).unwrap();
        assert!(
            cfb.is_storage("/LEGACY_STORAGE"),
            "the source keeps its storage"
        );
        assert!(
            cfb.is_storage("/EC7_5.4"),
            "the copy derives Altium's form of its name"
        );
        drop(cfb);
        let lib = PcbLib::open(&path).unwrap();
        assert_eq!(
            lib.get("EC7*5.4").unwrap().storage_name.as_deref(),
            Some("EC7_5.4")
        );
    }

    /// The `SchLib` copy likewise gets fresh record unique ids.
    #[test]
    fn copy_component_schlib_gives_the_copy_fresh_identities() {
        use crate::altium::schlib::{Pin, PinOrientation, Rectangle, Symbol};

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let mut symbol = Symbol::new("ORIG");
        symbol.designator = "U?".to_string();
        symbol.designator_unique_id = Some("DESIGUID".to_string());
        let mut rect = Rectangle::new(0.0, 0.0, 20.0, 20.0);
        rect.unique_id = Some("RECTUID1".to_string());
        symbol.add_rectangle(rect);
        symbol.add_pin(Pin::new("IN", "1", 0, 0, 10, PinOrientation::Left));
        let mut lib = SchLib::new();
        lib.add(symbol);
        let path = dir.path().join("Ident.SchLib");
        lib.save(&path).unwrap();

        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "ORIG",
            "target_name": "COPY",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));

        let lib = SchLib::open(&path).unwrap();
        let orig = lib.get("ORIG").unwrap();
        let copy = lib.get("COPY").unwrap();
        assert_eq!(orig.rectangles[0].unique_id.as_deref(), Some("RECTUID1"));
        assert_eq!(orig.designator_unique_id.as_deref(), Some("DESIGUID"));
        assert!(
            copy.rectangles[0].unique_id.is_some(),
            "a fresh id was minted"
        );
        assert_ne!(copy.rectangles[0].unique_id, orig.rectangles[0].unique_id);
        assert_ne!(copy.designator_unique_id, orig.designator_unique_id);
        assert_eq!(copy.pins.len(), 1, "geometry copied");
    }

    /// A cross-library copy or merge whose source IS the target is refused —
    /// it would duplicate components as identity-sharing twins — and the check
    /// sees through a differently spelled path to the same file.
    #[test]
    fn cross_library_tools_refuse_a_source_that_is_the_target() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Self.PcbLib");
        create_test_pcblib(&path);
        // A differently spelled path to the same file.
        let spelled = dir.path().join(".").join("Self.PcbLib");

        let result = server.call_copy_component_cross_library(&json!({
            "source_filepath": path.to_string_lossy(),
            "target_filepath": spelled.to_string_lossy(),
            "component_name": "CHIP_0402",
            "new_name": "CHIP_0402_COPY",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("use copy_component"));

        let result = server.call_merge_libraries(&json!({
            "source_filepaths": [spelled.to_string_lossy()],
            "target_filepath": path.to_string_lossy(),
            "on_duplicate": "rename",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("cannot be merged into itself"));

        assert_eq!(PcbLib::open(&path).unwrap().len(), 2, "nothing changed");
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

    /// A merged footprint's embedded 3D bodies must bring their model streams
    /// along — a body whose model stayed behind in the source is a dangling
    /// reference Altium cannot render. A model shared by two footprints is
    /// copied (and counted) once, and a dry run reports the count without
    /// writing anything.
    #[test]
    fn merge_libraries_pcblib_copies_embedded_models() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let model_id = "{11111111-2222-3333-4444-555555555555}";
        let mut lib = PcbLib::new();
        for name in ["QFN16_A", "QFN16_B"] {
            let mut fp = Footprint::new(name);
            fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.3, 0.8));
            fp.add_component_body(ComponentBody::new(model_id, "QFN16.step"));
            lib.add(fp);
        }
        lib.add_model(EmbeddedModel::new(
            model_id,
            "QFN16.step",
            b"ISO-10303-21; test model".to_vec(),
        ));
        let source = dir.path().join("MergeModels.PcbLib");
        lib.save(&source).unwrap();
        let target = dir.path().join("MergeModelsTarget.PcbLib");

        let dry = server.call_merge_libraries(&json!({
            "source_filepaths": [source.to_string_lossy()],
            "target_filepath": target.to_string_lossy(),
            "dry_run": true,
        }));
        assert!(!dry.is_error, "{}", get_result_text(&dry));
        let parsed = parse_result_json(&dry);
        assert_eq!(
            parsed["embedded_models_copied"], 1,
            "shared model counted once"
        );
        assert!(!target.exists(), "dry run writes nothing");

        let result = server.call_merge_libraries(&json!({
            "source_filepaths": [source.to_string_lossy()],
            "target_filepath": target.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["merged_count"], 2);
        assert_eq!(parsed["embedded_models_copied"], 1);
        assert_eq!(parsed["sources"][0]["embedded_models_copied"], 1);
        assert!(parsed.get("warnings").is_none(), "nothing to warn about");

        let out = PcbLib::open(&target).unwrap();
        assert_eq!(out.models().count(), 1);
        for name in ["QFN16_A", "QFN16_B"] {
            let body = &out.get(name).unwrap().component_bodies[0];
            assert!(body.embedded);
            assert!(
                out.get_model(&body.model_id).is_some(),
                "{name}'s body resolves to a model in the target"
            );
        }
    }

    /// A body whose model is missing from the source too is merged as-is
    /// and reported, rather than silently dropped or invented.
    #[test]
    fn merge_libraries_pcblib_reports_a_model_missing_from_the_source() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("ORPHAN");
        fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.3, 0.8));
        fp.add_component_body(ComponentBody::new(
            "{99999999-8888-7777-6666-555555555555}",
            "gone.step",
        ));
        lib.add(fp);
        let source = dir.path().join("MergeOrphan.PcbLib");
        lib.save(&source).unwrap();
        let target = dir.path().join("MergeOrphanTarget.PcbLib");

        let result = server.call_merge_libraries(&json!({
            "source_filepaths": [source.to_string_lossy()],
            "target_filepath": target.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["embedded_models_copied"], 0);
        let warnings = parsed["warnings"].as_array().expect("warning emitted");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].as_str().unwrap().contains("ORPHAN"));
        assert!(warnings[0].as_str().unwrap().contains("merged as-is"));

        let out = PcbLib::open(&target).unwrap();
        assert_eq!(out.get("ORPHAN").unwrap().component_bodies.len(), 1);
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

    // ==================== dry-run, cross-library and merge deep paths ====================

    mod deep_coverage {
        use super::*;

        #[test]
        fn copy_component_schlib_dry_run() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("CopyDry.SchLib");
            create_test_schlib(&path);
            let r = server.call_copy_component(&json!({
                "filepath": path.to_string_lossy(),
                "source_name": "RESISTOR",
                "target_name": "RESISTOR_COPY",
                "dry_run": true,
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "dry_run");
            assert_eq!(p["file_type"], "SchLib");
            assert!(SchLib::open(&path).unwrap().get("RESISTOR_COPY").is_none());
        }

        #[test]
        fn rename_component_pcblib_dry_run() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("RenameDry.PcbLib");
            create_test_pcblib(&path);
            let r = server.call_rename_component(&json!({
                "filepath": path.to_string_lossy(),
                "old_name": "CHIP_0402",
                "new_name": "CHIP_0402_R",
                "dry_run": true,
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "dry_run");
            assert_eq!(p["file_type"], "PcbLib");
            let lib = PcbLib::open(&path).unwrap();
            assert!(lib.get("CHIP_0402").is_some());
            assert!(lib.get("CHIP_0402_R").is_none());
        }

        /// Builds a `PcbLib` whose footprint references an embedded model id that
        /// is NOT present in the library (a dangling reference).
        fn lib_with_missing_model(path: &std::path::Path, model_id: &str) {
            let mut lib = PcbLib::new();
            let mut fp = Footprint::new("QFN_MISSING");
            // Interleaved — body, pad, body, pad — so dropping the dangling
            // body is observable in the order the rest keeps.
            fp.add_component_body(ComponentBody::new(model_id, "missing.step"));
            fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.3, 0.8));
            let mut external = ComponentBody::new("", "kept.step");
            external.embedded = false;
            fp.add_component_body(external);
            fp.add_pad(Pad::smd("2", 1.0, 0.0, 0.3, 0.8));
            lib.add(fp);
            lib.save(path).unwrap();
        }

        #[test]
        fn cross_library_copy_pcblib_missing_model_ignored() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let source = dir.path().join("Missing.PcbLib");
            lib_with_missing_model(&source, "{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}");
            let target = dir.path().join("MissingTarget.PcbLib");

            let r = server.call_copy_component_cross_library(&json!({
                "source_filepath": source.to_string_lossy(),
                "target_filepath": target.to_string_lossy(),
                "component_name": "QFN_MISSING",
                "ignore_missing_models": true,
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "success");
            assert_eq!(p["embedded_models_copied"], 0);
            let warnings = p["warnings"].as_array().unwrap();
            assert!(warnings
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("component body")));
            let out = PcbLib::open(&target).unwrap();
            let copied = out.get("QFN_MISSING").unwrap();
            assert_eq!(
                copied.component_bodies.len(),
                1,
                "the external reference stays"
            );
            assert_eq!(copied.component_bodies[0].model_name, "kept.step");
            // The dangling body went; everything else kept its place: pad 1,
            // the kept body, pad 2 — not the kept body in front of pad 1.
            assert_eq!(
                copied.primitive_order,
                [
                    crate::altium::pcblib::PrimitiveKind::Pad,
                    crate::altium::pcblib::PrimitiveKind::ComponentBody,
                    crate::altium::pcblib::PrimitiveKind::Pad,
                ],
                "the copy keeps the source's order minus the dropped body"
            );
        }

        #[test]
        fn cross_library_copy_pcblib_missing_model_errors() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let source = dir.path().join("MissingErr.PcbLib");
            lib_with_missing_model(&source, "{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}");
            let r = server.call_copy_component_cross_library(&json!({
                "source_filepath": source.to_string_lossy(),
                "target_filepath": dir.path().join("Err.PcbLib").to_string_lossy(),
                "component_name": "QFN_MISSING",
            }));
            assert!(r.is_error);
            assert!(get_result_text(&r).contains("missing embedded model"));
        }

        #[test]
        fn cross_library_copy_pcblib_preserve_external_paths() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let model_id = "{11111111-2222-3333-4444-555555555555}";
            let mut lib = PcbLib::new();
            let mut fp = Footprint::new("QFN16");
            fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.3, 0.8));
            fp.add_component_body(ComponentBody::new(model_id, "QFN16.step"));
            lib.add(fp);
            lib.add_model(EmbeddedModel::new(
                model_id,
                "QFN16.step",
                b"ISO-10303-21; test".to_vec(),
            ));
            let source = dir.path().join("Preserve.PcbLib");
            lib.save(&source).unwrap();
            let target = dir.path().join("PreserveTarget.PcbLib");

            let r = server.call_copy_component_cross_library(&json!({
                "source_filepath": source.to_string_lossy(),
                "target_filepath": target.to_string_lossy(),
                "component_name": "QFN16",
                "preserve_external_paths": true,
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["preserve_external_paths"], true);
            assert_eq!(p["embedded_models_copied"], 1);
            assert!(p["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("preserved")));
        }

        #[test]
        fn cross_library_copy_pcblib_into_existing_target() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let source = dir.path().join("XSrc.PcbLib");
            create_test_pcblib(&source);
            let target = dir.path().join("XDst.PcbLib");
            create_test_pcblib(&target); // pre-existing target
            let r = server.call_copy_component_cross_library(&json!({
                "source_filepath": source.to_string_lossy(),
                "target_filepath": target.to_string_lossy(),
                "component_name": "CHIP_0402",
                "new_name": "CHIP_0402_IMPORTED",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            assert_eq!(parse_result_json(&r)["target_component_count"], 3);
        }

        #[test]
        fn cross_library_copy_schlib_into_existing_target() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let source = dir.path().join("SXSrc.SchLib");
            create_test_schlib(&source);
            let target = dir.path().join("SXDst.SchLib");
            create_test_schlib(&target);
            let r = server.call_copy_component_cross_library(&json!({
                "source_filepath": source.to_string_lossy(),
                "target_filepath": target.to_string_lossy(),
                "component_name": "RESISTOR",
                "new_name": "RESISTOR_IMPORTED",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["file_type"], "SchLib");
            assert_eq!(p["target_component_count"], 3);
        }

        #[test]
        fn merge_libraries_rejects_mixed_types() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let pcb = dir.path().join("Mix.PcbLib");
            create_test_pcblib(&pcb);
            let sch = dir.path().join("Mix.SchLib");
            create_test_schlib(&sch);

            let r = server.call_merge_libraries(&json!({
                "source_filepaths": [pcb.to_string_lossy(), sch.to_string_lossy()],
                "target_filepath": dir.path().join("Out.PcbLib").to_string_lossy(),
            }));
            assert!(r.is_error);
            assert!(get_result_text(&r).contains("same type"));

            let r = server.call_merge_libraries(&json!({
                "source_filepaths": [pcb.to_string_lossy()],
                "target_filepath": dir.path().join("Out.SchLib").to_string_lossy(),
            }));
            assert!(r.is_error);
            assert!(get_result_text(&r).contains("Target library type"));
        }

        #[test]
        fn merge_libraries_pcblib_rename_into_existing_target() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let target = dir.path().join("ExMerge.PcbLib");
            create_test_pcblib(&target);
            let a = dir.path().join("SA.PcbLib");
            create_test_pcblib(&a);
            let b = dir.path().join("SB.PcbLib");
            create_test_pcblib(&b);
            let r = server.call_merge_libraries(&json!({
                "source_filepaths": [a.to_string_lossy(), b.to_string_lossy()],
                "target_filepath": target.to_string_lossy(),
                "on_duplicate": "rename",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["renamed_count"], 4);
            assert_eq!(p["final_count"], 6);
            let lib = PcbLib::open(&target).unwrap();
            assert!(lib.get("CHIP_0402_1").is_some());
            assert!(lib.get("CHIP_0402_2").is_some()); // proves the counter loop ran
        }

        #[test]
        fn merge_libraries_schlib_rename_into_existing_target() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let target = dir.path().join("SEx.SchLib");
            create_test_schlib(&target);
            let a = dir.path().join("SSA.SchLib");
            create_test_schlib(&a);
            let b = dir.path().join("SSB.SchLib");
            create_test_schlib(&b);
            let r = server.call_merge_libraries(&json!({
                "source_filepaths": [a.to_string_lossy(), b.to_string_lossy()],
                "target_filepath": target.to_string_lossy(),
                "on_duplicate": "rename",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["file_type"], "SchLib");
            assert_eq!(p["renamed_count"], 4);
            assert_eq!(p["final_count"], 6);
            assert!(SchLib::open(&target).unwrap().get("RESISTOR_2").is_some());
        }

        #[test]
        fn merge_libraries_schlib_skip_and_dry_run() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let a = dir.path().join("SkA.SchLib");
            create_test_schlib(&a);
            let b = dir.path().join("SkB.SchLib");
            create_test_schlib(&b);
            let target = dir.path().join("Sk.SchLib");
            let r = server.call_merge_libraries(&json!({
                "source_filepaths": [a.to_string_lossy(), b.to_string_lossy()],
                "target_filepath": target.to_string_lossy(),
                "on_duplicate": "skip",
                "dry_run": true,
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "dry_run");
            assert_eq!(p["merged_count"], 2);
            assert_eq!(p["skipped_count"], 2);
            assert!(!target.exists());
        }

        #[test]
        fn reorder_components_pcblib_error_branches() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let txt = dir.path().join("thing.txt");
            let r = server.call_reorder_components(&json!({
                "filepath": txt.to_string_lossy(),
                "component_order": ["A"],
            }));
            assert!(r.is_error);
            assert!(get_result_text(&r).contains("Unsupported file type"));

            let ghost = dir.path().join("Ghost.PcbLib");
            let r = server.call_reorder_components(&json!({
                "filepath": ghost.to_string_lossy(),
                "component_order": ["A"],
            }));
            assert!(r.is_error);
            assert_eq!(parse_result_json(&r)["status"], "error");
        }

        #[test]
        fn reorder_components_schlib_open_error() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let ghost = dir.path().join("Ghost.SchLib");
            let r = server.call_reorder_components(&json!({
                "filepath": ghost.to_string_lossy(),
                "component_order": ["RESISTOR"],
            }));
            assert!(r.is_error);
            assert_eq!(parse_result_json(&r)["status"], "error");
        }
    }

    // ==================== rejection paths, both file types ===================
    //
    // Each of the six tools here dispatches on the file extension into a
    // PcbLib and a SchLib implementation that reject in the same places. The
    // fixtures above exercise the happy path and the PcbLib side; these cover
    // the argument guards, the dispatch arms, and every rejection on both
    // sides, plus the write failure each one funnels through.

    mod rejections {
        use crate::altium::{PcbLib, SchLib};
        use crate::mcp::tools::test_support::{
            create_test_pcblib, create_test_schlib, create_test_server, get_result_text,
            test_temp_dir,
        };
        use serde_json::json;
        use tempfile::TempDir;

        /// Bytes that are not an OLE compound file, so `open` fails.
        fn write_garbage(path: &std::path::Path) {
            std::fs::write(path, b"not an OLE compound document").unwrap();
        }

        /// Flips a file's read-only bit. A read-only library still opens and
        /// still backs up (both only read it), so the save is what fails —
        /// which is the branch each tool funnels its write errors through.
        /// Fails the library's next save — and ONLY the save — by occupying
        /// the deterministic temp path `save_atomic` must create beside the
        /// target (`<name>.pcblib.tmp` / `<name>.schlib.tmp`) with a
        /// directory: `File::create` over a directory fails on every platform,
        /// while the `.bak` backup (a plain copy) is untouched. Same mechanism
        /// as `BlockedSave` in `library_ops.rs`. Permissions cannot do this
        /// portably: a read-only FILE only blocks the rename-over on Windows
        /// (on Unix that permission belongs to the parent directory), and a
        /// read-only DIRECTORY fails the backup before the save is reached.
        fn block_save(path: &std::path::Path, blocked: bool) {
            let tmp_ext = if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("schlib"))
            {
                "schlib.tmp"
            } else {
                "pcblib.tmp"
            };
            let tmp = path.with_extension(tmp_ext);
            if blocked {
                std::fs::create_dir(&tmp).expect("occupy the save temp path");
            } else {
                let _ = std::fs::remove_dir(&tmp);
            }
        }

        /// A temp dir holding a populated library of each type, plus a corrupt
        /// one of each, so a test can pick whichever shape it needs.
        struct Fixtures {
            dir: TempDir,
        }

        impl Fixtures {
            fn new() -> Self {
                let dir = test_temp_dir();
                create_test_pcblib(&dir.path().join("Lib.PcbLib"));
                create_test_schlib(&dir.path().join("Lib.SchLib"));
                write_garbage(&dir.path().join("Bad.PcbLib"));
                write_garbage(&dir.path().join("Bad.SchLib"));
                Self { dir }
            }

            fn path(&self, name: &str) -> String {
                self.dir.path().join(name).to_string_lossy().into_owned()
            }
        }

        /// Asserts the call failed and its message mentions `needle`.
        fn assert_error_mentions(result: &crate::mcp::server::ToolCallResult, needle: &str) {
            let text = get_result_text(result);
            assert!(result.is_error, "expected an error, got: {text}");
            assert!(
                text.contains(needle),
                "expected the error to mention {needle:?}, got: {text}"
            );
        }

        // ---- copy_component ---------------------------------------------------

        #[test]
        fn copy_component_names_each_missing_argument_and_bad_extension() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            let missing_source = server.call_copy_component(&json!({
                "filepath": fx.path("Lib.PcbLib"), "target_name": "NEW",
            }));
            assert_error_mentions(&missing_source, "source_name");

            let missing_target = server.call_copy_component(&json!({
                "filepath": fx.path("Lib.PcbLib"), "source_name": "CHIP_0402",
            }));
            assert_error_mentions(&missing_target, "target_name");

            // A path with no extension cannot be dispatched at all.
            let no_ext = server.call_copy_component(&json!({
                "filepath": fx.path("Lib"), "source_name": "A", "target_name": "B",
            }));
            assert_error_mentions(&no_ext, "no file extension");

            let wrong_ext = server.call_copy_component(&json!({
                "filepath": fx.path("Lib.txt"), "source_name": "A", "target_name": "B",
            }));
            assert_error_mentions(&wrong_ext, "Unsupported file type");
        }

        #[test]
        fn copy_component_rejects_a_path_outside_the_allowlist() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());
            let r = server.call_copy_component(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
                "source_name": "A", "target_name": "B",
            }));
            assert!(r.is_error, "{}", get_result_text(&r));
        }

        #[test]
        fn copy_component_rejects_unreadable_libraries_of_both_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            for lib in ["Bad.PcbLib", "Bad.SchLib"] {
                let r = server.call_copy_component(&json!({
                    "filepath": fx.path(lib), "source_name": "A", "target_name": "B",
                }));
                assert_error_mentions(&r, "Failed to read library");
            }
        }

        #[test]
        fn copy_component_rejects_an_existing_target_and_a_missing_source() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            // SchLib side: the PcbLib equivalents are covered by the fixtures
            // above, but each implementation has its own copy of both guards.
            let exists = server.call_copy_component(&json!({
                "filepath": fx.path("Lib.SchLib"),
                "source_name": "RESISTOR", "target_name": "CAPACITOR",
            }));
            assert_error_mentions(&exists, "already exists");

            let missing = server.call_copy_component(&json!({
                "filepath": fx.path("Lib.SchLib"),
                "source_name": "GHOST", "target_name": "NEW",
            }));
            assert_error_mentions(&missing, "not found in library");
        }

        #[test]
        fn copy_component_applies_the_description_and_reports_a_failed_write() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let path = fx.path("Lib.SchLib");

            let copied = server.call_copy_component(&json!({
                "filepath": &path,
                "source_name": "RESISTOR", "target_name": "RESISTOR_2",
                "description": "a copy with its own description",
            }));
            assert!(!copied.is_error, "{}", get_result_text(&copied));
            let lib = SchLib::open(&path).unwrap();
            assert_eq!(
                lib.get("RESISTOR_2").unwrap().description,
                "a copy with its own description"
            );

            block_save(std::path::Path::new(&path), true);
            let blocked = server.call_copy_component(&json!({
                "filepath": &path, "source_name": "RESISTOR", "target_name": "RESISTOR_3",
            }));
            block_save(std::path::Path::new(&path), false);
            assert!(blocked.is_error, "{}", get_result_text(&blocked));
        }

        #[test]
        fn copy_component_pcblib_reports_a_failed_write() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let path = fx.path("Lib.PcbLib");

            block_save(std::path::Path::new(&path), true);
            let blocked = server.call_copy_component(&json!({
                "filepath": &path, "source_name": "CHIP_0402", "target_name": "CHIP_0402_COPY",
            }));
            block_save(std::path::Path::new(&path), false);
            assert!(blocked.is_error, "{}", get_result_text(&blocked));
        }

        // ---- rename_component -------------------------------------------------

        #[test]
        fn rename_component_names_each_missing_argument_and_bad_name() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());

            let no_old = server.call_rename_component(&json!({
                "filepath": fx.path("Lib.PcbLib"), "new_name": "NEW",
            }));
            assert_error_mentions(&no_old, "old_name");

            let no_new = server.call_rename_component(&json!({
                "filepath": fx.path("Lib.PcbLib"), "old_name": "CHIP_0402",
            }));
            assert_error_mentions(&no_new, "new_name");

            let escaped = server.call_rename_component(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
                "old_name": "A", "new_name": "B",
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));

            // A name cannot carry a control character.
            let bad_name = server.call_rename_component(&json!({
                "filepath": fx.path("Lib.PcbLib"), "old_name": "CHIP_0402", "new_name": "A\tB",
            }));
            assert_error_mentions(&bad_name, "control character");

            let no_ext = server.call_rename_component(&json!({
                "filepath": fx.path("Lib"), "old_name": "A", "new_name": "B",
            }));
            assert_error_mentions(&no_ext, "no file extension");

            let wrong_ext = server.call_rename_component(&json!({
                "filepath": fx.path("Lib.txt"), "old_name": "A", "new_name": "B",
            }));
            assert_error_mentions(&wrong_ext, "Unsupported file type");
        }

        #[test]
        fn rename_component_rejects_unreadable_libraries_of_both_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            for lib in ["Bad.PcbLib", "Bad.SchLib"] {
                let r = server.call_rename_component(&json!({
                    "filepath": fx.path(lib), "old_name": "A", "new_name": "B",
                }));
                assert_error_mentions(&r, "Failed to read library");
            }
        }

        #[test]
        fn rename_component_schlib_rejects_a_taken_name_and_a_missing_source() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            let taken = server.call_rename_component(&json!({
                "filepath": fx.path("Lib.SchLib"), "old_name": "RESISTOR", "new_name": "CAPACITOR",
            }));
            assert_error_mentions(&taken, "already exists");

            let missing = server.call_rename_component(&json!({
                "filepath": fx.path("Lib.SchLib"), "old_name": "GHOST", "new_name": "NEW",
            }));
            assert_error_mentions(&missing, "not found in library");
        }

        #[test]
        fn rename_component_reports_a_failed_write_for_both_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for (lib, old, new) in [
                ("Lib.PcbLib", "CHIP_0402", "CHIP_0402_R"),
                ("Lib.SchLib", "RESISTOR", "RESISTOR_R"),
            ] {
                let path = fx.path(lib);
                block_save(std::path::Path::new(&path), true);
                let r = server.call_rename_component(&json!({
                    "filepath": &path, "old_name": old, "new_name": new,
                }));
                block_save(std::path::Path::new(&path), false);
                assert!(r.is_error, "{}", get_result_text(&r));
            }
        }

        // ---- copy_component_cross_library -------------------------------------

        #[test]
        fn cross_library_copy_names_each_missing_argument_and_bad_dispatch() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());

            let no_target = server.call_copy_component_cross_library(&json!({
                "source_filepath": fx.path("Lib.PcbLib"), "component_name": "CHIP_0402",
            }));
            assert_error_mentions(&no_target, "target_filepath");

            let no_component = server.call_copy_component_cross_library(&json!({
                "source_filepath": fx.path("Lib.PcbLib"), "target_filepath": fx.path("T.PcbLib"),
            }));
            assert_error_mentions(&no_component, "component_name");

            // Both paths are read or written, so each is gated separately.
            let escaped_source = server.call_copy_component_cross_library(&json!({
                "source_filepath": outside.path().join("S.PcbLib").to_string_lossy(),
                "target_filepath": fx.path("T.PcbLib"), "component_name": "A",
            }));
            assert!(escaped_source.is_error);

            let escaped_target = server.call_copy_component_cross_library(&json!({
                "source_filepath": fx.path("Lib.PcbLib"),
                "target_filepath": outside.path().join("T.PcbLib").to_string_lossy(),
                "component_name": "A",
            }));
            assert!(escaped_target.is_error);

            let bad_name = server.call_copy_component_cross_library(&json!({
                "source_filepath": fx.path("Lib.PcbLib"), "target_filepath": fx.path("T.PcbLib"),
                "component_name": "CHIP_0402", "new_name": "A\tB",
            }));
            assert_error_mentions(&bad_name, "control character");

            // Copying between a footprint library and a symbol library is not
            // a conversion this tool performs.
            let mixed = server.call_copy_component_cross_library(&json!({
                "source_filepath": fx.path("Lib.PcbLib"), "target_filepath": fx.path("T.SchLib"),
                "component_name": "CHIP_0402",
            }));
            assert_error_mentions(&mixed, "must be the same type");

            let no_ext = server.call_copy_component_cross_library(&json!({
                "source_filepath": fx.path("S"), "target_filepath": fx.path("T"),
                "component_name": "A",
            }));
            assert_error_mentions(&no_ext, "no file extension");

            let wrong_ext = server.call_copy_component_cross_library(&json!({
                "source_filepath": fx.path("S.txt"), "target_filepath": fx.path("T.txt"),
                "component_name": "A",
            }));
            assert_error_mentions(&wrong_ext, "Unsupported file type");
        }

        #[test]
        fn cross_library_copy_reports_unreadable_source_and_target() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for (source, target, component) in [
                ("Bad.PcbLib", "T.PcbLib", "A"),
                ("Bad.SchLib", "T.SchLib", "A"),
            ] {
                let r = server.call_copy_component_cross_library(&json!({
                    "source_filepath": fx.path(source), "target_filepath": fx.path(target),
                    "component_name": component,
                }));
                assert_error_mentions(&r, "Failed to read source library");
            }

            // An existing but corrupt target is a separate failure from a
            // missing one, which is simply created.
            for (source, component) in [("Lib.PcbLib", "CHIP_0402"), ("Lib.SchLib", "RESISTOR")] {
                let target = if source.ends_with("PcbLib") {
                    "Bad.PcbLib"
                } else {
                    "Bad.SchLib"
                };
                let r = server.call_copy_component_cross_library(&json!({
                    "source_filepath": fx.path(source), "target_filepath": fx.path(target),
                    "component_name": component,
                }));
                assert_error_mentions(&r, "Failed to read target library");
            }
        }

        #[test]
        fn cross_library_copy_reports_a_missing_component_and_a_taken_target_name() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for (source, target) in [("Lib.PcbLib", "T1.PcbLib"), ("Lib.SchLib", "T1.SchLib")] {
                let r = server.call_copy_component_cross_library(&json!({
                    "source_filepath": fx.path(source), "target_filepath": fx.path(target),
                    "component_name": "GHOST",
                }));
                assert_error_mentions(&r, "not found in source library");
            }

            // Copying the same component twice: the second lands on a name the
            // target already holds.
            for (source, target, component) in [
                ("Lib.PcbLib", "T2.PcbLib", "CHIP_0402"),
                ("Lib.SchLib", "T2.SchLib", "RESISTOR"),
            ] {
                let args = json!({
                    "source_filepath": fx.path(source), "target_filepath": fx.path(target),
                    "component_name": component,
                });
                let first = server.call_copy_component_cross_library(&args);
                assert!(!first.is_error, "{}", get_result_text(&first));
                let second = server.call_copy_component_cross_library(&args);
                assert_error_mentions(&second, "already exists in target library");
            }
        }

        #[test]
        fn cross_library_copy_reports_a_failed_write_for_both_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for (source, target, component) in [
                ("Lib.PcbLib", "Locked.PcbLib", "CHIP_0402"),
                ("Lib.SchLib", "Locked.SchLib", "RESISTOR"),
            ] {
                // Seed the target so it exists, then lock it.
                let target_path = fx.path(target);
                if target.ends_with("PcbLib") {
                    PcbLib::new().save(&target_path).unwrap();
                } else {
                    SchLib::new().save(&target_path).unwrap();
                }
                block_save(std::path::Path::new(&target_path), true);
                let r = server.call_copy_component_cross_library(&json!({
                    "source_filepath": fx.path(source), "target_filepath": &target_path,
                    "component_name": component,
                }));
                block_save(std::path::Path::new(&target_path), false);
                assert!(r.is_error, "{}", get_result_text(&r));
            }
        }

        // ---- merge_libraries ---------------------------------------------------

        #[test]
        fn merge_names_each_missing_argument_and_rejects_a_bad_duplicate_policy() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());

            let no_sources = server.call_merge_libraries(&json!({
                "target_filepath": fx.path("M.PcbLib"),
            }));
            assert_error_mentions(&no_sources, "source_filepaths");

            // Present but carrying nothing usable, so the emptiness check is
            // what rejects it rather than the missing-parameter guard.
            let empty_sources = server.call_merge_libraries(&json!({
                "source_filepaths": [], "target_filepath": fx.path("M.PcbLib"),
            }));
            assert_error_mentions(&empty_sources, "at least one path");

            let no_target = server.call_merge_libraries(&json!({
                "source_filepaths": [fx.path("Lib.PcbLib")],
            }));
            assert_error_mentions(&no_target, "target_filepath");

            let bad_policy = server.call_merge_libraries(&json!({
                "source_filepaths": [fx.path("Lib.PcbLib")],
                "target_filepath": fx.path("M.PcbLib"), "on_duplicate": "overwrite",
            }));
            assert_error_mentions(&bad_policy, "must be one of");

            let escaped_source = server.call_merge_libraries(&json!({
                "source_filepaths": [outside.path().join("S.PcbLib").to_string_lossy()],
                "target_filepath": fx.path("M.PcbLib"),
            }));
            assert!(escaped_source.is_error);

            let escaped_target = server.call_merge_libraries(&json!({
                "source_filepaths": [fx.path("Lib.PcbLib")],
                "target_filepath": outside.path().join("M.PcbLib").to_string_lossy(),
            }));
            assert!(escaped_target.is_error);
        }

        #[test]
        fn merge_requires_one_library_type_throughout() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            // Sources disagreeing with each other is reported against the
            // first source, naming the offender.
            let mixed_sources = server.call_merge_libraries(&json!({
                "source_filepaths": [fx.path("Lib.PcbLib"), fx.path("Lib.SchLib")],
                "target_filepath": fx.path("M.PcbLib"),
            }));
            assert_error_mentions(&mixed_sources, "must be the same type");

            // Sources agreeing but the target disagreeing is a separate check.
            let mixed_target = server.call_merge_libraries(&json!({
                "source_filepaths": [fx.path("Lib.PcbLib")],
                "target_filepath": fx.path("M.SchLib"),
            }));
            assert_error_mentions(&mixed_target, "must match source libraries");

            let no_ext = server.call_merge_libraries(&json!({
                "source_filepaths": [fx.path("S")], "target_filepath": fx.path("M"),
            }));
            assert_error_mentions(&no_ext, "no file extension");

            let wrong_ext = server.call_merge_libraries(&json!({
                "source_filepaths": [fx.path("S.txt")], "target_filepath": fx.path("M.txt"),
            }));
            assert_error_mentions(&wrong_ext, "Unsupported file type");
        }

        #[test]
        fn merge_reports_unreadable_sources_and_targets_of_both_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for (source, target) in [("Lib.PcbLib", "Bad.PcbLib"), ("Lib.SchLib", "Bad.SchLib")] {
                let r = server.call_merge_libraries(&json!({
                    "source_filepaths": [fx.path(source)], "target_filepath": fx.path(target),
                }));
                assert_error_mentions(&r, "Failed to read target library");
            }

            for (source, target) in [("Bad.PcbLib", "MA.PcbLib"), ("Bad.SchLib", "MA.SchLib")] {
                let r = server.call_merge_libraries(&json!({
                    "source_filepaths": [fx.path(source)], "target_filepath": fx.path(target),
                }));
                assert_error_mentions(&r, "Failed to read source library");
            }
        }

        #[test]
        fn merge_renames_a_duplicate_rather_than_dropping_it() {
            // Merging a library into itself makes every name a duplicate, so
            // the rename policy has to invent a free name for each one — and
            // the dry run must simulate that without touching the target.
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for (source, target) in [("Lib.PcbLib", "R.PcbLib"), ("Lib.SchLib", "R.SchLib")] {
                let args = json!({
                    "source_filepaths": [fx.path(source), fx.path(source)],
                    "target_filepath": fx.path(target),
                    "on_duplicate": "rename",
                });

                let dry = server.call_merge_libraries(&json!({
                    "source_filepaths": [fx.path(source), fx.path(source)],
                    "target_filepath": fx.path(target),
                    "on_duplicate": "rename",
                    "dry_run": true,
                }));
                assert!(!dry.is_error, "{}", get_result_text(&dry));
                assert!(
                    !std::path::Path::new(&fx.path(target)).exists(),
                    "a dry run must not create the target"
                );

                let real = server.call_merge_libraries(&args);
                assert!(!real.is_error, "{}", get_result_text(&real));
                let names: Vec<String> = if target.ends_with("PcbLib") {
                    PcbLib::open(fx.path(target)).unwrap().names()
                } else {
                    SchLib::open(fx.path(target)).unwrap().names()
                };
                assert_eq!(names.len(), 4, "both copies should survive: {names:?}");
                assert!(
                    names.iter().any(|n| n.ends_with("_1")),
                    "the duplicate should be renamed, got: {names:?}"
                );
            }
        }

        #[test]
        fn merge_reports_a_failed_write_for_both_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for (source, target) in [("Lib.PcbLib", "ML.PcbLib"), ("Lib.SchLib", "ML.SchLib")] {
                let target_path = fx.path(target);
                if target.ends_with("PcbLib") {
                    PcbLib::new().save(&target_path).unwrap();
                } else {
                    SchLib::new().save(&target_path).unwrap();
                }
                block_save(std::path::Path::new(&target_path), true);
                let r = server.call_merge_libraries(&json!({
                    "source_filepaths": [fx.path(source)], "target_filepath": &target_path,
                }));
                block_save(std::path::Path::new(&target_path), false);
                assert!(r.is_error, "{}", get_result_text(&r));
            }
        }

        // ---- reorder_components ------------------------------------------------

        #[test]
        fn reorder_names_each_missing_argument_and_bad_extension() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());

            let escaped = server.call_reorder_components(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
                "component_order": ["A"],
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));

            let no_order = server.call_reorder_components(&json!({
                "filepath": fx.path("Lib.PcbLib"),
            }));
            assert_error_mentions(&no_order, "component_order");

            // Present but holding nothing readable as a name.
            let empty_order = server.call_reorder_components(&json!({
                "filepath": fx.path("Lib.PcbLib"), "component_order": [1, 2],
            }));
            assert_error_mentions(&empty_order, "empty or contains no strings");

            let wrong_ext = server.call_reorder_components(&json!({
                "filepath": fx.path("Lib.txt"), "component_order": ["A"],
            }));
            assert_error_mentions(&wrong_ext, "Unsupported file type");
        }

        #[test]
        fn reorder_reports_unreadable_libraries_of_both_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            for lib in ["Bad.PcbLib", "Bad.SchLib"] {
                let r = server.call_reorder_components(&json!({
                    "filepath": fx.path(lib), "component_order": ["A"],
                }));
                assert!(r.is_error, "{}", get_result_text(&r));
            }
        }

        #[test]
        fn reorder_reports_names_it_could_not_place_and_ones_it_appended() {
            // A caller listing a name the library does not hold, and omitting
            // one it does, gets both facts back rather than a silent reorder.
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for (lib, present) in [("Lib.PcbLib", "CHIP_0603"), ("Lib.SchLib", "CAPACITOR")] {
                let r = server.call_reorder_components(&json!({
                    "filepath": fx.path(lib), "component_order": [present, "GHOST"],
                }));
                assert!(!r.is_error, "{}", get_result_text(&r));
                let text = get_result_text(&r);
                assert!(text.contains("GHOST"), "{text}");
                assert!(text.contains("not found"), "{text}");
                assert!(text.contains("appended at end"), "{text}");
            }
        }

        #[test]
        fn reorder_reports_a_failed_write_for_both_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for (lib, present) in [("Lib.PcbLib", "CHIP_0603"), ("Lib.SchLib", "CAPACITOR")] {
                let path = fx.path(lib);
                block_save(std::path::Path::new(&path), true);
                let r = server.call_reorder_components(&json!({
                    "filepath": &path, "component_order": [present],
                }));
                block_save(std::path::Path::new(&path), false);
                assert!(r.is_error, "{}", get_result_text(&r));
                assert!(
                    get_result_text(&r).contains("Failed to write library"),
                    "{}",
                    get_result_text(&r)
                );
            }
        }
    }

    /// A name that differs from an existing one only in case is the same
    /// storage to the OLE directory and the same component to Altium. Every
    /// tool that creates a name refuses it — naming the existing spelling —
    /// and a rename onto the component's own name in another case is the
    /// one such rename that is allowed.
    #[test]
    fn a_name_differing_only_in_case_is_taken() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Case.PcbLib");
        create_test_pcblib(&path); // CHIP_0402 + CHIP_0603

        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "CHIP_0402",
            "target_name": "chip_0603",
        }));
        assert!(result.is_error);
        assert!(
            get_result_text(&result).contains(
                "already exists in library as 'CHIP_0603' (component names are case-insensitive)"
            ),
            "{}",
            get_result_text(&result)
        );

        let result = server.call_rename_component(&json!({
            "filepath": path.to_string_lossy(),
            "old_name": "CHIP_0402",
            "new_name": "chip_0603",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("as 'CHIP_0603'"));

        // The component's own name in another case is a legitimate rename,
        // however the caller spells the old name.
        let result = server.call_rename_component(&json!({
            "filepath": path.to_string_lossy(),
            "old_name": "Chip_0402",
            "new_name": "chip_0402",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let lib = PcbLib::open(&path).unwrap();
        assert_eq!(
            lib.names(),
            ["chip_0402", "CHIP_0603"],
            "renamed in place, not moved to the end"
        );

        // Across libraries too.
        let other = dir.path().join("Other.PcbLib");
        create_test_pcblib(&other);
        let result = server.call_copy_component_cross_library(&json!({
            "source_filepath": other.to_string_lossy(),
            "target_filepath": path.to_string_lossy(),
            "component_name": "CHIP_0402",
            "new_name": "Chip_0402",
        }));
        assert!(result.is_error);
        assert!(
            get_result_text(&result).contains("as 'chip_0402'"),
            "{}",
            get_result_text(&result)
        );

        // A merge sees the clash the same way in a dry run and for real.
        for dry_run in [true, false] {
            let result = server.call_merge_libraries(&json!({
                "source_filepaths": [other.to_string_lossy()],
                "target_filepath": path.to_string_lossy(),
                "on_duplicate": "skip",
                "dry_run": dry_run,
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            assert_eq!(parsed["skipped_count"], 2, "dry_run={dry_run}: {parsed}");
        }
        assert_eq!(PcbLib::open(&path).unwrap().len(), 2, "nothing merged");

        // The same for symbols.
        let sch = dir.path().join("Case.SchLib");
        create_test_schlib(&sch); // RESISTOR + CAPACITOR
        let result = server.call_copy_component(&json!({
            "filepath": sch.to_string_lossy(),
            "source_name": "RESISTOR",
            "target_name": "capacitor",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("as 'CAPACITOR'"));
        let result = server.call_rename_component(&json!({
            "filepath": sch.to_string_lossy(),
            "old_name": "RESISTOR",
            "new_name": "Resistor",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        assert_eq!(
            SchLib::open(&sch).unwrap().names(),
            ["Resistor", "CAPACITOR"]
        );
    }

    /// Without a new name a cross-library copy keeps the source's spelling,
    /// however the caller spelt the component name.
    #[test]
    fn a_cross_library_copy_keeps_the_source_spelling() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Case.PcbLib");
        create_test_pcblib(&path); // CHIP_0402 + CHIP_0603
        let other = dir.path().join("Other.PcbLib");
        create_test_pcblib(&other);

        let result = server.call_copy_component_cross_library(&json!({
            "source_filepath": other.to_string_lossy(),
            "target_filepath": path.to_string_lossy(),
            "component_name": "chip_0603",
        }));
        assert!(result.is_error, "{}", get_result_text(&result));
        assert!(
            get_result_text(&result).contains("'CHIP_0603' already exists"),
            "{}",
            get_result_text(&result)
        );
        let spare = dir.path().join("Spare.PcbLib");
        PcbLib::new().save(&spare).unwrap();
        let result = server.call_copy_component_cross_library(&json!({
            "source_filepath": other.to_string_lossy(),
            "target_filepath": spare.to_string_lossy(),
            "component_name": "chip_0603",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        assert_eq!(parse_result_json(&result)["target_name"], "CHIP_0603");
        assert_eq!(PcbLib::open(&spare).unwrap().names(), ["CHIP_0603"]);
    }

    /// A cross-library copy takes a new description when one is given, and
    /// keeps the source's when not.
    #[test]
    fn a_cross_library_copy_takes_the_description_it_is_given() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let source = dir.path().join("Source.PcbLib");
        create_test_pcblib(&source);
        let target = dir.path().join("Target.PcbLib");
        PcbLib::new().save(&target).unwrap();

        let result = server.call_copy_component_cross_library(&json!({
            "source_filepath": source.to_string_lossy(),
            "target_filepath": target.to_string_lossy(),
            "component_name": "CHIP_0402",
            "description": "described on the way over",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let lib = PcbLib::open(&target).unwrap();
        assert_eq!(
            lib.get("CHIP_0402").unwrap().description,
            "described on the way over"
        );

        let result = server.call_copy_component_cross_library(&json!({
            "source_filepath": source.to_string_lossy(),
            "target_filepath": target.to_string_lossy(),
            "component_name": "CHIP_0603",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let lib = PcbLib::open(&target).unwrap();
        assert_eq!(
            lib.get("CHIP_0603").unwrap().description,
            PcbLib::open(&source)
                .unwrap()
                .get("CHIP_0603")
                .unwrap()
                .description,
            "without a description the source's is kept"
        );
    }

    /// `on_duplicate: "error"` stops a merge at the first name the target
    /// already holds, naming it and its source, and writes nothing — in both
    /// formats.
    #[test]
    fn a_merge_that_must_not_meet_a_duplicate_stops_at_the_first_one() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let pcb_target = dir.path().join("Target.PcbLib");
        create_test_pcblib(&pcb_target);
        let pcb_source = dir.path().join("Source.PcbLib");
        create_test_pcblib(&pcb_source); // the same two names again
        let before = std::fs::read(&pcb_target).unwrap();
        let result = server.call_merge_libraries(&json!({
            "source_filepaths": [pcb_source.to_string_lossy()],
            "target_filepath": pcb_target.to_string_lossy(),
            "on_duplicate": "error",
        }));
        assert!(result.is_error, "{}", get_result_text(&result));
        let text = get_result_text(&result);
        assert!(
            text.contains("Duplicate component name 'CHIP_0402'"),
            "{text}"
        );
        assert!(text.contains("'skip' or 'rename'"), "{text}");
        assert_eq!(
            std::fs::read(&pcb_target).unwrap(),
            before,
            "nothing written"
        );

        let sch_target = dir.path().join("Target.SchLib");
        create_test_schlib(&sch_target);
        let sch_source = dir.path().join("Source.SchLib");
        create_test_schlib(&sch_source);
        let before = std::fs::read(&sch_target).unwrap();
        let result = server.call_merge_libraries(&json!({
            "source_filepaths": [sch_source.to_string_lossy()],
            "target_filepath": sch_target.to_string_lossy(),
            "on_duplicate": "error",
        }));
        assert!(result.is_error, "{}", get_result_text(&result));
        let text = get_result_text(&result);
        assert!(
            text.contains("Duplicate component name 'RESISTOR'"),
            "{text}"
        );
        assert_eq!(
            std::fs::read(&sch_target).unwrap(),
            before,
            "nothing written"
        );
    }

    /// A copy given a description the record cannot hold is refused by
    /// field, with the library untouched and no backup made.
    #[test]
    fn copy_component_refuses_a_pipe_in_the_description_before_any_backup() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Pipe.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_copy_component(&json!({
            "filepath": path.to_string_lossy(),
            "source_name": "CHIP_0402",
            "target_name": "CHIP_COPY",
            "description": "A|B",
        }));
        assert!(result.is_error);
        let text = get_result_text(&result);
        assert!(
            text.contains("Footprint 'CHIP_COPY' description contains '|'"),
            "{text}"
        );
        let backups = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "bak"))
            .count();
        assert_eq!(backups, 0, "no backup for a save that never happens");
        assert!(
            PcbLib::open(&path).unwrap().get("CHIP_COPY").is_none(),
            "nothing copied"
        );
    }
}
