//! Component comparison tools, split from `server.rs`.
//!
//! Comparison strategy:
//!
//! - **Keyed primitives** (pads and pins by designator, parameters by name) are
//!   compared through [`compare_keyed`], which tolerates duplicate keys — the
//!   k-th occurrence in A is paired with the k-th in B and every unpaired
//!   occurrence is reported, instead of silently collapsing duplicates into a
//!   `HashMap` and comparing only the last one.
//! - **Geometric primitives** without an identity (tracks, arcs, vias, fills,
//!   regions, text) are greedily matched by their defining geometry within the
//!   caller's tolerance; unmatched items are reported per side.
//! - **`SchLib` graphic shapes** are compared as serialised multisets through
//!   [`compare_serialized`]: any shape without an exact counterpart on the other
//!   side is reported in full, so no modified shape can go unreported.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

/// Compares two keyed primitive lists, tolerating duplicate keys.
///
/// Items are grouped by key on each side (preserving first-appearance order, so
/// the report is deterministic); the k-th occurrence in A is paired with the
/// k-th occurrence in B. Paired items are compared with `compare_pair` (an
/// empty change list means identical); unpaired occurrences are reported as
/// `only_in_a` / `only_in_b`, decorated with the fields from `describe`. When a
/// key occurs more than once on either side, each report entry carries an
/// `occurrence` index so duplicates stay distinguishable.
fn compare_keyed<'a, T>(
    items_a: &'a [T],
    items_b: &'a [T],
    key_field: &str,
    key_of: impl Fn(&'a T) -> &'a str,
    describe: impl Fn(&'a T) -> Vec<(&'static str, Value)>,
    compare_pair: impl Fn(&'a T, &'a T) -> Vec<Value>,
) -> Vec<Value> {
    let group = |items: &'a [T]| {
        let mut map: indexmap::IndexMap<&'a str, Vec<&'a T>> = indexmap::IndexMap::new();
        for item in items {
            map.entry(key_of(item)).or_default().push(item);
        }
        map
    };
    let map_a = group(items_a);
    let map_b = group(items_b);

    let empty: Vec<&T> = Vec::new();
    let mut diffs = Vec::new();
    let keys = map_a
        .keys()
        .copied()
        .chain(map_b.keys().copied().filter(|k| !map_a.contains_key(*k)));
    for key in keys {
        let group_a = map_a.get(key).unwrap_or(&empty);
        let group_b = map_b.get(key).unwrap_or(&empty);
        let duplicated = group_a.len().max(group_b.len()) > 1;
        let paired = group_a.len().min(group_b.len());

        for k in 0..paired {
            let changes = compare_pair(group_a[k], group_b[k]);
            if !changes.is_empty() {
                let mut entry = serde_json::Map::new();
                entry.insert(key_field.to_string(), json!(key));
                entry.insert("status".to_string(), json!("modified"));
                if duplicated {
                    entry.insert("occurrence".to_string(), json!(k));
                }
                entry.insert("changes".to_string(), json!(changes));
                diffs.push(Value::Object(entry));
            }
        }

        for (status, group) in [("only_in_a", group_a), ("only_in_b", group_b)] {
            for (k, item) in group.iter().enumerate().skip(paired) {
                let mut entry = serde_json::Map::new();
                entry.insert(key_field.to_string(), json!(key));
                entry.insert("status".to_string(), json!(status));
                if duplicated {
                    entry.insert("occurrence".to_string(), json!(k));
                }
                for (prop, value) in describe(item) {
                    entry.insert(prop.to_string(), value);
                }
                diffs.push(Value::Object(entry));
            }
        }
    }
    diffs
}

/// Compares two primitive lists as serialised multisets.
///
/// Each item is serialised to JSON; items with an exact counterpart on the
/// other side are matched off, and every leftover is reported in full as
/// `only_in_a` / `only_in_b` (a shape edited in place therefore surfaces as one
/// entry on each side). Used for the `SchLib` graphic shapes, whose integer /
/// 6-decimal-rounded coordinates make exact JSON equality a faithful test.
fn compare_serialized<T: serde::Serialize>(items_a: &[T], items_b: &[T]) -> Vec<Value> {
    let to_values = |items: &[T]| -> Vec<Value> {
        items
            .iter()
            .map(|item| serde_json::to_value(item).unwrap_or(Value::Null))
            .collect()
    };
    let values_a = to_values(items_a);
    let values_b = to_values(items_b);

    let mut matched_b = vec![false; values_b.len()];
    let mut diffs = Vec::new();
    for (i, value_a) in values_a.iter().enumerate() {
        let matched = values_b
            .iter()
            .enumerate()
            .find(|&(j, value_b)| !matched_b[j] && value_b == value_a)
            .map(|(j, _)| j);
        if let Some(j) = matched {
            matched_b[j] = true;
        } else {
            diffs.push(json!({
                "index": i,
                "status": "only_in_a",
                "primitive": value_a
            }));
        }
    }
    for (j, value_b) in values_b.iter().enumerate() {
        if !matched_b[j] {
            diffs.push(json!({
                "index": j,
                "status": "only_in_b",
                "primitive": value_b
            }));
        }
    }
    diffs
}

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

        // Compare via counts
        if fp_a.vias.len() != fp_b.vias.len() {
            differences.push(json!({
                "field": "via_count",
                "component_a": fp_a.vias.len(),
                "component_b": fp_b.vias.len()
            }));
        }

        // Compare vias in detail
        if include_geometry {
            let via_diffs = Self::compare_vias(&fp_a.vias, &fp_b.vias, tolerance);
            if !via_diffs.is_empty() {
                differences.push(json!({
                    "field": "vias",
                    "differences": via_diffs
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

        // Compare regions in detail
        if include_geometry {
            let region_diffs = Self::compare_regions(&fp_a.regions, &fp_b.regions, tolerance);
            if !region_diffs.is_empty() {
                differences.push(json!({
                    "field": "regions",
                    "differences": region_diffs
                }));
            }
        }

        // Compare text counts
        if fp_a.text.len() != fp_b.text.len() {
            differences.push(json!({
                "field": "text_count",
                "component_a": fp_a.text.len(),
                "component_b": fp_b.text.len()
            }));
        }

        // Compare text in detail
        if include_geometry {
            let text_diffs = Self::compare_pcb_text(&fp_a.text, &fp_b.text, tolerance);
            if !text_diffs.is_empty() {
                differences.push(json!({
                    "field": "text",
                    "differences": text_diffs
                }));
            }
        }

        // Compare fill counts
        if fp_a.fills.len() != fp_b.fills.len() {
            differences.push(json!({
                "field": "fill_count",
                "component_a": fp_a.fills.len(),
                "component_b": fp_b.fills.len()
            }));
        }

        // Compare fills in detail
        if include_geometry {
            let fill_diffs = Self::compare_fills(&fp_a.fills, &fp_b.fills, tolerance);
            if !fill_diffs.is_empty() {
                differences.push(json!({
                    "field": "fills",
                    "differences": fill_diffs
                }));
            }
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
                "vias_a": fp_a.vias.len(),
                "vias_b": fp_b.vias.len(),
                "tracks_a": fp_a.tracks.len(),
                "tracks_b": fp_b.tracks.len(),
                "arcs_a": fp_a.arcs.len(),
                "arcs_b": fp_b.arcs.len(),
                "regions_a": fp_a.regions.len(),
                "regions_b": fp_b.regions.len(),
                "text_a": fp_a.text.len(),
                "text_b": fp_b.text.len(),
                "fills_a": fp_a.fills.len(),
                "fills_b": fp_b.fills.len()
            }
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Compares two lists of pads by designator, tolerating duplicate
    /// designators (legal in Altium — e.g. a thermal pad split across several
    /// same-designator pads): every occurrence is compared, none is dropped.
    pub(crate) fn compare_pads(
        pads_a: &[crate::altium::pcblib::Pad],
        pads_b: &[crate::altium::pcblib::Pad],
        tolerance: f64,
    ) -> Vec<Value> {
        compare_keyed(
            pads_a,
            pads_b,
            "designator",
            |p| p.designator.as_str(),
            |_| Vec::new(),
            |pad_a, pad_b| {
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

                changes
            },
        )
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

    /// Compares two lists of vias, matched by position within `tolerance`.
    pub(crate) fn compare_vias(
        vias_a: &[crate::altium::pcblib::Via],
        vias_b: &[crate::altium::pcblib::Via],
        tolerance: f64,
    ) -> Vec<Value> {
        let mut diffs = Vec::new();
        let mut matched_b = vec![false; vias_b.len()];

        for (i, via_a) in vias_a.iter().enumerate() {
            let matched = vias_b
                .iter()
                .enumerate()
                .find(|&(j, via_b)| {
                    !matched_b[j]
                        && (via_a.x - via_b.x).abs() <= tolerance
                        && (via_a.y - via_b.y).abs() <= tolerance
                })
                .map(|(j, _)| j);

            if let Some(j) = matched {
                matched_b[j] = true;
                let counterpart = &vias_b[j];

                let mut changes = Vec::new();
                if (via_a.diameter - counterpart.diameter).abs() > tolerance {
                    changes.push(json!({
                        "property": "diameter",
                        "a": via_a.diameter,
                        "b": counterpart.diameter
                    }));
                }
                if (via_a.hole_size - counterpart.hole_size).abs() > tolerance {
                    changes.push(json!({
                        "property": "hole_size",
                        "a": via_a.hole_size,
                        "b": counterpart.hole_size
                    }));
                }
                if via_a.from_layer != counterpart.from_layer
                    || via_a.to_layer != counterpart.to_layer
                {
                    changes.push(json!({
                        "property": "layer_span",
                        "a": { "from": via_a.from_layer, "to": via_a.to_layer },
                        "b": { "from": counterpart.from_layer, "to": counterpart.to_layer }
                    }));
                }

                if !changes.is_empty() {
                    diffs.push(json!({
                        "via_index": i,
                        "status": "modified",
                        "position": { "x": via_a.x, "y": via_a.y },
                        "changes": changes
                    }));
                }
            } else {
                diffs.push(json!({
                    "via_index": i,
                    "status": "only_in_a",
                    "position": { "x": via_a.x, "y": via_a.y },
                    "diameter": via_a.diameter,
                    "hole_size": via_a.hole_size
                }));
            }
        }

        for (j, via_b) in vias_b.iter().enumerate() {
            if !matched_b[j] {
                diffs.push(json!({
                    "via_index": j,
                    "status": "only_in_b",
                    "position": { "x": via_b.x, "y": via_b.y },
                    "diameter": via_b.diameter,
                    "hole_size": via_b.hole_size
                }));
            }
        }

        diffs
    }

    /// Compares two lists of fills, matched by their corner rectangle within
    /// `tolerance`. Corners are normalised (min/max), so the same rectangle
    /// described from opposite corners still matches.
    pub(crate) fn compare_fills(
        fills_a: &[crate::altium::pcblib::Fill],
        fills_b: &[crate::altium::pcblib::Fill],
        tolerance: f64,
    ) -> Vec<Value> {
        // Normalised corner rectangle: (min_x, min_y, max_x, max_y).
        let rect = |f: &crate::altium::pcblib::Fill| {
            (
                f.x1.min(f.x2),
                f.y1.min(f.y2),
                f.x1.max(f.x2),
                f.y1.max(f.y2),
            )
        };

        let mut diffs = Vec::new();
        let mut matched_b = vec![false; fills_b.len()];

        for (i, fill_a) in fills_a.iter().enumerate() {
            let rect_a = rect(fill_a);
            let matched = fills_b
                .iter()
                .enumerate()
                .find(|&(j, fill_b)| {
                    let rect_b = rect(fill_b);
                    !matched_b[j]
                        && (rect_a.0 - rect_b.0).abs() <= tolerance
                        && (rect_a.1 - rect_b.1).abs() <= tolerance
                        && (rect_a.2 - rect_b.2).abs() <= tolerance
                        && (rect_a.3 - rect_b.3).abs() <= tolerance
                })
                .map(|(j, _)| j);

            if let Some(j) = matched {
                matched_b[j] = true;
                let counterpart = &fills_b[j];

                let mut changes = Vec::new();
                if fill_a.layer != counterpart.layer {
                    changes.push(json!({
                        "property": "layer",
                        "a": fill_a.layer,
                        "b": counterpart.layer
                    }));
                }
                if (fill_a.rotation - counterpart.rotation).abs() > tolerance {
                    changes.push(json!({
                        "property": "rotation",
                        "a": fill_a.rotation,
                        "b": counterpart.rotation
                    }));
                }

                if !changes.is_empty() {
                    diffs.push(json!({
                        "fill_index": i,
                        "status": "modified",
                        "corners": {
                            "x1": fill_a.x1, "y1": fill_a.y1,
                            "x2": fill_a.x2, "y2": fill_a.y2
                        },
                        "changes": changes
                    }));
                }
            } else {
                diffs.push(json!({
                    "fill_index": i,
                    "status": "only_in_a",
                    "corners": {
                        "x1": fill_a.x1, "y1": fill_a.y1,
                        "x2": fill_a.x2, "y2": fill_a.y2
                    },
                    "layer": fill_a.layer
                }));
            }
        }

        for (j, fill_b) in fills_b.iter().enumerate() {
            if !matched_b[j] {
                diffs.push(json!({
                    "fill_index": j,
                    "status": "only_in_b",
                    "corners": {
                        "x1": fill_b.x1, "y1": fill_b.y1,
                        "x2": fill_b.x2, "y2": fill_b.y2
                    },
                    "layer": fill_b.layer
                }));
            }
        }

        diffs
    }

    /// Compares two lists of regions.
    ///
    /// Pass 1 matches regions whose layer and outline agree within `tolerance`
    /// and reports property differences (kind, name, hole count). Pass 2
    /// matches the remainder by layer and vertex count and reports the outline
    /// drift, so a moved region surfaces as `modified` rather than as an
    /// unrelated add/remove pair. Whatever is still unmatched is reported per
    /// side.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn compare_regions(
        regions_a: &[crate::altium::pcblib::Region],
        regions_b: &[crate::altium::pcblib::Region],
        tolerance: f64,
    ) -> Vec<Value> {
        use crate::altium::pcblib::Region;

        let outlines_match = |a: &Region, b: &Region| {
            a.vertices.len() == b.vertices.len()
                && a.vertices.iter().zip(&b.vertices).all(|(va, vb)| {
                    (va.x - vb.x).abs() <= tolerance && (va.y - vb.y).abs() <= tolerance
                })
        };
        let property_changes = |a: &Region, b: &Region| {
            let mut changes = Vec::new();
            if a.kind != b.kind {
                changes.push(json!({
                    "property": "kind",
                    "a": format!("{:?}", a.kind),
                    "b": format!("{:?}", b.kind)
                }));
            }
            if a.name != b.name {
                changes.push(json!({
                    "property": "name",
                    "a": a.name,
                    "b": b.name
                }));
            }
            if a.holes.len() != b.holes.len() {
                changes.push(json!({
                    "property": "hole_count",
                    "a": a.holes.len(),
                    "b": b.holes.len()
                }));
            }
            changes
        };

        let mut diffs = Vec::new();
        let mut matched_a = vec![false; regions_a.len()];
        let mut matched_b = vec![false; regions_b.len()];

        // Pass 1: same layer + same outline → compare properties.
        for (i, reg_a) in regions_a.iter().enumerate() {
            let matched = regions_b
                .iter()
                .enumerate()
                .find(|&(j, reg_b)| {
                    !matched_b[j] && reg_a.layer == reg_b.layer && outlines_match(reg_a, reg_b)
                })
                .map(|(j, _)| j);
            if let Some(j) = matched {
                matched_a[i] = true;
                matched_b[j] = true;
                let changes = property_changes(reg_a, &regions_b[j]);
                if !changes.is_empty() {
                    diffs.push(json!({
                        "region_index": i,
                        "status": "modified",
                        "layer": reg_a.layer,
                        "vertex_count": reg_a.vertices.len(),
                        "changes": changes
                    }));
                }
            }
        }

        // Pass 2: same layer + same vertex count → report the outline drift.
        for (i, reg_a) in regions_a.iter().enumerate() {
            if matched_a[i] {
                continue;
            }
            let matched = regions_b
                .iter()
                .enumerate()
                .find(|&(j, reg_b)| {
                    !matched_b[j]
                        && reg_a.layer == reg_b.layer
                        && reg_a.vertices.len() == reg_b.vertices.len()
                })
                .map(|(j, _)| j);
            if let Some(j) = matched {
                matched_a[i] = true;
                matched_b[j] = true;
                let reg_b = &regions_b[j];

                let mut changes = property_changes(reg_a, reg_b);
                if let Some((k, (va, vb))) = reg_a
                    .vertices
                    .iter()
                    .zip(&reg_b.vertices)
                    .enumerate()
                    .find(|(_, (va, vb))| {
                        (va.x - vb.x).abs() > tolerance || (va.y - vb.y).abs() > tolerance
                    })
                {
                    changes.push(json!({
                        "property": "vertices",
                        "first_mismatch_index": k,
                        "a": { "x": va.x, "y": va.y },
                        "b": { "x": vb.x, "y": vb.y }
                    }));
                }
                diffs.push(json!({
                    "region_index": i,
                    "status": "modified",
                    "layer": reg_a.layer,
                    "vertex_count": reg_a.vertices.len(),
                    "changes": changes
                }));
            }
        }

        // Whatever is left is genuinely one-sided.
        for (i, reg_a) in regions_a.iter().enumerate() {
            if !matched_a[i] {
                diffs.push(json!({
                    "region_index": i,
                    "status": "only_in_a",
                    "layer": reg_a.layer,
                    "vertex_count": reg_a.vertices.len(),
                    "kind": format!("{:?}", reg_a.kind)
                }));
            }
        }
        for (j, reg_b) in regions_b.iter().enumerate() {
            if !matched_b[j] {
                diffs.push(json!({
                    "region_index": j,
                    "status": "only_in_b",
                    "layer": reg_b.layer,
                    "vertex_count": reg_b.vertices.len(),
                    "kind": format!("{:?}", reg_b.kind)
                }));
            }
        }

        diffs
    }

    /// Compares two lists of PCB text items.
    ///
    /// Pass 1 matches items with the same content at the same position (within
    /// `tolerance`) and reports property differences; pass 2 matches the
    /// remainder by content alone, so a moved text surfaces as `modified` with
    /// a position change. Whatever is still unmatched is reported per side.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn compare_pcb_text(
        text_a: &[crate::altium::pcblib::Text],
        text_b: &[crate::altium::pcblib::Text],
        tolerance: f64,
    ) -> Vec<Value> {
        use crate::altium::pcblib::Text;

        let property_changes = |a: &Text, b: &Text| {
            let mut changes = Vec::new();
            if (a.height - b.height).abs() > tolerance {
                changes.push(json!({
                    "property": "height",
                    "a": a.height,
                    "b": b.height
                }));
            }
            if (a.rotation - b.rotation).abs() > tolerance {
                changes.push(json!({
                    "property": "rotation",
                    "a": a.rotation,
                    "b": b.rotation
                }));
            }
            if a.layer != b.layer {
                changes.push(json!({
                    "property": "layer",
                    "a": a.layer,
                    "b": b.layer
                }));
            }
            if a.kind != b.kind {
                changes.push(json!({
                    "property": "kind",
                    "a": format!("{:?}", a.kind),
                    "b": format!("{:?}", b.kind)
                }));
            }
            if a.mirror != b.mirror {
                changes.push(json!({
                    "property": "mirror",
                    "a": a.mirror,
                    "b": b.mirror
                }));
            }
            changes
        };

        let mut diffs = Vec::new();
        let mut matched_a = vec![false; text_a.len()];
        let mut matched_b = vec![false; text_b.len()];

        // Pass 1: same content at the same position → compare properties.
        for (i, item_a) in text_a.iter().enumerate() {
            let matched = text_b
                .iter()
                .enumerate()
                .find(|&(j, item_b)| {
                    !matched_b[j]
                        && item_a.text == item_b.text
                        && (item_a.x - item_b.x).abs() <= tolerance
                        && (item_a.y - item_b.y).abs() <= tolerance
                })
                .map(|(j, _)| j);
            if let Some(j) = matched {
                matched_a[i] = true;
                matched_b[j] = true;
                let changes = property_changes(item_a, &text_b[j]);
                if !changes.is_empty() {
                    diffs.push(json!({
                        "text_index": i,
                        "status": "modified",
                        "text": item_a.text,
                        "changes": changes
                    }));
                }
            }
        }

        // Pass 2: same content anywhere → report the position change too.
        for (i, item_a) in text_a.iter().enumerate() {
            if matched_a[i] {
                continue;
            }
            let matched = text_b
                .iter()
                .enumerate()
                .find(|&(j, item_b)| !matched_b[j] && item_a.text == item_b.text)
                .map(|(j, _)| j);
            if let Some(j) = matched {
                matched_a[i] = true;
                matched_b[j] = true;
                let item_b = &text_b[j];

                let mut changes = vec![json!({
                    "property": "position",
                    "a": { "x": item_a.x, "y": item_a.y },
                    "b": { "x": item_b.x, "y": item_b.y }
                })];
                changes.extend(property_changes(item_a, item_b));
                diffs.push(json!({
                    "text_index": i,
                    "status": "modified",
                    "text": item_a.text,
                    "changes": changes
                }));
            }
        }

        // Whatever is left is genuinely one-sided.
        for (i, item_a) in text_a.iter().enumerate() {
            if !matched_a[i] {
                diffs.push(json!({
                    "text_index": i,
                    "status": "only_in_a",
                    "text": item_a.text,
                    "position": { "x": item_a.x, "y": item_a.y },
                    "layer": item_a.layer
                }));
            }
        }
        for (j, item_b) in text_b.iter().enumerate() {
            if !matched_b[j] {
                diffs.push(json!({
                    "text_index": j,
                    "status": "only_in_b",
                    "text": item_b.text,
                    "position": { "x": item_b.x, "y": item_b.y },
                    "layer": item_b.layer
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

        // Compare part counts (multi-part symbols)
        if sym_a.part_count != sym_b.part_count {
            differences.push(json!({
                "field": "part_count",
                "component_a": sym_a.part_count,
                "component_b": sym_b.part_count
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

        // Compare every graphic-shape family: counts always, full-depth
        // serialised diffs under include_geometry (any shape without an exact
        // counterpart on the other side is reported, so an in-place edit can
        // never go unreported).
        macro_rules! compare_family {
            ($count_field:literal, $field:literal, $family:ident) => {
                if sym_a.$family.len() != sym_b.$family.len() {
                    differences.push(json!({
                        "field": $count_field,
                        "component_a": sym_a.$family.len(),
                        "component_b": sym_b.$family.len()
                    }));
                }
                if include_geometry {
                    let family_diffs = compare_serialized(&sym_a.$family, &sym_b.$family);
                    if !family_diffs.is_empty() {
                        differences.push(json!({
                            "field": $field,
                            "differences": family_diffs
                        }));
                    }
                }
            };
        }
        compare_family!("rectangle_count", "rectangles", rectangles);
        compare_family!("line_count", "lines", lines);
        compare_family!("polyline_count", "polylines", polylines);
        compare_family!("polygon_count", "polygons", polygons);
        compare_family!("arc_count", "arcs", arcs);
        compare_family!("pie_count", "pies", pies);
        compare_family!("image_count", "images", images);
        compare_family!("bezier_count", "beziers", beziers);
        compare_family!("ellipse_count", "ellipses", ellipses);
        compare_family!("round_rect_count", "round_rects", round_rects);
        compare_family!("elliptical_arc_count", "elliptical_arcs", elliptical_arcs);
        compare_family!("label_count", "labels", labels);
        compare_family!("text_count", "text", text);

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

    /// Compares two lists of pins by designator, tolerating duplicate
    /// designators: every occurrence is compared, none is dropped.
    pub(crate) fn compare_pins(
        pins_a: &[crate::altium::schlib::Pin],
        pins_b: &[crate::altium::schlib::Pin],
    ) -> Vec<Value> {
        compare_keyed(
            pins_a,
            pins_b,
            "designator",
            |p| p.designator.as_str(),
            |_| Vec::new(),
            |pin_a, pin_b| {
                let mut changes = Vec::new();

                // Compare position (integer schematic units — exact compare)
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

                changes
            },
        )
    }

    /// Compares two lists of parameters by name, tolerating duplicate names:
    /// every occurrence is compared, none is dropped.
    pub(crate) fn compare_parameters(
        params_a: &[crate::altium::schlib::Parameter],
        params_b: &[crate::altium::schlib::Parameter],
    ) -> Vec<Value> {
        compare_keyed(
            params_a,
            params_b,
            "name",
            |p| p.name.as_str(),
            |p| vec![("value", json!(p.value))],
            |param_a, param_b| {
                if param_a.value == param_b.value {
                    Vec::new()
                } else {
                    vec![json!({
                        "property": "value",
                        "a": param_a.value,
                        "b": param_b.value
                    })]
                }
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::altium::pcblib::{
        Fill, Layer, Pad, PcbFlags, Region, RegionKind, Text, TextJustification, TextKind, Via,
    };
    use crate::altium::schlib::{Parameter, Rectangle};

    /// Builds a minimal stroke text at the given position.
    fn make_text(content: &str, x: f64, y: f64) -> Text {
        Text {
            x,
            y,
            text: content.to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            kind: TextKind::Stroke,
            rotation: 0.0,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::default(),
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::default(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
        }
    }

    #[test]
    fn duplicate_pad_designators_all_occurrences_compared() {
        // Two same-designator pads per side (a legal thermal-pad split); the
        // second occurrence differs. The old HashMap indexing collapsed the
        // group to its last member and could report nothing.
        let pads_a = [
            Pad::smd("9", 0.0, 0.0, 1.0, 1.0),
            Pad::smd("9", 2.0, 0.0, 1.0, 1.0),
        ];
        let mut wider = Pad::smd("9", 2.0, 0.0, 1.5, 1.0);
        wider.rotation = 0.0;
        let pads_b = [Pad::smd("9", 0.0, 0.0, 1.0, 1.0), wider];

        let diffs = McpServer::compare_pads(&pads_a, &pads_b, 0.001);
        assert_eq!(diffs.len(), 1, "exactly the second occurrence differs");
        assert_eq!(diffs[0]["designator"], "9");
        assert_eq!(diffs[0]["status"], "modified");
        assert_eq!(diffs[0]["occurrence"], 1);
    }

    #[test]
    fn duplicate_pad_extra_occurrence_reported_one_sided() {
        // A has two "9" pads, B has one: the unpaired occurrence must be
        // reported instead of vanishing into a same-key HashMap slot.
        let pads_a = [
            Pad::smd("9", 0.0, 0.0, 1.0, 1.0),
            Pad::smd("9", 2.0, 0.0, 1.0, 1.0),
        ];
        let pads_b = [Pad::smd("9", 0.0, 0.0, 1.0, 1.0)];

        let diffs = McpServer::compare_pads(&pads_a, &pads_b, 0.001);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0]["status"], "only_in_a");
        assert_eq!(diffs[0]["occurrence"], 1);
    }

    #[test]
    fn unique_pad_entries_keep_plain_shape_and_order() {
        // Unique designators keep the historical entry shape (no `occurrence`
        // key) and are reported in first-appearance order (deterministic).
        let pads_a = [
            Pad::smd("1", 0.0, 0.0, 1.0, 1.0),
            Pad::smd("2", 2.0, 0.0, 1.0, 1.0),
        ];
        let pads_b = [
            Pad::smd("1", 0.5, 0.0, 1.0, 1.0),
            Pad::smd("2", 2.5, 0.0, 1.0, 1.0),
        ];

        let diffs = McpServer::compare_pads(&pads_a, &pads_b, 0.001);
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[0]["designator"], "1");
        assert_eq!(diffs[1]["designator"], "2");
        assert!(
            !diffs[0].as_object().unwrap().contains_key("occurrence"),
            "unique keys must not carry an occurrence index"
        );
    }

    #[test]
    fn duplicate_parameter_names_all_occurrences_compared() {
        let params_a = [Parameter::new("Value", "1k"), Parameter::new("Value", "2k")];
        let params_b = [Parameter::new("Value", "1k"), Parameter::new("Value", "3k")];

        let diffs = McpServer::compare_parameters(&params_a, &params_b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0]["status"], "modified");
        assert_eq!(diffs[0]["occurrence"], 1);
        assert_eq!(diffs[0]["changes"][0]["property"], "value");
        assert_eq!(diffs[0]["changes"][0]["a"], "2k");
        assert_eq!(diffs[0]["changes"][0]["b"], "3k");
    }

    #[test]
    fn region_kind_change_reported_as_modified() {
        let region_a = Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopCourtyard);
        let mut region_b = region_a.clone();
        region_b.kind = RegionKind::Cutout;

        let diffs = McpServer::compare_regions(&[region_a], &[region_b], 0.001);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0]["status"], "modified");
        assert_eq!(diffs[0]["changes"][0]["property"], "kind");
    }

    #[test]
    fn region_moved_vertex_reported_as_outline_drift() {
        let region_a = Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopCourtyard);
        let mut region_b = region_a.clone();
        region_b.vertices[0].x += 0.5;

        let diffs = McpServer::compare_regions(&[region_a], &[region_b], 0.001);
        assert_eq!(
            diffs.len(),
            1,
            "a moved region is one modification, not an add/remove pair"
        );
        assert_eq!(diffs[0]["status"], "modified");
        let changes = diffs[0]["changes"].as_array().unwrap();
        assert!(changes.iter().any(|c| c["property"] == "vertices"));
    }

    #[test]
    fn region_layer_change_reported_per_side() {
        let region_a = Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopCourtyard);
        let mut region_b = region_a.clone();
        region_b.layer = Layer::BottomCourtyard;

        let diffs = McpServer::compare_regions(&[region_a], &[region_b], 0.001);
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[0]["status"], "only_in_a");
        assert_eq!(diffs[1]["status"], "only_in_b");
    }

    #[test]
    fn moved_text_reported_with_position_change() {
        let text_a = make_text("REF", 0.0, 0.0);
        let text_b = make_text("REF", 3.0, 0.0);

        let diffs = McpServer::compare_pcb_text(&[text_a], &[text_b], 0.001);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0]["status"], "modified");
        assert_eq!(diffs[0]["changes"][0]["property"], "position");
    }

    #[test]
    fn text_height_change_reported() {
        let text_a = make_text("REF", 0.0, 0.0);
        let mut text_b = make_text("REF", 0.0, 0.0);
        text_b.height = 2.0;

        let diffs = McpServer::compare_pcb_text(&[text_a], &[text_b], 0.001);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0]["status"], "modified");
        assert_eq!(diffs[0]["changes"][0]["property"], "height");
    }

    #[test]
    fn via_hole_change_reported() {
        let via_a = Via::new(0.0, 0.0, 0.6, 0.3);
        let via_b = Via::new(0.0, 0.0, 0.6, 0.4);

        let diffs = McpServer::compare_vias(&[via_a], &[via_b], 0.001);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0]["status"], "modified");
        assert_eq!(diffs[0]["changes"][0]["property"], "hole_size");
    }

    #[test]
    fn fill_swapped_corners_still_match() {
        // The same rectangle described from opposite corners is not a diff.
        let fill_a = Fill::new(-0.5, -0.5, 0.5, 0.5, Layer::TopOverlay);
        let fill_b = Fill::new(0.5, 0.5, -0.5, -0.5, Layer::TopOverlay);
        assert!(McpServer::compare_fills(&[fill_a], &[fill_b], 0.001).is_empty());
    }

    #[test]
    fn fill_layer_change_reported() {
        let fill_a = Fill::new(-0.5, -0.5, 0.5, 0.5, Layer::TopOverlay);
        let mut fill_b = fill_a.clone();
        fill_b.layer = Layer::BottomOverlay;

        let diffs = McpServer::compare_fills(&[fill_a], &[fill_b], 0.001);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0]["status"], "modified");
        assert_eq!(diffs[0]["changes"][0]["property"], "layer");
    }

    #[test]
    fn serialized_family_diff_reports_in_place_edit_per_side() {
        // An edited SchLib shape surfaces as one entry per side; identical
        // shapes match off regardless of order.
        let shapes_a = [Rectangle::new(0, 0, 10, 10), Rectangle::new(0, 0, 20, 20)];
        let mut edited = Rectangle::new(0, 0, 20, 20);
        edited.filled = false;
        let shapes_b = [edited, Rectangle::new(0, 0, 10, 10)];

        let diffs = compare_serialized(&shapes_a, &shapes_b);
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[0]["status"], "only_in_a");
        assert_eq!(diffs[0]["index"], 1);
        assert_eq!(diffs[1]["status"], "only_in_b");
        assert_eq!(diffs[1]["index"], 0);
    }

    #[test]
    fn serialized_family_identical_multisets_are_clean() {
        let shapes_a = [Rectangle::new(0, 0, 10, 10), Rectangle::new(0, 0, 10, 10)];
        let shapes_b = [Rectangle::new(0, 0, 10, 10), Rectangle::new(0, 0, 10, 10)];
        assert!(compare_serialized(&shapes_a, &shapes_b).is_empty());
    }
}
