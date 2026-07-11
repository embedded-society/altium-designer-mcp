//! Binary writer for `SchLib` Data streams.
//!
//! This module handles encoding symbol primitives to the binary format
//! used in Altium `SchLib` Data streams.
//!
//! # Data Stream Format
//!
//! ```text
//! [length:3 LE u24][flags:1 u8][data:length]
//! ...
//! ```
//!
//! The 4-byte record header is Altium's single 32-bit little-endian size word:
//! the low 24 bits are the payload length and the high byte is a flag (0x00
//! text record, 0x01 binary pin record). Records run until the stream is
//! exhausted — there is NO end-of-stream marker (a trailing 0x0000 would be
//! mis-read as a zero-length record).
//!
//! Record types:
//! - `0x0000`: Text record (pipe-delimited key=value pairs)
//! - `0x0001`: Binary pin record

use super::coord;
use super::primitives::{
    Arc, Bezier, Ellipse, EllipticalArc, FootprintModel, Image, Label, Line, Parameter, Pie, Pin,
    Polygon, Polyline, Rectangle, RoundRect, ShapeDisplayFlags, Text, TextFrame, TextJustification,
};
use super::Symbol;
use crate::altium::framing::{write_cstring_param_block, write_pascal_string};

/// Writes a record frame to the output: Altium's `[u24 length LE][u8 flags]`
/// header followed by the payload. `flags` is 0 for a text record and 1 for a
/// binary pin record. For payloads under 16 MiB (always, in practice) the third
/// length byte is 0, so this is byte-identical to the older
/// `[u16 length LE][u16 BE type]` framing.
///
/// # Errors
///
/// Returns an error if `payload` exceeds the 24-bit length field (16 MiB),
/// which the on-disk header cannot represent (a `u16` cast would otherwise
/// truncate the length and desync the whole record stream).
fn write_record_frame(
    data: &mut Vec<u8>,
    payload: &[u8],
    flags: u8,
) -> crate::altium::error::AltiumResult<()> {
    use crate::altium::error::AltiumError;

    if payload.len() > 0x00FF_FFFF {
        return Err(AltiumError::InvalidParameter {
            name: "record".to_string(),
            message: format!(
                "Record length {} exceeds the 16 MiB on-disk maximum",
                payload.len()
            ),
        });
    }
    #[allow(clippy::cast_possible_truncation)] // bounded above
    let len = payload.len() as u32;
    data.push((len & 0xFF) as u8);
    data.push(((len >> 8) & 0xFF) as u8);
    data.push(((len >> 16) & 0xFF) as u8);
    data.push(flags);
    data.extend_from_slice(payload);
    Ok(())
}

/// Writes a text record (type 0) to the output.
///
/// # Errors
///
/// Returns an error if the encoded record exceeds the 16 MiB record limit.
fn write_text_record(data: &mut Vec<u8>, content: &str) -> crate::altium::error::AltiumResult<()> {
    let mut record = crate::altium::encode_windows1252(content);
    record.push(0x00); // Null terminator
    write_record_frame(data, &record, 0) // flags 0 = text
}

/// Writes a binary pin record (type 1) to the output.
///
/// # Errors
///
/// Returns an error if:
/// - Pin coordinates (x, y, length) exceed the i16 range (±32767)
/// - Pin name, designator, or description exceeds 255 bytes
#[allow(clippy::too_many_lines)] // Complex binary format requires detailed validation and encoding
pub(crate) fn write_binary_pin(
    data: &mut Vec<u8>,
    pin: &Pin,
) -> crate::altium::error::AltiumResult<()> {
    use crate::altium::error::AltiumError;

    // Validation constants
    const I16_MIN: i32 = i16::MIN as i32;
    const I16_MAX: i32 = i16::MAX as i32;
    const MAX_STRING_LEN: usize = 255;

    // Validate that coordinates fit in i16 range

    if pin.x < I16_MIN || pin.x > I16_MAX {
        return Err(AltiumError::InvalidParameter {
            name: "pin.x".to_string(),
            message: format!(
                "Pin '{}' x coordinate {} exceeds i16 range (±32767)",
                pin.designator, pin.x
            ),
        });
    }
    if pin.y < I16_MIN || pin.y > I16_MAX {
        return Err(AltiumError::InvalidParameter {
            name: "pin.y".to_string(),
            message: format!(
                "Pin '{}' y coordinate {} exceeds i16 range (±32767)",
                pin.designator, pin.y
            ),
        });
    }
    if pin.length < I16_MIN || pin.length > I16_MAX {
        return Err(AltiumError::InvalidParameter {
            name: "pin.length".to_string(),
            message: format!(
                "Pin '{}' length {} exceeds i16 range (±32767)",
                pin.designator, pin.length
            ),
        });
    }

    // Strings are stored as Windows-1252 Pascal short strings; validate the
    // ENCODED byte length (what the u8 length prefix actually holds), not the
    // UTF-8 String length — otherwise non-ASCII text is wrongly rejected even
    // though it fits in 255 encoded bytes.
    let name = crate::altium::encode_windows1252(&pin.name);
    let designator = crate::altium::encode_windows1252(&pin.designator);
    let description = crate::altium::encode_windows1252(&pin.description);
    let swap_id_group = crate::altium::encode_windows1252(&pin.swap_id_group);
    let part_and_sequence = crate::altium::encode_windows1252(&pin.part_and_sequence);
    let default_value = crate::altium::encode_windows1252(&pin.default_value);
    for (bytes, field) in [
        (&name, "name"),
        (&designator, "designator"),
        (&description, "description"),
        (&swap_id_group, "swap_id_group"),
        (&part_and_sequence, "part_and_sequence"),
        (&default_value, "default_value"),
    ] {
        if bytes.len() > MAX_STRING_LEN {
            return Err(AltiumError::InvalidParameter {
                name: format!("pin.{field}"),
                message: format!(
                    "Pin '{}' {field} length {} exceeds maximum of {MAX_STRING_LEN} bytes",
                    pin.designator,
                    bytes.len(),
                ),
            });
        }
    }

    let mut record = Vec::with_capacity(64);

    // Record type (4 bytes) - always 2 for pin
    record.extend_from_slice(&2i32.to_le_bytes());

    // Unknown byte
    record.push(0x00);

    // Owner part ID (2 bytes)
    if pin.owner_part_id < I16_MIN || pin.owner_part_id > I16_MAX {
        return Err(AltiumError::InvalidParameter {
            name: "pin.owner_part_id".to_string(),
            message: format!(
                "Pin '{}' owner_part_id {} exceeds i16 range (±32767)",
                pin.designator, pin.owner_part_id
            ),
        });
    }
    #[allow(clippy::cast_possible_truncation)]
    let owner_part = pin.owner_part_id as i16;
    record.extend_from_slice(&owner_part.to_le_bytes());

    // Owner part display mode (1 byte). Round-tripped from the pin; a from-scratch
    // pin defaults to 0, matching Altium's output byte-for-byte.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    record.push(pin.owner_part_display_mode as u8);

    // Symbol flags (4 bytes: inner_edge, outer_edge, inside, outside)
    record.push(pin.symbol_inner_edge.to_id());
    record.push(pin.symbol_outer_edge.to_id());
    record.push(pin.symbol_inside.to_id());
    record.push(pin.symbol_outside.to_id());

    // Description: Pascal short string [length:1][string]
    write_pascal_string(&mut record, &description);

    // Formal type (1 byte) - 0x01 for a normal pin; round-tripped from the pin.
    record.push(pin.formal_type);

    // Electrical type (1 byte)
    record.push(pin.electrical_type.to_id());

    // Flags (1 byte)
    let (rotated, flipped) = pin.orientation.to_flags();
    let mut flags: u8 = 0;
    if rotated {
        flags |= 0x01;
    }
    if flipped {
        flags |= 0x02;
    }
    if pin.hidden {
        flags |= 0x04;
    }
    if pin.show_name {
        flags |= 0x08;
    }
    if pin.show_designator {
        flags |= 0x10;
    }
    if pin.graphically_locked {
        flags |= 0x40;
    }
    if pin.is_not_accessible {
        flags |= 0x20;
    }
    record.push(flags);

    // Length (2 bytes)
    #[allow(clippy::cast_possible_truncation)]
    let length = pin.length as i16;
    record.extend_from_slice(&length.to_le_bytes());

    // Location X, Y (2 bytes each, signed)
    #[allow(clippy::cast_possible_truncation)]
    let x = pin.x as i16;
    #[allow(clippy::cast_possible_truncation)]
    let y = pin.y as i16;
    record.extend_from_slice(&x.to_le_bytes());
    record.extend_from_slice(&y.to_le_bytes());

    // Colour (4 bytes)
    record.extend_from_slice(&pin.colour.to_le_bytes());

    // Name: [length:1][string]
    write_pascal_string(&mut record, &name);

    // Designator: [length:1][string]
    write_pascal_string(&mut record, &designator);

    // Pin swap-id tail (Pascal short strings), round-tripped from the pin. For a
    // from-scratch pin the defaults (`""`, `"|&|"`, `""`) reproduce Altium's
    // output byte-for-byte.
    write_pascal_string(&mut record, &swap_id_group);
    write_pascal_string(&mut record, &part_and_sequence);
    write_pascal_string(&mut record, &default_value);

    // Header: Altium's [u24 length LE][u8 flags=1 for pin], then the record.
    write_record_frame(data, &record, 1)
}

/// Encodes a component header record.
fn encode_component_header(symbol: &Symbol) -> String {
    let part_id_locked = if symbol.part_id_locked { "T" } else { "F" };
    let parts = vec![
        "RECORD=1".to_string(),
        format!("LibReference={}", symbol.name),
        format!("ComponentDescription={}", symbol.description),
        format!("PartCount={}", symbol.part_count + 1), // Altium uses part_count + 1
        format!("DisplayModeCount={}", symbol.display_mode_count),
        "IndexInSheet=-1".to_string(),
        "OwnerPartId=-1".to_string(),
        format!("CurrentPartId={}", symbol.current_part_id),
        "LibraryPath=*".to_string(),
        format!("SourceLibraryName={}", symbol.source_library_name),
        "SheetPartFileName=*".to_string(),
        format!("TargetFileName={}", symbol.target_file_name),
        format!("AllPinCount={}", symbol.pins.len()),
        "AreaColor=11599871".to_string(), // Light yellow fill
        "Color=128".to_string(),          // Dark red outline
        format!("PartIDLocked={part_id_locked}"),
    ];

    // Leading pipe, NO trailing pipe (matches Altium's ParametersToString).
    format!("|{}", parts.join("|"))
}

/// Formats a text field as `<key>=<value>`, promoting it to `%UTF8%<key>` when
/// the value carries characters Windows-1252 cannot represent.
///
/// A pure-Windows-1252 value emits the plain `<key>=<value>` — byte-identical to
/// the pre-UTF-8 output, so the common case (and everything in the golden library)
/// is unchanged. A value with non-Windows-1252 characters (Cyrillic, CJK, Greek
/// `Ω`, …) would otherwise be silently corrupted to `?` by the record's
/// Windows-1252 encoder; instead it is emitted as `%UTF8%<key>=<utf8-bytes>` so
/// the true value survives, matching Altium / `AltiumSharp`. Only one of the two
/// keys is ever written (never both), mirroring `AltiumSharp`'s writer.
fn text_field(key: &str, value: &str) -> String {
    if crate::altium::requires_utf8(value) {
        format!(
            "%UTF8%{key}={}",
            crate::altium::encode_utf8_param_value(value)
        )
    } else {
        format!("{key}={value}")
    }
}

/// Returns `"|Key=value"` when `value` is non-zero, or an empty string when it
/// is zero. Altium omits zero-valued integer parameters such as `Color` and
/// `AreaColor` from a record's text (its `AddNonZero` helper); our reader
/// defaults the absent key back to 0, so this round-trips.
fn nonzero(key: &str, value: u32) -> String {
    if value == 0 {
        String::new()
    } else {
        format!("|{key}={value}")
    }
}

/// Emits an Altium coordinate parameter: `|<key>=<int>` followed by
/// `|<key>_Frac=<frac>` when the (signed) fractional part is non-zero (omitted
/// otherwise, so integer-grid coordinates stay byte-identical to
/// pre-fractional output). Matching AD24, a zero integer part is omitted when
/// the fraction is non-zero (the FRACSHAPES golden arc stores centre 0.05 as
/// `Location.X_Frac=5000` with no `Location.X` key); an on-grid zero still
/// emits `|<key>=0` as before. See [`super::coord`] for the toward-zero /
/// signed-fraction split.
fn coord_param(key: &str, value: f64) -> String {
    let (int, frac) = coord::split(value);
    if frac == 0 {
        format!("|{key}={int}")
    } else if int == 0 {
        format!("|{key}_Frac={frac}")
    } else {
        format!("|{key}={int}|{key}_Frac={frac}")
    }
}

/// Pushes a numbered polyline/polygon vertex (`X{n}`/`Y{n}`) into a `parts`
/// vector that is later joined by `|`, emitting `X{n}_Frac`/`Y{n}_Frac` only when
/// the (signed) fractional part is non-zero. Mirrors [`coord_param`] for the
/// list-style records that build their text from `parts.join("|")`, including
/// the omit-zero-integer-when-fractional rule.
fn push_point(parts: &mut Vec<String>, n: usize, x: f64, y: f64) {
    push_coord(parts, &format!("X{n}"), x);
    push_coord(parts, &format!("Y{n}"), y);
}

/// Pushes a named coordinate (`KEY=int` + optional `KEY_Frac=frac`) into a
/// `parts` vector joined by `|`. The named equivalent of [`push_point`] /
/// [`coord_param`] for list-style records, following the same AD24 rules: the
/// `_Frac` companion is omitted when zero, and a zero integer part is omitted
/// when the fraction is non-zero.
fn push_coord(parts: &mut Vec<String>, key: &str, value: f64) {
    let (int, frac) = coord::split(value);
    if int != 0 || frac == 0 {
        parts.push(format!("{key}={int}"));
    }
    if frac != 0 {
        parts.push(format!("{key}_Frac={frac}"));
    }
}

/// Emits the four universal display/lock flags as `|KEY=VALUE` tokens, each
/// only when non-default. Matching Altium's omit-when-default behaviour, a shape
/// carrying only defaults emits nothing here (so its record stays byte-identical
/// to pre-flag output). Bool flags emit `=T` when set; `OwnerPartDisplayMode`
/// emits its integer when non-zero.
fn write_display_flags(flags: ShapeDisplayFlags) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    if flags.graphically_locked {
        out.push_str("|GraphicallyLocked=T");
    }
    if flags.disabled {
        out.push_str("|Disabled=T");
    }
    if flags.dimmed {
        out.push_str("|Dimmed=T");
    }
    if flags.owner_part_display_mode != 0 {
        let _ = write!(
            out,
            "|OwnerPartDisplayMode={}",
            flags.owner_part_display_mode
        );
    }
    out
}

/// Pushes the universal display/lock flags into a `parts` vector that is later
/// joined by `|` (the list-style encoders: parameter, polyline, polygon). Each
/// key is pushed only when non-default, mirroring [`write_display_flags`].
fn push_display_flags(parts: &mut Vec<String>, flags: ShapeDisplayFlags) {
    if flags.graphically_locked {
        parts.push("GraphicallyLocked=T".to_string());
    }
    if flags.disabled {
        parts.push("Disabled=T".to_string());
    }
    if flags.dimmed {
        parts.push("Dimmed=T".to_string());
    }
    if flags.owner_part_display_mode != 0 {
        parts.push(format!(
            "OwnerPartDisplayMode={}",
            flags.owner_part_display_mode
        ));
    }
}

/// Encodes a rectangle record.
fn encode_rectangle(rect: &Rectangle, index: usize) -> String {
    let transparent = if rect.transparent { "T" } else { "F" };
    // Altium emits IsSolid only when the shape is filled, and omits it otherwise.
    let is_solid = if rect.filled { "|IsSolid=T" } else { "" };
    // Rectangles store the line style in LineStyleExt (Altium omits LineStyle),
    // and omit it when zero.
    let line_style = nonzero("LineStyleExt", u32::from(rect.line_style));
    format!(
        "|RECORD=14|IndexInSheet={}|OwnerPartId={}|IsNotAccesible=T\
         {}{}{}{}\
         |LineWidth={}{}{}{}{}|Transparent={}{}|UniqueID={}",
        index,
        rect.owner_part_id,
        coord_param("Location.X", rect.x1),
        coord_param("Location.Y", rect.y1),
        coord_param("Corner.X", rect.x2),
        coord_param("Corner.Y", rect.y2),
        rect.line_width,
        nonzero("Color", rect.line_color),
        nonzero("AreaColor", rect.fill_color),
        line_style,
        is_solid,
        transparent,
        write_display_flags(rect.display_flags),
        rect.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes a line record.
fn encode_line(line: &Line, index: usize) -> String {
    // Altium tags lines IsNotAccesible (its own single-'s' spelling); emit only when set.
    let not_accessible = if line.is_not_accessible {
        "|IsNotAccesible=T"
    } else {
        ""
    };
    let line_style = nonzero("LineStyle", u32::from(line.line_style));
    format!(
        "|RECORD=13|IndexInSheet={}|OwnerPartId={}{}{}{}{}{}|LineWidth={}{}{}{}|UniqueID={}",
        index,
        line.owner_part_id,
        not_accessible,
        coord_param("Location.X", line.x1),
        coord_param("Location.Y", line.y1),
        coord_param("Corner.X", line.x2),
        coord_param("Corner.Y", line.y2),
        line.line_width,
        nonzero("Color", line.color),
        line_style,
        write_display_flags(line.display_flags),
        line.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes a parameter record.
///
/// Follows Altium's conventions: `IsHidden` is emitted only when hidden (never
/// `=F`), `ReadOnlyState` / `ParamType` / `Orientation` only when non-zero,
/// `ShowName` / `HideName` / `IsConfigurable` only when set, `Text` /
/// `Description` only when non-empty, and the read `UniqueID` is preserved.
#[allow(clippy::similar_names)] // px/py are the obvious names for the x/y integer parts
fn encode_parameter(param: &Parameter, index: usize) -> String {
    let (px, px_frac) = coord::split(param.x);
    let (py, py_frac) = coord::split(param.y);
    let mut parts = vec![
        "RECORD=41".to_string(),
        format!("IndexInSheet={index}"),
        format!("OwnerPartId={}", param.owner_part_id),
        format!("Location.X={px}"),
        format!("Location.Y={py}"),
        format!("Color={}", param.color),
        format!("FontID={}", param.font_id),
    ];
    // Fractional companions (omitted when zero, so integer positions are unchanged).
    if px_frac != 0 {
        parts.push(format!("Location.X_Frac={px_frac}"));
    }
    if py_frac != 0 {
        parts.push(format!("Location.Y_Frac={py_frac}"));
    }
    if param.hidden {
        parts.push("IsHidden=T".to_string());
    }
    if param.read_only_state != 0 {
        parts.push(format!("ReadOnlyState={}", param.read_only_state));
    }
    if param.param_type != 0 {
        parts.push(format!("ParamType={}", param.param_type));
    }
    // EE-meaningful display fields, each omit-when-default so a from-scratch
    // parameter stays byte-identical to Altium (the golden's parameters carry
    // none of these keys).
    if param.orientation != 0 {
        parts.push(format!("Orientation={}", param.orientation));
    }
    if param.show_name {
        parts.push("ShowName=T".to_string());
    }
    if param.hide_name {
        parts.push("HideName=T".to_string());
    }
    if param.is_configurable {
        parts.push("IsConfigurable=T".to_string());
    }
    if !param.value.is_empty() {
        parts.push(text_field("Text", &param.value));
    }
    if !param.description.is_empty() {
        parts.push(format!("Description={}", param.description));
    }
    parts.push(format!("Name={}", param.name));
    push_display_flags(&mut parts, param.display_flags);
    parts.push(format!(
        "UniqueID={}",
        param.unique_id.clone().unwrap_or_else(generate_unique_id)
    ));
    format!("|{}", parts.join("|"))
}

/// Encodes a designator record.
fn encode_designator(designator: &str) -> String {
    format!(
        "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.Y=-6|Color=8388608|FontID=1|{}|Name=Designator|ReadOnlyState=1|UniqueID={}",
        text_field("Text", designator),
        generate_unique_id()
    )
}

/// Encodes a polyline record.
fn encode_polyline(polyline: &Polyline, index: usize) -> String {
    let mut parts = vec![
        "RECORD=6".to_string(),
        format!("IndexInSheet={index}"),
        format!("OwnerPartId={}", polyline.owner_part_id),
        format!("LineWidth={}", polyline.line_width),
    ];
    if polyline.color != 0 {
        parts.push(format!("Color={}", polyline.color));
    }
    parts.extend([
        format!("LineStyle={}", polyline.line_style),
        format!("StartLineShape={}", polyline.start_line_shape),
        format!("EndLineShape={}", polyline.end_line_shape),
        format!("LineShapeSize={}", polyline.line_shape_size),
        format!("LocationCount={}", polyline.points.len()),
    ]);

    for (i, (x, y)) in polyline.points.iter().enumerate() {
        push_point(&mut parts, i + 1, *x, *y);
    }

    // Altium emits Transparent only when true; absent means opaque.
    if polyline.transparent {
        parts.push("Transparent=T".to_string());
    }

    push_display_flags(&mut parts, polyline.display_flags);

    parts.push(format!(
        "UniqueID={}",
        polyline
            .unique_id
            .clone()
            .unwrap_or_else(generate_unique_id)
    ));

    format!("|{}", parts.join("|"))
}

/// Encodes a polygon record.
fn encode_polygon(polygon: &Polygon, index: usize) -> String {
    let mut parts = vec![
        "RECORD=7".to_string(),
        format!("IndexInSheet={index}"),
        format!("OwnerPartId={}", polygon.owner_part_id),
    ];
    // Altium tags polygons IsNotAccesible (its own single-'s' spelling); emit
    // only when set, so a `false` polygon omits the key and round-trips as false.
    if polygon.is_not_accessible {
        parts.push("IsNotAccesible=T".to_string());
    }
    parts.push(format!("LineWidth={}", polygon.line_width));
    // Altium omits Color / AreaColor when zero (AddNonZero).
    if polygon.line_color != 0 {
        parts.push(format!("Color={}", polygon.line_color));
    }
    if polygon.fill_color != 0 {
        parts.push(format!("AreaColor={}", polygon.fill_color));
    }
    // Altium omits LineStyle when zero (Solid).
    if polygon.line_style != 0 {
        parts.push(format!("LineStyle={}", polygon.line_style));
    }
    // Altium emits IsSolid only when filled, and omits it otherwise.
    if polygon.filled {
        parts.push("IsSolid=T".to_string());
    }
    parts.push(format!("LocationCount={}", polygon.points.len()));

    for (i, (x, y)) in polygon.points.iter().enumerate() {
        push_point(&mut parts, i + 1, *x, *y);
    }

    // Altium emits Transparent only when true; absent means opaque.
    if polygon.transparent {
        parts.push("Transparent=T".to_string());
    }

    push_display_flags(&mut parts, polygon.display_flags);

    parts.push(format!(
        "UniqueID={}",
        polygon.unique_id.clone().unwrap_or_else(generate_unique_id)
    ));

    format!("|{}", parts.join("|"))
}

/// Encodes an arc record.
fn encode_arc(arc: &Arc, index: usize) -> String {
    // Altium tags arcs IsNotAccesible (its own single-'s' spelling); emit only when set.
    let not_accessible = if arc.is_not_accessible {
        "|IsNotAccesible=T"
    } else {
        ""
    };
    format!(
        "|RECORD=12|IndexInSheet={}|OwnerPartId={}{}{}{}{}|StartAngle={}|EndAngle={}|LineWidth={}{}{}{}|UniqueID={}",
        index,
        arc.owner_part_id,
        not_accessible,
        coord_param("Location.X", arc.x),
        coord_param("Location.Y", arc.y),
        coord_param("Radius", arc.radius),
        arc.start_angle,
        arc.end_angle,
        arc.line_width,
        nonzero("Color", arc.color),
        nonzero("AreaColor", arc.fill_color),
        write_display_flags(arc.display_flags),
        arc.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes a Bezier curve record.
fn encode_bezier(bezier: &Bezier, index: usize) -> String {
    // Altium tags Beziers IsNotAccesible (its own single-'s' spelling); emit only when set.
    let not_accessible = if bezier.is_not_accessible {
        "|IsNotAccesible=T"
    } else {
        ""
    };
    format!(
        "|RECORD=5|IndexInSheet={}|OwnerPartId={}{}|LineWidth={}{}|LocationCount=4{}{}{}{}{}{}{}{}|UniqueID={}",
        index,
        bezier.owner_part_id,
        not_accessible,
        bezier.line_width,
        nonzero("Color", bezier.color),
        coord_param("X1", bezier.x1),
        coord_param("Y1", bezier.y1),
        coord_param("X2", bezier.x2),
        coord_param("Y2", bezier.y2),
        coord_param("X3", bezier.x3),
        coord_param("Y3", bezier.y3),
        coord_param("X4", bezier.x4),
        coord_param("Y4", bezier.y4),
        bezier.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes an ellipse record.
/// Encodes a pie (filled circular sector) record (`RECORD=9`).
fn encode_pie(pie: &Pie, index: usize) -> String {
    // Altium tags shapes IsNotAccesible (its own single-'s' spelling); emit only when set.
    let not_accessible = if pie.is_not_accessible {
        "|IsNotAccesible=T"
    } else {
        ""
    };
    // Altium emits IsSolid only when filled, Transparent only when true.
    let is_solid = if pie.filled { "|IsSolid=T" } else { "" };
    let transparent = if pie.transparent {
        "|Transparent=T"
    } else {
        ""
    };
    format!(
        "|RECORD=9|IndexInSheet={}|OwnerPartId={}{}{}{}{}|StartAngle={}|EndAngle={}|LineWidth={}{}{}{}{}{}|UniqueID={}",
        index,
        pie.owner_part_id,
        not_accessible,
        coord_param("Location.X", pie.x),
        coord_param("Location.Y", pie.y),
        coord_param("Radius", pie.radius),
        pie.start_angle,
        pie.end_angle,
        pie.line_width,
        nonzero("Color", pie.line_color),
        nonzero("AreaColor", pie.fill_color),
        is_solid,
        transparent,
        write_display_flags(pie.display_flags),
        pie.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes an image record (`RECORD=30`) — the picture metadata (bounding box,
/// border, fill, filename, flags). Embedded image bytes live in `/Storage` and
/// are not written here.
fn encode_image(image: &Image, index: usize) -> String {
    let mut parts = vec![
        "RECORD=30".to_string(),
        format!("IndexInSheet={index}"),
        format!("OwnerPartId={}", image.owner_part_id),
    ];
    if image.is_not_accessible {
        parts.push("IsNotAccesible=T".to_string());
    }
    // Bounding box: Location (corner 1) + Corner (corner 2), each with optional _Frac.
    push_coord(&mut parts, "Location.X", image.x1);
    push_coord(&mut parts, "Location.Y", image.y1);
    push_coord(&mut parts, "Corner.X", image.x2);
    push_coord(&mut parts, "Corner.Y", image.y2);
    parts.push(format!("LineWidth={}", image.line_width));
    if image.line_color != 0 {
        parts.push(format!("Color={}", image.line_color));
    }
    if image.line_style != 0 {
        parts.push(format!("LineStyle={}", image.line_style));
    }
    if image.fill_color != 0 {
        parts.push(format!("AreaColor={}", image.fill_color));
    }
    if image.filled {
        parts.push("IsSolid=T".to_string());
    }
    if image.transparent {
        parts.push("Transparent=T".to_string());
    }
    if image.show_border {
        parts.push("ShowBorder=T".to_string());
    }
    if image.keep_aspect {
        parts.push("KeepAspect=T".to_string());
    }
    if image.embed_image {
        parts.push("EmbedImage=T".to_string());
    }
    if !image.file_name.is_empty() {
        parts.push(format!("FileName={}", image.file_name));
    }
    push_display_flags(&mut parts, image.display_flags);
    parts.push(format!(
        "UniqueID={}",
        image.unique_id.clone().unwrap_or_else(generate_unique_id)
    ));
    format!("|{}", parts.join("|"))
}

/// Encodes a text frame record (`RECORD=28`) — a bordered multi-line text box.
///
/// Token order and omit-when-default behaviour match Altium's own output (both
/// the regenerated golden and `AltiumSharp`'s from-scratch record): no
/// `IndexInSheet` is written (Altium emits none for this record), `LineWidth` /
/// `LineStyle` / `Color` / `AreaColor` / `TextColor` / `FontID` / `Alignment` /
/// `Orientation` only when non-zero, the `T`-flags only when true, and
/// `TextMargin` as a coordinate whose zero integer part is omitted (a default
/// frame carries only `TextMargin_Frac=5`).
fn encode_text_frame(frame: &TextFrame) -> String {
    let mut parts = vec!["RECORD=28".to_string()];
    if frame.is_not_accessible {
        parts.push("IsNotAccesible=T".to_string());
    }
    parts.push(format!("OwnerPartId={}", frame.owner_part_id));
    // Frame box: Location (corner 1) + Corner (corner 2), each with optional _Frac.
    push_coord(&mut parts, "Location.X", frame.x1);
    push_coord(&mut parts, "Location.Y", frame.y1);
    push_coord(&mut parts, "Corner.X", frame.x2);
    push_coord(&mut parts, "Corner.Y", frame.y2);
    if frame.line_width != 0 {
        parts.push(format!("LineWidth={}", frame.line_width));
    }
    if frame.line_style != 0 {
        parts.push(format!("LineStyle={}", frame.line_style));
    }
    if frame.color != 0 {
        parts.push(format!("Color={}", frame.color));
    }
    if frame.area_color != 0 {
        parts.push(format!("AreaColor={}", frame.area_color));
    }
    if frame.text_color != 0 {
        parts.push(format!("TextColor={}", frame.text_color));
    }
    if frame.font_id != 0 {
        parts.push(format!("FontID={}", frame.font_id));
    }
    if frame.is_solid {
        parts.push("IsSolid=T".to_string());
    }
    if frame.show_border {
        parts.push("ShowBorder=T".to_string());
    }
    if frame.alignment != 0 {
        parts.push(format!("Alignment={}", frame.alignment));
    }
    if frame.word_wrap {
        parts.push("WordWrap=T".to_string());
    }
    if frame.clip_to_rect {
        parts.push("ClipToRect=T".to_string());
    }
    if frame.transparent {
        parts.push("Transparent=T".to_string());
    }
    // Text is always written (with %UTF8% promotion, like Label/Text).
    parts.push(text_field("Text", &frame.text));
    if frame.orientation != 0 {
        parts.push(format!("Orientation={}", frame.orientation));
    }
    // TextMargin is a coordinate whose zero integer part Altium omits entirely
    // (unlike Location/Corner, which always carry the integer key).
    let (margin_int, margin_frac) = coord::split(frame.text_margin);
    if margin_int != 0 {
        parts.push(format!("TextMargin={margin_int}"));
    }
    if margin_frac != 0 {
        parts.push(format!("TextMargin_Frac={margin_frac}"));
    }
    push_display_flags(&mut parts, frame.display_flags);
    parts.push(format!(
        "UniqueID={}",
        frame.unique_id.clone().unwrap_or_else(generate_unique_id)
    ));
    format!("|{}", parts.join("|"))
}

fn encode_ellipse(ellipse: &Ellipse, index: usize) -> String {
    // Altium emits IsSolid only when filled, and omits it otherwise.
    let is_solid = if ellipse.filled { "|IsSolid=T" } else { "" };
    // Altium emits Transparent only when true; absent means opaque.
    let transparent = if ellipse.transparent {
        "|Transparent=T"
    } else {
        ""
    };
    format!(
        "|RECORD=8|IndexInSheet={}|OwnerPartId={}{}{}{}{}|LineWidth={}{}{}{}{}{}|UniqueID={}",
        index,
        ellipse.owner_part_id,
        coord_param("Location.X", ellipse.x),
        coord_param("Location.Y", ellipse.y),
        coord_param("Radius", ellipse.radius_x),
        coord_param("SecondaryRadius", ellipse.radius_y),
        ellipse.line_width,
        nonzero("Color", ellipse.line_color),
        nonzero("AreaColor", ellipse.fill_color),
        is_solid,
        transparent,
        write_display_flags(ellipse.display_flags),
        ellipse.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes a rounded rectangle record.
fn encode_round_rect(round_rect: &RoundRect, index: usize) -> String {
    // Altium emits IsSolid only when filled, and omits it otherwise.
    let is_solid = if round_rect.filled { "|IsSolid=T" } else { "" };
    let line_style = nonzero("LineStyle", u32::from(round_rect.line_style));
    // Altium emits Transparent only when true; absent means opaque.
    let transparent = if round_rect.transparent {
        "|Transparent=T"
    } else {
        ""
    };
    format!(
        "|RECORD=10|IndexInSheet={}|OwnerPartId={}|IsNotAccesible=T\
         {}{}{}{}\
         {}{}\
         |LineWidth={}{}{}{}{}{}{}|UniqueID={}",
        index,
        round_rect.owner_part_id,
        coord_param("Location.X", round_rect.x1),
        coord_param("Location.Y", round_rect.y1),
        coord_param("Corner.X", round_rect.x2),
        coord_param("Corner.Y", round_rect.y2),
        coord_param("CornerXRadius", round_rect.corner_x_radius),
        coord_param("CornerYRadius", round_rect.corner_y_radius),
        round_rect.line_width,
        nonzero("Color", round_rect.line_color),
        nonzero("AreaColor", round_rect.fill_color),
        line_style,
        is_solid,
        transparent,
        write_display_flags(round_rect.display_flags),
        round_rect
            .unique_id
            .clone()
            .unwrap_or_else(generate_unique_id)
    )
}

/// Encodes an elliptical arc record.
fn encode_elliptical_arc(arc: &EllipticalArc, index: usize) -> String {
    // Each radius splits into an integer part plus a signed `_Frac` companion
    // (scaled by 100,000), carrying near-boundary values into the integer part.
    // See [`super::coord`] for the shared encoding.
    format!(
        "|RECORD=11|IndexInSheet={}|OwnerPartId={}|IsNotAccesible=T\
         {}{}\
         {}\
         {}\
         |StartAngle={}|EndAngle={}\
         |LineWidth={}{}{}|UniqueID={}",
        index,
        arc.owner_part_id,
        coord_param("Location.X", arc.x),
        coord_param("Location.Y", arc.y),
        coord_param("Radius", arc.radius),
        coord_param("SecondaryRadius", arc.secondary_radius),
        arc.start_angle,
        arc.end_angle,
        arc.line_width,
        nonzero("Color", arc.color),
        nonzero("AreaColor", arc.fill_color),
        arc.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes a label record.
fn encode_label(label: &Label, index: usize) -> String {
    #[allow(clippy::cast_possible_truncation)]
    let orientation = (label.rotation / 90.0).round() as i32 % 4;
    let justification = justification_to_id(label.justification);
    // Altium emits IsMirrored / IsHidden only when true — never `=F`.
    let is_mirrored = if label.is_mirrored {
        "|IsMirrored=T"
    } else {
        ""
    };
    let is_hidden = if label.is_hidden { "|IsHidden=T" } else { "" };
    format!(
        "|RECORD=4|IndexInSheet={}|OwnerPartId={}|IsNotAccesible=T{}{}{}|FontID={}|Orientation={}|Justification={}{}{}{}|{}|UniqueID={}",
        index,
        label.owner_part_id,
        coord_param("Location.X", label.x),
        coord_param("Location.Y", label.y),
        nonzero("Color", label.color),
        label.font_id,
        orientation,
        justification,
        is_mirrored,
        is_hidden,
        write_display_flags(label.display_flags),
        text_field("Text", &label.text),
        label.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes a text annotation record.
fn encode_text(text: &Text, index: usize) -> String {
    #[allow(clippy::cast_possible_truncation)]
    let orientation = (text.rotation / 90.0).round() as i32 % 4;
    let justification = justification_to_id(text.justification);
    // Altium emits IsMirrored / IsHidden only when true — never `=F`.
    let is_mirrored = if text.is_mirrored {
        "|IsMirrored=T"
    } else {
        ""
    };
    let is_hidden = if text.is_hidden { "|IsHidden=T" } else { "" };
    format!(
        // RECORD=3 is the Text-annotation id (the reader dispatches 3 -> parse_text,
        // 4 -> parse_label); emitting 4 here made a Text round-trip back as a Label.
        "|RECORD=3|IndexInSheet={}|OwnerPartId={}|IsNotAccesible=T{}{}{}|FontID={}|Orientation={}|Justification={}{}{}|{}|UniqueID={}",
        index,
        text.owner_part_id,
        coord_param("Location.X", text.x),
        coord_param("Location.Y", text.y),
        nonzero("Color", text.color),
        text.font_id,
        orientation,
        justification,
        is_mirrored,
        is_hidden,
        text_field("Text", &text.text),
        text.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Converts `TextJustification` to Altium ID.
const fn justification_to_id(justification: TextJustification) -> u8 {
    match justification {
        TextJustification::BottomLeft => 0,
        TextJustification::BottomCenter => 1,
        TextJustification::BottomRight => 2,
        TextJustification::MiddleLeft => 3,
        TextJustification::MiddleCenter => 4,
        TextJustification::MiddleRight => 5,
        TextJustification::TopLeft => 6,
        TextJustification::TopCenter => 7,
        TextJustification::TopRight => 8,
    }
}

/// Encodes an implementation list record (start of model list). Altium always
/// writes this record, even when a symbol has no footprint models.
fn encode_implementation_list() -> String {
    "|RECORD=44".to_string()
}

/// Counts the records already written to a Data-stream buffer, using the
/// `[u24 length LE][u8 flags][payload]` framing. The result is the stream-index
/// the next record will occupy (records are 0-indexed, matching the values
/// Altium stores in `OwnerIndex`).
fn count_records(data: &[u8]) -> usize {
    let mut offset = 0;
    let mut count = 0;
    while offset + 4 <= data.len() {
        let len = (data[offset] as usize)
            | ((data[offset + 1] as usize) << 8)
            | ((data[offset + 2] as usize) << 16);
        offset += 4 + len;
        count += 1;
    }
    count
}

/// Encodes a footprint model record (`RECORD=45`).
///
/// `owner_index` is the stream-index of the owning `RECORD=44` implementation list.
/// `is_current` marks the default footprint (`IsCurrent=T`, set on one model).
///
/// `DatafileCount=1` plus `ModelDatafileEntity0` is what lets Altium *resolve*
/// the model to an actual footprint in a `PcbLib` (rendering the preview and
/// finding it on placement); a name-only record with `DatafileCount=0` shows in
/// the list but reports "model not found".
fn encode_footprint_model(model: &FootprintModel, owner_index: usize, is_current: bool) -> String {
    // ModelDatafile0 (the .PcbLib path) is what lets Altium resolve the footprint
    // directly; omitted when no path is known (falls back to name search).
    let datafile = model
        .library_path
        .as_deref()
        .map(|p| format!("|ModelDatafile0={p}"))
        .unwrap_or_default();
    format!(
        "|RECORD=45|OwnerIndex={}|IndexInSheet=-1|Description={}|ModelName={}|ModelType=PCBLIB|DatafileCount=1{}|ModelDatafileEntity0={}|ModelDatafileKind0=PCBLib|IsCurrent={}|UniqueID={}",
        owner_index,
        model.description,
        model.name,
        datafile,
        model.name,
        if is_current { "T" } else { "F" },
        generate_unique_id()
    )
}

/// Encodes a model datafile link record (`RECORD=46`) — a child of a footprint
/// model. `owner_index` is the stream-index of the owning `RECORD=45`.
fn encode_model_datafile_link(owner_index: usize) -> String {
    format!("|RECORD=46|OwnerIndex={owner_index}")
}

/// Encodes an implementation record (`RECORD=48`) — a child of a footprint
/// model. `owner_index` is the stream-index of the owning `RECORD=45`.
fn encode_implementation(owner_index: usize) -> String {
    format!("|RECORD=48|OwnerIndex={owner_index}")
}

/// Generates a random 8-character unique ID (similar to Altium's format).
///
/// Uses a combination of system time and an atomic counter to ensure uniqueness
/// even when called multiple times in rapid succession.
use crate::util::generate_unique_id;

/// Encodes symbol primitives to binary format for the Data stream.
///
/// # Errors
///
/// Returns an error if any pin coordinates exceed the i16 range (±32767).
pub fn encode_data_stream(symbol: &Symbol) -> crate::altium::error::AltiumResult<Vec<u8>> {
    let mut data = Vec::new();
    let mut index_counter = 0usize;

    // 1. Component header
    let header = encode_component_header(symbol);
    write_text_record(&mut data, &header)?;

    // 2. Parameters (Value, Part Number, etc.)
    for param in &symbol.parameters {
        let record = encode_parameter(param, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 3. Rectangles — written before the pins so the body shape sits at the
    //    back. Emitting pins first lets a solid-filled body paint over the pin
    //    names that sit inside it (names vanish). This matches Altium's own
    //    ordering (body rectangle precedes pins in its symbol records).
    for rect in &symbol.rectangles {
        let record = encode_rectangle(rect, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 4. Pins (binary format)
    for pin in &symbol.pins {
        write_binary_pin(&mut data, pin)?;
        // Pins occupy an ordinal slot too; advance so later records' IndexInSheet
        // values match Altium's primitive numbering.
        index_counter += 1;
    }

    // 5. Lines
    for line in &symbol.lines {
        let record = encode_line(line, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 6. Polylines
    for polyline in &symbol.polylines {
        let record = encode_polyline(polyline, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 7. Polygons
    for polygon in &symbol.polygons {
        let record = encode_polygon(polygon, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 8. Arcs
    for arc in &symbol.arcs {
        let record = encode_arc(arc, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 8b. Pies (filled sectors, RECORD=9)
    for pie in &symbol.pies {
        let record = encode_pie(pie, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 8c. Images (RECORD=30)
    for image in &symbol.images {
        let record = encode_image(image, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 8d. Text frames (RECORD=28). Altium writes no IndexInSheet key for this
    // record, but a frame still occupies an ordinal slot, so the counter
    // advances to keep later records' IndexInSheet values in step.
    for text_frame in &symbol.text_frames {
        let record = encode_text_frame(text_frame);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 9. Bezier curves
    for bezier in &symbol.beziers {
        let record = encode_bezier(bezier, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 10. Ellipses
    for ellipse in &symbol.ellipses {
        let record = encode_ellipse(ellipse, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 11. Rounded rectangles
    for round_rect in &symbol.round_rects {
        let record = encode_round_rect(round_rect, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 12. Elliptical arcs
    for elliptical_arc in &symbol.elliptical_arcs {
        let record = encode_elliptical_arc(elliptical_arc, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 13. Labels
    for label in &symbol.labels {
        let record = encode_label(label, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 14. Text annotations
    for text in &symbol.text {
        let record = encode_text(text, index_counter);
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 15. Designator
    if !symbol.designator.is_empty() {
        let record = encode_designator(&symbol.designator);
        write_text_record(&mut data, &record)?;
    }

    // 16. Implementation list — Altium always writes RECORD=44, then a model
    // record per footprint.
    // Every footprint model (RECORD=45) is owned by the single RECORD=44
    // ImplementationList, so its OwnerIndex must be that record's stream-index —
    // not the model's own position (the previous behaviour, which orphaned every
    // model after the first).
    let impl_index = count_records(&data);
    write_text_record(&mut data, &encode_implementation_list())?;
    for (i, model) in symbol.footprints.iter().enumerate() {
        // The RECORD=45 is owned by the RECORD=44; its RECORD=46/48 children are
        // in turn owned by the RECORD=45 (its own stream-index).
        let model_index = count_records(&data);
        write_text_record(
            &mut data,
            &encode_footprint_model(model, impl_index, i == 0),
        )?;
        write_text_record(&mut data, &encode_model_datafile_link(model_index))?;
        write_text_record(&mut data, &encode_implementation(model_index))?;
    }

    // No end-of-stream sentinel: Altium reads records until the stream is
    // exhausted, and a trailing 0x0000 is mis-framed as a zero-length record
    // (issue #68, "Data does not end with 0x00").

    Ok(data)
}

/// Encodes the `FileHeader` stream content.
///
/// # Arguments
///
/// * `symbols` - The symbols to encode
/// * `ole_names` - OLE-safe storage names for each symbol (≤31 chars, unique)
#[must_use]
pub fn encode_file_header(symbols: &[&Symbol], ole_names: &[String]) -> Vec<u8> {
    let mut parts = vec![
        "HEADER=Protel for Windows - Schematic Library Editor Binary File Version 5.0".to_string(),
        "Weight=47".to_string(),
        "MinorVersion=9".to_string(),
        format!("UniqueID={}", generate_unique_id()),
        "FontIdCount=1".to_string(),
        "Size1=10".to_string(),
        "FontName1=Times New Roman".to_string(),
        "UseMBCS=T".to_string(),
        "IsBOC=T".to_string(),
        "SheetStyle=9".to_string(),
        "BorderOn=T".to_string(),
        "SheetNumberSpaceSize=12".to_string(),
        "AreaColor=16317695".to_string(),
        "SnapGridOn=T".to_string(),
        "SnapGridSize=10".to_string(),
        "VisibleGridOn=T".to_string(),
        "VisibleGridSize=10".to_string(),
        "CustomX=18000".to_string(),
        "CustomY=18000".to_string(),
        "UseCustomSheet=T".to_string(),
        "ReferenceZonesOn=T".to_string(),
        "Display_Unit=0".to_string(),
        format!("CompCount={}", symbols.len()),
    ];

    // Add component references using OLE-safe names for storage lookup
    for (i, (symbol, ole_name)) in symbols.iter().zip(ole_names.iter()).enumerate() {
        // LibRef uses the OLE-safe name (for storage path lookup)
        parts.push(format!("LibRef{i}={ole_name}"));
        parts.push(format!("CompDescr{}={}", i, symbol.description));
        parts.push(format!("PartCount{}={}", i, symbol.part_count + 1));
    }

    let text = format!("|{}", parts.join("|"));
    // Altium stores parameter strings as Windows-1252, not UTF-8 (#68).
    let text_bytes = crate::altium::encode_windows1252(&text);

    // Format: [length:4 LE][text + 0x00]. The block is a C-string: it MUST be
    // null-terminated and the length MUST include the terminator (matches Altium
    // WriteCStringParameterBlockRaw). Omitting it is issue #68's "Data does not
    // end with 0x00".
    let mut data = Vec::with_capacity(4 + text_bytes.len() + 1);
    write_cstring_param_block(&mut data, &text_bytes);

    data
}

#[cfg(test)]
mod tests {
    use super::super::primitives::PinOrientation;
    use super::*;

    #[test]
    fn single_part_symbol_emits_partcount_one() {
        // internal part_count 0 (single part) must re-emit PartCount=1, not the old
        // floored PartCount=2 — the write-back half of the round-trip fix.
        let mut symbol = Symbol::new("PC");
        symbol.part_count = 0;
        let header = encode_component_header(&symbol);
        assert!(
            header.contains("|PartCount=1|"),
            "single-part symbol re-emits PartCount=1: {header}"
        );
    }

    #[test]
    fn pin_tail_default_is_byte_identical() {
        use crate::altium::schlib::primitives::Pin;
        let pin = Pin::new("VCC", "1", 0, 0, 100, PinOrientation::Right);
        let mut data = Vec::new();
        write_binary_pin(&mut data, &pin).unwrap();
        // Default tail must be exactly: swap_id_group="", part_and_sequence="|&|",
        // default_value="" — the same bytes the writer emitted before the tail
        // fields became round-trippable. This is the load-bearing byte-identity
        // check; formal_type=1 leaves the formal-type byte at 0x01 unchanged.
        assert!(data.ends_with(&[0x00, 0x03, b'|', b'&', b'|', 0x00]));
    }

    #[test]
    fn test_write_text_record() {
        let mut data = Vec::new();
        write_text_record(&mut data, "|RECORD=1|Name=Test|").unwrap();

        // Check header
        let length = u16::from_le_bytes([data[0], data[1]]);
        let record_type = u16::from_be_bytes([data[2], data[3]]);

        assert_eq!(length, 21); // "|RECORD=1|Name=Test|" + null
        assert_eq!(record_type, 0); // Text record
    }

    #[test]
    fn test_encode_simple_symbol() {
        let mut symbol = Symbol::new("TEST");
        symbol.description = "Test symbol".to_string();
        symbol.designator = "U?".to_string();
        symbol.add_pin(Pin::new("IN", "1", -10, 0, 10, PinOrientation::Right));
        symbol.add_rectangle(Rectangle::new(-5, -5, 5, 5));

        let data = encode_data_stream(&symbol).expect("encoding should succeed");

        // Should have content
        assert!(!data.is_empty());

        // No end-of-stream sentinel; the stream ends with the last text record's
        // null terminator (the always-present RECORD=44 implementation list).
        assert_eq!(*data.last().unwrap(), 0x00);
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("RECORD=1"), "component record present");
        assert!(text.contains("RECORD=44"), "implementation list present");
    }

    #[test]
    fn test_rectangle_issolid_emitted_only_when_filled() {
        // Altium omits IsSolid for unfilled shapes and emits IsSolid=T only when
        // filled — never IsSolid=F.
        let mut unfilled = Rectangle::new(-5, -5, 5, 5);
        unfilled.filled = false;
        let s = encode_rectangle(&unfilled, 1);
        assert!(
            !s.contains("IsSolid"),
            "unfilled rectangle must omit IsSolid: {s}"
        );

        let mut filled = Rectangle::new(-5, -5, 5, 5);
        filled.filled = true;
        let s = encode_rectangle(&filled, 1);
        assert!(
            s.contains("|IsSolid=T"),
            "filled rectangle must emit IsSolid=T: {s}"
        );
        assert!(!s.contains("IsSolid=F"), "never emit IsSolid=F: {s}");
    }

    #[test]
    fn text_frame_default_is_byte_identical_to_altium() {
        // A from-scratch TextFrame must emit exactly the record Altium itself
        // writes for a from-scratch frame (AltiumSharp's generated
        // TEXTFRAME_TEST.SchLib golden), token for token. Only the trailing
        // UniqueID (freshly generated) differs.
        let frame = TextFrame::new(-20, -10, 20, 10, "Test Frame");
        let s = encode_text_frame(&frame);
        assert!(
            s.starts_with(
                "|RECORD=28|IsNotAccesible=T|OwnerPartId=1\
                 |Location.X=-20|Location.Y=-10|Corner.X=20|Corner.Y=10\
                 |AreaColor=16777215|FontID=1|ShowBorder=T|Alignment=1\
                 |WordWrap=T|ClipToRect=T|Text=Test Frame|TextMargin_Frac=5\
                 |UniqueID="
            ),
            "default text frame must be byte-identical to Altium's own record: {s}"
        );
        // Omit-when-default keys a default frame must NOT carry.
        for absent in [
            "IndexInSheet",
            "LineWidth",
            "LineStyle",
            "|Color=",
            "TextColor",
            "IsSolid",
            "Transparent",
            "Orientation",
            "TextMargin=",
        ] {
            assert!(!s.contains(absent), "default frame must omit {absent}: {s}");
        }
    }

    #[test]
    fn footprint_models_owned_by_implementation_list() {
        let mut symbol = Symbol::new("R");
        symbol.add_pin(Pin::new("1", "1", -10, 0, 10, PinOrientation::Left));
        symbol.add_pin(Pin::new("2", "2", 10, 0, 10, PinOrientation::Right));
        let mut a = FootprintModel::new("R0402");
        a.library_path = Some("X:/Lib/Test.PcbLib".to_string());
        symbol.add_footprint(a);
        symbol.add_footprint(FootprintModel::new("R0603"));

        let data = encode_data_stream(&symbol).expect("encode");

        // Parse records: [u24 length LE][u8 flags][payload].
        let mut records: Vec<String> = Vec::new();
        let mut off = 0;
        while off + 4 <= data.len() {
            let len = data[off] as usize
                | ((data[off + 1] as usize) << 8)
                | ((data[off + 2] as usize) << 16);
            records.push(String::from_utf8_lossy(&data[off + 4..off + 4 + len]).into_owned());
            off += 4 + len;
        }

        let impl_idx = records
            .iter()
            .position(|t| t.contains("|RECORD=44"))
            .expect("RECORD=44 present");
        let models: Vec<&String> = records
            .iter()
            .filter(|t| t.contains("|RECORD=45"))
            .collect();
        assert_eq!(models.len(), 2, "both footprint models written");
        for m in &models {
            // Every model is owned by the single implementation list, not its own index.
            assert!(
                m.contains(&format!("OwnerIndex={impl_idx}")),
                "model owned by RECORD=44 (index {impl_idx}): {m}"
            );
        }
        // The library path is emitted as ModelDatafile0 so Altium can resolve it.
        assert!(records
            .iter()
            .any(|t| t.contains("ModelDatafile0=X:/Lib/Test.PcbLib")));
        // Each model carries its RECORD=46 / RECORD=48 children.
        assert!(records.iter().any(|t| t.contains("|RECORD=46")));
        assert!(records.iter().any(|t| t.contains("|RECORD=48")));
    }

    #[test]
    fn body_rectangle_is_written_before_pins() {
        // A solid-filled body must sit behind the pins, else its fill paints
        // over the pin names. The rectangle record must precede every pin.
        let mut symbol = Symbol::new("TEST");
        symbol.add_rectangle(Rectangle::new(-30, -30, 30, 30));
        symbol.add_pin(Pin::new("IN", "1", -60, 10, 30, PinOrientation::Left));
        symbol.add_pin(Pin::new("OUT", "2", 60, 10, 30, PinOrientation::Right));

        let data = encode_data_stream(&symbol).expect("encoding should succeed");

        // Walk the record stream: [len:3 LE][flags:1][payload]; flags 1 = pin.
        let mut off = 0;
        let mut rect_idx = None;
        let mut first_pin_idx = None;
        let mut idx = 0;
        while off + 4 <= data.len() {
            let len = (data[off] as usize)
                | ((data[off + 1] as usize) << 8)
                | ((data[off + 2] as usize) << 16);
            let flags = data[off + 3];
            let payload = &data[off + 4..off + 4 + len];
            if flags == 1 && first_pin_idx.is_none() {
                first_pin_idx = Some(idx);
            } else if flags == 0
                && rect_idx.is_none()
                && String::from_utf8_lossy(payload).contains("RECORD=14")
            {
                rect_idx = Some(idx);
            }
            off += 4 + len;
            idx += 1;
        }
        let rect_idx = rect_idx.expect("rectangle record present");
        let first_pin_idx = first_pin_idx.expect("pin record present");
        assert!(
            rect_idx < first_pin_idx,
            "rectangle (idx {rect_idx}) must precede the first pin (idx {first_pin_idx})"
        );
    }

    #[test]
    fn test_encode_pin_coordinate_overflow() {
        let mut symbol = Symbol::new("TEST");
        symbol.add_pin(Pin::new("IN", "1", 50000, 0, 10, PinOrientation::Right)); // x exceeds i16

        let result = encode_data_stream(&symbol);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("exceeds i16 range"));
    }

    #[test]
    fn test_encode_pin_name_too_long() {
        let mut symbol = Symbol::new("TEST");
        let long_name = "A".repeat(256); // Exceeds 255 byte limit
        symbol.add_pin(Pin::new(&long_name, "1", 0, 0, 10, PinOrientation::Right));

        let result = encode_data_stream(&symbol);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("exceeds maximum of 255 bytes"));
    }

    #[test]
    fn test_encode_file_header() {
        let symbol = Symbol::new("TEST_SYMBOL");
        let symbols = vec![&symbol];
        let ole_names = vec!["TEST_SYMBOL".to_string()];

        let data = encode_file_header(&symbols, &ole_names);

        // Should start with length
        assert!(data.len() > 4);
        let length = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        assert_eq!(data.len(), 4 + length);

        // Should contain component info
        let text = String::from_utf8_lossy(&data[4..]);
        assert!(text.contains("HEADER="));
        assert!(text.contains("CompCount=1"));
        assert!(text.contains("LibRef0=TEST_SYMBOL"));
    }

    #[test]
    fn test_encode_file_header_long_name() {
        let long_name = "A".repeat(64);
        let symbol = Symbol::new(&long_name);
        let symbols = vec![&symbol];
        // OLE name is truncated
        let ole_names = vec!["AAAAAAAAAAAAAAAAAAAAAAAAAAA~001".to_string()];

        let data = encode_file_header(&symbols, &ole_names);

        // LibRef should use the OLE-safe name
        let text = String::from_utf8_lossy(&data[4..]);
        assert!(text.contains("LibRef0=AAAAAAAAAAAAAAAAAAAAAAAAAAA~001"));
    }

    #[test]
    fn test_parameter_canonical_emission() {
        // Not hidden, empty value, zero read-only/param-type: Altium omits those keys.
        let mut p = Parameter::new("Comment", "");
        let s = encode_parameter(&p, 1);
        assert!(
            !s.contains("IsHidden"),
            "omit IsHidden when not hidden: {s}"
        );
        assert!(!s.contains("Text="), "omit Text when empty: {s}");
        assert!(
            !s.contains("ReadOnlyState"),
            "omit ReadOnlyState when 0: {s}"
        );
        assert!(!s.contains("ParamType"), "omit ParamType when 0: {s}");
        // The EE-meaningful display fields are omit-when-default too, so a
        // from-scratch parameter stays byte-identical to Altium's output.
        assert!(!s.contains("Orientation"), "omit Orientation when 0: {s}");
        assert!(!s.contains("ShowName"), "omit ShowName when false: {s}");
        assert!(!s.contains("HideName"), "omit HideName when false: {s}");
        assert!(
            !s.contains("Description"),
            "omit Description when empty: {s}"
        );
        assert!(
            !s.contains("IsConfigurable"),
            "omit IsConfigurable when false: {s}"
        );

        // Hidden + value + a preserved UniqueID.
        p.hidden = true;
        p.value = "10k".to_string();
        p.unique_id = Some("ABCD1234".to_string());
        let s = encode_parameter(&p, 1);
        assert!(
            s.contains("|IsHidden=T"),
            "emit IsHidden=T when hidden: {s}"
        );
        assert!(!s.contains("IsHidden=F"), "never IsHidden=F: {s}");
        assert!(s.contains("|Text=10k"), "emit Text when set: {s}");
        assert!(
            s.contains("|UniqueID=ABCD1234"),
            "preserve read UniqueID: {s}"
        );

        // Non-default EE-meaningful fields are each emitted with the Altium key.
        p.orientation = 2;
        p.show_name = true;
        p.hide_name = true;
        p.is_configurable = true;
        p.description = "Resistance".to_string();
        let s = encode_parameter(&p, 1);
        assert!(
            s.contains("|Orientation=2"),
            "emit Orientation when set: {s}"
        );
        assert!(s.contains("|ShowName=T"), "emit ShowName when set: {s}");
        assert!(s.contains("|HideName=T"), "emit HideName when set: {s}");
        assert!(
            s.contains("|IsConfigurable=T"),
            "emit IsConfigurable when set: {s}"
        );
        assert!(
            s.contains("|Description=Resistance"),
            "emit Description when set: {s}"
        );
    }

    #[test]
    fn test_parameter_ee_fields_roundtrip() {
        // A parameter with the de-hardcoded + EE-meaningful fields set survives a
        // full write -> read round-trip through a one-symbol library.
        let mut symbol = Symbol::new("R");
        let mut p = Parameter::new("Value", "10k");
        p.read_only_state = 1;
        p.param_type = 2;
        p.orientation = 3;
        p.show_name = true;
        p.hide_name = true;
        p.description = "Resistance".to_string();
        p.is_configurable = true;
        p.unique_id = Some("WXYZ7890".to_string());
        symbol.add_parameter(p);

        let mut lib = crate::altium::schlib::SchLib::new();
        lib.add(symbol);
        let mut buf = std::io::Cursor::new(Vec::new());
        lib.write(&mut buf).expect("library should serialise");
        buf.set_position(0);
        let back_lib =
            crate::altium::schlib::SchLib::read(buf).expect("library should deserialise");
        let back_sym = back_lib.get("R").expect("symbol R round-trips");
        let back = back_sym
            .parameters
            .iter()
            .find(|q| q.name == "Value")
            .expect("Value parameter round-trips");
        assert_eq!(back.read_only_state, 1);
        assert_eq!(back.param_type, 2);
        assert_eq!(back.orientation, 3);
        assert!(back.show_name);
        assert!(back.hide_name);
        assert_eq!(back.description, "Resistance");
        assert!(back.is_configurable);
        assert_eq!(back.unique_id.as_deref(), Some("WXYZ7890"));
    }

    #[test]
    fn test_rectangle_unique_id_roundtrip() {
        // PR-R1: a SchLib shape's identity GUID (`unique_id`) survives a full
        // write -> read round-trip, so a read-modify-write keeps stable primitive
        // identity instead of regenerating a fresh GUID.
        let mut symbol = Symbol::new("R");
        let mut rect = Rectangle::new(-10, -5, 10, 5);
        rect.unique_id = Some("RECTUID7".to_string());
        symbol.add_rectangle(rect);

        let mut lib = crate::altium::schlib::SchLib::new();
        lib.add(symbol);
        let mut buf = std::io::Cursor::new(Vec::new());
        lib.write(&mut buf).expect("library should serialise");
        buf.set_position(0);
        let back_lib =
            crate::altium::schlib::SchLib::read(buf).expect("library should deserialise");
        let back_sym = back_lib.get("R").expect("symbol R round-trips");
        assert_eq!(
            back_sym.rectangles[0].unique_id.as_deref(),
            Some("RECTUID7")
        );
    }

    #[test]
    fn test_label_booleans_only_when_true() {
        let mut label = Label {
            x: 0.0,
            y: 0.0,
            text: "R".to_string(),
            font_id: 1,
            color: 0,
            justification: TextJustification::BottomLeft,
            rotation: 0.0,
            is_mirrored: false,
            is_hidden: false,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: Some("ABCD1234".to_string()),
        };
        let s = encode_label(&label, 1);
        assert!(!s.contains("IsMirrored"), "omit IsMirrored when false: {s}");
        assert!(!s.contains("IsHidden"), "omit IsHidden when false: {s}");

        label.is_mirrored = true;
        label.is_hidden = true;
        let s = encode_label(&label, 1);
        assert!(s.contains("|IsMirrored=T"), "emit IsMirrored=T: {s}");
        assert!(s.contains("|IsHidden=T"), "emit IsHidden=T: {s}");
        assert!(
            !s.contains("IsMirrored=F") && !s.contains("IsHidden=F"),
            "never =F: {s}"
        );
    }

    #[test]
    fn test_arc_tags_is_not_accessible() {
        let arc = Arc {
            x: 0.0,
            y: 0.0,
            radius: 10.0,
            is_not_accessible: true,
            start_angle: 0.0,
            end_angle: 360.0,
            line_width: 1,
            color: 0,
            fill_color: 0,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: Some("ABCD1234".to_string()),
        };
        let s = encode_arc(&arc, 1);
        assert!(
            s.contains("|IsNotAccesible=T"),
            "arc must tag IsNotAccesible: {s}"
        );
    }

    #[test]
    fn test_colour_omitted_when_zero() {
        // Altium omits Color / AreaColor when 0 (AddNonZero); emits them otherwise.
        let mut arc = Arc {
            x: 0.0,
            y: 0.0,
            radius: 10.0,
            is_not_accessible: true,
            start_angle: 0.0,
            end_angle: 360.0,
            line_width: 1,
            color: 0,
            fill_color: 0,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: Some("ABCD1234".to_string()),
        };
        assert!(
            !encode_arc(&arc, 1).contains("Color="),
            "zero arc Color must be omitted"
        );
        arc.color = 255;
        assert!(
            encode_arc(&arc, 1).contains("|Color=255"),
            "non-zero arc Color must be emitted"
        );

        let s = encode_text(
            &Text {
                x: 0.0,
                y: 0.0,
                text: "hi".to_string(),
                font_id: 1,
                color: 0,
                justification: TextJustification::BottomLeft,
                rotation: 0.0,
                is_mirrored: false,
                is_hidden: false,
                owner_part_id: 1,
                unique_id: Some("ABCD1234".to_string()),
            },
            1,
        );
        assert!(!s.contains("Color="), "zero text Color omitted: {s}");
        assert!(!s.contains("=F"), "text never emits a boolean =F: {s}");
    }

    #[test]
    fn encode_line_omits_frac_for_integer_coords() {
        // Byte-identity: an integer-grid line must emit its coordinates plainly
        // with no `_Frac` companion, so existing files are unchanged by the
        // f64 coordinate migration.
        let s = encode_line(&Line::new(-10, 0, 10, 0), 1);
        assert!(
            s.contains("|Location.X=-10|"),
            "integer X emitted plainly: {s}"
        );
        assert!(s.contains("|Corner.X=10|"), "integer corner X plainly: {s}");
        assert!(
            !s.contains("_Frac"),
            "an integer-grid line must emit no _Frac token: {s}"
        );
    }

    #[test]
    fn display_flags_default_shapes_are_byte_identical() {
        // A default shape (all four universal flags at their defaults) must emit
        // NO new key — Altium omits them when default, so the encoded record is
        // unchanged from pre-flag output. Covers all nine graphic shapes.
        use crate::altium::schlib::primitives::{
            Ellipse, Label, Parameter, Polygon, Polyline, RoundRect,
        };

        let rect = encode_rectangle(&Rectangle::new(-5, -5, 5, 5), 1);
        let round = encode_round_rect(&RoundRect::new(-5, -5, 5, 5, 1, 1), 1);
        let ell = encode_ellipse(&Ellipse::new(0, 0, 5, 5), 1);
        let line = encode_line(&Line::new(-5, 0, 5, 0), 1);
        let poly_line = encode_polyline(
            &Polyline {
                points: vec![(0.0, 0.0), (5.0, 5.0)],
                line_width: 1,
                color: 0,
                line_style: 0,
                start_line_shape: 0,
                end_line_shape: 0,
                line_shape_size: 0,
                transparent: false,
                owner_part_id: 1,
                display_flags: ShapeDisplayFlags::default(),
                unique_id: Some("ABCD1234".to_string()),
            },
            1,
        );
        let poly = encode_polygon(
            &Polygon {
                points: vec![(0.0, 0.0), (5.0, 0.0), (2.5, 5.0)],
                line_width: 1,
                line_color: 0,
                fill_color: 0,
                line_style: 0,
                filled: true,
                transparent: false,
                is_not_accessible: true,
                owner_part_id: 1,
                display_flags: ShapeDisplayFlags::default(),
                unique_id: Some("ABCD1234".to_string()),
            },
            1,
        );
        let arc = encode_arc(
            &Arc {
                x: 0.0,
                y: 0.0,
                radius: 10.0,
                is_not_accessible: true,
                start_angle: 0.0,
                end_angle: 360.0,
                line_width: 1,
                color: 0,
                fill_color: 0,
                owner_part_id: 1,
                display_flags: ShapeDisplayFlags::default(),
                unique_id: Some("ABCD1234".to_string()),
            },
            1,
        );
        let label = encode_label(
            &Label {
                x: 0.0,
                y: 0.0,
                text: "R".to_string(),
                font_id: 1,
                color: 0,
                justification: TextJustification::BottomLeft,
                rotation: 0.0,
                is_mirrored: false,
                is_hidden: false,
                owner_part_id: 1,
                display_flags: ShapeDisplayFlags::default(),
                unique_id: Some("ABCD1234".to_string()),
            },
            1,
        );
        let param = encode_parameter(&Parameter::new("Value", ""), 1);

        for (name, s) in [
            ("rectangle", rect),
            ("round_rect", round),
            ("ellipse", ell),
            ("line", line),
            ("polyline", poly_line),
            ("polygon", poly),
            ("arc", arc),
            ("label", label),
            ("parameter", param),
        ] {
            assert!(
                !s.contains("GraphicallyLocked")
                    && !s.contains("Disabled")
                    && !s.contains("Dimmed")
                    && !s.contains("OwnerPartDisplayMode"),
                "{name} with default display flags must emit no flag key: {s}"
            );
        }
    }

    #[test]
    fn display_flags_emitted_only_when_non_default() {
        let mut rect = Rectangle::new(-5, -5, 5, 5);
        rect.display_flags.graphically_locked = true;
        rect.display_flags.disabled = true;
        rect.display_flags.dimmed = true;
        rect.display_flags.owner_part_display_mode = 1;
        let s = encode_rectangle(&rect, 1);
        assert!(s.contains("|GraphicallyLocked=T"), "emit locked: {s}");
        assert!(s.contains("|Disabled=T"), "emit disabled: {s}");
        assert!(s.contains("|Dimmed=T"), "emit dimmed: {s}");
        assert!(
            s.contains("|OwnerPartDisplayMode=1"),
            "emit display mode: {s}"
        );
        // Never a `=F` for the three display booleans (matches omit-when-default).
        assert!(
            !s.contains("GraphicallyLocked=F")
                && !s.contains("Disabled=F")
                && !s.contains("Dimmed=F"),
            "never emit a display-flag boolean =F: {s}"
        );
    }

    #[test]
    fn encode_line_emits_frac_for_fractional_and_negative_coords() {
        // AD24's toward-zero/signed split (the FRACSHAPES golden convention):
        // -5.45 -> Location.X=-5 with Location.X_Frac=-45000; the positive
        // 7.5 -> 7 + 50000. This is the capability the integer field could not
        // represent at all.
        let mut line = Line::new(-5.45, 7.5, 5.55, 0);
        line.unique_id = Some("ABCD1234".to_string());
        let s = encode_line(&line, 1);
        assert!(
            s.contains("|Location.X=-5|Location.X_Frac=-45000|"),
            "negative off-grid coordinate emits Altium's exact signed form: {s}"
        );
        assert!(s.contains("|Location.Y=7|"), "Y integer part: {s}");
        assert!(s.contains("|Location.Y_Frac=50000|"), "Y fractional: {s}");
        assert!(
            s.contains("|Corner.X=5|Corner.X_Frac=55000|"),
            "positive off-grid coordinate: {s}"
        );
    }

    #[test]
    fn encode_arc_omits_zero_integer_when_fractional() {
        // AD24 omits a zero integer coordinate key when its `_Frac` companion is
        // non-zero (the FRACSHAPES golden arc carries `Location.X_Frac=5000`
        // with no `Location.X` key); an on-grid zero still emits `=0`.
        let arc = Arc {
            x: 0.05,
            y: 0.05,
            radius: 4.05,
            is_not_accessible: true,
            start_angle: 0.0,
            end_angle: 270.0,
            line_width: 1,
            color: 0,
            fill_color: 0,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: Some("ABCD1234".to_string()),
        };
        let s = encode_arc(&arc, 1);
        assert!(
            !s.contains("|Location.X=") && !s.contains("|Location.Y="),
            "zero integer part with a fraction omits the integer key: {s}"
        );
        assert!(
            s.contains("|Location.X_Frac=5000|") && s.contains("|Location.Y_Frac=5000|"),
            "the fraction alone carries the coordinate: {s}"
        );
        assert!(
            s.contains("|Radius=4|Radius_Frac=5000|"),
            "non-zero integer keeps both keys: {s}"
        );
    }

    #[test]
    fn win1252_text_stays_byte_identical_no_utf8_key() {
        // A pure-Windows-1252 value (the common case, and everything in the golden
        // library) must emit the plain `Text=` key exactly as before the UTF-8 fix
        // — no `%UTF8%Text` key, so the record bytes are unchanged (oracle-clean).
        // `µ` (U+00B5) is representable in Windows-1252, so it stays plain.
        let mut p = Parameter::new("Value", "10\u{00B5}F"); // "10µF"
        p.unique_id = Some("ABCD1234".to_string());
        let s = encode_parameter(&p, 1);
        assert!(s.contains("|Text=10\u{00B5}F|"), "plain Text key: {s}");
        assert!(
            !s.contains("%UTF8%"),
            "no %UTF8% key for Win-1252 value: {s}"
        );

        let mut label = Label {
            x: 0.0,
            y: 0.0,
            text: "caf\u{00E9}".to_string(), // "café" — all Windows-1252
            font_id: 1,
            color: 0,
            justification: TextJustification::BottomLeft,
            rotation: 0.0,
            is_mirrored: false,
            is_hidden: false,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: Some("ABCD1234".to_string()),
        };
        let s = encode_label(&label, 1);
        assert!(s.contains("|Text=caf\u{00E9}|"), "plain Text key: {s}");
        assert!(
            !s.contains("%UTF8%"),
            "no %UTF8% key for Win-1252 label: {s}"
        );

        // And an ASCII label is byte-identical to the pre-change output.
        label.text = "R".to_string();
        let s = encode_label(&label, 1);
        assert!(s.contains("|Text=R|"), "plain ASCII Text: {s}");
        assert!(!s.contains("%UTF8%"), "no %UTF8% key for ASCII: {s}");
    }

    #[test]
    fn non_win1252_text_emits_only_utf8_key() {
        // Greek Ω (U+03A9) is NOT in Windows-1252. The writer must emit the value
        // behind `%UTF8%Text` (never a lossy plain `Text=10k?`), matching Altium.
        let mut p = Parameter::new("Value", "10k\u{03A9}"); // "10kΩ"
        p.unique_id = Some("ABCD1234".to_string());
        let s = encode_parameter(&p, 1);
        assert!(s.contains("|%UTF8%Text="), "emit %UTF8%Text key: {s}");
        // Exactly one Text key, and no lossy plain `Text=...?`.
        assert!(
            !s.contains("|Text="),
            "must not also emit a lossy plain Text: {s}"
        );
        // The stored value is the UTF-8 byte sequence mapped one-char-per-byte.
        let expected = crate::altium::encode_utf8_param_value("10k\u{03A9}");
        assert!(
            s.contains(&format!("|%UTF8%Text={expected}|")),
            "stored UTF-8 form: {s}"
        );
    }

    #[test]
    fn non_latin_text_round_trips_intact_through_library() {
        // The headline correctness fix: a Label and a Parameter whose values are
        // NOT representable in Windows-1252 survive a full write -> read round-trip
        // with the exact Unicode string intact — not the `?`-mangled corruption
        // that today's plain-Text-only path produces.
        for value in [
            "10k\u{03A9}",
            "\u{041F}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}",
            "\u{6284}\u{6297}\u{5668}",
        ] {
            let mut symbol = Symbol::new("R");
            let mut p = Parameter::new("Value", value);
            p.unique_id = Some("WXYZ7890".to_string());
            symbol.add_parameter(p);
            symbol.add_label(Label {
                x: 0.0,
                y: 0.0,
                text: value.to_string(),
                font_id: 1,
                color: 0,
                justification: TextJustification::BottomLeft,
                rotation: 0.0,
                is_mirrored: false,
                is_hidden: false,
                owner_part_id: 1,
                display_flags: ShapeDisplayFlags::default(),
                unique_id: Some("ABCD1234".to_string()),
            });
            symbol.designator = value.to_string();

            let mut lib = crate::altium::schlib::SchLib::new();
            lib.add(symbol);
            let mut buf = std::io::Cursor::new(Vec::new());
            lib.write(&mut buf).expect("library should serialise");
            buf.set_position(0);
            let back_lib =
                crate::altium::schlib::SchLib::read(buf).expect("library should deserialise");
            let sym = back_lib.get("R").expect("symbol R round-trips");

            let param = sym
                .parameters
                .iter()
                .find(|q| q.name == "Value")
                .expect("Value parameter round-trips");
            assert_eq!(
                param.value, value,
                "parameter value must survive UTF-8 round-trip intact, not be ?-mangled"
            );
            assert_eq!(
                sym.labels[0].text, value,
                "label text must survive UTF-8 round-trip intact"
            );
            assert_eq!(
                sym.designator, value,
                "designator must survive UTF-8 round-trip intact"
            );
        }
    }
}
