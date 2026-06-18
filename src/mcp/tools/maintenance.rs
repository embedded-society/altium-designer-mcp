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
            // Create backup before destructive operation
            if let Err(e) = Self::create_backup(filepath) {
                return ToolCallResult::error(e);
            }

            if let Err(e) = library.save(filepath) {
                return ToolCallResult::error(format!("Failed to save library: {e}"));
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

            for (old_name, new_name) in &renames {
                if let Some(mut footprint) = library.remove(old_name) {
                    footprint.name.clone_from(new_name);
                    library.add(footprint);
                }
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

            for (old_name, new_name) in &renames {
                if let Some(mut symbol) = library.remove(old_name) {
                    symbol.name.clone_from(new_name);
                    library.add(symbol);
                }
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
                // Extract timestamp from filename
                let middle = &name[backup_pattern.len()..name.len() - 4]; // Remove prefix and ".bak"

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
                    let middle = &name[backup_pattern.len()..name.len() - 4];
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
                "roundedrectangle" | "rounded" => PadShape::RoundedRectangle,
                _ => {
                    return ToolCallResult::error(format!(
                    "Invalid shape '{shape_str}'. Valid: Rectangle, Round, Oval, RoundedRectangle"
                ))
                }
            };
            changes.push(
                json!({"property": "shape", "old": format!("{:?}", pad.shape), "new": shape_str}),
            );
            pad.shape = new_shape;
        }

        if changes.is_empty() {
            return ToolCallResult::error("No valid updates specified");
        }

        // Save if not dry run
        if !dry_run {
            if let Err(e) = Self::create_backup(filepath) {
                return ToolCallResult::error(e);
            }
            if let Err(e) = library.save(filepath) {
                return ToolCallResult::error(format!("Failed to save library: {e}"));
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

        // Save if not dry run
        if !dry_run {
            if let Err(e) = Self::create_backup(filepath) {
                return ToolCallResult::error(e);
            }
            if let Err(e) = library.save(filepath) {
                return ToolCallResult::error(format!("Failed to save library: {e}"));
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
