//! Repair/bulk-rename/backup/update tools. Split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

/// The properties `update_pad` applies.
pub const UPDATE_PAD_KEYS: &[&str] = &[
    "x",
    "y",
    "width",
    "height",
    "rotation",
    "hole_size",
    "shape",
];

/// The properties `update_primitive` applies to a primitive kind, or `None`
/// for a kind it does not address.
pub fn update_primitive_keys(primitive_type: &str) -> Option<&'static [&'static str]> {
    Some(match primitive_type {
        "track" => &["x1", "y1", "x2", "y2", "width", "layer"],
        "arc" => &[
            "x1",
            "y1",
            "x",
            "y",
            "radius",
            "start_angle",
            "end_angle",
            "width",
            "layer",
        ],
        "text" => &["x", "y", "height", "rotation", "text", "layer"],
        "fill" => &["x1", "y1", "x", "y", "x2", "y2", "rotation", "layer"],
        "region" => &["layer"],
        "via" => &["x", "y", "diameter", "hole_size", "from_layer", "to_layer"],
        _ => return None,
    })
}

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
        // Names clash the way the library resolves them: regardless of case.
        let existing_names: std::collections::HashSet<String> = names
            .iter()
            .map(|n| crate::altium::folded_name(n))
            .collect();
        let mut new_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (old_name, new_name) in &renames {
            // A regex replacement can produce anything, including an empty
            // string or a name no storage can carry.
            if let Err(e) = Self::validate_ole_name(new_name) {
                errors.push(format!("Cannot rename '{old_name}': {e}"));
                continue;
            }
            // Check if new name conflicts with an existing name that's not being renamed
            if existing_names.contains(&crate::altium::folded_name(new_name)) {
                let is_being_renamed = renames
                    .iter()
                    .any(|(o, _)| crate::altium::same_name(o, new_name));
                if !is_being_renamed {
                    errors.push(format!(
                        "Cannot rename '{old_name}' to '{new_name}': target name already exists"
                    ));
                }
            }
            // Check for duplicate new names
            if !new_names.insert(crate::altium::folded_name(new_name)) {
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

            // Every rename resolves against the names before the call and is
            // applied in place, so a chained rename like A->B, B->C renames
            // both and every footprint keeps its position in the library.
            library.rename_all(&renames);

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
        // Names clash the way the library resolves them: regardless of case.
        let existing_names: std::collections::HashSet<String> = names
            .iter()
            .map(|n| crate::altium::folded_name(n))
            .collect();
        let mut new_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (old_name, new_name) in &renames {
            // A regex replacement can produce anything, including an empty
            // string or a name no storage can carry.
            if let Err(e) = Self::validate_ole_name(new_name) {
                errors.push(format!("Cannot rename '{old_name}': {e}"));
                continue;
            }
            if existing_names.contains(&crate::altium::folded_name(new_name)) {
                let is_being_renamed = renames
                    .iter()
                    .any(|(o, _)| crate::altium::same_name(o, new_name));
                if !is_being_renamed {
                    errors.push(format!(
                        "Cannot rename '{old_name}' to '{new_name}': target name already exists"
                    ));
                }
            }
            if !new_names.insert(crate::altium::folded_name(new_name)) {
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
            // Every rename resolves against the names before the call and is
            // applied in place, so a chained rename like A->B, B->C (which the
            // conflict check permits, the target being itself renamed) renames
            // both and every symbol keeps its position in the library.
            library.rename_all(&renames);

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
        let original_size = std::fs::metadata(filepath).map(|m| m.len()).ok();

        // Read the backup first: snapshotting the current file below rotates
        // the backup set, and the oldest entry it evicts could be the very
        // file being restored.
        let bytes = match std::fs::read(&backup_path) {
            Ok(bytes) => bytes,
            Err(e) => return ToolCallResult::error(format!("Failed to read backup: {e}")),
        };
        let backup_size = bytes.len();

        // A restore overwrites the current file, which is as destructive as
        // any edit — the wrong backup picked, or unsaved-elsewhere work in the
        // current state, must stay recoverable. Snapshot it like every other
        // mutating tool does, then write atomically so a failure mid-way
        // cannot leave a half-restored library.
        let pre_restore_backup = match Self::create_backup(filepath) {
            Ok(path) => path,
            Err(e) => return ToolCallResult::error(e),
        };
        let written = crate::altium::save_atomic(Path::new(filepath), "restore.tmp", |image| {
            use std::io::Write as _;
            image
                .write_all(&bytes)
                .map_err(|e| crate::altium::AltiumError::file_write(Path::new(filepath), e))
        });
        if let Err(e) = written {
            return ToolCallResult::error(format!("Failed to restore backup: {e}"));
        }

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "restored_from": backup_path,
            "backup_size_bytes": backup_size,
            "original_size_bytes": original_size,
            "pre_restore_backup": pre_restore_backup,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Carries a primary width / height / shape edit into a stacked pad's
    /// per-layer tables: every layer whose value matched the old primary takes
    /// the new one, a layer with its own value keeps it. Returns how many
    /// per-layer entries changed.
    fn propagate_pad_edit_to_stack(
        pad: &mut crate::altium::pcblib::Pad,
        old_width: f64,
        old_height: f64,
        old_shape: crate::altium::pcblib::PadShape,
    ) -> usize {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
        let mut followed = 0;
        if let Some(sizes) = pad.per_layer_sizes.as_mut() {
            for size in sizes.iter_mut() {
                let width_follows = !close(old_width, pad.width) && close(size.0, old_width);
                let height_follows = !close(old_height, pad.height) && close(size.1, old_height);
                if width_follows {
                    size.0 = pad.width;
                }
                if height_follows {
                    size.1 = pad.height;
                }
                followed += usize::from(width_follows || height_follows);
            }
        }
        if old_shape != pad.shape {
            if let Some(shapes) = pad.per_layer_shapes.as_mut() {
                for shape in shapes.iter_mut().filter(|s| **s == old_shape) {
                    *shape = pad.shape;
                    followed += 1;
                }
            }
        }
        followed
    }

    /// Carries a via diameter edit into its per-layer table (a non-simple
    /// stack): every layer whose diameter matched the old primary takes the
    /// new one, a layer with its own value keeps it. Returns how many layers
    /// followed.
    fn propagate_via_edit_to_stack(
        via: &mut crate::altium::pcblib::Via,
        old_diameter: f64,
    ) -> usize {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
        if close(old_diameter, via.diameter) {
            return 0;
        }
        let new_diameter = via.diameter;
        via.per_layer_diameters.as_mut().map_or(0, |layers| {
            layers
                .iter_mut()
                .filter(|d| close(**d, old_diameter))
                .map(|d| *d = new_diameter)
                .count()
        })
    }

    /// Applies an `update_pad` request's `updates` object to `pad`, returning
    /// the change records for the response (or the message for an invalid
    /// value). A stacked pad's per-layer tables follow a primary edit — see
    /// [`Self::propagate_pad_edit_to_stack`].
    fn apply_pad_updates(
        pad: &mut crate::altium::pcblib::Pad,
        updates: &Value,
    ) -> Result<Vec<Value>, String> {
        let mut changes: Vec<Value> = Vec::new();
        let (old_width, old_height, old_shape) = (pad.width, pad.height, pad.shape);

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
            let Some(new_shape) = Self::parse_pad_shape(shape_str) else {
                return Err(format!(
                    "Invalid shape '{shape_str}'. {}",
                    crate::mcp::tools::parsing::PAD_SHAPE_HELP
                ));
            };
            changes.push(
                json!({"property": "shape", "old": format!("{:?}", pad.shape), "new": shape_str}),
            );
            pad.shape = new_shape;
            // The format cannot say "Round with a corner radius" — shape id 1
            // plus a radius under 100% IS a rounded rectangle — so leaving the
            // old radius behind would silently re-round the pad on read.
            if new_shape != crate::altium::pcblib::PadShape::RoundedRectangle {
                pad.corner_radius_percent = None;
                pad.per_layer_corner_radii = None;
            }
        }

        // A stacked pad's per-layer tables are what the writer emits for each
        // layer, so a primary size or shape edit that left them alone did not
        // take in Altium. Layers that shared the old primary value follow the
        // edit; a layer carrying its own deliberate value keeps it. The count
        // is reported so a caller can see how much of the stack moved.
        if pad.stack_mode != crate::altium::pcblib::PadStackMode::Simple {
            let followed = Self::propagate_pad_edit_to_stack(pad, old_width, old_height, old_shape);
            if followed > 0 {
                changes.push(json!({
                    "property": "per_layer_stack",
                    "layers_followed": followed,
                    "note": "layers that shared the old primary value now carry the new one; layers with their own value were left alone",
                }));
            }
        }

        Ok(changes)
    }

    /// Updates specific properties of a pad in a `PcbLib` footprint.
        fn parse_numbered_layer(s: &str) -> Option<Layer> {
            let parse_family = |prefix: &str, f: fn(u8) -> Option<Layer>| -> Option<Layer> {
                let num = s.strip_prefix(prefix)?.parse::<u8>().ok()?;
                f(num)
            };

            parse_family("mechanical", |n| match n {
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
            })
            .or_else(|| parse_family("midlayer", |n| match n {
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
            }))
            .or_else(|| parse_family("internalplane", |n| match n {
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
            }))
        }

    pub(crate) fn call_update_pad(&self, arguments: &Value) -> ToolCallResult {
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
        // A key this tool does not apply is a typo to refuse, not a no-op.
        if let Err(e) = Self::check_unknown_fields(updates, UPDATE_PAD_KEYS) {
            return ToolCallResult::error(e);
        }

        // Validate path
                s if s.starts_with("mechanical")
                    || s.starts_with("midlayer")
                    || s.starts_with("internalplane") =>
                {
                    parse_numbered_layer(s)
        let changes = match Self::apply_pad_updates(pad, updates) {
            Ok(changes) => changes,
            Err(e) => return ToolCallResult::error(e),
        };

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
        // The properties this primitive kind can take; anything else is a
        // typo or a property of another kind, refused rather than ignored.
        let Some(keys) = update_primitive_keys(primitive_type) else {
            return ToolCallResult::error(format!(
                "Invalid primitive_type '{primitive_type}'. Valid: track, arc, region, text, fill, via"
            ));
        };
        if let Err(e) = Self::check_unknown_fields(updates, keys) {
            return ToolCallResult::error(e);
        }

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
                    if let Some(layer) = Layer::parse(layer_str) {
                        changes.push(json!({"property": "layer", "old": format!("{:?}", track.layer), "new": layer_str}));
                        track.layer = layer;
                        // A header byte carried from the read names the old
                        // layer; the new one derives its own.
                        track.raw_layer_id = None;
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
                    if let Some(layer) = Layer::parse(layer_str) {
                        changes.push(json!({"property": "layer", "old": format!("{:?}", arc.layer), "new": layer_str}));
                        arc.layer = layer;
                        // A header byte carried from the read names the old
                        // layer; the new one derives its own.
                        arc.raw_layer_id = None;
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
                    if let Some(layer) = Layer::parse(layer_str) {
                        changes.push(json!({"property": "layer", "old": format!("{:?}", text.layer), "new": layer_str}));
                        text.layer = layer;
                        // A header byte carried from the read names the old
                        // layer; the new one derives its own.
                        text.raw_layer_id = None;
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
                    if let Some(layer) = Layer::parse(layer_str) {
                        changes.push(json!({"property": "layer", "old": format!("{:?}", fill.layer), "new": layer_str}));
                        fill.layer = layer;
                        // A header byte carried from the read names the old
                        // layer; the new one derives its own.
                        fill.raw_layer_id = None;
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
                    if let Some(layer) = Layer::parse(layer_str) {
                        changes.push(json!({"property": "layer", "old": format!("{:?}", region.layer), "new": layer_str}));
                        region.layer = layer;
                        // A replayed V7_LAYER token names the layer the region
                        // was read on; moving it makes that stale, so the
                        // writer derives the token from the new layer instead.
                        region.v7_layer = None;
                    } else {
                        return ToolCallResult::error(format!("Invalid layer: {layer_str}"));
                    }
                }
                // Note: Updating region vertices would require array-based updates, which is more complex
            }
            "via" => {
                if index >= footprint.vias.len() {
                    return ToolCallResult::error(format!(
                        "Via index {} out of range (0..{})",
                        index,
                        footprint.vias.len()
                    ));
                }
                let via = &mut footprint.vias[index];

                // Vias carry no designator, so they are addressed positionally
                // like tracks and arcs rather than by name like pads.
                if let Some(x) = updates.get("x").and_then(Value::as_f64) {
                    changes.push(json!({"property": "x", "old": via.x, "new": x}));
                    via.x = x;
                }
                if let Some(y) = updates.get("y").and_then(Value::as_f64) {
                    changes.push(json!({"property": "y", "old": via.y, "new": y}));
                    via.y = y;
                }
                if let Some(diameter) = updates.get("diameter").and_then(Value::as_f64) {
                    changes.push(
                        json!({"property": "diameter", "old": via.diameter, "new": diameter}),
                    );
                    let old_diameter = via.diameter;
                    via.diameter = diameter;
                    // A stacked via's per-layer diameters are what the writer
                    // emits for each layer, so a primary edit that left them
                    // alone did not take in Altium (the pad's stack follows a
                    // size edit the same way). Layers that shared the old
                    // diameter follow; a layer with its own value keeps it.
                    let followed = Self::propagate_via_edit_to_stack(via, old_diameter);
                    if followed > 0 {
                        changes.push(json!({
                            "property": "per_layer_diameters",
                            "layers_followed": followed,
                            "note": "layers that shared the old diameter now carry the new one; layers with their own value were left alone",
                        }));
                    }
                }
                if let Some(hole_size) = updates.get("hole_size").and_then(Value::as_f64) {
                    changes.push(
                        json!({"property": "hole_size", "old": via.hole_size, "new": hole_size}),
                    );
                    via.hole_size = hole_size;
                }
                for (key, is_from) in [("from_layer", true), ("to_layer", false)] {
                    if let Some(layer_str) = updates.get(key).and_then(Value::as_str) {
                        let Some(layer) = Layer::parse(layer_str) else {
                            return ToolCallResult::error(format!("Invalid layer: {layer_str}"));
                        };
                        let old = if is_from {
                            via.from_layer
                        } else {
                            via.to_layer
                        };
                        changes.push(json!({
                            "property": key,
                            "old": format!("{old:?}"),
                            "new": layer_str
                        }));
                        if is_from {
                            via.from_layer = layer;
                        } else {
                            via.to_layer = layer;
                        }
                    }
                }
            }
            _ => {
                return ToolCallResult::error(format!(
                    "Invalid primitive_type '{primitive_type}'. Valid: track, arc, region, text, fill, via"
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
            "via" => {
                // Mirrors parse_via's create-path checks: the hole must fit inside
                // the annular ring, both positive.
                let v = &footprint.vias[index];
                if v.diameter <= 0.0 {
                    Some("via diameter must be positive")
                } else if v.hole_size <= 0.0 {
                    Some("via hole_size must be positive")
                } else if v.hole_size >= v.diameter {
                    Some("via hole_size must be smaller than diameter")
                } else {
                    None
                }
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
        ComponentBody, EmbeddedModel, Footprint, Layer, Pad, PadShape, PadStackMode, PcbLib, Track,
        Via, ViaStackMode,
    };
    use crate::altium::SchLib;
    use crate::mcp::server::McpServer;
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

    /// A replacement can produce a name no storage (or file) can carry, or
    /// nothing at all; both are refused by name, on both formats, and the
    /// library is left untouched.
    #[test]
    fn bulk_rename_refuses_names_no_storage_can_carry() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let pcb = dir.path().join("BulkBad.PcbLib");
        create_test_pcblib(&pcb);
        let sch = dir.path().join("BulkBad.SchLib");
        create_test_schlib(&sch);

        for (path, pattern, replacement, expect) in [
            (&pcb, "^CHIP_(.*)$", "CHIP:$1", "invalid character ':'"),
            (&pcb, "^CHIP_0402$", "", "cannot be empty"),
            (&sch, "^RESISTOR$", "RES/1", "invalid character '/'"),
        ] {
            let result = server.call_bulk_rename(&json!({
                "filepath": path.to_string_lossy(),
                "pattern": pattern,
                "replacement": replacement,
            }));
            assert!(result.is_error, "{pattern} -> {replacement:?}");
            let text = get_result_text(&result);
            assert!(text.contains(expect), "{text}");
        }
        assert!(
            PcbLib::open(&pcb).unwrap().get("CHIP_0402").is_some(),
            "untouched"
        );
        assert!(
            SchLib::open(&sch).unwrap().get("RESISTOR").is_some(),
            "untouched"
        );
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

        // And the state that was overwritten — the one-footprint library — is
        // itself recoverable: the restore snapshotted it as a fresh backup
        // before writing, so a wrong pick costs nothing.
        let pre = parsed["pre_restore_backup"]
            .as_str()
            .expect("restore reports the snapshot it took");
        assert!(
            std::path::Path::new(pre)
                .extension()
                .is_some_and(|e| e == "bak"),
            "{pre}"
        );
        let snapshot = PcbLib::open(pre).unwrap();
        assert_eq!(snapshot.len(), 1, "the overwritten state was preserved");
        assert!(snapshot.get("CHIP_0402").is_none());
        assert!(
            !path.with_extension("restore.tmp").exists(),
            "the atomic-write temp file is gone"
        );
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

        // Explicit backup path that exists but cannot be read as a file (a
        // directory): reported, and nothing touched — the restore reads the
        // backup before it snapshots or writes anything.
        let dir_bak = dir.path().join("NotAFile.bak");
        std::fs::create_dir(&dir_bak).unwrap();
        let before = std::fs::read(&path).unwrap();
        let result = server.call_restore_backup(&json!({
            "filepath": path.to_string_lossy(),
            "backup_path": dir_bak.to_string_lossy(),
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Failed to read backup"));
        assert_eq!(std::fs::read(&path).unwrap(), before, "target untouched");

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

    /// On a stacked pad the per-layer tables are what the writer emits, so a
    /// primary edit must reach them: layers that shared the old value follow,
    /// a layer with its own deliberate value keeps it, and the response says
    /// how many followed.
    #[test]
    fn update_pad_carries_an_edit_into_a_stacked_pad() {
        use crate::altium::pcblib::{PadShape, PadStackMode};

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        // 32 layers at the primary size, except layer 5 which is deliberately
        // wider; shapes all match the primary.
        let mut pad = Pad::smd("1", 0.0, 0.0, 0.6, 0.5);
        pad.stack_mode = PadStackMode::FullStack;
        let mut sizes = vec![(0.6, 0.5); 32];
        sizes[5] = (1.2, 0.5);
        pad.per_layer_sizes = Some(sizes);
        pad.per_layer_shapes = Some(vec![pad.shape; 32]);
        let mut fp = Footprint::new("STACKED");
        fp.add_pad(pad);
        let mut lib = PcbLib::new();
        lib.add(fp);
        let path = dir.path().join("Stacked.PcbLib");
        lib.save(&path).unwrap();

        let result = server.call_update_pad(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "STACKED",
            "designator": "1",
            "updates": { "width": 0.8, "shape": "round" },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        let stack_change = parsed["changes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["property"] == "per_layer_stack")
            .expect("stack propagation reported");
        // 31 layers followed the width and all 32 the shape.
        assert_eq!(stack_change["layers_followed"], 31 + 32);

        let reopened = PcbLib::open(&path).unwrap();
        let pad = &reopened.get("STACKED").unwrap().pads[0];
        assert_eq!(pad.stack_mode, PadStackMode::FullStack, "mode untouched");
        let sizes = pad.per_layer_sizes.as_ref().unwrap();
        assert!(
            (sizes[0].0 - 0.8).abs() < 1e-4,
            "a matching layer followed: {sizes:?}"
        );
        assert!(
            (sizes[5].0 - 1.2).abs() < 1e-4,
            "the deliberate layer kept its own width"
        );
        assert!(
            sizes.iter().all(|s| (s.1 - 0.5).abs() < 1e-4),
            "untouched height unchanged"
        );
        assert!(pad
            .per_layer_shapes
            .as_ref()
            .unwrap()
            .iter()
            .all(|s| *s == PadShape::Round));

        // A Simple pad reports no stack propagation at all.
        let simple = dir.path().join("Simple.PcbLib");
        create_test_pcblib(&simple);
        let result = server.call_update_pad(&json!({
            "filepath": simple.to_string_lossy(),
            "component_name": "CHIP_0402",
            "designator": "1",
            "updates": { "width": 0.8 },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        assert!(!parse_result_json(&result)["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["property"] == "per_layer_stack"));
    }

    #[test]
    fn update_pad_accepts_write_pcblib_shape_spellings() {
        // Regression: update_pad had its own shape vocabulary, so the spellings
        // write_pcblib documents and defaults to were rejected here — round-tripping
        // a pad through the two tools failed on `rounded_rectangle` and `circle`.
        for spelling in ["rounded_rectangle", "circle", "ROUND"] {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("PadShape.PcbLib");
            create_test_pcblib(&path);

            let result = server.call_update_pad(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "CHIP_0402",
                "designator": "1",
                "updates": { "shape": spelling },
            }));
            assert!(
                !result.is_error,
                "shape {spelling:?} must be accepted: {}",
                get_result_text(&result)
            );
        }

        // Still rejects genuine nonsense, with the shared guidance text.
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("PadBad.PcbLib");
        create_test_pcblib(&path);
        let bad = server.call_update_pad(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "CHIP_0402",
            "designator": "1",
            "updates": { "shape": "hexagon" },
        }));
        assert!(bad.is_error);
        assert!(
            get_result_text(&bad).contains("rounded_rectangle"),
            "error should list the accepted spellings: {}",
            get_result_text(&bad)
        );
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

        // A key the tool does not apply is refused, naming the ones it does.
        let result = server.call_update_pad(&json!({
            "filepath": filepath,
            "component_name": "CHIP_0402",
            "designator": "1",
            "updates": { "bogus": 1.0 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unknown field 'bogus'"));
        assert!(get_result_text(&result).contains("\"hole_size\""));

        // An empty update set is a caller error rather than a silent no-op write.
        let result = server.call_update_pad(&json!({
            "filepath": filepath,
            "component_name": "CHIP_0402",
            "designator": "1",
            "updates": {},
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

    /// A primitive read off an unmapped header byte sits on the Multi-Layer
    /// catch-all with the byte carried; moving it to a layer the model can
    /// name drops the byte, so the file gets that layer's own, not 100.
    #[test]
    fn update_primitive_moving_a_primitive_drops_its_carried_byte() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Carried.PcbLib");
        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("CARRY");
        let mut track = Track::new(-1.0, 0.0, 1.0, 0.0, 0.2, Layer::MultiLayer);
        track.raw_layer_id = Some(100);
        fp.add_track(track);
        lib.add(fp);
        lib.save(&path).expect("save");
        let read = PcbLib::open(&path).expect("reopen");
        assert_eq!(
            read.get("CARRY").unwrap().tracks[0].raw_layer_id,
            Some(100),
            "byte 100 was written and read back"
        );

        let result = server.call_update_primitive(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "CARRY",
            "primitive_type": "track",
            "index": 0,
            "updates": { "layer": "Top Overlay" },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let moved = PcbLib::open(&path).expect("reopen");
        let track = &moved.get("CARRY").unwrap().tracks[0];
        assert_eq!(track.layer, Layer::TopOverlay);
        assert_eq!(
            track.raw_layer_id, None,
            "the new layer's own byte is on file"
        );
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
            Arc, Fill, PcbFlags, Region, Text, TextJustification, TextKind, Via,
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
            fp.add_via(Via::new(0.6, 0.6, 0.6, 0.3));
            fp.add_text(Text {
                raw_layer_id: None,
                barcode_full_width: None,
                barcode_full_height: None,
                barcode_x_margin: None,
                barcode_y_margin: None,
                barcode_kind: 0,
                barcode_font_name: String::new(),
                barcode_inverted: false,
                barcode_show_text: false,
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
                guid: None,
                raw_geometry: None,
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
        fn update_primitive_via_arm_persists() {
            // Vias were the only footprint primitive with no update path: pads have
            // update_pad (addressed by designator), everything else is reachable
            // through update_primitive by index, but vias were reachable by neither,
            // so moving one meant rewriting the whole footprint.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Via.PcbLib");
            create_rich_pcblib(&path);

            let result = server.call_update_primitive(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RICH",
                "primitive_type": "via",
                "index": 0,
                "updates": {
                    "x": -0.55, "y": 0.55,
                    "diameter": 0.8, "hole_size": 0.4,
                    "from_layer": "Top Layer", "to_layer": "Bottom Layer",
                },
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            assert_eq!(parsed["primitive_type"], "via");
            let props = change_props(&parsed);
            for want in ["x", "y", "diameter", "hole_size", "from_layer", "to_layer"] {
                assert!(props.contains(&want.to_string()), "missing change {want}");
            }

            let lib = PcbLib::open(&path).unwrap();
            let via = &lib.get("RICH").unwrap().vias[0];
            assert!((via.x - -0.55).abs() < 1e-4, "x {}", via.x);
            assert!((via.y - 0.55).abs() < 1e-4, "y {}", via.y);
            assert!((via.diameter - 0.8).abs() < 1e-4, "dia {}", via.diameter);
            assert!((via.hole_size - 0.4).abs() < 1e-4, "hole {}", via.hole_size);
            assert_eq!(via.from_layer, Layer::TopLayer);
            assert_eq!(via.to_layer, Layer::BottomLayer);
        }

        /// A stacked via's per-layer diameters are what Altium draws per layer,
        /// so a diameter edit has to reach them: layers that shared the old
        /// diameter follow, a layer with its own value keeps it — the pad
        /// rule of #407, applied to vias.
        #[test]
        fn update_primitive_via_diameter_reaches_the_stack() {
            use crate::altium::pcblib::{Via, ViaStackMode};

            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Stack.PcbLib");
            let mut lib = PcbLib::new();
            let mut fp = Footprint::new("STACK");
            let mut via = Via::new(0.0, 0.0, 0.6, 0.3);
            via.diameter_stack_mode = ViaStackMode::FullStack;
            let mut layers = vec![0.6; 32];
            layers[1] = 0.9; // a deliberate bottom-layer value
            via.per_layer_diameters = Some(layers);
            fp.add_via(via);
            lib.add(fp);
            lib.save(&path).expect("save");

            let result = server.call_update_primitive(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "STACK",
                "primitive_type": "via",
                "index": 0,
                "updates": { "diameter": 0.8 },
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            let stack = parsed["changes"]
                .as_array()
                .expect("changes")
                .iter()
                .find(|c| c["property"] == "per_layer_diameters")
                .expect("the stack followed");
            assert_eq!(stack["layers_followed"], 31);

            let lib = PcbLib::open(&path).expect("reopen");
            let via = &lib.get("STACK").unwrap().vias[0];
            assert!((via.diameter - 0.8).abs() < 1e-4);
            let layers = via.per_layer_diameters.as_ref().expect("stacked");
            assert!((layers[0] - 0.8).abs() < 1e-4, "top followed: {layers:?}");
            assert!(
                (layers[1] - 0.9).abs() < 1e-4,
                "the deliberate value stayed"
            );
            assert!(layers[2..].iter().all(|d| (d - 0.8).abs() < 1e-4));
        }

        #[test]
        fn update_primitive_via_rejects_degenerate_geometry() {
            // Same invariants parse_via enforces on the create path: the hole must
            // fit inside the annular ring, both positive. update_primitive bypasses
            // parse_via, so the check has to be repeated here.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("ViaBad.PcbLib");

            for (updates, needle) in [
                (json!({"hole_size": 0.9}), "smaller than diameter"),
                (json!({"diameter": 0.0}), "diameter must be positive"),
                (json!({"hole_size": 0.0}), "hole_size must be positive"),
            ] {
                create_rich_pcblib(&path);
                let result = server.call_update_primitive(&json!({
                    "filepath": path.to_string_lossy(),
                    "component_name": "RICH",
                    "primitive_type": "via",
                    "index": 0,
                    "updates": updates,
                }));
                assert!(result.is_error, "{updates:?} must be rejected");
                let txt = get_result_text(&result);
                assert!(txt.contains(needle), "expected {needle:?} in: {txt}");

                // Rejected before save: the file keeps its original geometry.
                let lib = PcbLib::open(&path).unwrap();
                let via = &lib.get("RICH").unwrap().vias[0];
                assert!((via.diameter - 0.6).abs() < 1e-4, "unchanged diameter");
                assert!((via.hole_size - 0.3).abs() < 1e-4, "unchanged hole");
            }
        }

        #[test]
        fn update_primitive_via_index_out_of_range() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("ViaRange.PcbLib");
            create_rich_pcblib(&path);

            let result = server.call_update_primitive(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RICH",
                "primitive_type": "via",
                "index": 7,
                "updates": { "x": 0.0 },
            }));
            assert!(result.is_error);
            assert!(
                get_result_text(&result).contains("Via index 7 out of range"),
                "{}",
                get_result_text(&result)
            );
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

        /// A region carrying a replayed `V7_LAYER` override loses it when its
        /// layer changes, so the saved token names the new layer — `None` on
        /// re-read means the token agrees with the byte.
        #[test]
        fn update_primitive_region_layer_change_drops_stale_v7_token() {
            use crate::altium::pcblib::{Footprint, Pad, Region};

            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let mut region = Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopOverlay);
            region.v7_layer = Some("MECHANICAL4".to_string());
            let mut fp = Footprint::new("RGN");
            fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.6, 0.5));
            fp.add_region(region);
            let mut lib = PcbLib::new();
            lib.add(fp);
            let path = dir.path().join("RegionToken.PcbLib");
            lib.save(&path).unwrap();

            let result = server.call_update_primitive(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RGN",
                "primitive_type": "region",
                "index": 0,
                "updates": { "layer": "Bottom Overlay" },
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));

            let lib = PcbLib::open(&path).unwrap();
            let moved = &lib.get("RGN").unwrap().regions[0];
            assert_eq!(moved.layer, Layer::BottomOverlay);
            assert_eq!(moved.v7_layer, None, "stale token not replayed");
        }

        #[test]
        fn update_primitive_spaceless_layer_aliases_resolve() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Alias.PcbLib");
            create_rich_pcblib(&path);
            let filepath = path.to_string_lossy().to_string();

            // Space-less names bypass Layer::parse and exercise the alias arms.
            // The last member of each numbered family pins the full range now
            // that the aliases delegate to Layer::parse canonical names.
            for (input, expected) in [
                ("MidLayer5", Layer::MidLayer5),
                ("InternalPlane2", Layer::InternalPlane2),
                ("Mechanical10", Layer::Mechanical10),
                ("MidLayer30", Layer::MidLayer30),
                ("InternalPlane16", Layer::InternalPlane16),
                ("Mechanical32", Layer::Mechanical32),
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

            // Out-of-range family numbers must be rejected, not clamped.
            for input in ["Mechanical33", "MidLayer31", "InternalPlane17"] {
                let result = server.call_update_primitive(&json!({
                    "filepath": filepath,
                    "component_name": "RICH",
                    "primitive_type": "track",
                    "index": 0,
                    "updates": { "layer": input },
                }));
                assert!(result.is_error, "{input} must be rejected");
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

    // ==================== error paths ====================

    /// The guard branches the behavioural tests above never reach.
    ///
    /// `update_pad` and `update_primitive` already have their own error-path
    /// tests; `bulk_rename` had none at all despite fourteen error returns, and
    /// `list_backups` and `repair_library` were only ever called with valid
    /// arguments on an existing library.
    mod error_paths {
        use super::*;
        use std::path::Path;

        /// Writes bytes that are not an OLE compound document, standing in for a
        /// truncated or transfer-mangled library file.
        fn write_corrupt(path: &Path) {
            std::fs::write(path, b"not an OLE compound document").expect("write corrupt file");
        }

        // -------------------- bulk_rename --------------------

        #[test]
        fn bulk_rename_requires_filepath_pattern_and_replacement() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let filepath = dir.path().join("Any.PcbLib").to_string_lossy().to_string();

            for (args, want) in [
                (
                    json!({ "pattern": "a", "replacement": "b" }),
                    "Missing required parameter: filepath",
                ),
                (
                    json!({ "filepath": filepath, "replacement": "b" }),
                    "Missing required parameter: pattern",
                ),
                (
                    json!({ "filepath": filepath, "pattern": "a" }),
                    "Missing required parameter: replacement",
                ),
            ] {
                let result = server.call_bulk_rename(&args);
                assert!(result.is_error, "{args} must be rejected");
                assert_eq!(get_result_text(&result), want);
            }
        }

        #[test]
        fn bulk_rename_rejects_a_path_outside_the_allowed_roots() {
            let dir = test_temp_dir();
            let other = test_temp_dir();
            let server = create_test_server(dir.path());
            let outside = other.path().join("Out.PcbLib");
            create_test_pcblib(&outside);

            let result = server.call_bulk_rename(&json!({
                "filepath": outside.to_string_lossy(),
                "pattern": "CHIP",
                "replacement": "PART",
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("Access denied"));
        }

        #[test]
        fn bulk_rename_reports_an_invalid_regex() {
            // The pattern reaches `Regex::new` verbatim, so a user typo must come
            // back as a regex error rather than a panic or a silent no-op.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Rx.PcbLib");
            create_test_pcblib(&path);

            let result = server.call_bulk_rename(&json!({
                "filepath": path.to_string_lossy(),
                "pattern": "CHIP(",
                "replacement": "PART",
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("Invalid regex pattern"));
        }

        #[test]
        fn bulk_rename_rejects_an_unsupported_extension() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let result = server.call_bulk_rename(&json!({
                "filepath": dir.path().join("Notes.txt").to_string_lossy(),
                "pattern": "a",
                "replacement": "b",
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("Unsupported file type"));
        }

        #[test]
        fn bulk_rename_reports_a_corrupt_library_for_both_types() {
            // PcbLib and SchLib open the library on separate arms.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            for name in ["Corrupt.PcbLib", "Corrupt.SchLib"] {
                let path = dir.path().join(name);
                write_corrupt(&path);
                let result = server.call_bulk_rename(&json!({
                    "filepath": path.to_string_lossy(),
                    "pattern": "A",
                    "replacement": "B",
                }));
                assert!(result.is_error, "{name} must fail to read");
                assert!(
                    get_result_text(&result).contains("Failed to read library"),
                    "{name} must name the read failure, got: {}",
                    get_result_text(&result)
                );
            }
        }

        #[test]
        fn bulk_rename_pcblib_reports_rename_conflicts() {
            // The SchLib conflict arm is covered by
            // `bulk_rename_schlib_dry_run_and_conflicts`; the PcbLib arm is a
            // separate branch. Collapsing both fixtures onto one name is the
            // conflict.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Clash.PcbLib");
            create_test_pcblib(&path);

            let result = server.call_bulk_rename(&json!({
                "filepath": path.to_string_lossy(),
                "pattern": "CHIP_0402|CHIP_0603",
                "replacement": "CHIP",
            }));
            assert!(result.is_error);
            assert!(
                get_result_text(&result).contains("Rename conflicts detected"),
                "got: {}",
                get_result_text(&result)
            );
        }

        // -------------------- list_backups --------------------

        #[test]
        fn list_backups_requires_a_filepath() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let result = server.call_list_backups(&json!({}));
            assert!(result.is_error);
            assert_eq!(
                get_result_text(&result),
                "Missing required parameter: filepath"
            );
        }

        #[test]
        fn list_backups_rejects_a_path_outside_the_allowed_roots() {
            let dir = test_temp_dir();
            let other = test_temp_dir();
            let server = create_test_server(dir.path());

            let result = server.call_list_backups(&json!({
                "filepath": other.path().join("Out.PcbLib").to_string_lossy(),
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("Access denied"));
        }

        #[test]
        fn list_backups_rejects_a_missing_parent_directory() {
            // A missing parent must be an error, not an empty backup list — the
            // latter reads as "nothing to restore" when the truth is "wrong path".
            //
            // Note this is caught by `validate_path`, not by the `read_dir` arm
            // further down: by the time the scan runs, the parent has been proven
            // to exist. That arm is defensive against a race or a permission
            // change between the two, which no portable test can stage, so it is
            // deliberately left uncovered rather than forced.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let result = server.call_list_backups(&json!({
                "filepath": dir.path().join("gone").join("Lib.PcbLib").to_string_lossy(),
            }));
            assert!(result.is_error);
            assert!(
                get_result_text(&result).contains("Parent directory"),
                "got: {}",
                get_result_text(&result)
            );
        }

        // -------------------- repair_library --------------------

        #[test]
        fn repair_library_requires_a_filepath() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let result = server.call_repair_library(&json!({}));
            assert!(result.is_error);
            assert_eq!(
                get_result_text(&result),
                "Missing required parameter: filepath"
            );
        }

        #[test]
        fn repair_library_rejects_a_path_outside_the_allowed_roots() {
            let dir = test_temp_dir();
            let other = test_temp_dir();
            let server = create_test_server(dir.path());
            let outside = other.path().join("Out.PcbLib");
            create_test_pcblib(&outside);

            let result = server.call_repair_library(&json!({
                "filepath": outside.to_string_lossy(),
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("Access denied"));
        }

        #[test]
        fn repair_library_reports_a_corrupt_library() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Corrupt.PcbLib");
            write_corrupt(&path);

            let result = server.call_repair_library(&json!({
                "filepath": path.to_string_lossy(),
            }));
            assert!(result.is_error);
            assert!(
                get_result_text(&result).contains("Failed to read library"),
                "got: {}",
                get_result_text(&result)
            );
        }
    }

    // ==================== rejection paths across the six tools ===============
    //
    // Each tool guards its arguments, its file type, the read, the lookup and
    // the write. The fixtures above cover the happy paths; these cover what
    // happens when any of those guards trips.

    mod rejections {
        use crate::mcp::tools::test_support::{
            create_test_pcblib, create_test_schlib, create_test_server, get_result_text,
            parse_result_json, test_temp_dir,
        };
        use serde_json::json;
        use tempfile::TempDir;

        fn write_garbage(path: &std::path::Path) {
            std::fs::write(path, b"not an OLE compound document").unwrap();
        }

        /// A read-only library still opens and still backs up — both only read
        /// it — so the save is what fails.
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

        /// Writes a footprint carrying one of every primitive family, so
        /// `update_primitive` can be driven through each of its arms.
        fn write_rich_library(server: &crate::mcp::server::McpServer, path: &str) {
            let r = server.call_write_pcblib(&json!({
                "filepath": path,
                "footprints": [{
                    "name": "RICH",
                    "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                    "tracks": [{ "x1": -1.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.2, "layer": "Top Overlay" }],
                    "arcs": [{ "x": 0.0, "y": 0.0, "radius": 2.0, "start_angle": 0.0, "end_angle": 90.0, "width": 0.2, "layer": "Top Overlay" }],
                    "text": [{ "x": 0.0, "y": 3.0, "text": "REF", "height": 1.0, "layer": "Top Overlay" }],
                    "fills": [{ "x1": -1.0, "y1": -1.0, "x2": 1.0, "y2": 1.0, "layer": "Top Layer" }],
                    "regions": [{
                        "layer": "Mechanical 1",
                        "vertices": [
                            { "x": -1.0, "y": -1.0 }, { "x": 1.0, "y": -1.0 }, { "x": 0.0, "y": 1.0 },
                        ],
                    }],
                    "vias": [{ "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3 }],
                }],
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
        }

        fn assert_error_mentions(result: &crate::mcp::server::ToolCallResult, needle: &str) {
            let text = get_result_text(result);
            assert!(result.is_error, "expected an error, got: {text}");
            assert!(
                text.contains(needle),
                "expected the error to mention {needle:?}, got: {text}"
            );
        }

        // ---- repair_library ----------------------------------------------------

        #[test]
        fn repair_library_guards_its_path_file_type_and_read() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());

            let escaped = server.call_repair_library(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));

            // Orphaned models are a PcbLib concept, so a symbol library has
            // nothing for this tool to do and is refused rather than no-op'd.
            let schlib = server.call_repair_library(&json!({ "filepath": fx.path("Lib.SchLib") }));
            assert_error_mentions(&schlib, "only supports .PcbLib");

            let unreadable =
                server.call_repair_library(&json!({ "filepath": fx.path("Bad.PcbLib") }));
            assert_error_mentions(&unreadable, "Failed to read library");
        }

        // ---- bulk_rename -------------------------------------------------------

        #[test]
        fn bulk_rename_names_each_missing_argument_and_bad_input() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());

            let no_pattern = server.call_bulk_rename(&json!({
                "filepath": fx.path("Lib.PcbLib"), "replacement": "X",
            }));
            assert_error_mentions(&no_pattern, "pattern");

            let no_replacement = server.call_bulk_rename(&json!({
                "filepath": fx.path("Lib.PcbLib"), "pattern": "CHIP",
            }));
            assert_error_mentions(&no_replacement, "replacement");

            let escaped = server.call_bulk_rename(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
                "pattern": "A", "replacement": "B",
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));

            // An unbalanced bracket is a caller error, not a panic.
            let bad_regex = server.call_bulk_rename(&json!({
                "filepath": fx.path("Lib.PcbLib"), "pattern": "CHIP_[0-9", "replacement": "X",
            }));
            assert_error_mentions(&bad_regex, "Invalid regex pattern");

            let wrong_ext = server.call_bulk_rename(&json!({
                "filepath": fx.path("Lib.txt"), "pattern": "A", "replacement": "B",
            }));
            assert_error_mentions(&wrong_ext, "Unsupported file type");
        }

        #[test]
        fn bulk_rename_reports_unreadable_libraries_of_both_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            for lib in ["Bad.PcbLib", "Bad.SchLib"] {
                let r = server.call_bulk_rename(&json!({
                    "filepath": fx.path(lib), "pattern": "A", "replacement": "B",
                }));
                assert_error_mentions(&r, "Failed to read library");
            }
        }

        #[test]
        fn bulk_rename_refuses_to_collide_two_components_into_one_name() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            // Renaming onto a name the library already holds, where that name
            // is not itself moving out of the way.
            for (lib, pattern, replacement) in [
                ("Lib.PcbLib", "CHIP_0402", "CHIP_0603"),
                ("Lib.SchLib", "RESISTOR", "CAPACITOR"),
            ] {
                let r = server.call_bulk_rename(&json!({
                    "filepath": fx.path(lib), "pattern": pattern, "replacement": replacement,
                }));
                assert_error_mentions(&r, "already exists");
            }

            // Two components collapsing onto the same new name is caught even
            // though neither target exists yet.
            let collapse = server.call_bulk_rename(&json!({
                "filepath": fx.path("Lib.PcbLib"), "pattern": r"CHIP_\d+", "replacement": "CHIP",
            }));
            assert_error_mentions(&collapse, "conflict");
        }

        #[test]
        fn bulk_rename_reports_a_failed_write_for_both_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for (lib, pattern, replacement) in [
                ("Lib.PcbLib", "CHIP_0402", "PART_A"),
                ("Lib.SchLib", "RESISTOR", "PART_A"),
            ] {
                let path = fx.path(lib);
                block_save(std::path::Path::new(&path), true);
                let r = server.call_bulk_rename(&json!({
                    "filepath": &path, "pattern": pattern, "replacement": replacement,
                }));
                block_save(std::path::Path::new(&path), false);
                assert_error_mentions(&r, "Failed to save library");
            }
        }

        // ---- list_backups / restore_backup -------------------------------------

        #[test]
        fn list_backups_reports_only_properly_stamped_files_newest_first() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let lib = fx.path("Lib.PcbLib");

            // Two real backups, plus three near-misses that must not be listed:
            // no timestamp segment, a malformed stamp, and a different library.
            for name in [
                "Lib.PcbLib.20260101_010101.bak",
                "Lib.PcbLib.20260202_020202.bak",
                "Lib.PcbLib.bak",
                "Lib.PcbLib.not-a-stamp.bak",
                "Other.PcbLib.20260303_030303.bak",
            ] {
                std::fs::write(fx.dir.path().join(name), b"backup").unwrap();
            }

            let r = server.call_list_backups(&json!({ "filepath": &lib }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let parsed = parse_result_json(&r);
            assert_eq!(parsed["backup_count"], 2, "{parsed}");
            // Sorted newest first, so the caller can restore [0] blindly.
            assert_eq!(parsed["backups"][0]["timestamp"], "20260202_020202");
            assert_eq!(parsed["backups"][1]["timestamp"], "20260101_010101");
            assert!(parsed["backups"][0]["size_bytes"].as_u64().unwrap() > 0);
        }

        #[test]
        fn list_backups_rejects_a_path_outside_the_allowlist() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());
            let r = server.call_list_backups(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
            }));
            assert!(r.is_error, "{}", get_result_text(&r));
        }

        #[test]
        fn restore_backup_picks_the_newest_stamp_when_none_is_named() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let lib = fx.path("Lib.PcbLib");

            std::fs::write(
                fx.dir.path().join("Lib.PcbLib.20260101_010101.bak"),
                b"older",
            )
            .unwrap();
            std::fs::write(
                fx.dir.path().join("Lib.PcbLib.20260202_020202.bak"),
                b"newer",
            )
            .unwrap();
            // Skipped: no stamp segment, and a stamp of the wrong shape.
            std::fs::write(fx.dir.path().join("Lib.PcbLib.bak"), b"unstamped").unwrap();
            std::fs::write(fx.dir.path().join("Lib.PcbLib.xxxxxxxxxxxxxxx.bak"), b"bad").unwrap();

            let r = server.call_restore_backup(&json!({ "filepath": &lib }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            assert_eq!(std::fs::read(&lib).unwrap(), b"newer");
        }

        #[test]
        fn restore_backup_reports_when_there_is_nothing_to_restore() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());

            let escaped = server.call_restore_backup(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));

            // A named backup is gated separately from the library path, since
            // it is read from disk too.
            let escaped_backup = server.call_restore_backup(&json!({
                "filepath": fx.path("Lib.PcbLib"),
                "backup_path": outside.path().join("X.bak").to_string_lossy(),
            }));
            assert!(
                escaped_backup.is_error,
                "{}",
                get_result_text(&escaped_backup)
            );

            let none = server.call_restore_backup(&json!({ "filepath": fx.path("Lib.PcbLib") }));
            assert_error_mentions(&none, "No backup files found");

            let missing = server.call_restore_backup(&json!({
                "filepath": fx.path("Lib.PcbLib"), "backup_path": fx.path("Nope.bak"),
            }));
            assert_error_mentions(&missing, "does not exist");
        }

        // ---- update_pad ---------------------------------------------------------

        #[test]
        fn update_pad_names_each_missing_argument() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());
            let lib = fx.path("Lib.PcbLib");

            let no_component = server.call_update_pad(&json!({
                "filepath": &lib, "designator": "1", "updates": { "x": 1.0 },
            }));
            assert_error_mentions(&no_component, "component_name");

            let no_designator = server.call_update_pad(&json!({
                "filepath": &lib, "component_name": "CHIP_0402", "updates": { "x": 1.0 },
            }));
            assert_error_mentions(&no_designator, "designator");

            let no_updates = server.call_update_pad(&json!({
                "filepath": &lib, "component_name": "CHIP_0402", "designator": "1",
            }));
            assert_error_mentions(&no_updates, "updates");

            let escaped = server.call_update_pad(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
                "component_name": "A", "designator": "1", "updates": { "x": 1.0 },
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));
        }

        #[test]
        fn update_pad_reports_an_unreadable_library_and_a_missing_target() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            let unreadable = server.call_update_pad(&json!({
                "filepath": fx.path("Bad.PcbLib"), "component_name": "A",
                "designator": "1", "updates": { "x": 1.0 },
            }));
            assert_error_mentions(&unreadable, "Failed to read library");

            // Both lookups list what was available, so the caller can correct
            // the request without a second round trip.
            let no_footprint = server.call_update_pad(&json!({
                "filepath": fx.path("Lib.PcbLib"), "component_name": "GHOST",
                "designator": "1", "updates": { "x": 1.0 },
            }));
            assert_error_mentions(&no_footprint, "Available");

            let no_pad = server.call_update_pad(&json!({
                "filepath": fx.path("Lib.PcbLib"), "component_name": "CHIP_0402",
                "designator": "99", "updates": { "x": 1.0 },
            }));
            assert_error_mentions(&no_pad, "not found in footprint");
        }

        #[test]
        fn update_pad_rejects_geometry_the_create_path_would_have_refused() {
            // `update` bypasses the create-path checks, so it repeats them:
            // otherwise a degenerate pad or an out-of-range coordinate would
            // saturate silently on save.
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let lib = fx.path("Lib.PcbLib");

            let update = |updates: serde_json::Value| {
                server.call_update_pad(&json!({
                    "filepath": &lib, "component_name": "CHIP_0402",
                    "designator": "1", "updates": updates, "dry_run": true,
                }))
            };

            assert_error_mentions(&update(json!({ "width": 0.0 })), "must be positive");
            assert_error_mentions(&update(json!({ "height": -1.0 })), "must be positive");
            assert_error_mentions(&update(json!({ "hole_size": -0.5 })), "must be >= 0");
            assert_error_mentions(&update(json!({ "shape": "trapezoid" })), "Invalid shape");
            assert_error_mentions(&update(json!({ "unknown_key": 1 })), "Unknown field");
            assert_error_mentions(&update(json!({})), "No valid updates");
            assert_error_mentions(
                &update(json!({ "x": 99_999.0 })),
                "exceeds the maximum safe range",
            );
        }

        #[test]
        fn update_pad_applies_every_property_and_reports_a_failed_write() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let lib = fx.path("Lib.PcbLib");

            let applied = server.call_update_pad(&json!({
                "filepath": &lib, "component_name": "CHIP_0402", "designator": "1",
                "updates": {
                    "x": 0.1, "y": 0.2, "width": 0.7, "height": 0.6,
                    "rotation": 90.0, "hole_size": 0.4, "shape": "round",
                },
            }));
            assert!(!applied.is_error, "{}", get_result_text(&applied));
            let changed: Vec<String> = parse_result_json(&applied)["changes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["property"].as_str().unwrap().to_string())
                .collect();
            for property in [
                "x",
                "y",
                "width",
                "height",
                "rotation",
                "hole_size",
                "shape",
            ] {
                assert!(changed.contains(&property.to_string()), "{changed:?}");
            }

            block_save(std::path::Path::new(&lib), true);
            let blocked = server.call_update_pad(&json!({
                "filepath": &lib, "component_name": "CHIP_0402",
                "designator": "1", "updates": { "x": 0.3 },
            }));
            block_save(std::path::Path::new(&lib), false);
            assert!(blocked.is_error, "{}", get_result_text(&blocked));
        }

        // ---- update_primitive ---------------------------------------------------

        #[test]
        fn update_primitive_names_each_missing_argument() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());
            let lib = fx.path("Lib.PcbLib");

            let no_component = server.call_update_primitive(&json!({
                "filepath": &lib, "primitive_type": "track", "index": 0, "updates": { "width": 1.0 },
            }));
            assert_error_mentions(&no_component, "component_name");

            let no_type = server.call_update_primitive(&json!({
                "filepath": &lib, "component_name": "CHIP_0402", "index": 0, "updates": { "width": 1.0 },
            }));
            assert_error_mentions(&no_type, "primitive_type");

            let no_index = server.call_update_primitive(&json!({
                "filepath": &lib, "component_name": "CHIP_0402",
                "primitive_type": "track", "updates": { "width": 1.0 },
            }));
            assert_error_mentions(&no_index, "index");

            let no_updates = server.call_update_primitive(&json!({
                "filepath": &lib, "component_name": "CHIP_0402",
                "primitive_type": "track", "index": 0,
            }));
            assert_error_mentions(&no_updates, "updates");

            let escaped = server.call_update_primitive(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
                "component_name": "A", "primitive_type": "track", "index": 0,
                "updates": { "width": 1.0 },
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));
        }

        #[test]
        fn update_primitive_reports_an_unreadable_library_and_a_missing_footprint() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            let unreadable = server.call_update_primitive(&json!({
                "filepath": fx.path("Bad.PcbLib"), "component_name": "A",
                "primitive_type": "track", "index": 0, "updates": { "width": 1.0 },
            }));
            assert_error_mentions(&unreadable, "Failed to read library");

            let missing = server.call_update_primitive(&json!({
                "filepath": fx.path("Lib.PcbLib"), "component_name": "GHOST",
                "primitive_type": "track", "index": 0, "updates": { "width": 1.0 },
            }));
            assert_error_mentions(&missing, "Available");
        }

        #[test]
        fn update_primitive_bounds_checks_every_family_and_rejects_unknown_ones() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let lib = fx.path("Rich.PcbLib");
            write_rich_library(&server, &lib);

            // Each family is addressed positionally, so each keeps its own
            // range check naming the family.
            for family in ["track", "arc", "text", "fill", "region", "via"] {
                let updates = if family == "via" {
                    json!({ "diameter": 0.6 })
                } else {
                    json!({ "layer": "Top Layer" })
                };
                let r = server.call_update_primitive(&json!({
                    "filepath": &lib, "component_name": "RICH",
                    "primitive_type": family, "index": 99, "updates": updates,
                }));
                assert_error_mentions(&r, "out of range");
            }

            let unknown = server.call_update_primitive(&json!({
                "filepath": &lib, "component_name": "RICH",
                "primitive_type": "polygon", "index": 0, "updates": { "layer": "Top Layer" },
            }));
            assert_error_mentions(&unknown, "Invalid primitive_type");
        }

        #[test]
        fn update_primitive_rejects_an_unknown_layer_on_every_family() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let lib = fx.path("Rich.PcbLib");
            write_rich_library(&server, &lib);

            for family in ["track", "arc", "text", "fill", "region"] {
                let r = server.call_update_primitive(&json!({
                    "filepath": &lib, "component_name": "RICH",
                    "primitive_type": family, "index": 0,
                    "updates": { "layer": "Nowhere" }, "dry_run": true,
                }));
                assert_error_mentions(&r, "Invalid layer");
            }

            // A via spans two layers, so both ends are parsed and both reject.
            for key in ["from_layer", "to_layer"] {
                let r = server.call_update_primitive(&json!({
                    "filepath": &lib, "component_name": "RICH",
                    "primitive_type": "via", "index": 0,
                    "updates": { key: "Nowhere" }, "dry_run": true,
                }));
                assert_error_mentions(&r, "Invalid layer");
            }
        }

        #[test]
        fn update_primitive_moves_a_track_and_reports_every_change() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let lib = fx.path("Rich.PcbLib");
            write_rich_library(&server, &lib);

            let r = server.call_update_primitive(&json!({
                "filepath": &lib, "component_name": "RICH", "primitive_type": "track", "index": 0,
                "updates": {
                    "x1": -2.0, "y1": -0.5, "x2": 2.0, "y2": 0.5,
                    "width": 0.3, "layer": "Bottom Overlay",
                },
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let changed: Vec<String> = parse_result_json(&r)["changes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["property"].as_str().unwrap().to_string())
                .collect();
            for property in ["x1", "y1", "x2", "y2", "width", "layer"] {
                assert!(changed.contains(&property.to_string()), "{changed:?}");
            }
        }

        #[test]
        fn update_primitive_rejects_degenerate_geometry_per_family() {
            // The same reasoning as update_pad: these checks mirror the create
            // path, which the update route bypasses.
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let lib = fx.path("Rich.PcbLib");
            write_rich_library(&server, &lib);

            let update = |family: &str, updates: serde_json::Value| {
                server.call_update_primitive(&json!({
                    "filepath": &lib, "component_name": "RICH",
                    "primitive_type": family, "index": 0, "updates": updates, "dry_run": true,
                }))
            };

            assert_error_mentions(
                &update("track", json!({ "width": 0.0 })),
                "must be positive",
            );
            assert_error_mentions(&update("arc", json!({ "radius": 0.0 })), "must be positive");
            assert_error_mentions(&update("arc", json!({ "width": -1.0 })), "must be >= 0");
            assert_error_mentions(
                &update("text", json!({ "height": 0.0 })),
                "must be positive",
            );
            assert_error_mentions(
                &update("via", json!({ "diameter": 0.0 })),
                "must be positive",
            );
            assert_error_mentions(
                &update("via", json!({ "hole_size": 0.0 })),
                "must be positive",
            );
            assert_error_mentions(
                &update("via", json!({ "hole_size": 0.9 })),
                "smaller than diameter",
            );

            // A key the family does not take is refused (a typo, or another
            // family's property); nothing at all is a caller error rather than
            // a silent no-op write.
            assert_error_mentions(
                &update("track", json!({ "nope": 1 })),
                "Unknown field 'nope'",
            );
            assert_error_mentions(
                &update("region", json!({ "width": 0.2 })),
                "Unknown field 'width'",
            );
            assert_error_mentions(&update("track", json!({})), "No valid updates");

            // And the whole-footprint coordinate check still applies.
            assert_error_mentions(
                &update("track", json!({ "x1": 99_999.0 })),
                "exceeds the maximum safe range",
            );
        }

        #[test]
        fn update_primitive_reports_a_failed_write() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let lib = fx.path("Rich.PcbLib");
            write_rich_library(&server, &lib);

            block_save(std::path::Path::new(&lib), true);
            let r = server.call_update_primitive(&json!({
                "filepath": &lib, "component_name": "RICH",
                "primitive_type": "track", "index": 0, "updates": { "width": 0.3 },
            }));
            block_save(std::path::Path::new(&lib), false);
            assert!(r.is_error, "{}", get_result_text(&r));
        }
    }

    /// A primary edit reaches only the per-layer entries that shared the old
    /// value: a height change follows where the height matched, a shape
    /// change where the shape matched, and a layer with its own value keeps
    /// it.
    #[test]
    fn a_stacked_pad_follows_a_height_and_a_shape_edit_where_layers_matched() {
        let mut pad = Pad::smd("1", 0.0, 0.0, 1.0, 2.0);
        pad.stack_mode = PadStackMode::TopMiddleBottom;
        pad.per_layer_sizes = Some(vec![(1.0, 2.0), (1.0, 2.0), (0.8, 1.5)]);
        pad.per_layer_shapes = Some(vec![PadShape::Round, PadShape::Round, PadShape::Rectangle]);

        // The edit: height 2.0 -> 2.6, shape Round -> Oval; width untouched.
        pad.height = 2.6;
        pad.shape = PadShape::Oval;
        let followed = McpServer::propagate_pad_edit_to_stack(&mut pad, 1.0, 2.0, PadShape::Round);

        assert_eq!(followed, 4, "two heights and two shapes followed");
        assert_eq!(
            pad.per_layer_sizes.as_ref().unwrap(),
            &vec![(1.0, 2.6), (1.0, 2.6), (0.8, 1.5)],
            "the layer with its own height keeps it"
        );
        assert_eq!(
            pad.per_layer_shapes.as_ref().unwrap(),
            &vec![PadShape::Oval, PadShape::Oval, PadShape::Rectangle],
            "the layer with its own shape keeps it"
        );
    }

    /// A via edit that leaves the diameter as it was touches no layer.
    #[test]
    fn a_via_edit_without_a_diameter_change_touches_no_layer() {
        let mut via = Via::new(0.0, 0.0, 0.6, 0.3);
        via.diameter_stack_mode = ViaStackMode::TopMiddleBottom;
        via.per_layer_diameters = Some(vec![0.6, 0.6, 0.6]);
        let followed = McpServer::propagate_via_edit_to_stack(&mut via, 0.6);
        assert_eq!(followed, 0);
        assert_eq!(
            via.per_layer_diameters.as_ref().unwrap(),
            &vec![0.6, 0.6, 0.6]
        );
    }
}
