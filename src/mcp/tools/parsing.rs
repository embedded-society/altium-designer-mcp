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

/// Reads the four universal display/lock flags shared by every `SchLib` graphic
/// shape (`graphically_locked` / `disabled` / `dimmed` /
/// `owner_part_display_mode`) from a shape's JSON. Absent keys default to
/// `false` / `0`, matching Altium's omit-when-default records.
fn parse_schlib_display_flags(json: &Value) -> crate::altium::schlib::ShapeDisplayFlags {
    #[allow(clippy::cast_possible_truncation)]
    crate::altium::schlib::ShapeDisplayFlags {
        graphically_locked: json
            .get("graphically_locked")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        disabled: json
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        dimmed: json.get("dimmed").and_then(Value::as_bool).unwrap_or(false),
        owner_part_display_mode: json_i32(json, "owner_part_display_mode").unwrap_or(0),
    }
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

/// Reads the optional common-header net index (u16) of a `PcbLib` primitive,
/// defaulting to `0xFFFF` ("no net") — the from-scratch value the writer's
/// header fill emits. Mirrors how `read_pcblib` serialises the `net_index` field.
fn json_net_index(json: &Value) -> u16 {
    json.get("net_index")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .unwrap_or(0xFFFF)
}

/// Reads the optional common-header polygon index (u16) of a `PcbLib` primitive,
/// defaulting to `0xFFFF` (none) — the from-scratch value.
fn json_polygon_index(json: &Value) -> u16 {
    json.get("polygon_index")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .unwrap_or(0xFFFF)
}

/// Reads the optional common-header component index (i32) of a `PcbLib`
/// primitive, defaulting to `-1` (free primitive; stored as the `0xFFFF`
/// sentinel) — the from-scratch value.
fn json_component_index(json: &Value) -> i32 {
    json.get("component_index")
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
        .unwrap_or(-1)
}

/// Reads the optional `unique_id` (identity GUID) of any primitive.
///
/// `read_pcblib` / `read_schlib` surface each primitive's 8-char Altium unique
/// ID via serde, so an AI doing a read-modify-write can pass it straight back
/// here to preserve stable primitive identity across saves (Altium tracks
/// primitives by this GUID for ECO). An absent value yields `None`, letting the
/// writer auto-generate a fresh GUID exactly as it does for from-scratch output.
fn json_unique_id(json: &Value) -> Option<String> {
    json.get("unique_id")
        .and_then(Value::as_str)
        .map(str::to_string)
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
        use crate::altium::pcblib::{
            Layer, MaskExpansionMode, Pad, PadShape, PadStackMode, PowerPlaneConnectStyle,
        };

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

        // Plated hole (main-block byte @60). Altium defaults this to true for
        // every pad, SMD included (matches the golden fixture and AltiumSharp).
        let is_plated = json
            .get("is_plated")
            .and_then(Value::as_bool)
            .unwrap_or(true);

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

        // Thermal-relief / power-plane connection fields. Absent keys keep the
        // from-scratch defaults (= Altium's pad template), so an unspecified pad
        // round-trips byte-identically.
        let power_plane_connect_style = json
            .get("power_plane_connect_style")
            .and_then(Value::as_str)
            .map(|s| match s.to_lowercase().as_str() {
                "direct" => PowerPlaneConnectStyle::Direct,
                "no_connect" | "noconnect" => PowerPlaneConnectStyle::NoConnect,
                _ => PowerPlaneConnectStyle::Relief,
            })
            .unwrap_or_default();
        let relief_conductor_width = json
            .get("relief_conductor_width")
            .and_then(Value::as_f64)
            .unwrap_or(0.254);
        let relief_entries = json
            .get("relief_entries")
            .and_then(Value::as_i64)
            .and_then(|v| i16::try_from(v).ok())
            .unwrap_or(4);
        let relief_air_gap = json
            .get("relief_air_gap")
            .and_then(Value::as_f64)
            .unwrap_or(0.254);
        let power_plane_relief_expansion = json
            .get("power_plane_relief_expansion")
            .and_then(Value::as_f64)
            .unwrap_or(0.508);
        let power_plane_clearance = json
            .get("power_plane_clearance")
            .and_then(Value::as_f64)
            .unwrap_or(0.508);

        // Slot geometry + drill tolerances. Absent keys keep the struct defaults
        // (slot 0, rotation 0, tolerances unset), so an unspecified pad round-trips
        // byte-identically.
        let hole_slot_length = json
            .get("hole_slot_length")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let hole_rotation = json
            .get("hole_rotation")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let hole_positive_tolerance = json.get("hole_positive_tolerance").and_then(Value::as_f64);
        let hole_negative_tolerance = json.get("hole_negative_tolerance").and_then(Value::as_f64);

        // Identity GUIDs (extended tail @126/@142). Absent -> None, so the
        // writer generates fresh per-pad GUIDs; a read-modify-write passes the
        // read value back and preserves the on-disk bytes verbatim.
        let identity_guid = json
            .get("identity_guid")
            .and_then(Value::as_str)
            .map(str::to_string);
        let identity_guid_b = json
            .get("identity_guid_b")
            .and_then(Value::as_str)
            .map(str::to_string);

        Ok(Pad {
            designator: designator.to_string(),
            x,
            y,
            width,
            height,
            shape,
            layer,
            hole_size,
            is_plated,
            hole_shape,
            hole_slot_length,
            hole_rotation,
            hole_positive_tolerance,
            hole_negative_tolerance,
            rotation,
            paste_mask_expansion,
            solder_mask_expansion,
            paste_mask_expansion_mode,
            solder_mask_expansion_mode,
            power_plane_connect_style,
            relief_conductor_width,
            relief_entries,
            relief_air_gap,
            power_plane_relief_expansion,
            power_plane_clearance,
            corner_radius_percent,
            stack_mode: PadStackMode::Simple,
            per_layer_sizes: None,
            per_layer_shapes: None,
            per_layer_corner_radii: None,
            per_layer_offsets: None,
            net_index: json_net_index(json),
            polygon_index: json_polygon_index(json),
            component_index: json_component_index(json),
            flags: json_flags(json),
            unique_id: json_unique_id(json),
            identity_guid,
            identity_guid_b,
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
        track.net_index = json_net_index(json);
        track.polygon_index = json_polygon_index(json);
        track.component_index = json_component_index(json);
        track.solder_mask_expansion = json_f64(json, "solder_mask_expansion");
        track.keepout_restrictions = json_keepout(json);
        track.unique_id = json_unique_id(json);
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
            net_index: json_net_index(json),
            polygon_index: json_polygon_index(json),
            component_index: json_component_index(json),
            unique_id: json_unique_id(json),
            // Optional EE tail (mirrors the modelled optionals; absent keys keep
            // the default `None` so a from-scratch arc is byte-identical).
            solder_mask_expansion: json_f64(json, "solder_mask_expansion"),
            keepout_restrictions: json_keepout(json),
        })
    }

    /// Parses a region from JSON.
    pub(crate) fn parse_region(json: &Value) -> Option<crate::altium::pcblib::Region> {
        use crate::altium::pcblib::{Layer, Region, RegionKind, Vertex};

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

        // `kind` accepts a name ("copper"/"cutout") or a raw KIND integer.
        let parse_kind_str = |s: &str| match s.to_ascii_lowercase().as_str() {
            "cutout" => RegionKind::Cutout,
            "copper" => RegionKind::Copper,
            other => other
                .parse::<i32>()
                .map_or(RegionKind::Copper, RegionKind::from_id),
        };
        let kind = match json.get("kind") {
            Some(v) if v.is_string() => parse_kind_str(v.as_str().unwrap_or("copper")),
            Some(v) => v
                .as_i64()
                .and_then(|i| i32::try_from(i).ok())
                .map_or(RegionKind::Copper, RegionKind::from_id),
            None => RegionKind::Copper,
        };
        let name = json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let net_index = json_net_index(json);
        let polygon_index = json_polygon_index(json);
        let component_index = json_component_index(json);
        let cavity_height = json
            .get("cavity_height")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        // These four are always serialised by read_pcblib (no skip_serializing_if),
        // so a read-modify-write must accept AND preserve them, not reset to default.
        // Their defaults mirror Region::default() so a from-scratch region is unchanged.
        let arc_resolution = json
            .get("arc_resolution")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let sub_poly_index = json_i32(json, "sub_poly_index").unwrap_or(-1);
        let union_index = json_i32(json, "union_index").unwrap_or(0);
        let is_shape_based = json
            .get("is_shape_based")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Optional interior hole contours: an array of vertex arrays (each >= 3 pts).
        let holes: Vec<Vec<Vertex>> = json
            .get("holes")
            .and_then(Value::as_array)
            .map(|contours| {
                contours
                    .iter()
                    .filter_map(Value::as_array)
                    .map(|contour| {
                        contour
                            .iter()
                            .filter_map(|v| {
                                let x = v.get("x").and_then(Value::as_f64)?;
                                let y = v.get("y").and_then(Value::as_f64)?;
                                Some(Vertex { x, y })
                            })
                            .collect()
                    })
                    .collect()
            })
            .unwrap_or_default();

        let unique_id = json
            .get("unique_id")
            .and_then(Value::as_str)
            .map(str::to_string);

        // Round-trip unmodelled board-region keys captured on read (a `read_pcblib`
        // emits `additional_parameters` as an array of `[key, value]` pairs; accept
        // that verbatim so a modify-write preserves them). Absent -> empty -> the
        // writer appends nothing (byte-identical to a from-scratch region).
        let additional_parameters = Self::parse_additional_parameters(json);

        Some(Region {
            vertices,
            holes,
            layer,
            flags: json_flags(json),
            kind,
            name,
            net_index,
            polygon_index,
            component_index,
            arc_resolution,
            cavity_height,
            sub_poly_index,
            union_index,
            is_shape_based,
            unique_id,
            additional_parameters,
        })
    }

    /// Parses an `additional_parameters` catch-all from a primitive's JSON: an
    /// array of `[key, value]` string pairs (the shape `read_pcblib` emits for the
    /// `Vec<(String, String)>` field). Missing/malformed entries yield an empty
    /// vector, so a from-scratch primitive re-emits nothing.
    pub(crate) fn parse_additional_parameters(json: &Value) -> Vec<(String, String)> {
        json.get("additional_parameters")
            .and_then(Value::as_array)
            .map(|pairs| {
                pairs
                    .iter()
                    .filter_map(|pair| {
                        let arr = pair.as_array()?;
                        let key = arr.first()?.as_str()?;
                        let value = arr.get(1)?.as_str()?;
                        Some((key.to_string(), value.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Parses text from JSON.
    pub(crate) fn parse_text(json: &Value) -> Option<crate::altium::pcblib::Text> {
        use crate::altium::pcblib::{Layer, StrokeFont, Text, TextJustification, TextKind};

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

        // Style/font fields are now authored from JSON instead of being hard-coded.
        // The string enums (`kind`, `stroke_font`, `justification`) deserialise via
        // serde so the accepted tokens match exactly what `read_pcblib` emits; an
        // absent or unparseable value falls back to the from-scratch default (which
        // keeps a default text byte-identical to the template).
        let kind = json
            .get("kind")
            .and_then(|v| serde_json::from_value::<TextKind>(v.clone()).ok())
            .unwrap_or_default();
        let stroke_font = json
            .get("stroke_font")
            .and_then(|v| serde_json::from_value::<StrokeFont>(v.clone()).ok());
        let italic = json.get("italic").and_then(Value::as_bool).unwrap_or(false);
        let bold = json.get("bold").and_then(Value::as_bool).unwrap_or(false);
        let mirror = json.get("mirror").and_then(Value::as_bool).unwrap_or(false);
        // Comment/Designator field markers (geometry @40/@41). Absent -> false,
        // the template bytes, so an unspecified text stays byte-identical.
        let is_comment = json
            .get("is_comment")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_designator = json
            .get("is_designator")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let font_name = json
            .get("font_name")
            .and_then(Value::as_str)
            .map_or_else(|| "Arial".to_string(), ToString::to_string);
        // The from-scratch default is `BottomLeft` (encodes to the template's
        // 0x03 byte, keeping a default text byte-identical).
        let justification = json
            .get("justification")
            .and_then(|v| serde_json::from_value::<TextJustification>(v.clone()).ok())
            .unwrap_or(TextJustification::BottomLeft);

        // Inverted (knockout) text-box descriptor. Absent booleans default to
        // false and absent dimensions to `None`, keeping a default text
        // byte-identical to the template.
        let is_inverted = json
            .get("is_inverted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let inverted_border = json.get("inverted_border").and_then(Value::as_f64);
        let use_inverted_rectangle = json
            .get("use_inverted_rectangle")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let inverted_rect_width = json.get("inverted_rect_width").and_then(Value::as_f64);
        let inverted_rect_height = json.get("inverted_rect_height").and_then(Value::as_f64);
        let inverted_rect_text_offset = json
            .get("inverted_rect_text_offset")
            .and_then(Value::as_f64);

        Some(Text {
            x,
            y,
            text: text.to_string(),
            height,
            layer,
            rotation,
            kind,
            stroke_font,
            stroke_width,
            italic,
            bold,
            mirror,
            is_comment,
            is_designator,
            font_name,
            justification,
            is_inverted,
            inverted_border,
            use_inverted_rectangle,
            inverted_rect_width,
            inverted_rect_height,
            inverted_rect_text_offset,
            flags: json_flags(json),
            net_index: json_net_index(json),
            polygon_index: json_polygon_index(json),
            component_index: json_component_index(json),
            unique_id: json_unique_id(json),
        })
    }

    /// Parses a via from JSON.
    ///
    /// Mirrors [`Self::parse_pad`]'s layer-name parsing for the `from_layer` /
    /// Parses a `ComponentBody` (3D body) from JSON. Shared by the write-tool
    /// create path (`call_write_pcblib`) and the in-place update path
    /// (`update_pcblib_component`) so neither can silently drop bodies or drift.
    /// Every field defaults to the exact value the create handler used to
    /// hard-code, so a from-scratch body stays byte-identical (oracle 0).
    #[allow(clippy::too_many_lines)] // ComponentBody has many optional fields
    pub(crate) fn parse_component_body_json(
        body_json: &Value,
    ) -> crate::altium::pcblib::ComponentBody {
        use crate::altium::pcblib::{ComponentBody, Layer};

        let layer = body_json
            .get("layer")
            .and_then(Value::as_str)
            .and_then(Layer::parse)
            .unwrap_or(Layer::Top3DBody);
        let outline = body_json
            .get("outline")
            .and_then(Value::as_array)
            .map(|verts| {
                verts
                    .iter()
                    .filter_map(|v| {
                        Some((
                            v.get("x").and_then(Value::as_f64)?,
                            v.get("y").and_then(Value::as_f64)?,
                        ))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let f = |k: &str| body_json.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let str_or = |k: &str, d: &str| {
            body_json
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or(d)
                .to_string()
        };
        ComponentBody {
            model_id: str_or("model_id", ""),
            model_name: str_or("model_name", ""),
            embedded: body_json
                .get("embedded")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            rotation_x: f("rotation_x"),
            rotation_y: f("rotation_y"),
            rotation_z: f("rotation_z"),
            z_offset: f("z_offset"),
            overall_height: f("overall_height"),
            standoff_height: f("standoff_height"),
            layer,
            outline,
            unique_id: body_json
                .get("unique_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            model_checksum: body_json
                .get("model_checksum")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            name: str_or("name", " "),
            kind: body_json
                .get("kind")
                .and_then(Value::as_u64)
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(0),
            sub_poly_index: body_json
                .get("sub_poly_index")
                .and_then(Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .unwrap_or(-1),
            union_index: body_json
                .get("union_index")
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0),
            is_shape_based: body_json
                .get("is_shape_based")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            body_projection: body_json
                .get("body_projection")
                .and_then(Value::as_u64)
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(0),
            body_color_3d: body_json
                .get("body_color_3d")
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(8_421_504),
            body_opacity_3d: body_json
                .get("body_opacity_3d")
                .and_then(Value::as_f64)
                .unwrap_or(1.0),
            model_2d_rotation: body_json
                .get("model_2d_rotation")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            net_index: body_json
                .get("net_index")
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .unwrap_or(0xFFFF),
            polygon_index: body_json
                .get("polygon_index")
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .unwrap_or(0xFFFF),
            component_index: body_json
                .get("component_index")
                .and_then(Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .unwrap_or(-1),
            additional_parameters: Self::parse_additional_parameters(body_json),
        }
    }

    /// `to_layer` fields and reuses [`crate::altium::pcblib::MaskExpansionMode`]
    /// string parsing for the mask mode. Optionals default exactly as
    /// [`crate::altium::pcblib::Via::new`] does when absent.
    #[allow(clippy::too_many_lines)] // Via has many optional fields requiring individual parsing
    pub(crate) fn parse_via(json: &Value) -> Result<crate::altium::pcblib::Via, String> {
        use crate::altium::pcblib::{Layer, MaskExpansionMode, PowerPlaneConnectStyle, Via};

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

        // Power-plane connection (SubRecord-1 @31/@42/@46) + paste-mask @50 +
        // net index @3. Absent keys keep the from-scratch defaults (= Altium's
        // via template), so an unspecified via round-trips byte-identically.
        if let Some(s) = json
            .get("power_plane_connect_style")
            .and_then(Value::as_str)
        {
            via.power_plane_connect_style = match s.to_lowercase().as_str() {
                "direct" => PowerPlaneConnectStyle::Direct,
                "no_connect" | "noconnect" => PowerPlaneConnectStyle::NoConnect,
                _ => PowerPlaneConnectStyle::Relief,
            };
        }
        if let Some(v) = json
            .get("power_plane_relief_expansion")
            .and_then(Value::as_f64)
        {
            via.power_plane_relief_expansion = v;
        }
        if let Some(v) = json.get("power_plane_clearance").and_then(Value::as_f64) {
            via.power_plane_clearance = v;
        }
        if let Some(v) = json.get("paste_mask_expansion").and_then(Value::as_f64) {
            via.paste_mask_expansion = v;
        }
        if let Some(v) = json
            .get("net_index")
            .and_then(Value::as_u64)
            .and_then(|v| u16::try_from(v).ok())
        {
            via.net_index = v;
        }
        // Polygon @5 / component @7 connectivity indices. Absent keys keep the
        // from-scratch defaults (none / free primitive), byte-identical.
        via.polygon_index = json_polygon_index(json);
        via.component_index = json_component_index(json);

        // Drill tolerances (SubRecord-1 @291/@295). Absent keys keep the
        // from-scratch defaults (tolerances unset), so an unspecified via
        // round-trips byte-identically.
        if let Some(v) = json.get("hole_positive_tolerance").and_then(Value::as_f64) {
            via.hole_positive_tolerance = Some(v);
        }
        if let Some(v) = json.get("hole_negative_tolerance").and_then(Value::as_f64) {
            via.hole_negative_tolerance = Some(v);
        }

        via.flags = json_flags(json);
        via.unique_id = json_unique_id(json);

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
        fill.net_index = json_net_index(json);
        fill.polygon_index = json_polygon_index(json);
        fill.component_index = json_component_index(json);
        fill.solder_mask_expansion = json.get("solder_mask_expansion").and_then(Value::as_f64);
        fill.keepout_restrictions = json_keepout(json);
        fill.unique_id = json_unique_id(json);

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
                "leftrightsignalflow" | "leftright" => PinSymbol::LeftRightSignalFlow,
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

        // Authoring fields these previously hard-coded; read each from JSON so an
        // AI can set them, matching the names `read_schlib` exposes (serialised
        // straight from the `Pin` struct). `colour` is a BGR integer; absent keys
        // keep the from-scratch defaults (`part_and_sequence` defaults to "|&|").
        let description = json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let colour = json.get("colour").and_then(Value::as_u64).unwrap_or(0) as u32;
        let graphically_locked = json
            .get("graphically_locked")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let swap_id_group = json
            .get("swap_id_group")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let part_and_sequence = json
            .get("part_and_sequence")
            .and_then(Value::as_str)
            .unwrap_or("|&|")
            .to_string();
        let default_value = json
            .get("default_value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // Pin binary-record display mode (own byte, distinct from the shape flag).
        let owner_part_display_mode = json_i32(json, "owner_part_display_mode").unwrap_or(0);
        // Symbol line width; 0 (default) writes no PinSymbolLineWidth aux stream.
        let symbol_line_width = json_i32(json, "symbol_line_width").unwrap_or(0);
        // Fractional pin coordinates ({x,y,length} in 1/100000 DXP units); an
        // all-zero or absent object writes no PinFrac aux stream.
        let frac = json.get("frac").and_then(|f| {
            let pf = crate::altium::schlib::PinFrac {
                x: json_i32(f, "x").unwrap_or(0),
                y: json_i32(f, "y").unwrap_or(0),
                length: json_i32(f, "length").unwrap_or(0),
            };
            (!pf.is_zero()).then_some(pf)
        });
        // Both fields are always serialised by read_schlib (no skip_serializing_if),
        // so a read-modify-write round-trip must accept and preserve them rather than
        // reset them to a hard-coded default. `is_not_accessible` defaults false (the
        // pin conglomerate `0x20` bit); `formal_type` defaults 1 (Altium's normal pin).
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let formal_type = json
            .get("formal_type")
            .and_then(Value::as_u64)
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(1);

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
            description,
            owner_part_id,
            owner_part_display_mode,
            colour,
            graphically_locked,
            symbol_inner_edge,
            symbol_outer_edge,
            symbol_inside,
            symbol_outside,
            is_not_accessible,
            formal_type,
            swap_id_group,
            part_and_sequence,
            default_value,
            symbol_line_width,
            frac,
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
        // Style fields these previously hard-coded; read from JSON (matches the
        // names `read_schlib` exposes). `line_style`: 0=Solid, 1=Dashed, 2=Dotted.
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Rectangle {
            x1,
            y1,
            x2,
            y2,
            line_width,
            line_color,
            fill_color,
            line_style,
            filled,
            transparent,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
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
        // Style fields these previously hard-coded; read from JSON (matches the
        // names `read_schlib` exposes). `line_style`: 0=Solid, 1=Dashed, 2=Dotted.
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
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
            line_style,
            filled,
            transparent,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
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
        // `line_style` previously hard-coded; read from JSON (matches the name
        // `read_schlib` exposes). 0=Solid, 1=Dashed, 2=Dotted.
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        // `is_not_accessible` previously hard-coded true; read from JSON (matches
        // the name `read_schlib` exposes). Altium tags every line, so default true.
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Line {
            x1,
            y1,
            x2,
            y2,
            line_width,
            color,
            line_style,
            is_not_accessible,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
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
        // De-hardcoded: the core already models these, so read them from JSON.
        // Defaults equal the previous hard-coded values, keeping a default
        // parameter byte-identical.
        let read_only_state = json
            .get("read_only_state")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
        let param_type = json.get("param_type").and_then(Value::as_u64).unwrap_or(0) as u8;
        let unique_id = json
            .get("unique_id")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        // EE-meaningful display fields (omit-when-default).
        let orientation = json_i32(json, "orientation").unwrap_or(0);
        let show_name = json
            .get("show_name")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let hide_name = json
            .get("hide_name")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let description = json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let is_configurable = json
            .get("is_configurable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Parameter {
            name: name.to_string(),
            value,
            x,
            y,
            font_id,
            color,
            hidden,
            read_only_state,
            param_type,
            orientation,
            show_name,
            hide_name,
            description,
            is_configurable,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id,
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
        // Style + arrowhead fields these previously hard-coded; read from JSON
        // (matches the names `read_schlib` exposes). `line_style`: 0=Solid,
        // 1=Dashed, 2=Dotted. `start_line_shape`/`end_line_shape` are endpoint
        // (arrowhead) shapes and `line_shape_size` their size.
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let start_line_shape = json
            .get("start_line_shape")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
        let end_line_shape = json
            .get("end_line_shape")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
        let line_shape_size = json
            .get("line_shape_size")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Polyline {
            points,
            line_width,
            color,
            line_style,
            start_line_shape,
            end_line_shape,
            line_shape_size,
            transparent,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
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
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Polygon {
            points,
            line_width,
            line_color,
            fill_color,
            line_style,
            filled,
            transparent,
            is_not_accessible,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
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
        // `fill_color` previously hard-coded to 0; read from JSON (matches the name
        // `read_schlib` exposes). Maps to the `AreaColor` param; 0 = no fill.
        let fill_color = json.get("fill_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        // `is_not_accessible` previously hard-coded true; read from JSON (matches
        // the name `read_schlib` exposes). Altium tags every arc, so default true.
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Arc {
            x,
            y,
            radius,
            is_not_accessible,
            start_angle,
            end_angle,
            line_width,
            color,
            fill_color,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
        })
    }

    /// Parses a schematic pie (filled circular sector, `RECORD=9`) from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_pie(json: &Value) -> Option<crate::altium::schlib::Pie> {
        use crate::altium::schlib::Pie;

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
        let line_color = json.get("line_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let fill_color = json.get("fill_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Pie {
            x,
            y,
            radius,
            is_not_accessible,
            start_angle,
            end_angle,
            line_width,
            line_color,
            fill_color,
            filled,
            transparent,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
        })
    }

    /// Parses a schematic image (embedded/linked picture, `RECORD=30`) from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_image(json: &Value) -> Option<crate::altium::schlib::Image> {
        use crate::altium::schlib::Image;

        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;
        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json.get("line_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let fill_color = json.get("fill_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let b = |k: &str| json.get(k).and_then(Value::as_bool).unwrap_or(false);
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let file_name = json
            .get("file_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // Base64-encoded raw image bytes destined for the library /Storage
        // stream. Invalid base64 is treated as absent (this parser is lenient
        // Option-style throughout), with a debug log for diagnosis.
        let image_data = json
            .get("image_data")
            .and_then(Value::as_str)
            .and_then(|s| {
                use base64::Engine as _;
                match base64::engine::general_purpose::STANDARD.decode(s.as_bytes()) {
                    Ok(bytes) => Some(bytes),
                    Err(e) => {
                        tracing::debug!(error = %e, "invalid base64 in image_data; ignoring");
                        None
                    }
                }
            });
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Image {
            x1,
            y1,
            x2,
            y2,
            is_not_accessible,
            line_width,
            line_color,
            line_style,
            fill_color,
            filled: b("filled"),
            transparent: b("transparent"),
            show_border: b("show_border"),
            keep_aspect: b("keep_aspect"),
            embed_image: b("embed_image"),
            file_name,
            image_data,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
        })
    }

    /// Parses a schematic text frame (bordered multi-line text box,
    /// `RECORD=28`) from JSON. Requires the frame box (`x1`..`y2`) and `text`;
    /// optionals default as [`crate::altium::schlib::TextFrame::new`] does when
    /// absent (white fill, centre alignment, border/word-wrap/clip-to-rect on).
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_text_frame(
        json: &Value,
    ) -> Option<crate::altium::schlib::TextFrame> {
        use crate::altium::schlib::TextFrame;

        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;
        let text = json.get("text").and_then(Value::as_str)?.to_string();
        let color = json.get("color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let area_color = json
            .get("area_color")
            .and_then(Value::as_u64)
            .unwrap_or(16_777_215) as u32;
        let text_color = json.get("text_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let text_margin = json
            .get("text_margin")
            .and_then(Value::as_f64)
            .unwrap_or(0.000_05);
        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(0) as u8;
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let font_id = json.get("font_id").and_then(Value::as_u64).unwrap_or(1) as u8;
        let orientation = json.get("orientation").and_then(Value::as_u64).unwrap_or(0) as u8;
        let alignment = json.get("alignment").and_then(Value::as_u64).unwrap_or(1) as u8;
        let b_false = |k: &str| json.get(k).and_then(Value::as_bool).unwrap_or(false);
        let b_true = |k: &str| json.get(k).and_then(Value::as_bool).unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(TextFrame {
            x1,
            y1,
            x2,
            y2,
            text,
            color,
            area_color,
            text_color,
            text_margin,
            line_width,
            line_style,
            transparent: b_false("transparent"),
            font_id,
            orientation,
            alignment,
            is_solid: b_false("is_solid"),
            show_border: b_true("show_border"),
            word_wrap: b_true("word_wrap"),
            clip_to_rect: b_true("clip_to_rect"),
            is_not_accessible: b_true("is_not_accessible"),
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
        })
    }

    /// Parses a `SchLib` Bezier from JSON. Requires the four control points
    /// (`x1`..`y4`); optionals default as [`crate::altium::schlib::Bezier::new`]
    /// does when absent.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_bezier(json: &Value) -> Option<crate::altium::schlib::Bezier> {
        use crate::altium::schlib::Bezier;

        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;
        let x3 = json_f64(json, "x3")?;
        let y3 = json_f64(json, "y3")?;
        let x4 = json_f64(json, "x4")?;
        let y4 = json_f64(json, "y4")?;
        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Bezier {
            x1,
            y1,
            x2,
            y2,
            x3,
            y3,
            x4,
            y4,
            line_width,
            color,
            is_not_accessible,
            owner_part_id,
            unique_id: json_unique_id(json),
        })
    }

    /// Parses a `SchLib` elliptical arc from JSON. Requires centre and both
    /// radii; optionals default as
    /// [`crate::altium::schlib::EllipticalArc::new`] does when absent.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_elliptical_arc(
        json: &Value,
    ) -> Option<crate::altium::schlib::EllipticalArc> {
        use crate::altium::schlib::EllipticalArc;

        let x = json_f64(json, "x")?;
        let y = json_f64(json, "y")?;
        let radius = json_f64(json, "radius")?;
        let secondary_radius = json_f64(json, "secondary_radius")?;
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
        let fill_color = json.get("fill_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(EllipticalArc {
            x,
            y,
            radius,
            secondary_radius,
            start_angle,
            end_angle,
            line_width,
            color,
            fill_color,
            owner_part_id,
            unique_id: json_unique_id(json),
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
        // `transparent` previously hard-coded; read from JSON (matches the name
        // `read_schlib` exposes). The ellipse struct carries no `line_style`.
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
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
            transparent,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
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
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
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
            unique_id: json_unique_id(json),
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
    fn parse_pad_reads_plating_and_identity_guids() {
        // is_plated @60 and the two identity GUIDs @126/@142 flow from JSON so
        // a read-modify-write preserves them.
        let pad = McpServer::parse_pad(&json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
            "is_plated": false,
            "identity_guid": "{A5172B29-10E4-C726-929A-64E441352E67}",
            "identity_guid_b": "{00000000-0000-0000-0000-000000000000}",
        }))
        .expect("pad should parse");
        assert!(!pad.is_plated);
        assert_eq!(
            pad.identity_guid.as_deref(),
            Some("{A5172B29-10E4-C726-929A-64E441352E67}")
        );
        assert_eq!(
            pad.identity_guid_b.as_deref(),
            Some("{00000000-0000-0000-0000-000000000000}")
        );

        // Absent keys keep the from-scratch defaults: plated (Altium's default
        // for every pad) and fresh writer-generated GUIDs (None).
        let bare = McpServer::parse_pad(&json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
        }))
        .expect("bare pad should parse");
        assert!(bare.is_plated);
        assert_eq!(bare.identity_guid, None);
        assert_eq!(bare.identity_guid_b, None);
    }

    #[test]
    fn parse_pad_reads_thermal_relief_fields() {
        use crate::altium::pcblib::PowerPlaneConnectStyle;
        // Non-default thermal-relief / power-plane keys parse into the model.
        let pad = McpServer::parse_pad(&json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
            "power_plane_connect_style": "direct",
            "relief_conductor_width": 0.3,
            "relief_entries": 2,
            "relief_air_gap": 0.2,
            "power_plane_relief_expansion": 0.6,
            "power_plane_clearance": 0.7,
        }))
        .expect("pad should parse");
        assert_eq!(
            pad.power_plane_connect_style,
            PowerPlaneConnectStyle::Direct
        );
        assert!((pad.relief_conductor_width - 0.3).abs() < 1e-9);
        assert_eq!(pad.relief_entries, 2);
        assert!((pad.relief_air_gap - 0.2).abs() < 1e-9);
        assert!((pad.power_plane_relief_expansion - 0.6).abs() < 1e-9);
        assert!((pad.power_plane_clearance - 0.7).abs() < 1e-9);
    }

    #[test]
    fn parse_pad_thermal_relief_defaults() {
        use crate::altium::pcblib::PowerPlaneConnectStyle;
        // Absent keys keep the from-scratch defaults (= Altium's pad template).
        let pad = McpServer::parse_pad(&json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
        }))
        .expect("pad should parse");
        assert_eq!(
            pad.power_plane_connect_style,
            PowerPlaneConnectStyle::Relief
        );
        assert!((pad.relief_conductor_width - 0.254).abs() < 1e-9);
        assert_eq!(pad.relief_entries, 4);
        assert!((pad.relief_air_gap - 0.254).abs() < 1e-9);
        assert!((pad.power_plane_relief_expansion - 0.508).abs() < 1e-9);
        assert!((pad.power_plane_clearance - 0.508).abs() < 1e-9);
    }

    #[test]
    fn parse_via_reads_power_plane_and_flags() {
        use crate::altium::pcblib::{PcbFlags, PowerPlaneConnectStyle};
        // PR-7: power-plane connection, paste-mask, net index and flags parse in.
        let via = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.8, "hole_size": 0.4,
            "power_plane_connect_style": "direct",
            "power_plane_relief_expansion": 0.6,
            "power_plane_clearance": 0.7,
            "paste_mask_expansion": 0.05,
            "net_index": 42,
            "flags": "TENTING_TOP | LOCKED",
        }))
        .expect("via should parse");
        assert_eq!(
            via.power_plane_connect_style,
            PowerPlaneConnectStyle::Direct
        );
        assert!((via.power_plane_relief_expansion - 0.6).abs() < 1e-9);
        assert!((via.power_plane_clearance - 0.7).abs() < 1e-9);
        assert!((via.paste_mask_expansion - 0.05).abs() < 1e-9);
        assert_eq!(via.net_index, 42);
        assert!(via.flags.contains(PcbFlags::TENTING_TOP));
        assert!(via.flags.contains(PcbFlags::LOCKED));
    }

    #[test]
    fn parse_via_defaults_match_template() {
        use crate::altium::pcblib::{PcbFlags, PowerPlaneConnectStyle};
        // Absent keys keep the from-scratch defaults (= Altium's via template).
        let via = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.8, "hole_size": 0.4,
        }))
        .expect("via should parse");
        assert_eq!(
            via.power_plane_connect_style,
            PowerPlaneConnectStyle::Relief
        );
        assert!((via.power_plane_relief_expansion - 0.508).abs() < 1e-9);
        assert!((via.power_plane_clearance - 0.508).abs() < 1e-9);
        assert!((via.paste_mask_expansion - 0.0).abs() < 1e-9);
        assert_eq!(via.net_index, 0xFFFF);
        assert_eq!(via.flags, PcbFlags::empty());
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
    fn parse_region_reads_additional_parameters() {
        // PR-R5: the read DTO's `additional_parameters` (an array of [key, value]
        // pairs) must land on the struct so a read-modify-write preserves them.
        let region = McpServer::parse_region(&json!({
            "layer": "Top Courtyard",
            "vertices": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 0.0}, {"x": 0.0, "y": 1.0}],
            "additional_parameters": [["LAYER", "TOP"], ["LAYERSTACKID", "7"]],
        }))
        .expect("region should parse");
        assert_eq!(
            region.additional_parameters,
            vec![
                ("LAYER".to_string(), "TOP".to_string()),
                ("LAYERSTACKID".to_string(), "7".to_string()),
            ],
        );
    }

    #[test]
    fn parse_region_additional_parameters_default_empty() {
        // Absent -> empty, so a from-scratch region re-emits nothing (byte-identical).
        let region = McpServer::parse_region(&json!({
            "layer": "Top Courtyard",
            "vertices": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 0.0}, {"x": 0.0, "y": 1.0}],
        }))
        .expect("region should parse");
        assert!(region.additional_parameters.is_empty());
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

    #[test]
    fn parse_text_reads_authoring_fields() {
        // PR-10: kind/stroke_font/italic/bold/mirror/font_name/justification were
        // previously hard-coded; each must now flow from JSON onto the struct.
        use crate::altium::pcblib::{StrokeFont, TextJustification, TextKind};
        let text = McpServer::parse_text(&json!({
            "x": 0.0, "y": 0.0, "text": "REF", "height": 0.5, "layer": "Top Overlay",
            "kind": "true_type",
            "stroke_font": "serif",
            "italic": true,
            "bold": true,
            "mirror": true,
            "is_comment": true,
            "is_designator": true,
            "font_name": "Times New Roman",
            "justification": "top_right",
        }))
        .expect("text should parse");
        assert_eq!(text.kind, TextKind::TrueType);
        assert_eq!(text.stroke_font, Some(StrokeFont::Serif));
        assert!(text.italic);
        assert!(text.bold);
        assert!(text.mirror);
        assert!(text.is_comment);
        assert!(text.is_designator);
        assert_eq!(text.font_name, "Times New Roman");
        assert_eq!(text.justification, TextJustification::TopRight);
    }

    #[test]
    fn parse_text_defaults_are_template_identical() {
        // A minimal text must keep the from-scratch defaults (stroke, no font
        // override, Arial, middle-center) so it stays byte-identical on write.
        use crate::altium::pcblib::{TextJustification, TextKind};
        let text = McpServer::parse_text(&json!({
            "x": 0.0, "y": 0.0, "text": "REF", "height": 0.5, "layer": "Top Overlay",
        }))
        .expect("text should parse");
        assert_eq!(text.kind, TextKind::Stroke);
        assert_eq!(text.stroke_font, None);
        assert!(!text.italic);
        assert!(!text.bold);
        assert!(!text.mirror);
        assert!(!text.is_comment, "absent is_comment stays template false");
        assert!(
            !text.is_designator,
            "absent is_designator stays template false"
        );
        assert_eq!(text.font_name, "Arial");
        assert_eq!(text.justification, TextJustification::BottomLeft);
    }

    // --- PR-12/PR-13: SchLib write-path authoring fields. These were previously
    // hard-coded in the parsers, so the structs round-tripped them on read but no
    // JSON value reached them on write. Each test sets a non-default value and
    // asserts it lands on the struct (the field names match the read DTO).

    #[test]
    fn parse_schlib_pin_reads_authoring_fields() {
        let pin = McpServer::parse_schlib_pin(&json!({
            "designator": "1", "name": "P1", "x": 0, "y": 0, "length": 10,
            "orientation": "left",
            "description": "clock input",
            "colour": 0x00_FF_00,
            "graphically_locked": true,
            "swap_id_group": "grpA",
            "part_and_sequence": "|1&2|",
            "default_value": "0",
            "owner_part_display_mode": 2,
            "symbol_line_width": 3,
            "frac": { "x": 50000, "y": -25000, "length": 0 },
        }))
        .expect("pin should parse");
        assert_eq!(pin.description, "clock input");
        assert_eq!(pin.colour, 0x00_FF_00);
        assert!(pin.graphically_locked);
        assert_eq!(pin.swap_id_group, "grpA");
        assert_eq!(pin.part_and_sequence, "|1&2|");
        assert_eq!(pin.default_value, "0");
        assert_eq!(pin.owner_part_display_mode, 2);
        assert_eq!(pin.symbol_line_width, 3);
        assert_eq!(
            pin.frac,
            Some(crate::altium::schlib::PinFrac {
                x: 50000,
                y: -25000,
                length: 0
            })
        );
    }

    #[test]
    fn parse_schlib_pin_defaults_match_struct() {
        // Absent authoring keys keep the from-scratch defaults (notably the
        // `|&|` part_and_sequence Altium uses for a fresh pin).
        let pin = McpServer::parse_schlib_pin(&json!({
            "designator": "1", "name": "P1", "x": 0, "y": 0, "length": 10,
            "orientation": "left",
        }))
        .expect("pin should parse");
        assert_eq!(pin.description, "");
        assert_eq!(pin.colour, 0);
        assert!(!pin.graphically_locked);
        assert_eq!(pin.swap_id_group, "");
        assert_eq!(pin.part_and_sequence, "|&|");
        assert_eq!(pin.default_value, "");
        // PR-R3 aux fields default so no aux stream is written for a plain pin.
        assert_eq!(pin.owner_part_display_mode, 0);
        assert_eq!(pin.symbol_line_width, 0);
        assert_eq!(pin.frac, None);
    }

    #[test]
    fn parse_schlib_pin_reads_open_collector_electrical_type() {
        use crate::altium::schlib::PinElectricalType;
        let oc = McpServer::parse_schlib_pin(&json!({
            "designator": "1", "name": "P1", "x": 0, "y": 0, "length": 10,
            "orientation": "left", "electrical_type": "open_collector",
        }))
        .expect("pin should parse");
        assert_eq!(oc.electrical_type, PinElectricalType::OpenCollector);
        // `tristate` is the advertised alias for HiZ.
        let tri = McpServer::parse_schlib_pin(&json!({
            "designator": "2", "name": "P2", "x": 0, "y": 0, "length": 10,
            "orientation": "left", "electrical_type": "tristate",
        }))
        .expect("pin should parse");
        assert_eq!(tri.electrical_type, PinElectricalType::HiZ);
    }

    #[test]
    fn parse_schlib_rectangle_reads_line_style_and_transparent() {
        let rect = McpServer::parse_schlib_rectangle(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 10.0,
            "line_style": 2, "transparent": true,
        }))
        .expect("rectangle should parse");
        assert_eq!(rect.line_style, 2);
        assert!(rect.transparent);
    }

    #[test]
    fn parse_schlib_round_rect_reads_line_style_and_transparent() {
        let rr = McpServer::parse_schlib_round_rect(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 10.0,
            "corner_x_radius": 2.0, "corner_y_radius": 2.0,
            "line_style": 1, "transparent": true,
        }))
        .expect("round_rect should parse");
        assert_eq!(rr.line_style, 1);
        assert!(rr.transparent);
    }

    #[test]
    fn parse_schlib_line_reads_line_style() {
        let line = McpServer::parse_schlib_line(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 0.0, "line_style": 2,
        }))
        .expect("line should parse");
        assert_eq!(line.line_style, 2);
    }

    #[test]
    fn parse_schlib_polyline_reads_style_and_arrowheads() {
        let pl = McpServer::parse_schlib_polyline(&json!({
            "points": [{"x": 0.0, "y": 0.0}, {"x": 10.0, "y": 0.0}],
            "line_style": 1,
            "start_line_shape": 2,
            "end_line_shape": 3,
            "line_shape_size": 4,
            "transparent": true,
        }))
        .expect("polyline should parse");
        assert_eq!(pl.line_style, 1);
        assert_eq!(pl.start_line_shape, 2);
        assert_eq!(pl.end_line_shape, 3);
        assert_eq!(pl.line_shape_size, 4);
        assert!(pl.transparent);
    }

    #[test]
    fn parse_schlib_arc_reads_fill_color() {
        let arc = McpServer::parse_schlib_arc(&json!({
            "x": 0.0, "y": 0.0, "radius": 5.0, "fill_color": 0x11_22_33,
        }))
        .expect("arc should parse");
        assert_eq!(arc.fill_color, 0x11_22_33);
    }

    #[test]
    fn parse_schlib_ellipse_reads_transparent() {
        let el = McpServer::parse_schlib_ellipse(&json!({
            "x": 0.0, "y": 0.0, "radius_x": 5.0, "radius_y": 3.0, "transparent": true,
        }))
        .expect("ellipse should parse");
        assert!(el.transparent);
    }

    // --- PR-R1: round-trip preservation of a primitive's `unique_id` (identity
    // GUID). The write-tool parsers previously hard-coded `unique_id: None`,
    // dropping whatever the reader surfaced; these lock the accept-fix. Absent
    // `unique_id` MUST stay `None` (the writer then auto-generates, keeping
    // from-scratch output byte-identical).

    #[test]
    fn json_unique_id_reads_and_defaults() {
        assert_eq!(
            super::json_unique_id(&json!({ "unique_id": "QHHMRSCB" })).as_deref(),
            Some("QHHMRSCB")
        );
        // Absent -> None, so the writer auto-generates exactly as before.
        assert_eq!(super::json_unique_id(&json!({})), None);
    }

    #[test]
    fn parse_via_preserves_provided_unique_id() {
        let via = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
            "unique_id": "VIAUID01",
        }))
        .expect("via should parse");
        assert_eq!(via.unique_id.as_deref(), Some("VIAUID01"));
    }

    #[test]
    fn parse_via_without_unique_id_defaults_none() {
        // From-scratch: no unique_id -> None (writer auto-generates; byte-identical).
        let via = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
        }))
        .expect("via should parse");
        assert_eq!(via.unique_id, None);
    }

    #[test]
    fn parse_schlib_rectangle_preserves_provided_unique_id() {
        let rect = McpServer::parse_schlib_rectangle(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 10.0,
            "unique_id": "RECTUID1",
        }))
        .expect("rectangle should parse");
        assert_eq!(rect.unique_id.as_deref(), Some("RECTUID1"));
        // From-scratch -> None (writer auto-generates; byte-identical).
        let plain = McpServer::parse_schlib_rectangle(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 10.0,
        }))
        .expect("rectangle should parse");
        assert_eq!(plain.unique_id, None);
    }
}
