//! Delete/validate/export/import tools. Split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

impl McpServer {
    // ==================== Library Management Tools ====================

    /// Deletes one or more components from a library file.
    ///
    /// Supports both `.PcbLib` and `.SchLib` files. The file type is auto-detected
    /// from the extension. Returns per-component status (`deleted`, `not_found`, or `error`).
    pub(crate) fn call_delete_component(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(component_names) = arguments.get("component_names").and_then(Value::as_array)
        else {
            return ToolCallResult::error("Missing required parameter: component_names");
        };

        let names: Vec<&str> = component_names.iter().filter_map(Value::as_str).collect();

        if names.is_empty() {
            return ToolCallResult::error("component_names array is empty or contains no strings");
        }

        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Determine file type from extension
        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => Self::delete_from_pcblib(filepath, &names, dry_run),
            Some("schlib") => Self::delete_from_schlib(filepath, &names, dry_run),
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

    /// Deletes components from a `PcbLib` file.
    pub(crate) fn delete_from_pcblib(
        filepath: &str,
        names: &[&str],
        dry_run: bool,
    ) -> ToolCallResult {
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

        let original_count = library.len();
        let mut results: Vec<Value> = Vec::with_capacity(names.len());
        let mut deleted_count = 0;

        // Check which components exist (for dry_run) or remove them
        for name in names {
            if dry_run {
                // In dry-run mode, just check if component exists
                if library.get(name).is_some() {
                    results.push(json!({
                        "name": name,
                        "status": "would_delete"
                    }));
                    deleted_count += 1;
                } else {
                    results.push(json!({
                        "name": name,
                        "status": "not_found"
                    }));
                }
            } else if library.remove(name).is_some() {
                results.push(json!({
                    "name": name,
                    "status": "deleted"
                }));
                deleted_count += 1;
            } else {
                results.push(json!({
                    "name": name,
                    "status": "not_found"
                }));
            }
        }

        // Clean up orphaned embedded models after deleting footprints
        let orphaned_models_removed = if deleted_count > 0 && !dry_run {
            library.remove_orphaned_models()
        } else {
            0
        };

        // Only write if something was deleted (and not dry-run)
        if deleted_count > 0 && !dry_run {
            // Create backup before destructive operation
            if let Err(e) = Self::create_backup(filepath) {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e,
                    "results": results,
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }

            if let Err(e) = library.save(filepath) {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": format!("Failed to write library: {e}"),
                    "results": results,
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        }

        let mut result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "filepath": filepath,
            "file_type": "PcbLib",
            "dry_run": dry_run,
            "original_count": original_count,
            "deleted_count": deleted_count,
            "remaining_count": if dry_run { original_count - deleted_count } else { library.len() },
            "orphaned_models_removed": orphaned_models_removed,
            "results": results,
        });

        // Run post-write validation (only if actual changes were made)
        if deleted_count > 0 && !dry_run {
            if let Some(validation) = Self::post_write_validation_pcblib(filepath) {
                result["validation"] = validation;
            }
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Deletes components from a `SchLib` file.
    pub(crate) fn delete_from_schlib(
        filepath: &str,
        names: &[&str],
        dry_run: bool,
    ) -> ToolCallResult {
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

        let original_count = library.len();
        let mut results: Vec<Value> = Vec::with_capacity(names.len());
        let mut deleted_count = 0;

        // Check which components exist (for dry_run) or remove them
        for name in names {
            if dry_run {
                // In dry-run mode, just check if component exists
                if library.get(name).is_some() {
                    results.push(json!({
                        "name": name,
                        "status": "would_delete"
                    }));
                    deleted_count += 1;
                } else {
                    results.push(json!({
                        "name": name,
                        "status": "not_found"
                    }));
                }
            } else if library.remove(name).is_some() {
                results.push(json!({
                    "name": name,
                    "status": "deleted"
                }));
                deleted_count += 1;
            } else {
                results.push(json!({
                    "name": name,
                    "status": "not_found"
                }));
            }
        }

        // Only write if something was deleted (and not dry-run)
        if deleted_count > 0 && !dry_run {
            // Create backup before destructive operation
            if let Err(e) = Self::create_backup(filepath) {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e,
                    "results": results,
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }

            if let Err(e) = library.save(filepath) {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": format!("Failed to write library: {e}"),
                    "results": results,
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        }

        let mut result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "filepath": filepath,
            "file_type": "SchLib",
            "dry_run": dry_run,
            "original_count": original_count,
            "deleted_count": deleted_count,
            "remaining_count": if dry_run { original_count - deleted_count } else { library.len() },
            "results": results,
        });

        // Run post-write validation (only if actual changes were made)
        if deleted_count > 0 && !dry_run {
            if let Some(validation) = Self::post_write_validation_schlib(filepath) {
                result["validation"] = validation;
            }
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    // ==================== Library Validation Tools ====================

    /// Validates an Altium library file for common issues.
    ///
    /// Checks for empty components, duplicate designators, invalid coordinates,
    /// zero-size primitives, and other integrity problems.
    pub(crate) fn call_validate_library(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Determine file type from extension
        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => Self::validate_pcblib(filepath),
            Some("schlib") => Self::validate_schlib(filepath),
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

    /// Validates a `PcbLib` file.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn validate_pcblib(filepath: &str) -> ToolCallResult {
        use crate::altium::PcbLib;
        use std::collections::HashSet;

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

        let mut issues: Vec<Value> = Vec::new();
        let component_count = library.len();

        // Check for empty library
        if component_count == 0 {
            issues.push(json!({
                "severity": "warning",
                "component": null,
                "issue": "Library is empty (no footprints)"
            }));
        }

        // Validate each footprint
        for fp in library.iter() {
            let name = &fp.name;

            // Check for empty name
            if name.is_empty() {
                issues.push(json!({
                    "severity": "error",
                    "component": name,
                    "issue": "Footprint has empty name"
                }));
            }

            // Check for no pads
            if fp.pads.is_empty() {
                issues.push(json!({
                    "severity": "warning",
                    "component": name,
                    "issue": "Footprint has no pads"
                }));
            }

            // Check for duplicate pad designators
            let mut seen_designators: HashSet<&str> = HashSet::new();
            for pad in &fp.pads {
                if !seen_designators.insert(&pad.designator) {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Duplicate pad designator: '{}'", pad.designator)
                    }));
                }

                // Check for empty designator
                if pad.designator.is_empty() {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": "Pad has empty designator"
                    }));
                }

                // Check for zero or negative dimensions
                if pad.width <= 0.0 || pad.height <= 0.0 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Pad '{}' has invalid dimensions (width: {}, height: {})",
                            pad.designator, pad.width, pad.height)
                    }));
                }

                // Check for invalid coordinates
                if !pad.x.is_finite() || !pad.y.is_finite() {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Pad '{}' has invalid coordinates (x: {}, y: {})",
                            pad.designator, pad.x, pad.y)
                    }));
                }
            }

            // Check tracks for invalid values
            for (i, track) in fp.tracks.iter().enumerate() {
                if track.width <= 0.0 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Track {} has invalid width: {}", i, track.width)
                    }));
                }
                if !track.x1.is_finite()
                    || !track.y1.is_finite()
                    || !track.x2.is_finite()
                    || !track.y2.is_finite()
                {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Track {} has invalid coordinates", i)
                    }));
                }
            }

            // Check arcs for invalid values
            for (i, arc) in fp.arcs.iter().enumerate() {
                if arc.radius <= 0.0 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Arc {} has invalid radius: {}", i, arc.radius)
                    }));
                }
                if !arc.x.is_finite() || !arc.y.is_finite() {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Arc {} has invalid centre coordinates", i)
                    }));
                }
            }

            // Check regions for minimum vertices
            for (i, region) in fp.regions.iter().enumerate() {
                if region.vertices.len() < 3 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Region {} has fewer than 3 vertices", i)
                    }));
                }
            }
        }

        let error_count = issues.iter().filter(|i| i["severity"] == "error").count();
        let warning_count = issues.iter().filter(|i| i["severity"] == "warning").count();

        let result = json!({
            "status": if error_count > 0 { "invalid" } else if warning_count > 0 { "warnings" } else { "valid" },
            "filepath": filepath,
            "file_type": "PcbLib",
            "component_count": component_count,
            "error_count": error_count,
            "warning_count": warning_count,
            "issues": issues,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Validates a `SchLib` file.
    pub(crate) fn validate_schlib(filepath: &str) -> ToolCallResult {
        use crate::altium::SchLib;
        use std::collections::HashSet;

        // Read the library
        let library = match SchLib::open(filepath) {
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

        let mut issues: Vec<Value> = Vec::new();
        let component_count = library.len();

        // Check for empty library
        if component_count == 0 {
            issues.push(json!({
                "severity": "warning",
                "component": null,
                "issue": "Library is empty (no symbols)"
            }));
        }

        // Validate each symbol
        for symbol in library.iter() {
            let name = &symbol.name;
            // Check for empty name
            if name.is_empty() {
                issues.push(json!({
                    "severity": "error",
                    "component": name,
                    "issue": "Symbol has empty name"
                }));
            }

            // Check for no pins
            if symbol.pins.is_empty() {
                issues.push(json!({
                    "severity": "warning",
                    "component": name,
                    "issue": "Symbol has no pins"
                }));
            }

            // Check for duplicate pin designators
            let mut seen_designators: HashSet<&str> = HashSet::new();
            for pin in &symbol.pins {
                if !seen_designators.insert(&pin.designator) {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Duplicate pin designator: '{}'", pin.designator)
                    }));
                }

                // Check for empty designator
                if pin.designator.is_empty() {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": "Pin has empty designator"
                    }));
                }

                // Check for zero or negative pin length
                if pin.length <= 0 {
                    issues.push(json!({
                        "severity": "warning",
                        "component": name,
                        "issue": format!("Pin '{}' has zero or negative length: {}",
                            pin.designator, pin.length)
                    }));
                }
            }

            // Check rectangles for inverted corners
            for (i, rect) in symbol.rectangles.iter().enumerate() {
                if rect.x1 > rect.x2 || rect.y1 > rect.y2 {
                    issues.push(json!({
                        "severity": "warning",
                        "component": name,
                        "issue": format!("Rectangle {} has inverted corners (x1={}, y1={}, x2={}, y2={})",
                            i, rect.x1, rect.y1, rect.x2, rect.y2)
                    }));
                }
            }

            // Check for symbols with no body (no rectangles, lines, or other graphics)
            let has_body = !symbol.rectangles.is_empty()
                || !symbol.lines.is_empty()
                || !symbol.polylines.is_empty()
                || !symbol.arcs.is_empty()
                || !symbol.ellipses.is_empty();

            if !has_body && !symbol.pins.is_empty() {
                issues.push(json!({
                    "severity": "warning",
                    "component": name,
                    "issue": "Symbol has pins but no body graphics (rectangles, lines, etc.)"
                }));
            }
        }

        let error_count = issues.iter().filter(|i| i["severity"] == "error").count();
        let warning_count = issues.iter().filter(|i| i["severity"] == "warning").count();

        let result = json!({
            "status": if error_count > 0 { "invalid" } else if warning_count > 0 { "warnings" } else { "valid" },
            "filepath": filepath,
            "file_type": "SchLib",
            "component_count": component_count,
            "error_count": error_count,
            "warning_count": warning_count,
            "issues": issues,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Runs post-write validation on a `PcbLib` file and returns validation info.
    ///
    /// Returns a JSON value with validation results that can be included in write operation responses.
    /// Returns `None` if the file cannot be read (which would indicate a serious write failure).
    pub(crate) fn post_write_validation_pcblib(filepath: &str) -> Option<Value> {
        use crate::altium::PcbLib;
        use std::collections::HashSet;

        let library = PcbLib::open(filepath).ok()?;
        let mut issues: Vec<Value> = Vec::new();

        for fp in library.iter() {
            let name = &fp.name;

            // Check for empty name
            if name.is_empty() {
                issues.push(json!({
                    "severity": "error",
                    "component": name,
                    "issue": "Footprint has empty name"
                }));
            }

            // Check for duplicate pad designators
            let mut seen_designators: HashSet<&str> = HashSet::new();
            for pad in &fp.pads {
                if !seen_designators.insert(&pad.designator) {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Duplicate pad designator: '{}'", pad.designator)
                    }));
                }

                // Check for zero or negative dimensions
                if pad.width <= 0.0 || pad.height <= 0.0 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Pad '{}' has invalid dimensions", pad.designator)
                    }));
                }
            }

            // Check tracks for invalid values
            for (i, track) in fp.tracks.iter().enumerate() {
                if track.width <= 0.0 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Track {} has invalid width", i)
                    }));
                }
            }

            // Check arcs for invalid values
            for (i, arc) in fp.arcs.iter().enumerate() {
                if arc.radius <= 0.0 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Arc {} has invalid radius", i)
                    }));
                }
            }

            // Check regions for minimum vertices
            for (i, region) in fp.regions.iter().enumerate() {
                if region.vertices.len() < 3 {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Region {} has fewer than 3 vertices", i)
                    }));
                }
            }
        }

        let error_count = issues.iter().filter(|i| i["severity"] == "error").count();
        let warning_count = issues.iter().filter(|i| i["severity"] == "warning").count();

        Some(json!({
            "status": if error_count > 0 { "invalid" } else if warning_count > 0 { "warnings" } else { "valid" },
            "error_count": error_count,
            "warning_count": warning_count,
            "issues": issues,
        }))
    }

    /// Runs post-write validation on a `SchLib` file and returns validation info.
    ///
    /// Returns a JSON value with validation results that can be included in write operation responses.
    /// Returns `None` if the file cannot be read (which would indicate a serious write failure).
    pub(crate) fn post_write_validation_schlib(filepath: &str) -> Option<Value> {
        use crate::altium::SchLib;
        use std::collections::HashSet;

        let library = SchLib::open(filepath).ok()?;
        let mut issues: Vec<Value> = Vec::new();

        for symbol in library.iter() {
            let name = &symbol.name;

            // Check for empty name
            if name.is_empty() {
                issues.push(json!({
                    "severity": "error",
                    "component": name,
                    "issue": "Symbol has empty name"
                }));
            }

            // Check for duplicate pin designators
            let mut seen_designators: HashSet<&str> = HashSet::new();
            for pin in &symbol.pins {
                if !seen_designators.insert(&pin.designator) {
                    issues.push(json!({
                        "severity": "error",
                        "component": name,
                        "issue": format!("Duplicate pin designator: '{}'", pin.designator)
                    }));
                }
            }

            // Check rectangles for inverted corners
            for (i, rect) in symbol.rectangles.iter().enumerate() {
                if rect.x1 > rect.x2 || rect.y1 > rect.y2 {
                    issues.push(json!({
                        "severity": "warning",
                        "component": name,
                        "issue": format!("Rectangle {} has inverted corners", i)
                    }));
                }
            }
        }

        let error_count = issues.iter().filter(|i| i["severity"] == "error").count();
        let warning_count = issues.iter().filter(|i| i["severity"] == "warning").count();

        Some(json!({
            "status": if error_count > 0 { "invalid" } else if warning_count > 0 { "warnings" } else { "valid" },
            "error_count": error_count,
            "warning_count": warning_count,
            "issues": issues,
        }))
    }

    // ==================== Library Export Tools ====================

    /// Exports an Altium library to JSON or CSV format.
    pub(crate) fn call_export_library(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(format) = arguments.get("format").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: format");
        };

        let format_lower = format.to_lowercase();
        if format_lower != "json" && format_lower != "csv" {
            return ToolCallResult::error("Invalid format. Expected 'json' or 'csv'.");
        }

        // Parse compact parameter (default: true)
        let compact = arguments
            .get("compact")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        // Determine file type from extension
        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => Self::export_pcblib(filepath, &format_lower, compact),
            Some("schlib") => Self::export_schlib(filepath, &format_lower),
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

    /// Exports a `PcbLib` file to JSON or CSV.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn export_pcblib(filepath: &str, format: &str, compact: bool) -> ToolCallResult {
        use crate::altium::pcblib::primitives::PadStackMode;
        use crate::altium::PcbLib;

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

        if format == "json" {
            // Full JSON export
            let footprints: Vec<Value> = library
                .iter()
                .map(|fp| {
                    // If compact mode, strip per-layer data when it's redundant
                    let pads: Vec<Value> = if compact {
                        fp.pads
                            .iter()
                            .map(|pad| {
                                let mut pad_json = serde_json::to_value(pad).unwrap();
                                // Remove per-layer data if stack_mode is Simple OR all values are uniform
                                let should_strip = pad.stack_mode == PadStackMode::Simple
                                    || Self::pad_has_uniform_per_layer_data(pad);
                                if should_strip {
                                    if let Value::Object(ref mut obj) = pad_json {
                                        obj.remove("per_layer_sizes");
                                        obj.remove("per_layer_shapes");
                                        obj.remove("per_layer_corner_radii");
                                        obj.remove("per_layer_offsets");
                                        // Downgrade stack_mode to simple if we stripped uniform data
                                        if pad.stack_mode != PadStackMode::Simple {
                                            obj.insert("stack_mode".to_string(), json!("simple"));
                                        }
                                    }
                                }
                                pad_json
                            })
                            .collect()
                    } else {
                        fp.pads
                            .iter()
                            .map(|p| serde_json::to_value(p).unwrap())
                            .collect()
                    };

                    json!({
                        "name": fp.name,
                        "description": fp.description,
                        "pads": pads,
                        "tracks": fp.tracks,
                        "arcs": fp.arcs,
                        "regions": fp.regions,
                        "text": fp.text,
                        "model_3d": fp.model_3d,
                        "component_bodies": fp.component_bodies,
                    })
                })
                .collect();

            let result = json!({
                "status": "success",
                "filepath": filepath,
                "file_type": "PcbLib",
                "format": "json",
                "units": "mm",
                "component_count": library.len(),
                "footprints": footprints,
            });

            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
        } else {
            // CSV export - summary table
            let mut csv_lines: Vec<String> = Vec::new();
            csv_lines.push("name,description,pad_count,track_count,arc_count,region_count,text_count,external_3d_model,embedded_3d_bodies".to_string());

            for fp in library.iter() {
                let has_external_model = if fp.model_3d.is_some() { "yes" } else { "no" };
                let embedded_body_count = fp.component_bodies.len();
                csv_lines.push(format!(
                    "{},{},{},{},{},{},{},{},{}",
                    crate::util::escape_csv_field(&fp.name),
                    crate::util::escape_csv_field(&fp.description),
                    fp.pads.len(),
                    fp.tracks.len(),
                    fp.arcs.len(),
                    fp.regions.len(),
                    fp.text.len(),
                    has_external_model,
                    embedded_body_count
                ));
            }

            let csv_content = csv_lines.join("\n");

            let result = json!({
                "status": "success",
                "filepath": filepath,
                "file_type": "PcbLib",
                "format": "csv",
                "component_count": library.len(),
                "csv": csv_content,
            });

            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
        }
    }

    /// Exports a `SchLib` file to JSON or CSV.
    pub(crate) fn export_schlib(filepath: &str, format: &str) -> ToolCallResult {
        use crate::altium::SchLib;

        // Read the library
        let library = match SchLib::open(filepath) {
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

        if format == "json" {
            // Full JSON export
            let symbols: Vec<Value> = library
                .iter()
                .map(|symbol| {
                    json!({
                        "name": symbol.name,
                        "description": symbol.description,
                        "designator": symbol.designator,
                        "pins": symbol.pins,
                        "rectangles": symbol.rectangles,
                        "lines": symbol.lines,
                        "polylines": symbol.polylines,
                        "arcs": symbol.arcs,
                        "ellipses": symbol.ellipses,
                        "labels": symbol.labels,
                        "parameters": symbol.parameters,
                        "footprints": symbol.footprints,
                    })
                })
                .collect();

            let result = json!({
                "status": "success",
                "filepath": filepath,
                "file_type": "SchLib",
                "format": "json",
                "units": "schematic units (10 = 1 grid)",
                "component_count": library.len(),
                "symbols": symbols,
            });

            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
        } else {
            // CSV export - summary table
            let mut csv_lines: Vec<String> = Vec::new();
            csv_lines.push(
                "name,description,designator,pin_count,rectangle_count,line_count,footprint_count"
                    .to_string(),
            );

            for symbol in library.iter() {
                csv_lines.push(format!(
                    "{},{},{},{},{},{},{}",
                    crate::util::escape_csv_field(&symbol.name),
                    crate::util::escape_csv_field(&symbol.description),
                    crate::util::escape_csv_field(&symbol.designator),
                    symbol.pins.len(),
                    symbol.rectangles.len(),
                    symbol.lines.len(),
                    symbol.footprints.len()
                ));
            }

            let csv_content = csv_lines.join("\n");

            let result = json!({
                "status": "success",
                "filepath": filepath,
                "file_type": "SchLib",
                "format": "csv",
                "component_count": library.len(),
                "csv": csv_content,
            });

            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
        }
    }

    // ==================== Library Import ====================

    /// Imports components from JSON data into an Altium library file.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn call_import_library(&self, arguments: &Value) -> ToolCallResult {
        let Some(output_path) = arguments.get("output_path").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: output_path");
        };

        // Validate output path
        if let Err(e) = self.validate_path(output_path) {
            return ToolCallResult::error(e);
        }

        let Some(json_data) = arguments.get("json_data") else {
            return ToolCallResult::error("Missing required parameter: json_data");
        };

        let append = arguments
            .get("append")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Detect file type from JSON data or output path extension
        let file_type = json_data
            .get("file_type")
            .and_then(Value::as_str)
            .map(str::to_lowercase);

        let ext = std::path::Path::new(output_path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        // Determine library type - prefer JSON file_type, fall back to extension
        let library_type = match (file_type.as_deref(), ext.as_deref()) {
            (Some("pcblib"), _) | (None, Some("pcblib")) => "pcblib",
            (Some("schlib"), _) | (None, Some("schlib")) => "schlib",
            _ => {
                return ToolCallResult::error(
                    "Cannot determine library type. Provide 'file_type' in JSON or use .PcbLib/.SchLib extension.",
                );
            }
        };

        match library_type {
            "pcblib" => Self::import_pcblib(output_path, json_data, append),
            "schlib" => Self::import_schlib(output_path, json_data, append),
            _ => unreachable!(),
        }
    }

    /// Imports footprints from JSON into a `PcbLib` file.
    pub(crate) fn import_pcblib(
        output_path: &str,
        json_data: &Value,
        append: bool,
    ) -> ToolCallResult {
        use crate::altium::pcblib::{Footprint, PcbLib};

        // Get footprints array
        let Some(footprints_json) = json_data.get("footprints").and_then(Value::as_array) else {
            return ToolCallResult::error("JSON data must contain 'footprints' array");
        };

        // If append mode and file exists, read existing library; otherwise create new
        let mut library = if append && std::path::Path::new(output_path).exists() {
            match PcbLib::open(output_path) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to read existing library for append: {e}"
                    ));
                }
            }
        } else {
            PcbLib::new()
        };

        let mut imported_count = 0;

        // Parse and add each footprint
        for (idx, fp_json) in footprints_json.iter().enumerate() {
            let name = fp_json
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unnamed");

            // Check for duplicate
            if library.get(name).is_some() {
                return ToolCallResult::error(format!(
                    "Component '{name}' already exists in library"
                ));
            }

            // Use write_pcblib parsing logic via serde
            match serde_json::from_value::<Footprint>(fp_json.clone()) {
                Ok(footprint) => {
                    library.add(footprint);
                    imported_count += 1;
                }
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to parse footprint {idx} ('{name}'): {e}"
                    ));
                }
            }
        }

        // Create backup before destructive operation (if file exists)
        if let Err(e) = Self::create_backup(output_path) {
            return ToolCallResult::error(e);
        }

        // Write the library
        if let Err(e) = library.save(output_path) {
            return ToolCallResult::error(format!("Failed to write library: {e}"));
        }

        let total_count = library.len();
        let mut result = json!({
            "status": "success",
            "output_path": output_path,
            "file_type": "PcbLib",
            "imported_count": imported_count,
            "total_count": total_count,
            "append": append,
            "message": if append {
                format!("Imported {imported_count} footprints (library now has {total_count} total)")
            } else {
                format!("Created library with {imported_count} footprints")
            },
        });

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_pcblib(output_path) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Imports symbols from JSON into a `SchLib` file.
    /// Validates a symbol JSON structure before serde parsing to provide clearer error messages.
    ///
    /// Returns `Ok(())` if validation passes, or an error message with context about
    /// which specific field is missing and in which primitive.
    pub(crate) fn validate_symbol_json(sym_json: &Value) -> Result<(), String> {
        let name = sym_json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Unnamed");

        // Validate pins have required x/y
        if let Some(pins) = sym_json.get("pins").and_then(Value::as_array) {
            for (pin_idx, pin) in pins.iter().enumerate() {
                let pin_name = pin.get("name").and_then(Value::as_str).unwrap_or("?");
                let pin_designator = pin.get("designator").and_then(Value::as_str).unwrap_or("?");

                if pin.get("x").is_none() {
                    return Err(format!(
                        "Symbol '{name}' pin {pin_idx} (name='{pin_name}', designator='{pin_designator}') missing required field 'x'"
                    ));
                }
                if pin.get("y").is_none() {
                    return Err(format!(
                        "Symbol '{name}' pin {pin_idx} (name='{pin_name}', designator='{pin_designator}') missing required field 'y'"
                    ));
                }
                if pin.get("length").is_none() {
                    return Err(format!(
                        "Symbol '{name}' pin {pin_idx} (name='{pin_name}', designator='{pin_designator}') missing required field 'length'"
                    ));
                }
            }
        }

        // Validate rectangles have required coordinates
        if let Some(rects) = sym_json.get("rectangles").and_then(Value::as_array) {
            for (rect_idx, rect) in rects.iter().enumerate() {
                for field in ["x1", "y1", "x2", "y2"] {
                    if rect.get(field).is_none() {
                        return Err(format!(
                            "Symbol '{name}' rectangle {rect_idx} missing required field '{field}'"
                        ));
                    }
                }
            }
        }

        // Validate lines have required coordinates
        if let Some(lines) = sym_json.get("lines").and_then(Value::as_array) {
            for (line_idx, line) in lines.iter().enumerate() {
                for field in ["x1", "y1", "x2", "y2"] {
                    if line.get(field).is_none() {
                        return Err(format!(
                            "Symbol '{name}' line {line_idx} missing required field '{field}'"
                        ));
                    }
                }
            }
        }

        // Validate arcs have required fields
        if let Some(arcs) = sym_json.get("arcs").and_then(Value::as_array) {
            for (arc_idx, arc) in arcs.iter().enumerate() {
                for field in ["x", "y", "radius"] {
                    if arc.get(field).is_none() {
                        return Err(format!(
                            "Symbol '{name}' arc {arc_idx} missing required field '{field}'"
                        ));
                    }
                }
            }
        }

        // Validate ellipses have required fields
        if let Some(ellipses) = sym_json.get("ellipses").and_then(Value::as_array) {
            for (ellipse_idx, ellipse) in ellipses.iter().enumerate() {
                for field in ["x", "y", "radius_x", "radius_y"] {
                    if ellipse.get(field).is_none() {
                        return Err(format!(
                            "Symbol '{name}' ellipse {ellipse_idx} missing required field '{field}'"
                        ));
                    }
                }
            }
        }

        // Validate labels have required fields
        if let Some(labels) = sym_json.get("labels").and_then(Value::as_array) {
            for (label_idx, label) in labels.iter().enumerate() {
                let label_text = label.get("text").and_then(Value::as_str).unwrap_or("?");
                for field in ["x", "y", "text"] {
                    if label.get(field).is_none() {
                        return Err(format!(
                            "Symbol '{name}' label {label_idx} (text='{label_text}') missing required field '{field}'"
                        ));
                    }
                }
            }
        }

        // Note: parameters now have defaults for x/y/value, so no validation needed

        Ok(())
    }

    pub(crate) fn import_schlib(
        output_path: &str,
        json_data: &Value,
        append: bool,
    ) -> ToolCallResult {
        use crate::altium::schlib::Symbol;
        use crate::altium::SchLib;

        // Get symbols array
        let Some(symbols_json) = json_data.get("symbols").and_then(Value::as_array) else {
            return ToolCallResult::error("JSON data must contain 'symbols' array");
        };

        // If append mode and file exists, read existing library; otherwise create new
        let mut library = if append && std::path::Path::new(output_path).exists() {
            match SchLib::open(output_path) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to read existing library for append: {e}"
                    ));
                }
            }
        } else {
            SchLib::new()
        };

        let mut imported_count = 0;

        // Parse and add each symbol
        for (idx, sym_json) in symbols_json.iter().enumerate() {
            let name = sym_json
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unnamed");

            // Check for duplicate
            if library.get(name).is_some() {
                return ToolCallResult::error(format!(
                    "Component '{name}' already exists in library"
                ));
            }

            // Validate symbol structure before serde parsing for better error messages
            if let Err(e) = Self::validate_symbol_json(sym_json) {
                return ToolCallResult::error(e);
            }

            // Parse symbol via serde
            match serde_json::from_value::<Symbol>(sym_json.clone()) {
                Ok(symbol) => {
                    library.add(symbol);
                    imported_count += 1;
                }
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to parse symbol {idx} ('{name}'): {e}"
                    ));
                }
            }
        }

        // Create backup before destructive operation (if file exists)
        if let Err(e) = Self::create_backup(output_path) {
            return ToolCallResult::error(e);
        }

        // Write the library
        if let Err(e) = library.save(output_path) {
            return ToolCallResult::error(format!("Failed to write library: {e}"));
        }

        let total_count = library.len();
        let mut result = json!({
            "status": "success",
            "output_path": output_path,
            "file_type": "SchLib",
            "imported_count": imported_count,
            "total_count": total_count,
            "append": append,
            "message": if append {
                format!("Imported {imported_count} symbols (library now has {total_count} total)")
            } else {
                format!("Created library with {imported_count} symbols")
            },
        });

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_schlib(output_path) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }
}
