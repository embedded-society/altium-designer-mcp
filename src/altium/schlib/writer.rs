//! Binary writer for `SchLib` Data streams.
//!
//! This module handles encoding symbol primitives to the binary format
//! used in Altium `SchLib` Data streams.
//!
//! # Data Stream Format
//!
//! ```text
//! [RecordLength:2 LE][RecordType:2 BE][data:RecordLength]
//! ...
//! ```
//!
//! The 4-byte record header is equivalent to Altium's single 32-bit
//! little-endian size word whose high byte is a flag (0x00 text, 0x01 pin).
//! Records run until the stream is exhausted — there is NO end-of-stream
//! marker (a trailing 0x0000 would be mis-read as a zero-length record).
//!
//! Record types:
//! - `0x0000`: Text record (pipe-delimited key=value pairs)
//! - `0x0001`: Binary pin record

use super::primitives::{
    Arc, Bezier, Ellipse, EllipticalArc, FootprintModel, Label, Line, Parameter, Pin, Polygon,
    Polyline, Rectangle, RoundRect, Text, TextJustification,
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
fn write_binary_pin(data: &mut Vec<u8>, pin: &Pin) -> crate::altium::error::AltiumResult<()> {
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
    for (bytes, field) in [
        (&name, "name"),
        (&designator, "designator"),
        (&description, "description"),
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

    // Owner part display mode (1 byte)
    record.push(0x00);

    // Symbol flags (4 bytes: inner_edge, outer_edge, inside, outside)
    record.push(pin.symbol_inner_edge.to_id());
    record.push(pin.symbol_outer_edge.to_id());
    record.push(pin.symbol_inside.to_id());
    record.push(pin.symbol_outside.to_id());

    // Description: Pascal short string [length:1][string]
    write_pascal_string(&mut record, &description);

    // Formal type (1 byte) - 0x01 for a normal pin (matches Altium's output).
    record.push(0x01);

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

    // Pin swap-id tail (Pascal short strings), matching Altium's output:
    //   SwapIdGroup = "" , PartAndSequence = "|&|" , DefaultValue = "".
    record.push(0); // SwapIdGroup (empty)
    record.push(3); // PartAndSequence length
    record.extend_from_slice(b"|&|");
    record.push(0); // DefaultValue (empty)

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
         |Location.X={}|Location.Y={}|Corner.X={}|Corner.Y={}\
         |LineWidth={}{}{}{}{}|Transparent={}|UniqueID={}",
        index,
        rect.owner_part_id,
        rect.x1,
        rect.y1,
        rect.x2,
        rect.y2,
        rect.line_width,
        nonzero("Color", rect.line_color),
        nonzero("AreaColor", rect.fill_color),
        line_style,
        is_solid,
        transparent,
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
        "|RECORD=13|IndexInSheet={}|OwnerPartId={}{}|Location.X={}|Location.Y={}|Corner.X={}|Corner.Y={}|LineWidth={}{}{}|UniqueID={}",
        index,
        line.owner_part_id,
        not_accessible,
        line.x1,
        line.y1,
        line.x2,
        line.y2,
        line.line_width,
        nonzero("Color", line.color),
        line_style,
        line.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes a parameter record.
///
/// Follows Altium's conventions: `IsHidden` is emitted only when hidden (never
/// `=F`), `ReadOnlyState` / `ParamType` only when non-zero, `Text` only when
/// non-empty, and the read `UniqueID` is preserved.
fn encode_parameter(param: &Parameter, index: usize) -> String {
    let mut parts = vec![
        "RECORD=41".to_string(),
        format!("IndexInSheet={index}"),
        format!("OwnerPartId={}", param.owner_part_id),
        format!("Location.X={}", param.x),
        format!("Location.Y={}", param.y),
        format!("Color={}", param.color),
        format!("FontID={}", param.font_id),
    ];
    if param.hidden {
        parts.push("IsHidden=T".to_string());
    }
    if param.read_only_state != 0 {
        parts.push(format!("ReadOnlyState={}", param.read_only_state));
    }
    if param.param_type != 0 {
        parts.push(format!("ParamType={}", param.param_type));
    }
    if !param.value.is_empty() {
        parts.push(format!("Text={}", param.value));
    }
    parts.push(format!("Name={}", param.name));
    parts.push(format!(
        "UniqueID={}",
        param.unique_id.clone().unwrap_or_else(generate_unique_id)
    ));
    format!("|{}", parts.join("|"))
}

/// Encodes a designator record.
fn encode_designator(designator: &str) -> String {
    format!(
        "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.Y=-6|Color=8388608|FontID=1|Text={}|Name=Designator|ReadOnlyState=1|UniqueID={}",
        designator,
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
        parts.push(format!("X{}={}", i + 1, x));
        parts.push(format!("Y{}={}", i + 1, y));
    }

    // Altium emits Transparent only when true; absent means opaque.
    if polyline.transparent {
        parts.push("Transparent=T".to_string());
    }

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
        "IsNotAccesible=T".to_string(),
        format!("LineWidth={}", polygon.line_width),
    ];
    // Altium omits Color / AreaColor when zero (AddNonZero).
    if polygon.line_color != 0 {
        parts.push(format!("Color={}", polygon.line_color));
    }
    if polygon.fill_color != 0 {
        parts.push(format!("AreaColor={}", polygon.fill_color));
    }
    // Altium emits IsSolid only when filled, and omits it otherwise.
    if polygon.filled {
        parts.push("IsSolid=T".to_string());
    }
    parts.push(format!("LocationCount={}", polygon.points.len()));

    for (i, (x, y)) in polygon.points.iter().enumerate() {
        parts.push(format!("X{}={}", i + 1, x));
        parts.push(format!("Y{}={}", i + 1, y));
    }

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
        "|RECORD=12|IndexInSheet={}|OwnerPartId={}{}|Location.X={}|Location.Y={}|Radius={}|StartAngle={}|EndAngle={}|LineWidth={}{}{}|UniqueID={}",
        index,
        arc.owner_part_id,
        not_accessible,
        arc.x,
        arc.y,
        arc.radius,
        arc.start_angle,
        arc.end_angle,
        arc.line_width,
        nonzero("Color", arc.color),
        nonzero("AreaColor", arc.fill_color),
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
        "|RECORD=5|IndexInSheet={}|OwnerPartId={}{}|LineWidth={}{}|LocationCount=4|X1={}|Y1={}|X2={}|Y2={}|X3={}|Y3={}|X4={}|Y4={}|UniqueID={}",
        index,
        bezier.owner_part_id,
        not_accessible,
        bezier.line_width,
        nonzero("Color", bezier.color),
        bezier.x1,
        bezier.y1,
        bezier.x2,
        bezier.y2,
        bezier.x3,
        bezier.y3,
        bezier.x4,
        bezier.y4,
        bezier.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes an ellipse record.
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
        "|RECORD=8|IndexInSheet={}|OwnerPartId={}|Location.X={}|Location.Y={}|Radius={}|SecondaryRadius={}|LineWidth={}{}{}{}{}|UniqueID={}",
        index,
        ellipse.owner_part_id,
        ellipse.x,
        ellipse.y,
        ellipse.radius_x,
        ellipse.radius_y,
        ellipse.line_width,
        nonzero("Color", ellipse.line_color),
        nonzero("AreaColor", ellipse.fill_color),
        is_solid,
        transparent,
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
         |Location.X={}|Location.Y={}|Corner.X={}|Corner.Y={}\
         |CornerXRadius={}|CornerYRadius={}\
         |LineWidth={}{}{}{}{}{}|UniqueID={}",
        index,
        round_rect.owner_part_id,
        round_rect.x1,
        round_rect.y1,
        round_rect.x2,
        round_rect.y2,
        round_rect.corner_x_radius,
        round_rect.corner_y_radius,
        round_rect.line_width,
        nonzero("Color", round_rect.line_color),
        nonzero("AreaColor", round_rect.fill_color),
        line_style,
        is_solid,
        transparent,
        round_rect
            .unique_id
            .clone()
            .unwrap_or_else(generate_unique_id)
    )
}

/// Encodes an elliptical arc record.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn encode_elliptical_arc(arc: &EllipticalArc, index: usize) -> String {
    // Split each radius from a single rounded raw integer (`raw = round(r * 100_000)`)
    // so a near-boundary value carries into the integer part exactly, matching
    // AltiumSharp's `AddCoordParam` — rather than the old `trunc`/`fract` split, which
    // clamped e.g. 4.999995 to `Radius=4|Radius_Frac=99999` instead of `Radius=5`.
    // Radii are non-negative, so `raw % 100_000` is non-negative.
    let radius_raw = (arc.radius * 100_000.0).round() as i64;
    let radius_int = (radius_raw / 100_000) as i32;
    let radius_frac = (radius_raw % 100_000) as u32;

    let secondary_raw = (arc.secondary_radius * 100_000.0).round() as i64;
    let secondary_radius_int = (secondary_raw / 100_000) as i32;
    let secondary_radius_frac = (secondary_raw % 100_000) as u32;

    format!(
        "|RECORD=11|IndexInSheet={}|OwnerPartId={}|IsNotAccesible=T\
         |Location.X={}|Location.Y={}\
         |Radius={}{}\
         |SecondaryRadius={}{}\
         |StartAngle={}|EndAngle={}\
         |LineWidth={}{}{}|UniqueID={}",
        index,
        arc.owner_part_id,
        arc.x,
        arc.y,
        radius_int,
        nonzero("Radius_Frac", radius_frac),
        secondary_radius_int,
        nonzero("SecondaryRadius_Frac", secondary_radius_frac),
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
        "|RECORD=4|IndexInSheet={}|OwnerPartId={}|IsNotAccesible=T|Location.X={}|Location.Y={}{}|FontID={}|Orientation={}|Justification={}{}{}|Text={}|UniqueID={}",
        index,
        label.owner_part_id,
        label.x,
        label.y,
        nonzero("Color", label.color),
        label.font_id,
        orientation,
        justification,
        is_mirrored,
        is_hidden,
        label.text,
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
        "|RECORD=4|IndexInSheet={}|OwnerPartId={}|IsNotAccesible=T|Location.X={}|Location.Y={}{}|FontID={}|Orientation={}|Justification={}{}{}|Text={}|UniqueID={}",
        index,
        text.owner_part_id,
        text.x,
        text.y,
        nonzero("Color", text.color),
        text.font_id,
        orientation,
        justification,
        is_mirrored,
        is_hidden,
        text.text,
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
    }

    #[test]
    fn test_label_booleans_only_when_true() {
        let mut label = Label {
            x: 0,
            y: 0,
            text: "R".to_string(),
            font_id: 1,
            color: 0,
            justification: TextJustification::BottomLeft,
            rotation: 0.0,
            is_mirrored: false,
            is_hidden: false,
            owner_part_id: 1,
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
            x: 0,
            y: 0,
            radius: 10,
            is_not_accessible: true,
            start_angle: 0.0,
            end_angle: 360.0,
            line_width: 1,
            color: 0,
            fill_color: 0,
            owner_part_id: 1,
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
            x: 0,
            y: 0,
            radius: 10,
            is_not_accessible: true,
            start_angle: 0.0,
            end_angle: 360.0,
            line_width: 1,
            color: 0,
            fill_color: 0,
            owner_part_id: 1,
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
                x: 0,
                y: 0,
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
}
