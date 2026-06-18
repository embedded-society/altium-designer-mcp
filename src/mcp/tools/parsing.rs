//! JSON -> Altium primitive parsing helpers, split from `server.rs`.
//!
//! These extend `McpServer` with an additional `impl` block; the handlers in
//! other modules call them via `Self::parse_*` unchanged.

use serde_json::Value;

use crate::mcp::server::McpServer;

impl McpServer {
    // ==================== Primitive Parsing Helpers ====================

    /// Parses a pad from JSON.
    #[allow(clippy::too_many_lines)] // Pad has many fields requiring individual parsing
    pub(crate) fn parse_pad(json: &Value) -> Result<crate::altium::pcblib::Pad, String> {
        use crate::altium::pcblib::{Layer, Pad, PadShape, PadStackMode, PcbFlags};

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
            "rounded_rectangle" => PadShape::RoundedRectangle,
            _ => {
                return Err(format!(
                    "Pad '{designator}' has invalid shape '{shape_str}'. \
                     Valid shapes are: rectangle, round, circle, oval, rounded_rectangle"
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
        let paste_mask_expansion_manual = json
            .get("paste_mask_expansion_manual")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let solder_mask_expansion_manual = json
            .get("solder_mask_expansion_manual")
            .and_then(Value::as_bool)
            .unwrap_or(false);

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
            paste_mask_expansion_manual,
            solder_mask_expansion_manual,
            corner_radius_percent,
            stack_mode: PadStackMode::Simple,
            per_layer_sizes: None,
            per_layer_shapes: None,
            per_layer_corner_radii: None,
            per_layer_offsets: None,
            flags: PcbFlags::empty(),
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

        Ok(Track::new(x1, y1, x2, y2, width, layer))
    }

    /// Parses an arc from JSON.
    pub(crate) fn parse_arc(json: &Value) -> Result<crate::altium::pcblib::Arc, String> {
        use crate::altium::pcblib::{Arc, Layer, PcbFlags};

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
            flags: PcbFlags::empty(),
            unique_id: None,
        })
    }

    /// Parses a region from JSON.
    pub(crate) fn parse_region(json: &Value) -> Option<crate::altium::pcblib::Region> {
        use crate::altium::pcblib::{Layer, PcbFlags, Region, Vertex};

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
            layer,
            flags: PcbFlags::empty(),
            unique_id: None,
        })
    }

    /// Parses text from JSON.
    pub(crate) fn parse_text(json: &Value) -> Option<crate::altium::pcblib::Text> {
        use crate::altium::pcblib::{Layer, PcbFlags, Text, TextJustification, TextKind};

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

        Some(Text {
            x,
            y,
            text: text.to_string(),
            height,
            layer,
            rotation,
            kind: TextKind::Stroke,
            stroke_font: None,
            justification: TextJustification::MiddleCenter,
            flags: PcbFlags::empty(),
            unique_id: None,
        })
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
        let x = json.get("x").and_then(Value::as_i64)? as i32;
        let y = json.get("y").and_then(Value::as_i64)? as i32;
        let length = json.get("length").and_then(Value::as_i64).unwrap_or(10) as i32;

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
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

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
        })
    }

    /// Parses a schematic rectangle from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_rectangle(json: &Value) -> Option<crate::altium::schlib::Rectangle> {
        use crate::altium::schlib::Rectangle;

        let x1 = json.get("x1").and_then(Value::as_i64)? as i32;
        let y1 = json.get("y1").and_then(Value::as_i64)? as i32;
        let x2 = json.get("x2").and_then(Value::as_i64)? as i32;
        let y2 = json.get("y2").and_then(Value::as_i64)? as i32;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xFF_FF_B0) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Rectangle {
            x1,
            y1,
            x2,
            y2,
            line_width,
            line_color,
            fill_color,
            filled,
            transparent: false,
            owner_part_id,
        })
    }

    /// Parses a schematic line from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_line(json: &Value) -> Option<crate::altium::schlib::Line> {
        use crate::altium::schlib::Line;

        let x1 = json.get("x1").and_then(Value::as_i64)? as i32;
        let y1 = json.get("y1").and_then(Value::as_i64)? as i32;
        let x2 = json.get("x2").and_then(Value::as_i64)? as i32;
        let y2 = json.get("y2").and_then(Value::as_i64)? as i32;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Line {
            x1,
            y1,
            x2,
            y2,
            line_width,
            color,
            owner_part_id,
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

        let x = json.get("x").and_then(Value::as_i64).unwrap_or(0) as i32;
        let y = json.get("y").and_then(Value::as_i64).unwrap_or(0) as i32;
        let font_id = json.get("font_id").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x80_00_00) as u32;
        let hidden = json.get("hidden").and_then(Value::as_bool).unwrap_or(false);
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

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
        let points: Vec<(i32, i32)> = points_json
            .iter()
            .filter_map(|p| {
                let x = p.get("x").and_then(Value::as_i64)? as i32;
                let y = p.get("y").and_then(Value::as_i64)? as i32;
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
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Polyline {
            points,
            line_width,
            color,
            line_style: 0,
            start_line_shape: 0,
            end_line_shape: 0,
            line_shape_size: 0,
            owner_part_id,
        })
    }

    /// Parses a schematic arc from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_arc(json: &Value) -> Option<crate::altium::schlib::Arc> {
        use crate::altium::schlib::Arc;

        let x = json.get("x").and_then(Value::as_i64)? as i32;
        let y = json.get("y").and_then(Value::as_i64)? as i32;
        let radius = json.get("radius").and_then(Value::as_i64)? as i32;
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
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Arc {
            x,
            y,
            radius,
            start_angle,
            end_angle,
            line_width,
            color,
            owner_part_id,
        })
    }

    /// Parses a schematic ellipse from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_ellipse(json: &Value) -> Option<crate::altium::schlib::Ellipse> {
        use crate::altium::schlib::Ellipse;

        let x = json.get("x").and_then(Value::as_i64)? as i32;
        let y = json.get("y").and_then(Value::as_i64)? as i32;
        let radius_x = json.get("radius_x").and_then(Value::as_i64)? as i32;
        let radius_y = json.get("radius_y").and_then(Value::as_i64)? as i32;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xFF_FF_B0) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        Some(Ellipse {
            x,
            y,
            radius_x,
            radius_y,
            line_width,
            line_color,
            fill_color,
            filled,
            owner_part_id,
        })
    }

    /// Parses a schematic label from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_label(json: &Value) -> Option<crate::altium::schlib::Label> {
        use crate::altium::schlib::{Label, TextJustification};

        let x = json.get("x").and_then(Value::as_i64)? as i32;
        let y = json.get("y").and_then(Value::as_i64)? as i32;
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
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

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
        })
    }

    /// Parses a schematic text annotation from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_text(json: &Value) -> Option<crate::altium::schlib::Text> {
        use crate::altium::schlib::{Text, TextJustification};

        let x = json.get("x").and_then(Value::as_i64)? as i32;
        let y = json.get("y").and_then(Value::as_i64)? as i32;
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
        let owner_part_id = json
            .get("owner_part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

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
        })
    }
}
