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
    Arc, Bezier, Ellipse, EllipticalArc, FootprintModel, IeeeSymbol, Image, Label, Line, Parameter,
    Pie, Pin, PinElectricalType, PinOrientation, PinSymbol, Polygon, Polyline, Rectangle,
    RoundRect, ShapeDisplayFlags, TextFrame, TextJustification,
};
use super::Symbol;
use crate::altium::bytes::{
    read_i16_le as read_i16, read_i32_le as read_i32, read_u32_le as read_u32,
};
use std::collections::HashMap;

mod parsers;

#[allow(clippy::wildcard_imports)] // tightly-coupled reader split
use parsers::*;

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

/// A text record's segments exactly as stored: every `key=value` in order,
/// an empty segment (the UI's `%UTF8%Key=…|||Key=…` form) as `("", "")`.
fn record_segments(text: &str) -> Vec<(String, String)> {
    text.trim_end_matches('\0')
        .split('|')
        .skip(1)
        .map(|segment| {
            segment.split_once('=').map_or_else(
                || (segment.to_string(), String::new()),
                |(key, value)| (key.to_string(), value.to_string()),
            )
        })
        .collect()
}

/// [`parse_text_record_from_string`] for the writer's tests.
#[cfg(test)]
pub(crate) fn parse_text_record_from_string_for_test(symbol: &mut Symbol, text: &str) {
    parse_text_record_from_string(symbol, text);
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
            if let Some(name) = read_utf8_text_field(&props, "libreference") {
                if !name.is_empty() {
                    symbol.name = name;
                }
            }
            if let Some(desc) = read_utf8_text_field(&props, "componentdescription") {
                symbol.description = desc;
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
            if let Some(source_lib) = read_utf8_text_field(&props, "sourcelibraryname") {
                symbol.source_library_name = source_lib;
            }
            if let Some(target_file) = props.get("targetfilename") {
                symbol.target_file_name.clone_from(target_file);
            }
            if let Some(count) = props.get("allpincount").and_then(|v| v.trim().parse().ok()) {
                symbol.all_pin_count = Some(count);
            }
            // The record exactly as stored — every segment, an empty one
            // included, because the UI writes a `%UTF8%` twin as
            // `%UTF8%Key=…|||Key=…` — so the writer reproduces this header
            // rather than the canonical one.
            symbol.header_params = record_segments(text);
        }
        14 => {
            // Rectangle
            if let Some(mut rect) = parse_rectangle(&props) {
                rect.raw_params = record_segments(text);
                symbol.add_rectangle(rect);
            }
        }
        13 => {
            // Line
            if let Some(mut line) = parse_line(&props) {
                line.raw_params = record_segments(text);
                symbol.add_line(line);
            }
        }
        34 => {
            // Designator (a parameter record; its value uses the same
            // `%UTF8%Text` convention as any other text field). Its position
            // and UniqueID are preserved so a read-modify-write re-emits the
            // record byte-identically (absent coordinate keys decode to 0,
            // which is the authored value — Altium omits zero keys).
            if let Some(text) = read_utf8_text_field(&props, "text") {
                symbol.designator = text;
                symbol.designator_x = crate::altium::schlib::coord::read(&props, "location.x");
                symbol.designator_y = crate::altium::schlib::coord::read(&props, "location.y");
                symbol.designator_unique_id = props.get("uniqueid").cloned();
            }
        }
        41 => {
            // Parameter
            if let Some(mut param) = parse_parameter(&props) {
                param.raw_params = record_segments(text);
                symbol.add_parameter(param);
            }
        }
        45 => {
            // Model (footprint reference)
            if let Some(name) = props.get("modelname") {
                let mut fp = FootprintModel::new(name);
                fp.raw_params = record_segments(text);
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
                fp.unique_id = props.get("uniqueid").cloned();
                symbol.add_footprint(fp);
            }
        }
        6 => {
            // Polyline
            if let Some(mut polyline) = parse_polyline(&props) {
                polyline.raw_params = record_segments(text);
                symbol.add_polyline(polyline);
            }
        }
        8 => {
            // Ellipse
            if let Some(mut ellipse) = parse_ellipse(&props) {
                ellipse.raw_params = record_segments(text);
                symbol.add_ellipse(ellipse);
            }
        }
        9 => {
            // Pie (filled circular sector)
            if let Some(mut pie) = parse_pie(&props) {
                pie.raw_params = record_segments(text);
                symbol.add_pie(pie);
            }
        }
        30 => {
            // Image (embedded/linked picture)
            if let Some(mut image) = parse_image(&props) {
                image.raw_params = record_segments(text);
                symbol.add_image(image);
            }
        }
        28 => {
            // Text frame (bordered multi-line text box)
            if let Some(mut text_frame) = parse_text_frame(&props) {
                text_frame.raw_params = record_segments(text);
                symbol.add_text_frame(text_frame);
            }
        }
        12 => {
            // Arc
            if let Some(mut arc) = parse_arc(&props) {
                arc.raw_params = record_segments(text);
                symbol.add_arc(arc);
            }
        }
        3 => {
            // IEEE symbol
            if let Some(mut ieee_symbol) = parse_ieee_symbol(&props) {
                ieee_symbol.raw_params = record_segments(text);
                symbol.add_ieee_symbol(ieee_symbol);
            }
        }
        4 => {
            // Label
            if let Some(mut label) = parse_label(&props) {
                label.raw_params = record_segments(text);
                symbol.add_label(label);
            }
        }
        5 => {
            // Bezier curve
            if let Some(mut bezier) = parse_bezier(&props) {
                bezier.raw_params = record_segments(text);
                symbol.add_bezier(bezier);
            }
        }
        7 => {
            // Polygon
            if let Some(mut polygon) = parse_polygon(&props) {
                polygon.raw_params = record_segments(text);
                symbol.add_polygon(polygon);
            }
        }
        10 => {
            // Rounded Rectangle
            if let Some(mut round_rect) = parse_round_rect(&props) {
                round_rect.raw_params = record_segments(text);
                symbol.add_round_rect(round_rect);
            }
        }
        11 => {
            // Elliptical Arc
            if let Some(mut elliptical_arc) = parse_elliptical_arc(&props) {
                elliptical_arc.raw_params = record_segments(text);
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

/// Reads a record's text value (see [`field_text`]). `key` is the lower-cased
/// field name (e.g. `"text"`), matching [`parse_properties`]'s lower-casing.
/// Returns `None` when neither key is present so callers can distinguish an
/// absent field from an empty one.
fn read_utf8_text_field(props: &HashMap<String, String>, key: &str) -> Option<String> {
    field_text(
        props.get(key).map(String::as_str),
        props.get(&format!("%utf8%{key}")).map(String::as_str),
    )
}

/// The text a field holds, given its plain value and its `%UTF8%` twin as
/// they sit in the Windows-1252-decoded record.
///
/// Altium writes a value outside ASCII twice: the twin with its UTF-8 bytes,
/// and the plain key in the writing machine's code page — `?` for a character
/// the page cannot hold, byte `0x8E` for an escaped pipe and `0x8E 0x8E` for a
/// literal one. The twin is therefore the value. A script-authored library's
/// twin holds the value's UTF-8 bytes widened through the authoring code page
/// (Windows-1250 for the golden: Altium took the script's bytes for
/// characters), which [`crate::altium::fold_ansi_widened`] undoes. Without a
/// twin the plain key is the value — its raw UTF-8 bytes where they decode as
/// such, else Windows-1252. `None` when the field has neither.
pub(crate) fn field_text(plain: Option<&str>, twin: Option<&str>) -> Option<String> {
    if let Some(twin) = twin {
        let value = crate::altium::decode_utf8_param_value(twin);
        return Some(crate::altium::fold_ansi_widened(&value).unwrap_or(value));
    }
    plain.map(|plain| recover_utf8_bytes(plain).unwrap_or_else(|| plain.to_string()))
}

/// Recovers a value stored as raw UTF-8 bytes inside a Windows-1252 record.
///
/// Returns `None` when `raw` is plain ASCII (nothing to recover) or when its
/// bytes are not valid UTF-8, in which case it is a genuine Windows-1252 value
/// and must be taken verbatim.
fn recover_utf8_bytes(raw: &str) -> Option<String> {
    if raw.is_ascii() {
        return None;
    }
    // Every char came from a Windows-1252 decode, so re-encoding is exact and
    // gives back the bytes as they sit in the record.
    let bytes = crate::altium::encode_windows1252(raw);
    std::str::from_utf8(&bytes).ok().map(str::to_string)
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
    fn every_justification_id_maps_to_its_own_anchor() {
        // The ids are Altium's, and a wrong arm silently re-anchors text on
        // read. Anything outside the table is Altium's own 0 = BottomLeft.
        for (id, expected) in [
            (0, TextJustification::BottomLeft),
            (1, TextJustification::BottomCenter),
            (2, TextJustification::BottomRight),
            (3, TextJustification::MiddleLeft),
            (4, TextJustification::MiddleCenter),
            (5, TextJustification::MiddleRight),
            (6, TextJustification::TopLeft),
            (7, TextJustification::TopCenter),
            (8, TextJustification::TopRight),
            (9, TextJustification::BottomLeft),
            (255, TextJustification::BottomLeft),
        ] {
            assert_eq!(justification_from_id(id), expected, "id {id}");
        }
    }

    #[test]
    fn a_malformed_data_stream_stops_at_the_damage_and_keeps_what_parsed() {
        // Every framing fault a truncated or foreign file can present has to
        // end the walk rather than panic on the slice: too short to hold a
        // header at all, a zero-length end marker, a record claiming more
        // bytes than remain, and a record type this reader does not know.
        let mut symbol = Symbol::new("SHORT");
        parse_data_stream(&mut symbol, &[1, 2, 3]);
        assert!(symbol.pins.is_empty());

        // A zero length word is Altium's end marker; anything after it is not
        // read, so the trailing bytes must not become a second record.
        let mut symbol = Symbol::new("END_MARKER");
        parse_data_stream(&mut symbol, &[0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(symbol.pins.is_empty());

        // Claims a 255-byte record with only a handful of bytes behind it.
        let mut symbol = Symbol::new("OVERRUN");
        parse_data_stream(&mut symbol, &[255, 0, 0, 0, b'x', b'y']);
        assert!(symbol.pins.is_empty());

        // A record type neither text (0) nor binary pin (1) is skipped, and
        // the walk carries on to the record behind it.
        let mut symbol = Symbol::new("UNKNOWN_KIND");
        let mut data = vec![1, 0, 0, 0x7F, b'?'];
        let text = b"|RECORD=1|ComponentDescription=survived";
        data.extend_from_slice(&u32::try_from(text.len()).unwrap().to_le_bytes()[..3]);
        data.push(0);
        data.extend_from_slice(text);
        parse_data_stream(&mut symbol, &data);
        assert_eq!(symbol.description, "survived");
    }

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
    fn display_flags_encode_decode_round_trip() {
        // encode -> decode via the writer/reader keeps the four flags on a shape.
        use crate::altium::schlib::writer;
        let mut label = Label {
            raw_params: Vec::new(),
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

    #[test]
    fn empty_or_unparsable_record_fields_leave_the_symbol_as_it_was() {
        // An empty LibReference is not a name — taking it would blank out the
        // symbol's OLE storage name and make it unaddressable.
        let mut symbol = Symbol::new("KEEP_ME");
        parse_text_record_from_string(&mut symbol, "|RECORD=1|LibReference=");
        assert_eq!(symbol.name, "KEEP_ME");

        // A polyline record carrying no usable geometry is dropped rather than
        // added as a degenerate shape.
        let mut symbol = Symbol::new("NO_POLYLINE");
        parse_text_record_from_string(&mut symbol, "|RECORD=6|LocationCount=0");
        assert!(symbol.polylines.is_empty());
    }

    /// A field's text is its `%UTF8%` twin where there is one — the plain key
    /// holds only the code page's narrowing, `?`s and Altium's `0x8E` escape
    /// — with a script's widened twin folded back; without a twin, the plain
    /// key, its UTF-8 bytes recovered where they decode.
    #[test]
    fn a_field_reads_its_twin_first() {
        let utf8 = |s: &str| crate::altium::encode_utf8_param_value(s);
        // The UI's form: the twin is the value, the plain key a husk.
        assert_eq!(
            field_text(Some("????"), Some(&utf8("\u{9b0}\u{9cb}\u{9a7}\u{995}"))).as_deref(),
            Some("\u{9b0}\u{9cb}\u{9a7}\u{995}")
        );
        // An escaped pipe in the plain key (byte 0x8E, `Ž` once decoded);
        // the twin holds the broken bar.
        assert_eq!(
            field_text(Some("A\u{17d}B"), Some(&utf8("A\u{a6}B"))).as_deref(),
            Some("A\u{a6}B")
        );
        // A script's twin: the value's UTF-8 bytes widened through
        // Windows-1250 (`RĂ©sistance`), folded back to the word.
        let widened = crate::altium::widen_through("R\u{e9}sistance", 1250);
        assert_eq!(
            field_text(Some("x"), Some(&utf8(&widened))).as_deref(),
            Some("R\u{e9}sistance")
        );
        // No twin: the plain key, raw UTF-8 bytes recovered.
        assert_eq!(field_text(Some("ASCII"), None).as_deref(), Some("ASCII"));
        assert_eq!(
            field_text(Some(&utf8("\u{420}\u{435}\u{437}")), None).as_deref(),
            Some("\u{420}\u{435}\u{437}")
        );
        assert_eq!(field_text(None, None), None);
    }
}
