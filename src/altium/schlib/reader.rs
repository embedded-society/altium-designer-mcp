//! Binary reader for `SchLib` Data streams.
//!
//! This module handles parsing the binary format of Altium `SchLib` Data streams,
//! which contain the primitives (pins, rectangles, lines, etc.) that make up symbols.
//!
//! # Data Stream Format
//!
//! ```text
//! [length:3 LE][flags:1][data:length]
//! ...
//! ```
//!
//! The 4-byte header is one 32-bit little-endian size word: low 24 bits = payload
//! length, high byte = flag. There is NO end-of-stream marker — records run until
//! the stream is exhausted (a trailing `0x0000` would be mis-read as a zero-length
//! record; see issue #68). The reader stops on a zero length defensively.
//!
//! # Record Types (flag byte)
//!
//! - `0x00`: Text record (pipe-delimited key=value)
//! - `0x01`: Binary pin record

use super::primitives::{
    Arc, Bezier, Ellipse, EllipticalArc, FootprintModel, Label, Line, Parameter, Pin,
    PinElectricalType, PinOrientation, PinSymbol, Polygon, Polyline, Rectangle, RoundRect,
    ShapeDisplayFlags, Text, TextJustification,
};
use super::Symbol;
use crate::altium::bytes::{
    read_i16_le as read_i16, read_i32_le as read_i32, read_u32_le as read_u32,
};
use std::collections::HashMap;

/// Parses primitives from a `SchLib` Data stream.
pub fn parse_data_stream(symbol: &mut Symbol, data: &[u8]) {
    if data.len() < 4 {
        tracing::warn!("Data stream too short");
        return;
    }

    let mut offset = 0;

    // Parse records until end marker or end of data
    while offset + 4 <= data.len() {
        // Read header: Altium's [u24 length LE][u8 flags]. For records under
        // 16 MiB (always, in practice) the third length byte is 0, so this also
        // reads our older [u16 length LE][u16 BE type] frames identically.
        let record_length =
            u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], 0]) as usize;
        let record_type = u16::from(data[offset + 3]);

        if record_length == 0 {
            // End marker
            break;
        }

        if offset + 4 + record_length > data.len() {
            tracing::warn!("Record extends beyond data at offset {offset:#x}");
            break;
        }

        let record_data = &data[offset + 4..offset + 4 + record_length];

        match record_type {
            0 => {
                // Text record (pipe-delimited key=value)
                parse_text_record(symbol, record_data);
            }
            1 => {
                // Binary pin record
                if let Some(pin) = parse_binary_pin(record_data) {
                    symbol.add_pin(pin);
                }
            }
            _ => {
                tracing::debug!("Unknown record type {record_type:#x} at offset {offset:#x}");
            }
        }

        offset += 4 + record_length;
    }
}

/// Parses a text record (pipe-delimited key=value pairs).
fn parse_text_record(symbol: &mut Symbol, data: &[u8]) {
    // Remove null terminator if present
    let data = data.split(|&b| b == 0).next().unwrap_or(data);

    // A record carrying a `%UTF8%`-prefixed key (e.g. `%UTF8%Text`) stores that
    // value as raw UTF-8 bytes inside an otherwise Windows-1252 record. Decode
    // the whole record as Windows-1252 so the UTF-8 value arrives as deterministic
    // one-char-per-byte "mojibake"; the field parser then re-decodes it as UTF-8
    // (see `decode_utf8_param_value`). Without the `%UTF8%` marker the record is
    // decoded exactly as before: UTF-8 when valid, else Windows-1252.
    let text = if contains_utf8_marker(data) {
        crate::altium::decode_windows1252(data)
    } else {
        std::str::from_utf8(data).map_or_else(
            |_| {
                // Decode as Windows-1252 (legacy Altium encoding)
                let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(data);
                decoded.into_owned()
            },
            str::to_string,
        )
    };

    parse_text_record_from_string(symbol, &text);
}

/// Returns `true` when the raw record bytes contain a `%UTF8%` key prefix
/// (case-insensitive), signalling that at least one value is stored as raw UTF-8
/// bytes and the record must be decoded as Windows-1252 to recover it.
fn contains_utf8_marker(data: &[u8]) -> bool {
    const MARKER: &[u8; 6] = b"%UTF8%";
    data.windows(MARKER.len())
        .any(|w| w.eq_ignore_ascii_case(MARKER))
}

/// Parses a text record from a decoded string.
#[allow(clippy::too_many_lines)] // Property parsing for all record types
fn parse_text_record_from_string(symbol: &mut Symbol, text: &str) {
    let props = parse_properties(text);

    let record_id: u32 = props
        .get("record")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    match record_id {
        1 => {
            // Component header
            // LibReference contains the full symbol name (may differ from OLE storage name)
            if let Some(name) = props.get("libreference") {
                if !name.is_empty() {
                    symbol.name.clone_from(name);
                }
            }
            if let Some(desc) = props.get("componentdescription") {
                symbol.description.clone_from(desc);
            }
            if let Some(part_count) = props.get("partcount") {
                // Altium stores part_count + 1 (part 0 is the common part), so we
                // subtract 1 when reading. No floor at 1: a single-part symbol stores
                // PartCount=1 => internal 0; flooring it to 1 round-tripped as
                // PartCount=2 (corruption). Matches AltiumSharp's Math.Max(0, dto - 1).
                let raw_count: u32 = part_count.trim().parse().unwrap_or(2);
                symbol.part_count = raw_count.saturating_sub(1);
            }
            if let Some(display_mode_count) = props.get("displaymodecount") {
                symbol.display_mode_count = display_mode_count.parse().unwrap_or(1);
            }
            if let Some(current_part_id) = props.get("currentpartid") {
                symbol.current_part_id = current_part_id.parse().unwrap_or(1);
            }
            if let Some(part_id_locked) = props.get("partidlocked") {
                symbol.part_id_locked = part_id_locked == "T";
            }
            if let Some(source_lib) = props.get("sourcelibraryname") {
                symbol.source_library_name.clone_from(source_lib);
            }
            if let Some(target_file) = props.get("targetfilename") {
                symbol.target_file_name.clone_from(target_file);
            }
        }
        14 => {
            // Rectangle
            if let Some(rect) = parse_rectangle(&props) {
                symbol.add_rectangle(rect);
            }
        }
        13 => {
            // Line
            if let Some(line) = parse_line(&props) {
                symbol.add_line(line);
            }
        }
        34 => {
            // Designator (a parameter record; its value uses the same
            // `%UTF8%Text` convention as any other text field).
            if let Some(text) = read_utf8_text_field(&props, "text") {
                symbol.designator = text;
            }
        }
        41 => {
            // Parameter
            if let Some(param) = parse_parameter(&props) {
                symbol.add_parameter(param);
            }
        }
        45 => {
            // Model (footprint reference)
            if let Some(name) = props.get("modelname") {
                let mut fp = FootprintModel::new(name);
                if let Some(desc) = props.get("description") {
                    fp.description.clone_from(desc);
                }
                // Preserve the PcbLib path (ModelDatafile0) so it round-trips.
                if let Some(path) = props.get("modeldatafile0") {
                    if !path.is_empty() {
                        fp.library_path = Some(path.clone());
                    }
                }
                // Preserve the current/default-model flag (Altium omits it when false).
                fp.is_current = props.get("iscurrent").is_some_and(|v| v == "T");
                symbol.add_footprint(fp);
            }
        }
        6 => {
            // Polyline
            if let Some(polyline) = parse_polyline(&props) {
                symbol.add_polyline(polyline);
            }
        }
        8 => {
            // Ellipse
            if let Some(ellipse) = parse_ellipse(&props) {
                symbol.add_ellipse(ellipse);
            }
        }
        12 => {
            // Arc
            if let Some(arc) = parse_arc(&props) {
                symbol.add_arc(arc);
            }
        }
        3 => {
            // Text annotation
            if let Some(text) = parse_text(&props) {
                symbol.add_text(text);
            }
        }
        4 => {
            // Label
            if let Some(label) = parse_label(&props) {
                symbol.add_label(label);
            }
        }
        5 => {
            // Bezier curve
            if let Some(bezier) = parse_bezier(&props) {
                symbol.add_bezier(bezier);
            }
        }
        7 => {
            // Polygon
            if let Some(polygon) = parse_polygon(&props) {
                symbol.add_polygon(polygon);
            }
        }
        10 => {
            // Rounded Rectangle
            if let Some(round_rect) = parse_round_rect(&props) {
                symbol.add_round_rect(round_rect);
            }
        }
        11 => {
            // Elliptical Arc
            if let Some(elliptical_arc) = parse_elliptical_arc(&props) {
                symbol.add_elliptical_arc(elliptical_arc);
            }
        }
        2 | 44 | 46 | 47 | 48 => {
            // Known but not yet implemented:
            // 2=Pin(text),
            // 44=ImplementationList, 46/47/48=Model data
            tracing::trace!("Skipping record type {record_id}");
        }
        _ => {
            tracing::debug!("Unknown text record type {record_id}");
        }
    }
}

/// Parses pipe-delimited key=value properties (shared with the rest of the
/// crate via [`crate::altium::parse_pipe_params`]).
fn parse_properties(text: &str) -> HashMap<String, String> {
    crate::altium::parse_pipe_params(text)
}

/// Parses a binary pin record.
fn parse_binary_pin(data: &[u8]) -> Option<Pin> {
    if data.len() < 20 {
        return None;
    }

    let mut offset = 0;

    // record type (4 bytes) - should be 2 for pin
    let _record_type = read_i32(data, offset)?;
    offset += 4;

    // unknown byte
    offset += 1;

    // owner_part_id (2 bytes)
    let owner_part_id = read_i16(data, offset)?;
    offset += 2;

    // owner_part_display_mode (1 byte): the pin's own alternate-view index,
    // stored in the binary record (AltiumSharp reads it here). Preserved so a
    // pin authored on a non-default display mode round-trips; the golden pins
    // carry 0, so a from-scratch pin stays byte-identical.
    let owner_part_display_mode = i32::from(data.get(offset).copied().unwrap_or(0));
    offset += 1;

    // symbol flags (4 bytes: inner_edge, outer_edge, inside, outside)
    let symbol_inner_edge = PinSymbol::from_id(data.get(offset).copied().unwrap_or(0));
    let symbol_outer_edge = PinSymbol::from_id(data.get(offset + 1).copied().unwrap_or(0));
    let symbol_inside = PinSymbol::from_id(data.get(offset + 2).copied().unwrap_or(0));
    let symbol_outside = PinSymbol::from_id(data.get(offset + 3).copied().unwrap_or(0));
    offset += 4;

    // description: Pascal short string [length:1][string]
    let (description, next) = crate::altium::framing::read_pascal_string(data, offset);
    offset = next;

    // formal_type (1 byte): preserved on round-trip (Altium emits 1).
    let formal_type = data.get(offset).copied().unwrap_or(1);
    offset += 1;

    // electrical_type (1 byte)
    let electrical_type = data.get(offset).copied().unwrap_or(4);
    offset += 1;

    // flags (1 byte)
    let flags = data.get(offset).copied().unwrap_or(0);
    offset += 1;

    let rotated = (flags & 0x01) != 0;
    let flipped = (flags & 0x02) != 0;
    let hidden = (flags & 0x04) != 0;
    let show_name = (flags & 0x08) != 0;
    let show_designator = (flags & 0x10) != 0;
    let graphically_locked = (flags & 0x40) != 0;
    let is_not_accessible = (flags & 0x20) != 0;

    // length (2 bytes)
    let length = i32::from(read_i16(data, offset).unwrap_or(10));
    offset += 2;

    // location X, Y (2 bytes each, signed)
    let x = i32::from(read_i16(data, offset).unwrap_or(0));
    offset += 2;
    let y = i32::from(read_i16(data, offset).unwrap_or(0));
    offset += 2;

    // colour (4 bytes)
    let colour = read_u32(data, offset).unwrap_or(0);
    offset += 4;

    // name: [length:1][string]
    let (name, next) = crate::altium::framing::read_pascal_string(data, offset);
    offset = next;

    // designator: [length:1][string]
    let (designator, next) = crate::altium::framing::read_pascal_string(data, offset);
    offset = next;

    // Swap-id tail (Pascal short strings), in order: swap_id_group,
    // part_and_sequence, default_value. `read_pascal_string` returns ("", offset)
    // safely past the end of a truncated record, so a legacy/short pin reads ""
    // for any absent tail field; we reproduce exactly what was read (do NOT
    // coerce an empty part_and_sequence back to "|&|" here).
    let (swap_id_group, next) = crate::altium::framing::read_pascal_string(data, offset);
    offset = next;
    let (part_and_sequence, next) = crate::altium::framing::read_pascal_string(data, offset);
    offset = next;
    let (default_value, _) = crate::altium::framing::read_pascal_string(data, offset);

    Some(Pin {
        name,
        designator,
        x,
        y,
        length,
        orientation: PinOrientation::from_flags(rotated, flipped),
        electrical_type: PinElectricalType::from_id(electrical_type),
        hidden,
        show_name,
        show_designator,
        description,
        owner_part_id: owner_part_id.into(),
        owner_part_display_mode,
        colour,
        graphically_locked,
        is_not_accessible,
        symbol_inner_edge,
        symbol_outer_edge,
        symbol_inside,
        symbol_outside,
        formal_type,
        swap_id_group,
        part_and_sequence,
        default_value,
        // Aux-stream fields, filled by apply_pin_frac / apply_pin_symbol_line_widths
        // after the whole Data stream is parsed (they are keyed by pin ordinal).
        symbol_line_width: 0,
        frac: None,
    })
}

/// Reads the four universal display/lock flags shared by every graphic shape
/// (`GRAPHICALLYLOCKED` / `DISABLED` / `DIMMED` / `OWNERPARTDISPLAYMODE`).
/// Altium omits each key when it holds its default, so an absent key defaults to
/// `false` / `0` — matching `AltiumSharp`'s `TryGetBool` / `TryGetInt`.
fn read_display_flags(props: &HashMap<String, String>) -> ShapeDisplayFlags {
    ShapeDisplayFlags {
        graphically_locked: props.get("graphicallylocked").is_some_and(|s| s == "T"),
        disabled: props.get("disabled").is_some_and(|s| s == "T"),
        dimmed: props.get("dimmed").is_some_and(|s| s == "T"),
        owner_part_display_mode: props
            .get("ownerpartdisplaymode")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
    }
}

/// Reads a record's text value, preferring a `%UTF8%`-prefixed key when present.
///
/// Altium stores a text value that Windows-1252 cannot represent (Cyrillic, CJK,
/// Greek `Ω`, …) under a `%UTF8%<Key>` key holding the raw UTF-8 bytes, instead
/// of the lossy plain `<Key>`. When that key is present its value (mojibake from
/// the Windows-1252 record decode) is re-decoded as UTF-8; otherwise the plain
/// `<Key>` is read verbatim. `key` is the lower-cased field name (e.g. `"text"`),
/// matching [`parse_properties`]'s lower-casing. Returns `None` when neither key
/// is present so callers can distinguish an absent field from an empty one.
fn read_utf8_text_field(props: &HashMap<String, String>, key: &str) -> Option<String> {
    if let Some(raw) = props.get(&format!("%utf8%{key}")) {
        return Some(crate::altium::decode_utf8_param_value(raw));
    }
    props.get(key).cloned()
}

/// Parses a rectangle from properties.
#[allow(clippy::unnecessary_wraps)] // infallible (all coords default); Option kept for uniform parser dispatch
fn parse_rectangle(props: &HashMap<String, String>) -> Option<Rectangle> {
    let x1 = crate::altium::schlib::coord::read(props, "location.x");
    let y1 = crate::altium::schlib::coord::read(props, "location.y");
    let x2 = crate::altium::schlib::coord::read(props, "corner.x");
    let y2 = crate::altium::schlib::coord::read(props, "corner.y");

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let line_color = props.get("color").and_then(|s| s.parse().ok()).unwrap_or(0);
    let fill_color = props
        .get("areacolor")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    // Rectangles store their line style in LINESTYLEEXT (Altium omits LINESTYLE).
    let line_style = props
        .get("linestyleext")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let transparent = props.get("transparent").is_some_and(|s| s == "T");

    Some(Rectangle {
        x1,
        y1,
        x2,
        y2,
        line_width,
        line_color,
        fill_color,
        line_style,
        filled: props.get("issolid").is_some_and(|s| s == "T"),
        transparent,
        owner_part_id,
        display_flags: read_display_flags(props),
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Parses a line from properties.
#[allow(clippy::unnecessary_wraps)] // infallible (all coords default); Option kept for uniform parser dispatch
fn parse_line(props: &HashMap<String, String>) -> Option<Line> {
    // Each coordinate may carry a `…_Frac` companion (off-grid endpoints).
    let x1 = crate::altium::schlib::coord::read(props, "location.x");
    let y1 = crate::altium::schlib::coord::read(props, "location.y");
    let x2 = crate::altium::schlib::coord::read(props, "corner.x");
    let y2 = crate::altium::schlib::coord::read(props, "corner.y");

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props.get("color").and_then(|s| s.parse().ok()).unwrap_or(0);
    let line_style = props
        .get("linestyle")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    // Altium omits IsNotAccesible when false (accessible), so absent => false — matching
    // parse_arc and AltiumSharp. A fresh line defaults true (struct), so it still emits =T.
    let is_not_accessible = props.get("isnotaccesible").is_some_and(|s| s == "T");
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

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
        display_flags: read_display_flags(props),
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Parses a parameter from properties.
fn parse_parameter(props: &HashMap<String, String>) -> Option<Parameter> {
    let name = props.get("name")?.clone();
    let value = read_utf8_text_field(props, "text").unwrap_or_default();

    let x = crate::altium::schlib::coord::read(props, "location.x");
    let y = crate::altium::schlib::coord::read(props, "location.y");
    let font_id = props
        .get("fontid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props
        .get("color")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x80_00_00);
    let hidden = props.get("ishidden").is_some_and(|s| s == "T");
    let read_only_state = props
        .get("readonlystate")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let param_type = props
        .get("paramtype")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let orientation = props
        .get("orientation")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let show_name = props.get("showname").is_some_and(|s| s == "T");
    let hide_name = props.get("hidename").is_some_and(|s| s == "T");
    let description = props.get("description").cloned().unwrap_or_default();
    let is_configurable = props.get("isconfigurable").is_some_and(|s| s == "T");
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    Some(Parameter {
        name,
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
        display_flags: read_display_flags(props),
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Parses a polyline from properties.
fn parse_polyline(props: &HashMap<String, String>) -> Option<Polyline> {
    // Polylines have LocationCount and X{n}/Y{n} vertex properties
    let location_count: usize = props
        .get("locationcount")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if location_count < 2 {
        return None;
    }

    let mut points = Vec::with_capacity(location_count);
    for i in 1..=location_count {
        let x_key = format!("x{i}");
        let y_key = format!("y{i}");
        let x = crate::altium::schlib::coord::read(props, &x_key);
        let y = crate::altium::schlib::coord::read(props, &y_key);
        points.push((x, y));
    }

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props.get("color").and_then(|s| s.parse().ok()).unwrap_or(0);
    let line_style = props
        .get("linestyle")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let start_line_shape = props
        .get("startlineshape")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let end_line_shape = props
        .get("endlineshape")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let line_shape_size = props
        .get("lineshapesize")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let transparent = props.get("transparent").is_some_and(|s| s == "T");
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

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
        display_flags: read_display_flags(props),
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Parses a polygon from properties.
fn parse_polygon(props: &HashMap<String, String>) -> Option<Polygon> {
    // Polygons have LocationCount and X{n}/Y{n} properties
    let location_count: usize = props
        .get("locationcount")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if location_count < 3 {
        return None;
    }

    let mut points = Vec::with_capacity(location_count);
    for i in 1..=location_count {
        let x_key = format!("x{i}");
        let y_key = format!("y{i}");
        let x = crate::altium::schlib::coord::read(props, &x_key);
        let y = crate::altium::schlib::coord::read(props, &y_key);
        points.push((x, y));
    }

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let line_color = props.get("color").and_then(|s| s.parse().ok()).unwrap_or(0);
    let fill_color = props
        .get("areacolor")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let line_style = props
        .get("linestyle")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let filled = props.get("issolid").is_some_and(|s| s == "T");
    let transparent = props.get("transparent").is_some_and(|s| s == "T");
    let is_not_accessible = props.get("isnotaccesible").is_some_and(|s| s == "T");
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

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
        display_flags: read_display_flags(props),
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Parses an ellipse from properties.
#[allow(clippy::unnecessary_wraps)] // infallible (all coords default); Option kept for uniform parser dispatch
fn parse_ellipse(props: &HashMap<String, String>) -> Option<Ellipse> {
    let x = crate::altium::schlib::coord::read(props, "location.x");
    let y = crate::altium::schlib::coord::read(props, "location.y");
    let radius_x = crate::altium::schlib::coord::read(props, "radius");
    // Secondary radius, defaults to radius for circles
    let radius_y = if props.contains_key("secondaryradius") {
        crate::altium::schlib::coord::read(props, "secondaryradius")
    } else {
        radius_x
    };

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let line_color = props.get("color").and_then(|s| s.parse().ok()).unwrap_or(0);
    let fill_color = props
        .get("areacolor")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let filled = props.get("issolid").is_some_and(|s| s == "T");
    let transparent = props.get("transparent").is_some_and(|s| s == "T");
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

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
        display_flags: read_display_flags(props),
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Parses an arc from properties.
#[allow(clippy::unnecessary_wraps)] // infallible (all coords default); Option kept for uniform parser dispatch
fn parse_arc(props: &HashMap<String, String>) -> Option<Arc> {
    let x = crate::altium::schlib::coord::read(props, "location.x");
    let y = crate::altium::schlib::coord::read(props, "location.y");
    let radius = crate::altium::schlib::coord::read(props, "radius");

    let start_angle = props
        .get("startangle")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let end_angle = props
        .get("endangle")
        .and_then(|s| s.parse().ok())
        .unwrap_or(360.0);

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props.get("color").and_then(|s| s.parse().ok()).unwrap_or(0);
    let fill_color = props
        .get("areacolor")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    Some(Arc {
        x,
        y,
        radius,
        is_not_accessible: props.get("isnotaccesible").is_some_and(|s| s == "T"),
        start_angle,
        end_angle,
        line_width,
        color,
        fill_color,
        owner_part_id,
        display_flags: read_display_flags(props),
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Parses a Bezier curve from properties.
#[allow(clippy::unnecessary_wraps)] // infallible (all coords default); Option kept for uniform parser dispatch
fn parse_bezier(props: &HashMap<String, String>) -> Option<Bezier> {
    // Bezier curves have 4 control points: X1,Y1 through X4,Y4
    let x1 = crate::altium::schlib::coord::read(props, "x1");
    let y1 = crate::altium::schlib::coord::read(props, "y1");
    let x2 = crate::altium::schlib::coord::read(props, "x2");
    let y2 = crate::altium::schlib::coord::read(props, "y2");
    let x3 = crate::altium::schlib::coord::read(props, "x3");
    let y3 = crate::altium::schlib::coord::read(props, "y3");
    let x4 = crate::altium::schlib::coord::read(props, "x4");
    let y4 = crate::altium::schlib::coord::read(props, "y4");

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props.get("color").and_then(|s| s.parse().ok()).unwrap_or(0);
    // Altium omits IsNotAccesible when false (accessible), so absent => false — matching
    // parse_arc and AltiumSharp. A fresh bezier defaults true (struct), so it still emits =T.
    let is_not_accessible = props.get("isnotaccesible").is_some_and(|s| s == "T");
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

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
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Parses a rounded rectangle from properties.
#[allow(clippy::similar_names)]
#[allow(clippy::unnecessary_wraps)] // infallible (all coords default); Option kept for uniform parser dispatch
fn parse_round_rect(props: &HashMap<String, String>) -> Option<RoundRect> {
    let x1 = crate::altium::schlib::coord::read(props, "location.x");
    let y1 = crate::altium::schlib::coord::read(props, "location.y");
    let x2 = crate::altium::schlib::coord::read(props, "corner.x");
    let y2 = crate::altium::schlib::coord::read(props, "corner.y");

    let corner_x_radius = crate::altium::schlib::coord::read(props, "cornerxradius");
    let corner_y_radius = crate::altium::schlib::coord::read(props, "corneryradius");

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let line_color = props.get("color").and_then(|s| s.parse().ok()).unwrap_or(0);
    let fill_color = props
        .get("areacolor")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let line_style = props
        .get("linestyle")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let filled = props.get("issolid").is_some_and(|s| s == "T");
    let transparent = props.get("transparent").is_some_and(|s| s == "T");
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

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
        display_flags: read_display_flags(props),
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Parses an elliptical arc from properties.
#[allow(clippy::unnecessary_wraps)] // infallible (all coords default); Option kept for uniform parser dispatch
fn parse_elliptical_arc(props: &HashMap<String, String>) -> Option<EllipticalArc> {
    let x = crate::altium::schlib::coord::read(props, "location.x");
    let y = crate::altium::schlib::coord::read(props, "location.y");

    // Primary radius with optional fractional part (`Radius` + `Radius_Frac`).
    let radius = crate::altium::schlib::coord::read(props, "radius");

    // Secondary radius with optional fractional part; defaults to the primary
    // radius when absent (a circular arc).
    let secondary_radius = if props.contains_key("secondaryradius") {
        crate::altium::schlib::coord::read(props, "secondaryradius")
    } else {
        radius
    };

    let start_angle = props
        .get("startangle")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let end_angle = props
        .get("endangle")
        .and_then(|s| s.parse().ok())
        .unwrap_or(360.0);

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props.get("color").and_then(|s| s.parse().ok()).unwrap_or(0);
    let fill_color = props
        .get("areacolor")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

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
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Parses a label from properties.
#[allow(clippy::unnecessary_wraps)] // infallible (all coords default); Option kept for uniform parser dispatch
fn parse_label(props: &HashMap<String, String>) -> Option<Label> {
    let x = crate::altium::schlib::coord::read(props, "location.x");
    let y = crate::altium::schlib::coord::read(props, "location.y");
    let text = read_utf8_text_field(props, "text").unwrap_or_default();

    let font_id = props
        .get("fontid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props.get("color").and_then(|s| s.parse().ok()).unwrap_or(0);
    let rotation = props
        .get("orientation")
        .and_then(|s| s.parse::<i32>().ok())
        .map_or(0.0, |o| f64::from(o) * 90.0);
    let justification = props
        .get("justification")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(TextJustification::BottomLeft, justification_from_id);
    let is_mirrored = props.get("ismirrored").is_some_and(|s| s == "T");
    let is_hidden = props.get("ishidden").is_some_and(|s| s == "T");
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

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
        display_flags: read_display_flags(props),
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Parses a text annotation from properties.
#[allow(clippy::unnecessary_wraps)] // infallible (all coords default); Option kept for uniform parser dispatch
fn parse_text(props: &HashMap<String, String>) -> Option<Text> {
    let x = crate::altium::schlib::coord::read(props, "location.x");
    let y = crate::altium::schlib::coord::read(props, "location.y");
    let text = read_utf8_text_field(props, "text").unwrap_or_default();

    let font_id = props
        .get("fontid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props.get("color").and_then(|s| s.parse().ok()).unwrap_or(0);
    let rotation = props
        .get("orientation")
        .and_then(|s| s.parse::<i32>().ok())
        .map_or(0.0, |o| f64::from(o) * 90.0);
    let justification = props
        .get("justification")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(TextJustification::BottomLeft, justification_from_id);
    let is_mirrored = props.get("ismirrored").is_some_and(|s| s == "T");
    let is_hidden = props.get("ishidden").is_some_and(|s| s == "T");
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

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
        unique_id: props.get("uniqueid").cloned(),
    })
}

/// Converts Altium justification ID to our enum.
const fn justification_from_id(id: u8) -> TextJustification {
    match id {
        1 => TextJustification::BottomCenter,
        2 => TextJustification::BottomRight,
        3 => TextJustification::MiddleLeft,
        4 => TextJustification::MiddleCenter,
        5 => TextJustification::MiddleRight,
        6 => TextJustification::TopLeft,
        7 => TextJustification::TopCenter,
        8 => TextJustification::TopRight,
        // 0 and unknown default to BottomLeft
        _ => TextJustification::BottomLeft,
    }
}

// Bounds-checked little-endian scalar readers are shared (crate::altium::bytes).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partcount_one_decodes_to_zero_no_floor() {
        // Altium stores count+1, so a single-part symbol stores PartCount=1 => internal
        // 0. The old `.max(1)` floor mutated that to 1, which the writer re-emitted as
        // PartCount=2 — corrupting the round-trip. Decode must now yield exactly 0.
        let mut symbol = Symbol::new("PARTCOUNT_TEST");
        parse_text_record_from_string(&mut symbol, "|RECORD=1|LibReference=R1|PartCount=1");
        assert_eq!(
            symbol.part_count, 0,
            "raw PartCount=1 must decode to 0, not be floored to 1"
        );
    }

    #[test]
    fn pin_tail_round_trips_non_default_values() {
        use crate::altium::schlib::primitives::{Pin, PinOrientation};
        use crate::altium::schlib::writer::write_binary_pin;

        let mut pin = Pin::new("VCC", "1", 10, -20, 100, PinOrientation::Right);
        pin.formal_type = 7;
        pin.swap_id_group = "GRP".to_string();
        pin.part_and_sequence = "A|&|B".to_string();
        pin.default_value = "5V".to_string();
        pin.owner_part_display_mode = 3;

        let mut data = Vec::new();
        write_binary_pin(&mut data, &pin).unwrap();

        // Strip the [u24 length LE][u8 flags] frame written by write_record_frame.
        let body = &data[4..];
        let parsed = parse_binary_pin(body).unwrap();

        assert_eq!(parsed.formal_type, 7);
        assert_eq!(parsed.swap_id_group, "GRP");
        assert_eq!(parsed.part_and_sequence, "A|&|B");
        assert_eq!(parsed.default_value, "5V");
        assert_eq!(
            parsed.owner_part_display_mode, 3,
            "OwnerPartDisplayMode byte (offset 7) must round-trip"
        );
    }

    #[test]
    fn pin_owner_part_display_mode_default_byte_is_zero() {
        // Byte-identity: a from-scratch pin must leave the OwnerPartDisplayMode
        // byte at 0x00 (the value every golden pin carries). The record layout is
        // [len:3][flag:1] frame, then payload: i32 rectype(2), u8 unknown,
        // i16 owner_part_id, then this byte at payload offset 7 => data offset 11.
        use crate::altium::schlib::primitives::Pin;
        use crate::altium::schlib::writer::write_binary_pin;
        let pin = Pin::new("VCC", "1", 0, 0, 100, PinOrientation::Right);
        let mut data = Vec::new();
        write_binary_pin(&mut data, &pin).unwrap();
        assert_eq!(
            data[4 + 7],
            0x00,
            "default pin OwnerPartDisplayMode byte must be 0x00 (byte-identical to Altium)"
        );
    }

    #[test]
    fn display_flags_round_trip_through_text_record() {
        // A rectangle carrying all four non-default flags survives a
        // parse of an Altium-style record; absent keys default to false/0.
        let rect = parse_rectangle(&parse_properties(
            "|RECORD=14|Location.X=-5|Location.Y=-5|Corner.X=5|Corner.Y=5|LineWidth=1\
             |GraphicallyLocked=T|Disabled=T|Dimmed=T|OwnerPartDisplayMode=1",
        ))
        .unwrap();
        assert!(rect.display_flags.graphically_locked);
        assert!(rect.display_flags.disabled);
        assert!(rect.display_flags.dimmed);
        assert_eq!(rect.display_flags.owner_part_display_mode, 1);

        // A record omitting them reads all defaults.
        let plain = parse_rectangle(&parse_properties(
            "|RECORD=14|Location.X=-5|Location.Y=-5|Corner.X=5|Corner.Y=5|LineWidth=1",
        ))
        .unwrap();
        assert_eq!(plain.display_flags, ShapeDisplayFlags::default());
    }

    #[test]
    fn display_flags_encode_decode_round_trip() {
        // encode -> decode via the writer/reader keeps the four flags on a shape.
        use crate::altium::schlib::writer;
        let mut label = Label {
            x: 0.0,
            y: 0.0,
            text: "L".to_string(),
            font_id: 1,
            color: 0,
            justification: TextJustification::BottomLeft,
            rotation: 0.0,
            is_mirrored: false,
            is_hidden: false,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags {
                graphically_locked: true,
                disabled: true,
                dimmed: true,
                owner_part_display_mode: 1,
            },
            unique_id: Some("ABCD1234".to_string()),
        };
        let mut symbol = Symbol::new("FLAGS");
        symbol.add_label(label.clone());
        let data = writer::encode_data_stream(&symbol).unwrap();
        let mut round = Symbol::new("FLAGS");
        parse_data_stream(&mut round, &data);
        let parsed = &round.labels[0];
        assert_eq!(parsed.display_flags, label.display_flags);
        // Sanity: also exercise a defaulted label to prove absence reads default.
        label.display_flags = ShapeDisplayFlags::default();
        assert_eq!(label.display_flags, ShapeDisplayFlags::default());
    }

    #[test]
    fn test_parse_properties() {
        let props = parse_properties("|RECORD=14|Location.X=-10|Location.Y=-4|");
        assert_eq!(props.get("record"), Some(&"14".to_string()));
        assert_eq!(props.get("location.x"), Some(&"-10".to_string()));
    }

    #[test]
    fn test_parse_rectangle() {
        let props = parse_properties(
            "|RECORD=14|Location.X=-10|Location.Y=-4|Corner.X=10|Corner.Y=4|LineWidth=1|",
        );
        let rect = parse_rectangle(&props).unwrap();
        assert_eq!(
            (rect.x1, rect.y1, rect.x2, rect.y2),
            (-10.0, -4.0, 10.0, 4.0)
        );
    }

    #[test]
    fn test_parse_shapes_with_omitted_zero_coords_not_dropped() {
        // Altium omits zero-valued coordinates, so a shape sitting on a zero
        // axis arrives with the key missing. The old `?` reader dropped it; we
        // must instead default the missing coordinate to 0.
        let rect = parse_rectangle(&parse_properties(
            "|RECORD=14|Corner.X=10|Corner.Y=4|", // Location.X / Location.Y omitted (== 0)
        ))
        .expect("rectangle with omitted zero Location must not be dropped");
        assert_eq!((rect.x1, rect.y1, rect.x2, rect.y2), (0.0, 0.0, 10.0, 4.0));

        let arc = parse_arc(&parse_properties(
            "|RECORD=12|Radius=20|StartAngle=0|EndAngle=90|", // Location.X / Location.Y omitted
        ))
        .expect("arc with omitted zero Location must not be dropped");
        assert_eq!((arc.x, arc.y, arc.radius), (0.0, 0.0, 20.0));
    }

    #[test]
    fn test_parse_fill_polarity_matches_altium() {
        // Altium emits IsSolid only when filled; absent means unfilled.
        let unfilled = parse_rectangle(&parse_properties(
            "|RECORD=14|Location.X=-1|Location.Y=-1|Corner.X=1|Corner.Y=1|",
        ))
        .unwrap();
        assert!(!unfilled.filled, "absent IsSolid must read as unfilled");

        let filled = parse_rectangle(&parse_properties(
            "|RECORD=14|Location.X=-1|Location.Y=-1|Corner.X=1|Corner.Y=1|IsSolid=T|",
        ))
        .unwrap();
        assert!(filled.filled, "IsSolid=T must read as filled");
    }

    #[test]
    fn test_parameter_uniqueid_preserved() {
        let p = parse_parameter(&parse_properties(
            "|RECORD=41|Name=Value|Text=10k|UniqueID=ABCD1234|",
        ))
        .unwrap();
        assert_eq!(p.unique_id.as_deref(), Some("ABCD1234"));
        assert_eq!(p.name, "Value");
        assert_eq!(p.value, "10k");
    }

    #[test]
    fn test_arc_is_not_accessible_parsed() {
        let tagged = parse_arc(&parse_properties(
            "|RECORD=12|Location.X=5|Location.Y=5|Radius=10|IsNotAccesible=T|",
        ))
        .unwrap();
        assert!(
            tagged.is_not_accessible,
            "IsNotAccesible=T must parse as true"
        );

        let untagged = parse_arc(&parse_properties(
            "|RECORD=12|Location.X=5|Location.Y=5|Radius=10|",
        ))
        .unwrap();
        assert!(
            !untagged.is_not_accessible,
            "absent IsNotAccesible defaults false on read"
        );
    }

    #[test]
    fn test_absent_colour_reads_black() {
        // Altium omits Color/AreaColor when 0; AltiumSharp defaults absent to 0
        // (black). We previously fabricated navy / pale-yellow defaults, so reading
        // an Altium shape that omits these surfaced the wrong colour.
        let arc = parse_arc(&parse_properties(
            "|RECORD=12|Location.X=5|Location.Y=5|Radius=10|",
        ))
        .unwrap();
        assert_eq!(arc.color, 0, "absent arc Color must read as black");

        let poly = parse_polygon(&parse_properties(
            "|RECORD=7|LocationCount=3|X1=0|Y1=0|X2=10|Y2=0|X3=5|Y3=10|",
        ))
        .unwrap();
        assert_eq!(
            poly.line_color, 0,
            "absent polygon Color must read as black"
        );
        assert_eq!(
            poly.fill_color, 0,
            "absent polygon AreaColor must read as 0"
        );

        let label = parse_label(&parse_properties(
            "|RECORD=4|Location.X=5|Location.Y=5|Text=R|",
        ))
        .unwrap();
        assert_eq!(label.color, 0, "absent label Color must read as black");
    }

    #[test]
    fn utf8_text_record_decodes_intact_from_raw_bytes() {
        // Build the on-disk bytes for a Label whose value is Greek Ω (not in
        // Windows-1252): the record is Windows-1252, with the Ω stored as its raw
        // UTF-8 bytes behind `%UTF8%Text`. The reader must recover "10kΩ" exactly.
        // A reader that only read a plain `Text=` (Windows-1252) would corrupt it.
        let value = "10k\u{03A9}";
        let utf8_form = crate::altium::encode_utf8_param_value(value);
        let record = format!(
            "|RECORD=4|Location.X=5|Location.Y=5|FontID=1|%UTF8%Text={utf8_form}|UniqueID=ABCD1234"
        );
        let bytes = crate::altium::encode_windows1252(&record);

        let mut symbol = Symbol::new("L");
        parse_text_record(&mut symbol, &bytes);
        assert_eq!(
            symbol.labels[0].text, value,
            "%UTF8%Text label must decode to the true Unicode value, not ?-mangled"
        );

        // The same convention for a Parameter (RECORD=41).
        let record = format!(
            "|RECORD=41|Location.X=0|Location.Y=0|Color=0|FontID=1|%UTF8%Text={utf8_form}|Name=Value|UniqueID=ABCD1234"
        );
        let bytes = crate::altium::encode_windows1252(&record);
        let mut symbol = Symbol::new("P");
        parse_text_record(&mut symbol, &bytes);
        assert_eq!(symbol.parameters[0].value, value);
        assert_eq!(symbol.parameters[0].name, "Value");
    }

    #[test]
    fn plain_win1252_text_record_reads_unchanged() {
        // A record with no `%UTF8%` marker takes the unchanged decode path and
        // reads the Windows-1252 value verbatim (here `µ`, byte 0xB5).
        let bytes = crate::altium::encode_windows1252(
            "|RECORD=41|Location.X=0|Location.Y=0|Color=0|FontID=1|Text=10\u{00B5}F|Name=Value|UniqueID=ABCD1234",
        );
        let mut symbol = Symbol::new("P");
        parse_text_record(&mut symbol, &bytes);
        assert_eq!(symbol.parameters[0].value, "10\u{00B5}F");
    }
}
