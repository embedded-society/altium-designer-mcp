//! Repair/bulk-rename/backup/update tools. Split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

impl McpServer {
    /// Repairs a library by removing orphaned references.
    pub(crate) fn call_repair_library(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::PcbLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Currently only supports PcbLib
        if !filepath.to_lowercase().ends_with(".pcblib") {
            return ToolCallResult::error("repair_library currently only supports .PcbLib files");
        }

        // Read the library
        let mut library = match PcbLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        let original_model_count = library.model_count();
        let original_component_body_count: usize =
            library.iter().map(|fp| fp.component_bodies.len()).sum();

        // Remove orphaned models (models not referenced by any footprint)
        let orphaned_models_removed = library.remove_orphaned_models();

        // Remove orphaned component body references (references to non-existent models)
        let orphaned_bodies_info = library.remove_orphaned_component_bodies();
        let orphaned_bodies_removed: usize = orphaned_bodies_info.iter().map(|(_, c)| c).sum();

        let needs_save = orphaned_models_removed > 0 || orphaned_bodies_removed > 0;

        // Save if not dry run and changes were made
        if needs_save && !dry_run {
            if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
                return resp;
            }
        }

        let mut result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "filepath": filepath,
            "dry_run": dry_run,
            "repairs": {
                "orphaned_models_removed": orphaned_models_removed,
                "orphaned_component_bodies_removed": orphaned_bodies_removed,
                "affected_footprints": orphaned_bodies_info.iter()
                    .map(|(name, count)| json!({"name": name, "removed": count}))
                    .collect::<Vec<_>>()
            },
            "before": {
                "model_count": original_model_count,
                "total_component_bodies": original_component_body_count
            },
            "after": {
                "model_count": library.model_count(),
                "total_component_bodies": library.iter()
                    .map(|fp| fp.component_bodies.len())
                    .sum::<usize>()
            }
        });

        if needs_save && !dry_run {
            result["message"] = json!(format!(
                "Repaired library: removed {} orphaned models and {} orphaned component body references",
                orphaned_models_removed, orphaned_bodies_removed
            ));
        } else if needs_save && dry_run {
            result["message"] = json!(format!(
                "Would remove {} orphaned models and {} orphaned component body references",
                orphaned_models_removed, orphaned_bodies_removed
            ));
        } else {
            result["message"] = json!("No repairs needed - library is clean");
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Renames multiple components using regex pattern matching.
    pub(crate) fn call_bulk_rename(&self, arguments: &Value) -> ToolCallResult {
        use regex::Regex;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };
        let Some(pattern) = arguments.get("pattern").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: pattern");
        };
        let Some(replacement) = arguments.get("replacement").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: replacement");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Compile regex
        let regex = match Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(format!("Invalid regex pattern: {e}")),
        };

        let filepath_lower = filepath.to_lowercase();
        if filepath_lower.ends_with(".pcblib") {
            Self::bulk_rename_pcblib(filepath, &regex, replacement, dry_run)
        } else if filepath_lower.ends_with(".schlib") {
            Self::bulk_rename_schlib(filepath, &regex, replacement, dry_run)
        } else {
            ToolCallResult::error("Unsupported file type. Expected .PcbLib or .SchLib")
        }
    }

    /// Bulk rename components in a `PcbLib` file.
    pub(crate) fn bulk_rename_pcblib(
        filepath: &str,
        regex: &regex::Regex,
        replacement: &str,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::PcbLib;

        let mut library = match PcbLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        let mut renames: Vec<(String, String)> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        // Collect all renames first (to check for conflicts)
        let names: Vec<String> = library.names().into_iter().collect();
        for name in &names {
            if regex.is_match(name) {
                let new_name = regex.replace(name, replacement).to_string();
                if new_name != *name {
                    renames.push((name.clone(), new_name));
                }
            }
        }

        // Check for conflicts (new name already exists or duplicates in renames)
        let existing_names: std::collections::HashSet<&str> =
            names.iter().map(String::as_str).collect();
        let mut new_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (old_name, new_name) in &renames {
            // Check if new name conflicts with an existing name that's not being renamed
            if existing_names.contains(new_name.as_str()) {
                let is_being_renamed = renames.iter().any(|(o, _)| o == new_name);
                if !is_being_renamed {
                    errors.push(format!(
                        "Cannot rename '{old_name}' to '{new_name}': target name already exists"
                    ));
                }
            }
            // Check for duplicate new names
            if !new_names.insert(new_name.clone()) {
                errors.push(format!(
                    "Multiple components would be renamed to '{new_name}' (conflict)"
                ));
            }
        }

        if !errors.is_empty() {
            return ToolCallResult::error(format!(
                "Rename conflicts detected:\n{}",
                errors.join("\n")
            ));
        }

        // Perform renames (if not dry run)
        if !dry_run && !renames.is_empty() {
            // Create backup before destructive operation
            if let Err(e) = Self::create_backup(filepath) {
                return ToolCallResult::error(e);
            }

            // Two-phase remove-then-add (see the SchLib path): `add` overwrites on
            // key collision, so a one-pass loop loses a footprint on a chained
            // rename like A->B, B->C.
            let mut pending: Vec<crate::altium::pcblib::Footprint> =
                Vec::with_capacity(renames.len());
            for (old_name, new_name) in &renames {
                if let Some(mut footprint) = library.remove(old_name) {
                    footprint.name.clone_from(new_name);
                    pending.push(footprint);
                }
            }
            for footprint in pending {
                library.add(footprint);
            }

            if let Err(e) = library.save(filepath) {
                return ToolCallResult::error(format!("Failed to save library: {e}"));
            }
        }

        let result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "filepath": filepath,
            "file_type": "PcbLib",
            "dry_run": dry_run,
            "pattern": regex.as_str(),
            "replacement": replacement,
            "renamed_count": renames.len(),
            "renames": renames.iter()
                .map(|(old, new)| json!({"from": old, "to": new}))
                .collect::<Vec<_>>()
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Bulk rename components in a `SchLib` file.
    pub(crate) fn bulk_rename_schlib(
        filepath: &str,
        regex: &regex::Regex,
        replacement: &str,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::SchLib;

        let mut library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        let mut renames: Vec<(String, String)> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        // Collect all renames first (to check for conflicts)
        let names: Vec<String> = library.names().into_iter().collect();
        for name in &names {
            if regex.is_match(name) {
                let new_name = regex.replace(name, replacement).to_string();
                if new_name != *name {
                    renames.push((name.clone(), new_name));
                }
            }
        }

        // Check for conflicts
        let existing_names: std::collections::HashSet<&str> =
            names.iter().map(String::as_str).collect();
        let mut new_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (old_name, new_name) in &renames {
            if existing_names.contains(new_name.as_str()) {
                let is_being_renamed = renames.iter().any(|(o, _)| o == new_name);
                if !is_being_renamed {
                    errors.push(format!(
                        "Cannot rename '{old_name}' to '{new_name}': target name already exists"
                    ));
                }
            }
            if !new_names.insert(new_name.clone()) {
                errors.push(format!(
                    "Multiple components would be renamed to '{new_name}' (conflict)"
                ));
            }
        }

        if !errors.is_empty() {
            return ToolCallResult::error(format!(
                "Rename conflicts detected:\n{}",
                errors.join("\n")
            ));
        }

        // Perform renames (if not dry run)
        if !dry_run && !renames.is_empty() {
            // Create backup before destructive operation
            if let Err(e) = Self::create_backup(filepath) {
                return ToolCallResult::error(e);
            }

            // Two-phase: remove EVERY source before adding ANY target. `add` is
            // IndexMap::insert (overwrites on key collision), so a one-pass
            // remove-then-add loses a symbol on a chained rename like A->B, B->C —
            // adding B (still present) clobbers the original B before B->C removes
            // it. The conflict check permits such chains (target is itself being
            // renamed), so the application must be collision-safe.
            let mut pending: Vec<crate::altium::schlib::Symbol> = Vec::with_capacity(renames.len());
            for (old_name, new_name) in &renames {
                if let Some(mut symbol) = library.remove(old_name) {
                    symbol.name.clone_from(new_name);
                    pending.push(symbol);
                }
            }
            for symbol in pending {
                library.add(symbol);
            }

            if let Err(e) = library.save(filepath) {
                return ToolCallResult::error(format!("Failed to save library: {e}"));
            }
        }

        let result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "filepath": filepath,
            "file_type": "SchLib",
            "dry_run": dry_run,
            "pattern": regex.as_str(),
            "replacement": replacement,
            "renamed_count": renames.len(),
            "renames": renames.iter()
                .map(|(old, new)| json!({"from": old, "to": new}))
                .collect::<Vec<_>>()
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Lists available backup files for an Altium library.
    pub(crate) fn call_list_backups(&self, arguments: &Value) -> ToolCallResult {
        use std::path::Path;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let path = Path::new(filepath);
        let Some(parent) = path.parent() else {
            return ToolCallResult::error("Cannot determine parent directory");
        };
        let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
            return ToolCallResult::error("Cannot determine filename");
        };

        // Find backup files matching pattern: {filename}.{timestamp}.bak
        let backup_pattern = format!("{filename}.");
        let mut backups: Vec<Value> = Vec::new();

        let entries = match std::fs::read_dir(parent) {
            Ok(e) => e,
            Err(e) => return ToolCallResult::error(format!("Failed to read directory: {e}")),
        };

        for entry in entries.flatten() {
            let entry_name = entry.file_name();
            let Some(name) = entry_name.to_str() else {
                continue;
            };

            // Check if this is a backup file for our target
            let is_bak = Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("bak"));
            if name.starts_with(&backup_pattern) && is_bak {
                // Extract timestamp from filename. A file such as `<lib>.bak` (no
                // timestamp segment) still matches the prefix + `.bak` suffix but is
                // shorter than `<pattern><15-char stamp>.bak`, so a raw slice would
                // panic (start > end). `get` yields None and we skip it.
                let Some(middle) = name.get(backup_pattern.len()..name.len() - 4) else {
                    continue;
                };

                // Validate timestamp format (YYYYMMDD_HHMMSS)
                if middle.len() == 15 && middle.chars().nth(8) == Some('_') {
                    let metadata = entry.metadata().ok();
                    let size = metadata.as_ref().map_or(0, std::fs::Metadata::len);
                    let modified = metadata.and_then(|m| m.modified().ok()).and_then(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_secs())
                    });

                    backups.push(json!({
                        "path": entry.path().to_string_lossy(),
                        "timestamp": middle,
                        "size_bytes": size,
                        "modified_unix": modified
                    }));
                }
            }
        }

        // Sort by timestamp descending (most recent first)
        backups.sort_by(|a, b| {
            let ts_a = a.get("timestamp").and_then(Value::as_str).unwrap_or("");
            let ts_b = b.get("timestamp").and_then(Value::as_str).unwrap_or("");
            ts_b.cmp(ts_a)
        });

        let result = json!({
            "filepath": filepath,
            "backup_count": backups.len(),
            "backups": backups
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Restores an Altium library from a backup file.
    pub(crate) fn call_restore_backup(&self, arguments: &Value) -> ToolCallResult {
        use std::path::Path;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let backup_path = if let Some(bp) = arguments.get("backup_path").and_then(Value::as_str) {
            // User specified a backup path - validate it
            if let Err(e) = self.validate_path(bp) {
                return ToolCallResult::error(e);
            }
            bp.to_string()
        } else {
            // Find the most recent backup
            let path = Path::new(filepath);
            let Some(parent) = path.parent() else {
                return ToolCallResult::error("Cannot determine parent directory");
            };
            let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
                return ToolCallResult::error("Cannot determine filename");
            };

            let backup_pattern = format!("{filename}.");
            let mut most_recent: Option<(String, String)> = None;

            let entries = match std::fs::read_dir(parent) {
                Ok(e) => e,
                Err(e) => return ToolCallResult::error(format!("Failed to read directory: {e}")),
            };

            for entry in entries.flatten() {
                let entry_name = entry.file_name();
                let Some(name) = entry_name.to_str() else {
                    continue;
                };

                let is_bak = Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("bak"));
                if name.starts_with(&backup_pattern) && is_bak {
                    // `get` (not a raw slice) so a timestamp-less `<lib>.bak` — which
                    // still matches prefix + `.bak` but is shorter than the stamped
                    // form — is skipped instead of panicking (start > end).
                    let Some(middle) = name.get(backup_pattern.len()..name.len() - 4) else {
                        continue;
                    };
                    if middle.len() == 15 && middle.chars().nth(8) == Some('_') {
                        let entry_path = entry.path().to_string_lossy().into_owned();
                        if most_recent
                            .as_ref()
                            .map_or(true, |(_, ts)| middle > ts.as_str())
                        {
                            most_recent = Some((entry_path, middle.to_string()));
                        }
                    }
                }
            }

            match most_recent {
                Some((path, _)) => path,
                None => {
                    return ToolCallResult::error(format!(
                        "No backup files found for '{}'",
                        path.file_name().map_or_else(
                            || "file".to_string(),
                            |n| n.to_string_lossy().into_owned()
                        )
                    ))
                }
            }
        };

        // Verify backup exists
        let backup = Path::new(&backup_path);
        if !backup.exists() {
            return ToolCallResult::error(format!("Backup file does not exist: {backup_path}"));
        }

        // Get file sizes for reporting
        let backup_size = std::fs::metadata(&backup_path).map_or(0, |m| m.len());
        let original_size = std::fs::metadata(filepath).map(|m| m.len()).ok();

        // Copy backup over the original
        if let Err(e) = std::fs::copy(&backup_path, filepath) {
            return ToolCallResult::error(format!("Failed to restore backup: {e}"));
        }

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "restored_from": backup_path,
            "backup_size_bytes": backup_size,
            "original_size_bytes": original_size
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Updates specific properties of a pad in a `PcbLib` footprint.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn call_update_pad(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::pcblib::primitives::PadShape;
        use crate::altium::PcbLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };
        let Some(component_name) = arguments.get("component_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_name");
        };
        let Some(designator) = arguments.get("designator").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: designator");
        };
        let Some(updates) = arguments.get("updates") else {
            return ToolCallResult::error("Missing required parameter: updates");
        };

        // Validate path
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Read library
        let mut library = match PcbLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Find footprint
        let Some(footprint) = library.get_mut(component_name) else {
            let available: Vec<String> = library.names();
            return ToolCallResult::error(format!(
                "Footprint '{component_name}' not found. Available: {available:?}"
            ));
        };

        // Find pad by designator
        let Some(pad) = footprint
            .pads
            .iter_mut()
            .find(|p| p.designator == designator)
        else {
            let available: Vec<&str> = footprint
                .pads
                .iter()
                .map(|p| p.designator.as_str())
                .collect();
            return ToolCallResult::error(format!(
                "Pad '{designator}' not found in footprint '{component_name}'. Available: {available:?}"
            ));
        };

        // Track changes for reporting
        let mut changes: Vec<Value> = Vec::new();

        // Apply updates
        if let Some(x) = updates.get("x").and_then(Value::as_f64) {
            changes.push(json!({"property": "x", "old": pad.x, "new": x}));
            pad.x = x;
        }
        if let Some(y) = updates.get("y").and_then(Value::as_f64) {
            changes.push(json!({"property": "y", "old": pad.y, "new": y}));
            pad.y = y;
        }
        if let Some(width) = updates.get("width").and_then(Value::as_f64) {
            changes.push(json!({"property": "width", "old": pad.width, "new": width}));
            pad.width = width;
        }
        if let Some(height) = updates.get("height").and_then(Value::as_f64) {
            changes.push(json!({"property": "height", "old": pad.height, "new": height}));
            pad.height = height;
        }
        if let Some(rotation) = updates.get("rotation").and_then(Value::as_f64) {
            changes.push(json!({"property": "rotation", "old": pad.rotation, "new": rotation}));
            pad.rotation = rotation;
        }
        if let Some(hole_size) = updates.get("hole_size").and_then(Value::as_f64) {
            changes.push(json!({"property": "hole_size", "old": pad.hole_size, "new": hole_size}));
            pad.hole_size = Some(hole_size);
        }
        if let Some(shape_str) = updates.get("shape").and_then(Value::as_str) {
            let new_shape = match shape_str.to_lowercase().as_str() {
                "rectangle" | "rect" => PadShape::Rectangle,
                "round" | "circular" => PadShape::Round,
                "oval" | "oblong" => PadShape::Oval,
                "octagonal" | "octagon" => PadShape::Octagonal,
                "roundedrectangle" | "rounded" => PadShape::RoundedRectangle,
                _ => {
                    return ToolCallResult::error(format!(
                    "Invalid shape '{shape_str}'. Valid: Rectangle, Round, Oval, Octagonal, RoundedRectangle"
                ))
                }
            };
            changes.push(
                json!({"property": "shape", "old": format!("{:?}", pad.shape), "new": shape_str}),
            );
            pad.shape = new_shape;
        }

        // Reject invalid geometry the create path enforces — update bypassed it,
        // and out-of-range values would silently saturate in from_mm() on save.
        if pad.width <= 0.0 || pad.height <= 0.0 {
            return ToolCallResult::error(format!(
                "Pad '{designator}': width and height must be positive"
            ));
        }
        if pad.hole_size.is_some_and(|h| h < 0.0) {
            return ToolCallResult::error(format!("Pad '{designator}': hole_size must be >= 0"));
        }

        if changes.is_empty() {
            return ToolCallResult::error("No valid updates specified");
        }

        // Coordinate range check over the whole footprint (matches the write path).
        if let Err(e) = Self::validate_footprint_coordinates(footprint) {
            return ToolCallResult::error(e);
        }

        // Save if not dry run
        if !dry_run {
            if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
                return resp;
            }
        }

        let result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "filepath": filepath,
            "component_name": component_name,
            "designator": designator,
            "changes": changes,
            "dry_run": dry_run
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Updates specific properties of a primitive in a `PcbLib` footprint.
    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    pub(crate) fn call_update_primitive(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::pcblib::primitives::Layer;
        use crate::altium::PcbLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };
        let Some(component_name) = arguments.get("component_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_name");
        };
        let Some(primitive_type) = arguments.get("primitive_type").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: primitive_type");
        };
        let Some(index) = arguments.get("index").and_then(Value::as_u64) else {
            return ToolCallResult::error("Missing required parameter: index");
        };
        let index = index as usize;
        let Some(updates) = arguments.get("updates") else {
            return ToolCallResult::error("Missing required parameter: updates");
        };

        // Validate path
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Read library
        let mut library = match PcbLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Find footprint
        let Some(footprint) = library.get_mut(component_name) else {
            let available: Vec<String> = library.names();
            return ToolCallResult::error(format!(
                "Footprint '{component_name}' not found. Available: {available:?}"
            ));
        };

        // Parse layer from string if provided
        // Use Layer::parse for exact Altium names (e.g., "Top Overlay")
        // Also support common aliases for convenience
        let parse_layer = |s: &str| -> Option<Layer> {
            // First try exact parse (handles all Altium layer names like "Top Overlay")
            if let Some(layer) = Layer::parse(s) {
                return Some(layer);
            }
            // Fall back to common aliases (case-insensitive)
            match s.to_lowercase().replace([' ', '-'], "").as_str() {
                "toplayer" | "top" => Some(Layer::TopLayer),
                "bottomlayer" | "bottom" => Some(Layer::BottomLayer),
                "topoverlay" | "topsilk" => Some(Layer::TopOverlay),
                "bottomoverlay" | "bottomsilk" => Some(Layer::BottomOverlay),
                "multilayer" => Some(Layer::MultiLayer),
                "topsolder" => Some(Layer::TopSolder),
                "bottomsolder" => Some(Layer::BottomSolder),
                "toppaste" => Some(Layer::TopPaste),
                "bottompaste" => Some(Layer::BottomPaste),
                "topassembly" => Some(Layer::TopAssembly),
                "bottomassembly" => Some(Layer::BottomAssembly),
                "top3dbody" => Some(Layer::Top3DBody),
                "bottom3dbody" => Some(Layer::Bottom3DBody),
                "keepout" | "keepoutlayer" => Some(Layer::KeepOut),
                s if s.starts_with("mechanical") => {
                    // Handle Mechanical1-32
                    let num_str = s.strip_prefix("mechanical")?;
                    let num: u8 = num_str.parse().ok()?;
                    match num {
                        1 => Some(Layer::Mechanical1),
                        2 => Some(Layer::Mechanical2),
                        3 => Some(Layer::Mechanical3),
                        4 => Some(Layer::Mechanical4),
                        5 => Some(Layer::Mechanical5),
                        6 => Some(Layer::Mechanical6),
                        7 => Some(Layer::Mechanical7),
                        8 => Some(Layer::Mechanical8),
                        9 => Some(Layer::Mechanical9),
                        10 => Some(Layer::Mechanical10),
                        11 => Some(Layer::Mechanical11),
                        12 => Some(Layer::Mechanical12),
                        13 => Some(Layer::Mechanical13),
                        14 => Some(Layer::Mechanical14),
                        15 => Some(Layer::Mechanical15),
                        16 => Some(Layer::Mechanical16),
                        17 => Some(Layer::Mechanical17),
                        18 => Some(Layer::Mechanical18),
                        19 => Some(Layer::Mechanical19),
                        20 => Some(Layer::Mechanical20),
                        21 => Some(Layer::Mechanical21),
                        22 => Some(Layer::Mechanical22),
                        23 => Some(Layer::Mechanical23),
                        24 => Some(Layer::Mechanical24),
                        25 => Some(Layer::Mechanical25),
                        26 => Some(Layer::Mechanical26),
                        27 => Some(Layer::Mechanical27),
                        28 => Some(Layer::Mechanical28),
                        29 => Some(Layer::Mechanical29),
                        30 => Some(Layer::Mechanical30),
                        31 => Some(Layer::Mechanical31),
                        32 => Some(Layer::Mechanical32),
                        _ => None,
                    }
                }
                s if s.starts_with("midlayer") => {
                    // Handle Mid-Layer 1-30
                    let num_str = s.strip_prefix("midlayer")?;
                    let num: u8 = num_str.parse().ok()?;
                    match num {
                        1 => Some(Layer::MidLayer1),
                        2 => Some(Layer::MidLayer2),
                        3 => Some(Layer::MidLayer3),
                        4 => Some(Layer::MidLayer4),
                        5 => Some(Layer::MidLayer5),
                        6 => Some(Layer::MidLayer6),
                        7 => Some(Layer::MidLayer7),
                        8 => Some(Layer::MidLayer8),
                        9 => Some(Layer::MidLayer9),
                        10 => Some(Layer::MidLayer10),
                        11 => Some(Layer::MidLayer11),
                        12 => Some(Layer::MidLayer12),
                        13 => Some(Layer::MidLayer13),
                        14 => Some(Layer::MidLayer14),
                        15 => Some(Layer::MidLayer15),
                        16 => Some(Layer::MidLayer16),
                        17 => Some(Layer::MidLayer17),
                        18 => Some(Layer::MidLayer18),
                        19 => Some(Layer::MidLayer19),
                        20 => Some(Layer::MidLayer20),
                        21 => Some(Layer::MidLayer21),
                        22 => Some(Layer::MidLayer22),
                        23 => Some(Layer::MidLayer23),
                        24 => Some(Layer::MidLayer24),
                        25 => Some(Layer::MidLayer25),
                        26 => Some(Layer::MidLayer26),
                        27 => Some(Layer::MidLayer27),
                        28 => Some(Layer::MidLayer28),
                        29 => Some(Layer::MidLayer29),
                        30 => Some(Layer::MidLayer30),
                        _ => None,
                    }
                }
                s if s.starts_with("internalplane") => {
                    // Handle Internal Plane 1-16
                    let num_str = s.strip_prefix("internalplane")?;
                    let num: u8 = num_str.parse().ok()?;
                    match num {
                        1 => Some(Layer::InternalPlane1),
                        2 => Some(Layer::InternalPlane2),
                        3 => Some(Layer::InternalPlane3),
                        4 => Some(Layer::InternalPlane4),
                        5 => Some(Layer::InternalPlane5),
                        6 => Some(Layer::InternalPlane6),
                        7 => Some(Layer::InternalPlane7),
                        8 => Some(Layer::InternalPlane8),
                        9 => Some(Layer::InternalPlane9),
                        10 => Some(Layer::InternalPlane10),
                        11 => Some(Layer::InternalPlane11),
                        12 => Some(Layer::InternalPlane12),
                        13 => Some(Layer::InternalPlane13),
                        14 => Some(Layer::InternalPlane14),
                        15 => Some(Layer::InternalPlane15),
                        16 => Some(Layer::InternalPlane16),
                        _ => None,
                    }
                }
                _ => None,
            }
        };

        let mut changes: Vec<Value> = Vec::new();

        match primitive_type {
            "track" => {
                if index >= footprint.tracks.len() {
                    return ToolCallResult::error(format!(
                        "Track index {} out of range (0..{})",
                        index,
                        footprint.tracks.len()
                    ));
                }
                let track = &mut footprint.tracks[index];

                if let Some(x1) = updates.get("x1").and_then(Value::as_f64) {
                    changes.push(json!({"property": "x1", "old": track.x1, "new": x1}));
                    track.x1 = x1;
                }
                if let Some(y1) = updates.get("y1").and_then(Value::as_f64) {
                    changes.push(json!({"property": "y1", "old": track.y1, "new": y1}));
                    track.y1 = y1;
                }
                if let Some(x2) = updates.get("x2").and_then(Value::as_f64) {
                    changes.push(json!({"property": "x2", "old": track.x2, "new": x2}));
                    track.x2 = x2;
                }
                if let Some(y2) = updates.get("y2").and_then(Value::as_f64) {
                    changes.push(json!({"property": "y2", "old": track.y2, "new": y2}));
                    track.y2 = y2;
                }
                if let Some(width) = updates.get("width").and_then(Value::as_f64) {
                    changes.push(json!({"property": "width", "old": track.width, "new": width}));
                    track.width = width;
                }
                if let Some(layer_str) = updates.get("layer").and_then(Value::as_str) {
                    if let Some(layer) = parse_layer(layer_str) {
                        changes.push(json!({"property": "layer", "old": format!("{:?}", track.layer), "new": layer_str}));
                        track.layer = layer;
                    } else {
                        return ToolCallResult::error(format!("Invalid layer: {layer_str}"));
                    }
                }
            }
            "arc" => {
                if index >= footprint.arcs.len() {
                    return ToolCallResult::error(format!(
                        "Arc index {} out of range (0..{})",
                        index,
                        footprint.arcs.len()
                    ));
                }
                let arc = &mut footprint.arcs[index];

                if let Some(x) = updates
                    .get("x1")
                    .or_else(|| updates.get("x"))
                    .and_then(Value::as_f64)
                {
                    changes.push(json!({"property": "x", "old": arc.x, "new": x}));
                    arc.x = x;
                }
                if let Some(y) = updates
                    .get("y1")
                    .or_else(|| updates.get("y"))
                    .and_then(Value::as_f64)
                {
                    changes.push(json!({"property": "y", "old": arc.y, "new": y}));
                    arc.y = y;
                }
                if let Some(radius) = updates.get("radius").and_then(Value::as_f64) {
                    changes.push(json!({"property": "radius", "old": arc.radius, "new": radius}));
                    arc.radius = radius;
                }
                if let Some(start_angle) = updates.get("start_angle").and_then(Value::as_f64) {
                    changes.push(json!({"property": "start_angle", "old": arc.start_angle, "new": start_angle}));
                    arc.start_angle = start_angle;
                }
                if let Some(end_angle) = updates.get("end_angle").and_then(Value::as_f64) {
                    changes.push(
                        json!({"property": "end_angle", "old": arc.end_angle, "new": end_angle}),
                    );
                    arc.end_angle = end_angle;
                }
                if let Some(width) = updates.get("width").and_then(Value::as_f64) {
                    changes.push(json!({"property": "width", "old": arc.width, "new": width}));
                    arc.width = width;
                }
                if let Some(layer_str) = updates.get("layer").and_then(Value::as_str) {
                    if let Some(layer) = parse_layer(layer_str) {
                        changes.push(json!({"property": "layer", "old": format!("{:?}", arc.layer), "new": layer_str}));
                        arc.layer = layer;
                    } else {
                        return ToolCallResult::error(format!("Invalid layer: {layer_str}"));
                    }
                }
            }
            "text" => {
                if index >= footprint.text.len() {
                    return ToolCallResult::error(format!(
                        "Text index {} out of range (0..{})",
                        index,
                        footprint.text.len()
                    ));
                }
                let text = &mut footprint.text[index];

                if let Some(x) = updates.get("x").and_then(Value::as_f64) {
                    changes.push(json!({"property": "x", "old": text.x, "new": x}));
                    text.x = x;
                }
                if let Some(y) = updates.get("y").and_then(Value::as_f64) {
                    changes.push(json!({"property": "y", "old": text.y, "new": y}));
                    text.y = y;
                }
                if let Some(height) = updates.get("height").and_then(Value::as_f64) {
                    changes.push(json!({"property": "height", "old": text.height, "new": height}));
                    text.height = height;
                }
                if let Some(rotation) = updates.get("rotation").and_then(Value::as_f64) {
                    changes.push(
                        json!({"property": "rotation", "old": text.rotation, "new": rotation}),
                    );
                    text.rotation = rotation;
                }
                if let Some(content) = updates.get("text").and_then(Value::as_str) {
                    changes.push(
                        json!({"property": "text", "old": text.text.clone(), "new": content}),
                    );
                    text.text = content.to_string();
                }
                if let Some(layer_str) = updates.get("layer").and_then(Value::as_str) {
                    if let Some(layer) = parse_layer(layer_str) {
                        changes.push(json!({"property": "layer", "old": format!("{:?}", text.layer), "new": layer_str}));
                        text.layer = layer;
                    } else {
                        return ToolCallResult::error(format!("Invalid layer: {layer_str}"));
                    }
                }
            }
            "fill" => {
                if index >= footprint.fills.len() {
                    return ToolCallResult::error(format!(
                        "Fill index {} out of range (0..{})",
                        index,
                        footprint.fills.len()
                    ));
                }
                let fill = &mut footprint.fills[index];

                if let Some(x1) = updates
                    .get("x1")
                    .or_else(|| updates.get("x"))
                    .and_then(Value::as_f64)
                {
                    changes.push(json!({"property": "x1", "old": fill.x1, "new": x1}));
                    fill.x1 = x1;
                }
                if let Some(y1) = updates
                    .get("y1")
                    .or_else(|| updates.get("y"))
                    .and_then(Value::as_f64)
                {
                    changes.push(json!({"property": "y1", "old": fill.y1, "new": y1}));
                    fill.y1 = y1;
                }
                if let Some(x2) = updates.get("x2").and_then(Value::as_f64) {
                    changes.push(json!({"property": "x2", "old": fill.x2, "new": x2}));
                    fill.x2 = x2;
                }
                if let Some(y2) = updates.get("y2").and_then(Value::as_f64) {
                    changes.push(json!({"property": "y2", "old": fill.y2, "new": y2}));
                    fill.y2 = y2;
                }
                if let Some(rotation) = updates.get("rotation").and_then(Value::as_f64) {
                    changes.push(
                        json!({"property": "rotation", "old": fill.rotation, "new": rotation}),
                    );
                    fill.rotation = rotation;
                }
                if let Some(layer_str) = updates.get("layer").and_then(Value::as_str) {
                    if let Some(layer) = parse_layer(layer_str) {
                        changes.push(json!({"property": "layer", "old": format!("{:?}", fill.layer), "new": layer_str}));
                        fill.layer = layer;
                    } else {
                        return ToolCallResult::error(format!("Invalid layer: {layer_str}"));
                    }
                }
            }
            "region" => {
                if index >= footprint.regions.len() {
                    return ToolCallResult::error(format!(
                        "Region index {} out of range (0..{})",
                        index,
                        footprint.regions.len()
                    ));
                }
                let region = &mut footprint.regions[index];

                // Regions mainly have vertices and layer
                if let Some(layer_str) = updates.get("layer").and_then(Value::as_str) {
                    if let Some(layer) = parse_layer(layer_str) {
                        changes.push(json!({"property": "layer", "old": format!("{:?}", region.layer), "new": layer_str}));
                        region.layer = layer;
                    } else {
                        return ToolCallResult::error(format!("Invalid layer: {layer_str}"));
                    }
                }
                // Note: Updating region vertices would require array-based updates, which is more complex
            }
            _ => {
                return ToolCallResult::error(format!(
                    "Invalid primitive_type '{primitive_type}'. Valid: track, arc, region, text, fill"
                ));
            }
        }

        if changes.is_empty() {
            return ToolCallResult::error("No valid updates specified for this primitive type");
        }

        // Re-validate after the in-place edits: update bypassed the create-path
        // checks, so an out-of-range coordinate would silently saturate in
        // from_mm() and a non-positive dimension would write a degenerate shape.
        if let Err(e) = Self::validate_footprint_coordinates(footprint) {
            return ToolCallResult::error(e);
        }
        let dim_err = match primitive_type {
            "track" => {
                (footprint.tracks[index].width <= 0.0).then_some("track width must be positive")
            }
            "arc" => {
                let a = &footprint.arcs[index];
                if a.radius <= 0.0 {
                    Some("arc radius must be positive")
                } else if a.width < 0.0 {
                    Some("arc width must be >= 0")
                } else {
                    None
                }
            }
            "text" => {
                (footprint.text[index].height <= 0.0).then_some("text height must be positive")
            }
            _ => None,
        };
        if let Some(msg) = dim_err {
            return ToolCallResult::error(format!("Primitive {index} ({primitive_type}): {msg}"));
        }

        // Save if not dry run
        if !dry_run {
            if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
                return resp;
            }
        }

        let result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "filepath": filepath,
            "component_name": component_name,
            "primitive_type": primitive_type,
            "index": index,
            "changes": changes,
            "dry_run": dry_run
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }
}

#[cfg(test)]
mod tests {

    use crate::altium::pcblib::{
        ComponentBody, EmbeddedModel, Footprint, Layer, Pad, PcbLib, Track,
    };
    use crate::altium::SchLib;
    use crate::mcp::tools::test_support::{
        create_test_pcblib, create_test_schlib, create_test_server, get_result_text,
        parse_result_json, test_temp_dir,
    };
    use serde_json::json;

    // ==================== repair_library ====================

    #[test]
    fn repair_library_clean_library_needs_no_repairs() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Clean.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_repair_library(&json!({
            "filepath": path.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["repairs"]["orphaned_models_removed"], 0);
        assert_eq!(parsed["repairs"]["orphaned_component_bodies_removed"], 0);
        assert_eq!(parsed["message"], "No repairs needed - library is clean");
    }

    /// Builds a library with one orphaned embedded model (no footprint
    /// references it). Note the reverse case — a component body referencing a
    /// missing model — cannot be authored through `PcbLib::save`, which
    /// validates embedded references at write time; it only arises from
    /// external tools.
    fn create_dirty_pcblib(path: &std::path::Path) {
        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("DIRTY");
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
        lib.add(fp);
        lib.add_model(EmbeddedModel::new(
            "{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}",
            "orphan.step",
            b"ISO-10303-21; orphaned".to_vec(),
        ));
        lib.save(path).expect("Failed to create dirty PcbLib");
    }

    #[test]
    fn repair_library_removes_orphaned_models() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Dirty.PcbLib");
        create_dirty_pcblib(&path);

        let result = server.call_repair_library(&json!({
            "filepath": path.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["repairs"]["orphaned_models_removed"], 1);
        assert_eq!(parsed["repairs"]["orphaned_component_bodies_removed"], 0);
        assert_eq!(parsed["before"]["model_count"], 1);
        assert_eq!(parsed["after"]["model_count"], 0);

        // The repair persisted.
        let lib = PcbLib::open(&path).unwrap();
        assert_eq!(lib.model_count(), 0);
    }

    #[test]
    fn repair_library_removes_orphaned_component_bodies_in_memory() {
        // The on-disk orphaned-body state cannot be authored through the
        // writer (it validates embedded references), so exercise the library
        // layer the handler delegates to directly.
        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("DIRTY");
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
        fp.add_component_body(ComponentBody::new(
            "{99999999-9999-9999-9999-999999999999}",
            "missing.step",
        ));
        lib.add(fp);

        let removed = lib.remove_orphaned_component_bodies();
        assert_eq!(removed, vec![("DIRTY".to_string(), 1)]);
        assert!(lib.get("DIRTY").unwrap().component_bodies.is_empty());
    }

    #[test]
    fn repair_library_dry_run_previews_without_writing() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("DirtyDry.PcbLib");
        create_dirty_pcblib(&path);

        let result = server.call_repair_library(&json!({
            "filepath": path.to_string_lossy(),
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "dry_run");
        assert!(parsed["message"]
            .as_str()
            .unwrap()
            .starts_with("Would remove"));

        // Nothing was written: the orphaned model is still there.
        let lib = PcbLib::open(&path).unwrap();
        assert_eq!(lib.model_count(), 1);
    }

    #[test]
    fn repair_library_rejects_non_pcblib() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Repair.SchLib");
        create_test_schlib(&path);

        let result = server.call_repair_library(&json!({
            "filepath": path.to_string_lossy(),
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("only supports .PcbLib"));
    }

    // ==================== bulk_rename ====================

    #[test]
    fn bulk_rename_pcblib_applies_regex_replacement() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Bulk.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_bulk_rename(&json!({
            "filepath": path.to_string_lossy(),
            "pattern": "^CHIP_",
            "replacement": "RES_",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["renamed_count"], 2);
        assert_eq!(parsed["renames"][0]["from"], "CHIP_0402");
        assert_eq!(parsed["renames"][0]["to"], "RES_0402");

        let lib = PcbLib::open(&path).unwrap();
        assert!(lib.get("RES_0402").is_some());
        assert!(lib.get("RES_0603").is_some());
        assert!(lib.get("CHIP_0402").is_none());
    }

    #[test]
    fn bulk_rename_schlib_dry_run_and_conflicts() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Bulk.SchLib");
        create_test_schlib(&path);
        let filepath = path.to_string_lossy().to_string();

        // Dry run reports the plan but writes nothing.
        let result = server.call_bulk_rename(&json!({
            "filepath": filepath,
            "pattern": "^RESISTOR$",
            "replacement": "RES",
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "dry_run");
        assert_eq!(parsed["renamed_count"], 1);
        let lib = SchLib::open(&path).unwrap();
        assert!(lib.get("RESISTOR").is_some());

        // Mapping both symbols to the same name is a conflict.
        let result = server.call_bulk_rename(&json!({
            "filepath": filepath,
            "pattern": "^(RESISTOR|CAPACITOR)$",
            "replacement": "PART",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Rename conflicts detected"));

        // Invalid regex is rejected.
        let result = server.call_bulk_rename(&json!({
            "filepath": filepath,
            "pattern": "(unclosed",
            "replacement": "X",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Invalid regex pattern"));
    }

    // ==================== list_backups / restore_backup ====================

    #[test]
    fn list_backups_finds_only_timestamped_backups() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Lib.PcbLib");
        create_test_pcblib(&path);

        // Two valid backups plus a timestamp-less .bak that must be ignored.
        let bak_old = dir.path().join("Lib.PcbLib.20260101_090000.bak");
        let bak_new = dir.path().join("Lib.PcbLib.20260301_120000.bak");
        std::fs::copy(&path, &bak_old).unwrap();
        std::fs::copy(&path, &bak_new).unwrap();
        std::fs::write(dir.path().join("Lib.PcbLib.bak"), b"stray").unwrap();

        let result = server.call_list_backups(&json!({
            "filepath": path.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["backup_count"], 2);
        // Sorted most recent first.
        assert_eq!(parsed["backups"][0]["timestamp"], "20260301_120000");
        assert_eq!(parsed["backups"][1]["timestamp"], "20260101_090000");
        assert!(parsed["backups"][0]["size_bytes"].as_u64().unwrap() > 0);
    }

    #[test]
    fn list_backups_empty_when_none_exist() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("NoBak.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_list_backups(&json!({
            "filepath": path.to_string_lossy(),
        }));
        assert!(!result.is_error);
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["backup_count"], 0);
        assert_eq!(parsed["backups"], json!([]));
    }

    #[test]
    fn restore_backup_restores_most_recent() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Restore.PcbLib");
        create_test_pcblib(&path);

        // Snapshot the pristine two-footprint state as the newest backup.
        let bak = dir.path().join("Restore.PcbLib.20260301_120000.bak");
        std::fs::copy(&path, &bak).unwrap();

        // Mutate the live library.
        let mut lib = PcbLib::open(&path).unwrap();
        lib.remove("CHIP_0402");
        lib.save(&path).unwrap();
        assert_eq!(PcbLib::open(&path).unwrap().len(), 1);

        let result = server.call_restore_backup(&json!({
            "filepath": path.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert!(parsed["restored_from"]
            .as_str()
            .unwrap()
            .ends_with("Restore.PcbLib.20260301_120000.bak"));

        // The pristine state is back.
        let lib = PcbLib::open(&path).unwrap();
        assert_eq!(lib.len(), 2);
        assert!(lib.get("CHIP_0402").is_some());
    }

    #[test]
    fn restore_backup_with_explicit_path_and_error_paths() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("RestoreX.PcbLib");
        create_test_pcblib(&path);

        // No backups yet.
        let result = server.call_restore_backup(&json!({
            "filepath": path.to_string_lossy(),
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("No backup files found"));

        // Explicit backup path that does not exist.
        let ghost = dir.path().join("Ghost.bak");
        let result = server.call_restore_backup(&json!({
            "filepath": path.to_string_lossy(),
            "backup_path": ghost.to_string_lossy(),
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("does not exist"));

        // Explicit backup path that does exist.
        let bak = dir.path().join("RestoreX.PcbLib.20260101_000000.bak");
        std::fs::copy(&path, &bak).unwrap();
        let result = server.call_restore_backup(&json!({
            "filepath": path.to_string_lossy(),
            "backup_path": bak.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(
            parsed["backup_size_bytes"].as_u64().unwrap(),
            std::fs::metadata(&bak).unwrap().len()
        );
    }

    // ==================== update_pad ====================

    #[test]
    fn update_pad_changes_geometry_and_persists() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Pad.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_update_pad(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "CHIP_0402",
            "designator": "1",
            "updates": { "x": -0.6, "width": 0.7, "shape": "round" },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["dry_run"], false);
        let changes = parsed["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 3);
        assert!(changes
            .iter()
            .any(|c| c["property"] == "x" && c["new"] == -0.6));
        assert!(changes
            .iter()
            .any(|c| c["property"] == "shape" && c["new"] == "round"));

        let lib = PcbLib::open(&path).unwrap();
        let pad = &lib.get("CHIP_0402").unwrap().pads[0];
        assert!((pad.x - -0.6).abs() < 1e-4);
        assert!((pad.width - 0.7).abs() < 1e-4);
        assert_eq!(format!("{:?}", pad.shape), "Round");
    }

    #[test]
    fn update_pad_dry_run_leaves_file_untouched() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("PadDry.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_update_pad(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "CHIP_0402",
            "designator": "1",
            "updates": { "width": 0.9 },
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "dry_run");
        assert_eq!(parsed["dry_run"], true);

        let lib = PcbLib::open(&path).unwrap();
        assert!((lib.get("CHIP_0402").unwrap().pads[0].width - 0.6).abs() < 1e-4);
    }

    #[test]
    fn update_pad_error_paths() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("PadErr.PcbLib");
        create_test_pcblib(&path);
        let filepath = path.to_string_lossy().to_string();

        // Unknown footprint (with available list).
        let result = server.call_update_pad(&json!({
            "filepath": filepath,
            "component_name": "NOPE",
            "designator": "1",
            "updates": { "width": 0.9 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("'NOPE' not found"));
        assert!(get_result_text(&result).contains("CHIP_0402"));

        // Unknown pad designator.
        let result = server.call_update_pad(&json!({
            "filepath": filepath,
            "component_name": "CHIP_0402",
            "designator": "99",
            "updates": { "width": 0.9 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Pad '99' not found"));

        // Invalid shape name.
        let result = server.call_update_pad(&json!({
            "filepath": filepath,
            "component_name": "CHIP_0402",
            "designator": "1",
            "updates": { "shape": "hexagon" },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Invalid shape 'hexagon'"));

        // Non-positive dimensions are rejected.
        let result = server.call_update_pad(&json!({
            "filepath": filepath,
            "component_name": "CHIP_0402",
            "designator": "1",
            "updates": { "width": -1.0 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("must be positive"));

        // No recognised update keys.
        let result = server.call_update_pad(&json!({
            "filepath": filepath,
            "component_name": "CHIP_0402",
            "designator": "1",
            "updates": { "bogus": 1.0 },
        }));
        assert!(result.is_error);
        assert_eq!(get_result_text(&result), "No valid updates specified");
    }

    // ==================== update_primitive ====================

    /// Builds a library whose single footprint carries a track, an arc-free
    /// text and enough primitives to exercise `update_primitive`.
    fn create_primitive_pcblib(path: &std::path::Path) {
        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("PRIMS");
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
        fp.add_track(Track::new(-1.0, -1.0, 1.0, -1.0, 0.2, Layer::TopOverlay));
        lib.add(fp);
        lib.save(path).expect("Failed to create primitives PcbLib");
    }

    #[test]
    fn update_primitive_track_changes_persist() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Prim.PcbLib");
        create_primitive_pcblib(&path);

        let result = server.call_update_primitive(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "PRIMS",
            "primitive_type": "track",
            "index": 0,
            "updates": { "width": 0.3, "layer": "Mechanical 1" },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["primitive_type"], "track");
        assert_eq!(parsed["index"], 0);
        let changes = parsed["changes"].as_array().unwrap();
        assert!(changes
            .iter()
            .any(|c| c["property"] == "width" && c["new"] == 0.3));
        assert!(changes
            .iter()
            .any(|c| c["property"] == "layer" && c["new"] == "Mechanical 1"));

        let lib = PcbLib::open(&path).unwrap();
        let track = &lib.get("PRIMS").unwrap().tracks[0];
        assert!((track.width - 0.3).abs() < 1e-4);
        assert_eq!(track.layer, Layer::Mechanical1);
    }

    #[test]
    fn update_primitive_dry_run_leaves_file_untouched() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("PrimDry.PcbLib");
        create_primitive_pcblib(&path);

        let result = server.call_update_primitive(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "PRIMS",
            "primitive_type": "track",
            "index": 0,
            "updates": { "width": 0.5 },
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "dry_run");

        let lib = PcbLib::open(&path).unwrap();
        assert!((lib.get("PRIMS").unwrap().tracks[0].width - 0.2).abs() < 1e-4);
    }

    #[test]
    fn update_primitive_error_paths() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("PrimErr.PcbLib");
        create_primitive_pcblib(&path);
        let filepath = path.to_string_lossy().to_string();

        // Index out of range.
        let result = server.call_update_primitive(&json!({
            "filepath": filepath,
            "component_name": "PRIMS",
            "primitive_type": "track",
            "index": 7,
            "updates": { "width": 0.3 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("out of range"));

        // Invalid primitive type.
        let result = server.call_update_primitive(&json!({
            "filepath": filepath,
            "component_name": "PRIMS",
            "primitive_type": "sprocket",
            "index": 0,
            "updates": { "width": 0.3 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Invalid primitive_type 'sprocket'"));

        // Invalid layer name.
        let result = server.call_update_primitive(&json!({
            "filepath": filepath,
            "component_name": "PRIMS",
            "primitive_type": "track",
            "index": 0,
            "updates": { "layer": "NotALayer" },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Invalid layer: NotALayer"));

        // Non-positive track width is rejected.
        let result = server.call_update_primitive(&json!({
            "filepath": filepath,
            "component_name": "PRIMS",
            "primitive_type": "track",
            "index": 0,
            "updates": { "width": 0.0 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("track width must be positive"));

        // Missing required index.
        let result = server.call_update_primitive(&json!({
            "filepath": filepath,
            "component_name": "PRIMS",
            "primitive_type": "track",
            "updates": { "width": 0.3 },
        }));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: index"
        );
    }

    // ==================== update_primitive: arc/text/fill/region arms ====================

    mod primitive_arms {
        use super::*;
        use crate::altium::pcblib::{
            Arc, Fill, PcbFlags, Region, Text, TextJustification, TextKind,
        };

        /// A footprint carrying one of every 2D primitive at index 0, so each
        /// `update_primitive` arm has a target.
        fn create_rich_pcblib(path: &std::path::Path) {
            let mut lib = PcbLib::new();
            let mut fp = Footprint::new("RICH");
            fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
            fp.add_track(Track::new(-1.0, -1.0, 1.0, -1.0, 0.2, Layer::TopOverlay));
            fp.add_arc(Arc::circle(0.0, 0.0, 1.0, 0.15, Layer::TopOverlay));
            fp.add_fill(Fill::new(-0.5, -0.5, 0.5, 0.5, Layer::TopPaste));
            fp.add_region(Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopLayer));
            fp.add_text(Text {
                x: 0.0,
                y: 1.0,
                text: "REF".to_string(),
                height: 0.5,
                layer: Layer::TopOverlay,
                rotation: 0.0,
                kind: TextKind::Stroke,
                stroke_font: None,
                stroke_width: None,
                italic: false,
                bold: false,
                mirror: false,
                is_comment: false,
                is_designator: false,
                font_name: "Arial".to_string(),
                justification: TextJustification::BottomLeft,
                is_inverted: false,
                inverted_border: None,
                use_inverted_rectangle: false,
                inverted_rect_width: None,
                inverted_rect_height: None,
                inverted_rect_text_offset: None,
                flags: PcbFlags::empty(),
                net_index: 0xFFFF,
                polygon_index: 0xFFFF,
                component_index: -1,
                unique_id: None,
            });
            lib.add(fp);
            lib.save(path).expect("Failed to create rich PcbLib");
        }

        fn change_props(parsed: &serde_json::Value) -> Vec<String> {
            parsed["changes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["property"].as_str().unwrap_or("").to_string())
                .collect()
        }

        #[test]
        fn update_primitive_arc_arm_persists() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Arc.PcbLib");
            create_rich_pcblib(&path);

            let result = server.call_update_primitive(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RICH",
                "primitive_type": "arc",
                "index": 0,
                "updates": {
                    "x": 0.25, "y": -0.25, "radius": 1.5,
                    "start_angle": 10.0, "end_angle": 200.0, "width": 0.2,
                    "layer": "Bottom Overlay",
                },
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            assert_eq!(parsed["primitive_type"], "arc");
            let props = change_props(&parsed);
            assert!(props.contains(&"radius".to_string()));
            assert!(props.contains(&"x".to_string()));

            let lib = PcbLib::open(&path).unwrap();
            let arc = &lib.get("RICH").unwrap().arcs[0];
            assert!((arc.radius - 1.5).abs() < 1e-4);
            assert_eq!(arc.layer, Layer::BottomOverlay);
        }

        #[test]
        fn update_primitive_arc_zero_radius_rejected() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("ArcBad.PcbLib");
            create_rich_pcblib(&path);
            let result = server.call_update_primitive(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RICH",
                "primitive_type": "arc",
                "index": 0,
                "updates": { "radius": 0.0 },
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("radius must be positive"));
        }

        #[test]
        fn update_primitive_text_arm_persists() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Text.PcbLib");
            create_rich_pcblib(&path);

            let result = server.call_update_primitive(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RICH",
                "primitive_type": "text",
                "index": 0,
                "updates": { "x": 0.1, "y": 0.2, "height": 0.7, "rotation": 90.0, "text": "NEW", "layer": "Top Overlay" },
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let props = change_props(&parse_result_json(&result));
            assert!(props.contains(&"text".to_string()));
            assert!(props.contains(&"height".to_string()));

            let lib = PcbLib::open(&path).unwrap();
            assert_eq!(lib.get("RICH").unwrap().text[0].text, "NEW");
        }

        #[test]
        fn update_primitive_fill_arm_persists() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Fill.PcbLib");
            create_rich_pcblib(&path);

            let result = server.call_update_primitive(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RICH",
                "primitive_type": "fill",
                "index": 0,
                "updates": { "x": -0.3, "y": -0.3, "x2": 0.4, "y2": 0.4, "rotation": 45.0, "layer": "Bottom Paste" },
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let props = change_props(&parse_result_json(&result));
            assert!(props.contains(&"rotation".to_string()));
            assert!(props.contains(&"x1".to_string()));

            let lib = PcbLib::open(&path).unwrap();
            assert!((lib.get("RICH").unwrap().fills[0].rotation - 45.0).abs() < 1e-4);
        }

        #[test]
        fn update_primitive_region_arm_persists() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Region.PcbLib");
            create_rich_pcblib(&path);

            let result = server.call_update_primitive(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RICH",
                "primitive_type": "region",
                "index": 0,
                "updates": { "layer": "Bottom Layer" },
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let props = change_props(&parse_result_json(&result));
            assert_eq!(props, vec!["layer".to_string()]);

            let lib = PcbLib::open(&path).unwrap();
            assert_eq!(
                lib.get("RICH").unwrap().regions[0].layer,
                Layer::BottomLayer
            );
        }

        #[test]
        fn update_primitive_spaceless_layer_aliases_resolve() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Alias.PcbLib");
            create_rich_pcblib(&path);
            let filepath = path.to_string_lossy().to_string();

            // Space-less names bypass Layer::parse and exercise the alias arms.
            for (input, expected) in [
                ("MidLayer5", Layer::MidLayer5),
                ("InternalPlane2", Layer::InternalPlane2),
                ("Mechanical10", Layer::Mechanical10),
            ] {
                let result = server.call_update_primitive(&json!({
                    "filepath": filepath,
                    "component_name": "RICH",
                    "primitive_type": "track",
                    "index": 0,
                    "updates": { "layer": input },
                }));
                assert!(!result.is_error, "{}", get_result_text(&result));
                let lib = PcbLib::open(&path).unwrap();
                assert_eq!(
                    lib.get("RICH").unwrap().tracks[0].layer,
                    expected,
                    "{input}"
                );
            }
        }
    }

    // ==================== update_pad: remaining update keys ====================

    #[test]
    fn update_pad_y_height_rotation_hole_size_persist() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("PadKeys.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_update_pad(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "CHIP_0402",
            "designator": "1",
            "updates": { "y": 0.3, "height": 0.7, "rotation": 90.0, "hole_size": 0.2 },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let changes = parse_result_json(&result);
        let props: Vec<&str> = changes["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["property"].as_str().unwrap_or(""))
            .collect();
        for expected in ["y", "height", "rotation", "hole_size"] {
            assert!(props.contains(&expected), "missing {expected}: {props:?}");
        }

        let lib = PcbLib::open(&path).unwrap();
        let pad = lib
            .get("CHIP_0402")
            .unwrap()
            .pads
            .iter()
            .find(|p| p.designator == "1")
            .unwrap();
        assert!((pad.height - 0.7).abs() < 1e-4);
        assert_eq!(pad.hole_size, Some(0.2));
    }

    #[test]
    fn update_pad_negative_hole_size_rejected() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("PadBad.PcbLib");
        create_test_pcblib(&path);
        let result = server.call_update_pad(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "CHIP_0402",
            "designator": "1",
            "updates": { "hole_size": -0.1 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("hole_size must be >= 0"));
    }

    // ==================== bulk_rename: SchLib real apply ====================

    #[test]
    fn bulk_rename_schlib_applies_and_persists() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("BulkApply.SchLib");
        create_test_schlib(&path);

        let result = server.call_bulk_rename(&json!({
            "filepath": path.to_string_lossy(),
            "pattern": "^RESISTOR$",
            "replacement": "RES",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["file_type"], "SchLib");
        assert_eq!(parsed["renamed_count"], 1);
        assert_eq!(parsed["renames"][0]["from"], "RESISTOR");
        assert_eq!(parsed["renames"][0]["to"], "RES");

        let lib = SchLib::open(&path).unwrap();
        assert!(lib.get("RES").is_some());
        assert!(lib.get("RESISTOR").is_none());
        assert!(lib.get("CAPACITOR").is_some());
    }
}
