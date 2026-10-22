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
            _ => ToolCallResult::error(super::unsupported_file_type(filepath_a)),
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
            return ToolCallResult::error(super::component_not_found_in(
                name_a,
                &format!("'{filepath_a}'"),
                &lib_a.names(),
            ));
        };
        let Some(fp_b) = lib_b.get(name_b) else {
            return ToolCallResult::error(super::component_not_found_in(
                name_b,
                &format!("'{filepath_b}'"),
                &lib_b.names(),
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

        // Compare component body counts
        if fp_a.component_bodies.len() != fp_b.component_bodies.len() {
            differences.push(json!({
                "field": "component_body_count",
                "component_a": fp_a.component_bodies.len(),
                "component_b": fp_b.component_bodies.len()
            }));
        }

        // Compare component bodies in detail
        if include_geometry {
            let body_diffs =
                Self::compare_bodies(&fp_a.component_bodies, &fp_b.component_bodies, tolerance);
            if !body_diffs.is_empty() {
                differences.push(json!({
                    "field": "component_bodies",
                    "differences": body_diffs
                }));
            }
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
                "fills_b": fp_b.fills.len(),
                "component_bodies_a": fp_a.component_bodies.len(),
                "component_bodies_b": fp_b.component_bodies.len()
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

                // Compare plating (plated vs non-plated hole is an electrical
                // difference, e.g. an NPTH mounting hole vs a through pad).
                if pad_a.is_plated != pad_b.is_plated {
                    changes.push(json!({
                        "property": "is_plated",
                        "a": pad_a.is_plated,
                        "b": pad_b.is_plated
                    }));
                }

                // Compare hole shape (round vs square vs slot).
                if pad_a.hole_shape != pad_b.hole_shape {
                    changes.push(json!({
                        "property": "hole_shape",
                        "a": pad_a.hole_shape,
                        "b": pad_b.hole_shape
                    }));
                }

                // The identity GUIDs (identity_guid / identity_guid_b) are
                // deliberately NOT compared: they identify the pad instance,
                // not its geometry or electrical behaviour — two otherwise
                // identical pads with different GUIDs are the same pad.

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

    /// Compares two lists of component bodies keyed by model name (the STEP
    /// file a body shows; extruded bodies share the empty name), tolerating
    /// duplicate names like pads tolerate duplicate designators. Identity
    /// (GUIDs, unique ids, checksums) is not a difference; the outline, layer,
    /// heights, rotations, offset, kind, colour and embedding are.
    pub(crate) fn compare_bodies(
        bodies_a: &[crate::altium::pcblib::ComponentBody],
        bodies_b: &[crate::altium::pcblib::ComponentBody],
        tolerance: f64,
    ) -> Vec<Value> {
        use crate::altium::pcblib::ComponentBody;

        let property_changes = |a: &ComponentBody, b: &ComponentBody| {
            let mut changes = Vec::new();
            if a.outline.len() != b.outline.len() {
                changes.push(json!({
                    "property": "vertex_count",
                    "a": a.outline.len(),
                    "b": b.outline.len()
                }));
            } else if let Some((k, (va, vb))) =
                a.outline
                    .iter()
                    .zip(&b.outline)
                    .enumerate()
                    .find(|(_, (va, vb))| {
                        (va.0 - vb.0).abs() > tolerance || (va.1 - vb.1).abs() > tolerance
                    })
            {
                changes.push(json!({
                    "property": "outline",
                    "first_mismatch_index": k,
                    "a": { "x": va.0, "y": va.1 },
                    "b": { "x": vb.0, "y": vb.1 }
                }));
            }
            if a.layer != b.layer {
                changes.push(json!({
                    "property": "layer",
                    "a": a.layer,
                    "b": b.layer
                }));
            }
            for (property, va, vb) in [
                ("overall_height", a.overall_height, b.overall_height),
                ("standoff_height", a.standoff_height, b.standoff_height),
                ("z_offset", a.z_offset, b.z_offset),
                ("rotation_x", a.rotation_x, b.rotation_x),
                ("rotation_y", a.rotation_y, b.rotation_y),
                ("rotation_z", a.rotation_z, b.rotation_z),
                ("body_opacity_3d", a.body_opacity_3d, b.body_opacity_3d),
            ] {
                if (va - vb).abs() > tolerance {
                    changes.push(json!({
                        "property": property,
                        "a": va,
                        "b": vb
                    }));
                }
            }
            if a.kind != b.kind {
                changes.push(json!({
                    "property": "kind",
                    "a": a.kind,
                    "b": b.kind
                }));
            }
            if a.embedded != b.embedded {
                changes.push(json!({
                    "property": "embedded",
                    "a": a.embedded,
                    "b": b.embedded
                }));
            }
            if a.body_color_3d != b.body_color_3d {
                changes.push(json!({
                    "property": "body_color_3d",
                    "a": a.body_color_3d,
                    "b": b.body_color_3d
                }));
            }
            if a.name != b.name {
                changes.push(json!({
                    "property": "name",
                    "a": a.name,
                    "b": b.name
                }));
            }
            changes
        };

        compare_keyed(
            bodies_a,
            bodies_b,
            "model_name",
            |body| body.model_name.as_str(),
            |body| {
                vec![
                    ("layer", json!(body.layer)),
                    ("vertex_count", json!(body.outline.len())),
                    ("overall_height", json!(body.overall_height)),
                ]
            },
            property_changes,
        )
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
            return ToolCallResult::error(super::component_not_found_in(
                name_a,
                &format!("'{filepath_a}'"),
                &lib_a.names(),
            ));
        };
        let Some(sym_b) = lib_b.get(name_b) else {
            return ToolCallResult::error(super::component_not_found_in(
                name_b,
                &format!("'{filepath_b}'"),
                &lib_b.names(),
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
        compare_family!("text_frame_count", "text_frames", text_frames);
        compare_family!("bezier_count", "beziers", beziers);
        compare_family!("ellipse_count", "ellipses", ellipses);
        compare_family!("round_rect_count", "round_rects", round_rects);
        compare_family!("elliptical_arc_count", "elliptical_arcs", elliptical_arcs);
        compare_family!("label_count", "labels", labels);
        compare_family!("ieee_symbol_count", "ieee_symbols", ieee_symbols);

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
        Arc, ComponentBody, Fill, Layer, Pad, PcbFlags, Region, RegionKind, Text,
        TextJustification, TextKind, Track, Via,
    };
    use crate::altium::schlib::{Parameter, Rectangle};

    /// True when a `modified` diff entry carries a change for `property`.
    fn has_change(diffs: &[Value], property: &str) -> bool {
        diffs.iter().any(|d| {
            d["status"] == "modified"
                && d["changes"]
                    .as_array()
                    .is_some_and(|cs| cs.iter().any(|c| c["property"] == property))
        })
    }

    /// Builds a minimal stroke text at the given position.
    fn make_text(content: &str, x: f64, y: f64) -> Text {
        Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
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
            is_comment: false,
            is_designator: false,
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
            guid: None,
            raw_geometry: None,
        }
    }

    #[test]
    fn duplicate_pad_designators_all_occurrences_compared() {
        // Two same-designator pads per side (a legal thermal-pad split); the
        // second occurrence differs. HashMap indexing would collapse the
        // group to its last member and report nothing.
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
    fn pads_differing_only_in_plating_report_modified() {
        // is_plated is an electrical property (PTH vs NPTH); a pair identical
        // in every geometric field but plating must still be flagged.
        let pad_a = Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8);
        let mut pad_b = pad_a.clone();
        pad_b.is_plated = false;

        let diffs = McpServer::compare_pads(&[pad_a], &[pad_b], 0.001);
        assert_eq!(diffs.len(), 1, "the plating change must be detected");
        assert_eq!(diffs[0]["status"], "modified");
        assert_eq!(diffs[0]["changes"][0]["property"], "is_plated");
        assert_eq!(diffs[0]["changes"][0]["a"], true);
        assert_eq!(diffs[0]["changes"][0]["b"], false);
    }

    #[test]
    fn pads_differing_only_in_hole_shape_report_modified() {
        use crate::altium::pcblib::HoleShape;
        let pad_a = Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8);
        let mut pad_b = pad_a.clone();
        pad_b.hole_shape = HoleShape::Slot;

        let diffs = McpServer::compare_pads(&[pad_a], &[pad_b], 0.001);
        assert_eq!(diffs.len(), 1, "the hole-shape change must be detected");
        assert_eq!(diffs[0]["status"], "modified");
        assert_eq!(diffs[0]["changes"][0]["property"], "hole_shape");
    }

    #[test]
    fn pads_differing_only_in_identity_guids_report_identical() {
        // The identity GUIDs are instance identity, not geometry — they are
        // deliberately excluded from the pair-compare.
        let pad_a = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        let mut pad_b = pad_a.clone();
        pad_b.identity_guid = Some("{11111111-2222-3333-4444-555555555555}".to_string());
        pad_b.identity_guid_b = Some("{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}".to_string());

        let diffs = McpServer::compare_pads(&[pad_a], &[pad_b], 0.001);
        assert!(
            diffs.is_empty(),
            "identity GUIDs alone are not a difference"
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

    /// A body's height, outline, layer or placement is a reported change;
    /// two copies that differ only in identity are the same body.
    #[test]
    fn body_changes_reported_and_identity_ignored() {
        let mut body_a = ComponentBody::new("", "part.step");
        body_a.outline = vec![(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (0.0, 1.0)];
        body_a.overall_height = 1.5;
        let mut twin = body_a.clone();
        twin.guid = Some("{11111111-2222-3333-4444-555555555555}".to_string());
        twin.unique_id = Some("ABCDEFGH".to_string());
        twin.model_checksum = 42;
        assert!(McpServer::compare_bodies(&[body_a.clone()], &[twin], 0.001).is_empty());

        let mut body_b = body_a.clone();
        body_b.overall_height = 2.5;
        body_b.outline[2] = (2.0, 1.2);
        body_b.layer = Layer::Mechanical13;
        let diffs = McpServer::compare_bodies(&[body_a.clone()], &[body_b], 0.001);
        assert_eq!(diffs.len(), 1, "{diffs:?}");
        assert_eq!(diffs[0]["status"], "modified");
        assert_eq!(diffs[0]["model_name"], "part.step");
        for property in ["outline", "layer", "overall_height"] {
            assert!(has_change(&diffs, property), "{property}: {diffs:?}");
        }
        assert_eq!(diffs[0]["changes"][0]["first_mismatch_index"], 2);

        // Every other property a body carries is a reported change too, and
        // an outline of another vertex count is reported as such rather than
        // as a drift.
        let mut body_c = body_a.clone();
        body_c.outline.push((1.0, 2.0));
        body_c.kind = 1;
        body_c.embedded = !body_a.embedded;
        body_c.body_color_3d = 0x00_FF00;
        body_c.name = "renamed".to_string();
        body_c.standoff_height = 0.3;
        body_c.rotation_z = 90.0;
        let diffs = McpServer::compare_bodies(&[body_a.clone()], &[body_c], 0.001);
        assert_eq!(diffs.len(), 1, "{diffs:?}");
        for property in [
            "vertex_count",
            "kind",
            "embedded",
            "body_color_3d",
            "name",
            "standoff_height",
            "rotation_z",
        ] {
            assert!(has_change(&diffs, property), "{property}: {diffs:?}");
        }
        assert!(
            !has_change(&diffs, "outline"),
            "a different vertex count is not also an outline drift: {diffs:?}"
        );

        // A different model is a one-sided pair, described by layer and size.
        let other = ComponentBody::new("", "other.step");
        let diffs = McpServer::compare_bodies(&[body_a], &[other], 0.001);
        assert_eq!(diffs.len(), 2, "{diffs:?}");
        assert_eq!(diffs[0]["status"], "only_in_a");
        assert_eq!(diffs[0]["vertex_count"], 4);
        assert_eq!(diffs[1]["status"], "only_in_b");
        assert_eq!(diffs[1]["model_name"], "other.step");
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

    // ---- track detail (compare_tracks) ----

    #[test]
    fn track_width_and_layer_changes_reported() {
        let a = Track::new(0.0, 0.0, 5.0, 0.0, 0.25, Layer::TopOverlay);
        let mut wider = a.clone();
        wider.width = 0.5;
        assert!(has_change(
            &McpServer::compare_tracks(std::slice::from_ref(&a), &[wider], 0.001),
            "width"
        ));

        let mut moved_layer = a.clone();
        moved_layer.layer = Layer::BottomOverlay;
        assert!(has_change(
            &McpServer::compare_tracks(&[a], &[moved_layer], 0.001),
            "layer"
        ));
    }

    #[test]
    fn track_reverse_endpoints_still_match_as_modified() {
        // Same segment described end-for-end, only the width differs.
        let a = Track::new(0.0, 0.0, 5.0, 0.0, 0.25, Layer::TopOverlay);
        let b = Track::new(5.0, 0.0, 0.0, 0.0, 0.5, Layer::TopOverlay);
        let diffs = McpServer::compare_tracks(&[a], &[b], 0.001);
        assert_eq!(diffs.len(), 1);
        assert!(has_change(&diffs, "width"));
    }

    #[test]
    fn track_one_sided_reported_per_side() {
        let a = Track::new(0.0, 0.0, 5.0, 0.0, 0.25, Layer::TopOverlay);
        let only_a = McpServer::compare_tracks(std::slice::from_ref(&a), &[], 0.001);
        assert_eq!(only_a.len(), 1);
        assert_eq!(only_a[0]["status"], "only_in_a");
        let only_b = McpServer::compare_tracks(&[], &[a], 0.001);
        assert_eq!(only_b[0]["status"], "only_in_b");
    }

    // ---- arc detail (compare_pcb_arcs) ----

    #[test]
    fn arc_property_changes_reported() {
        let a = Arc::circle(0.0, 0.0, 1.0, 0.15, Layer::TopOverlay);

        let mut start = a.clone();
        start.start_angle = 90.0;
        assert!(has_change(
            &McpServer::compare_pcb_arcs(std::slice::from_ref(&a), &[start], 0.001),
            "start_angle"
        ));

        let mut end = a.clone();
        end.end_angle = 180.0;
        assert!(has_change(
            &McpServer::compare_pcb_arcs(std::slice::from_ref(&a), &[end], 0.001),
            "end_angle"
        ));

        let mut wide = a.clone();
        wide.width = 0.3;
        assert!(has_change(
            &McpServer::compare_pcb_arcs(std::slice::from_ref(&a), &[wide], 0.001),
            "width"
        ));

        let mut layer = a.clone();
        layer.layer = Layer::BottomOverlay;
        assert!(has_change(
            &McpServer::compare_pcb_arcs(&[a], &[layer], 0.001),
            "layer"
        ));
    }

    #[test]
    fn arc_one_sided_reported_per_side() {
        let a = Arc::circle(0.0, 0.0, 1.0, 0.15, Layer::TopOverlay);
        let only_a = McpServer::compare_pcb_arcs(std::slice::from_ref(&a), &[], 0.001);
        assert_eq!(only_a[0]["status"], "only_in_a");
        let only_b = McpServer::compare_pcb_arcs(&[], &[a], 0.001);
        assert_eq!(only_b[0]["status"], "only_in_b");
    }

    // ---- fill rotation + one-sided (compare_fills) ----

    #[test]
    fn fill_rotation_change_reported() {
        let a = Fill::new(-0.5, -0.5, 0.5, 0.5, Layer::TopOverlay);
        let mut rotated = a.clone();
        rotated.rotation = 90.0;
        assert!(has_change(
            &McpServer::compare_fills(&[a], &[rotated], 0.001),
            "rotation"
        ));
    }

    #[test]
    fn fill_one_sided_reported_per_side() {
        let a = Fill::new(-0.5, -0.5, 0.5, 0.5, Layer::TopOverlay);
        let b = Fill::new(5.0, 5.0, 6.0, 6.0, Layer::TopOverlay);
        let diffs = McpServer::compare_fills(&[a], &[b], 0.001);
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[0]["status"], "only_in_a");
        assert_eq!(diffs[1]["status"], "only_in_b");
    }

    // ---- text detail (compare_pcb_text) ----

    #[test]
    fn text_property_changes_reported() {
        let a = make_text("REF", 0.0, 0.0);

        let mut rotated = make_text("REF", 0.0, 0.0);
        rotated.rotation = 90.0;
        assert!(has_change(
            &McpServer::compare_pcb_text(std::slice::from_ref(&a), &[rotated], 0.001),
            "rotation"
        ));

        let mut layer = make_text("REF", 0.0, 0.0);
        layer.layer = Layer::BottomOverlay;
        assert!(has_change(
            &McpServer::compare_pcb_text(std::slice::from_ref(&a), &[layer], 0.001),
            "layer"
        ));

        let mut kind = make_text("REF", 0.0, 0.0);
        kind.kind = TextKind::TrueType;
        assert!(has_change(
            &McpServer::compare_pcb_text(std::slice::from_ref(&a), &[kind], 0.001),
            "kind"
        ));

        let mut mirror = make_text("REF", 0.0, 0.0);
        mirror.mirror = true;
        assert!(has_change(
            &McpServer::compare_pcb_text(&[a], &[mirror], 0.001),
            "mirror"
        ));
    }

    #[test]
    fn text_differing_content_reported_per_side() {
        let a = make_text("AAA", 0.0, 0.0);
        let b = make_text("BBB", 5.0, 0.0);
        let diffs = McpServer::compare_pcb_text(&[a], &[b], 0.001);
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[0]["status"], "only_in_a");
        assert_eq!(diffs[1]["status"], "only_in_b");
    }

    // ---- pin detail (compare_pins) ----

    mod pin_detail {
        use super::*;
        use crate::altium::schlib::{Pin, PinElectricalType, PinOrientation};

        #[test]
        fn pin_property_changes_reported() {
            let a = Pin::new("1", "1", -20, 0, 10, PinOrientation::Left);

            let mut moved = a.clone();
            moved.x = -10;
            assert!(has_change(
                &McpServer::compare_pins(std::slice::from_ref(&a), &[moved]),
                "position"
            ));

            let mut longer = a.clone();
            longer.length = 20;
            assert!(has_change(
                &McpServer::compare_pins(std::slice::from_ref(&a), &[longer]),
                "length"
            ));

            let mut renamed = a.clone();
            renamed.name = "PIN1".to_string();
            assert!(has_change(
                &McpServer::compare_pins(std::slice::from_ref(&a), &[renamed]),
                "name"
            ));

            let mut etype = a.clone();
            etype.electrical_type = PinElectricalType::Input;
            assert!(has_change(
                &McpServer::compare_pins(std::slice::from_ref(&a), &[etype]),
                "electrical_type"
            ));

            let mut oriented = a.clone();
            oriented.orientation = PinOrientation::Right;
            assert!(has_change(
                &McpServer::compare_pins(&[a], &[oriented]),
                "orientation"
            ));
        }
    }

    // ==================== call_compare_components dispatcher ====================

    mod dispatcher {
        use super::*;
        use crate::altium::schlib::{Pin, PinOrientation, SchLib, Symbol};
        use crate::mcp::tools::test_support::{
            create_test_pcblib, create_test_schlib, create_test_server, get_result_text,
            parse_result_json, test_temp_dir,
        };

        #[test]
        fn compare_components_missing_parameters() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let result = server.call_compare_components(&json!({}));
            assert!(result.is_error);
            assert_eq!(
                get_result_text(&result),
                "Missing required parameter: filepath_a"
            );

            let result = server.call_compare_components(&json!({ "filepath_a": "a.PcbLib" }));
            assert!(result.is_error);
            assert_eq!(
                get_result_text(&result),
                "Missing required parameter: component_a"
            );
        }

        #[test]
        fn compare_components_rejects_mismatched_extensions() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let pcb = dir.path().join("A.PcbLib");
            let sch = dir.path().join("B.SchLib");
            create_test_pcblib(&pcb);
            create_test_schlib(&sch);

            let result = server.call_compare_components(&json!({
                "filepath_a": pcb.to_string_lossy(),
                "component_a": "CHIP_0402",
                "filepath_b": sch.to_string_lossy(),
                "component_b": "RESISTOR",
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("File types must match"));
        }

        #[test]
        fn compare_components_rejects_path_outside_allowed() {
            let dir = test_temp_dir();
            let other = test_temp_dir();
            let server = create_test_server(dir.path());
            let outside = other.path().join("Out.PcbLib");
            create_test_pcblib(&outside);
            let inside = dir.path().join("In.PcbLib");
            create_test_pcblib(&inside);

            let result = server.call_compare_components(&json!({
                "filepath_a": outside.to_string_lossy(),
                "component_a": "CHIP_0402",
                "filepath_b": inside.to_string_lossy(),
                "component_b": "CHIP_0402",
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("Access denied"));
        }

        #[test]
        fn compare_footprints_identical_components() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Same.PcbLib");
            create_test_pcblib(&path);

            let result = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(),
                "component_a": "CHIP_0402",
                "filepath_b": path.to_string_lossy(),
                "component_b": "CHIP_0402",
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            assert_eq!(parsed["status"], "success");
            assert_eq!(parsed["file_type"], "PcbLib");
            assert_eq!(parsed["identical"], true);
            assert_eq!(parsed["difference_count"], 0);
            assert_eq!(parsed["summary"]["pads_a"], 2);
            assert_eq!(parsed["summary"]["pads_b"], 2);
        }

        #[test]
        fn compare_footprints_reports_differences() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Diff.PcbLib");
            create_test_pcblib(&path);

            // CHIP_0402 vs CHIP_0603: same pad count, different geometry and
            // descriptions.
            let result = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(),
                "component_a": "CHIP_0402",
                "filepath_b": path.to_string_lossy(),
                "component_b": "CHIP_0603",
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            assert_eq!(parsed["identical"], false);
            assert!(parsed["difference_count"].as_u64().unwrap() >= 2);
            let differences = parsed["differences"].as_array().unwrap();
            assert!(differences.iter().any(|d| d["field"] == "description"));
            assert!(differences.iter().any(|d| d["field"] == "pads"));
        }

        #[test]
        fn compare_footprints_component_not_found() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("NotFound.PcbLib");
            create_test_pcblib(&path);

            let result = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(),
                "component_a": "GHOST",
                "filepath_b": path.to_string_lossy(),
                "component_b": "CHIP_0402",
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("'GHOST' not found"));
        }

        #[test]
        fn compare_symbols_identical_components() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("SSame.SchLib");
            create_test_schlib(&path);

            let result = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(),
                "component_a": "RESISTOR",
                "filepath_b": path.to_string_lossy(),
                "component_b": "RESISTOR",
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            assert_eq!(parsed["status"], "success");
            assert_eq!(parsed["file_type"], "SchLib");
            assert_eq!(parsed["identical"], true);
            assert_eq!(parsed["difference_count"], 0);
            assert_eq!(parsed["summary"]["pins_a"], 2);
            assert_eq!(parsed["summary"]["pins_b"], 2);
        }

        #[test]
        fn compare_symbols_reports_pin_and_shape_differences() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path_a = dir.path().join("SDiffA.SchLib");
            create_test_schlib(&path_a);

            // A modified RESISTOR: moved pin 2, no rectangle, new description.
            let mut lib_b = SchLib::new();
            let mut sym = Symbol::new("RESISTOR");
            sym.description = "Precision resistor".to_string();
            sym.designator = "R?".to_string();
            sym.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
            sym.add_pin(Pin::new("2", "2", 30, 0, 10, PinOrientation::Right));
            lib_b.add(sym);
            let path_b = dir.path().join("SDiffB.SchLib");
            lib_b.save(&path_b).unwrap();

            let result = server.call_compare_components(&json!({
                "filepath_a": path_a.to_string_lossy(),
                "component_a": "RESISTOR",
                "filepath_b": path_b.to_string_lossy(),
                "component_b": "RESISTOR",
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            assert_eq!(parsed["identical"], false);
            let differences = parsed["differences"].as_array().unwrap();
            assert!(differences.iter().any(|d| d["field"] == "description"));
            assert!(differences.iter().any(|d| d["field"] == "pins"));
            assert!(differences.iter().any(|d| d["field"] == "rectangle_count"));
        }

        #[test]
        fn compare_symbols_reports_scalar_footprint_and_parameter_diffs() {
            use crate::altium::schlib::{FootprintModel, Parameter};

            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let mut lib_a = SchLib::new();
            let mut sym_a = Symbol::new("SYM");
            sym_a.designator = "R?".to_string();
            sym_a.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
            sym_a.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
            sym_a.add_parameter(Parameter::new("Value", "1k"));
            lib_a.add(sym_a);
            let path_a = dir.path().join("ScalarA.SchLib");
            lib_a.save(&path_a).unwrap();

            let mut lib_b = SchLib::new();
            let mut sym_b = Symbol::new("SYM");
            sym_b.designator = "U?".to_string(); // designator diff
            sym_b.part_count = 2; // part_count diff (default is 1)
            sym_b.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
            sym_b.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
            sym_b.add_pin(Pin::new("3", "3", 20, 20, 10, PinOrientation::Right)); // pin_count diff
            sym_b.add_parameter(Parameter::new("Value", "2k")); // parameter value diff
            sym_b.add_footprint(FootprintModel::new("0603")); // footprint_count + names diff
            lib_b.add(sym_b);
            let path_b = dir.path().join("ScalarB.SchLib");
            lib_b.save(&path_b).unwrap();

            let result = server.call_compare_components(&json!({
                "filepath_a": path_a.to_string_lossy(),
                "component_a": "SYM",
                "filepath_b": path_b.to_string_lossy(),
                "component_b": "SYM",
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            assert_eq!(parsed["identical"], false);
            let fields: Vec<&str> = parsed["differences"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| d["field"].as_str().unwrap_or(""))
                .collect();
            for expected in [
                "designator",
                "part_count",
                "pin_count",
                "footprint_count",
                "footprints",
                "parameters",
            ] {
                assert!(
                    fields.contains(&expected),
                    "missing field {expected}: {fields:?}"
                );
            }

            // The footprints entry names each side's unique footprint.
            let fp_diff = parsed["differences"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["field"] == "footprints")
                .unwrap();
            assert!(fp_diff["only_in_b"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "0603"));

            // The parameter diff reports the changed value.
            let param_diff = parsed["differences"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["field"] == "parameters")
                .unwrap();
            let changes = &param_diff["differences"][0]["changes"];
            assert_eq!(changes[0]["property"], "value");
        }
    }

    // ==================== rejection paths and the remaining diff arms ========
    //
    // `compare_components` answers a malformed or unreadable request with an
    // error result, and each primitive family has a difference arm of its own.
    // The fixtures elsewhere in this file are pad-only and mostly identical, so
    // the arms below need components built to differ on purpose.

    mod every_arm {
        use super::{make_text, McpServer};
        use crate::altium::pcblib::{
            Arc, ComponentBody, Fill, Footprint, Layer, Model3D, Pad, PcbLib, Region, Track,
            Vertex, Via,
        };
        use crate::altium::schlib::{Parameter, Pin, PinOrientation, SchLib, Symbol};
        use crate::mcp::tools::test_support::{
            create_test_server, get_result_text, parse_result_json, test_temp_dir,
        };
        use serde_json::json;
        use std::slice::from_ref;

        /// A footprint carrying exactly one of every primitive family, so a
        /// comparison against a deliberately richer twin walks every count arm
        /// and every detail arm in a single pass.
        fn one_of_each(name: &str) -> Footprint {
            let mut fp = Footprint::new(name);
            fp.description = "base".to_string();
            fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
            fp.add_via(Via::new(0.0, 0.0, 0.6, 0.3));
            fp.add_track(Track::new(-1.0, 0.0, 1.0, 0.0, 0.2, Layer::TopOverlay));
            fp.add_arc(Arc::circle(0.0, 0.0, 1.0, 0.2, Layer::TopOverlay));
            fp.add_region(Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopCourtyard));
            fp.add_text(make_text("REF", 0.0, 0.0));
            fp.add_fill(Fill::new(-0.5, -0.5, 0.5, 0.5, Layer::TopOverlay));
            fp
        }

        /// A placeholder extruded body, so the component-body count can differ.
        fn body() -> ComponentBody {
            ComponentBody {
                raw_layer_id: None,
                v7_layer: None,
                model_id: String::new(),
                identifier: String::new(),
                texture_center_x: None,
                texture_center_y: None,
                texture_size_x: None,
                texture_size_y: None,
                texture_rotation: None,
                model_name: String::new(),
                embedded: false,
                rotation_x: 0.0,
                rotation_y: 0.0,
                rotation_z: 0.0,
                z_offset: 0.0,
                overall_height: 1.0,
                standoff_height: 0.0,
                cavity_height: 0.0,
                layer: Layer::Top3DBody,
                outline: Vec::new(),
                unique_id: None,
                guid: None,
                model_checksum: 0,
                name: " ".to_string(),
                kind: 0,
                sub_poly_index: -1,
                union_index: 0,
                is_shape_based: false,
                body_projection: 0,
                body_color_3d: 8_421_504,
                body_opacity_3d: 1.0,
                model_2d_rotation: 0.0,
                model_2d_x: 0.0,
                model_2d_y: 0.0,
                net_index: 0xFFFF,
                polygon_index: 0xFFFF,
                component_index: -1,
                additional_parameters: Vec::new(),
                flags: crate::altium::pcblib::PcbFlags::empty(),
                model_type: None,
                param_key_order: Vec::new(),
            }
        }

        /// Writes bytes that are not an OLE compound file, so `open` fails.
        fn write_garbage(path: &std::path::Path) {
            std::fs::write(path, b"not an OLE compound document").unwrap();
        }

        /// A one-pin symbol, the schematic counterpart of `one_of_each`.
        fn simple_symbol(name: &str) -> Symbol {
            let mut sym = Symbol::new(name);
            sym.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
            sym
        }

        // ---- request validation ----------------------------------------------

        #[test]
        fn compare_components_requires_the_b_side_identifiers_too() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Lib.PcbLib");

            let no_filepath_b = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(), "component_a": "A",
            }));
            assert!(no_filepath_b.is_error);
            assert!(get_result_text(&no_filepath_b).contains("filepath_b"));

            let no_component_b = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(), "component_a": "A",
                "filepath_b": path.to_string_lossy(),
            }));
            assert!(no_component_b.is_error);
            assert!(get_result_text(&no_component_b).contains("component_b"));
        }

        #[test]
        fn compare_components_validates_the_second_path_as_well() {
            // Both sides are read, so gating only filepath_a would leave a
            // readable escape through filepath_b.
            let allowed = test_temp_dir();
            let outside = test_temp_dir();
            let server = create_test_server(allowed.path());
            let r = server.call_compare_components(&json!({
                "filepath_a": allowed.path().join("A.PcbLib").to_string_lossy(),
                "component_a": "A",
                "filepath_b": outside.path().join("B.PcbLib").to_string_lossy(),
                "component_b": "B",
            }));
            assert!(r.is_error, "{}", get_result_text(&r));
        }

        #[test]
        fn compare_components_rejects_an_unrecognised_extension() {
            // Matching extensions get past the same-type check, so an unknown
            // pair falls through to the file-type arm rather than the mismatch.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Notes.txt");
            std::fs::write(&path, b"x").unwrap();
            let r = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(), "component_a": "A",
                "filepath_b": path.to_string_lossy(), "component_b": "B",
            }));
            assert!(r.is_error);
            assert!(get_result_text(&r).contains("Unsupported file type"));
        }

        // ---- footprint comparison --------------------------------------------

        #[test]
        fn compare_footprints_reports_an_unreadable_library_on_either_side() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let good = dir.path().join("Good.PcbLib");
            let bad = dir.path().join("Bad.PcbLib");
            let mut lib = PcbLib::new();
            lib.add(one_of_each("FP"));
            lib.save(&good).unwrap();
            write_garbage(&bad);

            for (a, b) in [(&bad, &good), (&good, &bad)] {
                let r = server.call_compare_components(&json!({
                    "filepath_a": a.to_string_lossy(), "component_a": "FP",
                    "filepath_b": b.to_string_lossy(), "component_b": "FP",
                }));
                assert!(r.is_error, "{}", get_result_text(&r));
                assert!(get_result_text(&r).contains("Failed to read"));
            }
        }

        #[test]
        fn compare_footprints_reports_a_missing_component_on_the_b_side() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Lib.PcbLib");
            let mut lib = PcbLib::new();
            lib.add(one_of_each("FP"));
            lib.save(&path).unwrap();

            let r = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(), "component_a": "FP",
                "filepath_b": path.to_string_lossy(), "component_b": "GHOST",
            }));
            assert!(r.is_error);
            assert!(get_result_text(&r).contains("'GHOST' not found"));
        }

        #[test]
        fn compare_footprints_reports_a_difference_in_every_family() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Families.PcbLib");

            let base = one_of_each("BASE");

            // A richer twin: one extra of every family, placed where it cannot
            // pair off with the original, so both the count arm and the detail
            // arm fire for each.
            let mut rich = one_of_each("RICH");
            rich.description = "changed".to_string();
            rich.add_pad(Pad::smd("2", 5.0, 5.0, 1.0, 1.0));
            rich.add_via(Via::new(5.0, 5.0, 0.6, 0.3));
            rich.add_track(Track::new(-9.0, 9.0, 9.0, 9.0, 0.2, Layer::TopOverlay));
            rich.add_arc(Arc::circle(9.0, 9.0, 1.0, 0.2, Layer::TopOverlay));
            rich.add_region(Region::rectangle(8.0, 8.0, 9.0, 9.0, Layer::TopCourtyard));
            rich.add_text(make_text("EXTRA", 9.0, 9.0));
            rich.add_fill(Fill::new(8.0, 8.0, 9.0, 9.0, Layer::TopOverlay));
            rich.add_component_body(body());

            let mut lib = PcbLib::new();
            lib.add(base);
            lib.add(rich);
            lib.save(&path).unwrap();

            let r = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(), "component_a": "BASE",
                "filepath_b": path.to_string_lossy(), "component_b": "RICH",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let parsed = parse_result_json(&r);
            assert_eq!(parsed["identical"], false);

            let fields: Vec<&str> = parsed["differences"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|d| d["field"].as_str())
                .collect();
            for expected in [
                "description",
                "pad_count",
                "pads",
                "via_count",
                "vias",
                "track_count",
                "tracks",
                "arc_count",
                "arcs",
                "region_count",
                "regions",
                "text_count",
                "text",
                "fill_count",
                "fills",
                "component_body_count",
                "component_bodies",
            ] {
                assert!(
                    fields.contains(&expected),
                    "missing {expected:?} from the reported fields: {fields:?}"
                );
            }
            assert_eq!(parsed["summary"]["component_bodies_a"], 0);
            assert_eq!(parsed["summary"]["component_bodies_b"], 1);
        }

        /// Two footprints whose only difference is a body's height are not
        /// identical.
        #[test]
        fn compare_footprints_sees_a_body_change() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Bodies.PcbLib");

            let mut base = one_of_each("BASE");
            base.add_component_body(body());
            let mut taller = one_of_each("TALLER");
            let mut tall_body = body();
            tall_body.overall_height = 2.0;
            taller.add_component_body(tall_body);

            let mut lib = PcbLib::new();
            lib.add(base);
            lib.add(taller);
            lib.save(&path).unwrap();

            let r = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(), "component_a": "BASE",
                "filepath_b": path.to_string_lossy(), "component_b": "TALLER",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let parsed = parse_result_json(&r);
            assert_eq!(parsed["identical"], false, "{parsed}");
            let differences = parsed["differences"].as_array().unwrap();
            let bodies = differences
                .iter()
                .find(|d| d["field"] == "component_bodies")
                .expect("a body diff");
            assert_eq!(bodies["differences"][0]["status"], "modified");
            assert_eq!(
                bodies["differences"][0]["changes"][0]["property"],
                "overall_height"
            );
            assert_eq!(differences.len(), 1, "{differences:?}");
        }

        #[test]
        fn a_primitive_only_the_a_side_has_is_reported_from_its_own_side() {
            // Every family diff is directional. Comparing the richer component
            // as A exercises the only_in_a arms, which a B-side-only fixture
            // leaves untouched — and an unreported deletion is the worst kind.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("OnlyInA.PcbLib");

            let base = one_of_each("BASE");
            let mut rich = one_of_each("RICH");
            rich.add_track(Track::new(-9.0, 9.0, 9.0, 9.0, 0.2, Layer::TopOverlay));
            rich.add_arc(Arc::circle(9.0, 9.0, 1.0, 0.2, Layer::TopOverlay));

            let mut lib = PcbLib::new();
            lib.add(base);
            lib.add(rich);
            lib.save(&path).unwrap();

            // RICH first, so its unmatched track and arc are on the A side.
            let r = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(), "component_a": "RICH",
                "filepath_b": path.to_string_lossy(), "component_b": "BASE",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let parsed = parse_result_json(&r);

            for family in ["tracks", "arcs"] {
                let entry = parsed["differences"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|d| d["field"] == family)
                    .expect("a diff for this family");
                let only_in_a = entry["differences"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|d| d["status"] == "only_in_a");
                assert!(only_in_a, "{family}: {entry}");
            }
        }

        #[test]
        fn include_geometry_false_suppresses_symbol_detail_too() {
            // The SchLib side has its own pin, shape and parameter detail
            // blocks behind the same flag; each has to fall back to the count
            // alone rather than serialising the primitives.
            use crate::altium::schlib::{Parameter, Rectangle};

            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let mut lib_a = SchLib::new();
            let mut sym_a = Symbol::new("SYM");
            sym_a.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
            sym_a.add_parameter(Parameter::new("Value", "1k"));
            lib_a.add(sym_a);
            let path_a = dir.path().join("NoGeomA.SchLib");
            lib_a.save(&path_a).unwrap();

            let mut lib_b = SchLib::new();
            let mut sym_b = Symbol::new("SYM");
            sym_b.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
            sym_b.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
            sym_b.add_parameter(Parameter::new("Value", "2k"));
            sym_b.add_rectangle(Rectangle::new(-10, -10, 10, 10));
            lib_b.add(sym_b);
            let path_b = dir.path().join("NoGeomB.SchLib");
            lib_b.save(&path_b).unwrap();

            let r = server.call_compare_components(&json!({
                "filepath_a": path_a.to_string_lossy(), "component_a": "SYM",
                "filepath_b": path_b.to_string_lossy(), "component_b": "SYM",
                "include_geometry": false,
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let parsed = parse_result_json(&r);
            let fields: Vec<&str> = parsed["differences"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|d| d["field"].as_str())
                .collect();

            assert!(fields.contains(&"pin_count"), "{fields:?}");
            for suppressed in ["pins", "parameters", "rectangles"] {
                assert!(
                    !fields.contains(&suppressed),
                    "{suppressed:?} leaked with include_geometry=false: {fields:?}"
                );
            }
        }

        #[test]
        fn include_geometry_false_keeps_the_counts_and_drops_the_detail() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("NoGeom.PcbLib");

            let base = one_of_each("BASE");
            let mut rich = one_of_each("RICH");
            rich.add_pad(Pad::smd("2", 5.0, 5.0, 1.0, 1.0));
            rich.add_via(Via::new(5.0, 5.0, 0.6, 0.3));
            rich.add_track(Track::new(-9.0, 9.0, 9.0, 9.0, 0.2, Layer::TopOverlay));
            rich.add_arc(Arc::circle(9.0, 9.0, 1.0, 0.2, Layer::TopOverlay));
            rich.add_region(Region::rectangle(8.0, 8.0, 9.0, 9.0, Layer::TopCourtyard));
            rich.add_text(make_text("EXTRA", 9.0, 9.0));
            rich.add_fill(Fill::new(8.0, 8.0, 9.0, 9.0, Layer::TopOverlay));

            let mut lib = PcbLib::new();
            lib.add(base);
            lib.add(rich);
            lib.save(&path).unwrap();

            let r = server.call_compare_components(&json!({
                "filepath_a": path.to_string_lossy(), "component_a": "BASE",
                "filepath_b": path.to_string_lossy(), "component_b": "RICH",
                "include_geometry": false,
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let parsed = parse_result_json(&r);

            let fields: Vec<&str> = parsed["differences"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|d| d["field"].as_str())
                .collect();

            // The per-family counts still differ and must be reported...
            for expected in [
                "pad_count",
                "via_count",
                "track_count",
                "arc_count",
                "region_count",
                "text_count",
                "fill_count",
            ] {
                assert!(
                    fields.contains(&expected),
                    "missing {expected:?}: {fields:?}"
                );
            }
            // ...but the primitive-by-primitive detail is suppressed.
            for suppressed in ["pads", "vias", "tracks", "arcs", "regions", "text", "fills"] {
                assert!(
                    !fields.contains(&suppressed),
                    "{suppressed:?} leaked with include_geometry=false: {fields:?}"
                );
            }
        }

        #[test]
        fn compare_footprints_reports_3d_model_presence_then_path() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Models.PcbLib");

            // The save path reads the model off disk, so each reference has to
            // name a file that actually exists.
            let model = |name: &str| {
                let file = dir.path().join(name);
                std::fs::write(
                    &file,
                    b"ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;\n",
                )
                .unwrap();
                Model3D {
                    filepath: file.to_string_lossy().into_owned(),
                    x_offset: 0.0,
                    y_offset: 0.0,
                    z_offset: 0.0,
                    rotation: 0.0,
                }
            };

            let plain = one_of_each("PLAIN");
            let mut with_model = one_of_each("WITH_MODEL");
            with_model.model_3d = Some(model("a.step"));
            let mut other_model = one_of_each("OTHER_MODEL");
            other_model.model_3d = Some(model("b.step"));

            let mut lib = PcbLib::new();
            lib.add(plain);
            lib.add(with_model);
            lib.add(other_model);
            lib.save(&path).unwrap();

            let compare = |a: &str, b: &str| {
                let r = server.call_compare_components(&json!({
                    "filepath_a": path.to_string_lossy(), "component_a": a,
                    "filepath_b": path.to_string_lossy(), "component_b": b,
                }));
                assert!(!r.is_error, "{}", get_result_text(&r));
                parse_result_json(&r)
            };

            // One side has a model and the other does not: a presence diff.
            let presence = compare("PLAIN", "WITH_MODEL");
            assert!(presence["differences"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["field"] == "external_3d_model"));

            // Both have one, pointing at different files: a path diff instead.
            let paths = compare("WITH_MODEL", "OTHER_MODEL");
            assert!(paths["differences"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["field"] == "3d_model_path"));
        }

        // ---- primitive-level arms not reached by the fixtures above ----------

        #[test]
        fn compare_pads_reports_shape_layer_and_rotation_changes() {
            use super::has_change;
            use crate::altium::pcblib::PadShape;

            let base = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);

            let mut shape = base.clone();
            shape.shape = PadShape::Round;
            let mut other = base.clone();
            other.shape = PadShape::Rectangle;
            assert!(has_change(
                &McpServer::compare_pads(&[shape], &[other], 0.001),
                "shape"
            ));

            let mut layer = base.clone();
            layer.layer = Layer::BottomLayer;
            assert!(has_change(
                &McpServer::compare_pads(from_ref(&base), &[layer], 0.001),
                "layer"
            ));

            let mut rotated = base.clone();
            rotated.rotation = 90.0;
            assert!(has_change(
                &McpServer::compare_pads(from_ref(&base), &[rotated], 0.001),
                "rotation"
            ));

            // One side drilled, the other not: a hole appearing is a change
            // even though neither value can be compared numerically.
            let mut drilled = base.clone();
            drilled.hole_size = Some(0.5);
            assert!(has_change(
                &McpServer::compare_pads(&[base], &[drilled], 0.001),
                "hole_size"
            ));
        }

        #[test]
        fn compare_vias_reports_diameter_span_and_one_sided_entries() {
            use super::has_change;

            let base = Via::new(0.0, 0.0, 0.6, 0.3);

            let wider = Via::new(0.0, 0.0, 0.9, 0.3);
            assert!(has_change(
                &McpServer::compare_vias(from_ref(&base), &[wider], 0.001),
                "diameter"
            ));

            let mut spanned = base.clone();
            spanned.to_layer = Layer::Mechanical1;
            assert!(has_change(
                &McpServer::compare_vias(from_ref(&base), &[spanned], 0.001),
                "layer_span"
            ));

            // A via with no counterpart within tolerance is reported whole, on
            // whichever side it is missing from.
            let moved = Via::new(9.0, 9.0, 0.6, 0.3);
            let diffs = McpServer::compare_vias(from_ref(&base), from_ref(&moved), 0.001);
            assert_eq!(diffs.len(), 2, "{diffs:?}");
            assert_eq!(diffs[0]["status"], "only_in_a");
            assert_eq!(diffs[1]["status"], "only_in_b");
            assert_eq!(diffs[1]["position"]["x"], 9.0);
        }

        #[test]
        fn compare_regions_reports_name_and_hole_count_changes() {
            use super::has_change;

            let base = Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopCourtyard);

            let mut renamed = base.clone();
            renamed.name = "COURTYARD".to_string();
            assert!(has_change(
                &McpServer::compare_regions(from_ref(&base), &[renamed], 0.001),
                "name"
            ));

            // The outline still matches, so a region that gained an inner
            // contour is a modification rather than an add/remove pair.
            let mut holed = base.clone();
            holed.holes.push(vec![
                Vertex { x: -0.5, y: -0.5 },
                Vertex { x: 0.5, y: -0.5 },
                Vertex { x: 0.5, y: 0.5 },
                Vertex { x: -0.5, y: 0.5 },
            ]);
            assert!(has_change(
                &McpServer::compare_regions(&[base], &[holed], 0.001),
                "hole_count"
            ));
        }

        #[test]
        fn matched_counterparts_are_not_claimed_twice() {
            // Two identical primitives per side: the first pairing consumes
            // slot 0, so the second must skip it and take slot 1 instead of
            // reporting a false one-sided difference.
            let track = |width: f64| Track::new(-1.0, 0.0, 1.0, 0.0, width, Layer::TopOverlay);
            let tracks_a = [track(0.2), track(0.2)];
            let tracks_b = [track(0.2), track(0.2)];
            assert!(McpServer::compare_tracks(&tracks_a, &tracks_b, 0.001).is_empty());

            let arc = || Arc::circle(0.0, 0.0, 1.0, 0.2, Layer::TopOverlay);
            let arcs_a = [arc(), arc()];
            let arcs_b = [arc(), arc()];
            assert!(McpServer::compare_pcb_arcs(&arcs_a, &arcs_b, 0.001).is_empty());

            let via = || Via::new(0.0, 0.0, 0.6, 0.3);
            assert!(McpServer::compare_vias(&[via(), via()], &[via(), via()], 0.001).is_empty());
        }

        #[test]
        fn a_one_sided_parameter_occurrence_carries_its_value() {
            // The keyed comparison describes a leftover occurrence rather than
            // reporting a bare name, so the reader can see what was dropped.
            let params_a = [Parameter::new("Value", "1k"), Parameter::new("Value", "2k")];
            let params_b = [Parameter::new("Value", "1k")];

            let diffs = McpServer::compare_parameters(&params_a, &params_b);
            assert_eq!(diffs.len(), 1, "{diffs:?}");
            assert_eq!(diffs[0]["status"], "only_in_a");
            assert_eq!(diffs[0]["occurrence"], 1);
            assert_eq!(diffs[0]["value"], "2k");
        }

        // ---- symbol comparison -----------------------------------------------

        #[test]
        fn compare_symbols_reports_unreadable_libraries_and_missing_components() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let good = dir.path().join("Good.SchLib");
            let bad = dir.path().join("Bad.SchLib");
            let mut lib = SchLib::new();
            lib.add(simple_symbol("SYM"));
            lib.save(&good).unwrap();
            write_garbage(&bad);

            for (a, b) in [(&bad, &good), (&good, &bad)] {
                let r = server.call_compare_components(&json!({
                    "filepath_a": a.to_string_lossy(), "component_a": "SYM",
                    "filepath_b": b.to_string_lossy(), "component_b": "SYM",
                }));
                assert!(r.is_error, "{}", get_result_text(&r));
                assert!(get_result_text(&r).contains("Failed to read"));
            }

            for (a, b) in [("GHOST", "SYM"), ("SYM", "GHOST")] {
                let r = server.call_compare_components(&json!({
                    "filepath_a": good.to_string_lossy(), "component_a": a,
                    "filepath_b": good.to_string_lossy(), "component_b": b,
                }));
                assert!(r.is_error);
                assert!(get_result_text(&r).contains("'GHOST' not found"));
            }
        }
    }
}
