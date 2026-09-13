//! Read/write/list/style tools. Split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{ErrorContext, McpServer, ToolCallResult};
use crate::mcp::tools::allowed_keys;

/// Computes a pin's connection-tip coordinate from its body-attach end `(x,y)`,
/// `length`, and `orientation`, mirroring how the pin is drawn: the tip is
/// `length` units from `(x,y)` in the `orientation` direction.
const fn pin_tip(pin: &crate::altium::schlib::Pin) -> (i32, i32) {
    use crate::altium::schlib::PinOrientation::{Down, Left, Right, Up};
    match pin.orientation {
        Right => (pin.x + pin.length, pin.y),
        Left => (pin.x - pin.length, pin.y),
        Up => (pin.x, pin.y + pin.length),
        Down => (pin.x, pin.y - pin.length),
    }
}

/// Builds a geometry summary for a written symbol so the caller can self-check
/// pin placement (catching flipped or misaligned pins without opening Altium).
/// For each pin it reports the body-attach end, the computed connection tip, and
/// the orientation; plus the symbol's bounding box. All values are in schematic
/// units (10 = 1 grid square).
#[allow(clippy::cast_possible_truncation)] // rectangle coords rounded onto the integer bbox grid
fn symbol_geometry(symbol: &crate::altium::schlib::Symbol) -> Value {
    let mut xs: Vec<i32> = Vec::new();
    let mut ys: Vec<i32> = Vec::new();
    let pins: Vec<Value> = symbol
        .pins
        .iter()
        .map(|p| {
            let (tx, ty) = pin_tip(p);
            xs.push(p.x);
            xs.push(tx);
            ys.push(p.y);
            ys.push(ty);
            json!({
                "designator": p.designator,
                "name": p.name,
                "orientation": p.orientation,
                "body_end": { "x": p.x, "y": p.y },
                "tip": { "x": tx, "y": ty },
            })
        })
        .collect();
    for r in &symbol.rectangles {
        xs.push(r.x1.round() as i32);
        xs.push(r.x2.round() as i32);
        ys.push(r.y1.round() as i32);
        ys.push(r.y2.round() as i32);
    }
    let bounding_box = if xs.is_empty() {
        Value::Null
    } else {
        json!({
            "min_x": xs.iter().min(),
            "max_x": xs.iter().max(),
            "min_y": ys.iter().min(),
            "max_y": ys.iter().max(),
        })
    };
    json!({ "name": symbol.name, "pins": pins, "bounding_box": bounding_box })
}

/// True if the segment `(x1,y1)-(x2,y2)` intersects the axis-aligned rectangle
/// `[xmin,xmax] x [ymin,ymax]` (Liang-Barsky clip; an endpoint inside counts).
#[allow(clippy::too_many_arguments)]
fn segment_intersects_rect(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    xmin: f64,
    ymin: f64,
    xmax: f64,
    ymax: f64,
) -> bool {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let p = [-dx, dx, -dy, dy];
    let q = [x1 - xmin, xmax - x1, y1 - ymin, ymax - y1];
    let mut u1 = 0.0_f64;
    let mut u2 = 1.0_f64;
    for (&pi, &qi) in p.iter().zip(q.iter()) {
        if pi.abs() <= f64::EPSILON {
            if qi < 0.0 {
                return false; // parallel to this edge and outside the slab
            }
        } else {
            let t = qi / pi;
            if pi < 0.0 {
                if t > u2 {
                    return false;
                }
                u1 = u1.max(t);
            } else {
                if t < u1 {
                    return false;
                }
                u2 = u2.min(t);
            }
        }
    }
    u1 <= u2
}

/// Warns about silkscreen (overlay) tracks that overlap a pad's copper. Silk on a
/// pad is almost always a defect — it prints on the land and trips silk-to-mask
/// DRC. Only overlay TRACKS are checked (the common offender); text and arcs are
/// not. The pad rectangle is inflated by the track half-width so a grazing track
/// is caught. This is topology-agnostic, so it is safe for any footprint.
fn silk_over_pad_warnings(fp: &crate::altium::pcblib::Footprint) -> Vec<Value> {
    use crate::altium::pcblib::Layer;
    let mut warnings = Vec::new();
    for track in &fp.tracks {
        let (top, bottom) = match track.layer {
            Layer::TopOverlay => (true, false),
            Layer::BottomOverlay => (false, true),
            _ => continue,
        };
        let half = track.width / 2.0;
        for pad in &fp.pads {
            let pad_top = matches!(pad.layer, Layer::TopLayer | Layer::MultiLayer);
            let pad_bottom = matches!(pad.layer, Layer::BottomLayer | Layer::MultiLayer);
            if !((top && pad_top) || (bottom && pad_bottom)) {
                continue;
            }
            let hw = pad.width / 2.0 + half;
            let hh = pad.height / 2.0 + half;
            if segment_intersects_rect(
                track.x1,
                track.y1,
                track.x2,
                track.y2,
                pad.x - hw,
                pad.y - hh,
                pad.x + hw,
                pad.y + hh,
            ) {
                warnings.push(json!({
                    "footprint": fp.name,
                    "type": "silk_over_pad",
                    "layer": track.layer.as_str(),
                    "pad": pad.designator,
                    "message": format!(
                        "{} track overlaps pad '{}' — move silkscreen clear of the pad",
                        track.layer.as_str(),
                        pad.designator
                    ),
                }));
            }
        }
    }
    warnings
}

/// Warns when two pads' copper overlaps on a shared layer. Overlapping copper
/// merges into one net, so a footprint can be structurally valid while every pin
/// is shorted together. Advisory only — same-designator pads are excluded because
/// stacking them is a legitimate way to build a compound land.
///
/// Reporting is capped so a systematic error on a large BGA cannot bury the
/// response; the cap message carries the true total.
fn pad_copper_overlap_warnings(fp: &crate::altium::pcblib::Footprint) -> Vec<Value> {
    use crate::altium::pcblib::MAX_REPORTED_PAD_OVERLAPS as MAX_REPORTED;

    let hits = fp.overlapping_pad_pairs();
    let mut warnings: Vec<Value> = hits
        .iter()
        .take(MAX_REPORTED)
        .map(|&(i, j, ox, oy)| {
            let (a, b) = (&fp.pads[i], &fp.pads[j]);
            json!({
                "footprint": fp.name,
                "type": "pad_copper_overlap",
                "layer": a.layer.as_str(),
                "pads": [a.designator, b.designator],
                "overlap_mm": [ox, oy],
                "message": format!(
                    "pads '{}' and '{}' overlap by {:.3} x {:.3} mm on {} — overlapping copper merges into one net",
                    a.designator, b.designator, ox, oy, a.layer.as_str()
                ),
            })
        })
        .collect();
    if hits.len() > MAX_REPORTED {
        warnings.push(json!({
            "footprint": fp.name,
            "type": "pad_copper_overlap",
            "message": format!(
                "{} overlapping pad pairs total; {} shown",
                hits.len(),
                MAX_REPORTED
            ),
        }));
    }
    warnings
}

/// Summarises a footprint's 3D body for the `write_pcblib` response so the caller
/// knows the body height that was written and whether one was auto-created (with
/// a default, `assumed` height it should confirm). All heights are in mm.
fn body_3d_summary(fp: &crate::altium::pcblib::Footprint, assumed_height: bool) -> Value {
    if fp.model_3d.is_some() {
        return json!({ "name": fp.name, "source": "step-embedded" });
    }
    if let Some(ext) = fp
        .component_bodies
        .iter()
        .find(|b| !b.model_name.is_empty())
    {
        return json!({ "name": fp.name, "source": "step-external", "model": ext.model_name });
    }
    if let Some(b) = fp.component_bodies.iter().find(|b| b.model_name.is_empty()) {
        let mut summary = json!({
            "name": fp.name,
            "source": if assumed_height { "auto-extruded" } else { "extruded" },
            "overall_height": b.overall_height,
            "standoff_height": b.standoff_height,
            "assumed_height": assumed_height,
        });
        if assumed_height {
            // Make the placeholder actionable: tell the caller to replace it rather
            // than leaving the guessed 1.0 mm height in the part.
            summary["action_required"] = json!(format!(
                "No 3D body height was given for '{}', so a {} mm placeholder was used. \
                 This is almost certainly wrong — look up the component's real height from \
                 its datasheet and call write_pcblib again with component_bodies[].overall_height \
                 set to the correct value.",
                fp.name, b.overall_height
            ));
        }
        return summary;
    }
    json!({
        "name": fp.name,
        "source": "none",
        "note": "No 3D body written. Set component_bodies[].overall_height to the real \
                 part height, or pass auto_3d_body:true for a flagged 1.0 mm placeholder.",
    })
}

impl McpServer {
    // ==================== Tool Handlers ====================

    /// Reads a `PcbLib` file and returns its contents.
    /// Supports pagination via limit/offset and filtering by `component_name`.
    #[allow(clippy::too_many_lines)] // Complex formatting logic for compact mode
    pub(crate) fn call_read_pcblib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::pcblib::primitives::PadStackMode;
        use crate::altium::PcbLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional pagination/filter parameters
        let component_name = arguments.get("component_name").and_then(Value::as_str);
        let (limit, offset) = match super::page_arguments(arguments) {
            Ok(page) => page,
            Err(e) => return ToolCallResult::error(e),
        };

        // Parse compact parameter (default: true - omit redundant per-layer data)
        let compact = arguments
            .get("compact")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        match PcbLib::open(filepath) {
            Ok(library) => {
                let total_count = library.len();

                // A requested component is resolved as every tool resolves
                // one — regardless of case — and a miss is an error naming
                // what is there, not an empty success.
                let selected: Vec<_> = match component_name {
                    Some(name) => match library.get(name) {
                        Some(fp) => vec![fp],
                        None => {
                            return ToolCallResult::error(super::component_not_found(
                                name,
                                &library.names(),
                            ))
                        }
                    },
                    None => library.iter().collect(),
                };
                let footprints: Vec<_> = selected
                    .into_iter()
                    .skip(offset)
                    .take(limit.unwrap_or(usize::MAX))
                    .map(|fp| {
                        // The struct's own serde shape — the one write_pcblib
                        // and update_component accept in full, and the one
                        // get_component and export_library emit — so every
                        // fidelity carrier (identities, raw blocks, record
                        // order) reaches the caller without a hand-kept list.
                        let mut fp_json = serde_json::to_value(fp).unwrap_or(Value::Null);
                        // Compact mode: a Simple pad's per-layer arrays carry
                        // nothing the main size/shape do not, so they go. A
                        // stacked pad keeps them — and its stack_mode — even
                        // when every layer happens to match: the mode is a
                        // stored property Altium shows, and rewriting it to
                        // "simple" here silently changed the pad on the next
                        // write.
                        if compact {
                            if let Some(pads) =
                                fp_json.get_mut("pads").and_then(Value::as_array_mut)
                            {
                                for (pad, pad_json) in fp.pads.iter().zip(pads.iter_mut()) {
                                    if pad.stack_mode == PadStackMode::Simple {
                                        if let Value::Object(obj) = pad_json {
                                            obj.remove("per_layer_sizes");
                                            obj.remove("per_layer_shapes");
                                            obj.remove("per_layer_corner_radii");
                                            obj.remove("per_layer_offsets");
                                        }
                                    }
                                }
                            }
                        }
                        fp_json
                    })
                    .collect();

                let returned_count = footprints.len();
                let has_more = if component_name.is_some() {
                    false // Single component fetch, no pagination
                } else {
                    offset + returned_count < total_count
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "units": "mm",
                    "total_count": total_count,
                    "returned_count": returned_count,
                    "offset": offset,
                    "has_more": has_more,
                    "compact": compact,
                    "footprints": footprints,
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Writes footprints to a `PcbLib` file.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn call_write_pcblib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::pcblib::PcbLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(footprints_json) = arguments.get("footprints").and_then(Value::as_array) else {
            return ToolCallResult::error("Missing required parameter: footprints");
        };

        // Collect and validate footprint names for duplicates
        let new_names: Vec<&str> = footprints_json
            .iter()
            .filter_map(|fp| fp.get("name").and_then(Value::as_str))
            .collect();

        // Check for duplicates within the new footprints — regardless of
        // case, which is how the library and the file's directory resolve them.
        {
            let mut seen = std::collections::HashSet::new();
            for name in &new_names {
                if !seen.insert(crate::altium::folded_name(name)) {
                    return ToolCallResult::error_with_context(
                        ErrorContext::new(
                            "write_pcblib",
                            format!("Duplicate footprint name: '{name}'"),
                        )
                        .with_filepath(filepath)
                        .with_component(*name)
                        .with_details("Each footprint in the request must have a unique name"),
                    );
                }
            }
        }

        // Validate footprint names (OLE storage names are limited to 31 units,
        // but the library layer handles that by truncating the storage name
        // while PATTERN keeps the full one).
        for name in &new_names {
            if let Err(e) = Self::validate_ole_name(name) {
                return ToolCallResult::error(e);
            }
        }

        let append = arguments
            .get("append")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Opt-in: synthesise a placeholder extruded 3D body for footprints that have
        // pads but no body/STEP. Off by default so the tool never adds geometry the
        // caller didn't request (a body is wrong for fiducials / test points / mounting
        // holes); the always-on `bodies` echo still reports `source: "none"` to nudge.
        let auto_3d_body = arguments
            .get("auto_3d_body")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let auto_designator = arguments
            .get("auto_designator")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        // If append mode and file exists, read existing library; otherwise create new
        let mut library = if append && std::path::Path::new(filepath).exists() {
            match PcbLib::open(filepath) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error_with_context(
                        ErrorContext::new(
                            "write_pcblib",
                            format!("Failed to read existing library: {e}"),
                        )
                        .with_filepath(filepath)
                        .with_details(
                            "The library file exists but could not be opened for appending",
                        ),
                    );
                }
            }
        } else {
            PcbLib::new()
        };

        // Check for duplicates with existing footprints in append mode
        if append {
            for name in &new_names {
                if let Some(existing) = library.get(name) {
                    return ToolCallResult::error(Self::taken_name_error(
                        format!("Footprint '{name}' already exists in the library"),
                        name,
                        &existing.name,
                    ));
                }
            }
        }

        // Silkscreen-over-pad warnings, echoed back so the caller can fix silk that
        // prints on a pad (a DRC defect) without opening Altium.
        let mut silk_warnings: Vec<Value> = Vec::new();

        // Per-footprint 3D-body summary echoed back so the caller sees the body
        // height that was written and whether one was auto-created.
        let mut bodies_echo: Vec<Value> = Vec::new();

        let keys = allowed_keys::PcbLibKeys::new();
        for fp_json in footprints_json {
            let mut footprint = match self.parse_footprint_json(
                fp_json,
                &keys,
                "write_pcblib",
                filepath,
                "Unnamed",
            ) {
                Ok(footprint) => footprint,
                Err(result) => return result,
            };

            // Auto-inject the `.Designator` special string on the Top Overlay if the
            // caller did not provide one, so every placed footprint renders its
            // reference designator. Placed just above the topmost pad (or at the
            // origin when there are no pads); the user can reposition in Altium.
            // Never for a footprint echoed back from a read: Altium's own
            // library footprints carry no designator text (none of the 22
            // golden ones does), so adding one there would change every real
            // footprint on a read-modify-write. A read echo always carries
            // `primitive_order`; from-scratch JSON never does.
            let is_read_echo = fp_json.get("primitive_order").is_some();
            let has_designator = footprint
                .text
                .iter()
                .any(|t| t.text.trim().eq_ignore_ascii_case(".designator"));
            if auto_designator && !is_read_echo && !has_designator {
                use crate::altium::pcblib::{Layer, PcbFlags, Text, TextJustification, TextKind};
                let top = footprint
                    .pads
                    .iter()
                    .map(|p| p.y + p.height / 2.0)
                    .fold(f64::NEG_INFINITY, f64::max);
                let y = if top.is_finite() { top + 0.6 } else { 0.0 };
                footprint.add_text(Text {
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
                    y,
                    text: ".Designator".to_string(),
                    height: 1.0,
                    layer: Layer::TopOverlay,
                    rotation: 0.0,
                    kind: TextKind::Stroke,
                    stroke_font: None,
                    stroke_width: None,
                    italic: false,
                    bold: false,
                    mirror: false,
                    // The `.Designator` special string works through its content;
                    // is_designator@41 stays at the template's 0x00 (byte-identity —
                    // no golden carries a `.Designator` text to settle Altium's own
                    // authoring value for this byte).
                    is_comment: false,
                    is_designator: false,
                    font_name: "Arial".to_string(),
                    // BottomLeft = the template's 0x03 anchor: the writer now honours
                    // @132, so keep the auto-designator on the template default to stay
                    // byte-identical (and oracle-safe).
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
            }

            // Opt-in (`auto_3d_body`): synthesise an extruded 3D body for a footprint
            // with pads but no STEP model and no component body, so it has a 3D presence
            // in Altium. Height can't be inferred from a 2D footprint, so it defaults to
            // 1.0 mm and is flagged `assumed_height` for the caller to confirm/override.
            // The empty outline makes the writer synthesise a bounding box from pads.
            let assumed_height = if auto_3d_body
                && footprint.model_3d.is_none()
                && footprint.component_bodies.is_empty()
                && !footprint.pads.is_empty()
            {
                use crate::altium::pcblib::{ComponentBody, Layer};
                footprint.add_component_body(ComponentBody {
                    model_id: String::new(),
                    identifier: String::new(),
                    texture_center_x: None,
                    texture_center_y: None,
                    texture_size_x: None,
                    texture_size_y: None,
                    texture_rotation: None,
                    raw_layer_id: None,
                    v7_layer: None,
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
                    // Synthesised body: no board association (free primitive).
                    net_index: 0xFFFF,
                    polygon_index: 0xFFFF,
                    component_index: -1,
                    additional_parameters: Vec::new(),
                    param_key_order: Vec::new(),
                });
                true
            } else {
                false
            };
            bodies_echo.push(body_3d_summary(&footprint, assumed_height));

            silk_warnings.extend(silk_over_pad_warnings(&footprint));
            silk_warnings.extend(pad_copper_overlap_warnings(&footprint));

            library.add(footprint);
        }

        // Create backup before destructive operation (if file exists)
        if let Err(e) = Self::create_backup(filepath) {
            return ToolCallResult::error(e);
        }

        match library.save(filepath) {
            Ok(()) => {
                let mut result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "footprint_count": library.len(),
                    "footprint_names": library.names(),
                });

                // Silkscreen-over-pad warnings (non-blocking): silk printed on a pad
                // is almost always a defect. Always present so the caller knows the
                // check ran; empty array when clean.
                result["warnings"] = Value::Array(silk_warnings);

                // Echo each footprint's 3D body (height + source), so the caller can
                // confirm an auto-created body's assumed height or correct it.
                result["bodies"] = Value::Array(bodies_echo);

                // Run post-write validation
                if let Some(validation) = Self::post_write_validation_pcblib(filepath) {
                    result["validation"] = validation;
                }

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Reads a `SchLib` file and returns its contents.
    /// Supports pagination via limit/offset and filtering by `component_name`.
    pub(crate) fn call_read_schlib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::SchLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional pagination/filter parameters
        let component_name = arguments.get("component_name").and_then(Value::as_str);
        let (limit, offset) = match super::page_arguments(arguments) {
            Ok(page) => page,
            Err(e) => return ToolCallResult::error(e),
        };

        match SchLib::open(filepath) {
            Ok(library) => {
                let total_count = library.len();

                // A requested component is resolved as every tool resolves
                // one — regardless of case — and a miss is an error naming
                // what is there, not an empty success.
                let selected: Vec<_> = match component_name {
                    Some(name) => match library.get(name) {
                        Some(symbol) => vec![symbol],
                        None => {
                            return ToolCallResult::error(super::component_not_found(
                                name,
                                &library.names(),
                            ))
                        }
                    },
                    None => library.iter().collect(),
                };
                let symbols: Vec<_> = selected
                    .into_iter()
                    .skip(offset)
                    .take(limit.unwrap_or(usize::MAX))
                    .map(|symbol| {
                        // The struct's own serde shape (see read_pcblib).
                        serde_json::to_value(symbol).unwrap_or(Value::Null)
                    })
                    .collect();

                let returned_count = symbols.len();
                let has_more = if component_name.is_some() {
                    false // Single component fetch, no pagination
                } else {
                    offset + returned_count < total_count
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "units": "schematic units (10 = 1 grid)",
                    "total_count": total_count,
                    "returned_count": returned_count,
                    "offset": offset,
                    "has_more": has_more,
                    "symbols": symbols,
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Writes symbols to a `SchLib` file.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn call_write_schlib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::schlib::SchLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(symbols_json) = arguments.get("symbols").and_then(Value::as_array) else {
            return ToolCallResult::error("Missing required parameter: symbols");
        };

        // Collect and validate symbol names
        let new_names: Vec<&str> = symbols_json
            .iter()
            .filter_map(|sym| sym.get("name").and_then(Value::as_str))
            .collect();

        // Check for duplicates within the new symbols — regardless of
        // case, which is how the library and the file's directory resolve them.
        {
            let mut seen = std::collections::HashSet::new();
            for name in &new_names {
                if !seen.insert(crate::altium::folded_name(name)) {
                    return ToolCallResult::error_with_context(
                        ErrorContext::new(
                            "write_schlib",
                            format!("Duplicate symbol name: '{name}'"),
                        )
                        .with_filepath(filepath)
                        .with_component(*name)
                        .with_details("Each symbol in the request must have a unique name"),
                    );
                }
            }
        }

        // Validate symbol names (OLE storage names are limited to 31 units,
        // but the library layer handles that by truncating the storage name
        // while LIBREFERENCE keeps the full one).
        for name in &new_names {
            if let Err(e) = Self::validate_ole_name(name) {
                return ToolCallResult::error(e);
            }
        }

        let append = arguments
            .get("append")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // If append mode and file exists, read existing library; otherwise create new
        let mut library = if append && std::path::Path::new(filepath).exists() {
            match SchLib::open(filepath) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error_with_context(
                        ErrorContext::new(
                            "write_schlib",
                            format!("Failed to read existing library: {e}"),
                        )
                        .with_filepath(filepath)
                        .with_details(
                            "The library file exists but could not be opened for appending",
                        ),
                    );
                }
            }
        } else {
            SchLib::new()
        };

        // Check for duplicates with existing symbols in append mode
        if append {
            for name in &new_names {
                if let Some(existing) = library.get(name) {
                    return ToolCallResult::error(Self::taken_name_error(
                        format!("Symbol '{name}' already exists in the library"),
                        name,
                        &existing.name,
                    ));
                }
            }
        }

        // Names of the symbols written by *this* call. Recorded as they are added
        // (rather than reused from `new_names`) so a symbol that omitted "name" and
        // fell back to the default is still represented. Used to scope the geometry
        // echo below to what the caller actually wrote.
        let mut written_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        let keys = allowed_keys::SchLibKeys::new();
        for sym_json in symbols_json {
            let symbol = match self.parse_symbol_json(
                sym_json,
                &keys,
                "write_schlib",
                filepath,
                "Unnamed",
            ) {
                Ok(symbol) => symbol,
                Err(result) => return result,
            };

            written_names.insert(symbol.name.clone());
            library.add(symbol);
        }

        // Create backup before destructive operation (if file exists)
        if let Err(e) = Self::create_backup(filepath) {
            return ToolCallResult::error(e);
        }

        match library.save(filepath) {
            Ok(()) => {
                let symbol_names: Vec<_> = library.iter().map(|s| s.name.clone()).collect();
                let mut result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "symbol_count": library.len(),
                    "symbol_names": symbol_names,
                });

                // Echo computed pin geometry (body-attach end, connection tip,
                // orientation, bounding box) so the caller can verify pin placement
                // and catch flipped/misaligned pins without opening Altium.
                //
                // Scoped to the symbols written by this call. Echoing the whole
                // library made an `append: true` sequence grow the response
                // quadratically — a 27-symbol library built over 11 appends echoed
                // 196 symbol-geometry blocks instead of 26, large enough to stop the
                // response being usable — and pre-existing symbols tell the caller
                // nothing about the write it just performed.
                result["geometry"] = Value::Array(
                    library
                        .iter()
                        .filter(|s| written_names.contains(&s.name))
                        .map(symbol_geometry)
                        .collect(),
                );

                // Run post-write validation
                if let Some(validation) = Self::post_write_validation_schlib(filepath) {
                    result["validation"] = validation;
                }

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Writes an Altium Library Package (`.LibPkg`) project file that groups
    /// the given source documents so Altium can compile them into an
    /// Integrated Library. Only generates the project source; compiling to
    /// `.IntLib` is done inside Altium.
    pub(crate) fn call_write_libpkg(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::libpkg;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Validate file extension
        let ext = std::path::Path::new(filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);
        if ext.as_deref() != Some("libpkg") {
            return ToolCallResult::error("write_libpkg only supports .LibPkg files");
        }

        let Some(documents) = arguments.get("documents").and_then(Value::as_array) else {
            return ToolCallResult::error("Missing required parameter: documents");
        };
        let docs: Vec<String> = documents
            .iter()
            .filter_map(|d| d.as_str().map(String::from))
            .collect();
        if docs.is_empty() {
            return ToolCallResult::error(
                "documents must contain at least one .SchLib/.PcbLib path",
            );
        }

        let path = std::path::Path::new(filepath);
        let content = libpkg::build_libpkg(path, &docs);
        if let Err(e) = std::fs::write(path, content) {
            return ToolCallResult::error(format!("Failed to write LibPkg: {e}"));
        }

        let relative: Vec<String> = docs
            .iter()
            .map(|d| libpkg::relative_to_libpkg(path, d))
            .collect();
        let result = json!({
            "status": "success",
            "filepath": filepath,
            "documents": relative,
            "count": relative.len(),
            "note": "Open in Altium and run Project > Compile Integrated Library to produce the .IntLib.",
        });
        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Lists component names in a library file.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn call_list_components(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::{PcbLib, SchLib};

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional pagination parameters
        let (limit, offset) = match super::page_arguments(arguments) {
            Ok(page) => page,
            Err(e) => return ToolCallResult::error(e),
        };

        // Parse include_metadata parameter (default: false)
        let include_metadata = arguments
            .get("include_metadata")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Try to determine file type from extension
        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => match PcbLib::open(filepath) {
                Ok(library) => {
                    let total_count = library.len();

                    // Apply pagination and optionally include metadata
                    let components: Vec<Value> = if include_metadata {
                        library
                            .iter()
                            .skip(offset)
                            .take(limit.unwrap_or(usize::MAX))
                            .map(|fp| {
                                let mut entry = serde_json::Map::new();
                                entry.insert("name".to_string(), json!(fp.name));
                                entry.insert("description".to_string(), json!(fp.description));
                                // One count per primitive kind, every kind.
                                for kind in crate::altium::pcblib::PrimitiveKind::WRITE_ORDER {
                                    entry.insert(
                                        format!("{}_count", kind.name()),
                                        json!(fp.count_of(kind)),
                                    );
                                }
                                entry.insert(
                                    "has_3d_model".to_string(),
                                    json!(fp.model_3d.is_some() || !fp.component_bodies.is_empty()),
                                );
                                Value::Object(entry)
                            })
                            .collect()
                    } else {
                        library
                            .names()
                            .into_iter()
                            .skip(offset)
                            .take(limit.unwrap_or(usize::MAX))
                            .map(|n| json!(n))
                            .collect()
                    };

                    let returned_count = components.len();
                    let has_more = offset + returned_count < total_count;

                    let result = json!({
                        "status": "success",
                        "filepath": filepath,
                        "file_type": "PcbLib",
                        "total_count": total_count,
                        "returned_count": returned_count,
                        "offset": offset,
                        "has_more": has_more,
                        "include_metadata": include_metadata,
                        "components": components,
                    });
                    ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
                }
                Err(e) => {
                    let result = json!({
                        "status": "error",
                        "filepath": filepath,
                        "error": e.to_string(),
                    });
                    ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
                }
            },
            Some("schlib") => match SchLib::open(filepath) {
                Ok(library) => {
                    let total_count = library.len();

                    // Apply pagination and optionally include metadata
                    let components: Vec<Value> = if include_metadata {
                        library
                            .iter()
                            .skip(offset)
                            .take(limit.unwrap_or(usize::MAX))
                            .map(|s| {
                                json!({
                                    "name": s.name,
                                    "description": s.description,
                                    "designator": s.designator,
                                    "part_count": s.part_count,
                                    "pin_count": s.pins.len(),
                                    "footprint_count": s.footprints.len(),
                                })
                            })
                            .collect()
                    } else {
                        library
                            .iter()
                            .map(|s| json!(s.name.clone()))
                            .skip(offset)
                            .take(limit.unwrap_or(usize::MAX))
                            .collect()
                    };

                    let returned_count = components.len();
                    let has_more = offset + returned_count < total_count;

                    let result = json!({
                        "status": "success",
                        "filepath": filepath,
                        "file_type": "SchLib",
                        "total_count": total_count,
                        "returned_count": returned_count,
                        "offset": offset,
                        "has_more": has_more,
                        "include_metadata": include_metadata,
                        "components": components,
                    });
                    ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
                }
                Err(e) => {
                    let result = json!({
                        "status": "error",
                        "filepath": filepath,
                        "error": e.to_string(),
                    });
                    ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
                }
            },
            _ => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": super::unsupported_file_type(filepath),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Extracts style information from a library file.
    pub(crate) fn call_extract_style(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => Self::extract_pcblib_style(filepath),
            Some("schlib") => Self::extract_schlib_style(filepath),
            _ => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": super::unsupported_file_type(filepath),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Extracts style from a `PcbLib` file: stroke widths of tracks and arcs
    /// per layer, pad shapes, text heights, and the layer usage of every
    /// primitive kind.
    pub(crate) fn extract_pcblib_style(filepath: &str) -> ToolCallResult {
        use crate::altium::pcblib::PrimitiveKind;
        use crate::altium::PcbLib;
        use std::collections::HashMap;

        match PcbLib::open(filepath) {
            Ok(library) => {
                // Stroke widths by layer
                let mut track_widths: HashMap<String, Vec<f64>> = HashMap::new();
                let mut arc_widths: HashMap<String, Vec<f64>> = HashMap::new();
                // Pad shapes count
                let mut pad_shapes: HashMap<String, usize> = HashMap::new();
                // Text heights
                let mut text_heights: Vec<f64> = Vec::new();
                // Layers used, by every kind that sits on one
                let mut layers_used: HashMap<String, usize> = HashMap::new();

                for fp in library.iter() {
                    for track in &fp.tracks {
                        track_widths
                            .entry(track.layer.as_str().to_string())
                            .or_default()
                            .push(track.width);
                    }
                    for arc in &fp.arcs {
                        arc_widths
                            .entry(arc.layer.as_str().to_string())
                            .or_default()
                            .push(arc.width);
                    }
                    for pad in &fp.pads {
                        let shape_name = format!("{:?}", pad.shape);
                        *pad_shapes.entry(shape_name).or_insert(0) += 1;
                    }
                    for text in &fp.text {
                        text_heights.push(text.height);
                    }
                    for kind in PrimitiveKind::WRITE_ORDER {
                        for layer in fp.layers_of(kind) {
                            *layers_used.entry(layer.as_str().to_string()).or_insert(0) += 1;
                        }
                    }
                }

                let track_width_stats = Self::width_stats_by_layer(track_widths);
                let arc_width_stats = Self::width_stats_by_layer(arc_widths);

                // Calculate text height stats
                let text_height_stats = if text_heights.is_empty() {
                    json!(null)
                } else {
                    let min = text_heights.iter().copied().fold(f64::INFINITY, f64::min);
                    let max = text_heights
                        .iter()
                        .copied()
                        .fold(f64::NEG_INFINITY, f64::max);
                    let most_common = Self::most_common_f64(&text_heights);
                    json!({
                        "min_mm": min,
                        "max_mm": max,
                        "most_common_mm": most_common,
                        "count": text_heights.len()
                    })
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "file_type": "PcbLib",
                    "footprint_count": library.len(),
                    "style": {
                        "track_widths_by_layer": track_width_stats,
                        "arc_widths_by_layer": arc_width_stats,
                        "pad_shapes": pad_shapes,
                        "text_heights": text_height_stats,
                        "layers_used": layers_used
                    }
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Min / max / average / most common stroke width and the count, per
    /// layer name.
    #[allow(clippy::cast_precision_loss)]
    fn width_stats_by_layer(
        widths_by_layer: std::collections::HashMap<String, Vec<f64>>,
    ) -> std::collections::HashMap<String, Value> {
        widths_by_layer
            .into_iter()
            .map(|(layer, widths)| {
                let min = widths.iter().copied().fold(f64::INFINITY, f64::min);
                let max = widths.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let avg = widths.iter().sum::<f64>() / widths.len() as f64;
                let most_common = Self::most_common_f64(&widths);
                (
                    layer,
                    json!({
                        "min_mm": min,
                        "max_mm": max,
                        "avg_mm": avg,
                        "most_common_mm": most_common,
                        "count": widths.len()
                    }),
                )
            })
            .collect()
    }

    /// Extracts style from a `SchLib` file: pin lengths, and the stroke
    /// widths, stroke / fill / text colours of every record kind.
    pub(crate) fn extract_schlib_style(filepath: &str) -> ToolCallResult {
        use crate::altium::schlib::SchPrimitiveKind;
        use crate::altium::SchLib;
        use std::collections::HashMap;

        match SchLib::open(filepath) {
            Ok(library) => {
                // Line widths
                let mut line_widths: Vec<u8> = Vec::new();
                // Pin lengths
                let mut pin_lengths: Vec<i32> = Vec::new();
                // Colours used
                let mut line_colors: HashMap<String, usize> = HashMap::new();
                let mut fill_colors: HashMap<String, usize> = HashMap::new();
                let mut text_colors: HashMap<String, usize> = HashMap::new();
                // Rectangle stats
                let mut rect_filled_count = 0usize;
                let mut rect_unfilled_count = 0usize;

                let count_color = |colors: &mut HashMap<String, usize>, color: u32| {
                    *colors.entry(format!("#{color:06X}")).or_insert(0) += 1;
                };
                for symbol in library.iter() {
                    for pin in &symbol.pins {
                        pin_lengths.push(pin.length);
                    }
                    for rect in &symbol.rectangles {
                        if rect.filled {
                            rect_filled_count += 1;
                        } else {
                            rect_unfilled_count += 1;
                        }
                    }
                    for kind in SchPrimitiveKind::WRITE_ORDER {
                        for style in symbol.styles_of(kind) {
                            line_widths.extend(style.line_width);
                            if let Some(color) = style.line_color {
                                count_color(&mut line_colors, color);
                            }
                            if let Some(color) = style.fill_color {
                                count_color(&mut fill_colors, color);
                            }
                            if let Some(color) = style.text_color {
                                count_color(&mut text_colors, color);
                            }
                        }
                    }
                }

                // Calculate stats
                let pin_length_stats = if pin_lengths.is_empty() {
                    json!(null)
                } else {
                    let min = *pin_lengths.iter().min().unwrap();
                    let max = *pin_lengths.iter().max().unwrap();
                    let most_common = Self::most_common(&pin_lengths);
                    json!({
                        "min_units": min,
                        "max_units": max,
                        "most_common_units": most_common,
                        "count": pin_lengths.len()
                    })
                };

                let line_width_stats = if line_widths.is_empty() {
                    json!(null)
                } else {
                    let min = *line_widths.iter().min().unwrap();
                    let max = *line_widths.iter().max().unwrap();
                    let most_common = Self::most_common(&line_widths);
                    json!({
                        "min": min,
                        "max": max,
                        "most_common": most_common,
                        "count": line_widths.len()
                    })
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "file_type": "SchLib",
                    "symbol_count": library.len(),
                    "style": {
                        "pin_lengths": pin_length_stats,
                        "line_widths": line_width_stats,
                        "line_colors": line_colors,
                        "fill_colors": fill_colors,
                        "text_colors": text_colors,
                        "rectangles": {
                            "filled_count": rect_filled_count,
                            "unfilled_count": rect_unfilled_count
                        }
                    }
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Finds the most common value in a slice of hashable, copyable values.
    ///
    /// Returns the default value if the slice is empty.
    pub(crate) fn most_common<T>(values: &[T]) -> T
    where
        T: std::hash::Hash + Eq + Copy + Default,
    {
        use std::collections::HashMap;
        let mut counts: HashMap<T, usize> = HashMap::new();
        for &v in values {
            *counts.entry(v).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map_or_else(T::default, |(key, _)| key)
    }

    /// Finds the most common value in a slice of f64, rounded to 2 decimal places.
    ///
    /// Since f64 doesn't implement Hash/Eq, values are quantized to centesimal
    /// precision (0.01) for grouping purposes.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub(crate) fn most_common_f64(values: &[f64]) -> f64 {
        use std::collections::HashMap;
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for &v in values {
            // Round to 2 decimal places for grouping
            let key = (v * 100.0).round() as i64;
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map_or(0.0, |(key, _)| key as f64 / 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{body_3d_summary, pin_tip, symbol_geometry};
    use crate::altium::schlib::{Pin, PinOrientation, Rectangle, Symbol};
    use crate::mcp::tools::parsing::ieee_designator_prefix;

    #[test]
    fn segment_rect_intersection_detects_silk_over_pad_geometry() {
        use super::segment_intersects_rect;
        // Horizontal segment straight through the rect.
        assert!(segment_intersects_rect(
            -5.0, 0.0, 5.0, 0.0, -1.0, -1.0, 1.0, 1.0
        ));
        // Vertical stripe through the rect (the reported silk-on-pad case).
        assert!(segment_intersects_rect(
            0.0, -5.0, 0.0, 5.0, -1.0, -1.0, 1.0, 1.0
        ));
        // Endpoint inside the rect.
        assert!(segment_intersects_rect(
            0.0, 0.0, 5.0, 5.0, -1.0, -1.0, 1.0, 1.0
        ));
        // Clear of the rect (no overlap).
        assert!(!segment_intersects_rect(
            2.0, 2.0, 3.0, 3.0, -1.0, -1.0, 1.0, 1.0
        ));
        // Parallel and outside the slab.
        assert!(!segment_intersects_rect(
            -5.0, 2.0, 5.0, 2.0, -1.0, -1.0, 1.0, 1.0
        ));
    }

    #[test]
    fn body_3d_summary_reports_source_and_height() {
        use crate::altium::pcblib::{ComponentBody, Footprint, Layer};
        let body = |h: f64, name: &str| ComponentBody {
            raw_layer_id: None,
            v7_layer: None,
            model_id: String::new(),
            identifier: String::new(),
            texture_center_x: None,
            texture_center_y: None,
            texture_size_x: None,
            texture_size_y: None,
            texture_rotation: None,
            model_name: name.to_string(),
            embedded: false,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            z_offset: 0.0,
            overall_height: h,
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
            param_key_order: Vec::new(),
        };

        // Explicit extruded body: reports its height, not assumed.
        let mut ext = Footprint::new("EXT");
        ext.add_component_body(body(2.5, ""));
        assert_eq!(body_3d_summary(&ext, false)["source"], "extruded");
        assert_eq!(body_3d_summary(&ext, false)["overall_height"], 2.5);
        assert_eq!(body_3d_summary(&ext, false)["assumed_height"], false);

        // Same body, auto-created path: flagged assumed.
        assert_eq!(body_3d_summary(&ext, true)["source"], "auto-extruded");
        assert_eq!(body_3d_summary(&ext, true)["assumed_height"], true);
        // The assumed case carries an actionable message prompting a real height.
        assert!(body_3d_summary(&ext, true)["action_required"].is_string());
        // The explicit case does not.
        assert!(body_3d_summary(&ext, false)["action_required"].is_null());

        // No body at all: source none.
        let none = Footprint::new("NONE");
        assert_eq!(body_3d_summary(&none, false)["source"], "none");
    }

    #[test]
    fn pin_tip_points_outward_per_orientation() {
        assert_eq!(
            pin_tip(&Pin::new("N", "1", -40, 20, 30, PinOrientation::Left)),
            (-70, 20)
        );
        assert_eq!(
            pin_tip(&Pin::new("N", "1", 40, 20, 30, PinOrientation::Right)),
            (70, 20)
        );
        assert_eq!(
            pin_tip(&Pin::new("N", "1", 0, 0, 30, PinOrientation::Up)),
            (0, 30)
        );
        assert_eq!(
            pin_tip(&Pin::new("N", "1", 0, 0, 30, PinOrientation::Down)),
            (0, -30)
        );
    }

    #[test]
    fn symbol_geometry_reports_tip_orientation_and_bbox() {
        let mut s = Symbol::new("U1");
        s.add_pin(Pin::new("VIN", "1", -50, 20, 30, PinOrientation::Left));
        s.add_pin(Pin::new("OUT", "2", 50, 20, 30, PinOrientation::Right));
        s.add_rectangle(Rectangle::new(-50, 40, 50, -40));
        let g = symbol_geometry(&s);
        assert_eq!(g["pins"][0]["orientation"], "left");
        assert_eq!(g["pins"][0]["body_end"]["x"], -50);
        assert_eq!(g["pins"][0]["tip"]["x"], -80);
        assert_eq!(g["pins"][1]["tip"]["x"], 80);
        assert_eq!(g["bounding_box"]["min_x"], -80);
        assert_eq!(g["bounding_box"]["max_x"], 80);
    }

    #[test]
    fn ieee_map_known_types() {
        assert_eq!(ieee_designator_prefix("resistor"), "R");
        assert_eq!(ieee_designator_prefix("capacitor"), "C");
        assert_eq!(ieee_designator_prefix("inductor"), "L");
        assert_eq!(ieee_designator_prefix("diode"), "D");
        assert_eq!(ieee_designator_prefix("led"), "D");
        assert_eq!(ieee_designator_prefix("transistor"), "Q");
        assert_eq!(ieee_designator_prefix("mosfet"), "Q");
        assert_eq!(ieee_designator_prefix("connector"), "J");
        assert_eq!(ieee_designator_prefix("crystal"), "Y");
        assert_eq!(ieee_designator_prefix("ic"), "U");
        assert_eq!(ieee_designator_prefix("regulator"), "U");
    }

    #[test]
    fn ieee_map_is_case_and_whitespace_insensitive() {
        assert_eq!(ieee_designator_prefix("  Resistor "), "R");
        assert_eq!(ieee_designator_prefix("CAPACITOR"), "C");
    }

    #[test]
    fn ieee_map_unknown_falls_back_to_u() {
        assert_eq!(ieee_designator_prefix("flux_capacitor"), "U");
        assert_eq!(ieee_designator_prefix(""), "U");
    }

    // ==================== fidelity replay ====================

    mod fidelity_replay {
        use crate::altium::pcblib::{
            Fill, Footprint, Layer, Pad, PcbFlags, PcbLib, Text, TextJustification, TextKind,
            Track, Via,
        };
        use crate::mcp::tools::test_support::{
            assert_same_stream, component_streams, mask_generated_ids, unique_ids,
        };
        use crate::mcp::tools::test_support::{
            create_test_server, get_result_text, parse_result_json, test_temp_dir,
        };
        use serde_json::json;

        /// A footprint whose fidelity fields all carry values, in a
        /// deliberately non-canonical primitive order (text before pads) so
        /// order replay is observable rather than coincidental. The text is
        /// the `.Designator` special string a real footprint carries — also
        /// what keeps `write_pcblib`'s auto-injection of one from adding a
        /// primitive the source never had.
        fn replay_footprint() -> Footprint {
            let mut fp = Footprint::new("REPLAY");
            fp.guid = Some("{11111111-2222-3333-4444-555555555555}".to_string());
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
                y: -2.0,
                text: ".Designator".to_string(),
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
                guid: Some("{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}".to_string()),
                raw_geometry: None,
            });
            let mut via = Via::new(1.5, 0.0, 0.6, 0.3);
            via.guid = Some("{22222222-3333-4444-5555-666666666666}".to_string());
            fp.add_via(via);
            let mut track = Track::new(-1.0, -1.0, 1.0, -1.0, 0.2, Layer::TopOverlay);
            track.guid = Some("{33333333-4444-5555-6666-777777777777}".to_string());
            fp.add_track(track);
            let mut pad = Pad::smd("1", -0.5, 0.0, 0.6, 0.5);
            pad.guid = Some("{44444444-5555-6666-7777-888888888888}".to_string());
            fp.add_pad(pad);
            let mut fill = Fill::new(-0.5, 0.8, 0.5, 1.2, Layer::TopLayer);
            fill.guid = Some("{55555555-6666-7777-8888-999999999999}".to_string());
            fp.add_fill(fill);
            fp
        }

        /// `read_pcblib` → `write_pcblib` replays every fidelity field the
        /// reader emits — pad `raw_tail`, via `raw_block`, text
        /// `raw_geometry`, per-primitive and footprint `guid`s, and the
        /// interleaved `primitive_order` — so a read-modify-write through the
        /// tool layer preserves them exactly as the library API does. The
        /// assertion is total: the second read's footprint JSON must equal
        /// the first, field for field (raw fields included, so the written
        /// binary blocks were byte-identical to the source's).
        #[test]
        fn write_pcblib_replays_fidelity_fields_read_pcblib_emits() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let mut lib = PcbLib::new();
            lib.add(replay_footprint());
            let src = dir.path().join("Src.PcbLib");
            lib.save(&src).unwrap();

            let first_read = server.call_read_pcblib(&json!({
                "filepath": src.to_string_lossy(),
            }));
            assert!(!first_read.is_error, "{}", get_result_text(&first_read));
            let footprints_before = parse_result_json(&first_read)["footprints"].clone();

            // The first read must actually carry the replay fields, or the
            // equality below passes vacuously.
            let fp = &footprints_before[0];
            assert_eq!(
                fp["primitive_order"],
                json!(["text", "via", "track", "pad", "fill"]),
                "authored (non-canonical) order survives the first read"
            );
            assert!(fp["pads"][0]["raw_tail"].is_string(), "pad raw_tail read");
            assert!(fp["vias"][0]["raw_block"].is_string(), "via raw_block read");
            assert!(
                fp["text"][0]["raw_geometry"].is_string(),
                "text raw_geometry read"
            );
            for list in ["pads", "vias", "tracks", "fills", "text"] {
                assert!(fp[list][0]["guid"].is_string(), "{list} guid read");
            }
            assert!(fp["guid"].is_string(), "footprint guid read");

            let dst = dir.path().join("Dst.PcbLib");
            let write = server.call_write_pcblib(&json!({
                "filepath": dst.to_string_lossy(),
                "footprints": footprints_before,
            }));
            assert!(!write.is_error, "{}", get_result_text(&write));

            let second_read = server.call_read_pcblib(&json!({
                "filepath": dst.to_string_lossy(),
            }));
            assert!(!second_read.is_error, "{}", get_result_text(&second_read));
            let footprints_after = parse_result_json(&second_read)["footprints"].clone();

            assert_eq!(
                footprints_before, footprints_after,
                "read → write → read is lossless"
            );
        }

        /// An Altium-authored footprint carries no `.Designator` text (none of
        /// the golden's 22 does), so a read → write through the tool layer must
        /// not add one: the echo's `primitive_order` marks it as a replay and
        /// the primitive count comes back unchanged.
        #[test]
        fn write_pcblib_does_not_add_a_designator_to_a_read_echo() {
            let dir = test_temp_dir();
            let samples = std::path::Path::new("scripts/samples")
                .canonicalize()
                .unwrap();
            let server =
                crate::mcp::server::McpServer::new(vec![dir.path().to_path_buf(), samples.clone()]);

            let read = server.call_read_pcblib(&json!({
                "filepath": samples.join("footprints.PcbLib").to_string_lossy(),
                "component_name": "PAD_SHAPES",
            }));
            assert!(!read.is_error, "{}", get_result_text(&read));
            let footprints = parse_result_json(&read)["footprints"].clone();
            let texts_before = footprints[0]["text"].as_array().map_or(0, Vec::len);
            let has_designator = footprints[0]["text"].as_array().is_some_and(|texts| {
                texts.iter().any(|t| {
                    t["text"]
                        .as_str()
                        .is_some_and(|s| s.eq_ignore_ascii_case(".designator"))
                })
            });
            assert!(
                !has_designator,
                "the golden footprint has no designator text to begin with"
            );

            let out = dir.path().join("Echo.PcbLib");
            let written = server.call_write_pcblib(&json!({
                "filepath": out.to_string_lossy(),
                "footprints": footprints,
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));
            let back = server.call_read_pcblib(&json!({ "filepath": out.to_string_lossy() }));
            let texts_after = parse_result_json(&back)["footprints"][0]["text"]
                .as_array()
                .map_or(0, Vec::len);
            assert_eq!(
                texts_after, texts_before,
                "no primitive was added to a replayed footprint"
            );
        }

        /// The tool-layer twin of the library-level byte-fidelity suite: every
        /// golden footprint read as JSON through `read_pcblib` and written back
        /// through `write_pcblib` comes out byte-identical — the `Data` stream,
        /// the unique-id records and (as a set, since Altium scrambles their
        /// order) the primitive GUIDs. Anything the JSON boundary drops,
        /// invents or reorders fails here, footprint by footprint.
        #[test]
        fn write_pcblib_replays_every_golden_footprint_byte_for_byte() {
            let dir = test_temp_dir();
            let samples = std::path::Path::new("scripts/samples")
                .canonicalize()
                .unwrap();
            let server =
                crate::mcp::server::McpServer::new(vec![dir.path().to_path_buf(), samples.clone()]);
            let src = samples.join("footprints.PcbLib");

            let read = server.call_read_pcblib(&json!({ "filepath": src.to_string_lossy() }));
            assert!(!read.is_error, "{}", get_result_text(&read));
            let footprints = parse_result_json(&read)["footprints"].clone();
            assert!(footprints.as_array().is_some_and(|f| f.len() > 20));

            let out = dir.path().join("Replay.PcbLib");
            let written = server.call_write_pcblib(&json!({
                "filepath": out.to_string_lossy(),
                "footprints": footprints,
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));

            let (golden, ours) = (component_streams(&src), component_streams(&out));
            // A non-ASCII storage name is widened through the writing
            // machine's code page, so the one such golden footprint may sit
            // under a different storage name: pair the leftovers by elimination.
            let shared: Vec<String> = golden
                .keys()
                .filter(|k| ours.contains_key(*k))
                .cloned()
                .collect();
            let g_left: Vec<&String> = golden.keys().filter(|k| !ours.contains_key(*k)).collect();
            let o_left: Vec<&String> = ours.keys().filter(|k| !golden.contains_key(*k)).collect();
            assert_eq!(
                g_left.len(),
                o_left.len(),
                "footprints lost or invented: {g_left:?} vs {o_left:?}"
            );
            assert!(
                g_left.len() <= 1,
                "only the single non-ASCII name may be renamed: {g_left:?}"
            );
            let pairs: Vec<(&String, &String)> = shared
                .iter()
                .map(|k| (k, k))
                .chain(g_left.iter().copied().zip(o_left.iter().copied()))
                .collect();
            assert_eq!(pairs.len(), golden.len());

            for (g_name, o_name) in pairs {
                let (g, o) = (&golden[g_name], &ours[o_name]);
                for stream in ["Data", "WideStrings", "UniqueIDPrimitiveInformation/Data"] {
                    assert_same_stream(&format!("{g_name}/{stream}"), g.get(stream), o.get(stream));
                }
                // Altium scrambles the PrimitiveGuids record order while the
                // writer emits it canonically: compared as record sets.
                let guids = |bytes: Option<&Vec<u8>>| -> std::collections::BTreeSet<Vec<u8>> {
                    bytes
                        .map_or(&[][..], Vec::as_slice)
                        .chunks_exact(24)
                        .map(<[u8]>::to_vec)
                        .collect()
                };
                assert_eq!(
                    guids(g.get("PrimitiveGuids/Data")),
                    guids(o.get("PrimitiveGuids/Data")),
                    "{g_name}/PrimitiveGuids"
                );
            }
        }

        /// The `SchLib` twin: every golden symbol read as JSON through
        /// `read_schlib` and written back through `write_schlib` produces the
        /// same bytes as the library-level save the byte-fidelity suite holds
        /// to the golden — every symbol storage's streams and the shared image
        /// `Storage`, so a record the JSON boundary drops, invents or reorders
        /// fails here, symbol by symbol.
        #[test]
        fn write_schlib_replays_every_golden_symbol_byte_for_byte() {
            use crate::altium::SchLib;

            let dir = test_temp_dir();
            let samples = std::path::Path::new("scripts/samples")
                .canonicalize()
                .unwrap();
            let server =
                crate::mcp::server::McpServer::new(vec![dir.path().to_path_buf(), samples.clone()]);
            let src = samples.join("symbols.SchLib");

            let baseline = dir.path().join("Baseline.SchLib");
            SchLib::open(&src).unwrap().save(&baseline).unwrap();

            let read = server.call_read_schlib(&json!({ "filepath": src.to_string_lossy() }));
            assert!(!read.is_error, "{}", get_result_text(&read));
            let symbols = parse_result_json(&read)["symbols"].clone();
            assert!(symbols.as_array().is_some_and(|s| s.len() > 50));

            let out = dir.path().join("Replay.SchLib");
            let written = server.call_write_schlib(&json!({
                "filepath": out.to_string_lossy(),
                "symbols": symbols,
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));

            // A record the golden stores without a UniqueID gets a fresh one
            // on every save, so those IDs — and only those — are masked; an ID
            // the golden carries must come through unchanged.
            let golden_ids: std::collections::HashSet<Vec<u8>> = component_streams(&src)
                .values()
                .flat_map(|streams| streams.values())
                .flat_map(|bytes| unique_ids(bytes))
                .collect();
            let (expected, ours) = (component_streams(&baseline), component_streams(&out));
            assert_eq!(
                expected.keys().collect::<Vec<_>>(),
                ours.keys().collect::<Vec<_>>(),
                "symbol storages differ"
            );
            for (name, e) in &expected {
                let o = &ours[name];
                assert_eq!(
                    e.keys().collect::<Vec<_>>(),
                    o.keys().collect::<Vec<_>>(),
                    "{name}: streams differ"
                );
                for (stream, a) in e {
                    let a = mask_generated_ids(a, &golden_ids);
                    let b = o.get(stream).map(|b| mask_generated_ids(b, &golden_ids));
                    assert_same_stream(&format!("{name}/{stream}"), Some(&a), b.as_ref());
                }
            }
        }

        /// A footprint's own GUID is held to the GUID form, and a component
        /// body that cannot be built is refused by kind and index like a pad.
        #[test]
        fn write_tools_refuse_a_pipe_in_record_text_by_field() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let sch = dir.path().join("Pipes.SchLib");
            let result = server.call_write_schlib(&json!({
                "filepath": sch.to_string_lossy(),
                "symbols": [{
                    "name": "S",
                    "pins": [{ "designator": "1", "name": "p|q", "x": 0, "y": 0, "length": 10,
                               "orientation": "left" }],
                    "parameters": [{ "name": "Value", "value": "1|2" }],
                }],
            }));
            assert!(result.is_error);
            let text = get_result_text(&result);
            assert!(
                text.contains("Symbol 'S' parameters[].value contains '|'"),
                "{text}"
            );
            assert!(text.contains("U+00A6"), "{text}");
            assert!(!sch.exists(), "nothing written");

            let pcb = dir.path().join("Pipes.PcbLib");
            let result = server.call_write_pcblib(&json!({
                "filepath": pcb.to_string_lossy(),
                "footprints": [{
                    "name": "F",
                    "pads": [{ "designator": "1|2", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                    "regions": [{ "layer": "Top Layer", "name": "R|1",
                                  "vertices": [{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 }, { "x": 0.0, "y": 1.0 }] }],
                }],
            }));
            assert!(result.is_error);
            let text = get_result_text(&result);
            assert!(
                text.contains("Footprint 'F' regions[].name contains '|'"),
                "{text}"
            );
            assert!(!pcb.exists(), "nothing written");
        }

        #[test]
        fn write_pcblib_refuses_a_bad_footprint_guid_and_names_a_bad_body_by_index() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Guid.PcbLib");
            let pad = json!({ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 });

            let result = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{ "name": "F", "pads": [pad], "guid": "not-a-guid" }],
            }));
            assert!(result.is_error);
            let text = get_result_text(&result);
            assert!(
                text.contains("Footprint 'F' guid 'not-a-guid' is not a GUID"),
                "{text}"
            );

            let result = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{
                    "name": "F", "pads": [pad],
                    "component_bodies": [
                        { "overall_height": 1.0 },
                        { "overall_height": 1.0, "layer": "Nowhere" },
                    ],
                }],
            }));
            assert!(result.is_error);
            let text = get_result_text(&result);
            assert!(
                text.contains("Component body has invalid layer 'Nowhere'"),
                "{text}"
            );
            assert!(
                text.contains("Failed to parse component body at index 1"),
                "{text}"
            );
            assert!(!path.exists(), "nothing written");
        }

        /// A misspelled key on any `PcbLib` object is refused, not ignored —
        /// an ignored typo is a pad of the wrong shape or a track on the
        /// wrong layer, found in Altium.
        #[test]
        fn write_pcblib_refuses_a_typo_on_every_object_kind() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let out = dir.path().join("Typo.PcbLib");
            let pad = json!({ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 });
            let cases: Vec<(&str, serde_json::Value)> = vec![
                (
                    "footprint",
                    json!({ "name": "T", "pads": [pad], "descripton": "x" }),
                ),
                (
                    "pad",
                    json!({ "name": "T", "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0, "shpae": "round" }] }),
                ),
                (
                    "track",
                    json!({ "name": "T", "pads": [pad], "tracks": [{ "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0, "width": 0.2, "layr": "Top Overlay" }] }),
                ),
                (
                    "arc",
                    json!({ "name": "T", "pads": [pad], "arcs": [{ "x": 0.0, "y": 0.0, "radius": 1.0, "start_angle": 0.0, "end_angle": 90.0, "width": 0.2, "layre": "Top Overlay" }] }),
                ),
                (
                    "via",
                    json!({ "name": "T", "pads": [pad], "vias": [{ "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3, "hole_sixe": 0.3 }] }),
                ),
                (
                    "fill",
                    json!({ "name": "T", "pads": [pad], "fills": [{ "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0, "rotaton": 0.0 }] }),
                ),
                (
                    "region",
                    json!({ "name": "T", "pads": [pad], "regions": [{ "layer": "Top Overlay", "vertices": [{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 }, { "x": 1.0, "y": 1.0 }], "knid": "copper" }] }),
                ),
                (
                    "text",
                    json!({ "name": "T", "pads": [pad], "text": [{ "x": 0.0, "y": 0.0, "text": "hi", "height": 1.0, "hieght": 1.0 }] }),
                ),
                (
                    "component_body",
                    json!({ "name": "T", "pads": [pad], "component_bodies": [{ "overall_height": 1.0, "overal_height": 1.0 }] }),
                ),
                (
                    "step_model",
                    json!({ "name": "T", "pads": [pad], "step_model": { "filepath": "x.step", "embed": false, "rotaton": 0.0 } }),
                ),
                (
                    "model_3d",
                    json!({ "name": "T", "pads": [pad], "model_3d": { "filepath": "x.step", "z_offest": 0.0 } }),
                ),
            ];
            for (kind, footprint) in cases {
                let result = server.call_write_pcblib(&json!({
                    "filepath": out.to_string_lossy(),
                    "footprints": [footprint],
                }));
                let text = get_result_text(&result);
                assert!(result.is_error, "{kind}: a typo was accepted: {text}");
                assert!(text.contains("Unknown field"), "{kind}: {text}");
            }
        }

        /// Per-layer sizes and offsets are accepted in the documented
        /// `{width, height}` / `{x, y}` spelling and as bare pairs, and come
        /// back in the documented spelling (to the format's 0.0001 mil grid).
        #[test]
        fn write_pcblib_accepts_stack_geometry_in_both_spellings() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let close = |v: &serde_json::Value, expected: f64| {
                (v.as_f64().unwrap() - expected).abs() < 1e-5
            };
            let mut sizes_obj = vec![json!({ "width": 1.0, "height": 2.0 }); 32];
            sizes_obj[5] = json!({ "width": 0.5, "height": 0.6 });
            let mut sizes_pair = vec![json!([1.0, 2.0]); 32];
            sizes_pair[5] = json!([0.5, 0.6]);
            let mut offsets_obj = vec![json!({ "x": 0.0, "y": 0.0 }); 32];
            offsets_obj[7] = json!({ "x": -0.1, "y": -0.2 });
            let mut offsets_pair = vec![json!([0.0, 0.0]); 32];
            offsets_pair[7] = json!([-0.1, -0.2]);
            let mut shapes = vec![json!("round"); 32];
            shapes[5] = json!("rectangular");
            shapes[6] = json!("octagonal");
            shapes[8] = json!("rounded_rectangle");
            for (i, (sizes, offsets)) in [(sizes_obj, offsets_obj), (sizes_pair, offsets_pair)]
                .into_iter()
                .enumerate()
            {
                let out = dir.path().join(format!("Stack{i}.PcbLib"));
                let written = server.call_write_pcblib(&json!({
                    "filepath": out.to_string_lossy(),
                    "footprints": [{ "name": "S", "pads": [{
                        "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 2.0,
                        "layer": "Multi-Layer", "hole_size": 0.3,
                        "stack_mode": "full_stack",
                        "per_layer_sizes": sizes, "per_layer_offsets": offsets,
                        "per_layer_shapes": shapes,
                    }] }],
                }));
                assert!(!written.is_error, "{}", get_result_text(&written));
                let back = server.call_read_pcblib(&json!({ "filepath": out.to_string_lossy() }));
                let pad = parse_result_json(&back)["footprints"][0]["pads"][0].clone();
                assert_eq!(pad["stack_mode"], "full_stack");
                let size = &pad["per_layer_sizes"][5];
                assert!(
                    close(&size["width"], 0.5) && close(&size["height"], 0.6),
                    "{pad}"
                );
                let offset = &pad["per_layer_offsets"][7];
                assert!(
                    close(&offset["x"], -0.1) && close(&offset["y"], -0.2),
                    "{pad}"
                );
                assert_eq!(pad["per_layer_shapes"][0], "round", "{pad}");
                assert_eq!(pad["per_layer_shapes"][5], "rectangle", "{pad}");
                assert_eq!(pad["per_layer_shapes"][6], "octagonal", "{pad}");
                assert_eq!(pad["per_layer_shapes"][8], "rounded_rectangle", "{pad}");
            }
        }

        /// A body outline is accepted as `{x, y}` objects (documented) or
        /// bare pairs, and comes back as objects.
        #[test]
        fn write_pcblib_accepts_a_body_outline_in_both_spellings() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            for (i, outline) in [
                json!([{ "x": -1.5, "y": -0.5 }, { "x": 1.5, "y": -0.5 }, { "x": 1.5, "y": 0.5 }, { "x": -1.5, "y": 0.5 }]),
                json!([[-1.5, -0.5], [1.5, -0.5], [1.5, 0.5], [-1.5, 0.5]]),
            ]
            .into_iter()
            .enumerate()
            {
                let out = dir.path().join(format!("Body{i}.PcbLib"));
                let written = server.call_write_pcblib(&json!({
                    "filepath": out.to_string_lossy(),
                    "footprints": [{
                        "name": "B",
                        "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                        "component_bodies": [{ "overall_height": 1.2, "outline": outline }],
                    }],
                }));
                assert!(!written.is_error, "{}", get_result_text(&written));
                let back = server.call_read_pcblib(&json!({ "filepath": out.to_string_lossy() }));
                let body = parse_result_json(&back)["footprints"][0]["component_bodies"][0].clone();
                let outline = body["outline"].as_array().unwrap();
                assert_eq!(outline.len(), 4, "{body}");
                let corners = [(-1.5, -0.5), (1.5, -0.5), (1.5, 0.5), (-1.5, 0.5)];
                for (vertex, (x, y)) in outline.iter().zip(corners) {
                    assert!((vertex["x"].as_f64().unwrap() - x).abs() < 1e-5, "{body}");
                    assert!((vertex["y"].as_f64().unwrap() - y).abs() < 1e-5, "{body}");
                }
            }
        }

        /// Polygon and polyline points are accepted as `{x, y}` objects
        /// (documented) or bare pairs, and come back as objects.
        #[test]
        fn write_schlib_accepts_points_in_both_spellings() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            for (i, points) in [
                json!([{ "x": -10, "y": 0 }, { "x": 0, "y": 5 }, { "x": 10, "y": 0 }]),
                json!([[-10, 0], [0, 5], [10, 0]]),
            ]
            .into_iter()
            .enumerate()
            {
                let out = dir.path().join(format!("Points{i}.SchLib"));
                let written = server.call_write_schlib(&json!({
                    "filepath": out.to_string_lossy(),
                    "symbols": [{
                        "name": "P",
                        "polygons": [{ "points": points }],
                        "polylines": [{ "points": points }],
                    }],
                }));
                assert!(!written.is_error, "{}", get_result_text(&written));
                let back = server.call_read_schlib(&json!({ "filepath": out.to_string_lossy() }));
                let symbol = parse_result_json(&back)["symbols"][0].clone();
                let expected = json!([{ "x": -10.0, "y": 0.0 }, { "x": 0.0, "y": 5.0 }, { "x": 10.0, "y": 0.0 }]);
                assert_eq!(symbol["polygons"][0]["points"], expected, "{symbol}");
                assert_eq!(symbol["polylines"][0]["points"], expected, "{symbol}");
            }
        }

        /// A footprint link echoed back from a read keeps its record identity
        /// and current flag, so a read-modify-write re-emits the same
        /// `RECORD=45` rather than a freshly numbered one.
        #[test]
        fn write_schlib_replays_a_footprint_link_identity() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let out = dir.path().join("Link.SchLib");
            let written = server.call_write_schlib(&json!({
                "filepath": out.to_string_lossy(),
                "symbols": [{
                    "name": "L",
                    "footprints": [
                        { "name": "SOT-23", "description": "d", "library_path": "Lib.PcbLib", "unique_id": "ABCDEFGH", "is_current": true },
                        { "name": "SOT-23-ALT", "uniqe_id": "ABCDEFGH" },
                    ],
                }],
            }));
            assert!(written.is_error, "{}", get_result_text(&written));
            assert!(get_result_text(&written).contains("Unknown field 'uniqe_id'"));

            let written = server.call_write_schlib(&json!({
                "filepath": out.to_string_lossy(),
                "symbols": [{
                    "name": "L",
                    "footprints": [
                        { "name": "SOT-23", "description": "d", "library_path": "Lib.PcbLib", "unique_id": "ABCDEFGH", "is_current": true },
                    ],
                }],
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));
            let back = server.call_read_schlib(&json!({ "filepath": out.to_string_lossy() }));
            let link = parse_result_json(&back)["symbols"][0]["footprints"][0].clone();
            assert_eq!(link["unique_id"], "ABCDEFGH", "{link}");
            assert_eq!(link["is_current"], true, "{link}");
            assert_eq!(link["library_path"], "Lib.PcbLib", "{link}");
        }

        /// A stream this crate does not read (a newer Altium's
        /// `PinFunctionData`) crosses the JSON boundary as `read_schlib`
        /// emits it and is written back; a malformed carrier is no streams,
        /// not an error, like the other carriers.
        #[test]
        fn write_schlib_carries_a_symbol_stream_it_does_not_read() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let out = dir.path().join("Extra.SchLib");
            let written = server.call_write_schlib(&json!({
                "filepath": out.to_string_lossy(),
                "symbols": [{
                    "name": "X",
                    "extra_streams": [["PinFunctionData", "AQD/fg=="]],
                }],
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));
            let back = server.call_read_schlib(&json!({ "filepath": out.to_string_lossy() }));
            let symbol = parse_result_json(&back)["symbols"][0].clone();
            assert_eq!(
                symbol["extra_streams"],
                json!([["PinFunctionData", "AQD/fg=="]]),
                "{symbol}"
            );

            let written = server.call_write_schlib(&json!({
                "filepath": out.to_string_lossy(),
                "symbols": [{ "name": "X", "extra_streams": [["PinFunctionData", "not base64!"]] }],
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));
            let back = server.call_read_schlib(&json!({ "filepath": out.to_string_lossy() }));
            let symbol = parse_result_json(&back)["symbols"][0].clone();
            assert!(symbol.get("extra_streams").is_none(), "{symbol}");
        }

        /// Two names differing only in case are one storage to the OLE
        /// directory: within one request that is a duplicate, and against an
        /// existing library it is a clash, named after the spelling on file.
        #[test]
        fn write_tools_treat_a_case_variant_as_the_same_name() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let out = dir.path().join("Case.PcbLib");
            let pad = json!({ "designator": "1", "x": 0.0, "y": 0.0, "width": 0.6, "height": 0.5 });
            let written = server.call_write_pcblib(&json!({
                "filepath": out.to_string_lossy(),
                "footprints": [
                    { "name": "RES_0402", "pads": [pad] },
                    { "name": "res_0402", "pads": [pad] },
                ],
            }));
            assert!(written.is_error);
            assert!(get_result_text(&written).contains("Duplicate footprint name: 'res_0402'"));

            let written = server.call_write_pcblib(&json!({
                "filepath": out.to_string_lossy(),
                "footprints": [{ "name": "RES_0402", "pads": [pad] }],
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));
            let appended = server.call_write_pcblib(&json!({
                "filepath": out.to_string_lossy(),
                "append": true,
                "footprints": [{ "name": "res_0402", "pads": [pad] }],
            }));
            assert!(appended.is_error);
            assert!(
                get_result_text(&appended).contains(
                    "Footprint 'res_0402' already exists in the library as 'RES_0402' (component names are case-insensitive)"
                ),
                "{}",
                get_result_text(&appended)
            );

            let out = dir.path().join("Case.SchLib");
            let written = server.call_write_schlib(&json!({
                "filepath": out.to_string_lossy(),
                "symbols": [{ "name": "LM358" }, { "name": "lm358" }],
            }));
            assert!(written.is_error);
            assert!(get_result_text(&written).contains("Duplicate symbol name: 'lm358'"));
            let written = server.call_write_schlib(&json!({
                "filepath": out.to_string_lossy(),
                "symbols": [{ "name": "LM358" }],
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));
            let appended = server.call_write_schlib(&json!({
                "filepath": out.to_string_lossy(),
                "append": true,
                "symbols": [{ "name": "lm358" }],
            }));
            assert!(appended.is_error);
            assert!(
                get_result_text(&appended).contains("as 'LM358'"),
                "{}",
                get_result_text(&appended)
            );
        }

        /// The in-place twin: every golden footprint read through
        /// `read_pcblib` and handed back to `update_component` as its own
        /// replacement leaves the library byte-for-byte as the library-level
        /// save has it — the update path parses with its own hands, so it is
        /// held to the same bar as `write_pcblib`.
        #[test]
        fn update_component_replays_every_golden_footprint_byte_for_byte() {
            use crate::altium::PcbLib;

            let dir = test_temp_dir();
            let samples = std::path::Path::new("scripts/samples")
                .canonicalize()
                .unwrap();
            let server =
                crate::mcp::server::McpServer::new(vec![dir.path().to_path_buf(), samples.clone()]);
            let src = samples.join("footprints.PcbLib");

            let baseline = dir.path().join("Baseline.PcbLib");
            PcbLib::open(&src).unwrap().save(&baseline).unwrap();
            let work = dir.path().join("Work.PcbLib");
            std::fs::copy(&src, &work).unwrap();

            let read = server.call_read_pcblib(&json!({ "filepath": src.to_string_lossy() }));
            assert!(!read.is_error, "{}", get_result_text(&read));
            let footprints = parse_result_json(&read)["footprints"].clone();
            for (i, fp) in footprints.as_array().unwrap().iter().enumerate() {
                // Alternate between the two read shapes: read_pcblib builds
                // its JSON by hand, get_component serialises the struct.
                let fp = if i % 2 == 0 {
                    fp.clone()
                } else {
                    let got = server.call_get_component(&json!({
                        "filepath": src.to_string_lossy(),
                        "component_name": fp["name"],
                    }));
                    assert!(!got.is_error, "{}", get_result_text(&got));
                    parse_result_json(&got)["component"].clone()
                };
                let updated = server.call_update_component(&json!({
                    "filepath": work.to_string_lossy(),
                    "component_name": fp["name"],
                    "footprint": fp,
                }));
                assert!(
                    !updated.is_error,
                    "{}: {}",
                    fp["name"],
                    get_result_text(&updated)
                );
            }

            let (expected, ours) = (component_streams(&baseline), component_streams(&work));
            assert_eq!(
                expected.keys().collect::<Vec<_>>(),
                ours.keys().collect::<Vec<_>>(),
                "footprint storages differ"
            );
            for (name, e) in &expected {
                let o = &ours[name];
                for stream in ["Data", "WideStrings", "UniqueIDPrimitiveInformation/Data"] {
                    assert_same_stream(&format!("{name}/{stream}"), e.get(stream), o.get(stream));
                }
            }
        }

        /// `update_component` parses with `write_pcblib`'s parser: a typo on
        /// any object is refused, and the 3D-model spellings are honoured.
        #[test]
        fn update_component_parses_exactly_what_write_pcblib_does() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let lib = dir.path().join("Upd.PcbLib");
            let pad = json!({ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 });
            let written = server.call_write_pcblib(&json!({
                "filepath": lib.to_string_lossy(),
                "footprints": [{ "name": "U", "pads": [pad] }],
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));

            for (kind, footprint) in [
                (
                    "footprint",
                    json!({ "name": "U", "pads": [pad], "descripton": "x" }),
                ),
                (
                    "pad",
                    json!({ "name": "U", "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0, "shpae": "round" }] }),
                ),
                (
                    "track",
                    json!({ "name": "U", "pads": [pad], "tracks": [{ "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0, "width": 0.2, "layr": "Top Overlay" }] }),
                ),
                (
                    "body",
                    json!({ "name": "U", "pads": [pad], "component_bodies": [{ "overal_height": 1.0 }] }),
                ),
                (
                    "step_model",
                    json!({ "name": "U", "pads": [pad], "step_model": { "filepath": "x.step", "embed": false, "rotaton": 0.0 } }),
                ),
            ] {
                let result = server.call_update_component(&json!({
                    "filepath": lib.to_string_lossy(),
                    "component_name": "U",
                    "footprint": footprint,
                }));
                let text = get_result_text(&result);
                assert!(result.is_error, "{kind}: a typo was accepted: {text}");
                assert!(text.contains("Unknown field"), "{kind}: {text}");
            }

            // An external STEP reference lands as a component body, as it
            // does on a write.
            let result = server.call_update_component(&json!({
                "filepath": lib.to_string_lossy(),
                "component_name": "U",
                "footprint": { "name": "U", "pads": [pad], "step_model": { "filepath": "models/U.step", "embed": false, "rotation": 90.0 } },
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let back = server.call_read_pcblib(&json!({ "filepath": lib.to_string_lossy() }));
            let fp = parse_result_json(&back)["footprints"][0].clone();
            assert_eq!(
                fp["component_bodies"][0]["model_name"], "models/U.step",
                "{fp}"
            );
            assert_eq!(fp["component_bodies"][0]["embedded"], false, "{fp}");
        }

        /// Text beyond ASCII survives a write, a read and — the case that
        /// corrupted it a character per save — every further open/save.
        #[test]
        fn write_pcblib_keeps_non_ascii_text_through_repeated_saves() {
            use crate::altium::PcbLib;

            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let lib = dir.path().join("Wide.PcbLib");
            let texts = ["10µF", "Ω ±5%", "日本語", "€ 1,50"];
            let written = server.call_write_pcblib(&json!({
                "filepath": lib.to_string_lossy(),
                "footprints": [{
                    "name": "W",
                    "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                    "text": texts.iter().enumerate().map(|(i, t)| json!({
                        "x": 0.0, "y": f64::from(u8::try_from(i).unwrap()) * 2.0, "text": t, "height": 1.0,
                    })).collect::<Vec<_>>(),
                }],
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));

            let read_texts = |path: &std::path::Path| -> Vec<String> {
                let back = server.call_read_pcblib(&json!({ "filepath": path.to_string_lossy() }));
                parse_result_json(&back)["footprints"][0]["text"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|t| !t["text"].as_str().unwrap().starts_with('.'))
                    .map(|t| t["text"].as_str().unwrap().to_string())
                    .collect()
            };
            assert_eq!(read_texts(&lib), texts);

            let first = std::fs::read(&lib).unwrap();
            for _ in 0..3 {
                let mut opened = PcbLib::open(&lib).unwrap();
                opened.save(&lib).unwrap();
            }
            assert_eq!(read_texts(&lib), texts);
            let streams_after = component_streams(&lib);
            assert_eq!(
                component_streams(&dir.path().join("Wide.PcbLib"))["W"]["WideStrings"],
                streams_after["W"]["WideStrings"]
            );
            let wide = String::from_utf8_lossy(&streams_after["W"]["WideStrings"]).into_owned();
            assert!(wide.contains("|ENCODEDTEXT0=49,48,181,70|"), "{wide}");
            assert!(wide.contains("|ENCODEDTEXT1=937,32,177,53,37|"), "{wide}");
            assert!(wide.contains("|ENCODEDTEXT2=26085,26412,35486|"), "{wide}");
            assert!(wide.contains("|ENCODEDTEXT3=8364,32,49,44,53,48"), "{wide}");
            assert_eq!(
                std::fs::read(&lib).unwrap().len(),
                first.len(),
                "a save must not grow the file"
            );
        }

        /// The `SchLib` in-place twin: every golden symbol read through
        /// `read_schlib` and handed back to `update_component` as its own
        /// replacement — one save per symbol, each re-reading the last — ends
        /// identical to the library-level save (IDs the golden lacks masked).
        #[test]
        fn update_component_replays_every_golden_symbol_byte_for_byte() {
            use crate::altium::SchLib;

            let dir = test_temp_dir();
            let samples = std::path::Path::new("scripts/samples")
                .canonicalize()
                .unwrap();
            let server =
                crate::mcp::server::McpServer::new(vec![dir.path().to_path_buf(), samples.clone()]);
            let src = samples.join("symbols.SchLib");

            let baseline = dir.path().join("Baseline.SchLib");
            SchLib::open(&src).unwrap().save(&baseline).unwrap();
            let work = dir.path().join("Work.SchLib");
            std::fs::copy(&src, &work).unwrap();

            let read = server.call_read_schlib(&json!({ "filepath": src.to_string_lossy() }));
            assert!(!read.is_error, "{}", get_result_text(&read));
            let symbols = parse_result_json(&read)["symbols"].clone();
            // Every third symbol is enough saves to surface per-save drift
            // on all of them while keeping the test under a quarter minute.
            for (i, symbol) in symbols.as_array().unwrap().iter().step_by(3).enumerate() {
                let symbol = if i % 2 == 0 {
                    symbol.clone()
                } else {
                    let got = server.call_get_component(&json!({
                        "filepath": src.to_string_lossy(),
                        "component_name": symbol["name"],
                    }));
                    assert!(!got.is_error, "{}", get_result_text(&got));
                    parse_result_json(&got)["component"].clone()
                };
                let updated = server.call_update_component(&json!({
                    "filepath": work.to_string_lossy(),
                    "component_name": symbol["name"],
                    "symbol": symbol,
                }));
                assert!(
                    !updated.is_error,
                    "{}: {}",
                    symbol["name"],
                    get_result_text(&updated)
                );
            }

            let golden_ids: std::collections::HashSet<Vec<u8>> = component_streams(&src)
                .values()
                .flat_map(|streams| streams.values())
                .flat_map(|bytes| unique_ids(bytes))
                .collect();
            let (expected, ours) = (component_streams(&baseline), component_streams(&work));
            assert_eq!(
                expected.keys().collect::<Vec<_>>(),
                ours.keys().collect::<Vec<_>>(),
                "symbol storages differ"
            );
            for (name, e) in &expected {
                let o = &ours[name];
                assert_eq!(
                    e.keys().collect::<Vec<_>>(),
                    o.keys().collect::<Vec<_>>(),
                    "{name}: streams differ"
                );
                for (stream, a) in e {
                    let a = mask_generated_ids(a, &golden_ids);
                    let b = o.get(stream).map(|b| mask_generated_ids(b, &golden_ids));
                    assert_same_stream(&format!("{name}/{stream}"), Some(&a), b.as_ref());
                }
            }
        }

        /// `update_component` parses a symbol with `write_schlib`'s parser: a
        /// typo on any object is refused, and the designator is derived the
        /// same way when the replacement carries none.
        #[test]
        fn update_component_parses_exactly_what_write_schlib_does() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let lib = dir.path().join("Upd.SchLib");
            let written = server.call_write_schlib(&json!({
                "filepath": lib.to_string_lossy(),
                "symbols": [{ "name": "S", "designator": "U?" }],
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));

            for (kind, symbol) in [
                ("symbol", json!({ "name": "S", "descripton": "x" })),
                (
                    "pin",
                    json!({ "name": "S", "pins": [{ "name": "A", "designator": "1", "x": 0, "y": 0, "lenght": 10 }] }),
                ),
                (
                    "rectangle",
                    json!({ "name": "S", "rectangles": [{ "x1": 0, "y1": 0, "x2": 10, "y2": 10, "fillcolor": 1 }] }),
                ),
                (
                    "polygon",
                    json!({ "name": "S", "polygons": [{ "points": [{ "x": 0, "y": 0 }, { "x": 5, "y": 0 }, { "x": 0, "y": 5 }], "filed": true }] }),
                ),
                (
                    "parameter",
                    json!({ "name": "S", "parameters": [{ "name": "Value", "value": "1k", "hiden": true }] }),
                ),
                (
                    "footprint",
                    json!({ "name": "S", "footprints": [{ "name": "R0402", "library_pat": "x.PcbLib" }] }),
                ),
            ] {
                let result = server.call_update_component(&json!({
                    "filepath": lib.to_string_lossy(),
                    "component_name": "S",
                    "symbol": symbol,
                }));
                let text = get_result_text(&result);
                assert!(result.is_error, "{kind}: a typo was accepted: {text}");
                assert!(text.contains("Unknown field"), "{kind}: {text}");
            }

            let result = server.call_update_component(&json!({
                "filepath": lib.to_string_lossy(),
                "component_name": "S",
                "symbol": { "name": "S", "component_type": "resistor" },
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let back = server.call_read_schlib(&json!({ "filepath": lib.to_string_lossy() }));
            assert_eq!(parse_result_json(&back)["symbols"][0]["designator"], "R?");
        }

        /// A record its parser cannot build is refused and named, like a
        /// malformed pad — never silently left out of the file.
        #[test]
        fn malformed_records_are_refused_not_dropped() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let pad = json!({ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 });
            let pcb = dir.path().join("Bad.PcbLib");
            for (kind, footprint) in [
                (
                    "region",
                    json!({ "name": "B", "pads": [pad], "regions": [{ "layer": "Top Overlay" }] }),
                ),
                (
                    "text",
                    json!({ "name": "B", "pads": [pad], "text": [{ "x": 0.0, "y": 0.0 }] }),
                ),
            ] {
                let r = server.call_write_pcblib(&json!({
                    "filepath": pcb.to_string_lossy(),
                    "footprints": [footprint],
                }));
                let text = get_result_text(&r);
                assert!(
                    r.is_error,
                    "{kind}: a malformed record was accepted: {text}"
                );
                assert!(
                    text.contains(&format!("Failed to parse {kind} at index 0")),
                    "{text}"
                );
                assert!(text.contains("Malformed"), "{text}");
            }

            let sch = dir.path().join("Bad.SchLib");
            let empty = json!([{}]);
            for (kind, symbol) in [
                ("pin", json!({ "name": "B", "pins": [{ "name": "A" }] })),
                ("rectangle", json!({ "name": "B", "rectangles": empty })),
                ("round_rect", json!({ "name": "B", "round_rects": empty })),
                ("line", json!({ "name": "B", "lines": empty })),
                ("polyline", json!({ "name": "B", "polylines": empty })),
                (
                    "polygon",
                    json!({ "name": "B", "polygons": [{ "points": [{ "x": 0, "y": 0 }] }] }),
                ),
                ("arc", json!({ "name": "B", "arcs": empty })),
                ("pie", json!({ "name": "B", "pies": empty })),
                ("image", json!({ "name": "B", "images": empty })),
                ("text_frame", json!({ "name": "B", "text_frames": empty })),
                ("bezier", json!({ "name": "B", "beziers": empty })),
                (
                    "elliptical_arc",
                    json!({ "name": "B", "elliptical_arcs": empty }),
                ),
                ("ellipse", json!({ "name": "B", "ellipses": empty })),
                ("label", json!({ "name": "B", "labels": empty })),
                ("IEEE symbol", json!({ "name": "B", "ieee_symbols": empty })),
                (
                    "parameter",
                    json!({ "name": "B", "parameters": [{ "value": "1k" }] }),
                ),
                (
                    "footprint link",
                    json!({ "name": "B", "footprints": [{ "description": "no name" }] }),
                ),
            ] {
                let r = server.call_write_schlib(&json!({
                    "filepath": sch.to_string_lossy(),
                    "symbols": [symbol],
                }));
                let text = get_result_text(&r);
                assert!(
                    r.is_error,
                    "{kind}: a malformed record was accepted: {text}"
                );
                assert!(
                    text.contains(&format!("Failed to parse {kind} at index 0")),
                    "{text}"
                );
            }
            assert!(!pcb.exists() && !sch.exists(), "nothing was written");
        }

        /// From scratch the designator is still added by default, and
        /// `auto_designator: false` switches it off.
        #[test]
        fn write_pcblib_auto_designator_is_opt_out_for_new_footprints() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let pads =
                json!([{ "designator": "1", "x": 0.0, "y": 0.0, "width": 0.6, "height": 0.5 }]);
            for (i, (auto, expected_texts)) in [(None, 1), (Some(true), 1), (Some(false), 0)]
                .into_iter()
                .enumerate()
            {
                let path = dir.path().join(format!("Auto{i}.PcbLib"));
                let mut args = json!({
                    "filepath": path.to_string_lossy(),
                    "footprints": [{ "name": "FP", "pads": pads }],
                });
                if let Some(flag) = auto {
                    args["auto_designator"] = json!(flag);
                }
                let written = server.call_write_pcblib(&args);
                assert!(!written.is_error, "{}", get_result_text(&written));
                let back = server.call_read_pcblib(&json!({ "filepath": path.to_string_lossy() }));
                let texts = parse_result_json(&back)["footprints"][0]["text"]
                    .as_array()
                    .map_or(0, Vec::len);
                assert_eq!(texts, expected_texts, "auto_designator={auto:?}");
            }
        }

        /// Neither write tool requires the primitives its schema once
        /// claimed: a footprint with no pads (a logo, an outline-only
        /// mechanical part) and a symbol with no pins (a power port, a logo)
        /// are written and read back whole, so `tools/list` must not mark
        /// `pads` or `pins` required.
        #[test]
        fn a_footprint_without_pads_and_a_symbol_without_pins_are_written() {
            let schemas: std::collections::HashMap<String, serde_json::Value> =
                crate::mcp::server::McpServer::get_tool_definitions()
                    .into_iter()
                    .map(|t| (t.name, t.input_schema))
                    .collect();
            assert_eq!(
                schemas["write_pcblib"]["properties"]["footprints"]["items"]["required"],
                json!(["name"])
            );
            assert_eq!(
                schemas["write_schlib"]["properties"]["symbols"]["items"]["required"],
                json!(["name"])
            );

            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let pcb = dir.path().join("Logo.PcbLib");
            let result = server.call_write_pcblib(&json!({
                "filepath": pcb.to_string_lossy(),
                "footprints": [{
                    "name": "LOGO",
                    "tracks": [{ "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.1, "layer": "Top Overlay" }],
                }],
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let read = server.call_read_pcblib(&json!({ "filepath": pcb.to_string_lossy() }));
            assert!(!read.is_error, "{}", get_result_text(&read));
            let footprint = &parse_result_json(&read)["footprints"][0];
            assert_eq!(footprint["name"], "LOGO");
            assert_eq!(footprint["pads"].as_array().map_or(0, Vec::len), 0);
            assert_eq!(footprint["tracks"].as_array().unwrap().len(), 1);

            let sch = dir.path().join("Logo.SchLib");
            let result = server.call_write_schlib(&json!({
                "filepath": sch.to_string_lossy(),
                "symbols": [{
                    "name": "LOGO",
                    "rectangles": [{ "x1": 0, "y1": 0, "x2": 10, "y2": 10 }],
                }],
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let read = server.call_read_schlib(&json!({ "filepath": sch.to_string_lossy() }));
            assert!(!read.is_error, "{}", get_result_text(&read));
            let symbol = &parse_result_json(&read)["symbols"][0];
            assert_eq!(symbol["name"], "LOGO");
            assert_eq!(symbol["pins"].as_array().map_or(0, Vec::len), 0);
            assert_eq!(symbol["rectangles"].as_array().unwrap().len(), 1);
        }

        /// `read_schlib` → `write_schlib` replays a symbol's interleaved
        /// `primitive_order`, so a read-modify-write keeps the source's
        /// record order instead of regrouping records by kind.
        #[test]
        fn write_schlib_replays_the_symbol_primitive_order() {
            use crate::altium::schlib::{Line, Pin, PinOrientation, Rectangle, SchLib};

            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            // Line between the pin and the rectangle: a non-canonical
            // interleaving no grouped rewrite would reproduce. The part/mode
            // scalars carry non-defaults so their emission is observable too.
            let mut symbol = crate::altium::schlib::Symbol::new("REPLAY");
            symbol.part_count = 2;
            symbol.display_mode_count = 2;
            symbol.current_part_id = 2;
            symbol.part_id_locked = true;
            symbol.add_pin(Pin::new("IN", "1", 0, 0, 10, PinOrientation::Left));
            symbol.add_line(Line::new(0.0, 0.0, 10.0, 10.0));
            symbol.add_rectangle(Rectangle::new(0.0, 0.0, 20.0, 20.0));

            let mut lib = SchLib::new();
            lib.add(symbol);
            let src = dir.path().join("Src.SchLib");
            lib.save(&src).unwrap();

            let first_read = server.call_read_schlib(&json!({
                "filepath": src.to_string_lossy(),
            }));
            assert!(!first_read.is_error, "{}", get_result_text(&first_read));
            let symbols = parse_result_json(&first_read)["symbols"].clone();
            assert_eq!(
                symbols[0]["primitive_order"],
                json!(["pin", "line", "rectangle"]),
                "authored interleaving survives the first read"
            );

            let dst = dir.path().join("Dst.SchLib");
            let write = server.call_write_schlib(&json!({
                "filepath": dst.to_string_lossy(),
                "symbols": symbols,
            }));
            assert!(!write.is_error, "{}", get_result_text(&write));

            let second_read = server.call_read_schlib(&json!({
                "filepath": dst.to_string_lossy(),
            }));
            assert!(!second_read.is_error, "{}", get_result_text(&second_read));
            let symbol_after = parse_result_json(&second_read)["symbols"][0].clone();
            assert_eq!(
                symbol_after["primitive_order"],
                json!(["pin", "line", "rectangle"]),
                "record order survives the tool-layer round trip"
            );
            // The part/mode scalars ride the same read → write → read loop.
            assert_eq!(symbol_after["part_count"], 2);
            assert_eq!(symbol_after["display_mode_count"], 2);
            assert_eq!(symbol_after["current_part_id"], 2);
            assert_eq!(symbol_after["part_id_locked"], true);
        }

        /// A JSON-authored symbol has no authoring order to replay, so it
        /// takes `SchPrimitiveKind::WRITE_ORDER` — body graphics first. The
        /// parse order (pins, then rectangles) must not leak out as one: the
        /// records render in stream order, so a solid-filled body emitted
        /// after the pins paints over the pin names inside it.
        #[test]
        fn a_json_authored_symbol_writes_its_filled_body_before_the_pins() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let sch = dir.path().join("Body.SchLib");
            let write = server.call_write_schlib(&json!({
                "filepath": sch.to_string_lossy(),
                "symbols": [{
                    "name": "BODY",
                    "pins": [
                        { "designator": "1", "name": "VIN", "x": -50, "y": 20,
                          "length": 30, "orientation": "left" },
                        { "designator": "2", "name": "GND", "x": 50, "y": 20,
                          "length": 30, "orientation": "right" },
                    ],
                    "rectangles": [
                        { "x1": -50, "y1": 40, "x2": 50, "y2": -40, "filled": true },
                    ],
                }],
            }));
            assert!(!write.is_error, "{}", get_result_text(&write));

            let read = server.call_read_schlib(&json!({
                "filepath": sch.to_string_lossy(),
            }));
            assert!(!read.is_error, "{}", get_result_text(&read));
            let symbol = parse_result_json(&read)["symbols"][0].clone();
            assert_eq!(
                symbol["primitive_order"],
                json!(["rectangle", "pin", "pin"]),
                "the filled body must precede the pins it encloses"
            );
            assert_eq!(
                symbol["rectangles"][0]["filled"], true,
                "the body stays filled — transparency is not the fix"
            );
        }

        /// A malformed `primitive_order` is ignored with the default grouped
        /// order rather than failing the write — it is advisory, exactly as
        /// on the struct.
        #[test]
        fn invalid_primitive_order_is_ignored_not_fatal() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let pcb = dir.path().join("Bad.PcbLib");
            let write = server.call_write_pcblib(&json!({
                "filepath": pcb.to_string_lossy(),
                "footprints": [{
                    "name": "FP",
                    "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 0.6, "height": 0.5 }],
                    "primitive_order": ["not_a_kind"],
                }],
            }));
            assert!(!write.is_error, "{}", get_result_text(&write));

            let sch = dir.path().join("Bad.SchLib");
            let write = server.call_write_schlib(&json!({
                "filepath": sch.to_string_lossy(),
                "symbols": [{
                    "name": "SYM",
                    "pins": [{ "designator": "1", "name": "IN", "x": 0, "y": 0,
                               "orientation": "left" }],
                    "primitive_order": 42,
                }],
            }));
            assert!(!write.is_error, "{}", get_result_text(&write));
        }
    }

    // ==================== extract_style ====================

    mod extract_style {
        use crate::altium::pcblib::{
            Arc, ComponentBody, Fill, Footprint, Layer, Pad, PcbLib, Track, Via,
        };
        use crate::altium::schlib::{Label, Pin, PinOrientation, Polygon, SchLib, Symbol};
        use crate::mcp::tools::test_support::{
            create_test_schlib, create_test_server, get_result_text, parse_result_json,
            test_temp_dir,
        };
        use serde_json::json;

        #[test]
        fn extract_style_pcblib_reports_track_and_pad_statistics() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            // Two footprints: three 0.2 mm overlay tracks and one 0.4 mm, plus
            // four rectangular pads.
            let mut lib = PcbLib::new();
            let mut fp1 = Footprint::new("A");
            fp1.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
            fp1.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
            fp1.add_track(Track::new(-1.0, -1.0, 1.0, -1.0, 0.2, Layer::TopOverlay));
            fp1.add_track(Track::new(-1.0, 1.0, 1.0, 1.0, 0.2, Layer::TopOverlay));
            lib.add(fp1);
            let mut fp2 = Footprint::new("B");
            fp2.add_pad(Pad::smd("1", -0.8, 0.0, 0.8, 0.8));
            fp2.add_pad(Pad::smd("2", 0.8, 0.0, 0.8, 0.8));
            fp2.add_track(Track::new(-2.0, -2.0, 2.0, -2.0, 0.2, Layer::TopOverlay));
            fp2.add_track(Track::new(-2.0, 2.0, 2.0, 2.0, 0.4, Layer::TopOverlay));
            lib.add(fp2);
            let path = dir.path().join("Style.PcbLib");
            lib.save(&path).unwrap();

            let result = server.call_extract_style(&json!({
                "filepath": path.to_string_lossy(),
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            assert_eq!(parsed["status"], "success");
            assert_eq!(parsed["file_type"], "PcbLib");
            assert_eq!(parsed["footprint_count"], 2);

            let overlay = &parsed["style"]["track_widths_by_layer"]["Top Overlay"];
            assert_eq!(overlay["count"], 4);
            // Widths quantise to 0.01 mm for the most-common statistic.
            assert!((overlay["most_common_mm"].as_f64().unwrap() - 0.2).abs() < 1e-9);
            assert!((overlay["min_mm"].as_f64().unwrap() - 0.2).abs() < 1e-3);
            assert!((overlay["max_mm"].as_f64().unwrap() - 0.4).abs() < 1e-3);

            // `Pad::smd` creates rounded-rectangle pads.
            assert_eq!(parsed["style"]["pad_shapes"]["RoundedRectangle"], 4);
            assert_eq!(parsed["style"]["layers_used"]["Top Overlay"], 4);
            assert_eq!(parsed["style"]["text_heights"], serde_json::Value::Null);
        }

        #[test]
        fn extract_style_schlib_reports_pin_and_line_statistics() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Style.SchLib");
            create_test_schlib(&path);

            let result = server.call_extract_style(&json!({
                "filepath": path.to_string_lossy(),
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            assert_eq!(parsed["status"], "success");
            assert_eq!(parsed["file_type"], "SchLib");
            assert_eq!(parsed["symbol_count"], 2);

            // Four fixture pins, all 10 units long.
            let pins = &parsed["style"]["pin_lengths"];
            assert_eq!(pins["count"], 4);
            assert_eq!(pins["min_units"], 10);
            assert_eq!(pins["max_units"], 10);
            assert_eq!(pins["most_common_units"], 10);

            // One fixture rectangle contributes the only line width.
            assert_eq!(parsed["style"]["line_widths"]["count"], 1);
            assert_eq!(parsed["style"]["rectangles"]["filled_count"], 1);
            assert_eq!(parsed["style"]["rectangles"]["unfilled_count"], 0);
        }

        /// A layer used only by arcs, vias, fills or bodies is a used layer,
        /// and silkscreen arcs report their stroke width like tracks do.
        #[test]
        fn extract_style_pcblib_counts_every_kind_in_layer_usage() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let mut lib = PcbLib::new();
            let mut fp = Footprint::new("ROUND_CAP");
            fp.add_arc(Arc::circle(0.0, 0.0, 2.5, 0.15, Layer::TopOverlay));
            fp.add_via(Via::new(0.0, 0.0, 0.6, 0.3));
            fp.add_fill(Fill::new(-1.0, -1.0, 1.0, 1.0, Layer::TopPaste));
            let mut body = ComponentBody::new("", "body.step");
            body.layer = Layer::Mechanical13;
            fp.add_component_body(body);
            lib.add(fp);
            let path = dir.path().join("Kinds.PcbLib");
            lib.save(&path).unwrap();

            let result = server.call_extract_style(&json!({
                "filepath": path.to_string_lossy(),
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            let layers = &parsed["style"]["layers_used"];
            assert_eq!(layers["Top Overlay"], 1, "{layers}");
            assert_eq!(layers["Multi-Layer"], 1, "{layers}");
            assert_eq!(layers["Top Paste"], 1, "{layers}");
            assert_eq!(layers["Mechanical 13"], 1, "{layers}");
            let arcs = &parsed["style"]["arc_widths_by_layer"]["Top Overlay"];
            assert_eq!(arcs["count"], 1, "{arcs}");
            assert!((arcs["most_common_mm"].as_f64().unwrap() - 0.15).abs() < 1e-9);
            assert!(parsed["style"]["track_widths_by_layer"]
                .as_object()
                .unwrap()
                .is_empty());
        }

        /// A symbol drawn with polygons (an op-amp triangle) has stroke widths
        /// and colours; a pin's colour is a line colour, a label's a text one.
        #[test]
        fn extract_style_schlib_counts_every_kind_in_widths_and_colours() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let mut lib = SchLib::new();
            let mut sym = Symbol::new("OPAMP");
            let mut triangle = Polygon::new(vec![(0.0, 0.0), (0.0, 40.0), (40.0, 20.0)]);
            triangle.line_width = 2;
            triangle.line_color = 0x00_00FF;
            triangle.fill_color = 0xB0_FFFF;
            sym.add_polygon(triangle);
            let mut pin = Pin::new("1", "IN+", -10, 10, 10, PinOrientation::Right);
            pin.colour = 0x80_0000;
            sym.add_pin(pin);
            let mut label = Label::new(0, 50, "OP");
            label.color = 0x00_8000;
            sym.add_label(label);
            lib.add(sym);
            let path = dir.path().join("Kinds.SchLib");
            lib.save(&path).unwrap();

            let result = server.call_extract_style(&json!({
                "filepath": path.to_string_lossy(),
            }));
            assert!(!result.is_error, "{}", get_result_text(&result));
            let parsed = parse_result_json(&result);
            let style = &parsed["style"];
            assert_eq!(style["line_widths"]["count"], 1, "{style}");
            assert_eq!(style["line_widths"]["most_common"], 2, "{style}");
            assert_eq!(style["line_colors"]["#0000FF"], 1, "{style}");
            assert_eq!(style["line_colors"]["#800000"], 1, "{style}");
            assert_eq!(style["fill_colors"]["#B0FFFF"], 1, "{style}");
            assert_eq!(style["text_colors"]["#008000"], 1, "{style}");
            assert_eq!(style["rectangles"]["filled_count"], 0);
        }

        #[test]
        fn extract_style_error_paths() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let result = server.call_extract_style(&json!({}));
            assert!(result.is_error);
            assert_eq!(
                get_result_text(&result),
                "Missing required parameter: filepath"
            );

            // Unknown extension.
            let txt = dir.path().join("x.txt");
            let result = server.call_extract_style(&json!({
                "filepath": txt.to_string_lossy(),
            }));
            assert!(result.is_error);
            assert!(get_result_text(&result).contains("Unsupported file type"));

            // Unreadable library.
            let missing = dir.path().join("Missing.PcbLib");
            let result = server.call_extract_style(&json!({
                "filepath": missing.to_string_lossy(),
            }));
            assert!(result.is_error);
            let parsed = parse_result_json(&result);
            assert_eq!(parsed["status"], "error");
        }
    }

    // ==================== read/write handler error paths ====================

    mod handler_error_paths {
        use crate::mcp::tools::test_support::{
            create_test_pcblib, create_test_server, get_result_text, test_temp_dir,
        };
        use serde_json::json;

        #[test]
        fn read_pcblib_missing_filepath() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let r = server.call_read_pcblib(&json!({}));
            assert!(r.is_error);
            assert_eq!(get_result_text(&r), "Missing required parameter: filepath");
        }

        #[test]
        fn read_pcblib_denied_path_outside_allowed() {
            let allowed = test_temp_dir();
            let outside = test_temp_dir();
            let server = create_test_server(allowed.path());
            // Create a real library so its parent canonicalises — the denial is
            // about the path being outside the allow-list, not a missing file.
            let path = outside.path().join("X.PcbLib");
            create_test_pcblib(&path);
            let r = server.call_read_pcblib(&json!({ "filepath": path.to_string_lossy() }));
            assert!(r.is_error);
            assert!(
                get_result_text(&r).contains("Access denied"),
                "{}",
                get_result_text(&r)
            );
        }

        #[test]
        fn read_pcblib_nonexistent_file_is_error() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Nope.PcbLib");
            let r = server.call_read_pcblib(&json!({ "filepath": path.to_string_lossy() }));
            assert!(r.is_error);
        }

        #[test]
        fn write_pcblib_missing_filepath_then_footprints() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let r = server.call_write_pcblib(&json!({}));
            assert!(r.is_error);
            assert_eq!(get_result_text(&r), "Missing required parameter: filepath");

            let path = dir.path().join("W.PcbLib");
            let r = server.call_write_pcblib(&json!({ "filepath": path.to_string_lossy() }));
            assert!(r.is_error);
            assert_eq!(
                get_result_text(&r),
                "Missing required parameter: footprints"
            );
        }

        #[test]
        fn write_pcblib_denied_path_outside_allowed() {
            let allowed = test_temp_dir();
            let outside = test_temp_dir();
            let server = create_test_server(allowed.path());
            let path = outside.path().join("W.PcbLib");
            let r = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{ "name": "X", "pads": [] }],
                "append": false,
            }));
            assert!(r.is_error);
            assert!(
                get_result_text(&r).contains("Access denied"),
                "{}",
                get_result_text(&r)
            );
        }

        #[test]
        fn read_schlib_missing_filepath() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let r = server.call_read_schlib(&json!({}));
            assert!(r.is_error);
            assert_eq!(get_result_text(&r), "Missing required parameter: filepath");
        }

        #[test]
        fn write_schlib_missing_filepath_then_symbols() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let r = server.call_write_schlib(&json!({}));
            assert!(r.is_error);
            assert_eq!(get_result_text(&r), "Missing required parameter: filepath");

            let path = dir.path().join("W.SchLib");
            let r = server.call_write_schlib(&json!({ "filepath": path.to_string_lossy() }));
            assert!(r.is_error);
            assert_eq!(get_result_text(&r), "Missing required parameter: symbols");
        }

        #[test]
        fn list_components_missing_filepath_and_nonexistent() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let r = server.call_list_components(&json!({}));
            assert!(r.is_error);
            assert_eq!(get_result_text(&r), "Missing required parameter: filepath");

            let path = dir.path().join("Nope.PcbLib");
            let r = server.call_list_components(&json!({ "filepath": path.to_string_lossy() }));
            assert!(r.is_error);
        }
    }

    // ==================== read/write handler success paths ====================

    mod handler_success_paths {
        use crate::altium::pcblib::{Footprint, Pad, PcbLib};
        use crate::mcp::tools::test_support::{
            create_test_pcblib, create_test_schlib, create_test_server, get_result_text,
            parse_result_json, test_temp_dir,
        };
        use serde_json::json;

        // ---- write_pcblib 3D-body summary sources ----

        #[test]
        fn write_pcblib_component_body_reports_extruded() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Body.PcbLib");
            let r = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{
                    "name": "BODYFP",
                    "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                    "component_bodies": [{ "overall_height": 2.5, "standoff_height": 0.1 }],
                }],
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "success");
            assert_eq!(p["footprint_count"], 1);
            assert_eq!(p["bodies"][0]["source"], "extruded");
            assert_eq!(p["bodies"][0]["assumed_height"], false);
        }

        #[test]
        fn write_pcblib_step_model_external_reports_step_external() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Ext.PcbLib");
            let r = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{
                    "name": "EXTMODEL",
                    "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                    "step_model": { "filepath": "models/CHIP.step", "embed": false, "rotation": 90.0, "z_offset": 0.5 },
                }],
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["bodies"][0]["source"], "step-external");
        }

        #[test]
        fn write_pcblib_auto_3d_body_reports_auto_extruded() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Auto.PcbLib");
            let r = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "auto_3d_body": true,
                "footprints": [{
                    "name": "AUTO",
                    "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                }],
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["bodies"][0]["source"], "auto-extruded");
            assert_eq!(p["bodies"][0]["assumed_height"], true);
            assert!(p["bodies"][0]["action_required"].is_string());
        }

        #[test]
        fn write_pcblib_silk_over_pad_warns() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Silk.PcbLib");
            let r = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{
                    "name": "SILK",
                    "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                    "tracks": [{ "x1": -2.0, "y1": 0.0, "x2": 2.0, "y2": 0.0, "width": 0.2, "layer": "Top Overlay" }],
                }],
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            let warnings = p["warnings"].as_array().unwrap();
            assert!(!warnings.is_empty());
            assert_eq!(warnings[0]["type"], "silk_over_pad");
            assert_eq!(warnings[0]["pad"], "1");
        }

        #[test]
        fn write_pcblib_append_adds_to_existing() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Append.PcbLib");
            let fp = |name: &str| json!({ "name": name, "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }] });
            server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(), "footprints": [fp("A")],
            }));
            let r = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(), "append": true, "footprints": [fp("B")],
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            assert_eq!(parse_result_json(&r)["footprint_count"], 2);
        }

        // ---- read_pcblib emission + compact + pagination ----

        #[test]
        fn read_pcblib_emits_vias_fills_bodies_and_is_compact() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Rich.PcbLib");
            server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{
                    "name": "RICH",
                    "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                    "vias": [{ "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3 }],
                    "fills": [{ "x1": -1.0, "y1": -1.0, "x2": 1.0, "y2": 1.0, "layer": "Top Layer" }],
                    "component_bodies": [{ "overall_height": 2.0 }],
                }],
            }));

            let r = server.call_read_pcblib(&json!({ "filepath": path.to_string_lossy() }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "success");
            assert_eq!(p["compact"], true);
            assert_eq!(p["units"], "mm");
            let fp0 = &p["footprints"][0];
            assert_eq!(fp0["vias"].as_array().unwrap().len(), 1);
            assert_eq!(fp0["fills"].as_array().unwrap().len(), 1);
            assert_eq!(fp0["component_bodies"].as_array().unwrap().len(), 1);
        }

        #[test]
        fn read_pcblib_non_compact_and_pagination() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Page.PcbLib");
            create_test_pcblib(&path); // 2 footprints

            let non_compact = server
                .call_read_pcblib(&json!({ "filepath": path.to_string_lossy(), "compact": false }));
            assert_eq!(parse_result_json(&non_compact)["compact"], false);

            let paged = server.call_read_pcblib(&json!({
                "filepath": path.to_string_lossy(), "limit": 1, "offset": 0,
            }));
            let p = parse_result_json(&paged);
            assert_eq!(p["returned_count"], 1);
            assert_eq!(p["has_more"], true);

            let named = server.call_read_pcblib(&json!({
                "filepath": path.to_string_lossy(), "component_name": "CHIP_0402",
            }));
            let p = parse_result_json(&named);
            assert_eq!(p["returned_count"], 1);
            assert_eq!(p["has_more"], false);
        }

        // ---- read_schlib emission + write_schlib deep ----

        #[test]
        fn write_then_read_schlib_emits_parameters_and_footprints() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Sym.SchLib");
            let w = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [{
                    "name": "R1",
                    "component_type": "resistor",
                    "pins": [
                        { "name": "1", "designator": "1", "x": -20, "y": 0, "length": 10, "orientation": "left" },
                        { "name": "2", "designator": "2", "x": 20, "y": 0, "length": 10, "orientation": "right" },
                    ],
                    "parameters": [{ "name": "Value", "value": "10k" }],
                    "footprints": [{ "name": "CHIP_0402", "library_path": "parts.PcbLib" }],
                }],
            }));
            assert!(!w.is_error, "{}", get_result_text(&w));
            let wp = parse_result_json(&w);
            assert_eq!(wp["symbol_count"], 1);
            // geometry echo: left pin tip = x - length.
            assert_eq!(wp["geometry"][0]["pins"][0]["tip"]["x"], -30);
            assert_eq!(wp["geometry"][0]["bounding_box"]["min_x"], -30);

            let r = server.call_read_schlib(&json!({ "filepath": path.to_string_lossy() }));
            let p = parse_result_json(&r);
            assert_eq!(p["units"], "schematic units (10 = 1 grid)");
            let sym = &p["symbols"][0];
            assert_eq!(sym["parameters"].as_array().unwrap().len(), 1);
            assert_eq!(sym["footprints"].as_array().unwrap().len(), 1);
        }

        #[test]
        fn write_schlib_component_type_sets_designator_prefix() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Cap.SchLib");
            server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [{
                    "name": "C1",
                    "component_type": "capacitor",
                    "part_count": 2,
                    "pins": [{ "name": "1", "designator": "1", "x": -20, "y": 0, "length": 10, "orientation": "left" }],
                }],
            }));
            let r = server.call_read_schlib(&json!({ "filepath": path.to_string_lossy() }));
            let sym = &parse_result_json(&r)["symbols"][0];
            assert_eq!(sym["designator"], "C?");
            assert_eq!(sym["part_count"], 2);
        }

        // ---- write_libpkg (fully uncovered) ----

        #[test]
        fn write_libpkg_success_and_errors() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Proj.LibPkg");
            let r = server.call_write_libpkg(&json!({
                "filepath": path.to_string_lossy(),
                "documents": ["Symbols.SchLib", "Footprints.PcbLib"],
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "success");
            assert_eq!(p["count"], 2);
            assert!(p["note"].as_str().unwrap().contains("Compile"));

            // Wrong extension and empty documents are errors.
            let bad_ext = dir.path().join("x.txt");
            assert!(
                server
                    .call_write_libpkg(
                        &json!({ "filepath": bad_ext.to_string_lossy(), "documents": ["a.SchLib"] })
                    )
                    .is_error
            );
            assert!(
                server
                    .call_write_libpkg(
                        &json!({ "filepath": path.to_string_lossy(), "documents": [] })
                    )
                    .is_error
            );
        }

        // ---- list_components metadata + pagination ----

        #[test]
        fn list_components_pcblib_metadata_and_pagination() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("L.PcbLib");
            create_test_pcblib(&path);

            let meta = server.call_list_components(&json!({
                "filepath": path.to_string_lossy(), "include_metadata": true,
            }));
            let p = parse_result_json(&meta);
            assert_eq!(p["file_type"], "PcbLib");
            assert_eq!(p["include_metadata"], true);
            assert_eq!(p["components"][0]["pad_count"], 2);
            assert_eq!(p["components"][0]["has_3d_model"], false);
            // One count per primitive kind, every kind.
            for kind in crate::altium::pcblib::PrimitiveKind::WRITE_ORDER {
                let key = format!("{}_count", kind.name());
                assert!(
                    p["components"][0][&key].is_u64(),
                    "{key} missing: {}",
                    p["components"][0]
                );
            }

            let paged = server.call_list_components(&json!({
                "filepath": path.to_string_lossy(), "limit": 1, "offset": 0,
            }));
            let p = parse_result_json(&paged);
            assert_eq!(p["returned_count"], 1);
            assert_eq!(p["has_more"], true);
        }

        #[test]
        fn list_components_schlib_metadata() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("L.SchLib");
            create_test_schlib(&path);
            let r = server.call_list_components(&json!({
                "filepath": path.to_string_lossy(), "include_metadata": true,
            }));
            let p = parse_result_json(&r);
            assert_eq!(p["file_type"], "SchLib");
            assert_eq!(p["components"][0]["pin_count"], 2);
        }

        // ---- extract_style statistic branches ----

        #[test]
        fn extract_style_pcblib_text_heights_non_null() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("T.PcbLib");
            // write_pcblib auto-injects a .Designator text (height 1.0) per footprint.
            server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{ "name": "F", "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }] }],
            }));
            let r = server.call_extract_style(&json!({ "filepath": path.to_string_lossy() }));
            let p = parse_result_json(&r);
            assert!(p["style"]["text_heights"]["count"].as_u64().unwrap() >= 1);
        }

        #[test]
        fn extract_style_pcblib_pad_shape_distribution() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Shapes.PcbLib");
            let mut lib = PcbLib::new();
            let mut fp = Footprint::new("MIX");
            fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.6, 0.5)); // RoundedRectangle
            fp.add_pad(Pad::through_hole("2", 1.0, 0.0, 0.8, 0.8, 0.4)); // Round, Multi-Layer
            lib.add(fp);
            lib.save(&path).unwrap();

            let r = server.call_extract_style(&json!({ "filepath": path.to_string_lossy() }));
            let p = parse_result_json(&r);
            let shapes = &p["style"]["pad_shapes"];
            assert_eq!(shapes["RoundedRectangle"], 1);
            assert_eq!(shapes["Round"], 1);
            assert!(p["style"]["layers_used"].get("Multi-Layer").is_some());
        }

        #[test]
        fn extract_style_schlib_unfilled_rect_and_lines() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("S.SchLib");
            server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [{
                    "name": "S",
                    "pins": [{ "name": "1", "designator": "1", "x": -20, "y": 0, "length": 10, "orientation": "left" }],
                    "rectangles": [{ "x1": -10, "y1": -5, "x2": 10, "y2": 5, "filled": false }],
                    "lines": [{ "x1": -5, "y1": 0, "x2": 5, "y2": 0, "line_width": 1 }],
                }],
            }));
            let r = server.call_extract_style(&json!({ "filepath": path.to_string_lossy() }));
            let p = parse_result_json(&r);
            assert_eq!(p["style"]["rectangles"]["unfilled_count"], 1);
            assert_eq!(p["style"]["rectangles"]["filled_count"], 0);
            assert!(p["style"]["line_widths"]["count"].as_u64().unwrap() >= 1);
        }
    }

    // ==================== rejection and failure paths ====================
    //
    // Every handler in this file answers a bad request by returning a
    // `ToolCallResult::error` rather than by panicking, so the rejection is the
    // contract and needs a test each. Grouped by handler, in call order.

    mod failure_paths {
        use crate::mcp::tools::parsing::DESCRIPTION_MAX_LEN;
        use crate::mcp::tools::test_support::{
            create_test_server, get_result_text, parse_result_json, test_temp_dir,
        };
        use serde_json::{json, Value};

        /// The minimal pad payload a footprint needs to be writable, so each
        /// test can vary exactly one field away from a known-good request.
        fn pad(designator: &str) -> Value {
            json!({ "designator": designator, "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 })
        }

        /// A footprint carrying one valid pad.
        fn footprint(name: &str) -> Value {
            json!({ "name": name, "pads": [pad("1")] })
        }

        /// A symbol carrying one valid pin.
        fn symbol(name: &str) -> Value {
            json!({
                "name": name,
                "pins": [{
                    "name": "1", "designator": "1",
                    "x": -20, "y": 0, "length": 10, "orientation": "left",
                }],
            })
        }

        /// Writes bytes that are not an OLE compound file, so `open` fails.
        fn write_garbage(path: &std::path::Path) {
            std::fs::write(path, b"not an OLE compound document").unwrap();
        }

        // ---------------------------------------------------------------
        // Description length. The Altium 365 library importer refuses a
        // component whose description exceeds 256 characters and names
        // neither library nor component; Altium Designer itself opens and
        // reads such a library whole. So the write goes ahead and the
        // post-write validation carries a warning that names the component.
        // ---------------------------------------------------------------

        /// A description of exactly `n` characters.
        fn desc(n: usize) -> String {
            "d".repeat(n)
        }

        /// The validation warnings a write reported, as `component: issue`.
        fn warnings(result: &crate::mcp::server::ToolCallResult) -> Vec<String> {
            parse_result_json(result)["validation"]["issues"]
                .as_array()
                .map(|issues| {
                    issues
                        .iter()
                        .filter(|i| i["severity"] == "warning")
                        .map(|i| {
                            format!(
                                "{}: {}",
                                i["component"].as_str().unwrap_or(""),
                                i["issue"].as_str().unwrap_or("")
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        }

        #[test]
        fn a_footprint_description_at_the_limit_is_accepted_without_a_warning() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("AtLimit.PcbLib");
            let at_limit = desc(DESCRIPTION_MAX_LEN);

            let write = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{
                    "name": "FP", "description": at_limit, "pads": [pad("1")],
                }],
            }));
            assert!(!write.is_error, "{}", get_result_text(&write));
            assert!(
                warnings(&write)
                    .iter()
                    .all(|w| !w.contains("Description is")),
                "at the limit is not over it: {:?}",
                warnings(&write)
            );

            let read = server.call_read_pcblib(&json!({
                "filepath": path.to_string_lossy(),
            }));
            let got = parse_result_json(&read)["footprints"][0]["description"]
                .as_str()
                .unwrap()
                .to_string();
            assert_eq!(got, at_limit, "a description at the limit must survive");
        }

        #[test]
        fn an_over_length_footprint_description_is_written_with_a_warning() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("TooLong.PcbLib");

            let write = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{
                    "name": "WIDE_DESC",
                    "description": desc(DESCRIPTION_MAX_LEN + 1),
                    "pads": [pad("1")],
                }],
            }));
            assert!(
                !write.is_error,
                "Altium Designer opens such a library; it is written: {}",
                get_result_text(&write)
            );
            assert!(path.exists(), "the library is written as asked");
            let found = warnings(&write);
            let warning = found
                .iter()
                .find(|w| w.starts_with("WIDE_DESC: Description is"))
                .unwrap_or_else(|| panic!("a warning must name the footprint: {found:?}"));
            assert!(
                warning.contains("Altium 365"),
                "must name the importer: {warning}"
            );
            assert!(
                warning.contains(&DESCRIPTION_MAX_LEN.to_string()),
                "must state the limit: {warning}"
            );
            assert!(
                warning.contains("shorten it by 1 characters"),
                "must state the overshoot: {warning}"
            );

            // The description itself is untouched on disk.
            let read = server.call_read_pcblib(&json!({
                "filepath": path.to_string_lossy(),
            }));
            let got = parse_result_json(&read)["footprints"][0]["description"]
                .as_str()
                .unwrap()
                .chars()
                .count();
            assert_eq!(got, DESCRIPTION_MAX_LEN + 1, "written whole, not truncated");
        }

        #[test]
        fn an_over_length_symbol_description_is_written_with_a_warning() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("TooLong.SchLib");

            let mut sym = symbol("WIDE_SYM");
            sym["description"] = json!(desc(DESCRIPTION_MAX_LEN + 40));
            let write = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [sym],
            }));
            assert!(!write.is_error, "{}", get_result_text(&write));
            let found = warnings(&write);
            assert!(
                found
                    .iter()
                    .any(|w| w.starts_with("WIDE_SYM: Description is")
                        && w.contains("shorten it by 40 characters")),
                "must name the symbol and the overshoot: {found:?}"
            );
        }

        #[test]
        fn an_over_length_footprint_link_description_is_written_with_a_warning() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Link.SchLib");

            let mut sym = symbol("SYM");
            sym["footprints"] = json!([{
                "name": "LINKED_FP",
                "description": desc(DESCRIPTION_MAX_LEN + 5),
            }]);
            let write = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [sym],
            }));
            assert!(!write.is_error, "{}", get_result_text(&write));
            let found = warnings(&write);
            assert!(
                found
                    .iter()
                    .any(|w| w.starts_with("SYM -> LINKED_FP: Description is")),
                "the link's description is stored in the SchLib too, and named: {found:?}"
            );
        }

        /// `validate_library` reports the same finding on libraries this
        /// server did not author — an older build, or a description carried
        /// in by a copy — as a warning that names the component.
        #[test]
        fn validate_library_reports_an_over_length_description_as_a_warning() {
            use crate::altium::pcblib::{Footprint, Pad, PcbLib};

            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Legacy.PcbLib");

            let mut fp = Footprint::new("LEGACY_FP");
            fp.description = desc(DESCRIPTION_MAX_LEN + 90);
            fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
            let mut lib = PcbLib::new();
            lib.add(fp);
            lib.save(&path).expect("writing directly must succeed");

            let result = server.call_validate_library(&json!({
                "filepath": path.to_string_lossy(),
            }));
            let json_out = parse_result_json(&result);
            assert_eq!(
                json_out["error_count"].as_u64().unwrap(),
                0,
                "Altium Designer opens it, so not an error: {json_out}"
            );
            let issues = json_out["issues"].as_array().unwrap();
            assert!(
                issues.iter().any(|i| {
                    i["severity"] == "warning"
                        && i["component"] == "LEGACY_FP"
                        && i["issue"]
                            .as_str()
                            .unwrap_or("")
                            .contains("shorten it by 90 characters")
                }),
                "the offending component must be named: {json_out}"
            );
        }

        /// Flips a file's read-only bit, used to make a save fail without
        /// depending on the caller running unprivileged.
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

        /// Asserts the call failed and its message mentions `needle`.
        fn assert_error_mentions(result: &crate::mcp::server::ToolCallResult, needle: &str) {
            let text = get_result_text(result);
            assert!(result.is_error, "expected an error, got: {text}");
            assert!(
                text.contains(needle),
                "expected the error to mention {needle:?}, got: {text}"
            );
        }

        // ---- geometry helpers -------------------------------------------------

        #[test]
        fn segment_rect_misses_when_the_whole_segment_is_outside_the_slab() {
            use super::super::segment_intersects_rect;
            // Points away from the rect along +x while lying entirely to its
            // left: the entering parameter overshoots the exit, which is the
            // `t > u2` rejection rather than the `t < u1` one the other
            // direction takes.
            assert!(!segment_intersects_rect(
                -5.0, 0.0, -3.0, 0.0, -1.0, -1.0, 1.0, 1.0
            ));
            // Mirror case in -y, so the vertical slab takes the same branch.
            assert!(!segment_intersects_rect(
                0.0, 5.0, 0.0, 3.0, -1.0, -1.0, 1.0, 1.0
            ));
        }

        #[test]
        fn silk_warning_follows_the_side_the_track_is_on() {
            use super::super::silk_over_pad_warnings;
            use crate::altium::pcblib::{Footprint, Layer, Pad, Track};

            // Bottom overlay silk over a bottom-layer pad: reported.
            let mut hit = Footprint::new("BOT");
            let mut bottom_pad = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
            bottom_pad.layer = Layer::BottomLayer;
            hit.add_pad(bottom_pad);
            hit.add_track(Track::new(-2.0, 0.0, 2.0, 0.0, 0.2, Layer::BottomOverlay));
            let warnings = silk_over_pad_warnings(&hit);
            assert_eq!(warnings.len(), 1, "{warnings:?}");
            assert_eq!(warnings[0]["layer"], "Bottom Overlay");

            // Same silk, but the pad is top-only: opposite sides never clash,
            // so the pad is skipped even though the geometry overlaps.
            let mut miss = Footprint::new("TOP");
            let mut top_pad = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
            top_pad.layer = Layer::TopLayer;
            miss.add_pad(top_pad);
            miss.add_track(Track::new(-2.0, 0.0, 2.0, 0.0, 0.2, Layer::BottomOverlay));
            assert!(silk_over_pad_warnings(&miss).is_empty());
        }

        #[test]
        fn pad_overlap_warnings_report_pairs_and_cap_the_list() {
            use super::super::pad_copper_overlap_warnings;
            use crate::altium::pcblib::{Footprint, Pad, MAX_REPORTED_PAD_OVERLAPS};

            // Two overlapping pads: one warning naming both designators.
            let mut two = Footprint::new("TWO");
            two.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
            two.add_pad(Pad::smd("2", 0.2, 0.0, 1.0, 1.0));
            let warnings = pad_copper_overlap_warnings(&two);
            assert_eq!(warnings.len(), 1, "{warnings:?}");
            assert_eq!(warnings[0]["type"], "pad_copper_overlap");
            assert_eq!(warnings[0]["pads"], json!(["1", "2"]));

            // Overlapping pairs are quadratic in pad count: 8 stacked pads make
            // 28 pairs, which must truncate to the cap plus one summary line
            // carrying the true total.
            let mut many = Footprint::new("MANY");
            for i in 0..8 {
                many.add_pad(Pad::smd(format!("{i}"), 0.0, 0.0, 1.0, 1.0));
            }
            let warnings = pad_copper_overlap_warnings(&many);
            assert_eq!(warnings.len(), MAX_REPORTED_PAD_OVERLAPS + 1);
            let summary = warnings.last().unwrap()["message"].as_str().unwrap();
            assert!(
                summary.starts_with("28 overlapping pad pairs total"),
                "{summary}"
            );
        }

        // ---- write_pcblib -----------------------------------------------------

        #[test]
        fn write_pcblib_rejects_a_duplicate_name_within_one_request() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let r = server.call_write_pcblib(&json!({
                "filepath": dir.path().join("Dup.PcbLib").to_string_lossy(),
                "footprints": [footprint("SAME"), footprint("SAME")],
            }));
            assert_error_mentions(&r, "Duplicate footprint name");
        }

        #[test]
        fn write_pcblib_rejects_empty_and_invalid_names() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Names.PcbLib");

            let empty = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [footprint("")],
            }));
            assert_error_mentions(&empty, "cannot be empty");

            let invalid = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [footprint("BAD\tNAME")],
            }));
            assert_error_mentions(&invalid, "control character");
        }

        #[test]
        fn write_pcblib_append_reports_an_unreadable_existing_library() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Corrupt.PcbLib");
            write_garbage(&path);
            let r = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "append": true,
                "footprints": [footprint("A")],
            }));
            assert_error_mentions(&r, "Failed to read existing library");
        }

        #[test]
        fn write_pcblib_append_rejects_a_name_already_in_the_library() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Existing.PcbLib");
            server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [footprint("A")],
            }));
            let r = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "append": true,
                "footprints": [footprint("A")],
            }));
            assert_error_mentions(&r, "already exists in the library");
        }

        #[test]
        fn write_pcblib_reports_which_primitive_failed_to_parse() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Parse.PcbLib");

            // One case per primitive family, each malformed in its own way, so
            // the index and family named in `details` are both exercised.
            let cases: [(&str, Value, &str); 5] = [
                (
                    "pads",
                    json!([{ "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }]),
                    "pad at index 0",
                ),
                (
                    "tracks",
                    json!([{ "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.2 }]),
                    "track at index 0",
                ),
                (
                    "vias",
                    json!([{ "x": 0.0, "y": 0.0, "diameter": 0.0, "hole_size": 0.3 }]),
                    "via at index 0",
                ),
                (
                    "fills",
                    json!([{ "y1": 0.0, "x2": 1.0, "y2": 1.0 }]),
                    "fill at index 0",
                ),
                (
                    "arcs",
                    json!([{ "x": 0.0, "y": 0.0, "radius": 1.0, "start_angle": 0.0, "end_angle": 90.0 }]),
                    "arc at index 0",
                ),
            ];

            for (key, payload, expected) in cases {
                let mut fp = json!({ "name": "FP", "pads": [pad("1")] });
                fp[key] = payload;
                let r = server.call_write_pcblib(&json!({
                    "filepath": path.to_string_lossy(),
                    "footprints": [fp],
                }));
                assert_error_mentions(&r, expected);
            }
        }

        #[test]
        fn write_pcblib_gates_embedded_step_models_against_the_allowlist() {
            // The embed source is read from disk at save time, so a path
            // outside the allow-list would be an arbitrary-file read.
            let allowed = test_temp_dir();
            let outside = test_temp_dir();
            let server = create_test_server(allowed.path());
            let model = outside.path().join("secret.step");
            std::fs::write(&model, b"ISO-10303-21;\n").unwrap();

            let r = server.call_write_pcblib(&json!({
                "filepath": allowed.path().join("Gated.PcbLib").to_string_lossy(),
                "footprints": [{
                    "name": "FP",
                    "pads": [pad("1")],
                    "step_model": { "filepath": model.to_string_lossy(), "embed": true },
                }],
            }));
            assert!(r.is_error, "{}", get_result_text(&r));
        }

        #[test]
        fn write_pcblib_embeds_a_permitted_step_model_and_keeps_external_refs() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let model = dir.path().join("body.step");
            std::fs::write(
                &model,
                b"ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;\n",
            )
            .unwrap();

            // embed = true takes the Model3D path and reports step-embedded.
            let embedded = server.call_write_pcblib(&json!({
                "filepath": dir.path().join("Embed.PcbLib").to_string_lossy(),
                "footprints": [{
                    "name": "FP",
                    "pads": [pad("1")],
                    "step_model": {
                        "filepath": model.to_string_lossy(), "embed": true,
                        "x_offset": 1.0, "y_offset": 2.0, "z_offset": 3.0, "rotation": 90.0,
                    },
                }],
            }));
            assert!(!embedded.is_error, "{}", get_result_text(&embedded));
            assert_eq!(
                parse_result_json(&embedded)["bodies"][0]["source"],
                "step-embedded"
            );

            // embed = false stores a bare reference and never reads the file,
            // so it is not gated and reports step-external.
            let external = server.call_write_pcblib(&json!({
                "filepath": dir.path().join("External.PcbLib").to_string_lossy(),
                "footprints": [{
                    "name": "FP",
                    "pads": [pad("1")],
                    "step_model": {
                        "filepath": "3D_Models/elsewhere.step", "embed": false,
                        "rotation": 45.0, "z_offset": 1.5,
                    },
                }],
            }));
            assert!(!external.is_error, "{}", get_result_text(&external));
            let body = &parse_result_json(&external)["bodies"][0];
            assert_eq!(body["source"], "step-external");
            assert_eq!(body["model"], "3D_Models/elsewhere.step");
        }

        #[test]
        fn write_pcblib_gates_model_3d_only_when_it_names_a_real_file() {
            let allowed = test_temp_dir();
            let outside = test_temp_dir();
            let server = create_test_server(allowed.path());
            let model = outside.path().join("outside.step");
            std::fs::write(&model, b"ISO-10303-21;\n").unwrap();

            // An existing file outside the allow-list is refused...
            let gated = server.call_write_pcblib(&json!({
                "filepath": allowed.path().join("M1.PcbLib").to_string_lossy(),
                "footprints": [{
                    "name": "FP",
                    "pads": [pad("1")],
                    "model_3d": { "filepath": model.to_string_lossy() },
                }],
            }));
            assert!(gated.is_error, "{}", get_result_text(&gated));

            // ...while the same key pointing inside the allow-list is accepted
            // and lands on the footprint, so a read -> write replay keeps its
            // model instead of dropping it.
            let permitted = allowed.path().join("inside.step");
            std::fs::write(&permitted, b"ISO-10303-21;\n").unwrap();
            let replayed = server.call_write_pcblib(&json!({
                "filepath": allowed.path().join("M2.PcbLib").to_string_lossy(),
                "footprints": [{
                    "name": "FP",
                    "pads": [pad("1")],
                    "model_3d": { "filepath": permitted.to_string_lossy(), "z_offset": 0.5 },
                }],
            }));
            assert!(!replayed.is_error, "{}", get_result_text(&replayed));
            assert_eq!(
                parse_result_json(&replayed)["bodies"][0]["source"],
                "step-embedded"
            );
        }

        #[test]
        fn write_pcblib_rejects_out_of_range_coordinates() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let r = server.call_write_pcblib(&json!({
                "filepath": dir.path().join("Far.PcbLib").to_string_lossy(),
                "footprints": [{
                    "name": "FP",
                    "pads": [{ "designator": "1", "x": 99_999.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                }],
            }));
            assert_error_mentions(&r, "exceeds the maximum safe range");
        }

        #[test]
        fn write_pcblib_reports_backup_and_save_failures() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            // A directory sitting where the library should be: it exists, so a
            // backup is attempted, and copying a directory fails.
            let as_dir = dir.path().join("Blocked.PcbLib");
            std::fs::create_dir(&as_dir).unwrap();
            let backup = server.call_write_pcblib(&json!({
                "filepath": as_dir.to_string_lossy(),
                "footprints": [footprint("A")],
            }));
            assert_error_mentions(&backup, "backup");

            // With the save temp path blocked, the backup still succeeds —
            // it is a plain copy — so the save is what fails, and the failure
            // is reported as a structured result rather than a panic.
            let locked = dir.path().join("ReadOnly.PcbLib");
            server.call_write_pcblib(&json!({
                "filepath": locked.to_string_lossy(),
                "footprints": [footprint("A")],
            }));
            block_save(&locked, true);
            let save = server.call_write_pcblib(&json!({
                "filepath": locked.to_string_lossy(),
                "footprints": [footprint("B")],
            }));
            block_save(&locked, false); // frees the squatted temp path
            assert!(save.is_error, "{}", get_result_text(&save));
            assert_eq!(parse_result_json(&save)["status"], "error");
        }

        // ---- read_pcblib / read_schlib ---------------------------------------

        #[test]
        fn read_pcblib_compact_keeps_a_stacked_pad_intact() {
            // Compact mode strips the per-layer arrays of a Simple pad only.
            // A FullStack pad keeps its arrays and its stack_mode even when
            // every layer matches the primary pair: the mode is a stored
            // property Altium shows, and reporting it as "simple" turned the
            // pad into a simple one on the next write.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Stack.PcbLib");
            let uniform: Vec<Value> = (0..32)
                .map(|_| json!({ "width": 1.0, "height": 1.0 }))
                .collect();
            let written = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{
                    "name": "FP",
                    "pads": [
                        {
                            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
                            "stack_mode": "full_stack",
                            "per_layer_sizes": uniform,
                        },
                        { "designator": "2", "x": 2.0, "y": 0.0, "width": 1.0, "height": 1.0 }
                    ],
                }],
            }));
            assert!(!written.is_error, "{}", get_result_text(&written));

            let r = server.call_read_pcblib(&json!({
                "filepath": path.to_string_lossy(), "compact": true,
            }));
            let pads = &parse_result_json(&r)["footprints"][0]["pads"];
            assert_eq!(
                pads[0]["stack_mode"], "full_stack",
                "mode kept: {}",
                pads[0]
            );
            assert_eq!(
                pads[0]["per_layer_sizes"].as_array().map(Vec::len),
                Some(32),
                "arrays kept for a stacked pad: {}",
                pads[0]
            );
            assert_eq!(pads[1]["stack_mode"], "simple");
            assert!(
                pads[1].get("per_layer_sizes").is_none(),
                "a Simple pad's arrays are stripped: {}",
                pads[1]
            );

            // And the compact output writes back to the same stack mode.
            let back = dir.path().join("StackBack.PcbLib");
            let rewritten = server.call_write_pcblib(&json!({
                "filepath": back.to_string_lossy(),
                "footprints": parse_result_json(&r)["footprints"],
            }));
            assert!(!rewritten.is_error, "{}", get_result_text(&rewritten));
            let again = server.call_read_pcblib(&json!({ "filepath": back.to_string_lossy() }));
            assert_eq!(
                parse_result_json(&again)["footprints"][0]["pads"][0]["stack_mode"],
                "full_stack"
            );
        }

        #[test]
        fn read_schlib_single_component_fetch_reports_no_more_pages() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Two.SchLib");
            server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [symbol("A"), symbol("B")],
            }));
            let r = server.call_read_schlib(&json!({
                "filepath": path.to_string_lossy(), "component_name": "A",
            }));
            let p = parse_result_json(&r);
            assert_eq!(p["returned_count"], 1);
            assert_eq!(p["total_count"], 2);
            // Filtering is not pagination, so there is never a next page.
            assert_eq!(p["has_more"], false);
        }

        /// Each listing handler refuses an unusable page by name itself, so a
        /// direct caller gets the same answer the dispatch check gives.
        #[test]
        fn paging_arguments_are_refused_by_each_listing_handler() {
            use crate::mcp::tools::test_support::{create_test_pcblib, create_test_schlib};

            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let pcb = dir.path().join("Page.PcbLib");
            create_test_pcblib(&pcb);
            let sch = dir.path().join("Page.SchLib");
            create_test_schlib(&sch);

            let cases = [
                server.call_read_pcblib(&json!({ "filepath": pcb.to_string_lossy(), "limit": 0 })),
                server.call_read_schlib(&json!({ "filepath": sch.to_string_lossy(), "limit": 0 })),
                server.call_list_components(&json!({
                    "filepath": pcb.to_string_lossy(), "offset": -1,
                })),
            ];
            let expected = [
                "limit must be a whole number of 1 or more, got 0",
                "limit must be a whole number of 1 or more, got 0",
                "offset must be a whole number of 0 or more, got -1",
            ];
            for (result, expected) in cases.iter().zip(expected) {
                assert!(result.is_error);
                let text = get_result_text(result);
                assert!(text.contains(expected), "{text}");
            }
        }

        /// The read tools resolve a requested component as every other tool
        /// does — regardless of case, answering with the spelling on file —
        /// and report a miss rather than returning an empty success.
        #[test]
        fn read_tools_resolve_component_name_regardless_of_case_and_report_a_miss() {
            use crate::mcp::tools::test_support::{create_test_pcblib, create_test_schlib};

            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let pcb = dir.path().join("Case.PcbLib");
            create_test_pcblib(&pcb);
            let sch = dir.path().join("Case.SchLib");
            create_test_schlib(&sch);

            let r = server.call_read_pcblib(&json!({
                "filepath": pcb.to_string_lossy(), "component_name": "chip_0402",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["returned_count"], 1);
            assert_eq!(
                p["footprints"][0]["name"], "CHIP_0402",
                "the spelling on file"
            );

            let r = server.call_read_schlib(&json!({
                "filepath": sch.to_string_lossy(), "component_name": "resistor",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["returned_count"], 1);
            assert_eq!(p["symbols"][0]["name"], "RESISTOR");

            let r = server.call_read_pcblib(&json!({
                "filepath": pcb.to_string_lossy(), "component_name": "CHIP_0402X",
            }));
            assert!(r.is_error, "{}", get_result_text(&r));
            let text = get_result_text(&r);
            assert!(
                text.contains(
                    "Component 'CHIP_0402X' not found in library. Available: CHIP_0402, CHIP_0603"
                ),
                "{text}"
            );
            let r = server.call_read_schlib(&json!({
                "filepath": sch.to_string_lossy(), "component_name": "NOPE",
            }));
            assert!(r.is_error, "{}", get_result_text(&r));
            let text = get_result_text(&r);
            assert!(
                text.contains(
                    "Component 'NOPE' not found in library. Available: RESISTOR, CAPACITOR"
                ),
                "{text}"
            );
        }

        #[test]
        fn read_schlib_reports_an_unreadable_library() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Corrupt.SchLib");
            write_garbage(&path);
            let r = server.call_read_schlib(&json!({ "filepath": path.to_string_lossy() }));
            assert!(r.is_error, "{}", get_result_text(&r));
            assert_eq!(parse_result_json(&r)["status"], "error");
        }

        // ---- write_schlib -----------------------------------------------------

        #[test]
        fn write_schlib_rejects_a_path_outside_the_allowlist() {
            let allowed = test_temp_dir();
            let outside = test_temp_dir();
            let server = create_test_server(allowed.path());
            let r = server.call_write_schlib(&json!({
                "filepath": outside.path().join("Escape.SchLib").to_string_lossy(),
                "symbols": [symbol("A")],
            }));
            assert!(r.is_error, "{}", get_result_text(&r));
        }

        #[test]
        fn write_schlib_rejects_duplicate_empty_and_invalid_names() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Names.SchLib");

            let dup = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [symbol("SAME"), symbol("SAME")],
            }));
            assert_error_mentions(&dup, "Duplicate symbol name");

            let empty = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [symbol("")],
            }));
            assert_error_mentions(&empty, "cannot be empty");

            let invalid = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [symbol("BAD\tNAME")],
            }));
            assert_error_mentions(&invalid, "control character");
        }

        #[test]
        fn write_schlib_append_reports_an_unreadable_existing_library() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Corrupt.SchLib");
            write_garbage(&path);
            let r = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "append": true,
                "symbols": [symbol("A")],
            }));
            assert_error_mentions(&r, "Failed to read existing library");
        }

        #[test]
        fn write_schlib_append_rejects_a_name_already_in_the_library() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Existing.SchLib");
            server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(), "symbols": [symbol("A")],
            }));
            let r = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(), "append": true, "symbols": [symbol("A")],
            }));
            assert_error_mentions(&r, "already exists in the library");
        }

        #[test]
        fn write_schlib_keeps_the_supplied_designator_placement_and_identity() {
            // A read-modify-write replays these three fields, so they must
            // survive rather than reset to the AD24 defaults.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Desig.SchLib");
            let mut sym = symbol("A");
            sym["designator_x"] = json!(-12.0);
            sym["designator_y"] = json!(18.0);
            sym["designator_unique_id"] = json!("ABCDEFGH");
            let w = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(), "symbols": [sym],
            }));
            assert!(!w.is_error, "{}", get_result_text(&w));

            let r = server.call_read_schlib(&json!({ "filepath": path.to_string_lossy() }));
            let s = &parse_result_json(&r)["symbols"][0];
            assert_eq!(s["designator_x"], -12.0);
            assert_eq!(s["designator_y"], 18.0);
        }

        #[test]
        fn write_schlib_records_a_footprint_library_path() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Linked.SchLib");
            let mut sym = symbol("A");
            sym["footprints"] = json!([{
                "name": "CHIP_0402",
                "description": "0402 chip",
                "library_path": "Parts.PcbLib",
            }]);
            let w = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(), "symbols": [sym],
            }));
            assert!(!w.is_error, "{}", get_result_text(&w));

            let r = server.call_read_schlib(&json!({ "filepath": path.to_string_lossy() }));
            let fp = &parse_result_json(&r)["symbols"][0]["footprints"][0];
            assert_eq!(fp["name"], "CHIP_0402");
        }

        #[test]
        fn write_schlib_rejects_out_of_range_coordinates() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let r = server.call_write_schlib(&json!({
                "filepath": dir.path().join("Far.SchLib").to_string_lossy(),
                "symbols": [{
                    "name": "A",
                    "pins": [{
                        "name": "1", "designator": "1",
                        "x": 999_999, "y": 0, "length": 10, "orientation": "left",
                    }],
                }],
            }));
            assert_error_mentions(&r, "exceeds the maximum safe range");
        }

        #[test]
        fn write_schlib_reports_backup_and_save_failures() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let as_dir = dir.path().join("Blocked.SchLib");
            std::fs::create_dir(&as_dir).unwrap();
            let backup = server.call_write_schlib(&json!({
                "filepath": as_dir.to_string_lossy(), "symbols": [symbol("A")],
            }));
            assert_error_mentions(&backup, "backup");

            let locked = dir.path().join("ReadOnly.SchLib");
            server.call_write_schlib(&json!({
                "filepath": locked.to_string_lossy(), "symbols": [symbol("A")],
            }));
            block_save(&locked, true);
            let save = server.call_write_schlib(&json!({
                "filepath": locked.to_string_lossy(), "symbols": [symbol("B")],
            }));
            block_save(&locked, false); // frees the squatted temp path
            assert!(save.is_error, "{}", get_result_text(&save));
            assert_eq!(parse_result_json(&save)["status"], "error");
        }

        // ---- write_libpkg -----------------------------------------------------

        #[test]
        fn write_libpkg_rejects_bad_requests() {
            let allowed = test_temp_dir();
            let outside = test_temp_dir();
            let server = create_test_server(allowed.path());

            let escaped = server.call_write_libpkg(&json!({
                "filepath": outside.path().join("P.LibPkg").to_string_lossy(),
                "documents": ["A.SchLib"],
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));

            let no_docs = server.call_write_libpkg(&json!({
                "filepath": allowed.path().join("P.LibPkg").to_string_lossy(),
            }));
            assert_error_mentions(&no_docs, "Missing required parameter: documents");

            // Present but carrying nothing usable: the array exists, so the
            // emptiness check is what rejects it.
            let empty_docs = server.call_write_libpkg(&json!({
                "filepath": allowed.path().join("P.LibPkg").to_string_lossy(),
                "documents": [],
            }));
            assert_error_mentions(&empty_docs, "at least one");
        }

        #[test]
        fn write_libpkg_reports_a_failed_write() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            // A directory occupying the target path: the extension check passes
            // and the write is what fails.
            let as_dir = dir.path().join("Blocked.LibPkg");
            std::fs::create_dir(&as_dir).unwrap();
            let r = server.call_write_libpkg(&json!({
                "filepath": as_dir.to_string_lossy(),
                "documents": ["A.SchLib"],
            }));
            assert_error_mentions(&r, "Failed to write LibPkg");
        }

        // ---- list_components / extract_style ---------------------------------

        #[test]
        fn list_components_reports_an_unreadable_schlib() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Corrupt.SchLib");
            write_garbage(&path);
            let r = server.call_list_components(&json!({ "filepath": path.to_string_lossy() }));
            assert!(r.is_error, "{}", get_result_text(&r));
            assert_eq!(parse_result_json(&r)["status"], "error");
        }

        #[test]
        fn extract_style_rejects_a_path_outside_the_allowlist() {
            let allowed = test_temp_dir();
            let outside = test_temp_dir();
            let server = create_test_server(allowed.path());
            let r = server.call_extract_style(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
            }));
            assert!(r.is_error, "{}", get_result_text(&r));
        }

        #[test]
        fn extract_style_pcblib_counts_the_layer_a_region_sits_on() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Region.PcbLib");
            let w = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [{
                    "name": "FP",
                    "pads": [pad("1")],
                    "regions": [{
                        "layer": "Mechanical 1",
                        "vertices": [
                            { "x": -1.0, "y": -1.0 }, { "x": 1.0, "y": -1.0 },
                            { "x": 1.0, "y": 1.0 }, { "x": -1.0, "y": 1.0 },
                        ],
                    }],
                }],
            }));
            assert!(!w.is_error, "{}", get_result_text(&w));

            let r = server.call_extract_style(&json!({ "filepath": path.to_string_lossy() }));
            let p = parse_result_json(&r);
            assert!(
                p["style"]["layers_used"].get("Mechanical 1").is_some(),
                "region layer missing from the tally: {p}"
            );
        }

        #[test]
        fn extract_style_schlib_reports_null_stats_for_a_bare_symbol() {
            // A symbol with no pins and no lines has nothing to average, and the
            // stats read null rather than a zero-count block.
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Bare.SchLib");
            let w = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [{ "name": "BARE" }],
            }));
            assert!(!w.is_error, "{}", get_result_text(&w));

            let r = server.call_extract_style(&json!({ "filepath": path.to_string_lossy() }));
            let p = parse_result_json(&r);
            assert!(p["style"]["pin_lengths"].is_null(), "{p}");
            assert!(p["style"]["line_widths"].is_null(), "{p}");
        }

        #[test]
        fn extract_style_reports_an_unreadable_schlib() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("Corrupt.SchLib");
            write_garbage(&path);
            let r = server.call_extract_style(&json!({ "filepath": path.to_string_lossy() }));
            assert!(r.is_error, "{}", get_result_text(&r));
            assert_eq!(parse_result_json(&r)["status"], "error");
        }
    }

    #[test]
    fn reading_a_schlib_outside_the_allowed_directories_is_refused() {
        use crate::mcp::tools::test_support::{
            create_test_schlib, create_test_server, get_result_text, test_temp_dir,
        };
        use serde_json::json;

        let dir = test_temp_dir();
        let other = test_temp_dir();
        let server = create_test_server(dir.path());

        let outside = other.path().join("Outside.SchLib");
        create_test_schlib(&outside);

        let r = server.call_read_schlib(&json!({ "filepath": outside.to_string_lossy() }));
        assert!(r.is_error);
        assert!(
            get_result_text(&r).contains("Access denied"),
            "{}",
            get_result_text(&r)
        );
    }
}
