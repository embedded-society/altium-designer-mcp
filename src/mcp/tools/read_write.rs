//! Read/write/list/style tools. Split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{ErrorContext, McpServer, ToolCallResult};

/// Maps a free-text component type to its reference-designator class letter,
/// following the conventions of IEEE 315 / ASME Y14.44 (commercial usage).
///
/// Used as the fallback when a symbol is written without an explicit
/// `designator_prefix`. Unknown or unspecified types resolve to `"U"`
/// (integrated circuit / inseparable assembly), the most common case.
// The explicit IC/regulator arm shares the `"U"` body with the wildcard
// fallback; it is kept to document the recognised IC synonyms rather than
// silently folding them into `_`.
#[allow(clippy::match_same_arms)]
fn ieee_designator_prefix(component_type: &str) -> &'static str {
    match component_type.trim().to_ascii_lowercase().as_str() {
        "resistor" | "res" | "potentiometer" | "pot" | "trimmer" | "rheostat" => "R",
        "resistor_network" | "resistor_array" | "network" => "RN",
        "thermistor" | "ntc" | "ptc" => "RT",
        "varistor" | "mov" => "RV",
        "capacitor" | "cap" => "C",
        "inductor" | "coil" | "choke" | "ferrite" | "ferrite_bead" | "bead" => "L",
        "diode" | "rectifier" | "schottky" | "zener" | "tvs" | "led" => "D",
        "display" | "lamp" | "indicator" | "lightbulb" => "DS",
        "transistor" | "mosfet" | "fet" | "bjt" | "igbt" | "jfet" => "Q",
        "ic" | "integrated_circuit" | "microcircuit" | "opamp" | "mcu" | "regulator"
        | "voltage_regulator" => "U",
        "connector" | "header" | "jack" | "receptacle" => "J",
        "plug" => "P",
        "socket" => "X",
        "crystal" | "oscillator" | "resonator" | "xtal" => "Y",
        "switch" | "button" | "pushbutton" | "dip_switch" | "dipswitch" => "S",
        "relay" | "contactor" => "K",
        "transformer" => "T",
        "fuse" => "F",
        "filter" => "FL",
        "battery" | "cell" => "BT",
        "test_point" | "testpoint" => "TP",
        "terminal_block" | "terminal" => "TB",
        "speaker" | "loudspeaker" | "buzzer" => "LS",
        "microphone" => "MK",
        "motor" | "fan" | "blower" => "B",
        "module" | "assembly" | "subassembly" => "A",
        "mechanical" | "standoff" | "screw" | "mounting" => "MP",
        "jumper" | "wire" | "cable" => "W",
        _ => "U",
    }
}

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

macro_rules! check_keys {
    ($json:expr, $keys:expr) => {
        if let Err(e) = McpServer::check_unknown_fields($json, $keys) {
            return crate::mcp::server::ToolCallResult::error(e);
        }
    };
}

impl McpServer {
    // ==================== Tool Handlers ====================

    /// Reads a `PcbLib` file and returns its contents.
    /// Supports pagination via limit/offset and filtering by `component_name`.
    #[allow(clippy::cast_possible_truncation)]
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
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);
        let offset = arguments
            .get("offset")
            .and_then(Value::as_u64)
            .map_or(0, |v| v as usize);

        // Parse compact parameter (default: true - omit redundant per-layer data)
        let compact = arguments
            .get("compact")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        match PcbLib::open(filepath) {
            Ok(library) => {
                let total_count = library.len();

                // Apply filtering and pagination
                let footprints: Vec<_> = library
                    .iter()
                    .filter(|fp| {
                        // If component_name specified, only include matching
                        component_name.map_or(true, |name| fp.name == name)
                    })
                    .skip(offset)
                    .take(limit.unwrap_or(usize::MAX))
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
                                                obj.insert(
                                                    "stack_mode".to_string(),
                                                    json!("simple"),
                                                );
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
                            "vias": fp.vias,
                            "tracks": fp.tracks,
                            "arcs": fp.arcs,
                            "regions": fp.regions,
                            "fills": fp.fills,
                            "text": fp.text,
                            "model_3d": fp.model_3d,
                            "component_bodies": fp.component_bodies,
                        })
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
        use crate::altium::pcblib::{Footprint, Model3D, PcbLib};

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

        // Check for duplicates within the new footprints
        {
            let mut seen = std::collections::HashSet::new();
            for name in &new_names {
                if !seen.insert(*name) {
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

        // Validate footprint names
        // Note: OLE storage names are limited to 31 characters, but the library layer
        // handles this by truncating storage names while preserving full names in PATTERN.
        #[allow(clippy::items_after_statements)]
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
        for name in &new_names {
            if name.is_empty() {
                return ToolCallResult::error("Footprint name cannot be empty");
            }
            if let Some(c) = name.chars().find(|c| INVALID_CHARS.contains(c)) {
                return ToolCallResult::error(format!(
                    "Footprint name '{name}' contains invalid character '{c}'. \
                     Names cannot contain: / \\ : * ? \" < > |",
                ));
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
            let existing_names: std::collections::HashSet<_> =
                library.names().into_iter().collect();
            for name in &new_names {
                if existing_names.contains(*name) {
                    return ToolCallResult::error(format!(
                        "Footprint '{name}' already exists in the library"
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

        for fp_json in footprints_json {
            check_keys!(
                fp_json,
                &[
                    "name",
                    "description",
                    "pads",
                    "tracks",
                    "arcs",
                    "regions",
                    "text",
                    "vias",
                    "fills",
                    "step_model",
                    "model_3d",
                    "component_bodies"
                ]
            );
            let name = fp_json
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unnamed");
            let mut footprint = Footprint::new(name);

            if let Some(desc) = fp_json.get("description").and_then(Value::as_str) {
                footprint.description = desc.to_string();
            }

            // Parse pads
            if let Some(pads) = fp_json.get("pads").and_then(Value::as_array) {
                for (i, pad_json) in pads.iter().enumerate() {
                    match Self::parse_pad(pad_json) {
                        Ok(pad) => footprint.add_pad(pad),
                        Err(e) => {
                            return ToolCallResult::error_with_context(
                                ErrorContext::new("write_pcblib", e)
                                    .with_filepath(filepath)
                                    .with_component(name)
                                    .with_details(format!("Failed to parse pad at index {i}")),
                            )
                        }
                    }
                }
            }

            // Parse tracks
            if let Some(tracks) = fp_json.get("tracks").and_then(Value::as_array) {
                for (i, track_json) in tracks.iter().enumerate() {
                    match Self::parse_track(track_json) {
                        Ok(track) => footprint.add_track(track),
                        Err(e) => {
                            return ToolCallResult::error_with_context(
                                ErrorContext::new("write_pcblib", e)
                                    .with_filepath(filepath)
                                    .with_component(name)
                                    .with_details(format!("Failed to parse track at index {i}")),
                            )
                        }
                    }
                }
            }

            // Parse vias
            if let Some(vias) = fp_json.get("vias").and_then(Value::as_array) {
                for (i, via_json) in vias.iter().enumerate() {
                    match Self::parse_via(via_json) {
                        Ok(via) => footprint.add_via(via),
                        Err(e) => {
                            return ToolCallResult::error_with_context(
                                ErrorContext::new("write_pcblib", e)
                                    .with_filepath(filepath)
                                    .with_component(name)
                                    .with_details(format!("Failed to parse via at index {i}")),
                            )
                        }
                    }
                }
            }

            // Parse fills
            if let Some(fills) = fp_json.get("fills").and_then(Value::as_array) {
                for (i, fill_json) in fills.iter().enumerate() {
                    match Self::parse_fill(fill_json) {
                        Ok(fill) => footprint.add_fill(fill),
                        Err(e) => {
                            return ToolCallResult::error_with_context(
                                ErrorContext::new("write_pcblib", e)
                                    .with_filepath(filepath)
                                    .with_component(name)
                                    .with_details(format!("Failed to parse fill at index {i}")),
                            )
                        }
                    }
                }
            }

            // Parse arcs
            if let Some(arcs) = fp_json.get("arcs").and_then(Value::as_array) {
                for (i, arc_json) in arcs.iter().enumerate() {
                    match Self::parse_arc(arc_json) {
                        Ok(arc) => footprint.add_arc(arc),
                        Err(e) => {
                            return ToolCallResult::error_with_context(
                                ErrorContext::new("write_pcblib", e)
                                    .with_filepath(filepath)
                                    .with_component(name)
                                    .with_details(format!("Failed to parse arc at index {i}")),
                            )
                        }
                    }
                }
            }

            // Parse regions
            if let Some(regions) = fp_json.get("regions").and_then(Value::as_array) {
                for region_json in regions {
                    check_keys!(
                        region_json,
                        &[
                            "layer",
                            "vertices",
                            "flags",
                            "kind",
                            "name",
                            "net_index",
                            "polygon_index",
                            "component_index",
                            "arc_resolution",
                            "cavity_height",
                            "sub_poly_index",
                            "union_index",
                            "is_shape_based",
                            "holes",
                            "unique_id",
                            "additional_parameters"
                        ]
                    );
                    if let Some(region) = Self::parse_region(region_json) {
                        footprint.add_region(region);
                    }
                }
            }

            // Parse text
            if let Some(texts) = fp_json.get("text").and_then(Value::as_array) {
                for text_json in texts {
                    check_keys!(
                        text_json,
                        &[
                            "bold",
                            "component_index",
                            "flags",
                            "font_name",
                            "height",
                            "inverted_border",
                            "inverted_rect_height",
                            "inverted_rect_text_offset",
                            "inverted_rect_width",
                            "is_comment",
                            "is_designator",
                            "is_inverted",
                            "italic",
                            "justification",
                            "kind",
                            "layer",
                            "mirror",
                            "net_index",
                            "polygon_index",
                            "rotation",
                            "stroke_font",
                            "stroke_width",
                            "text",
                            "unique_id",
                            "use_inverted_rectangle",
                            "x",
                            "y"
                        ]
                    );
                    if let Some(text) = Self::parse_text(text_json) {
                        footprint.add_text(text);
                    }
                }
            }

            // Parse 3D model
            if let Some(model_json) = fp_json.get("step_model") {
                if let Some(model_path) = model_json.get("filepath").and_then(Value::as_str) {
                    let embed = model_json
                        .get("embed")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);

                    if embed {
                        // The embed source is read from disk at save time
                        // (prepare_3d_models_for_writing -> std::fs::read), far from
                        // this handler. Validate it against the allow-list now so a
                        // caller cannot embed an arbitrary file (e.g. "../../etc/passwd")
                        // into the library. External references (embed=false) are only
                        // stored as a string and never read, so they are not gated here.
                        if let Err(e) = self.validate_path(model_path) {
                            return ToolCallResult::error(e);
                        }

                        // Embedded model - use Model3D which will read the file on save
                        footprint.model_3d = Some(Model3D {
                            filepath: model_path.to_string(),
                            x_offset: model_json
                                .get("x_offset")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                            y_offset: model_json
                                .get("y_offset")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                            z_offset: model_json
                                .get("z_offset")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                            rotation: model_json
                                .get("rotation")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                        });
                    } else {
                        // External reference only - create ComponentBody directly
                        // Preserve the full path for external references so organized subfolders work
                        use crate::altium::pcblib::{ComponentBody, Layer};
                        footprint.add_component_body(ComponentBody {
                            model_id: String::new(),            // No GUID for external reference
                            model_name: model_path.to_string(), // Preserve full path
                            embedded: false,
                            rotation_x: 0.0,
                            rotation_y: 0.0,
                            rotation_z: model_json
                                .get("rotation")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                            z_offset: model_json
                                .get("z_offset")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                            overall_height: 0.0,
                            standoff_height: 0.0,
                            layer: Layer::Top3DBody,
                            outline: Vec::new(),
                            unique_id: None,
                            model_checksum: 0, // External reference: no embedded model.
                            name: " ".to_string(),
                            kind: 0,
                            sub_poly_index: -1,
                            union_index: 0,
                            is_shape_based: false,
                            body_projection: 0,
                            body_color_3d: 8_421_504,
                            body_opacity_3d: 1.0,
                            model_2d_rotation: 0.0,
                            // External reference: no board association (free primitive).
                            net_index: 0xFFFF,
                            polygon_index: 0xFFFF,
                            component_index: -1,
                            additional_parameters: Vec::new(),
                        });
                    }
                }
            }

            // Parse "model_3d" — read_pcblib's spelling of the same model
            // reference (it emits the key for every footprint, null when there
            // is no model), accepted so a read result replays into
            // write_pcblib unchanged. `step_model` wins when both are given
            // (it is the authoring-time spelling, incl. the embed switch);
            // null is ignored. The fields mirror the Model3D serde shape
            // (filepath + offsets/rotation).
            if fp_json.get("step_model").is_none() {
                if let Some(model_json) = fp_json.get("model_3d").filter(|v| !v.is_null()) {
                    let model_path = model_json
                        .get("filepath")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    // The save path embeds the file (std::fs::read) when the
                    // path resolves to an existing file, so gate exactly that
                    // case against the allow-list — the same arbitrary-file-
                    // read defence as step_model. Bare model names replayed
                    // from read_pcblib output don't exist on disk and are kept
                    // as inert references, so they are not gated.
                    if std::path::Path::new(model_path).is_file() {
                        if let Err(e) = self.validate_path(model_path) {
                            return ToolCallResult::error(e);
                        }
                    }
                    footprint.model_3d = Some(Model3D {
                        filepath: model_path.to_string(),
                        x_offset: model_json
                            .get("x_offset")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        y_offset: model_json
                            .get("y_offset")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        z_offset: model_json
                            .get("z_offset")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        rotation: model_json
                            .get("rotation")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                    });
                }
            }

            // Parse generic extruded 3D bodies (no STEP model). Each body is
            // defined by an optional 2D outline (auto-bounding-box from pads when
            // omitted) plus standoff/overall heights, on the Top/Bottom 3D Body
            // layer. model_id/model_name stay empty so the writer marks them as
            // shape-based extruded bodies.
            if let Some(bodies) = fp_json.get("component_bodies").and_then(Value::as_array) {
                for body_json in bodies {
                    footprint.add_component_body(Self::parse_component_body_json(body_json));
                }
            }

            // Auto-inject the `.Designator` special string on the Top Overlay if the
            // caller did not provide one, so every placed footprint renders its
            // reference designator. Placed just above the topmost pad (or at the
            // origin when there are no pads); the user can reposition in Altium.
            let has_designator = footprint
                .text
                .iter()
                .any(|t| t.text.trim().eq_ignore_ascii_case(".designator"));
            if !has_designator {
                use crate::altium::pcblib::{Layer, PcbFlags, Text, TextJustification, TextKind};
                let top = footprint
                    .pads
                    .iter()
                    .map(|p| p.y + p.height / 2.0)
                    .fold(f64::NEG_INFINITY, f64::max);
                let y = if top.is_finite() { top + 0.6 } else { 0.0 };
                footprint.add_text(Text {
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
                    model_name: String::new(),
                    embedded: false,
                    rotation_x: 0.0,
                    rotation_y: 0.0,
                    rotation_z: 0.0,
                    z_offset: 0.0,
                    overall_height: 1.0,
                    standoff_height: 0.0,
                    layer: Layer::Top3DBody,
                    outline: Vec::new(),
                    unique_id: None,
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
                    // Synthesised body: no board association (free primitive).
                    net_index: 0xFFFF,
                    polygon_index: 0xFFFF,
                    component_index: -1,
                    additional_parameters: Vec::new(),
                });
                true
            } else {
                false
            };
            bodies_echo.push(body_3d_summary(&footprint, assumed_height));

            // Validate coordinates before adding
            if let Err(e) = Self::validate_footprint_coordinates(&footprint) {
                return ToolCallResult::error(e);
            }

            silk_warnings.extend(silk_over_pad_warnings(&footprint));

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
    #[allow(clippy::cast_possible_truncation)]
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
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);
        let offset = arguments
            .get("offset")
            .and_then(Value::as_u64)
            .map_or(0, |v| v as usize);

        match SchLib::open(filepath) {
            Ok(library) => {
                let total_count = library.len();

                // Apply filtering and pagination
                let symbols: Vec<_> = library
                    .iter()
                    .filter(|symbol| {
                        // If component_name specified, only include matching
                        component_name.map_or(true, |filter| symbol.name == filter)
                    })
                    .skip(offset)
                    .take(limit.unwrap_or(usize::MAX))
                    .map(|symbol| {
                        json!({
                            "name": symbol.name,
                            "description": symbol.description,
                            "designator": symbol.designator,
                            "designator_x": symbol.designator_x,
                            "designator_y": symbol.designator_y,
                            "designator_unique_id": symbol.designator_unique_id,
                            "part_count": symbol.part_count,
                            "pins": symbol.pins,
                            "rectangles": symbol.rectangles,
                            "round_rects": symbol.round_rects,
                            "lines": symbol.lines,
                            "polylines": symbol.polylines,
                            "polygons": symbol.polygons,
                            "arcs": symbol.arcs,
                            "pies": symbol.pies,
                            "images": symbol.images,
                            "text_frames": symbol.text_frames,
                            "beziers": symbol.beziers,
                            "ellipses": symbol.ellipses,
                            "elliptical_arcs": symbol.elliptical_arcs,
                            "labels": symbol.labels,
                            "text": symbol.text,
                            "parameters": symbol.parameters,
                            "footprints": symbol.footprints,
                        })
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
        use crate::altium::schlib::{FootprintModel, SchLib, Symbol};

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

        // Check for duplicates within the new symbols
        {
            let mut seen = std::collections::HashSet::new();
            for name in &new_names {
                if !seen.insert(*name) {
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

        // Validate symbol names
        // Note: OLE storage names are limited to 31 characters, but the library layer
        // handles this by truncating storage names while preserving full names in LIBREFERENCE.
        #[allow(clippy::items_after_statements)]
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
        for name in &new_names {
            if name.is_empty() {
                return ToolCallResult::error("Symbol name cannot be empty");
            }
            if let Some(c) = name.chars().find(|c| INVALID_CHARS.contains(c)) {
                return ToolCallResult::error(format!(
                    "Symbol name '{name}' contains invalid character '{c}'. \
                     Names cannot contain: / \\ : * ? \" < > |",
                ));
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
                if library.get(name).is_some() {
                    return ToolCallResult::error(format!(
                        "Symbol '{name}' already exists in the library"
                    ));
                }
            }
        }

        for sym_json in symbols_json {
            check_keys!(
                sym_json,
                &[
                    "name",
                    "description",
                    "designator",
                    "designator_prefix",
                    "designator_x",
                    "designator_y",
                    "designator_unique_id",
                    "component_type",
                    "part_count",
                    "display_mode_count",
                    "current_part_id",
                    "part_id_locked",
                    "source_library_name",
                    "target_file_name",
                    "pins",
                    "rectangles",
                    "round_rects",
                    "lines",
                    "polylines",
                    "polygons",
                    "arcs",
                    "pies",
                    "images",
                    "text_frames",
                    "beziers",
                    "ellipses",
                    "elliptical_arcs",
                    "labels",
                    "text",
                    "parameters",
                    "footprints"
                ]
            );
            let name = sym_json
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unnamed");
            let mut symbol = Symbol::new(name);

            if let Some(desc) = sym_json.get("description").and_then(Value::as_str) {
                symbol.description = desc.to_string();
            }

            // Always assign a reference designator. Precedence:
            //   1. explicit `designator`
            //   2. explicit `designator_prefix`
            //   3. `component_type` mapped via IEEE 315 / ASME Y14.44 table
            //   4. fallback "U" (integrated circuit)
            // so every symbol carries a `<prefix>?` designator in the SchLib.
            let designator = sym_json
                .get("designator")
                .and_then(Value::as_str)
                .map_or_else(
                    || {
                        let prefix = sym_json
                            .get("designator_prefix")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .or_else(|| {
                                sym_json
                                    .get("component_type")
                                    .and_then(Value::as_str)
                                    .map(|t| ieee_designator_prefix(t).to_string())
                            })
                            .unwrap_or_else(|| "U".to_string());
                        format!("{prefix}?")
                    },
                    str::to_string,
                );
            symbol.designator = designator;

            // Designator text position (RECORD=34 Location.X/Y) and identity.
            // Defaults -5/5 per the AD24 golden; the unique id is reused when
            // supplied (e.g. a read-modify-write) so the record is deterministic.
            if let Some(x) = sym_json.get("designator_x").and_then(Value::as_f64) {
                symbol.designator_x = x;
            }
            if let Some(y) = sym_json.get("designator_y").and_then(Value::as_f64) {
                symbol.designator_y = y;
            }
            if let Some(uid) = sym_json.get("designator_unique_id").and_then(Value::as_str) {
                symbol.designator_unique_id = Some(uid.to_string());
            }

            // Parse part_count for multi-part symbols (e.g., dual op-amp)
            if let Some(part_count) = sym_json.get("part_count").and_then(Value::as_u64) {
                #[allow(clippy::cast_possible_truncation)]
                {
                    symbol.part_count = part_count.clamp(1, 255) as u32;
                }
            }

            // Parse the remaining symbol header fields (mirrors
            // update_schlib_component): export_schlib emits them, so an
            // export -> write_schlib round-trip must not reset them to
            // defaults (e.g. collapsing a two-display-mode symbol to one).
            if let Some(v) = sym_json.get("display_mode_count").and_then(Value::as_u64) {
                symbol.display_mode_count = u32::try_from(v).unwrap_or(symbol.display_mode_count);
            }
            if let Some(v) = sym_json.get("current_part_id").and_then(Value::as_u64) {
                symbol.current_part_id = u32::try_from(v).unwrap_or(symbol.current_part_id);
            }
            if let Some(v) = sym_json.get("part_id_locked").and_then(Value::as_bool) {
                symbol.part_id_locked = v;
            }
            if let Some(v) = sym_json.get("source_library_name").and_then(Value::as_str) {
                symbol.source_library_name = v.to_string();
            }
            if let Some(v) = sym_json.get("target_file_name").and_then(Value::as_str) {
                symbol.target_file_name = v.to_string();
            }

            // Parse pins
            if let Some(pins) = sym_json.get("pins").and_then(Value::as_array) {
                for pin_json in pins {
                    check_keys!(
                        pin_json,
                        &[
                            "name",
                            "designator",
                            "x",
                            "y",
                            "length",
                            "orientation",
                            "electrical_type",
                            "hidden",
                            "show_name",
                            "show_designator",
                            "owner_part_id",
                            "symbol_inner_edge",
                            "symbol_outer_edge",
                            "symbol_inside",
                            "symbol_outside",
                            "description",
                            "colour",
                            "graphically_locked",
                            "swap_id_group",
                            "part_and_sequence",
                            "default_value",
                            "owner_part_display_mode",
                            "symbol_line_width",
                            "frac",
                            "is_not_accessible",
                            "formal_type"
                        ]
                    );
                    if let Some(pin) = Self::parse_schlib_pin(pin_json) {
                        symbol.add_pin(pin);
                    }
                }
            }

            // Parse rectangles
            if let Some(rects) = sym_json.get("rectangles").and_then(Value::as_array) {
                for rect_json in rects {
                    check_keys!(
                        rect_json,
                        &[
                            "fill_color",
                            "filled",
                            "line_color",
                            "line_style",
                            "line_width",
                            "owner_part_id",
                            "transparent",
                            "x1",
                            "x2",
                            "y1",
                            "y2",
                            "unique_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(rect) = Self::parse_schlib_rectangle(rect_json) {
                        symbol.add_rectangle(rect);
                    }
                }
            }

            // Parse rounded rectangles
            if let Some(round_rects) = sym_json.get("round_rects").and_then(Value::as_array) {
                for round_rect_json in round_rects {
                    check_keys!(
                        round_rect_json,
                        &[
                            "corner_x_radius",
                            "corner_y_radius",
                            "fill_color",
                            "filled",
                            "line_color",
                            "line_style",
                            "line_width",
                            "owner_part_id",
                            "transparent",
                            "x1",
                            "x2",
                            "y1",
                            "y2",
                            "unique_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(round_rect) = Self::parse_schlib_round_rect(round_rect_json) {
                        symbol.add_round_rect(round_rect);
                    }
                }
            }

            // Parse lines
            if let Some(lines) = sym_json.get("lines").and_then(Value::as_array) {
                for line_json in lines {
                    check_keys!(
                        line_json,
                        &[
                            "color",
                            "line_style",
                            "line_width",
                            "is_not_accessible",
                            "owner_part_id",
                            "x1",
                            "x2",
                            "y1",
                            "y2",
                            "unique_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(line) = Self::parse_schlib_line(line_json) {
                        symbol.add_line(line);
                    }
                }
            }

            // Parse polylines
            if let Some(polylines) = sym_json.get("polylines").and_then(Value::as_array) {
                for polyline_json in polylines {
                    check_keys!(
                        polyline_json,
                        &[
                            "color",
                            "end_line_shape",
                            "line_shape_size",
                            "line_style",
                            "line_width",
                            "is_not_accessible",
                            "owner_part_id",
                            "points",
                            "start_line_shape",
                            "transparent",
                            "vertices",
                            "unique_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(polyline) = Self::parse_schlib_polyline(polyline_json) {
                        symbol.add_polyline(polyline);
                    }
                }
            }

            // Parse polygons
            if let Some(polygons) = sym_json.get("polygons").and_then(Value::as_array) {
                for polygon_json in polygons {
                    check_keys!(
                        polygon_json,
                        &[
                            "fill_color",
                            "filled",
                            "line_color",
                            "line_width",
                            "line_style",
                            "transparent",
                            "is_not_accessible",
                            "owner_part_id",
                            "points",
                            "vertices",
                            "unique_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(polygon) = Self::parse_schlib_polygon(polygon_json) {
                        symbol.add_polygon(polygon);
                    }
                }
            }

            // Parse arcs
            if let Some(arcs) = sym_json.get("arcs").and_then(Value::as_array) {
                for arc_json in arcs {
                    // SchLib arcs are centre/radius/angle based, NOT layer-based like PcbLib arcs; the
                    // allow-list must match the documented fields in tool_definitions or every arc is
                    // rejected as an "unknown field" (was erroneously copied from the PcbLib arc as ["layer"]).
                    check_keys!(
                        arc_json,
                        &[
                            "color",
                            "end_angle",
                            "fill_color",
                            "line_width",
                            "is_not_accessible",
                            "owner_part_id",
                            "radius",
                            "start_angle",
                            "x",
                            "y",
                            "unique_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(arc) = Self::parse_schlib_arc(arc_json) {
                        symbol.add_arc(arc);
                    }
                }
            }

            if let Some(pies) = sym_json.get("pies").and_then(Value::as_array) {
                for pie_json in pies {
                    check_keys!(
                        pie_json,
                        &[
                            "x",
                            "y",
                            "radius",
                            "start_angle",
                            "end_angle",
                            "line_width",
                            "line_color",
                            "fill_color",
                            "filled",
                            "transparent",
                            "is_not_accessible",
                            "owner_part_id",
                            "unique_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(pie) = Self::parse_schlib_pie(pie_json) {
                        symbol.add_pie(pie);
                    }
                }
            }

            if let Some(images) = sym_json.get("images").and_then(Value::as_array) {
                for image_json in images {
                    check_keys!(
                        image_json,
                        &[
                            "x1",
                            "y1",
                            "x2",
                            "y2",
                            "line_width",
                            "line_color",
                            "line_style",
                            "fill_color",
                            "filled",
                            "transparent",
                            "show_border",
                            "keep_aspect",
                            "embed_image",
                            "file_name",
                            "image_data",
                            "is_not_accessible",
                            "owner_part_id",
                            "unique_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(image) = Self::parse_schlib_image(image_json) {
                        symbol.add_image(image);
                    }
                }
            }

            if let Some(text_frames) = sym_json.get("text_frames").and_then(Value::as_array) {
                for frame_json in text_frames {
                    check_keys!(
                        frame_json,
                        &[
                            "x1",
                            "y1",
                            "x2",
                            "y2",
                            "text",
                            "color",
                            "area_color",
                            "text_color",
                            "text_margin",
                            "line_width",
                            "line_style",
                            "transparent",
                            "font_id",
                            "orientation",
                            "alignment",
                            "is_solid",
                            "show_border",
                            "word_wrap",
                            "clip_to_rect",
                            "is_not_accessible",
                            "owner_part_id",
                            "unique_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(text_frame) = Self::parse_schlib_text_frame(frame_json) {
                        symbol.add_text_frame(text_frame);
                    }
                }
            }

            if let Some(beziers) = sym_json.get("beziers").and_then(Value::as_array) {
                for bezier_json in beziers {
                    check_keys!(
                        bezier_json,
                        &[
                            "x1",
                            "y1",
                            "x2",
                            "y2",
                            "x3",
                            "y3",
                            "x4",
                            "y4",
                            "line_width",
                            "color",
                            "is_not_accessible",
                            "owner_part_id",
                            "unique_id"
                        ]
                    );
                    if let Some(bezier) = Self::parse_schlib_bezier(bezier_json) {
                        symbol.add_bezier(bezier);
                    }
                }
            }

            if let Some(ell_arcs) = sym_json.get("elliptical_arcs").and_then(Value::as_array) {
                for ell_arc_json in ell_arcs {
                    check_keys!(
                        ell_arc_json,
                        &[
                            "x",
                            "y",
                            "radius",
                            "secondary_radius",
                            "start_angle",
                            "end_angle",
                            "line_width",
                            "color",
                            "fill_color",
                            "owner_part_id",
                            "unique_id"
                        ]
                    );
                    if let Some(ell_arc) = Self::parse_schlib_elliptical_arc(ell_arc_json) {
                        symbol.add_elliptical_arc(ell_arc);
                    }
                }
            }

            // Parse ellipses
            if let Some(ellipses) = sym_json.get("ellipses").and_then(Value::as_array) {
                for ellipse_json in ellipses {
                    check_keys!(
                        ellipse_json,
                        &[
                            "fill_color",
                            "filled",
                            "line_color",
                            "line_width",
                            "is_not_accessible",
                            "owner_part_id",
                            "radius_x",
                            "radius_y",
                            "transparent",
                            "x",
                            "y",
                            "unique_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(ellipse) = Self::parse_schlib_ellipse(ellipse_json) {
                        symbol.add_ellipse(ellipse);
                    }
                }
            }

            // Parse labels
            if let Some(labels) = sym_json.get("labels").and_then(Value::as_array) {
                for label_json in labels {
                    check_keys!(
                        label_json,
                        &[
                            "color",
                            "font_id",
                            "hidden",
                            "is_hidden",
                            "is_mirrored",
                            "justification",
                            "owner_part_id",
                            "rotation",
                            "text",
                            "x",
                            "y",
                            "unique_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(label) = Self::parse_schlib_label(label_json) {
                        symbol.add_label(label);
                    }
                }
            }

            // Parse text annotations
            if let Some(texts) = sym_json.get("text").and_then(Value::as_array) {
                for text_json in texts {
                    check_keys!(
                        text_json,
                        &[
                            "color",
                            "font_id",
                            "hidden",
                            "is_hidden",
                            "is_mirrored",
                            "justification",
                            "owner_part_id",
                            "rotation",
                            "text",
                            "x",
                            "y",
                            "unique_id"
                        ]
                    );
                    if let Some(text) = Self::parse_schlib_text(text_json) {
                        symbol.add_text(text);
                    }
                }
            }

            // Parse parameters
            if let Some(params) = sym_json.get("parameters").and_then(Value::as_array) {
                for param_json in params {
                    check_keys!(
                        param_json,
                        &[
                            "name",
                            "value",
                            "x",
                            "y",
                            "hidden",
                            "font_id",
                            "color",
                            "read_only_state",
                            "param_type",
                            "unique_id",
                            "orientation",
                            "justification",
                            "show_name",
                            "hide_name",
                            "description",
                            "is_configurable",
                            "owner_part_id",
                            "graphically_locked",
                            "disabled",
                            "dimmed",
                            "owner_part_display_mode"
                        ]
                    );
                    if let Some(param) = Self::parse_schlib_parameter(param_json) {
                        symbol.add_parameter(param);
                    }
                }
            }

            // Parse footprint references
            if let Some(footprints) = sym_json.get("footprints").and_then(Value::as_array) {
                for fp_json in footprints {
                    // A footprint reference is a model link (name + optional
                    // description + library_path), not an embedded footprint, so
                    // only those fields are read here.
                    check_keys!(fp_json, &["name", "description", "library_path"]);
                    if let Some(fp_name) = fp_json.get("name").and_then(Value::as_str) {
                        let mut fp = FootprintModel::new(fp_name);
                        if let Some(desc) = fp_json.get("description").and_then(Value::as_str) {
                            fp.description = desc.to_string();
                        }
                        // Optional PcbLib path -> ModelDatafile0, so Altium can
                        // resolve the footprint instead of reporting "not found".
                        if let Some(lib_path) = fp_json.get("library_path").and_then(Value::as_str)
                        {
                            fp.library_path = Some(lib_path.to_string());
                        }
                        symbol.add_footprint(fp);
                    }
                }
            }

            // Validate coordinates before adding
            if let Err(e) = Self::validate_symbol_coordinates(&symbol) {
                return ToolCallResult::error(e);
            }

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
                result["geometry"] = Value::Array(library.iter().map(symbol_geometry).collect());

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
    #[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
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
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);
        let offset = arguments
            .get("offset")
            .and_then(Value::as_u64)
            .map_or(0, |v| v as usize);

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
                                json!({
                                    "name": fp.name,
                                    "description": fp.description,
                                    "pad_count": fp.pads.len(),
                                    "track_count": fp.tracks.len(),
                                    "arc_count": fp.arcs.len(),
                                    "region_count": fp.regions.len(),
                                    "text_count": fp.text.len(),
                                    "has_3d_model": fp.model_3d.is_some() || !fp.component_bodies.is_empty(),
                                })
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
                    "error": "Unknown file type. Expected .PcbLib or .SchLib extension.",
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
                    "error": "Unknown file type. Expected .PcbLib or .SchLib extension.",
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Extracts style from a `PcbLib` file.
    pub(crate) fn extract_pcblib_style(filepath: &str) -> ToolCallResult {
        use crate::altium::PcbLib;
        use std::collections::HashMap;

        match PcbLib::open(filepath) {
            Ok(library) => {
                // Track widths by layer
                let mut track_widths: HashMap<String, Vec<f64>> = HashMap::new();
                // Pad shapes count
                let mut pad_shapes: HashMap<String, usize> = HashMap::new();
                // Text heights
                let mut text_heights: Vec<f64> = Vec::new();
                // Layers used
                let mut layers_used: HashMap<String, usize> = HashMap::new();

                for fp in library.iter() {
                    // Analyse tracks
                    for track in &fp.tracks {
                        let layer_name = track.layer.as_str().to_string();
                        track_widths
                            .entry(layer_name.clone())
                            .or_default()
                            .push(track.width);
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }

                    // Analyse pads
                    for pad in &fp.pads {
                        let shape_name = format!("{:?}", pad.shape);
                        *pad_shapes.entry(shape_name).or_insert(0) += 1;
                        let layer_name = pad.layer.as_str().to_string();
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }

                    // Analyse text
                    for text in &fp.text {
                        text_heights.push(text.height);
                        let layer_name = text.layer.as_str().to_string();
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }

                    // Analyse regions
                    for region in &fp.regions {
                        let layer_name = region.layer.as_str().to_string();
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }
                }

                // Calculate statistics for track widths
                #[allow(clippy::cast_precision_loss)]
                let track_width_stats: HashMap<String, Value> = track_widths
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
                    .collect();

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

    /// Extracts style from a `SchLib` file.
    pub(crate) fn extract_schlib_style(filepath: &str) -> ToolCallResult {
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
                // Rectangle stats
                let mut rect_filled_count = 0usize;
                let mut rect_unfilled_count = 0usize;

                for symbol in library.iter() {
                    // Analyse pins
                    for pin in &symbol.pins {
                        pin_lengths.push(pin.length);
                    }

                    // Analyse rectangles
                    for rect in &symbol.rectangles {
                        line_widths.push(rect.line_width);
                        let line_color = format!("#{:06X}", rect.line_color);
                        let fill_color = format!("#{:06X}", rect.fill_color);
                        *line_colors.entry(line_color).or_insert(0) += 1;
                        *fill_colors.entry(fill_color).or_insert(0) += 1;
                        if rect.filled {
                            rect_filled_count += 1;
                        } else {
                            rect_unfilled_count += 1;
                        }
                    }

                    // Analyse lines
                    for line in &symbol.lines {
                        line_widths.push(line.line_width);
                        let color = format!("#{:06X}", line.color);
                        *line_colors.entry(color).or_insert(0) += 1;
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
    use super::ieee_designator_prefix;
    use super::{body_3d_summary, pin_tip, symbol_geometry};
    use crate::altium::schlib::{Pin, PinOrientation, Rectangle, Symbol};

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
            model_id: String::new(),
            model_name: name.to_string(),
            embedded: false,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            z_offset: 0.0,
            overall_height: h,
            standoff_height: 0.0,
            layer: Layer::Top3DBody,
            outline: Vec::new(),
            unique_id: None,
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
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            additional_parameters: Vec::new(),
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

    // ==================== extract_style ====================

    mod extract_style {
        use crate::altium::pcblib::{Footprint, Layer, Pad, PcbLib, Track};
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
            assert!(get_result_text(&result).contains("Unknown file type"));

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
}
