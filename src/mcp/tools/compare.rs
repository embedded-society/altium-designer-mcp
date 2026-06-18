//! Component comparison tools, split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

impl McpServer {
    /// Compares two specific components in detail.
    pub(crate) fn call_compare_components(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath_a) = arguments.get("filepath_a").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath_a");
        };
        let Some(component_a) = arguments.get("component_a").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_a");
        };
        let Some(filepath_b) = arguments.get("filepath_b").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath_b");
        };
        let Some(component_b) = arguments.get("component_b").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_b");
        };

        // Validate paths
        if let Err(e) = self.validate_path(filepath_a) {
            return ToolCallResult::error(e);
        }
        if let Err(e) = self.validate_path(filepath_b) {
            return ToolCallResult::error(e);
        }

        let include_geometry = arguments
            .get("include_geometry")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let tolerance = arguments
            .get("tolerance")
            .and_then(Value::as_f64)
            .unwrap_or(0.001);

        // Determine file types
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
            return ToolCallResult::error(format!(
                "File types must match. Got '{}' and '{}'",
                ext_a.as_deref().unwrap_or("unknown"),
                ext_b.as_deref().unwrap_or("unknown")
            ));
        }

        match ext_a.as_deref() {
            Some("pcblib") => Self::compare_footprints(
                filepath_a,
                component_a,
                filepath_b,
                component_b,
                include_geometry,
                tolerance,
            ),
            Some("schlib") => Self::compare_symbols(
                filepath_a,
                component_a,
                filepath_b,
                component_b,
                include_geometry,
            ),
            _ => ToolCallResult::error("Unknown file type. Expected .PcbLib or .SchLib extension."),
        }
    }

    /// Compares two footprints in detail.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn compare_footprints(
        filepath_a: &str,
        name_a: &str,
        filepath_b: &str,
        name_b: &str,
        include_geometry: bool,
        tolerance: f64,
    ) -> ToolCallResult {
        use crate::altium::PcbLib;

        // Read libraries
        let lib_a = match PcbLib::open(filepath_a) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read '{filepath_a}': {e}")),
        };
        let lib_b = match PcbLib::open(filepath_b) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read '{filepath_b}': {e}")),
        };

        // Get components
        let Some(fp_a) = lib_a.get(name_a) else {
            return ToolCallResult::error(format!(
                "Component '{name_a}' not found in '{filepath_a}'"
            ));
        };
        let Some(fp_b) = lib_b.get(name_b) else {
            return ToolCallResult::error(format!(
                "Component '{name_b}' not found in '{filepath_b}'"
            ));
        };

        let mut differences: Vec<Value> = Vec::new();

        // Compare description
        if fp_a.description != fp_b.description {
            differences.push(json!({
                "field": "description",
                "component_a": fp_a.description,
                "component_b": fp_b.description
            }));
        }

        // Compare pad counts
        if fp_a.pads.len() != fp_b.pads.len() {
            differences.push(json!({
                "field": "pad_count",
                "component_a": fp_a.pads.len(),
                "component_b": fp_b.pads.len()
            }));
        }

        // Compare pads in detail
        if include_geometry {
            let pad_diffs = Self::compare_pads(&fp_a.pads, &fp_b.pads, tolerance);
            if !pad_diffs.is_empty() {
                differences.push(json!({
                    "field": "pads",
                    "differences": pad_diffs
                }));
            }
        }

        // Compare track counts
        if fp_a.tracks.len() != fp_b.tracks.len() {
            differences.push(json!({
                "field": "track_count",
                "component_a": fp_a.tracks.len(),
                "component_b": fp_b.tracks.len()
            }));
        }

        // Compare tracks in detail
        if include_geometry {
            let track_diffs = Self::compare_tracks(&fp_a.tracks, &fp_b.tracks, tolerance);
            if !track_diffs.is_empty() {
                differences.push(json!({
                    "field": "tracks",
                    "differences": track_diffs
                }));
            }
        }

        // Compare arc counts
        if fp_a.arcs.len() != fp_b.arcs.len() {
            differences.push(json!({
                "field": "arc_count",
                "component_a": fp_a.arcs.len(),
                "component_b": fp_b.arcs.len()
            }));
        }

        // Compare arcs in detail
        if include_geometry {
            let arc_diffs = Self::compare_pcb_arcs(&fp_a.arcs, &fp_b.arcs, tolerance);
            if !arc_diffs.is_empty() {
                differences.push(json!({
                    "field": "arcs",
                    "differences": arc_diffs
                }));
            }
        }

        // Compare region counts
        if fp_a.regions.len() != fp_b.regions.len() {
            differences.push(json!({
                "field": "region_count",
                "component_a": fp_a.regions.len(),
                "component_b": fp_b.regions.len()
            }));
        }

        // Compare text counts
        if fp_a.text.len() != fp_b.text.len() {
            differences.push(json!({
                "field": "text_count",
                "component_a": fp_a.text.len(),
                "component_b": fp_b.text.len()
            }));
        }

        // Compare 3D model references
        let has_model_a = fp_a.model_3d.is_some();
        let has_model_b = fp_b.model_3d.is_some();
        if has_model_a != has_model_b {
            differences.push(json!({
                "field": "external_3d_model",
                "component_a": has_model_a,
                "component_b": has_model_b
            }));
        } else if has_model_a && has_model_b {
            let m_a = fp_a.model_3d.as_ref().unwrap();
            let m_b = fp_b.model_3d.as_ref().unwrap();
            if m_a.filepath != m_b.filepath {
                differences.push(json!({
                    "field": "3d_model_path",
                    "component_a": m_a.filepath,
                    "component_b": m_b.filepath
                }));
            }
        }

        // Compare component bodies count
        if fp_a.component_bodies.len() != fp_b.component_bodies.len() {
            differences.push(json!({
                "field": "component_body_count",
                "component_a": fp_a.component_bodies.len(),
                "component_b": fp_b.component_bodies.len()
            }));
        }

        let is_identical = differences.is_empty();

        let result = json!({
            "status": "success",
            "file_type": "PcbLib",
            "component_a": {
                "filepath": filepath_a,
                "name": name_a
            },
            "component_b": {
                "filepath": filepath_b,
                "name": name_b
            },
            "identical": is_identical,
            "difference_count": differences.len(),
            "differences": differences,
            "summary": {
                "pads_a": fp_a.pads.len(),
                "pads_b": fp_b.pads.len(),
                "tracks_a": fp_a.tracks.len(),
                "tracks_b": fp_b.tracks.len(),
                "arcs_a": fp_a.arcs.len(),
                "arcs_b": fp_b.arcs.len(),
                "regions_a": fp_a.regions.len(),
                "regions_b": fp_b.regions.len(),
                "text_a": fp_a.text.len(),
                "text_b": fp_b.text.len()
            }
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Compares two lists of pads.
    pub(crate) fn compare_pads(
        pads_a: &[crate::altium::pcblib::Pad],
        pads_b: &[crate::altium::pcblib::Pad],
        tolerance: f64,
    ) -> Vec<Value> {
        use std::collections::HashMap;

        let mut diffs = Vec::new();

        // Index pads by designator
        let map_a: HashMap<&str, &crate::altium::pcblib::Pad> =
            pads_a.iter().map(|p| (p.designator.as_str(), p)).collect();
        let map_b: HashMap<&str, &crate::altium::pcblib::Pad> =
            pads_b.iter().map(|p| (p.designator.as_str(), p)).collect();

        // Find pads only in A
        for des in map_a.keys() {
            if !map_b.contains_key(des) {
                diffs.push(json!({
                    "designator": des,
                    "status": "only_in_a"
                }));
            }
        }

        // Find pads only in B
        for des in map_b.keys() {
            if !map_a.contains_key(des) {
                diffs.push(json!({
                    "designator": des,
                    "status": "only_in_b"
                }));
            }
        }

        // Compare matching pads
        for (des, pad_a) in &map_a {
            if let Some(pad_b) = map_b.get(des) {
                let mut changes = Vec::new();

                // Compare position
                if (pad_a.x - pad_b.x).abs() > tolerance || (pad_a.y - pad_b.y).abs() > tolerance {
                    changes.push(json!({
                        "property": "position",
                        "a": { "x": pad_a.x, "y": pad_a.y },
                        "b": { "x": pad_b.x, "y": pad_b.y }
                    }));
                }

                // Compare size
                if (pad_a.width - pad_b.width).abs() > tolerance
                    || (pad_a.height - pad_b.height).abs() > tolerance
                {
                    changes.push(json!({
                        "property": "size",
                        "a": { "width": pad_a.width, "height": pad_a.height },
                        "b": { "width": pad_b.width, "height": pad_b.height }
                    }));
                }

                // Compare shape
                if pad_a.shape != pad_b.shape {
                    changes.push(json!({
                        "property": "shape",
                        "a": format!("{:?}", pad_a.shape),
                        "b": format!("{:?}", pad_b.shape)
                    }));
                }

                // Compare layer
                if pad_a.layer != pad_b.layer {
                    changes.push(json!({
                        "property": "layer",
                        "a": pad_a.layer,
                        "b": pad_b.layer
                    }));
                }

                // Compare hole size
                let hole_diff = match (pad_a.hole_size, pad_b.hole_size) {
                    (Some(a), Some(b)) => (a - b).abs() > tolerance,
                    (None, None) => false,
                    _ => true,
                };
                if hole_diff {
                    changes.push(json!({
                        "property": "hole_size",
                        "a": pad_a.hole_size,
                        "b": pad_b.hole_size
                    }));
                }

                // Compare rotation
                if (pad_a.rotation - pad_b.rotation).abs() > tolerance {
                    changes.push(json!({
                        "property": "rotation",
                        "a": pad_a.rotation,
                        "b": pad_b.rotation
                    }));
                }

                if !changes.is_empty() {
                    diffs.push(json!({
                        "designator": des,
                        "status": "modified",
                        "changes": changes
                    }));
                }
            }
        }

        diffs
    }

    /// Compares two lists of tracks.
    pub(crate) fn compare_tracks(
        tracks_a: &[crate::altium::pcblib::Track],
        tracks_b: &[crate::altium::pcblib::Track],
        tolerance: f64,
    ) -> Vec<Value> {
        let mut diffs = Vec::new();

        // For tracks, we compare by matching start/end coordinates
        // Since tracks don't have unique identifiers, we'll report aggregate differences
        let mut matched_b: Vec<bool> = vec![false; tracks_b.len()];

        for (i, track_a) in tracks_a.iter().enumerate() {
            let mut found_match = false;

            for (j, track_b) in tracks_b.iter().enumerate() {
                if matched_b[j] {
                    continue;
                }

                // Check if tracks match (same endpoints within tolerance)
                let same_forward = (track_a.x1 - track_b.x1).abs() <= tolerance
                    && (track_a.y1 - track_b.y1).abs() <= tolerance
                    && (track_a.x2 - track_b.x2).abs() <= tolerance
                    && (track_a.y2 - track_b.y2).abs() <= tolerance;

                let same_reverse = (track_a.x1 - track_b.x2).abs() <= tolerance
                    && (track_a.y1 - track_b.y2).abs() <= tolerance
                    && (track_a.x2 - track_b.x1).abs() <= tolerance
                    && (track_a.y2 - track_b.y1).abs() <= tolerance;

                if same_forward || same_reverse {
                    matched_b[j] = true;
                    found_match = true;

                    // Check for width/layer differences
                    let mut changes = Vec::new();
                    if (track_a.width - track_b.width).abs() > tolerance {
                        changes.push(json!({
                            "property": "width",
                            "a": track_a.width,
                            "b": track_b.width
                        }));
                    }
                    if track_a.layer != track_b.layer {
                        changes.push(json!({
                            "property": "layer",
                            "a": track_a.layer,
                            "b": track_b.layer
                        }));
                    }

                    if !changes.is_empty() {
                        diffs.push(json!({
                            "track_index": i,
                            "status": "modified",
                            "endpoints": {
                                "x1": track_a.x1, "y1": track_a.y1,
                                "x2": track_a.x2, "y2": track_a.y2
                            },
                            "changes": changes
                        }));
                    }
                    break;
                }
            }

            if !found_match {
                diffs.push(json!({
                    "track_index": i,
                    "status": "only_in_a",
                    "endpoints": {
                        "x1": track_a.x1, "y1": track_a.y1,
                        "x2": track_a.x2, "y2": track_a.y2
                    },
                    "layer": track_a.layer,
                    "width": track_a.width
                }));
            }
        }

        // Report unmatched tracks from B
        for (j, track_b) in tracks_b.iter().enumerate() {
            if !matched_b[j] {
                diffs.push(json!({
                    "track_index": j,
                    "status": "only_in_b",
                    "endpoints": {
                        "x1": track_b.x1, "y1": track_b.y1,
                        "x2": track_b.x2, "y2": track_b.y2
                    },
                    "layer": track_b.layer,
                    "width": track_b.width
                }));
            }
        }

        diffs
    }

    /// Compares two lists of PCB arcs.
    pub(crate) fn compare_pcb_arcs(
        arcs_a: &[crate::altium::pcblib::Arc],
        arcs_b: &[crate::altium::pcblib::Arc],
        tolerance: f64,
    ) -> Vec<Value> {
        let mut diffs = Vec::new();
        let mut matched_b: Vec<bool> = vec![false; arcs_b.len()];

        for (i, arc_a) in arcs_a.iter().enumerate() {
            let mut found_match = false;

            for (j, arc_b) in arcs_b.iter().enumerate() {
                if matched_b[j] {
                    continue;
                }

                // Match by centre and radius
                if (arc_a.x - arc_b.x).abs() <= tolerance
                    && (arc_a.y - arc_b.y).abs() <= tolerance
                    && (arc_a.radius - arc_b.radius).abs() <= tolerance
                {
                    matched_b[j] = true;
                    found_match = true;

                    let mut changes = Vec::new();
                    if (arc_a.start_angle - arc_b.start_angle).abs() > tolerance {
                        changes.push(json!({
                            "property": "start_angle",
                            "a": arc_a.start_angle,
                            "b": arc_b.start_angle
                        }));
                    }
                    if (arc_a.end_angle - arc_b.end_angle).abs() > tolerance {
                        changes.push(json!({
                            "property": "end_angle",
                            "a": arc_a.end_angle,
                            "b": arc_b.end_angle
                        }));
                    }
                    if (arc_a.width - arc_b.width).abs() > tolerance {
                        changes.push(json!({
                            "property": "width",
                            "a": arc_a.width,
                            "b": arc_b.width
                        }));
                    }
                    if arc_a.layer != arc_b.layer {
                        changes.push(json!({
                            "property": "layer",
                            "a": arc_a.layer,
                            "b": arc_b.layer
                        }));
                    }

                    if !changes.is_empty() {
                        diffs.push(json!({
                            "arc_index": i,
                            "status": "modified",
                            "centre": { "x": arc_a.x, "y": arc_a.y },
                            "radius": arc_a.radius,
                            "changes": changes
                        }));
                    }
                    break;
                }
            }

            if !found_match {
                diffs.push(json!({
                    "arc_index": i,
                    "status": "only_in_a",
                    "centre": { "x": arc_a.x, "y": arc_a.y },
                    "radius": arc_a.radius,
                    "layer": arc_a.layer
                }));
            }
        }

        for (j, arc_b) in arcs_b.iter().enumerate() {
            if !matched_b[j] {
                diffs.push(json!({
                    "arc_index": j,
                    "status": "only_in_b",
                    "centre": { "x": arc_b.x, "y": arc_b.y },
                    "radius": arc_b.radius,
                    "layer": arc_b.layer
                }));
            }
        }

        diffs
    }

    /// Compares two symbols in detail.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn compare_symbols(
        filepath_a: &str,
        name_a: &str,
        filepath_b: &str,
        name_b: &str,
        include_geometry: bool,
    ) -> ToolCallResult {
        use crate::altium::SchLib;

        // Read libraries
        let lib_a = match SchLib::open(filepath_a) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read '{filepath_a}': {e}")),
        };
        let lib_b = match SchLib::open(filepath_b) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read '{filepath_b}': {e}")),
        };

        // Get components
        let Some(sym_a) = lib_a.get(name_a) else {
            return ToolCallResult::error(format!(
                "Component '{name_a}' not found in '{filepath_a}'"
            ));
        };
        let Some(sym_b) = lib_b.get(name_b) else {
            return ToolCallResult::error(format!(
                "Component '{name_b}' not found in '{filepath_b}'"
            ));
        };

        let mut differences: Vec<Value> = Vec::new();

        // Compare description
        if sym_a.description != sym_b.description {
            differences.push(json!({
                "field": "description",
                "component_a": sym_a.description,
                "component_b": sym_b.description
            }));
        }

        // Compare designator
        if sym_a.designator != sym_b.designator {
            differences.push(json!({
                "field": "designator",
                "component_a": sym_a.designator,
                "component_b": sym_b.designator
            }));
        }

        // Compare pin counts
        if sym_a.pins.len() != sym_b.pins.len() {
            differences.push(json!({
                "field": "pin_count",
                "component_a": sym_a.pins.len(),
                "component_b": sym_b.pins.len()
            }));
        }

        // Compare pins in detail
        if include_geometry {
            let pin_diffs = Self::compare_pins(&sym_a.pins, &sym_b.pins);
            if !pin_diffs.is_empty() {
                differences.push(json!({
                    "field": "pins",
                    "differences": pin_diffs
                }));
            }
        }

        // Compare rectangle counts
        if sym_a.rectangles.len() != sym_b.rectangles.len() {
            differences.push(json!({
                "field": "rectangle_count",
                "component_a": sym_a.rectangles.len(),
                "component_b": sym_b.rectangles.len()
            }));
        }

        // Compare line counts
        if sym_a.lines.len() != sym_b.lines.len() {
            differences.push(json!({
                "field": "line_count",
                "component_a": sym_a.lines.len(),
                "component_b": sym_b.lines.len()
            }));
        }

        // Compare polyline counts
        if sym_a.polylines.len() != sym_b.polylines.len() {
            differences.push(json!({
                "field": "polyline_count",
                "component_a": sym_a.polylines.len(),
                "component_b": sym_b.polylines.len()
            }));
        }

        // Compare arc counts
        if sym_a.arcs.len() != sym_b.arcs.len() {
            differences.push(json!({
                "field": "arc_count",
                "component_a": sym_a.arcs.len(),
                "component_b": sym_b.arcs.len()
            }));
        }

        // Compare footprint references
        if sym_a.footprints.len() != sym_b.footprints.len() {
            differences.push(json!({
                "field": "footprint_count",
                "component_a": sym_a.footprints.len(),
                "component_b": sym_b.footprints.len()
            }));
        }

        // Compare footprint names
        if include_geometry {
            let fps_a: std::collections::HashSet<&str> =
                sym_a.footprints.iter().map(|f| f.name.as_str()).collect();
            let fps_b: std::collections::HashSet<&str> =
                sym_b.footprints.iter().map(|f| f.name.as_str()).collect();

            let only_in_a: Vec<_> = fps_a.difference(&fps_b).copied().collect();
            let only_in_b: Vec<_> = fps_b.difference(&fps_a).copied().collect();

            if !only_in_a.is_empty() || !only_in_b.is_empty() {
                differences.push(json!({
                    "field": "footprints",
                    "only_in_a": only_in_a,
                    "only_in_b": only_in_b
                }));
            }
        }

        // Compare parameters
        if include_geometry {
            let param_diffs = Self::compare_parameters(&sym_a.parameters, &sym_b.parameters);
            if !param_diffs.is_empty() {
                differences.push(json!({
                    "field": "parameters",
                    "differences": param_diffs
                }));
            }
        }

        let is_identical = differences.is_empty();

        let result = json!({
            "status": "success",
            "file_type": "SchLib",
            "component_a": {
                "filepath": filepath_a,
                "name": name_a
            },
            "component_b": {
                "filepath": filepath_b,
                "name": name_b
            },
            "identical": is_identical,
            "difference_count": differences.len(),
            "differences": differences,
            "summary": {
                "pins_a": sym_a.pins.len(),
                "pins_b": sym_b.pins.len(),
                "rectangles_a": sym_a.rectangles.len(),
                "rectangles_b": sym_b.rectangles.len(),
                "lines_a": sym_a.lines.len(),
                "lines_b": sym_b.lines.len(),
                "parameters_a": sym_a.parameters.len(),
                "parameters_b": sym_b.parameters.len(),
                "footprints_a": sym_a.footprints.len(),
                "footprints_b": sym_b.footprints.len()
            }
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Compares two lists of pins.
    pub(crate) fn compare_pins(
        pins_a: &[crate::altium::schlib::Pin],
        pins_b: &[crate::altium::schlib::Pin],
    ) -> Vec<Value> {
        use std::collections::HashMap;

        let mut diffs = Vec::new();

        // Index pins by designator
        let map_a: HashMap<&str, &crate::altium::schlib::Pin> =
            pins_a.iter().map(|p| (p.designator.as_str(), p)).collect();
        let map_b: HashMap<&str, &crate::altium::schlib::Pin> =
            pins_b.iter().map(|p| (p.designator.as_str(), p)).collect();

        // Find pins only in A
        for des in map_a.keys() {
            if !map_b.contains_key(des) {
                diffs.push(json!({
                    "designator": des,
                    "status": "only_in_a"
                }));
            }
        }

        // Find pins only in B
        for des in map_b.keys() {
            if !map_a.contains_key(des) {
                diffs.push(json!({
                    "designator": des,
                    "status": "only_in_b"
                }));
            }
        }

        // Compare matching pins
        for (des, pin_a) in &map_a {
            if let Some(pin_b) = map_b.get(des) {
                let mut changes = Vec::new();

                // Compare position
                if pin_a.x != pin_b.x || pin_a.y != pin_b.y {
                    changes.push(json!({
                        "property": "position",
                        "a": { "x": pin_a.x, "y": pin_a.y },
                        "b": { "x": pin_b.x, "y": pin_b.y }
                    }));
                }

                // Compare length
                if pin_a.length != pin_b.length {
                    changes.push(json!({
                        "property": "length",
                        "a": pin_a.length,
                        "b": pin_b.length
                    }));
                }

                // Compare name
                if pin_a.name != pin_b.name {
                    changes.push(json!({
                        "property": "name",
                        "a": pin_a.name,
                        "b": pin_b.name
                    }));
                }

                // Compare electrical type
                if pin_a.electrical_type != pin_b.electrical_type {
                    changes.push(json!({
                        "property": "electrical_type",
                        "a": format!("{:?}", pin_a.electrical_type),
                        "b": format!("{:?}", pin_b.electrical_type)
                    }));
                }

                // Compare orientation
                if pin_a.orientation != pin_b.orientation {
                    changes.push(json!({
                        "property": "orientation",
                        "a": format!("{:?}", pin_a.orientation),
                        "b": format!("{:?}", pin_b.orientation)
                    }));
                }

                if !changes.is_empty() {
                    diffs.push(json!({
                        "designator": des,
                        "status": "modified",
                        "changes": changes
                    }));
                }
            }
        }

        diffs
    }

    /// Compares two lists of parameters.
    pub(crate) fn compare_parameters(
        params_a: &[crate::altium::schlib::Parameter],
        params_b: &[crate::altium::schlib::Parameter],
    ) -> Vec<Value> {
        use std::collections::HashMap;

        let mut diffs = Vec::new();

        // Index parameters by name
        let map_a: HashMap<&str, &crate::altium::schlib::Parameter> =
            params_a.iter().map(|p| (p.name.as_str(), p)).collect();
        let map_b: HashMap<&str, &crate::altium::schlib::Parameter> =
            params_b.iter().map(|p| (p.name.as_str(), p)).collect();

        // Find parameters only in A
        for (name, param) in &map_a {
            if !map_b.contains_key(name) {
                diffs.push(json!({
                    "name": name,
                    "status": "only_in_a",
                    "value": param.value
                }));
            }
        }

        // Find parameters only in B
        for (name, param) in &map_b {
            if !map_a.contains_key(name) {
                diffs.push(json!({
                    "name": name,
                    "status": "only_in_b",
                    "value": param.value
                }));
            }
        }

        // Compare matching parameters
        for (name, param_a) in &map_a {
            if let Some(param_b) = map_b.get(name) {
                if param_a.value != param_b.value {
                    diffs.push(json!({
                        "name": name,
                        "status": "modified",
                        "value_a": param_a.value,
                        "value_b": param_b.value
                    }));
                }
            }
        }

        diffs
    }
}
