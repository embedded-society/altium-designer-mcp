//! Library diff tools, split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

impl McpServer {
    // ==================== Library Diff Tools ====================

    /// Compares two Altium library files and reports differences.
    pub(crate) fn call_diff_libraries(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath_a) = arguments.get("filepath_a").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath_a");
        };

        let Some(filepath_b) = arguments.get("filepath_b").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath_b");
        };

        // Validate both paths
        if let Err(e) = self.validate_path(filepath_a) {
            return ToolCallResult::error(e);
        }
        if let Err(e) = self.validate_path(filepath_b) {
            return ToolCallResult::error(e);
        }

        // Determine file types from extensions
        let path_a = std::path::Path::new(filepath_a);
        let path_b = std::path::Path::new(filepath_b);

        let ext_a = path_a
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);
        let ext_b = path_b
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        // Ensure both files are the same type
        if ext_a != ext_b {
            let result = json!({
                "status": "error",
                "error": format!("File types must match. Got '{}' and '{}'.",
                    ext_a.as_deref().unwrap_or("unknown"),
                    ext_b.as_deref().unwrap_or("unknown"))
            });
            return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
        }

        match ext_a.as_deref() {
            Some("pcblib") => Self::diff_pcblibs(filepath_a, filepath_b),
            Some("schlib") => Self::diff_schlibs(filepath_a, filepath_b),
            _ => {
                let result = json!({
                    "status": "error",
                    "error": "Unknown file type. Expected .PcbLib or .SchLib extension.",
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Compares two `PcbLib` files.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn diff_pcblibs(filepath_a: &str, filepath_b: &str) -> ToolCallResult {
        use crate::altium::PcbLib;
        use std::collections::HashSet;

        // Read both libraries
        let lib_a = match PcbLib::open(filepath_a) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "error": format!("Failed to read '{}': {e}", crate::altium::error::sanitise_path_for_client(std::path::Path::new(filepath_a))),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        let lib_b = match PcbLib::open(filepath_b) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "error": format!("Failed to read '{}': {e}", crate::altium::error::sanitise_path_for_client(std::path::Path::new(filepath_b))),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        // Get component names from both libraries
        let names_a: HashSet<String> = lib_a.iter().map(|f| f.name.clone()).collect();
        let names_b: HashSet<String> = lib_b.iter().map(|f| f.name.clone()).collect();

        // Find added, removed, and common components
        let added: Vec<&str> = names_b.difference(&names_a).map(String::as_str).collect();
        let removed: Vec<&str> = names_a.difference(&names_b).map(String::as_str).collect();
        let common: Vec<&str> = names_a.intersection(&names_b).map(String::as_str).collect();

        // Check for modifications in common components
        let mut modified: Vec<Value> = Vec::new();

        for name in &common {
            let fp_a = lib_a.get(name).unwrap();
            let fp_b = lib_b.get(name).unwrap();

            let mut changes: Vec<String> = Vec::new();

            // Compare descriptions
            if fp_a.description != fp_b.description {
                changes.push(format!(
                    "description: '{}' -> '{}'",
                    fp_a.description, fp_b.description
                ));
            }

            // Compare primitive counts
            if fp_a.pads.len() != fp_b.pads.len() {
                changes.push(format!(
                    "pad_count: {} -> {}",
                    fp_a.pads.len(),
                    fp_b.pads.len()
                ));
            }
            if fp_a.tracks.len() != fp_b.tracks.len() {
                changes.push(format!(
                    "track_count: {} -> {}",
                    fp_a.tracks.len(),
                    fp_b.tracks.len()
                ));
            }
            if fp_a.arcs.len() != fp_b.arcs.len() {
                changes.push(format!(
                    "arc_count: {} -> {}",
                    fp_a.arcs.len(),
                    fp_b.arcs.len()
                ));
            }
            if fp_a.regions.len() != fp_b.regions.len() {
                changes.push(format!(
                    "region_count: {} -> {}",
                    fp_a.regions.len(),
                    fp_b.regions.len()
                ));
            }
            if fp_a.text.len() != fp_b.text.len() {
                changes.push(format!(
                    "text_count: {} -> {}",
                    fp_a.text.len(),
                    fp_b.text.len()
                ));
            }

            // Compare 3D model presence (external references)
            let has_model_a = fp_a.model_3d.is_some();
            let has_model_b = fp_b.model_3d.is_some();
            if has_model_a != has_model_b {
                changes.push(format!(
                    "external_3d_model: {} -> {}",
                    if has_model_a { "yes" } else { "no" },
                    if has_model_b { "yes" } else { "no" }
                ));
            }

            // Compare embedded 3D bodies
            if fp_a.component_bodies.len() != fp_b.component_bodies.len() {
                changes.push(format!(
                    "component_body_count: {} -> {}",
                    fp_a.component_bodies.len(),
                    fp_b.component_bodies.len()
                ));
            }

            if !changes.is_empty() {
                modified.push(json!({
                    "name": name,
                    "changes": changes,
                }));
            }
        }

        let result = json!({
            "status": "success",
            "file_type": "PcbLib",
            "filepath_a": filepath_a,
            "filepath_b": filepath_b,
            "summary": {
                "components_in_a": lib_a.len(),
                "components_in_b": lib_b.len(),
                "added_count": added.len(),
                "removed_count": removed.len(),
                "modified_count": modified.len(),
                "unchanged_count": common.len() - modified.len(),
            },
            "added": added,
            "removed": removed,
            "modified": modified,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Compares two `SchLib` files.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn diff_schlibs(filepath_a: &str, filepath_b: &str) -> ToolCallResult {
        use crate::altium::SchLib;
        use std::collections::HashSet;

        // Read both libraries
        let lib_a = match SchLib::open(filepath_a) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "error": format!("Failed to read '{}': {e}", crate::altium::error::sanitise_path_for_client(std::path::Path::new(filepath_a))),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        let lib_b = match SchLib::open(filepath_b) {
            Ok(lib) => lib,
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "error": format!("Failed to read '{}': {e}", crate::altium::error::sanitise_path_for_client(std::path::Path::new(filepath_b))),
                });
                return ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap());
            }
        };

        // Get component names from both libraries
        let names_a: HashSet<String> = lib_a.iter().map(|s| s.name.clone()).collect();
        let names_b: HashSet<String> = lib_b.iter().map(|s| s.name.clone()).collect();

        // Find added, removed, and common components
        let added: Vec<&str> = names_b.difference(&names_a).map(String::as_str).collect();
        let removed: Vec<&str> = names_a.difference(&names_b).map(String::as_str).collect();
        let common: Vec<&str> = names_a.intersection(&names_b).map(String::as_str).collect();

        // Check for modifications in common components
        let mut modified: Vec<Value> = Vec::new();

        for name in &common {
            let sym_a = lib_a.get(name).unwrap();
            let sym_b = lib_b.get(name).unwrap();

            let mut changes: Vec<String> = Vec::new();

            // Compare descriptions
            if sym_a.description != sym_b.description {
                changes.push(format!(
                    "description: '{}' -> '{}'",
                    sym_a.description, sym_b.description
                ));
            }

            // Compare designators
            if sym_a.designator != sym_b.designator {
                changes.push(format!(
                    "designator: '{}' -> '{}'",
                    sym_a.designator, sym_b.designator
                ));
            }

            // Compare primitive counts
            if sym_a.pins.len() != sym_b.pins.len() {
                changes.push(format!(
                    "pin_count: {} -> {}",
                    sym_a.pins.len(),
                    sym_b.pins.len()
                ));
            }
            if sym_a.rectangles.len() != sym_b.rectangles.len() {
                changes.push(format!(
                    "rectangle_count: {} -> {}",
                    sym_a.rectangles.len(),
                    sym_b.rectangles.len()
                ));
            }
            if sym_a.lines.len() != sym_b.lines.len() {
                changes.push(format!(
                    "line_count: {} -> {}",
                    sym_a.lines.len(),
                    sym_b.lines.len()
                ));
            }
            if sym_a.polylines.len() != sym_b.polylines.len() {
                changes.push(format!(
                    "polyline_count: {} -> {}",
                    sym_a.polylines.len(),
                    sym_b.polylines.len()
                ));
            }
            if sym_a.arcs.len() != sym_b.arcs.len() {
                changes.push(format!(
                    "arc_count: {} -> {}",
                    sym_a.arcs.len(),
                    sym_b.arcs.len()
                ));
            }
            if sym_a.footprints.len() != sym_b.footprints.len() {
                changes.push(format!(
                    "footprint_count: {} -> {}",
                    sym_a.footprints.len(),
                    sym_b.footprints.len()
                ));
            }

            if !changes.is_empty() {
                modified.push(json!({
                    "name": name,
                    "changes": changes,
                }));
            }
        }

        let result = json!({
            "status": "success",
            "file_type": "SchLib",
            "filepath_a": filepath_a,
            "filepath_b": filepath_b,
            "summary": {
                "components_in_a": lib_a.len(),
                "components_in_b": lib_b.len(),
                "added_count": added.len(),
                "removed_count": removed.len(),
                "modified_count": modified.len(),
                "unchanged_count": common.len() - modified.len(),
            },
            "added": added,
            "removed": removed,
            "modified": modified,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }
}

#[cfg(test)]
mod tests {

    use crate::altium::pcblib::{Footprint, Pad, PcbLib};
    use crate::altium::schlib::{Pin, PinOrientation, SchLib, Symbol};
    use crate::mcp::tools::test_support::{
        create_test_pcblib, create_test_schlib, create_test_server, get_result_text,
        parse_result_json, test_temp_dir,
    };
    use serde_json::json;

    #[test]
    fn diff_libraries_missing_parameters() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let result = server.call_diff_libraries(&json!({}));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: filepath_a"
        );

        let result = server.call_diff_libraries(&json!({ "filepath_a": "a.PcbLib" }));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: filepath_b"
        );
    }

    #[test]
    fn diff_libraries_rejects_path_outside_allowed() {
        let dir = test_temp_dir();
        let other = test_temp_dir();
        let server = create_test_server(dir.path());
        let outside = other.path().join("Outside.PcbLib");
        create_test_pcblib(&outside);

        let inside = dir.path().join("Inside.PcbLib");
        create_test_pcblib(&inside);

        let result = server.call_diff_libraries(&json!({
            "filepath_a": outside.to_string_lossy(),
            "filepath_b": inside.to_string_lossy(),
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Access denied"));
    }

    #[test]
    fn diff_libraries_rejects_mismatched_extensions() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let pcb = dir.path().join("A.PcbLib");
        let sch = dir.path().join("B.SchLib");
        create_test_pcblib(&pcb);
        create_test_schlib(&sch);

        let result = server.call_diff_libraries(&json!({
            "filepath_a": pcb.to_string_lossy(),
            "filepath_b": sch.to_string_lossy(),
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("File types must match"));
    }

    #[test]
    fn diff_libraries_rejects_unknown_extension() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let a = dir.path().join("A.txt");
        let b = dir.path().join("B.txt");
        std::fs::write(&a, b"x").unwrap();
        std::fs::write(&b, b"x").unwrap();

        let result = server.call_diff_libraries(&json!({
            "filepath_a": a.to_string_lossy(),
            "filepath_b": b.to_string_lossy(),
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unknown file type"));
    }

    #[test]
    fn diff_pcblibs_reports_added_removed_and_modified() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        // Library A: the standard two-footprint fixture.
        let path_a = dir.path().join("A.PcbLib");
        create_test_pcblib(&path_a);

        // Library B: CHIP_0402 modified (extra pad, new description),
        // CHIP_0603 removed, CHIP_0805 added.
        let mut lib_b = PcbLib::new();
        let mut fp1 = Footprint::new("CHIP_0402");
        fp1.description = "modified".to_string();
        fp1.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        fp1.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
        fp1.add_pad(Pad::smd("3", 1.5, 0.0, 0.6, 0.5));
        lib_b.add(fp1);
        let mut fp3 = Footprint::new("CHIP_0805");
        fp3.add_pad(Pad::smd("1", -1.0, 0.0, 1.0, 1.3));
        lib_b.add(fp3);
        let path_b = dir.path().join("B.PcbLib");
        lib_b.save(&path_b).unwrap();

        let result = server.call_diff_libraries(&json!({
            "filepath_a": path_a.to_string_lossy(),
            "filepath_b": path_b.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));

        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["file_type"], "PcbLib");
        assert_eq!(parsed["summary"]["components_in_a"], 2);
        assert_eq!(parsed["summary"]["components_in_b"], 2);
        assert_eq!(parsed["summary"]["added_count"], 1);
        assert_eq!(parsed["summary"]["removed_count"], 1);
        assert_eq!(parsed["summary"]["modified_count"], 1);
        assert_eq!(parsed["summary"]["unchanged_count"], 0);
        assert_eq!(parsed["added"], json!(["CHIP_0805"]));
        assert_eq!(parsed["removed"], json!(["CHIP_0603"]));
        assert_eq!(parsed["modified"][0]["name"], "CHIP_0402");
        let changes = parsed["modified"][0]["changes"]
            .as_array()
            .expect("changes array");
        assert!(changes
            .iter()
            .any(|c| c.as_str().unwrap().starts_with("description:")));
        assert!(changes
            .iter()
            .any(|c| c.as_str().unwrap() == "pad_count: 2 -> 3"));
    }

    #[test]
    fn diff_pcblibs_identical_libraries_report_no_changes() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path_a = dir.path().join("A.PcbLib");
        let path_b = dir.path().join("B.PcbLib");
        create_test_pcblib(&path_a);
        create_test_pcblib(&path_b);

        let result = server.call_diff_libraries(&json!({
            "filepath_a": path_a.to_string_lossy(),
            "filepath_b": path_b.to_string_lossy(),
        }));
        assert!(!result.is_error);

        let parsed = parse_result_json(&result);
        assert_eq!(parsed["summary"]["added_count"], 0);
        assert_eq!(parsed["summary"]["removed_count"], 0);
        assert_eq!(parsed["summary"]["modified_count"], 0);
        assert_eq!(parsed["summary"]["unchanged_count"], 2);
    }

    #[test]
    fn diff_pcblibs_unreadable_file_is_an_error() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path_a = dir.path().join("A.PcbLib");
        create_test_pcblib(&path_a);
        let missing = dir.path().join("Missing.PcbLib");

        let result = server.call_diff_libraries(&json!({
            "filepath_a": missing.to_string_lossy(),
            "filepath_b": path_a.to_string_lossy(),
        }));
        assert!(result.is_error);
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "error");
    }

    #[test]
    fn diff_schlibs_reports_added_removed_and_modified() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let path_a = dir.path().join("A.SchLib");
        create_test_schlib(&path_a);

        // Library B: RESISTOR modified (designator + extra pin), CAPACITOR
        // removed, INDUCTOR added.
        let mut lib_b = SchLib::new();
        let mut sym1 = Symbol::new("RESISTOR");
        sym1.description = "Generic resistor".to_string();
        sym1.designator = "RES?".to_string();
        sym1.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
        sym1.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
        sym1.add_pin(Pin::new("3", "3", 0, 20, 10, PinOrientation::Up));
        lib_b.add(sym1);
        let mut sym3 = Symbol::new("INDUCTOR");
        sym3.designator = "L?".to_string();
        sym3.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
        lib_b.add(sym3);
        let path_b = dir.path().join("B.SchLib");
        lib_b.save(&path_b).unwrap();

        let result = server.call_diff_libraries(&json!({
            "filepath_a": path_a.to_string_lossy(),
            "filepath_b": path_b.to_string_lossy(),
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));

        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["file_type"], "SchLib");
        assert_eq!(parsed["summary"]["added_count"], 1);
        assert_eq!(parsed["summary"]["removed_count"], 1);
        assert_eq!(parsed["summary"]["modified_count"], 1);
        assert_eq!(parsed["added"], json!(["INDUCTOR"]));
        assert_eq!(parsed["removed"], json!(["CAPACITOR"]));
        assert_eq!(parsed["modified"][0]["name"], "RESISTOR");
        let changes = parsed["modified"][0]["changes"]
            .as_array()
            .expect("changes array");
        assert!(changes
            .iter()
            .any(|c| c.as_str().unwrap() == "designator: 'R?' -> 'RES?'"));
        assert!(changes
            .iter()
            .any(|c| c.as_str().unwrap() == "pin_count: 2 -> 3"));
        // The fixture rectangle only exists in A's RESISTOR.
        assert!(changes
            .iter()
            .any(|c| c.as_str().unwrap() == "rectangle_count: 1 -> 0"));
    }

    // ==================== per-family count-change detail ====================

    mod field_diffs {
        use super::*;
        use crate::altium::pcblib::PcbFlags;
        use crate::altium::pcblib::{
            ComponentBody, Layer, Region, Text, TextJustification, TextKind, Track,
        };
        use crate::altium::schlib::{FootprintModel, Line, Polyline, Rectangle, ShapeDisplayFlags};

        fn pcb_text() -> Text {
            Text {
                x: 0.0,
                y: 0.6,
                text: "R".to_string(),
                height: 0.3,
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
            }
        }

        fn sch_arc() -> crate::altium::schlib::Arc {
            crate::altium::schlib::Arc {
                x: 0.0,
                y: 0.0,
                radius: 6.0,
                is_not_accessible: true,
                start_angle: 0.0,
                end_angle: 180.0,
                line_width: 1,
                color: 0,
                fill_color: 0,
                owner_part_id: 1,
                display_flags: ShapeDisplayFlags::default(),
                unique_id: None,
            }
        }

        fn sch_poly() -> Polyline {
            Polyline {
                points: vec![(0.0, 0.0), (5.0, 5.0)],
                line_width: 1,
                color: 0,
                line_style: 0,
                start_line_shape: 0,
                end_line_shape: 0,
                line_shape_size: 0,
                transparent: false,
                is_not_accessible: true,
                owner_part_id: 1,
                display_flags: ShapeDisplayFlags::default(),
                unique_id: None,
            }
        }

        /// Change strings for the modified component named `name`.
        fn changes_for(parsed: &serde_json::Value, name: &str) -> Vec<String> {
            parsed["modified"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["name"] == name)
                .expect("modified entry present")["changes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap_or("").to_string())
                .collect()
        }

        #[test]
        fn diff_pcblibs_reports_all_family_count_changes() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path_a = dir.path().join("A.PcbLib");
            create_test_pcblib(&path_a);

            // B: CHIP_0402 same pads/description but one of each new family;
            // CHIP_0603 rebuilt identically so it stays unchanged.
            let mut lib_b = PcbLib::new();
            let mut fp = Footprint::new("CHIP_0402");
            fp.description = "0402 chip resistor".to_string();
            fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
            fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
            fp.add_track(Track::new(-0.5, 0.5, 0.5, 0.5, 0.15, Layer::TopOverlay));
            fp.add_arc(crate::altium::pcblib::Arc::circle(
                0.0,
                0.0,
                0.5,
                0.1,
                Layer::TopOverlay,
            ));
            fp.add_region(Region::rectangle(-0.5, -0.5, 0.5, 0.5, Layer::TopOverlay));
            fp.add_text(pcb_text());
            fp.add_component_body(ComponentBody::new(
                "{AAAA0000-0000-0000-0000-000000000000}",
                "m.step",
            ));
            lib_b.add(fp);
            let mut fp2 = Footprint::new("CHIP_0603");
            fp2.description = "0603 chip resistor".to_string();
            fp2.add_pad(Pad::smd("1", -0.8, 0.0, 0.8, 0.8));
            fp2.add_pad(Pad::smd("2", 0.8, 0.0, 0.8, 0.8));
            lib_b.add(fp2);
            let path_b = dir.path().join("B.PcbLib");
            lib_b.save(&path_b).unwrap();

            let r = server.call_diff_libraries(&json!({
                "filepath_a": path_a.to_string_lossy(),
                "filepath_b": path_b.to_string_lossy(),
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["summary"]["modified_count"], 1);
            assert_eq!(p["summary"]["unchanged_count"], 1);
            let changes = changes_for(&p, "CHIP_0402");
            for expected in [
                "track_count: 0 -> 1",
                "arc_count: 0 -> 1",
                "region_count: 0 -> 1",
                "text_count: 0 -> 1",
                "component_body_count: 0 -> 1",
            ] {
                assert!(
                    changes.iter().any(|c| c == expected),
                    "missing {expected}: {changes:?}"
                );
            }
        }

        #[test]
        fn diff_pcblibs_lib_b_unreadable_is_error() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path_a = dir.path().join("Good.PcbLib");
            create_test_pcblib(&path_a);
            let r = server.call_diff_libraries(&json!({
                "filepath_a": path_a.to_string_lossy(),
                "filepath_b": dir.path().join("Missing.PcbLib").to_string_lossy(),
            }));
            assert!(r.is_error);
            assert_eq!(parse_result_json(&r)["status"], "error");
        }

        #[test]
        fn diff_schlibs_open_errors_on_either_side() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let good = dir.path().join("Good.SchLib");
            create_test_schlib(&good);
            let missing = dir.path().join("Missing.SchLib");

            let a_bad = server.call_diff_libraries(&json!({
                "filepath_a": missing.to_string_lossy(),
                "filepath_b": good.to_string_lossy(),
            }));
            assert!(a_bad.is_error);
            let b_bad = server.call_diff_libraries(&json!({
                "filepath_a": good.to_string_lossy(),
                "filepath_b": missing.to_string_lossy(),
            }));
            assert!(b_bad.is_error);
        }

        #[test]
        fn diff_schlibs_reports_all_family_count_changes() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path_a = dir.path().join("A.SchLib");
            create_test_schlib(&path_a);

            let mut lib_b = SchLib::new();
            let mut sym = Symbol::new("RESISTOR");
            sym.description = "Precision resistor".to_string(); // description change
            sym.designator = "R?".to_string();
            sym.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
            sym.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
            sym.add_rectangle(Rectangle::new(-10, -5, 10, 5));
            sym.add_line(Line::new(-10, 0, 10, 0));
            sym.add_polyline(sch_poly());
            sym.add_arc(sch_arc());
            sym.add_footprint(FootprintModel::new("0402"));
            lib_b.add(sym);
            // CAPACITOR identical to fixture -> unchanged.
            let mut cap = Symbol::new("CAPACITOR");
            cap.description = "Generic capacitor".to_string();
            cap.designator = "C?".to_string();
            cap.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
            cap.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
            lib_b.add(cap);
            let path_b = dir.path().join("B.SchLib");
            lib_b.save(&path_b).unwrap();

            let r = server.call_diff_libraries(&json!({
                "filepath_a": path_a.to_string_lossy(),
                "filepath_b": path_b.to_string_lossy(),
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let changes = changes_for(&parse_result_json(&r), "RESISTOR");
            assert!(changes.iter().any(|c| c.starts_with("description:")));
            for expected in [
                "line_count: 0 -> 1",
                "polyline_count: 0 -> 1",
                "arc_count: 0 -> 1",
                "footprint_count: 0 -> 1",
            ] {
                assert!(
                    changes.iter().any(|c| c == expected),
                    "missing {expected}: {changes:?}"
                );
            }
        }
    }
}
