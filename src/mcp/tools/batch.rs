//! Batch update tools. Split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

impl McpServer {
    /// Performs batch updates across all components in a library file.
    pub(crate) fn call_batch_update(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(operation) = arguments.get("operation").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: operation");
        };

        let Some(parameters) = arguments.get("parameters") else {
            return ToolCallResult::error("Missing required parameter: parameters");
        };

        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Detect file type
        let ext = std::path::Path::new(filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match ext.as_deref() {
            Some("pcblib") => Self::batch_update_pcblib(filepath, operation, parameters, dry_run),
            Some("schlib") => Self::batch_update_schlib(filepath, operation, parameters, dry_run),
            _ => ToolCallResult::error("batch_update only supports .PcbLib and .SchLib files"),
        }
    }

    /// Performs batch updates on a `PcbLib` file.
    pub(crate) fn batch_update_pcblib(
        filepath: &str,
        operation: &str,
        parameters: &Value,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read the library
        let mut library = match PcbLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Perform the operation
        match operation {
            "update_track_width" => {
                Self::batch_update_track_width(&mut library, parameters, filepath, dry_run)
            }
            "rename_layer" => Self::batch_rename_layer(&mut library, parameters, filepath, dry_run),
            _ => ToolCallResult::error(format!(
                "Unknown PcbLib operation: {operation}. Valid: update_track_width, rename_layer"
            )),
        }
    }

    /// Performs batch updates on a `SchLib` file.
    pub(crate) fn batch_update_schlib(
        filepath: &str,
        operation: &str,
        parameters: &Value,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::schlib::SchLib;

        // Read the library
        let mut library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Perform the operation
        match operation {
            "update_parameters" => {
                Self::batch_update_schlib_parameters(&mut library, parameters, filepath, dry_run)
            }
            _ => ToolCallResult::error(format!(
                "Unknown SchLib operation: {operation}. Valid: update_parameters"
            )),
        }
    }

    /// Updates parameters across all symbols in a `SchLib`.
    pub(crate) fn batch_update_schlib_parameters(
        library: &mut crate::altium::schlib::SchLib,
        parameters: &Value,
        filepath: &str,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::schlib::Parameter;
        use regex::Regex;

        let Some(param_name) = parameters.get("param_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: param_name");
        };

        let Some(param_value) = parameters.get("param_value").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: param_value");
        };

        let add_if_missing = parameters
            .get("add_if_missing")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Compile symbol filter regex if provided
        let symbol_filter = parameters
            .get("symbol_filter")
            .and_then(Value::as_str)
            .map(Regex::new)
            .transpose();

        let symbol_filter = match symbol_filter {
            Ok(filter) => filter,
            Err(e) => {
                return ToolCallResult::error(format!("Invalid symbol_filter regex: {e}"));
            }
        };

        let mut updates = Vec::new();
        let mut symbols_updated = 0;
        let mut params_updated = 0;
        let mut params_added = 0;

        // Update parameters across all symbols
        for symbol in library.iter_mut() {
            // Check symbol filter
            if let Some(ref filter) = symbol_filter {
                if !filter.is_match(&symbol.name) {
                    continue;
                }
            }

            let mut updated_in_symbol = false;
            let mut added_in_symbol = false;

            // Try to find and update existing parameter
            for param in &mut symbol.parameters {
                if param.name == param_name {
                    let old_value = param.value.clone();
                    if !dry_run {
                        param.value = param_value.to_string();
                    }
                    updates.push(json!({
                        "symbol": symbol.name,
                        "action": if dry_run { "would_update" } else { "updated" },
                        "old_value": old_value,
                        "new_value": param_value
                    }));
                    params_updated += 1;
                    updated_in_symbol = true;
                    break;
                }
            }

            // Add parameter if not found and add_if_missing is true
            if !updated_in_symbol && add_if_missing {
                if !dry_run {
                    let param = Parameter::new(param_name, param_value);
                    symbol.add_parameter(param);
                }
                updates.push(json!({
                    "symbol": symbol.name,
                    "action": if dry_run { "would_add" } else { "added" },
                    "new_value": param_value
                }));
                params_added += 1;
                added_in_symbol = true;
            }

            if updated_in_symbol || added_in_symbol {
                symbols_updated += 1;
            }
        }

        // Write back if any updates were made (and not dry-run)
        if symbols_updated > 0 && !dry_run {
            if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
                return resp;
            }
        }

        let mut result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "dry_run": dry_run,
            "filepath": filepath,
            "operation": "update_parameters",
            "param_name": param_name,
            "param_value": param_value,
            "summary": {
                "symbols_updated": symbols_updated,
                "parameters_updated": params_updated,
                "parameters_added": params_added,
                "total_symbols": library.len()
            },
            "updates": updates
        });

        // Run post-write validation (only if actual changes were made)
        if symbols_updated > 0 && !dry_run {
            if let Some(validation) = Self::post_write_validation_schlib(filepath) {
                result["validation"] = validation;
            }
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Updates track widths across all footprints in a library.
    pub(crate) fn batch_update_track_width(
        library: &mut crate::altium::PcbLib,
        parameters: &Value,
        filepath: &str,
        dry_run: bool,
    ) -> ToolCallResult {
        let Some(from_width) = parameters.get("from_width").and_then(Value::as_f64) else {
            return ToolCallResult::error(
                "Missing required parameter: parameters.from_width (number)",
            );
        };

        let Some(to_width) = parameters.get("to_width").and_then(Value::as_f64) else {
            return ToolCallResult::error(
                "Missing required parameter: parameters.to_width (number)",
            );
        };

        let tolerance = parameters
            .get("tolerance")
            .and_then(Value::as_f64)
            .unwrap_or(0.001);

        if to_width <= 0.0 {
            return ToolCallResult::error("to_width must be greater than 0");
        }
        // Range-check too, so a huge width can't saturate in from_mm() on save.
        if let Err(e) = Self::validate_coordinate(to_width, "to_width") {
            return ToolCallResult::error(e);
        }

        let mut total_updated = 0usize;
        let mut footprints_updated = Vec::new();

        for fp in library.iter_mut() {
            let mut fp_count = 0usize;

            for track in &mut fp.tracks {
                if (track.width - from_width).abs() <= tolerance {
                    if !dry_run {
                        track.width = to_width;
                    }
                    fp_count += 1;
                }
            }

            if fp_count > 0 {
                footprints_updated.push(json!({
                    "name": fp.name,
                    "tracks_updated": fp_count,
                }));
                total_updated += fp_count;
            }
        }

        // Write the updated library if any changes were made (and not dry-run)
        if total_updated > 0 && !dry_run {
            if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
                return resp;
            }
        }

        let mut result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "dry_run": dry_run,
            "operation": "update_track_width",
            "filepath": filepath,
            "from_width": from_width,
            "to_width": to_width,
            "tolerance": tolerance,
            "total_tracks_updated": total_updated,
            "footprints_updated_count": footprints_updated.len(),
            "footprints_updated": footprints_updated,
        });

        // Run post-write validation (only if actual changes were made)
        if total_updated > 0 && !dry_run {
            if let Some(validation) = Self::post_write_validation_pcblib(filepath) {
                result["validation"] = validation;
            }
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Renames layers across all footprints in a library.
    pub(crate) fn batch_rename_layer(
        library: &mut crate::altium::PcbLib,
        parameters: &Value,
        filepath: &str,
        dry_run: bool,
    ) -> ToolCallResult {
        let Some(from_layer_str) = parameters.get("from_layer").and_then(Value::as_str) else {
            return ToolCallResult::error(
                "Missing required parameter: parameters.from_layer (string)",
            );
        };

        let Some(to_layer_str) = parameters.get("to_layer").and_then(Value::as_str) else {
            return ToolCallResult::error(
                "Missing required parameter: parameters.to_layer (string)",
            );
        };

        // Parse layer names (supports both "TopLayer" and "Top Layer" formats)
        let Some(from_layer) = Self::parse_layer_name(from_layer_str) else {
            return ToolCallResult::error(format!(
                "Invalid from_layer: '{from_layer_str}'. Use format like 'Top Layer', 'Bottom Layer', \
                 'Top Overlay', 'Mechanical 1', etc."
            ));
        };

        let Some(to_layer) = Self::parse_layer_name(to_layer_str) else {
            return ToolCallResult::error(format!(
                "Invalid to_layer: '{to_layer_str}'. Use format like 'Top Layer', 'Bottom Layer', \
                 'Top Overlay', 'Mechanical 1', etc."
            ));
        };

        let mut total_updated = 0usize;
        let mut footprints_updated = Vec::new();

        for fp in library.iter_mut() {
            let mut fp_changes = json!({
                "name": fp.name,
                "tracks": 0,
                "arcs": 0,
                "regions": 0,
                "text": 0,
            });
            let mut fp_total = 0usize;

            // Update tracks
            for track in &mut fp.tracks {
                if track.layer == from_layer {
                    if !dry_run {
                        track.layer = to_layer;
                    }
                    fp_changes["tracks"] = json!(fp_changes["tracks"].as_u64().unwrap_or(0) + 1);
                    fp_total += 1;
                }
            }

            // Update arcs
            for arc in &mut fp.arcs {
                if arc.layer == from_layer {
                    if !dry_run {
                        arc.layer = to_layer;
                    }
                    fp_changes["arcs"] = json!(fp_changes["arcs"].as_u64().unwrap_or(0) + 1);
                    fp_total += 1;
                }
            }

            // Update regions
            for region in &mut fp.regions {
                if region.layer == from_layer {
                    if !dry_run {
                        region.layer = to_layer;
                    }
                    fp_changes["regions"] = json!(fp_changes["regions"].as_u64().unwrap_or(0) + 1);
                    fp_total += 1;
                }
            }

            // Update text
            for text in &mut fp.text {
                if text.layer == from_layer {
                    if !dry_run {
                        text.layer = to_layer;
                    }
                    fp_changes["text"] = json!(fp_changes["text"].as_u64().unwrap_or(0) + 1);
                    fp_total += 1;
                }
            }

            if fp_total > 0 {
                fp_changes["total"] = json!(fp_total);
                footprints_updated.push(fp_changes);
                total_updated += fp_total;
            }
        }

        // Write the updated library if any changes were made (and not dry-run)
        if total_updated > 0 && !dry_run {
            if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
                return resp;
            }
        }

        let mut result = json!({
            "status": if dry_run { "dry_run" } else { "success" },
            "dry_run": dry_run,
            "operation": "rename_layer",
            "filepath": filepath,
            "from_layer": from_layer.as_str(),
            "to_layer": to_layer.as_str(),
            "total_primitives_updated": total_updated,
            "footprints_updated_count": footprints_updated.len(),
            "footprints_updated": footprints_updated,
        });

        // Run post-write validation (only if actual changes were made)
        if total_updated > 0 && !dry_run {
            if let Some(validation) = Self::post_write_validation_pcblib(filepath) {
                result["validation"] = validation;
            }
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Parses a layer name string, supporting both camelCase and spaced formats.
    pub(crate) fn parse_layer_name(s: &str) -> Option<crate::altium::pcblib::Layer> {
        use crate::altium::pcblib::Layer;

        // First try direct parsing (handles "Top Layer" format)
        if let Some(layer) = Layer::parse(s) {
            return Some(layer);
        }

        // Convert camelCase to spaced format and try again
        let spaced = match s {
            "TopLayer" => "Top Layer",
            "BottomLayer" => "Bottom Layer",
            "TopOverlay" => "Top Overlay",
            "BottomOverlay" => "Bottom Overlay",
            "TopPaste" => "Top Paste",
            "BottomPaste" => "Bottom Paste",
            "TopSolder" => "Top Solder",
            "BottomSolder" => "Bottom Solder",
            "MultiLayer" => "Multi-Layer",
            "KeepOutLayer" | "KeepOut" => "Keep-Out Layer",
            s if s.starts_with("MidLayer") => {
                let num = s.strip_prefix("MidLayer")?;
                return Layer::parse(&format!("Mid-Layer {num}"));
            }
            s if s.starts_with("Mechanical") => {
                let num = s.strip_prefix("Mechanical")?;
                return Layer::parse(&format!("Mechanical {num}"));
            }
            s if s.starts_with("InternalPlane") => {
                let num = s.strip_prefix("InternalPlane")?;
                return Layer::parse(&format!("Internal Plane {num}"));
            }
            _ => return None,
        };

        Layer::parse(spaced)
    }

    /// Validates a component name.
    ///
    /// Note: OLE storage names are limited to 31 characters, but the library layer
    /// handles this by truncating storage names while preserving full names in
    /// the PATTERN/LIBREFERENCE fields.
    pub(crate) fn validate_ole_name(name: &str) -> Result<(), String> {
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

        if name.is_empty() {
            return Err("Component name cannot be empty".to_string());
        }
        if let Some(c) = name.chars().find(|c| INVALID_CHARS.contains(c)) {
            return Err(format!(
                "Component name '{name}' contains invalid character '{c}'. \
                 Names cannot contain: / \\ : * ? \" < > |",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::altium::pcblib::{Footprint, Layer, Pad, PcbLib, Track};
    use crate::altium::SchLib;
    use crate::mcp::tools::test_support::{
        create_test_schlib, create_test_server, get_result_text, parse_result_json, test_temp_dir,
    };
    use serde_json::json;

    /// Builds a `PcbLib` whose footprints carry tracks on known layers.
    fn create_tracked_pcblib(path: &std::path::Path) {
        let mut lib = PcbLib::new();

        let mut fp1 = Footprint::new("SOIC8");
        fp1.add_pad(Pad::smd("1", -2.0, 0.0, 0.6, 1.5));
        fp1.add_track(Track::new(-2.0, -2.0, 2.0, -2.0, 0.2, Layer::TopOverlay));
        fp1.add_track(Track::new(-2.0, 2.0, 2.0, 2.0, 0.2, Layer::TopOverlay));
        fp1.add_track(Track::new(-2.0, -2.0, -2.0, 2.0, 0.3, Layer::Mechanical1));
        lib.add(fp1);

        let mut fp2 = Footprint::new("SOIC16");
        fp2.add_pad(Pad::smd("1", -3.0, 0.0, 0.6, 1.5));
        fp2.add_track(Track::new(-3.0, -3.0, 3.0, -3.0, 0.2, Layer::TopOverlay));
        lib.add(fp2);

        lib.save(path).expect("Failed to create tracked PcbLib");
    }

    #[test]
    fn batch_update_missing_required_arguments() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let result = server.call_batch_update(&json!({}));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: filepath"
        );

        let result = server.call_batch_update(&json!({ "filepath": "x.PcbLib" }));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: operation"
        );

        let result = server.call_batch_update(
            &json!({ "filepath": "x.PcbLib", "operation": "update_track_width" }),
        );
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: parameters"
        );
    }

    #[test]
    fn batch_update_rejects_unsupported_extension_and_unknown_operation() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let txt = dir.path().join("x.txt");
        let result = server.call_batch_update(&json!({
            "filepath": txt.to_string_lossy(),
            "operation": "update_track_width",
            "parameters": {},
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("only supports .PcbLib and .SchLib"));

        let pcb = dir.path().join("Tracks.PcbLib");
        create_tracked_pcblib(&pcb);
        let result = server.call_batch_update(&json!({
            "filepath": pcb.to_string_lossy(),
            "operation": "frobnicate",
            "parameters": {},
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unknown PcbLib operation: frobnicate"));

        let sch = dir.path().join("Syms.SchLib");
        create_test_schlib(&sch);
        let result = server.call_batch_update(&json!({
            "filepath": sch.to_string_lossy(),
            "operation": "frobnicate",
            "parameters": {},
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unknown SchLib operation: frobnicate"));
    }

    #[test]
    fn update_track_width_changes_matching_tracks() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Tracks.PcbLib");
        create_tracked_pcblib(&path);

        let result = server.call_batch_update(&json!({
            "filepath": path.to_string_lossy(),
            "operation": "update_track_width",
            "parameters": { "from_width": 0.2, "to_width": 0.15 },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));

        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["dry_run"], false);
        assert_eq!(parsed["total_tracks_updated"], 3);
        assert_eq!(parsed["footprints_updated_count"], 2);
        assert_eq!(parsed["footprints_updated"][0]["name"], "SOIC8");
        assert_eq!(parsed["footprints_updated"][0]["tracks_updated"], 2);
        assert_eq!(parsed["footprints_updated"][1]["name"], "SOIC16");

        // The 0.2 mm tracks are now 0.15 mm; the 0.3 mm track is untouched.
        let lib = PcbLib::open(&path).unwrap();
        let fp1 = lib.get("SOIC8").unwrap();
        assert!((fp1.tracks[0].width - 0.15).abs() < 1e-6);
        assert!((fp1.tracks[1].width - 0.15).abs() < 1e-6);
        assert!((fp1.tracks[2].width - 0.3).abs() < 1e-6);
    }

    #[test]
    fn update_track_width_dry_run_leaves_file_untouched() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("TracksDry.PcbLib");
        create_tracked_pcblib(&path);

        let result = server.call_batch_update(&json!({
            "filepath": path.to_string_lossy(),
            "operation": "update_track_width",
            "parameters": { "from_width": 0.2, "to_width": 0.15 },
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));

        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "dry_run");
        assert_eq!(parsed["dry_run"], true);
        assert_eq!(parsed["total_tracks_updated"], 3);

        // Nothing was written.
        let lib = PcbLib::open(&path).unwrap();
        assert!((lib.get("SOIC8").unwrap().tracks[0].width - 0.2).abs() < 1e-6);
    }

    #[test]
    fn update_track_width_validates_parameters() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("TracksBad.PcbLib");
        create_tracked_pcblib(&path);
        let filepath = path.to_string_lossy().to_string();

        let result = server.call_batch_update(&json!({
            "filepath": filepath,
            "operation": "update_track_width",
            "parameters": { "to_width": 0.15 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("from_width"));

        let result = server.call_batch_update(&json!({
            "filepath": filepath,
            "operation": "update_track_width",
            "parameters": { "from_width": 0.2 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("to_width"));

        let result = server.call_batch_update(&json!({
            "filepath": filepath,
            "operation": "update_track_width",
            "parameters": { "from_width": 0.2, "to_width": 0.0 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("to_width must be greater than 0"));

        // Out-of-range width is rejected before it could saturate on save.
        let result = server.call_batch_update(&json!({
            "filepath": filepath,
            "operation": "update_track_width",
            "parameters": { "from_width": 0.2, "to_width": 99999.0 },
        }));
        assert!(result.is_error);
    }

    #[test]
    fn rename_layer_moves_matching_primitives() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Layers.PcbLib");
        create_tracked_pcblib(&path);

        // camelCase alias is accepted for the source layer.
        let result = server.call_batch_update(&json!({
            "filepath": path.to_string_lossy(),
            "operation": "rename_layer",
            "parameters": { "from_layer": "TopOverlay", "to_layer": "Mechanical 13" },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));

        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["from_layer"], "Top Overlay");
        assert_eq!(parsed["to_layer"], "Mechanical 13");
        assert_eq!(parsed["total_primitives_updated"], 3);
        assert_eq!(parsed["footprints_updated_count"], 2);
        assert_eq!(parsed["footprints_updated"][0]["tracks"], 2);
        assert_eq!(parsed["footprints_updated"][0]["total"], 2);

        let lib = PcbLib::open(&path).unwrap();
        let fp1 = lib.get("SOIC8").unwrap();
        assert_eq!(fp1.tracks[0].layer, Layer::Mechanical13);
        assert_eq!(fp1.tracks[2].layer, Layer::Mechanical1);
    }

    #[test]
    fn rename_layer_rejects_invalid_layer_names() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("LayersBad.PcbLib");
        create_tracked_pcblib(&path);
        let filepath = path.to_string_lossy().to_string();

        let result = server.call_batch_update(&json!({
            "filepath": filepath,
            "operation": "rename_layer",
            "parameters": { "from_layer": "NotALayer", "to_layer": "Top Overlay" },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Invalid from_layer"));

        let result = server.call_batch_update(&json!({
            "filepath": filepath,
            "operation": "rename_layer",
            "parameters": { "from_layer": "Top Overlay", "to_layer": "NotALayer" },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Invalid to_layer"));
    }

    #[test]
    fn schlib_update_parameters_updates_and_adds() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("BatchParams.SchLib");
        create_test_schlib(&path);
        let filepath = path.to_string_lossy().to_string();

        // Neither symbol has the parameter; add_if_missing creates it on both.
        let result = server.call_batch_update(&json!({
            "filepath": filepath,
            "operation": "update_parameters",
            "parameters": {
                "param_name": "Manufacturer",
                "param_value": "ACME",
                "add_if_missing": true,
            },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["summary"]["symbols_updated"], 2);
        assert_eq!(parsed["summary"]["parameters_added"], 2);
        assert_eq!(parsed["summary"]["parameters_updated"], 0);
        assert_eq!(parsed["updates"][0]["action"], "added");

        // Second run with a symbol filter updates only the matching symbol.
        let result = server.call_batch_update(&json!({
            "filepath": filepath,
            "operation": "update_parameters",
            "parameters": {
                "param_name": "Manufacturer",
                "param_value": "Initech",
                "symbol_filter": "^RES",
            },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["summary"]["symbols_updated"], 1);
        assert_eq!(parsed["summary"]["parameters_updated"], 1);
        assert_eq!(parsed["updates"][0]["symbol"], "RESISTOR");
        assert_eq!(parsed["updates"][0]["old_value"], "ACME");
        assert_eq!(parsed["updates"][0]["new_value"], "Initech");

        let lib = SchLib::open(&path).unwrap();
        assert_eq!(lib.get("RESISTOR").unwrap().parameters[0].value, "Initech");
        assert_eq!(lib.get("CAPACITOR").unwrap().parameters[0].value, "ACME");
    }

    #[test]
    fn schlib_update_parameters_dry_run_previews_without_writing() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("BatchDry.SchLib");
        create_test_schlib(&path);

        let result = server.call_batch_update(&json!({
            "filepath": path.to_string_lossy(),
            "operation": "update_parameters",
            "parameters": {
                "param_name": "Manufacturer",
                "param_value": "ACME",
                "add_if_missing": true,
            },
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "dry_run");
        assert_eq!(parsed["updates"][0]["action"], "would_add");

        // Nothing was written.
        let lib = SchLib::open(&path).unwrap();
        assert!(lib.get("RESISTOR").unwrap().parameters.is_empty());
    }

    #[test]
    fn schlib_update_parameters_rejects_bad_input() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("BatchBad.SchLib");
        create_test_schlib(&path);
        let filepath = path.to_string_lossy().to_string();

        let result = server.call_batch_update(&json!({
            "filepath": filepath,
            "operation": "update_parameters",
            "parameters": { "param_value": "ACME" },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("param_name"));

        let result = server.call_batch_update(&json!({
            "filepath": filepath,
            "operation": "update_parameters",
            "parameters": {
                "param_name": "Manufacturer",
                "param_value": "ACME",
                "symbol_filter": "(unclosed",
            },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Invalid symbol_filter regex"));
    }

    #[test]
    fn parse_layer_name_accepts_both_formats() {
        assert_eq!(
            McpServer::parse_layer_name("Top Layer"),
            Some(crate::altium::pcblib::Layer::TopLayer)
        );
        assert_eq!(
            McpServer::parse_layer_name("BottomOverlay"),
            Some(crate::altium::pcblib::Layer::BottomOverlay)
        );
        assert_eq!(
            McpServer::parse_layer_name("Mechanical7"),
            Some(crate::altium::pcblib::Layer::Mechanical7)
        );
        assert_eq!(
            McpServer::parse_layer_name("MidLayer3"),
            Some(crate::altium::pcblib::Layer::MidLayer3)
        );
        assert_eq!(McpServer::parse_layer_name("NotALayer"), None);
    }

    #[test]
    fn validate_ole_name_rules() {
        assert!(McpServer::validate_ole_name("CHIP_0402").is_ok());
        assert!(McpServer::validate_ole_name("").is_err());
        for bad in [
            "a/b", "a\\b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b",
        ] {
            assert!(
                McpServer::validate_ole_name(bad).is_err(),
                "'{bad}' should be rejected"
            );
        }
    }
}
