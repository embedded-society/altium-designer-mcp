//! JSON -> Altium primitive parsing helpers, split from `server.rs`.
//!
//! These extend `McpServer` with an additional `impl` block; the handlers in
//! other modules call them via `Self::parse_*` unchanged.

use serde_json::Value;

use crate::mcp::server::McpServer;

/// Reads a JSON integer field as `i32`, returning `None` if it is missing, not
/// an integer, or outside `i32` range — so an out-of-range value is rejected
/// rather than silently wrapped (`as i32`), which previously let an absurd input
/// land as a small in-range coordinate that bypassed range validation.
fn json_i32(json: &Value, field: &str) -> Option<i32> {
    json.get(field)
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
}

/// Reads a JSON number field as `f64`, accepting both integer and fractional
/// JSON values and rejecting non-finite (NaN/∞) inputs. Schematic graphic
/// coordinates use this so off-grid (fractional) positions survive instead of
/// being dropped by the integer-only [`json_i32`].
fn json_f64(json: &Value, field: &str) -> Option<f64> {
    json.get(field)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
}

/// Reads the optional `flags` field of a `PcbLib` 2D primitive.
///
/// `read_pcblib` serialises [`crate::altium::pcblib::PcbFlags`] (a `bitflags`
/// set) via its serde impl, which in JSON is a string of `|`-separated flag
/// names, e.g. `"LOCKED"` or `"LOCKED | KEEPOUT"`. The write side deserialises
/// that exact form with serde so a value read from disk round-trips unchanged.
/// For caller convenience a raw `u16` bitmask integer is also accepted
/// (`1` = `LOCKED`, `4` = `KEEPOUT`, …). An absent or unparseable value yields
/// the empty flag set rather than erroring, matching the lenient handling of the
/// other optional tail fields.
fn json_flags(json: &Value) -> crate::altium::pcblib::PcbFlags {
    use crate::altium::pcblib::PcbFlags;
    match json.get("flags") {
        // Canonical round-trip shape: the bitflags serde string ("LOCKED | …").
        Some(v @ Value::String(_)) => {
            serde_json::from_value(v.clone()).unwrap_or_else(|_| PcbFlags::empty())
        }
        // Convenience shape: a raw bitmask integer.
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|v| u16::try_from(v).ok())
            .map_or_else(PcbFlags::empty, PcbFlags::from_bits_truncate),
        _ => PcbFlags::empty(),
    }
}

/// Reads the optional `keepout_restrictions` bitmask (`u8`) of a `PcbLib` 2D
/// primitive, mirroring how `read_pcblib` serialises the `Option<u8>` field.
fn json_keepout(json: &Value) -> Option<u8> {
    json.get("keepout_restrictions")
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok())
}

impl McpServer {
    // ==================== Primitive Parsing Helpers ====================

    pub(crate) fn check_unknown_fields(
        json: &serde_json::Value,
        allowed_keys: &[&str],
    ) -> Result<(), String> {
        if let Some(obj) = json.as_object() {
            for key in obj.keys() {
                if !allowed_keys.contains(&key.as_str()) {
                    return Err(format!(
                        "Unknown field '{key}'. Allowed fields are: {allowed_keys:?}"
                    ));
                }
            }
        }
        Ok(())
    }

    /// Parses a pad from JSON.
    #[allow(clippy::too_many_lines)] // Pad has many fields requiring individual parsing
    pub(crate) fn parse_pad(json: &Value) -> Result<crate::altium::pcblib::Pad, String> {
        use crate::altium::pcblib::{Layer, MaskExpansionMode, Pad, PadShape, PadStackMode};

        let designator = json
            .get("designator")
            .and_then(Value::as_str)
            .ok_or("Pad missing required field 'designator'")?;

        // Validate designator is not empty
        if designator.trim().is_empty() {
            return Err("Pad designator cannot be empty".to_string());
        }

        let x = json
            .get("x")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'x'")?;
        let y = json
            .get("y")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'y'")?;
        let width = json
            .get("width")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'width'")?;
        let height = json
            .get("height")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'height'")?;

        // Validate pad dimensions are positive
        if width <= 0.0 {
            return Err(format!(
                "Pad '{designator}' has invalid width {width}. Width must be greater than 0."
            ));
        }
        if height <= 0.0 {
            return Err(format!(
                "Pad '{designator}' has invalid height {height}. Height must be greater than 0."
            ));
        }

        let shape_str = json
            .get("shape")
            .and_then(Value::as_str)
            .unwrap_or("rounded_rectangle");
        let shape = match shape_str {
            "rectangle" => PadShape::Rectangle,
            "round" | "circle" => PadShape::Round,
            "oval" => PadShape::Oval,
            "octagonal" => PadShape::Octagonal,
            "rounded_rectangle" => PadShape::RoundedRectangle,
            _ => {
                return Err(format!(
                    "Pad '{designator}' has invalid shape '{shape_str}'. \
                     Valid shapes are: rectangle, round, circle, oval, octagonal, rounded_rectangle"
                ))
            }
        };

        // Parse hole_size first to determine default layer
        let hole_size = json.get("hole_size").and_then(Value::as_f64);
        let is_smd = hole_size.map_or(true, |h| h <= 0.0); // SMD if no hole or hole size <= 0

        let layer_str = json.get("layer").and_then(Value::as_str);
        let layer = match layer_str {
            Some(s) => Layer::parse(s).ok_or_else(|| {
                format!(
                    "Pad '{designator}' has invalid layer '{s}'. \
                     Valid layers include: Top Layer, Bottom Layer, Multi-Layer, Top Overlay, etc."
                )
            })?,
            // SMD pads default to Top Layer, through-hole pads default to Multi-Layer
            None => {
                if is_smd {
                    Layer::TopLayer
                } else {
                    Layer::MultiLayer
                }
            }
        };
        let rotation = json.get("rotation").and_then(Value::as_f64).unwrap_or(0.0);

        // Parse optional hole shape
        let hole_shape = json
            .get("hole_shape")
            .and_then(Value::as_str)
            .map(|s| match s.to_lowercase().as_str() {
                "square" => crate::altium::pcblib::HoleShape::Square,
                "slot" => crate::altium::pcblib::HoleShape::Slot,
                _ => crate::altium::pcblib::HoleShape::Round,
            })
            .unwrap_or_default();

        // Parse optional mask expansion values
        let paste_mask_expansion = json.get("paste_mask_expansion").and_then(Value::as_f64);
        let solder_mask_expansion = json.get("solder_mask_expansion").and_then(Value::as_f64);
        let paste_mask_expansion_mode = json
            .get("paste_mask_expansion_mode")
            .and_then(Value::as_str)
            .map(|s| match s.to_lowercase().as_str() {
                "none" => MaskExpansionMode::None,
                "manual" => MaskExpansionMode::Manual,
                _ => MaskExpansionMode::FromRule,
            })
            .unwrap_or_default();
        let solder_mask_expansion_mode = json
            .get("solder_mask_expansion_mode")
            .and_then(Value::as_str)
            .map(|s| match s.to_lowercase().as_str() {
                "none" => MaskExpansionMode::None,
                "manual" => MaskExpansionMode::Manual,
                _ => MaskExpansionMode::FromRule,
            })
            .unwrap_or_default();

        // Parse optional corner radius
        let corner_radius_percent = json
            .get("corner_radius_percent")
            .and_then(Value::as_u64)
            .and_then(|v| u8::try_from(v).ok())
            .filter(|&v| v <= 100);

        Ok(Pad {
            designator: designator.to_string(),
            x,
            y,
            width,
            height,
            shape,
            layer,
            hole_size,
            hole_shape,
            rotation,
            paste_mask_expansion,
            solder_mask_expansion,
            paste_mask_expansion_mode,
            solder_mask_expansion_mode,
            corner_radius_percent,
            stack_mode: PadStackMode::Simple,
            per_layer_sizes: None,
            per_layer_shapes: None,
            per_layer_corner_radii: None,
            per_layer_offsets: None,
            flags: json_flags(json),
            unique_id: None,
        })
    }

    /// Parses a track from JSON.
    pub(crate) fn parse_track(json: &Value) -> Result<crate::altium::pcblib::Track, String> {
        use crate::altium::pcblib::{Layer, Track};

        let x1 = json
            .get("x1")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'x1'")?;
        let y1 = json
            .get("y1")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'y1'")?;
        let x2 = json
            .get("x2")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'x2'")?;
        let y2 = json
            .get("y2")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'y2'")?;
        let width = json
            .get("width")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'width'")?;

        let layer_str = json.get("layer").and_then(Value::as_str);
        let layer = match layer_str {
            Some(s) => Layer::parse(s).ok_or_else(|| {
                format!(
                    "Track has invalid layer '{s}'. \
                     Valid layers include: Top Layer, Bottom Layer, Top Overlay, Top Assembly, etc."
                )
            })?,
            None => Layer::TopOverlay, // Default for tracks is Top Overlay
        };

        let mut track = Track::new(x1, y1, x2, y2, width, layer);
        // Optional EE tail (mirrors the modelled optionals; absent keys keep the
        // `Track::new` defaults so a from-scratch track is byte-identical).
        track.flags = json_flags(json);
        track.solder_mask_expansion = json_f64(json, "solder_mask_expansion");
        track.keepout_restrictions = json_keepout(json);
        Ok(track)
    }

    /// Parses an arc from JSON.
    pub(crate) fn parse_arc(json: &Value) -> Result<crate::altium::pcblib::Arc, String> {
        use crate::altium::pcblib::{Arc, Layer};

        let x = json
            .get("x")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'x'")?;
        let y = json
            .get("y")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'y'")?;
        let radius = json
            .get("radius")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'radius'")?;
        let start_angle = json
            .get("start_angle")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'start_angle'")?;
        let end_angle = json
            .get("end_angle")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'end_angle'")?;
        let width = json
            .get("width")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'width'")?;

        let layer_str = json.get("layer").and_then(Value::as_str);
        let layer = match layer_str {
            Some(s) => Layer::parse(s).ok_or_else(|| {
                format!(
                    "Arc has invalid layer '{s}'. \
                     Valid layers include: Top Layer, Bottom Layer, Top Overlay, Top Assembly, etc."
                )
            })?,
            None => Layer::TopOverlay, // Default for arcs is Top Overlay
        };

        Ok(Arc {
            x,
            y,
            radius,
            start_angle,
            end_angle,
            width,
            layer,
            flags: json_flags(json),
            unique_id: None,
            // Optional EE tail (mirrors the modelled optionals; absent keys keep
            // the default `None` so a from-scratch arc is byte-identical).
            solder_mask_expansion: json_f64(json, "solder_mask_expansion"),
            keepout_restrictions: json_keepout(json),
        })
    }

    /// Parses a region from JSON.
    pub(crate) fn parse_region(json: &Value) -> Option<crate::altium::pcblib::Region> {
        use crate::altium::pcblib::{Layer, Region, Vertex};

        let vertices_json = json.get("vertices").and_then(Value::as_array)?;
        let layer = json
            .get("layer")
            .and_then(Value::as_str)
            .and_then(Layer::parse)
            .unwrap_or(Layer::Mechanical15);

        let vertices: Vec<Vertex> = vertices_json
            .iter()
            .filter_map(|v| {
                let x = v.get("x").and_then(Value::as_f64)?;
                let y = v.get("y").and_then(Value::as_f64)?;
                Some(Vertex { x, y })
            })
            .collect();

        if vertices.len() < 3 {
            return None; // Need at least 3 vertices for a polygon
        }

        Some(Region {
            vertices,
            holes: Vec::new(),
            layer,
            flags: json_flags(json),
            unique_id: None,
        })
    }

    /// Parses text from JSON.
    pub(crate) fn parse_text(json: &Value) -> Option<crate::altium::pcblib::Text> {
        use crate::altium::pcblib::{Layer, Text, TextJustification, TextKind};

        let x = json.get("x").and_then(Value::as_f64)?;
        let y = json.get("y").and_then(Value::as_f64)?;
        let text = json.get("text").and_then(Value::as_str)?;
        let height = json.get("height").and_then(Value::as_f64)?;
        let layer = json
            .get("layer")
            .and_then(Value::as_str)
            .and_then(Layer::parse)
            .unwrap_or(Layer::TopOverlay);
        let rotation = json.get("rotation").and_then(Value::as_f64).unwrap_or(0.0);
        // Optional stroke line width in mm; `None` keeps Altium's template default.
        let stroke_width = json
            .get("stroke_width")
            .and_then(Value::as_f64)
            .filter(|&w| w > 0.0);

        Some(Text {
            x,
            y,
            text: text.to_string(),
            height,
            layer,
            rotation,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width,
            italic: false,
            justification: TextJustification::MiddleCenter,
            flags: json_flags(json),
            unique_id: None,
        })
    }

    /// Parses a via from JSON.
    ///
    /// Mirrors [`Self::parse_pad`]'s layer-name parsing for the `from_layer` /
    /// `to_layer` fields and reuses [`crate::altium::pcblib::MaskExpansionMode`]
    /// string parsing for the mask mode. Optionals default exactly as
    /// [`crate::altium::pcblib::Via::new`] does when absent.
    pub(crate) fn parse_via(json: &Value) -> Result<crate::altium::pcblib::Via, String> {
        use crate::altium::pcblib::{Layer, MaskExpansionMode, Via};

        let x = json
            .get("x")
            .and_then(Value::as_f64)
            .ok_or("Via missing required field 'x'")?;
        let y = json
            .get("y")
            .and_then(Value::as_f64)
            .ok_or("Via missing required field 'y'")?;
        let diameter = json
            .get("diameter")
            .and_then(Value::as_f64)
            .ok_or("Via missing required field 'diameter'")?;
        let hole_size = json
            .get("hole_size")
            .and_then(Value::as_f64)
            .ok_or("Via missing required field 'hole_size'")?;

        // Validate via dimensions are sensible: the hole must fit inside the
        // annular ring, both positive.
        if diameter <= 0.0 {
            return Err(format!(
                "Via has invalid diameter {diameter}. Diameter must be greater than 0."
            ));
        }
        if hole_size <= 0.0 {
            return Err(format!(
                "Via has invalid hole_size {hole_size}. Hole size must be greater than 0."
            ));
        }
        if hole_size >= diameter {
            return Err(format!(
                "Via hole_size {hole_size} must be smaller than diameter {diameter}."
            ));
        }

        // Start from the struct's defaults (top->bottom layers, standard thermal
        // relief), then override with any supplied fields.
        let mut via = Via::new(x, y, diameter, hole_size);

        if let Some(s) = json.get("from_layer").and_then(Value::as_str) {
            via.from_layer = Layer::parse(s).ok_or_else(|| {
                format!(
                    "Via has invalid from_layer '{s}'. \
                     Valid layers include: Top Layer, Bottom Layer, Mid-Layer 1, etc."
                )
            })?;
        }
        if let Some(s) = json.get("to_layer").and_then(Value::as_str) {
            via.to_layer = Layer::parse(s).ok_or_else(|| {
                format!(
                    "Via has invalid to_layer '{s}'. \
                     Valid layers include: Top Layer, Bottom Layer, Mid-Layer 1, etc."
                )
            })?;
        }

        if let Some(v) = json.get("solder_mask_expansion").and_then(Value::as_f64) {
            via.solder_mask_expansion = v;
        }
        if let Some(s) = json
            .get("solder_mask_expansion_mode")
            .and_then(Value::as_str)
        {
            via.solder_mask_expansion_mode = match s.to_lowercase().as_str() {
                "none" => MaskExpansionMode::None,
                "manual" => MaskExpansionMode::Manual,
                _ => MaskExpansionMode::FromRule,
            };
        }
        if let Some(v) = json.get("thermal_relief_gap").and_then(Value::as_f64) {
            via.thermal_relief_gap = v;
        }
        if let Some(v) = json
            .get("thermal_relief_conductors")
            .and_then(Value::as_u64)
            .and_then(|v| u8::try_from(v).ok())
        {
            via.thermal_relief_conductors = v;
        }
        if let Some(v) = json.get("thermal_relief_width").and_then(Value::as_f64) {
            via.thermal_relief_width = v;
        }

        Ok(via)
    }

    /// Parses a fill from JSON.
    ///
    /// Reuses [`Self::parse_pad`]'s layer-name parsing for `layer`. Geometry
    /// (`x1`/`y1`/`x2`/`y2`) is required; `rotation` and the mask/keepout
    /// optionals default as [`crate::altium::pcblib::Fill::new`] does when absent.
    pub(crate) fn parse_fill(json: &Value) -> Result<crate::altium::pcblib::Fill, String> {
        use crate::altium::pcblib::{Fill, Layer};

        let x1 = json
            .get("x1")
            .and_then(Value::as_f64)
            .ok_or("Fill missing required field 'x1'")?;
        let y1 = json
            .get("y1")
            .and_then(Value::as_f64)
            .ok_or("Fill missing required field 'y1'")?;
        let x2 = json
            .get("x2")
            .and_then(Value::as_f64)
            .ok_or("Fill missing required field 'x2'")?;
        let y2 = json
            .get("y2")
            .and_then(Value::as_f64)
            .ok_or("Fill missing required field 'y2'")?;

        let layer_str = json.get("layer").and_then(Value::as_str);
        let layer = match layer_str {
            Some(s) => Layer::parse(s).ok_or_else(|| {
                format!(
                    "Fill has invalid layer '{s}'. \
                     Valid layers include: Top Layer, Bottom Layer, Top Overlay, Mechanical 1, etc."
                )
            })?,
            None => Layer::TopLayer, // Default for fills is Top Layer
        };

        let mut fill = Fill::new(x1, y1, x2, y2, layer);

        if let Some(r) = json.get("rotation").and_then(Value::as_f64) {
            fill.rotation = r;
        }
        // Optional flags + mask/keepout tail (mirrors the modelled optionals).
        fill.flags = json_flags(json);
        fill.solder_mask_expansion = json.get("solder_mask_expansion").and_then(Value::as_f64);
        fill.keepout_restrictions = json_keepout(json);

        Ok(fill)
    }

    // ==================== SchLib Primitive Parsing Helpers ====================

    /// Parses a schematic pin from JSON.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::too_many_lines)] // Pin parsing with symbol attributes requires many lines
    pub(crate) fn parse_schlib_pin(json: &Value) -> Option<crate::altium::schlib::Pin> {
        use crate::altium::schlib::{Pin, PinElectricalType, PinOrientation, PinSymbol};

        let designator = json.get("designator").and_then(Value::as_str)?;
        let name = json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(designator);
        let x = json_i32(json, "x")?;
        let y = json_i32(json, "y")?;
        let length = json_i32(json, "length").unwrap_or(10);

        let orientation =
            json.get("orientation")
                .and_then(Value::as_str)
                .map_or(PinOrientation::Right, |s| match s.to_lowercase().as_str() {
                    "left" => PinOrientation::Left,
                    "up" => PinOrientation::Up,
                    "down" => PinOrientation::Down,
                    _ => PinOrientation::Right,
                });

        let electrical_type = json.get("electrical_type").and_then(Value::as_str).map_or(
            PinElectricalType::Passive,
            |s| match s.to_lowercase().as_str() {
                "input" => PinElectricalType::Input,
                "output" => PinElectricalType::Output,
                "bidirectional" | "io" | "input_output" => PinElectricalType::Bidirectional,
                "power" => PinElectricalType::Power,
                "open_collector" => PinElectricalType::OpenCollector,
                "open_emitter" => PinElectricalType::OpenEmitter,
                "hiz" | "hi_z" | "tristate" => PinElectricalType::HiZ,
                _ => PinElectricalType::Passive,
            },
        );

        let hidden = json.get("hidden").and_then(Value::as_bool).unwrap_or(false);
        let show_name = json
            .get("show_name")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let show_designator = json
            .get("show_designator")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        // Helper to parse PinSymbol from string
        let parse_pin_symbol = |s: &str| -> PinSymbol {
            match s.to_lowercase().replace(['-', '_'], "").as_str() {
                "dot" | "invert" | "inversion" => PinSymbol::Dot,
                "clock" | "clk" => PinSymbol::Clock,
                "activelowinput" | "lowinput" => PinSymbol::ActiveLowInput,
                "activelowoutput" | "lowoutput" => PinSymbol::ActiveLowOutput,
                "rightleftsignalflow" | "rightleft" => PinSymbol::RightLeftSignalFlow,
                "leftrrightsignalflow" | "leftright" => PinSymbol::LeftRightSignalFlow,
                "bidirectionalsignalflow" | "bidirectional" => PinSymbol::BidirectionalSignalFlow,
                "analogsignalin" | "analog" => PinSymbol::AnalogSignalIn,
                "digitalsignalin" | "digital" => PinSymbol::DigitalSignalIn,
                "notlogicconnection" | "notlogic" => PinSymbol::NotLogicConnection,
                "postponedoutput" | "postponed" => PinSymbol::PostponedOutput,
                "opencollector" => PinSymbol::OpenCollector,
                "opencollectorpullup" => PinSymbol::OpenCollectorPullUp,
                "openemitter" => PinSymbol::OpenEmitter,
                "openemitterpullup" => PinSymbol::OpenEmitterPullUp,
                "openoutput" => PinSymbol::OpenOutput,
                "hiz" | "highimpedance" => PinSymbol::HiZ,
                "highcurrent" => PinSymbol::HighCurrent,
                "pulse" => PinSymbol::Pulse,
                "schmitt" | "schmitttrigger" => PinSymbol::Schmitt,
                "shiftleft" => PinSymbol::ShiftLeft,
                _ => PinSymbol::None, // "none" and unknown values
            }
        };

        let symbol_inner_edge = json
            .get("symbol_inner_edge")
            .and_then(Value::as_str)
            .map_or(PinSymbol::None, parse_pin_symbol);
        let symbol_outer_edge = json
            .get("symbol_outer_edge")
            .and_then(Value::as_str)
            .map_or(PinSymbol::None, parse_pin_symbol);
        let symbol_inside = json
            .get("symbol_inside")
            .and_then(Value::as_str)
            .map_or(PinSymbol::None, parse_pin_symbol);
        let symbol_outside = json
            .get("symbol_outside")
            .and_then(Value::as_str)
            .map_or(PinSymbol::None, parse_pin_symbol);

        Some(Pin {
            name: name.to_string(),
            designator: designator.to_string(),
            x,
            y,
            length,
            orientation,
            electrical_type,
            hidden,
            show_name,
            show_designator,
            description: String::new(),
            owner_part_id,
            colour: 0,
            graphically_locked: false,
            symbol_inner_edge,
            symbol_outer_edge,
            symbol_inside,
            symbol_outside,
            is_not_accessible: false,
            formal_type: 1,
            swap_id_group: String::new(),
            part_and_sequence: "|&|".to_string(),
            default_value: String::new(),
        })
    }

    /// Parses a schematic rectangle from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_rectangle(json: &Value) -> Option<crate::altium::schlib::Rectangle> {
        use crate::altium::schlib::Rectangle;

        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xB0_FF_FF) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Rectangle {
            x1,
            y1,
            x2,
            y2,
            line_width,
            line_color,
            fill_color,
            line_style: 0,
            filled,
            transparent: false,
            owner_part_id,
            unique_id: None,
        })
    }

    /// Parses a schematic rounded rectangle from JSON.
    ///
    /// Mirrors [`Self::parse_schlib_rectangle`] (geometry + fill/border colours +
    /// `filled`), adding the `corner_x_radius` / `corner_y_radius` rounding fields.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::similar_names)] // corner_x_radius / corner_y_radius mirror the struct fields
    pub(crate) fn parse_schlib_round_rect(
        json: &Value,
    ) -> Option<crate::altium::schlib::RoundRect> {
        use crate::altium::schlib::RoundRect;

        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;
        let corner_x_radius = json_f64(json, "corner_x_radius").unwrap_or(0.0);
        let corner_y_radius = json_f64(json, "corner_y_radius").unwrap_or(0.0);

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xB0_FF_FF) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(RoundRect {
            x1,
            y1,
            x2,
            y2,
            corner_x_radius,
            corner_y_radius,
            line_width,
            line_color,
            fill_color,
            line_style: 0,
            filled,
            transparent: false,
            owner_part_id,
            unique_id: None,
        })
    }

    /// Parses a schematic line from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_line(json: &Value) -> Option<crate::altium::schlib::Line> {
        use crate::altium::schlib::Line;

        // Coordinates accept fractional values; integer-only `json_i32` would drop
        // an off-grid endpoint like 3.75.
        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Line {
            x1,
            y1,
            x2,
            y2,
            line_width,
            color,
            line_style: 0,
            is_not_accessible: true,
            owner_part_id,
            unique_id: None,
        })
    }

    /// Parses a schematic parameter from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_parameter(json: &Value) -> Option<crate::altium::schlib::Parameter> {
        use crate::altium::schlib::Parameter;

        let name = json.get("name").and_then(Value::as_str)?;
        let value = json
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("*")
            .to_string();

        let x = json_f64(json, "x").unwrap_or(0.0);
        let y = json_f64(json, "y").unwrap_or(0.0);
        let font_id = json.get("font_id").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x80_00_00) as u32;
        let hidden = json.get("hidden").and_then(Value::as_bool).unwrap_or(false);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Parameter {
            name: name.to_string(),
            value,
            x,
            y,
            font_id,
            color,
            hidden,
            read_only_state: 0,
            param_type: 0,
            owner_part_id,
            unique_id: None,
        })
    }

    /// Parses a schematic polyline from JSON.
    /// Accepts both "points" and "vertices" field names for the coordinate array.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_polyline(json: &Value) -> Option<crate::altium::schlib::Polyline> {
        use crate::altium::schlib::Polyline;

        // Accept both "points" and "vertices" for flexibility
        let points_json = json
            .get("points")
            .or_else(|| json.get("vertices"))
            .and_then(Value::as_array)?;
        let points: Vec<(f64, f64)> = points_json
            .iter()
            .filter_map(|p| {
                let x = json_f64(p, "x")?;
                let y = json_f64(p, "y")?;
                Some((x, y))
            })
            .collect();

        if points.len() < 2 {
            return None; // Need at least 2 points for a polyline
        }

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Polyline {
            points,
            line_width,
            color,
            line_style: 0,
            start_line_shape: 0,
            end_line_shape: 0,
            line_shape_size: 0,
            transparent: false,
            owner_part_id,
            unique_id: None,
        })
    }

    /// Parses a schematic filled polygon from JSON.
    ///
    /// Mirrors [`Self::parse_schlib_polyline`] (reads the `points`/`vertices`
    /// array of `[x, y]` pairs), adding the polygon's `filled` / `fill_color`
    /// fields.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_polygon(json: &Value) -> Option<crate::altium::schlib::Polygon> {
        use crate::altium::schlib::Polygon;

        // Accept both "points" and "vertices" for flexibility (matches polyline).
        let points_json = json
            .get("points")
            .or_else(|| json.get("vertices"))
            .and_then(Value::as_array)?;
        let points: Vec<(f64, f64)> = points_json
            .iter()
            .filter_map(|p| {
                let x = json_f64(p, "x")?;
                let y = json_f64(p, "y")?;
                Some((x, y))
            })
            .collect();

        if points.len() < 3 {
            return None; // Need at least 3 vertices for a polygon
        }

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xB0_FF_FF) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Polygon {
            points,
            line_width,
            line_color,
            fill_color,
            filled,
            owner_part_id,
            unique_id: None,
        })
    }

    /// Parses a schematic arc from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_arc(json: &Value) -> Option<crate::altium::schlib::Arc> {
        use crate::altium::schlib::Arc;

        let x = json_f64(json, "x")?;
        let y = json_f64(json, "y")?;
        let radius = json_f64(json, "radius")?;
        let start_angle = json
            .get("start_angle")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let end_angle = json
            .get("end_angle")
            .and_then(Value::as_f64)
            .unwrap_or(360.0);
        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Arc {
            x,
            y,
            radius,
            is_not_accessible: true,
            start_angle,
            end_angle,
            line_width,
            color,
            fill_color: 0,
            owner_part_id,
            unique_id: None,
        })
    }

    /// Parses a schematic ellipse from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_ellipse(json: &Value) -> Option<crate::altium::schlib::Ellipse> {
        use crate::altium::schlib::Ellipse;

        let x = json_f64(json, "x")?;
        let y = json_f64(json, "y")?;
        let radius_x = json_f64(json, "radius_x")?;
        let radius_y = json_f64(json, "radius_y")?;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xB0_FF_FF) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Ellipse {
            x,
            y,
            radius_x,
            radius_y,
            line_width,
            line_color,
            fill_color,
            filled,
            transparent: false,
            owner_part_id,
            unique_id: None,
        })
    }

    /// Parses a schematic label from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_label(json: &Value) -> Option<crate::altium::schlib::Label> {
        use crate::altium::schlib::{Label, TextJustification};

        let x = json_f64(json, "x")?;
        let y = json_f64(json, "y")?;
        let text = json.get("text").and_then(Value::as_str)?.to_string();

        let font_id = json.get("font_id").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let rotation = json.get("rotation").and_then(Value::as_f64).unwrap_or(0.0);
        let is_mirrored = json
            .get("is_mirrored")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_hidden = json
            .get("is_hidden")
            .or_else(|| json.get("hidden"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        let justification = json.get("justification").and_then(Value::as_str).map_or(
            TextJustification::BottomLeft,
            |s| {
                match s.to_lowercase().replace(['-', '_'], "").as_str() {
                    "bottomcenter" | "bottomcentre" => TextJustification::BottomCenter,
                    "bottomright" => TextJustification::BottomRight,
                    "middleleft" | "centerleft" | "centreleft" => TextJustification::MiddleLeft,
                    "middlecenter" | "middlecentre" | "center" | "centre" => {
                        TextJustification::MiddleCenter
                    }
                    "middleright" | "centerright" | "centreright" => TextJustification::MiddleRight,
                    "topleft" => TextJustification::TopLeft,
                    "topcenter" | "topcentre" => TextJustification::TopCenter,
                    "topright" => TextJustification::TopRight,
                    _ => TextJustification::BottomLeft, // "bottomleft" and unknown values
                }
            },
        );

        Some(Label {
            x,
            y,
            text,
            font_id,
            color,
            justification,
            rotation,
            is_mirrored,
            is_hidden,
            owner_part_id,
            unique_id: None,
        })
    }

    /// Parses a schematic text annotation from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_text(json: &Value) -> Option<crate::altium::schlib::Text> {
        use crate::altium::schlib::{Text, TextJustification};

        let x = json_f64(json, "x")?;
        let y = json_f64(json, "y")?;
        let text = json.get("text").and_then(Value::as_str)?.to_string();

        let font_id = json.get("font_id").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let rotation = json.get("rotation").and_then(Value::as_f64).unwrap_or(0.0);
        let is_mirrored = json
            .get("is_mirrored")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_hidden = json
            .get("is_hidden")
            .or_else(|| json.get("hidden"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        let justification = json.get("justification").and_then(Value::as_str).map_or(
            TextJustification::BottomLeft,
            |s| {
                match s.to_lowercase().replace(['-', '_'], "").as_str() {
                    "bottomcenter" | "bottomcentre" => TextJustification::BottomCenter,
                    "bottomright" => TextJustification::BottomRight,
                    "middleleft" | "centerleft" | "centreleft" => TextJustification::MiddleLeft,
                    "middlecenter" | "middlecentre" | "center" | "centre" => {
                        TextJustification::MiddleCenter
                    }
                    "middleright" | "centerright" | "centreright" => TextJustification::MiddleRight,
                    "topleft" => TextJustification::TopLeft,
                    "topcenter" | "topcentre" => TextJustification::TopCenter,
                    "topright" => TextJustification::TopRight,
                    _ => TextJustification::BottomLeft, // "bottomleft" and unknown values
                }
            },
        );

        Some(Text {
            x,
            y,
            text,
            font_id,
            color,
            justification,
            rotation,
            is_mirrored,
            is_hidden,
            owner_part_id,
            unique_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{json_f64, json_i32};
    use crate::mcp::server::McpServer;
    use serde_json::json;

    #[test]
    fn json_i32_drops_fractional_coordinate() {
        // Demonstrates the original defect: the integer reader rejects an
        // off-grid value, so a fractional coordinate was silently dropped while
        // an integer one passed through.
        assert_eq!(json_i32(&json!({ "x": 3.75 }), "x"), None);
        assert_eq!(json_i32(&json!({ "x": 3 }), "x"), Some(3));
    }

    #[test]
    fn json_f64_accepts_numbers_and_rejects_non_numeric() {
        // The fix: accept fractional and integer JSON numbers; reject non-numeric.
        assert_eq!(json_f64(&json!({ "x": 3.75 }), "x"), Some(3.75));
        assert_eq!(json_f64(&json!({ "x": -28 }), "x"), Some(-28.0));
        assert_eq!(json_f64(&json!({ "x": "nope" }), "x"), None);
        assert_eq!(json_f64(&json!({}), "x"), None);
    }

    #[test]
    fn parse_schlib_line_preserves_fractional_coords() {
        // End-to-end: a fractional line now parses (instead of being dropped)
        // and keeps its exact coordinates, including a negative fractional X.
        let line = McpServer::parse_schlib_line(&json!({
            "x1": -28.995, "y1": 7.5, "x2": 10.0, "y2": 0.0
        }))
        .expect("fractional line should parse");
        assert!((line.x1 - (-28.995)).abs() < 1e-9, "x1 kept: {}", line.x1);
        assert!((line.y1 - 7.5).abs() < 1e-9, "y1 kept: {}", line.y1);
        assert!((line.x2 - 10.0).abs() < 1e-9, "x2 kept: {}", line.x2);
    }

    // --- PR-4: flags / solder_mask_expansion / keepout_restrictions on the
    // write (JSON -> primitive) path. The `flags` JSON shape is the raw u16
    // bitmask that `read_pcblib` serialises (PcbFlags is #[serde(transparent)]),
    // so the values these tests feed in are the same ones a read would emit.

    #[test]
    fn json_flags_reads_read_dto_string_form() {
        use crate::altium::pcblib::PcbFlags;
        // Canonical round-trip shape: the bitflags serde string read_pcblib emits.
        let flags = super::json_flags(&json!({ "flags": "LOCKED | KEEPOUT" }));
        assert!(flags.contains(PcbFlags::LOCKED));
        assert!(flags.contains(PcbFlags::KEEPOUT));
        let single = super::json_flags(&json!({ "flags": "LOCKED" }));
        assert!(single.contains(PcbFlags::LOCKED));
        assert!(!single.contains(PcbFlags::KEEPOUT));
        // Convenience shape: a raw bitmask integer (LOCKED 1 | KEEPOUT 4 = 5).
        let int_flags = super::json_flags(&json!({ "flags": 5 }));
        assert!(int_flags.contains(PcbFlags::LOCKED));
        assert!(int_flags.contains(PcbFlags::KEEPOUT));
        // Absent key -> empty (default), matching the read-side skip_serializing_if.
        assert!(super::json_flags(&json!({})).is_empty());
    }

    #[test]
    fn pcbflags_write_then_read_dto_round_trip() {
        use crate::altium::pcblib::PcbFlags;
        // The string the read DTO serialises must parse back to the same flags on
        // the write path — guards the read/write shape reconciliation.
        let original = PcbFlags::LOCKED | PcbFlags::KEEPOUT;
        let dto = serde_json::to_value(original).expect("serialise flags");
        let parsed = super::json_flags(&json!({ "flags": dto }));
        assert_eq!(parsed, original);
    }

    #[test]
    fn parse_pad_reads_flags_and_solder_mask() {
        use crate::altium::pcblib::{MaskExpansionMode, PcbFlags};
        let pad = McpServer::parse_pad(&json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
            "flags": "LOCKED",
            "solder_mask_expansion": 0.05,
            "solder_mask_expansion_mode": "manual",
        }))
        .expect("pad should parse");
        assert!(pad.flags.contains(PcbFlags::LOCKED));
        assert_eq!(pad.solder_mask_expansion, Some(0.05));
        assert_eq!(pad.solder_mask_expansion_mode, MaskExpansionMode::Manual);
    }

    #[test]
    fn parse_track_reads_flags_solder_mask_keepout() {
        use crate::altium::pcblib::PcbFlags;
        let track = McpServer::parse_track(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.15,
            "layer": "Top Overlay",
            "flags": "KEEPOUT",
            "solder_mask_expansion": 0.1,
            "keepout_restrictions": 3,
        }))
        .expect("track should parse");
        assert!(track.flags.contains(PcbFlags::KEEPOUT));
        assert_eq!(track.solder_mask_expansion, Some(0.1));
        assert_eq!(track.keepout_restrictions, Some(3));
        // Absent keys leave the Track::new defaults untouched.
        let bare = McpServer::parse_track(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.15, "layer": "Top Overlay"
        }))
        .expect("bare track should parse");
        assert!(bare.flags.is_empty());
        assert_eq!(bare.solder_mask_expansion, None);
        assert_eq!(bare.keepout_restrictions, None);
    }

    #[test]
    fn parse_arc_reads_flags_solder_mask_keepout() {
        use crate::altium::pcblib::PcbFlags;
        let arc = McpServer::parse_arc(&json!({
            "x": 0.0, "y": 0.0, "radius": 1.0,
            "start_angle": 0.0, "end_angle": 90.0, "width": 0.15,
            "layer": "Top Overlay",
            "flags": "LOCKED",
            "solder_mask_expansion": 0.2,
            "keepout_restrictions": 5,
        }))
        .expect("arc should parse");
        assert!(arc.flags.contains(PcbFlags::LOCKED));
        assert_eq!(arc.solder_mask_expansion, Some(0.2));
        assert_eq!(arc.keepout_restrictions, Some(5));
    }

    #[test]
    fn parse_region_reads_flags() {
        use crate::altium::pcblib::PcbFlags;
        let region = McpServer::parse_region(&json!({
            "layer": "Top Courtyard",
            "vertices": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 0.0}, {"x": 0.0, "y": 1.0}],
            "flags": "KEEPOUT",
        }))
        .expect("region should parse");
        assert!(region.flags.contains(PcbFlags::KEEPOUT));
    }

    #[test]
    fn parse_fill_reads_flags() {
        use crate::altium::pcblib::PcbFlags;
        let fill = McpServer::parse_fill(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0, "layer": "Top Layer",
            "flags": "LOCKED",
            "solder_mask_expansion": 0.05,
            "keepout_restrictions": 2,
        }))
        .expect("fill should parse");
        assert!(fill.flags.contains(PcbFlags::LOCKED));
        assert_eq!(fill.solder_mask_expansion, Some(0.05));
        assert_eq!(fill.keepout_restrictions, Some(2));
    }

    #[test]
    fn parse_text_reads_flags() {
        use crate::altium::pcblib::PcbFlags;
        let text = McpServer::parse_text(&json!({
            "x": 0.0, "y": 0.0, "text": "REF", "height": 0.5, "layer": "Top Overlay",
            "flags": "LOCKED",
        }))
        .expect("text should parse");
        assert!(text.flags.contains(PcbFlags::LOCKED));
    }
}
