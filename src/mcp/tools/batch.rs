//! Batch update tools. Split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

/// The `parameters` keys each `batch_update` operation reads, or `None` for
/// an operation it does not know.
pub fn batch_parameter_keys(operation: &str) -> Option<&'static [&'static str]> {
    Some(match operation {
        "update_track_width" => &["from_width", "to_width", "tolerance"],
        "rename_layer" => &["from_layer", "to_layer"],
        "update_parameters" => &[
            "param_name",
            "param_value",
            "symbol_filter",
            "add_if_missing",
        ],
        _ => return None,
    })
}

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
        // The parameters each operation reads; a key outside them is a typo
        // (`tolerence`) that would otherwise silently fall back to a default.
        let Some(keys) = batch_parameter_keys(operation) else {
            return ToolCallResult::error(format!(
                "Unknown operation '{operation}'. Valid: update_track_width, rename_layer, \
                 update_parameters"
            ));
        };
        if let Err(e) = Self::check_unknown_fields(parameters, keys) {
            return ToolCallResult::error(e);
        }

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

            // Find and update the existing parameter; names match without
            // regard to case, as Altium and manage_schlib_parameters treat them.
            for param in &mut symbol.parameters {
                if param.name.eq_ignore_ascii_case(param_name) {
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
            if let Err(resp) = Self::backup_then_save(filepath, &mut *library) {
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
            if let Err(resp) = Self::backup_then_save(filepath, &mut *library) {
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
        let Some(from_layer) = crate::altium::pcblib::Layer::parse(from_layer_str) else {
            return ToolCallResult::error(format!(
                "Invalid from_layer: '{from_layer_str}'. Use format like 'Top Layer', 'Bottom Layer', \
                 'Top Overlay', 'Mechanical 1', etc."
            ));
        };

        let Some(to_layer) = crate::altium::pcblib::Layer::parse(to_layer_str) else {
            return ToolCallResult::error(format!(
                "Invalid to_layer: '{to_layer_str}'. Use format like 'Top Layer', 'Bottom Layer', \
                 'Top Overlay', 'Mechanical 1', etc."
            ));
        };

        let mut total_updated = 0usize;
        let mut footprints_updated = Vec::new();

        for fp in library.iter_mut() {
            // A dry run moves a copy, so the report is the real move's report.
            let moved = if dry_run {
                fp.clone().move_layer(from_layer, to_layer)
            } else {
                fp.move_layer(from_layer, to_layer)
            };
            if moved.total() > 0 {
                footprints_updated.push(json!({
                    "name": fp.name,
                    "tracks": moved.tracks,
                    "arcs": moved.arcs,
                    "regions": moved.regions,
                    "text": moved.text,
                    "fills": moved.fills,
                    "pads": moved.pads,
                    "component_bodies": moved.component_bodies,
                    "total": moved.total(),
                }));
                total_updated += moved.total();
            }
        }

        // Write the updated library if any changes were made (and not dry-run)
        if total_updated > 0 && !dry_run {
            if let Err(resp) = Self::backup_then_save(filepath, &mut *library) {
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
        assert!(get_result_text(&result).contains("Unknown operation 'frobnicate'"));

        let sch = dir.path().join("Syms.SchLib");
        create_test_schlib(&sch);
        let result = server.call_batch_update(&json!({
            "filepath": sch.to_string_lossy(),
            "operation": "frobnicate",
            "parameters": {},
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unknown operation 'frobnicate'"));

        // A valid operation on the other format is still refused by that
        // format's own dispatch.
        let result = server.call_batch_update(&json!({
            "filepath": sch.to_string_lossy(),
            "operation": "rename_layer",
            "parameters": { "from_layer": "Mechanical 1", "to_layer": "Mechanical 2" },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unknown SchLib operation: rename_layer"));

        // A misspelt parameter is refused rather than falling back to a default.
        let result = server.call_batch_update(&json!({
            "filepath": pcb.to_string_lossy(),
            "operation": "update_track_width",
            "parameters": { "from_width": 0.2, "to_width": 0.25, "tolerence": 0.01 },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unknown field 'tolerence'"));
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

    /// "Move every primitive" means every kind that sits on a layer: fills,
    /// pads and component bodies move with the tracks, arcs, regions and
    /// text, a dry run reports the same counts, and a moved region or body
    /// writes the token of its new layer.
    #[test]
    fn rename_layer_moves_fills_pads_and_bodies_too() {
        use crate::altium::pcblib::{Arc, ComponentBody, Fill, Region, Text};

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Kinds.PcbLib");
        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("KINDS");
        let from = Layer::Mechanical13;
        fp.add_track(Track::new(0.0, 0.0, 1.0, 0.0, 0.1, from));
        fp.add_arc(Arc::circle(0.0, 0.0, 1.0, 0.1, from));
        fp.add_text(Text::new(0.0, 0.0, "T", 1.0, from));
        fp.add_fill(Fill::new(0.0, 0.0, 1.0, 1.0, from));
        fp.add_region(Region::rectangle(0.0, 0.0, 1.0, 1.0, from));
        let mut pad = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        pad.layer = from;
        fp.add_pad(pad);
        let mut body = ComponentBody::new("", "body.step");
        body.embedded = false;
        body.layer = from;
        fp.add_component_body(body);
        lib.add(fp);
        lib.save(&path).unwrap();

        for dry_run in [true, false] {
            let result = server.call_batch_update(&json!({
                "filepath": path.to_string_lossy(),
                "operation": "rename_layer",
                "parameters": { "from_layer": "Mechanical 13", "to_layer": "Mechanical 20" },
                "dry_run": dry_run,
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            assert_eq!(
                parsed["total_primitives_updated"], 7,
                "dry_run={dry_run}: {parsed}"
            );
            let report = &parsed["footprints_updated"][0];
            for kind in [
                "tracks",
                "arcs",
                "regions",
                "text",
                "fills",
                "pads",
                "component_bodies",
            ] {
                assert_eq!(report[kind], 1, "dry_run={dry_run} {kind}: {parsed}");
            }
            assert_eq!(report["total"], 7);
        }

        let lib = PcbLib::open(&path).unwrap();
        let fp = lib.get("KINDS").unwrap();
        let to = Layer::Mechanical20;
        assert_eq!(fp.tracks[0].layer, to);
        assert_eq!(fp.arcs[0].layer, to);
        assert_eq!(fp.text[0].layer, to);
        assert_eq!(fp.fills[0].layer, to);
        assert_eq!(
            fp.regions[0].layer, to,
            "the region's token followed the move"
        );
        assert_eq!(fp.pads[0].layer, to);
        assert_eq!(
            fp.component_bodies[0].layer, to,
            "the body's token followed the move"
        );
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

    /// A parameter name matches without regard to case, as Altium and
    /// `manage_schlib_parameters` treat it: `MANUFACTURER` updates
    /// `Manufacturer` rather than adding a case-twin beside it.
    #[test]
    fn schlib_update_parameters_matches_names_without_regard_to_case() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("BatchParamCase.SchLib");
        create_test_schlib(&path);
        let filepath = path.to_string_lossy().to_string();

        for (name, value) in [("Manufacturer", "ACME"), ("MANUFACTURER", "Initech")] {
            let result = server.call_batch_update(&json!({
                "filepath": filepath,
                "operation": "update_parameters",
                "parameters": {
                    "param_name": name,
                    "param_value": value,
                    "add_if_missing": true,
                },
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
        }

        let lib = SchLib::open(&path).unwrap();
        for symbol in lib.iter() {
            let manufacturers: Vec<&str> = symbol
                .parameters
                .iter()
                .filter(|p| p.name.eq_ignore_ascii_case("manufacturer"))
                .map(|p| p.value.as_str())
                .collect();
            assert_eq!(manufacturers, ["Initech"], "{}", symbol.name);
            assert_eq!(symbol.parameters[0].name, "Manufacturer", "{}", symbol.name);
        }
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

    /// `rename_layer` resolves layer names exactly as every other tool does,
    /// through `Layer::parse`: Altium's spelling, the camel-case alias, any
    /// case.
    #[test]
    fn rename_layer_accepts_every_layer_spelling() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Spellings.PcbLib");
        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("FP");
        fp.add_track(Track::new(0.0, 0.0, 1.0, 0.0, 0.1, Layer::Mechanical13));
        lib.add(fp);
        lib.save(&path).unwrap();

        for (from, to, expected) in [
            ("mechanical13", "TopCourtyard", Layer::TopCourtyard),
            ("Top Courtyard", "top-assembly", Layer::TopAssembly),
            ("TopAssembly", "Mechanical 20", Layer::Mechanical20),
        ] {
            let result = server.call_batch_update(&json!({
                "filepath": path.to_string_lossy(),
                "operation": "rename_layer",
                "parameters": { "from_layer": from, "to_layer": to },
            }));
            assert!(
                !result.is_error,
                "{from} -> {to}: {}",
                get_result_text(&result)
            );
            let lib = PcbLib::open(&path).unwrap();
            assert_eq!(
                lib.get("FP").unwrap().tracks[0].layer,
                expected,
                "{from} -> {to}"
            );
        }
    }

    /// A name is refused only when empty or carrying a control character.
    /// Punctuation Altium itself uses in names — `EC10*10.5`, `L1210/3225` in
    /// an AD21-authored library (#507) — is accepted; the storage name is
    /// sanitised on save exactly as Altium does.
    #[test]
    fn validate_ole_name_rules() {
        assert!(McpServer::validate_ole_name("CHIP_0402").is_ok());
        assert!(McpServer::validate_ole_name("").is_err());
        for fine in [
            "a/b",
            "a\\b",
            "a:b",
            "EC10*10.5",
            "a?b",
            "a\"b",
            "a<b",
            "a>b",
            "a|b",
        ] {
            assert!(
                McpServer::validate_ole_name(fine).is_ok(),
                "'{fine}' is a name Altium can hold"
            );
        }
        let err = McpServer::validate_ole_name("tab\there").unwrap_err();
        assert!(err.contains("U+0009"), "{err}");
    }

    // ==================== operation success paths ====================

    #[test]
    fn batch_update_track_width_changes_matching_tracks() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Tracks.PcbLib");
        create_tracked_pcblib(&path);

        let r = server.call_batch_update(&json!({
            "filepath": path.to_string_lossy(),
            "operation": "update_track_width",
            "parameters": { "from_width": 0.2, "to_width": 0.25 },
        }));
        assert!(!r.is_error, "{}", get_result_text(&r));
        let p = parse_result_json(&r);
        assert_eq!(p["operation"], "update_track_width");
        assert_eq!(p["dry_run"], false);

        // The 0.2 mm tracks were widened; the 0.3 mm Mechanical1 track is untouched.
        let lib = PcbLib::open(&path).unwrap();
        let widened = lib
            .iter()
            .flat_map(|fp| fp.tracks.iter())
            .filter(|t| (t.width - 0.25).abs() < 1e-4)
            .count();
        assert_eq!(widened, 3);
    }

    #[test]
    fn batch_update_track_width_dry_run_leaves_file_untouched() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("TracksDry.PcbLib");
        create_tracked_pcblib(&path);

        let r = server.call_batch_update(&json!({
            "filepath": path.to_string_lossy(),
            "operation": "update_track_width",
            "parameters": { "from_width": 0.2, "to_width": 0.25 },
            "dry_run": true,
        }));
        assert!(!r.is_error, "{}", get_result_text(&r));
        assert_eq!(parse_result_json(&r)["dry_run"], true);

        // Nothing was written: the 0.2 mm tracks survive unchanged.
        let lib = PcbLib::open(&path).unwrap();
        let unchanged = lib
            .iter()
            .flat_map(|fp| fp.tracks.iter())
            .filter(|t| (t.width - 0.2).abs() < 1e-4)
            .count();
        assert_eq!(unchanged, 3);
    }

    #[test]
    fn batch_rename_layer_moves_primitives() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Rename.PcbLib");
        create_tracked_pcblib(&path);

        let r = server.call_batch_update(&json!({
            "filepath": path.to_string_lossy(),
            "operation": "rename_layer",
            "parameters": { "from_layer": "Top Overlay", "to_layer": "Bottom Overlay" },
        }));
        assert!(!r.is_error, "{}", get_result_text(&r));
        assert_eq!(parse_result_json(&r)["operation"], "rename_layer");

        let lib = PcbLib::open(&path).unwrap();
        let on_top = lib
            .iter()
            .flat_map(|fp| fp.tracks.iter())
            .filter(|t| t.layer == Layer::TopOverlay)
            .count();
        assert_eq!(on_top, 0, "all Top Overlay tracks were moved");
    }

    /// A region read with a `V7_LAYER` token that disagreed with its layer byte
    /// carries the token as an override; moving the region drops it, so the
    /// saved token names the new layer rather than the old one.
    #[test]
    fn batch_rename_layer_drops_a_region_stale_v7_token() {
        use crate::altium::pcblib::{Footprint, Region};

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let mut region = Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopOverlay);
        region.v7_layer = Some("MECHANICAL4".to_string()); // disagrees with Top Overlay
        let mut fp = Footprint::new("RGN");
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.6, 0.5));
        fp.add_region(region);
        let mut lib = PcbLib::new();
        lib.add(fp);
        let path = dir.path().join("RegionToken.PcbLib");
        lib.save(&path).unwrap();
        // The override round-trips as long as nothing moves the region.
        let before = PcbLib::open(&path).unwrap();
        assert_eq!(
            before.get("RGN").unwrap().regions[0].v7_layer.as_deref(),
            Some("MECHANICAL4")
        );

        let r = server.call_batch_update(&json!({
            "filepath": path.to_string_lossy(),
            "operation": "rename_layer",
            "parameters": { "from_layer": "Top Overlay", "to_layer": "Bottom Overlay" },
        }));
        assert!(!r.is_error, "{}", get_result_text(&r));

        let after = PcbLib::open(&path).unwrap();
        let moved = &after.get("RGN").unwrap().regions[0];
        assert_eq!(moved.layer, Layer::BottomOverlay);
        // The reader keeps a token only when it disagrees with the byte, so
        // `None` here means the saved token is Bottom Overlay's own.
        assert_eq!(moved.v7_layer, None, "stale token not replayed");
    }

    #[test]
    fn batch_rename_layer_rejects_invalid_layer() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("BadLayer.PcbLib");
        create_tracked_pcblib(&path);
        let r = server.call_batch_update(&json!({
            "filepath": path.to_string_lossy(),
            "operation": "rename_layer",
            "parameters": { "from_layer": "Nonexistent Layer", "to_layer": "Bottom Overlay" },
        }));
        assert!(r.is_error);
        assert!(get_result_text(&r).contains("Invalid from_layer"));
    }

    #[test]
    fn batch_update_schlib_parameters_adds_parameter() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Params.SchLib");
        create_test_schlib(&path);

        let r = server.call_batch_update(&json!({
            "filepath": path.to_string_lossy(),
            "operation": "update_parameters",
            "parameters": { "param_name": "Tolerance", "param_value": "1%", "add_if_missing": true },
        }));
        assert!(!r.is_error, "{}", get_result_text(&r));
        assert_eq!(parse_result_json(&r)["operation"], "update_parameters");

        let lib = SchLib::open(&path).unwrap();
        let has_param = lib
            .iter()
            .flat_map(|s| s.parameters.iter())
            .any(|p| p.name == "Tolerance" && p.value == "1%");
        assert!(has_param, "the parameter was added to at least one symbol");
    }

    // ==================== rejection paths and the remaining arms =============

    mod rejections {
        use crate::altium::pcblib::Layer;
        use crate::mcp::server::McpServer;
        use crate::mcp::tools::test_support::{
            create_test_pcblib, create_test_schlib, create_test_server, get_result_text,
            parse_result_json, test_temp_dir,
        };
        use serde_json::json;

        fn write_garbage(path: &std::path::Path) {
            std::fs::write(path, b"not an OLE compound document").unwrap();
        }

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

        fn assert_error_mentions(result: &crate::mcp::server::ToolCallResult, needle: &str) {
            let text = get_result_text(result);
            assert!(result.is_error, "expected an error, got: {text}");
            assert!(
                text.contains(needle),
                "expected the error to mention {needle:?}, got: {text}"
            );
        }

        /// A footprint with one primitive on Top Overlay in every family the
        /// layer rename walks, so a single rename exercises all four loops.
        fn write_overlay_library(server: &McpServer, path: &str) {
            let r = server.call_write_pcblib(&json!({
                "filepath": path,
                "footprints": [{
                    "name": "OVERLAY",
                    "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                    "tracks": [{ "x1": -1.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.2, "layer": "Top Overlay" }],
                    "arcs": [{ "x": 0.0, "y": 0.0, "radius": 2.0, "start_angle": 0.0, "end_angle": 90.0, "width": 0.2, "layer": "Top Overlay" }],
                    "text": [{ "x": 0.0, "y": 3.0, "text": "REF", "height": 1.0, "layer": "Top Overlay" }],
                    "regions": [{
                        "layer": "Top Overlay",
                        "vertices": [
                            { "x": -1.0, "y": -1.0 }, { "x": 1.0, "y": -1.0 }, { "x": 0.0, "y": 1.0 },
                        ],
                    }],
                }],
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
        }

        // ---- dispatch ----------------------------------------------------------

        #[test]
        fn batch_update_guards_its_path_and_file_type() {
            let dir = test_temp_dir();
            let outside = test_temp_dir();
            let server = create_test_server(dir.path());

            let escaped = server.call_batch_update(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
                "operation": "rename_layer", "parameters": {},
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));

            let wrong_ext = server.call_batch_update(&json!({
                "filepath": dir.path().join("X.txt").to_string_lossy(),
                "operation": "rename_layer", "parameters": {},
            }));
            assert_error_mentions(&wrong_ext, "only supports .PcbLib and .SchLib");
        }

        #[test]
        fn batch_update_reports_unreadable_libraries_and_unknown_operations() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let pcb = dir.path().join("Bad.PcbLib");
            let sch = dir.path().join("Bad.SchLib");
            write_garbage(&pcb);
            write_garbage(&sch);

            for path in [&pcb, &sch] {
                let r = server.call_batch_update(&json!({
                    "filepath": path.to_string_lossy(),
                    "operation": "rename_layer", "parameters": {},
                }));
                assert_error_mentions(&r, "Failed to read library");
            }

            // Each file type has its own operation vocabulary, and each names
            // what it does accept.
            let good_pcb = dir.path().join("Lib.PcbLib");
            let good_sch = dir.path().join("Lib.SchLib");
            create_test_pcblib(&good_pcb);
            create_test_schlib(&good_sch);

            let pcb_op = server.call_batch_update(&json!({
                "filepath": good_pcb.to_string_lossy(),
                "operation": "update_parameters", "parameters": {},
            }));
            assert_error_mentions(&pcb_op, "Unknown PcbLib operation");

            let sch_op = server.call_batch_update(&json!({
                "filepath": good_sch.to_string_lossy(),
                "operation": "rename_layer", "parameters": {},
            }));
            assert_error_mentions(&sch_op, "Unknown SchLib operation");
        }

        // ---- update_parameters (SchLib) -----------------------------------------

        #[test]
        fn update_parameters_names_its_missing_arguments_and_bad_filter() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Lib.SchLib");
            create_test_schlib(&path);

            let call = |parameters: serde_json::Value| {
                server.call_batch_update(&json!({
                    "filepath": path.to_string_lossy(),
                    "operation": "update_parameters", "parameters": parameters,
                }))
            };

            assert_error_mentions(&call(json!({ "param_value": "1%" })), "param_name");
            assert_error_mentions(&call(json!({ "param_name": "Tolerance" })), "param_value");
            assert_error_mentions(
                &call(json!({
                    "param_name": "Tolerance", "param_value": "1%", "symbol_filter": "RES[",
                })),
                "regex",
            );
        }

        #[test]
        fn update_parameters_adds_a_missing_parameter_only_when_asked() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Add.SchLib");
            create_test_schlib(&path);

            let call = |add: bool, dry: bool| {
                server.call_batch_update(&json!({
                    "filepath": path.to_string_lossy(),
                    "operation": "update_parameters",
                    "parameters": {
                        "param_name": "Supplier", "param_value": "Acme",
                        "add_if_missing": add,
                    },
                    "dry_run": dry,
                }))
            };

            // Without the opt-in, a parameter no symbol carries is left alone.
            let untouched = call(false, false);
            assert!(!untouched.is_error, "{}", get_result_text(&untouched));
            assert_eq!(
                parse_result_json(&untouched)["summary"]["parameters_added"],
                0
            );

            // A dry run reports what it would add without writing it.
            let dry = call(true, true);
            let dry_parsed = parse_result_json(&dry);
            assert!(dry_parsed["summary"]["parameters_added"].as_u64().unwrap() > 0);
            assert_eq!(dry_parsed["status"], "dry_run");

            let real = call(true, false);
            assert!(!real.is_error, "{}", get_result_text(&real));
            let lib = crate::altium::SchLib::open(&path).unwrap();
            assert!(lib
                .iter()
                .all(|s| s.parameters.iter().any(|p| p.name == "Supplier")));
        }

        #[test]
        fn update_parameters_reports_a_failed_write() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Locked.SchLib");
            create_test_schlib(&path);

            block_save(&path, true);
            let r = server.call_batch_update(&json!({
                "filepath": path.to_string_lossy(),
                "operation": "update_parameters",
                "parameters": {
                    "param_name": "Supplier", "param_value": "Acme", "add_if_missing": true,
                },
            }));
            block_save(&path, false);
            assert!(r.is_error, "{}", get_result_text(&r));
        }

        // ---- rename_layer / update_track_width (PcbLib) --------------------------

        #[test]
        fn rename_layer_names_its_missing_arguments_and_unknown_layers() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Lib.PcbLib");
            create_test_pcblib(&path);

            let call = |parameters: serde_json::Value| {
                server.call_batch_update(&json!({
                    "filepath": path.to_string_lossy(),
                    "operation": "rename_layer", "parameters": parameters,
                }))
            };

            assert_error_mentions(&call(json!({ "to_layer": "Top Layer" })), "from_layer");
            assert_error_mentions(&call(json!({ "from_layer": "Top Layer" })), "to_layer");
            // Both ends are parsed, so each rejects on its own.
            assert_error_mentions(
                &call(json!({ "from_layer": "Nowhere", "to_layer": "Top Layer" })),
                "Invalid from_layer",
            );
            assert_error_mentions(
                &call(json!({ "from_layer": "Top Layer", "to_layer": "Nowhere" })),
                "Invalid to_layer",
            );
        }

        #[test]
        fn rename_layer_moves_every_primitive_family_that_sits_on_it() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Overlay.PcbLib");
            let path_str = path.to_string_lossy().into_owned();
            write_overlay_library(&server, &path_str);

            let r = server.call_batch_update(&json!({
                "filepath": &path_str,
                "operation": "rename_layer",
                "parameters": { "from_layer": "Top Overlay", "to_layer": "Bottom Overlay" },
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));

            // Tracks, arcs, regions and text each have their own loop, so all
            // four have to be counted or a family would silently stay behind.
            // Text counts two: write_pcblib injects a `.Designator` on the
            // overlay alongside the one authored here.
            let changes = &parse_result_json(&r)["footprints_updated"][0];
            for family in ["tracks", "arcs", "regions"] {
                assert_eq!(changes[family], 1, "{family} not renamed: {changes}");
            }
            assert_eq!(changes["text"], 2, "text not renamed: {changes}");

            let lib = crate::altium::PcbLib::open(&path).unwrap();
            let fp = lib.get("OVERLAY").unwrap();
            assert_eq!(fp.tracks[0].layer, Layer::BottomOverlay);
            assert_eq!(fp.arcs[0].layer, Layer::BottomOverlay);
            assert_eq!(fp.regions[0].layer, Layer::BottomOverlay);
            assert_eq!(fp.text[0].layer, Layer::BottomOverlay);
        }

        #[test]
        fn pcblib_batch_operations_report_a_failed_write() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Locked.PcbLib");
            let path_str = path.to_string_lossy().into_owned();
            write_overlay_library(&server, &path_str);

            for (operation, parameters) in [
                (
                    "rename_layer",
                    json!({ "from_layer": "Top Overlay", "to_layer": "Bottom Overlay" }),
                ),
                (
                    "update_track_width",
                    json!({ "from_width": 0.2, "to_width": 0.3 }),
                ),
            ] {
                block_save(&path, true);
                let r = server.call_batch_update(&json!({
                    "filepath": &path_str, "operation": operation, "parameters": parameters,
                }));
                block_save(&path, false);
                assert!(r.is_error, "{operation}: {}", get_result_text(&r));
            }
        }
    }

    /// A batch parameter value the record cannot hold is refused by field,
    /// with the file untouched and no backup made.
    #[test]
    fn update_parameters_refuses_a_pipe_in_the_value_before_any_backup() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Pipe.SchLib");
        create_test_schlib(&path);

        let result = server.call_batch_update(&json!({
            "filepath": path.to_string_lossy(),
            "operation": "update_parameters",
            "parameters": { "param_name": "Value", "param_value": "1|2", "add_if_missing": true },
        }));
        assert!(result.is_error);
        let text = get_result_text(&result);
        assert!(text.contains("parameters[].value contains '|'"), "{text}");
        let backups = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "bak"))
            .count();
        assert_eq!(backups, 0, "no backup for a save that never happens");
    }
}
