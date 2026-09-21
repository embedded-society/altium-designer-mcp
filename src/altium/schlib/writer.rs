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
    Arc, Bezier, Ellipse, EllipticalArc, FootprintModel, IeeeSymbol, Image, Label, Line, Parameter,
    Pie, Pin, Polygon, Polyline, Rectangle, RoundRect, ShapeDisplayFlags, TextFrame,
    TextJustification,
};
use super::{SchPrimitiveKind, Symbol};
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
    //
    // The name is the exception: Altium stores a non-ASCII pin name as its
    // UTF-8 bytes (every one of the golden's 52 such pins, `Résistance`
    // included though Windows-1252 could hold it) with the `PinWideText`
    // stream beside it, so the record and the stream agree on every reader.
    let name = if pin.name.is_ascii() {
        crate::altium::encode_windows1252(&pin.name)
    } else {
        pin.name.as_bytes().to_vec()
    };
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

/// Header keys that exist only as constants: emitted for a symbol built from
/// scratch (the scripted golden carries them) and otherwise only when the
/// header read them — a UI-authored header omits them.
const HEADER_CONSTANT_KEYS: &[&str] = &["LibraryPath", "SheetPartFileName"];

/// Replays a read header segment by segment (see [`encode_component_header`]).
///
/// A segment whose field was not edited goes back verbatim: its plain value
/// still names the same text once decoded the way the reader decoded it, or
/// its canonical rendering is unchanged. A `%UTF8%` twin follows its plain
/// key's verdict. An edited field is emitted in canonical form, its stale
/// twin dropped when the new value is ASCII.
fn replay_header(symbol: &Symbol, canonical: &[(String, String)]) -> Vec<String> {
    let canonical_value = |key: &str| {
        canonical
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    };
    let is_text_key = |key: &str| {
        ["LibReference", "ComponentDescription", "SourceLibraryName"]
            .iter()
            .any(|k| k.eq_ignore_ascii_case(key))
    };
    // The value the reader derived from a plain text key.
    let read_text =
        |raw: &str| crate::altium::from_wire_text(raw).unwrap_or_else(|| raw.to_string());
    let current_text = |key: &str| -> Option<&str> {
        if key.eq_ignore_ascii_case("LibReference") {
            Some(symbol.name.as_str())
        } else if key.eq_ignore_ascii_case("ComponentDescription") {
            Some(symbol.description.as_str())
        } else if key.eq_ignore_ascii_case("SourceLibraryName") {
            Some(symbol.source_library_name.as_str())
        } else {
            None
        }
    };
    let plain_unchanged = |key: &str| {
        symbol
            .header_params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .is_some_and(|(_, raw)| current_text(key) == Some(read_text(raw).as_str()))
    };

    let mut placed: Vec<bool> = vec![false; canonical.len()];
    let mut parts = Vec::with_capacity(symbol.header_params.len() + 4);
    for (key, raw) in &symbol.header_params {
        if key.is_empty() {
            parts.push(String::new()); // an empty segment of `%UTF8%Key=…|||Key=…`
            continue;
        }
        let plain_key = key.strip_prefix("%UTF8%").unwrap_or(key);
        let index = canonical
            .iter()
            .position(|(k, _)| k.eq_ignore_ascii_case(key));
        if let Some(i) = index {
            placed[i] = true;
        }
        let unchanged = if is_text_key(plain_key) {
            plain_unchanged(plain_key)
        } else {
            index.is_none_or(|i| canonical[i].1 == *raw)
        };
        if unchanged {
            parts.push(format!("{key}={raw}"));
        } else if let Some(value) = canonical_value(key) {
            parts.push(format!("{key}={value}"));
        }
        // An edited text key whose stale twin is in canonical no longer:
        // dropped with the segment (`canonical_value` found nothing).
    }
    for (i, (key, value)) in canonical.iter().enumerate() {
        if !placed[i] && !HEADER_CONSTANT_KEYS.contains(&key.as_str()) {
            parts.push(format!("{key}={value}"));
        }
    }
    parts
}

/// Keys whose value is positional — assigned by the writer from the record's
/// place in the stream — so the canonical value always wins and a stale read
/// value is never replayed.
const POSITIONAL_KEYS: &[&str] = &["IndexInSheet", "OwnerIndex"];

/// A `key=value` the UI omits when it holds this value and a script writes
/// out: left out when the read record lacked it and the value is still the
/// implicit one, so the file comes back as Altium wrote it; emitted once the
/// field is edited away from it.
const IMPLICIT_DEFAULTS: &[(&str, &str)] = &[("LineWidth", "1"), ("Description", "")];

/// Every key a content record's encoder can emit — the keys a struct field
/// stands behind. A read key in this set that the canonical form now omits
/// is a field edited to its omitted default (`IsHidden` cleared) and is
/// dropped; a read key outside it is an Altium key this crate does not model
/// and is replayed verbatim. Held complete against the golden by
/// `every_golden_record_key_is_modelled`.
const MODELLED_RECORD_KEYS: &[&str] = &[
    "RECORD",
    "Alignment",
    "AreaColor",
    "ClipToRect",
    "Color",
    "Corner.X",
    "Corner.X_Frac",
    "Corner.Y",
    "Corner.Y_Frac",
    "CornerXRadius",
    "CornerXRadius_Frac",
    "CornerYRadius",
    "CornerYRadius_Frac",
    "Description",
    "%UTF8%Description",
    "Dimmed",
    "Disabled",
    "EmbedImage",
    "EndAngle",
    "EndLineShape",
    "FileName",
    "%UTF8%FileName",
    "FontID",
    "GraphicallyLocked",
    "HideName",
    "IndexInSheet",
    "IsConfigurable",
    "IsHidden",
    "IsMirrored",
    "IsNotAccesible",
    "IsRule",
    "IsSolid",
    "IsSystemParameter",
    "Justification",
    "KeepAspect",
    "LineShapeSize",
    "LineStyle",
    "LineStyleExt",
    "LineWidth",
    "Location.X",
    "Location.X_Frac",
    "Location.Y",
    "Location.Y_Frac",
    "LocationCount",
    "Mirror",
    "Name",
    "%UTF8%Name",
    "NotAutoPosition",
    "Orientation",
    "OwnerIndex",
    "OwnerPartDisplayMode",
    "OwnerPartId",
    "ParamType",
    "Radius",
    "Radius_Frac",
    "ReadOnlyState",
    "ScaleFactor",
    "ScaleFactor_Frac",
    "SecondaryRadius",
    "SecondaryRadius_Frac",
    "ShowBorder",
    "ShowName",
    "StartAngle",
    "StartLineShape",
    "Symbol",
    "Text",
    "%UTF8%Text",
    "TextColor",
    "TextHorzAnchor",
    "TextMargin",
    "TextMargin_Frac",
    "TextVertAnchor",
    "Transparent",
    "UniqueID",
    "WordWrap",
    // RECORD=45, the footprint link
    "ModelName",
    "ModelType",
    "DatafileCount",
    "ModelDatafile0",
    "ModelDatafileEntity0",
    "ModelDatafileKind0",
    "IsCurrent",
];

/// Whether `key` is one an encoder can emit: a vertex key (`X3`, `Y12`) or
/// one of [`MODELLED_RECORD_KEYS`].
fn is_modelled_record_key(key: &str) -> bool {
    let vertex = key
        .strip_prefix('X')
        .or_else(|| key.strip_prefix('Y'))
        .is_some_and(|rest| {
            let rest = rest.strip_suffix("_Frac").unwrap_or(rest);
            !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
        });
    vertex
        || MODELLED_RECORD_KEYS
            .iter()
            .any(|k| k.eq_ignore_ascii_case(key))
}

/// Replays a record read from a file over the canonical encoding of its
/// current state (see `raw_params` on every record struct).
///
/// Segment by segment in the read order: a key the canonical form carries
/// goes back verbatim when the two values decode to the same text (the UI
/// stores a Latin-1 value as Windows-1252 where the canonical form uses
/// UTF-8 bytes) and as the canonical value otherwise — an edit, or a
/// positional key such as `IndexInSheet`. A key the canonical form lacks is
/// dropped when an encoder could have emitted it (the field was edited to
/// its omitted default — see [`MODELLED_RECORD_KEYS`]) and replayed verbatim
/// otherwise, as an Altium key this crate does not model. Canonical keys the
/// record lacked are appended, except an [`IMPLICIT_DEFAULTS`] value the
/// file left implicit and a `UniqueID` the file never gave the record, which
/// would be invented afresh on every save. A record without raw segments —
/// built from scratch — is the canonical form.
fn replay_record(canonical: &str, raw: &[(String, String)]) -> String {
    if raw.is_empty() {
        return canonical.to_string();
    }
    let canonical_pairs: Vec<(&str, &str)> = canonical
        .split('|')
        .skip(1)
        .map(|segment| segment.split_once('=').unwrap_or((segment, "")))
        .collect();
    let canonical_value = |key: &str| {
        canonical_pairs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
    };
    let decoded =
        |value: &str| crate::altium::from_wire_text(value).unwrap_or_else(|| value.to_string());
    let is_positional = |key: &str| POSITIONAL_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key));

    // A `%UTF8%` twin's bytes are a locale artefact of the writing machine
    // (the golden's were widened through Windows-1250), so whether it goes
    // back verbatim follows its plain key, not its own bytes.
    let raw_value = |key: &str| {
        raw.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    };
    let plain_unchanged = |key: &str| {
        let plain = key.strip_prefix("%UTF8%").unwrap_or(key);
        match (raw_value(plain), canonical_value(plain)) {
            (Some(read), Some(current)) => read == current || decoded(read) == decoded(current),
            _ => false,
        }
    };

    let mut placed: Vec<bool> = vec![false; canonical_pairs.len()];
    let mut parts: Vec<String> = Vec::with_capacity(raw.len() + 2);
    for (key, read) in raw {
        if key.is_empty() {
            parts.push(String::new());
            continue;
        }
        if let Some(i) = canonical_pairs
            .iter()
            .position(|(k, _)| k.eq_ignore_ascii_case(key))
        {
            placed[i] = true;
            let current = canonical_pairs[i].1;
            let unchanged = !is_positional(key)
                && (read == current
                    || decoded(read) == decoded(current)
                    || (key.starts_with("%UTF8%") && plain_unchanged(key)));
            let value = if unchanged { read.as_str() } else { current };
            parts.push(format!("{key}={value}"));
        } else if !is_positional(key) && !is_modelled_record_key(key) {
            parts.push(format!("{key}={read}"));
        }
    }
    for (i, (key, value)) in canonical_pairs.iter().enumerate() {
        let implicit = IMPLICIT_DEFAULTS
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case(key) && v == value);
        // An identity the file never gave the record (Altium stores a pie
        // without one) is not invented on its behalf: the canonical UniqueID
        // here would be freshly generated, different on every save.
        let invented_identity = key.eq_ignore_ascii_case("UniqueID");
        if !placed[i] && !implicit && !invented_identity {
            parts.push(format!("{key}={value}"));
        }
    }
    format!("|{}", parts.join("|"))
}

/// Whether a parameter is Altium's own system `Comment` record — stored after
/// the designator with the `IndexInSheet=-1` sentinel, outside the counter.
/// `OwnerPartId=-1` alone does not make one: the UI stores every user
/// parameter that way too, as a content record with its own slot.
fn is_system_parameter(param: &super::Parameter) -> bool {
    param.owner_part_id == -1 && param.name.eq_ignore_ascii_case("Comment")
}

/// Encodes a component header record.
///
/// A symbol read from a file replays its own header (`header_params`):
/// every segment as read, verbatim, unless the field behind it was edited,
/// in which case the canonical form of the new value takes its place. That
/// keeps whichever `%UTF8%` layout Altium used (the UI's
/// `%UTF8%Key=<UTF-8>|||Key=<Windows-1252>`, the scripted one's UTF-8 bytes
/// in both keys), the keys it omitted and the ones this crate does not
/// model. Modelled keys the record lacked are appended so a typed edit is
/// never lost, except the constant ones it omitted on purpose. A symbol
/// built from scratch emits the canonical header.
fn encode_component_header(symbol: &Symbol) -> String {
    let from_file = !symbol.header_params.is_empty();
    let text = |key: &str, value: &str| -> Vec<(String, String)> {
        if value.is_ascii() {
            vec![(key.to_string(), value.to_string())]
        } else {
            let bytes = crate::altium::encode_utf8_param_value(value);
            vec![
                (key.to_string(), bytes.clone()),
                (format!("%UTF8%{key}"), bytes),
            ]
        }
    };
    let part_id_locked = if symbol.part_id_locked { "T" } else { "F" };
    let mut canonical: Vec<(String, String)> = vec![("RECORD".to_string(), "1".to_string())];
    canonical.extend(text("LibReference", &symbol.name));
    canonical.extend(text("ComponentDescription", &symbol.description));
    canonical.extend([
        ("PartCount".to_string(), (symbol.part_count + 1).to_string()), // Altium uses part_count + 1
        (
            "DisplayModeCount".to_string(),
            symbol.display_mode_count.to_string(),
        ),
        ("IndexInSheet".to_string(), "-1".to_string()),
        ("OwnerPartId".to_string(), "-1".to_string()),
        (
            "CurrentPartId".to_string(),
            symbol.current_part_id.to_string(),
        ),
        ("LibraryPath".to_string(), "*".to_string()),
    ]);
    canonical.extend(text("SourceLibraryName", &symbol.source_library_name));
    canonical.extend([
        ("SheetPartFileName".to_string(), "*".to_string()),
        (
            "TargetFileName".to_string(),
            symbol.target_file_name.clone(),
        ),
    ]);
    // Altium keeps a stale count here, so a read value is carried; a
    // from-scratch symbol writes its pin count, omitted at zero like every
    // zero-valued integer key (the golden's pinless symbols carry none).
    #[allow(clippy::cast_possible_truncation)]
    let all_pin_count = symbol.all_pin_count.unwrap_or(symbol.pins.len() as u32);
    if all_pin_count != 0 || symbol.all_pin_count.is_some() {
        canonical.push(("AllPinCount".to_string(), all_pin_count.to_string()));
    }
    canonical.extend([
        ("AreaColor".to_string(), "11599871".to_string()), // Light yellow fill
        ("Color".to_string(), "128".to_string()),          // Dark red outline
        ("PartIDLocked".to_string(), part_id_locked.to_string()),
    ]);

    let parts: Vec<String> = if from_file {
        replay_header(symbol, &canonical)
    } else {
        canonical
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect()
    };

    // Leading pipe, NO trailing pipe (matches Altium's ParametersToString).
    format!("|{}", parts.join("|"))
}

/// Formats a text field as `<key>=<value>`, promoting it to `%UTF8%<key>` when
/// the value carries characters Windows-1252 cannot represent.
///
/// A pure-Windows-1252 value emits the plain `<key>=<value>` — byte-identical to
/// the pre-UTF-8 output, so the common case (and everything in the golden library)
/// is unchanged.
///
/// A non-ASCII value is written **twice**, as Altium does: the plain `<key>`
/// carrying the value's raw UTF-8 bytes, and a `%UTF8%<key>` companion. Altium
/// reads the plain key, so omitting it leaves the name `?`-mangled in Altium
/// even though our own reader recovers it; the companion is what `AltiumSharp`
/// and older readers look for. Both are the same bytes on the wire, since the
/// record is encoded as Windows-1252.
///
/// The gate is ASCII, not Windows-1252-representability: the golden stores
/// `Résistance` as its UTF-8 bytes with a twin, even though `é` has a
/// perfectly good single-byte form — AD promotes any non-ASCII value.
fn text_field(key: &str, value: &str) -> String {
    if value.is_ascii() {
        format!("{key}={value}")
    } else {
        let bytes = crate::altium::encode_utf8_param_value(value);
        format!("{key}={bytes}|%UTF8%{key}={bytes}")
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

/// Emits an Altium coordinate parameter: `|<key>=<int>` when the integer part
/// is non-zero, followed by `|<key>_Frac=<frac>` when the (signed) fractional
/// part is non-zero. AD24 omits **every** zero coordinate key (its
/// `AddCoordParam` writes each half only when non-zero): the LINES golden line
/// (0,0)→(10,0) carries only `Corner.X=10`, and the FRACSHAPES golden arc
/// stores centre 0.05 as `Location.X_Frac=5000` with no `Location.X` key. A
/// coordinate of exactly 0 therefore emits nothing; [`super::coord::read`]
/// defaults the absent keys back to 0 on read. See [`super::coord`] for the
/// toward-zero / signed-fraction split.
fn coord_param(key: &str, value: f64) -> String {
    use std::fmt::Write as _;
    let (int, frac) = coord::split(value);
    let mut out = String::new();
    if int != 0 {
        let _ = write!(out, "|{key}={int}");
    }
    if frac != 0 {
        let _ = write!(out, "|{key}_Frac={frac}");
    }
    out
}

/// Formats an angle the way AD24 does: three decimal places with a period
/// separator (the ARCS golden stores `EndAngle=360.000`, the PIESYM golden
/// `StartAngle=30.000`).
fn format_angle(angle: f64) -> String {
    format!("{angle:.3}")
}

/// Emits Altium's arc angle pair: `StartAngle` only when non-zero, `EndAngle`
/// always, both in the 3-decimal [`format_angle`] form — the ARCS golden
/// quarter arc carries only `EndAngle=90.000`.
fn angle_params(start_angle: f64, end_angle: f64) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    if start_angle != 0.0 {
        let _ = write!(out, "|StartAngle={}", format_angle(start_angle));
    }
    let _ = write!(out, "|EndAngle={}", format_angle(end_angle));
    out
}

/// Pushes a numbered polyline/polygon vertex (`X{n}`/`Y{n}`) into a `parts`
/// vector that is later joined by `|`. Mirrors [`coord_param`] for the
/// list-style records that build their text from `parts.join("|")`: every zero
/// key (integer or `_Frac`) is omitted, matching AD24's `AddSchVertex` — the
/// POLYLINES golden vertices carry no `X1`/`Y1`/`X3` keys for the zero halves.
fn push_point(parts: &mut Vec<String>, n: usize, x: f64, y: f64) {
    push_coord(parts, &format!("X{n}"), x);
    push_coord(parts, &format!("Y{n}"), y);
}

/// Pushes a named coordinate (`KEY=int` + `KEY_Frac=frac`, each only when
/// non-zero) into a `parts` vector joined by `|`. The list-style equivalent of
/// [`coord_param`], following the same AD24 rule: every zero coordinate key is
/// omitted (the reader defaults absent keys back to 0).
fn push_coord(parts: &mut Vec<String>, key: &str, value: f64) {
    let (int, frac) = coord::split(value);
    if int != 0 {
        parts.push(format!("{key}={int}"));
    }
    if frac != 0 {
        parts.push(format!("{key}_Frac={frac}"));
    }
}

/// Formats the `|IndexInSheet=<n>` token for a content record, or an empty
/// string for the first content record (index 0), which Altium omits.
///
/// Real AD24 numbers a symbol's content records (every graphic shape, every
/// user Label/Parameter record AND every binary pin) with ONE shared 0-based
/// counter in stream order, omitting the token at 0 — confirmed against both
/// the regenerated golden fixture (`scripts/samples/symbols.SchLib`) and real
/// Altium-authored libraries. Pins carry no text token (the binary pin record
/// has no `IndexInSheet` field) but still consume a counter slot: a real
/// Altium symbol with parameters 0..2, two pins, then a rectangle stores
/// `IndexInSheet=5` on the rectangle. The value is purely positional, so the
/// writer derives it; nothing is stored on the primitive structs. The token
/// sits immediately after `IsNotAccesible` (before `OwnerPartId`), matching
/// the golden token order `|RECORD=12|IsNotAccesible=T|IndexInSheet=1|…`.
fn index_in_sheet(index: usize) -> String {
    if index == 0 {
        String::new()
    } else {
        format!("|IndexInSheet={index}")
    }
}

/// Pushes the `IndexInSheet=<n>` token into a `parts` vector joined by `|`,
/// skipping index 0 (omitted by Altium). The list-style equivalent of
/// [`index_in_sheet`].
fn push_index_in_sheet(parts: &mut Vec<String>, index: usize) {
    if index != 0 {
        parts.push(format!("IndexInSheet={index}"));
    }
}

/// Emits the four universal display/lock flags as `|KEY=VALUE` tokens, each
/// only when non-default. Matching Altium's omit-when-default behaviour, a shape
/// carrying only defaults emits nothing here (so its record stays byte-identical
/// to pre-flag output). Bool flags emit `=T` when set; `OwnerPartDisplayMode`
/// emits its integer when non-zero.
///
/// The tokens sit immediately after `OwnerPartId`, matching the golden
/// (`…|OwnerPartId=1|OwnerPartDisplayMode=1|Location.X=…` on the DISPMODE
/// rectangle, `…|OwnerPartId=1|GraphicallyLocked=T|Location.X=…` on LOCKFLAGS)
/// and `AltiumSharp`'s `AddCommonProperties`, whose intra-flag order
/// (`OwnerPartDisplayMode` first) this mirrors.
fn write_display_flags(flags: ShapeDisplayFlags) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    if flags.owner_part_display_mode != 0 {
        let _ = write!(
            out,
            "|OwnerPartDisplayMode={}",
            flags.owner_part_display_mode
        );
    }
    if flags.graphically_locked {
        out.push_str("|GraphicallyLocked=T");
    }
    if flags.disabled {
        out.push_str("|Disabled=T");
    }
    if flags.dimmed {
        out.push_str("|Dimmed=T");
    }
    out
}

/// Pushes the universal display/lock flags into a `parts` vector that is later
/// joined by `|` (the list-style encoders: parameter, polyline, polygon). Each
/// key is pushed only when non-default, mirroring [`write_display_flags`]
/// (including its immediately-after-`OwnerPartId` placement and intra-flag
/// order).
fn push_display_flags(parts: &mut Vec<String>, flags: ShapeDisplayFlags) {
    if flags.owner_part_display_mode != 0 {
        parts.push(format!(
            "OwnerPartDisplayMode={}",
            flags.owner_part_display_mode
        ));
    }
    if flags.graphically_locked {
        parts.push("GraphicallyLocked=T".to_string());
    }
    if flags.disabled {
        parts.push("Disabled=T".to_string());
    }
    if flags.dimmed {
        parts.push("Dimmed=T".to_string());
    }
}

/// Encodes a rectangle record.
fn encode_rectangle(rect: &Rectangle, index: usize) -> String {
    // Altium emits IsSolid only when the shape is filled and Transparent only
    // when true (the RECTS golden's unfilled rectangle carries neither key —
    // never `Transparent=F`).
    let is_solid = if rect.filled { "|IsSolid=T" } else { "" };
    let transparent = if rect.transparent {
        "|Transparent=T"
    } else {
        ""
    };
    // Rectangles store the line style in LineStyleExt (Altium omits LineStyle),
    // omitted when zero and placed between Corner.Y and LineWidth — an
    // AD24-authored dashed rectangle stores
    // `Corner.Y|LineStyleExt|LineWidth|AreaColor`.
    let line_style = nonzero("LineStyleExt", u32::from(rect.line_style));
    format!(
        "|RECORD=14|IsNotAccesible=T{}|OwnerPartId={}{}\
         {}{}{}{}\
         {}|LineWidth={}{}{}{}{}|UniqueID={}",
        index_in_sheet(index),
        rect.owner_part_id,
        write_display_flags(rect.display_flags),
        coord_param("Location.X", rect.x1),
        coord_param("Location.Y", rect.y1),
        coord_param("Corner.X", rect.x2),
        coord_param("Corner.Y", rect.y2),
        line_style,
        rect.line_width,
        nonzero("Color", rect.line_color),
        nonzero("AreaColor", rect.fill_color),
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
    // A styled line carries BOTH keys (the SHAPESTYLE golden dashed line stores
    // `LineStyle=1|LineStyleExt=1`), each omitted when zero (Solid); the golden
    // order is LineWidth, LineStyle, [Color,] LineStyleExt.
    let line_style = nonzero("LineStyle", u32::from(line.line_style));
    let line_style_ext = nonzero("LineStyleExt", u32::from(line.line_style));
    format!(
        "|RECORD=13{}{}|OwnerPartId={}{}{}{}{}{}|LineWidth={}{}{}{}|UniqueID={}",
        not_accessible,
        index_in_sheet(index),
        line.owner_part_id,
        write_display_flags(line.display_flags),
        coord_param("Location.X", line.x1),
        coord_param("Location.Y", line.y1),
        coord_param("Corner.X", line.x2),
        coord_param("Corner.Y", line.y2),
        line.line_width,
        line_style,
        nonzero("Color", line.color),
        line_style_ext,
        line.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes a parameter record.
///
/// Follows Altium's conventions and golden-verified token order (`Location`,
/// `Orientation`, `Justification`, `Color`, `FontID`, `IsHidden`, `Text`,
/// `Name`, `ReadOnlyState`, … — the JUSTIFY golden and real Altium-authored
/// `CrossRef` parameters agree): `IsHidden` is emitted only when hidden (never
/// `=F`), `ReadOnlyState` / `ParamType` / `Orientation` / `Justification` /
/// `Color` only when non-zero, `ShowName` / `HideName` / `IsConfigurable` only
/// when set, `Text` / `Description` only when non-empty, and the read
/// `UniqueID` is preserved.
///
/// The **system** parameter — Altium's own `Comment` record, see
/// [`is_system_parameter`] — carries the `IndexInSheet=-1` sentinel (every
/// golden symbol's system Comment stores
/// `|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|`) and never a content-counter
/// slot; `index` is ignored for it. Every other parameter follows the shared
/// 0-based content counter like every other record, whatever its owner — the
/// UI stores user parameters with `OwnerPartId=-1` too.
fn encode_parameter(param: &Parameter, index: usize) -> String {
    let mut parts = vec!["RECORD=41".to_string()];
    // IndexInSheet (system sentinel or counter, 0 omitted) directly after
    // RECORD, then OwnerPartId (parameters carry no IsNotAccesible token).
    if is_system_parameter(param) {
        parts.push("IndexInSheet=-1".to_string());
    } else {
        push_index_in_sheet(&mut parts, index);
    }
    parts.push(format!("OwnerPartId={}", param.owner_part_id));
    push_display_flags(&mut parts, param.display_flags);
    // Coordinates with their `_Frac` companions adjacent, every zero key omitted.
    push_coord(&mut parts, "Location.X", param.x);
    push_coord(&mut parts, "Location.Y", param.y);
    // EE-meaningful display fields, each omit-when-default so a from-scratch
    // parameter stays byte-identical to Altium.
    if param.orientation != 0 {
        parts.push(format!("Orientation={}", param.orientation));
    }
    if param.justification != 0 {
        parts.push(format!("Justification={}", param.justification));
    }
    if param.color != 0 {
        parts.push(format!("Color={}", param.color));
    }
    parts.push(format!("FontID={}", param.font_id));
    if param.hidden {
        parts.push("IsHidden=T".to_string());
    }
    if !param.value.is_empty() {
        parts.push(text_field("Text", &param.value));
    }
    parts.push(text_field("Name", &param.name));
    if param.read_only_state != 0 {
        parts.push(format!("ReadOnlyState={}", param.read_only_state));
    }
    if param.param_type != 0 {
        parts.push(format!("ParamType={}", param.param_type));
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
    if !param.auto_position {
        parts.push("NotAutoPosition=T".to_string());
    }
    if param.is_rule {
        parts.push("IsRule=T".to_string());
    }
    if param.is_system_parameter {
        parts.push("IsSystemParameter=T".to_string());
    }
    if param.text_horz_anchor != 0 {
        parts.push(format!("TextHorzAnchor={}", param.text_horz_anchor));
    }
    if param.text_vert_anchor != 0 {
        parts.push(format!("TextVertAnchor={}", param.text_vert_anchor));
    }
    if !param.description.is_empty() {
        parts.push(text_field("Description", &param.description));
    }
    parts.push(format!(
        "UniqueID={}",
        param.unique_id.clone().unwrap_or_else(generate_unique_id)
    ));
    // After UniqueID, matching the golden: AD24 emits `IsMirrored` last on a
    // parameter record, though it emits it before UniqueID on a label.
    if param.is_mirrored {
        parts.push("IsMirrored=T".to_string());
    }
    format!("|{}", parts.join("|"))
}

/// Encodes the system designator record (`RECORD=34`).
///
/// Golden-verified form: `|RECORD=34|IndexInSheet=-1|OwnerPartId=-1
/// |Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=<designator>
/// |Name=Designator|ReadOnlyState=1|UniqueID=…`. The position comes from the
/// symbol's `designator_x`/`designator_y` (defaults −5/5 per the golden, each
/// zero key omitted per AD24's coordinate rule) and the read `UniqueID` is
/// reused so a read-modify-write is deterministic (a fresh one is generated
/// only when absent).
fn encode_designator(symbol: &Symbol) -> String {
    format!(
        "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1{}{}|Color=8388608|FontID=1|{}|Name=Designator|ReadOnlyState=1|UniqueID={}",
        coord_param("Location.X", symbol.designator_x),
        coord_param("Location.Y", symbol.designator_y),
        text_field("Text", &symbol.designator),
        symbol
            .designator_unique_id
            .clone()
            .unwrap_or_else(generate_unique_id)
    )
}

/// Encodes a polyline record.
///
/// Golden/`AltiumSharp` token order: `LineWidth`, then `LineStyle` /
/// `StartLineShape` / `EndLineShape` / `LineShapeSize` / `Color` (each only
/// when non-zero, matching the POLYLINES golden which carries none of them),
/// `Transparent` (only when true) before `LocationCount`, the vertices, and a
/// trailing `LineStyleExt` companion when styled (mirroring the styled-line
/// dual-key rule).
fn encode_polyline(polyline: &Polyline, index: usize) -> String {
    let mut parts = vec!["RECORD=6".to_string()];
    // Altium tags polylines IsNotAccesible (its own single-'s' spelling); emit
    // only when set (the golden polyline carries IsNotAccesible=T).
    if polyline.is_not_accessible {
        parts.push("IsNotAccesible=T".to_string());
    }
    push_index_in_sheet(&mut parts, index);
    parts.push(format!("OwnerPartId={}", polyline.owner_part_id));
    push_display_flags(&mut parts, polyline.display_flags);
    parts.push(format!("LineWidth={}", polyline.line_width));
    if polyline.line_style != 0 {
        parts.push(format!("LineStyle={}", polyline.line_style));
    }
    if polyline.start_line_shape != 0 {
        parts.push(format!("StartLineShape={}", polyline.start_line_shape));
    }
    if polyline.end_line_shape != 0 {
        parts.push(format!("EndLineShape={}", polyline.end_line_shape));
    }
    if polyline.line_shape_size != 0 {
        parts.push(format!("LineShapeSize={}", polyline.line_shape_size));
    }
    if polyline.color != 0 {
        parts.push(format!("Color={}", polyline.color));
    }
    // Altium emits Transparent only when true; absent means opaque.
    if polyline.transparent {
        parts.push("Transparent=T".to_string());
    }
    parts.push(format!("LocationCount={}", polyline.points.len()));

    for (i, (x, y)) in polyline.points.iter().enumerate() {
        push_point(&mut parts, i + 1, *x, *y);
    }

    // A styled polyline carries the LineStyleExt companion after the vertices
    // (AltiumSharp's golden-derived placement), omitted when Solid.
    if polyline.line_style != 0 {
        parts.push(format!("LineStyleExt={}", polyline.line_style));
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
    let mut parts = vec!["RECORD=7".to_string()];
    // Altium tags polygons IsNotAccesible (its own single-'s' spelling); emit
    // only when set, so a `false` polygon omits the key and round-trips as false.
    if polygon.is_not_accessible {
        parts.push("IsNotAccesible=T".to_string());
    }
    push_index_in_sheet(&mut parts, index);
    parts.push(format!("OwnerPartId={}", polygon.owner_part_id));
    push_display_flags(&mut parts, polygon.display_flags);
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
    // Altium emits IsSolid only when filled and Transparent only when true,
    // both BEFORE LocationCount (the SHAPESTYLE golden polygon stores
    // `…|IsSolid=T|Transparent=T|LocationCount=3|…`).
    if polygon.filled {
        parts.push("IsSolid=T".to_string());
    }
    if polygon.transparent {
        parts.push("Transparent=T".to_string());
    }
    parts.push(format!("LocationCount={}", polygon.points.len()));

    for (i, (x, y)) in polygon.points.iter().enumerate() {
        push_point(&mut parts, i + 1, *x, *y);
    }

    parts.push(format!(
        "UniqueID={}",
        polygon.unique_id.clone().unwrap_or_else(generate_unique_id)
    ));

    format!("|{}", parts.join("|"))
}

/// Encodes an arc record. Golden token order (the ARCS fixture): `LineWidth`
/// BEFORE the angles, `StartAngle` omitted when 0, `EndAngle` always in the
/// 3-decimal form (`EndAngle=360.000`).
fn encode_arc(arc: &Arc, index: usize) -> String {
    // Altium tags arcs IsNotAccesible (its own single-'s' spelling); emit only when set.
    let not_accessible = if arc.is_not_accessible {
        "|IsNotAccesible=T"
    } else {
        ""
    };
    format!(
        "|RECORD=12{}{}|OwnerPartId={}{}{}{}{}|LineWidth={}{}{}{}|UniqueID={}",
        not_accessible,
        index_in_sheet(index),
        arc.owner_part_id,
        write_display_flags(arc.display_flags),
        coord_param("Location.X", arc.x),
        coord_param("Location.Y", arc.y),
        coord_param("Radius", arc.radius),
        arc.line_width,
        angle_params(arc.start_angle, arc.end_angle),
        nonzero("Color", arc.color),
        nonzero("AreaColor", arc.fill_color),
        arc.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes a Bezier curve record. Zero control-point halves are omitted per
/// AD24's coordinate rule (the BEZIERSYM golden carries no `Y1`/`Y4` keys).
fn encode_bezier(bezier: &Bezier, index: usize) -> String {
    // Altium tags Beziers IsNotAccesible (its own single-'s' spelling); emit only when set.
    let not_accessible = if bezier.is_not_accessible {
        "|IsNotAccesible=T"
    } else {
        ""
    };
    format!(
        "|RECORD=5{}{}|OwnerPartId={}{}|LineWidth={}{}|LocationCount=4{}{}{}{}{}{}{}{}|UniqueID={}",
        not_accessible,
        index_in_sheet(index),
        bezier.owner_part_id,
        write_display_flags(bezier.display_flags),
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
    // Altium emits IsSolid only when filled, Transparent only when true. The
    // PIESYM golden orders LineWidth BEFORE the 3-decimal angles
    // (`…|Radius=5|LineWidth=1|StartAngle=30.000|EndAngle=210.000|AreaColor=…`).
    let is_solid = if pie.filled { "|IsSolid=T" } else { "" };
    let transparent = if pie.transparent {
        "|Transparent=T"
    } else {
        ""
    };
    format!(
        "|RECORD=9{}{}|OwnerPartId={}{}{}{}{}|LineWidth={}{}{}{}{}{}|UniqueID={}",
        not_accessible,
        index_in_sheet(index),
        pie.owner_part_id,
        write_display_flags(pie.display_flags),
        coord_param("Location.X", pie.x),
        coord_param("Location.Y", pie.y),
        coord_param("Radius", pie.radius),
        pie.line_width,
        angle_params(pie.start_angle, pie.end_angle),
        nonzero("Color", pie.line_color),
        nonzero("AreaColor", pie.fill_color),
        is_solid,
        transparent,
        pie.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes an image record (`RECORD=30`) — the picture metadata (bounding box,
/// border, fill, filename, flags). Embedded image bytes live in `/Storage` and
/// are not written here.
fn encode_image(image: &Image, index: usize) -> String {
    let mut parts = vec!["RECORD=30".to_string()];
    if image.is_not_accessible {
        parts.push("IsNotAccesible=T".to_string());
    }
    push_index_in_sheet(&mut parts, index);
    parts.push(format!("OwnerPartId={}", image.owner_part_id));
    push_display_flags(&mut parts, image.display_flags);
    // Bounding box: Location (corner 1) + Corner (corner 2), each half omitted
    // when zero per AD24's coordinate rule.
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
        parts.push(text_field("FileName", &image.file_name));
    }
    parts.push(format!(
        "UniqueID={}",
        image.unique_id.clone().unwrap_or_else(generate_unique_id)
    ));
    format!("|{}", parts.join("|"))
}

/// Encodes a text frame record (`RECORD=28`) — a bordered multi-line text box.
///
/// Token order and omit-when-default behaviour match Altium's own output (both
/// the regenerated golden and `AltiumSharp`'s golden-derived writer):
/// `IndexInSheet` follows the shared content counter like every other shape
/// (the golden frame carries no token because it is the symbol's first content
/// record — slot 0, which Altium omits); then `[LineWidth][Color][LineStyle]
/// AreaColor [TextColor] FontID [IsSolid] [ShowBorder] [Orientation]
/// [Alignment] [WordWrap] [ClipToRect] Text TextMargin[_Frac] [Transparent]`.
/// `AreaColor` and `FontID` are written unconditionally (Altium emits
/// `AreaColor=16777215|FontID=1` even on a from-scratch frame); the bracketed
/// keys are omitted when zero/false. `TextMargin` is a coordinate following
/// AD24's omit-every-zero-key rule (a default frame carries only
/// `TextMargin_Frac=5`).
fn encode_text_frame(frame: &TextFrame, index: usize) -> String {
    let mut parts = vec!["RECORD=28".to_string()];
    if frame.is_not_accessible {
        parts.push("IsNotAccesible=T".to_string());
    }
    push_index_in_sheet(&mut parts, index);
    parts.push(format!("OwnerPartId={}", frame.owner_part_id));
    push_display_flags(&mut parts, frame.display_flags);
    // Frame box: Location (corner 1) + Corner (corner 2), each half omitted
    // when zero per AD24's coordinate rule.
    push_coord(&mut parts, "Location.X", frame.x1);
    push_coord(&mut parts, "Location.Y", frame.y1);
    push_coord(&mut parts, "Corner.X", frame.x2);
    push_coord(&mut parts, "Corner.Y", frame.y2);
    if frame.line_width != 0 {
        parts.push(format!("LineWidth={}", frame.line_width));
    }
    if frame.color != 0 {
        parts.push(format!("Color={}", frame.color));
    }
    if frame.line_style != 0 {
        parts.push(format!("LineStyle={}", frame.line_style));
    }
    parts.push(format!("AreaColor={}", frame.area_color));
    if frame.text_color != 0 {
        parts.push(format!("TextColor={}", frame.text_color));
    }
    parts.push(format!("FontID={}", frame.font_id));
    if frame.is_solid {
        parts.push("IsSolid=T".to_string());
    }
    if frame.show_border {
        parts.push("ShowBorder=T".to_string());
    }
    if frame.orientation != 0 {
        parts.push(format!("Orientation={}", frame.orientation));
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
    // Text is always written (with %UTF8% promotion, like Label/Text).
    parts.push(text_field("Text", &frame.text));
    push_coord(&mut parts, "TextMargin", frame.text_margin);
    if frame.transparent {
        parts.push("Transparent=T".to_string());
    }
    parts.push(format!(
        "UniqueID={}",
        frame.unique_id.clone().unwrap_or_else(generate_unique_id)
    ));
    format!("|{}", parts.join("|"))
}

fn encode_ellipse(ellipse: &Ellipse, index: usize) -> String {
    // Altium tags ellipses IsNotAccesible (its own single-'s' spelling); emit
    // only when set (the ELLIPSES golden carries IsNotAccesible=T).
    let not_accessible = if ellipse.is_not_accessible {
        "|IsNotAccesible=T"
    } else {
        ""
    };
    // Altium emits IsSolid only when filled, and omits it otherwise.
    let is_solid = if ellipse.filled { "|IsSolid=T" } else { "" };
    // Altium emits Transparent only when true; absent means opaque.
    let transparent = if ellipse.transparent {
        "|Transparent=T"
    } else {
        ""
    };
    format!(
        "|RECORD=8{}{}|OwnerPartId={}{}{}{}{}{}|LineWidth={}{}{}{}{}|UniqueID={}",
        not_accessible,
        index_in_sheet(index),
        ellipse.owner_part_id,
        write_display_flags(ellipse.display_flags),
        coord_param("Location.X", ellipse.x),
        coord_param("Location.Y", ellipse.y),
        coord_param("Radius", ellipse.radius_x),
        coord_param("SecondaryRadius", ellipse.radius_y),
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
    // LineStyle sits between LineWidth and Color (AltiumSharp's golden-derived
    // order), omitted when zero (Solid).
    let line_style = nonzero("LineStyle", u32::from(round_rect.line_style));
    // Altium emits Transparent only when true; absent means opaque.
    let transparent = if round_rect.transparent {
        "|Transparent=T"
    } else {
        ""
    };
    format!(
        "|RECORD=10|IsNotAccesible=T{}|OwnerPartId={}{}\
         {}{}{}{}\
         {}{}\
         |LineWidth={}{}{}{}{}{}|UniqueID={}",
        index_in_sheet(index),
        round_rect.owner_part_id,
        write_display_flags(round_rect.display_flags),
        coord_param("Location.X", round_rect.x1),
        coord_param("Location.Y", round_rect.y1),
        coord_param("Corner.X", round_rect.x2),
        coord_param("Corner.Y", round_rect.y2),
        coord_param("CornerXRadius", round_rect.corner_x_radius),
        coord_param("CornerYRadius", round_rect.corner_y_radius),
        round_rect.line_width,
        line_style,
        nonzero("Color", round_rect.line_color),
        nonzero("AreaColor", round_rect.fill_color),
        is_solid,
        transparent,
        round_rect
            .unique_id
            .clone()
            .unwrap_or_else(generate_unique_id)
    )
}

/// Encodes an elliptical arc record. Like [`encode_arc`], `LineWidth` precedes
/// the 3-decimal angles and a zero `StartAngle` is omitted.
fn encode_elliptical_arc(arc: &EllipticalArc, index: usize) -> String {
    // Each radius splits into an integer part plus a signed `_Frac` companion
    // (scaled by 100,000), carrying near-boundary values into the integer part.
    // See [`super::coord`] for the shared encoding.
    format!(
        "|RECORD=11|IsNotAccesible=T{}|OwnerPartId={}{}\
         {}{}\
         {}\
         {}\
         |LineWidth={}{}{}{}|UniqueID={}",
        index_in_sheet(index),
        arc.owner_part_id,
        write_display_flags(arc.display_flags),
        coord_param("Location.X", arc.x),
        coord_param("Location.Y", arc.y),
        coord_param("Radius", arc.radius),
        coord_param("SecondaryRadius", arc.secondary_radius),
        arc.line_width,
        angle_params(arc.start_angle, arc.end_angle),
        nonzero("Color", arc.color),
        nonzero("AreaColor", arc.fill_color),
        arc.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes a label record. Golden token order (the LABELS / JUSTIFY fixtures):
/// `Orientation` and `Justification` sit between the coordinates and
/// `Color`/`FontID`, each omitted when zero
/// (`…|Location.X=-10|Justification=8|FontID=1|Text=TR|…`).
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
    #[allow(clippy::cast_sign_loss)] // orientation is %4-bounded, non-negative
    let orientation_token = nonzero("Orientation", orientation.rem_euclid(4) as u32);
    format!(
        "|RECORD=4|IsNotAccesible=T{}|OwnerPartId={}{}{}{}{}{}{}|FontID={}|{}{}{}|UniqueID={}",
        index_in_sheet(index),
        label.owner_part_id,
        write_display_flags(label.display_flags),
        coord_param("Location.X", label.x),
        coord_param("Location.Y", label.y),
        orientation_token,
        nonzero("Justification", u32::from(justification)),
        nonzero("Color", label.color),
        label.font_id,
        text_field("Text", &label.text),
        is_hidden,
        is_mirrored,
        label.unique_id.clone().unwrap_or_else(generate_unique_id)
    )
}

/// Encodes an IEEE symbol record (`RECORD=3`). Key order as the `IEEESYM`
/// golden stores it: `Symbol`, the location, `ScaleFactor`, `Orientation`,
/// `LineWidth`, `Mirror`, `Color` — and no `UniqueID`, which Altium never
/// gives this record.
fn encode_ieee_symbol(symbol: &IeeeSymbol, index: usize) -> String {
    #[allow(clippy::cast_possible_truncation)]
    let orientation = ((symbol.rotation / 90.0).round() as i32).rem_euclid(4);
    #[allow(clippy::cast_sign_loss)] // rem_euclid(4) is non-negative
    let orientation_token = nonzero("Orientation", orientation as u32);
    let mirror = if symbol.is_mirrored { "|Mirror=T" } else { "" };
    format!(
        "|RECORD=3|IsNotAccesible=T{}|OwnerPartId={}{}|Symbol={}{}{}{}{}|LineWidth={}{}{}",
        index_in_sheet(index),
        symbol.owner_part_id,
        write_display_flags(symbol.display_flags),
        symbol.symbol,
        coord_param("Location.X", symbol.x),
        coord_param("Location.Y", symbol.y),
        coord_param("ScaleFactor", symbol.scale_factor),
        orientation_token,
        symbol.line_width,
        mirror,
        nonzero("Color", symbol.color),
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
/// `is_current` marks the default footprint (`IsCurrent=T` on that model; a
/// UI-authored library omits the key on every other one, like any false
/// boolean, rather than writing `IsCurrent=F`).
///
/// `DatafileCount=1` plus `ModelDatafileEntity0` is what lets Altium *resolve*
/// the model to an actual footprint in a `PcbLib` (rendering the preview and
/// finding it on placement); a name-only record with `DatafileCount=0` shows in
/// the list but reports "model not found".
fn encode_footprint_model(model: &FootprintModel, owner_index: usize, is_current: bool) -> String {
    // The datafile group — DatafileCount=1, the optional ModelDatafile0 path,
    // ModelDatafileEntity0 and ModelDatafileKind0 — is what lets Altium resolve
    // the footprint; a link built from scratch gets it. Altium itself omits the
    // whole group on a name-only link (the IMPLCHAIN golden), so a read link
    // keeps the group only when it had one or a path was given since.
    let linked = model.library_path.is_some()
        || model.raw_params.is_empty()
        || model
            .raw_params
            .iter()
            .any(|(key, _)| key.eq_ignore_ascii_case("DatafileCount"));
    let datafiles = if linked {
        let path = model
            .library_path
            .as_deref()
            .map(|p| format!("|ModelDatafile0={p}"))
            .unwrap_or_default();
        format!(
            "|DatafileCount=1{path}|ModelDatafileEntity0={}|ModelDatafileKind0=PCBLib",
            model.name
        )
    } else {
        String::new()
    };
    // The read UniqueID is re-emitted verbatim (deterministic RMW); only a
    // from-scratch model gets a fresh one.
    let unique_id = model.unique_id.clone().unwrap_or_else(generate_unique_id);
    format!(
        "|RECORD=45|OwnerIndex={}|IndexInSheet=-1|Description={}|ModelName={}|ModelType=PCBLIB{}{}|UniqueID={}",
        owner_index,
        model.description,
        model.name,
        datafiles,
        if is_current { "|IsCurrent=T" } else { "" },
        unique_id
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
#[allow(clippy::too_many_lines)] // one straight-line emission step per primitive family
pub fn encode_data_stream(symbol: &Symbol) -> crate::altium::error::AltiumResult<Vec<u8>> {
    let mut data = Vec::new();
    // The shared IndexInSheet counter over content records (shapes, user
    // labels/parameters AND pins) in stream order; slot 0's token is omitted
    // and the header, system designator AND system parameters
    // (owner_part_id == -1) stay at the -1 sentinel without consuming a slot.
    // See [`index_in_sheet`] and [`encode_parameter`] for the golden-confirmed
    // rules.
    let mut index_counter = 0usize;

    // 1. Component header
    let header = encode_component_header(symbol);
    write_text_record(&mut data, &header)?;

    // 3. Content records, in the symbol's own order — Altium's authoring
    //    order for anything read from a file (see `Symbol::primitive_order`)
    //    and the canonical kind order otherwise, which leads with the
    //    rectangles so a solid-filled body sits behind the pins.
    //
    //    System parameters (owner_part_id == -1, the Altium-authored
    //    Comment-class records) are skipped here: they belong after the
    //    designator and carry the IndexInSheet=-1 sentinel instead of a
    //    counter slot.
    for (kind, index) in symbol.write_sequence() {
        let record = match kind {
            SchPrimitiveKind::Pin => {
                // A binary pin record has no IndexInSheet field of its own but
                // still consumes a counter slot — confirmed against real
                // Altium-authored libraries, where a symbol with parameters
                // 0..2, two pins, then a rectangle stores IndexInSheet=5 on the
                // rectangle.
                write_binary_pin(&mut data, &symbol.pins[index])?;
                index_counter += 1;
                continue;
            }
            SchPrimitiveKind::Parameter => {
                let param = &symbol.parameters[index];
                if is_system_parameter(param) {
                    continue;
                }
                replay_record(&encode_parameter(param, index_counter), &param.raw_params)
            }
            SchPrimitiveKind::Rectangle => replay_record(
                &encode_rectangle(&symbol.rectangles[index], index_counter),
                &symbol.rectangles[index].raw_params,
            ),
            SchPrimitiveKind::Line => replay_record(
                &encode_line(&symbol.lines[index], index_counter),
                &symbol.lines[index].raw_params,
            ),
            SchPrimitiveKind::Polyline => replay_record(
                &encode_polyline(&symbol.polylines[index], index_counter),
                &symbol.polylines[index].raw_params,
            ),
            SchPrimitiveKind::Polygon => replay_record(
                &encode_polygon(&symbol.polygons[index], index_counter),
                &symbol.polygons[index].raw_params,
            ),
            SchPrimitiveKind::Arc => replay_record(
                &encode_arc(&symbol.arcs[index], index_counter),
                &symbol.arcs[index].raw_params,
            ),
            SchPrimitiveKind::Pie => replay_record(
                &encode_pie(&symbol.pies[index], index_counter),
                &symbol.pies[index].raw_params,
            ),
            SchPrimitiveKind::Image => replay_record(
                &encode_image(&symbol.images[index], index_counter),
                &symbol.images[index].raw_params,
            ),
            SchPrimitiveKind::TextFrame => replay_record(
                &encode_text_frame(&symbol.text_frames[index], index_counter),
                &symbol.text_frames[index].raw_params,
            ),
            SchPrimitiveKind::Bezier => replay_record(
                &encode_bezier(&symbol.beziers[index], index_counter),
                &symbol.beziers[index].raw_params,
            ),
            SchPrimitiveKind::Ellipse => replay_record(
                &encode_ellipse(&symbol.ellipses[index], index_counter),
                &symbol.ellipses[index].raw_params,
            ),
            SchPrimitiveKind::RoundRect => replay_record(
                &encode_round_rect(&symbol.round_rects[index], index_counter),
                &symbol.round_rects[index].raw_params,
            ),
            SchPrimitiveKind::EllipticalArc => replay_record(
                &encode_elliptical_arc(&symbol.elliptical_arcs[index], index_counter),
                &symbol.elliptical_arcs[index].raw_params,
            ),
            SchPrimitiveKind::Label => replay_record(
                &encode_label(&symbol.labels[index], index_counter),
                &symbol.labels[index].raw_params,
            ),
            SchPrimitiveKind::IeeeSymbol => replay_record(
                &encode_ieee_symbol(&symbol.ieee_symbols[index], index_counter),
                &symbol.ieee_symbols[index].raw_params,
            ),
        };
        write_text_record(&mut data, &record)?;
        index_counter += 1;
    }

    // 15. Designator (system record, IndexInSheet=-1 — no counter slot).
    if !symbol.designator.is_empty() {
        let record = encode_designator(symbol);
        write_text_record(&mut data, &record)?;
    }

    // 15b. The SYSTEM parameter — Altium's own Comment record: golden order
    // puts it after the designator, and it carries the IndexInSheet=-1
    // sentinel WITHOUT consuming a counter slot (the golden DISPMODE system
    // Comment stores `|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|` while the
    // rectangles keep slots 0 and 1). Regressing it onto the counter destroyed
    // the -1 and shifted every later content index by one on
    // read-modify-write. Only the Comment is system: a UI-authored Value or
    // Part Number carries OwnerPartId=-1 too and is still a content record
    // with a counter slot, stored before the graphics in authoring order.
    for param in symbol.parameters.iter().filter(|p| is_system_parameter(p)) {
        let record = replay_record(&encode_parameter(param, 0), &param.raw_params);
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
    // IsCurrent is the model's own read state, not its list position: a symbol
    // whose current footprint is the second model must keep it there through a
    // read-modify-write. Only when NO model claims currency (a from-scratch
    // list, where the field defaults false) does the first model take it, which
    // is what Altium's own editor does on the first assignment.
    let has_current = symbol.footprints.iter().any(|m| m.is_current);
    for (i, model) in symbol.footprints.iter().enumerate() {
        // The RECORD=45 is owned by the RECORD=44; its RECORD=46/48 children are
        // in turn owned by the RECORD=45 (its own stream-index).
        let model_index = count_records(&data);
        write_text_record(
            &mut data,
            &replay_record(
                &encode_footprint_model(
                    model,
                    impl_index,
                    model.is_current || (!has_current && i == 0),
                ),
                &model.raw_params,
            ),
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
/// * `unique_id` - The library's own identity, kept across saves
#[must_use]
pub fn encode_file_header(symbols: &[&Symbol], ole_names: &[String], unique_id: &str) -> Vec<u8> {
    let mut parts = vec![
        "HEADER=Protel for Windows - Schematic Library Editor Binary File Version 5.0".to_string(),
        "Weight=47".to_string(),
        "MinorVersion=9".to_string(),
        format!("UniqueID={unique_id}"),
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

    // LibRef{i} is the component's REAL name — the golden stores the full
    // 33-byte Khmer name here while its storage name is cut at 31 — with a
    // %UTF8% twin when the name leaves Windows-1252. The root SectionKeys
    // stream, not this list, is what maps a truncated storage name back.
    // `ole_names` still decides which entries need that map; it is unused here
    // beyond keeping the two lists in lockstep by construction.
    debug_assert_eq!(symbols.len(), ole_names.len());
    for (i, symbol) in symbols.iter().enumerate() {
        parts.push(text_field(&format!("LibRef{i}"), &symbol.name));
        parts.push(text_field(&format!("CompDescr{i}"), &symbol.description));
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

    // ==================== encoder rejections and flag arms ==================
    //
    // The writer refuses anything the on-disk record cannot hold. Every one of
    // these limits is a silent-corruption guard: the field it protects is
    // length-prefixed or fixed-width, so an oversize value would not fail at
    // the write, it would truncate and desync every record after it.

    mod rejections {
        use super::*;

        /// A pin that encodes cleanly, so a test can push one field past its
        /// limit and leave the rest valid.
        fn pin() -> Pin {
            Pin::new("NAME", "1", 0, 0, 10, PinOrientation::Left)
        }

        /// Asserts the encode failed and named `field`.
        fn rejects(result: crate::altium::error::AltiumResult<()>, field: &str) {
            let err = result.expect_err("expected the encoder to refuse this pin");
            let message = err.to_string();
            assert!(
                message.contains(field),
                "expected the error to name {field:?}, got: {message}"
            );
        }

        #[test]
        fn a_record_larger_than_the_header_can_describe_is_refused() {
            // The record length is a 24-bit field. A u32 cast would truncate it
            // and every following record would be read from the wrong offset,
            // so the whole library after this point would decode as garbage.
            let mut data = Vec::new();
            let oversize = vec![0_u8; 0x0100_0000];
            let err = write_record_frame(&mut data, &oversize, 0)
                .expect_err("a 16 MiB record must be refused");
            assert!(err.to_string().contains("16 MiB"), "{err}");
            assert!(data.is_empty(), "nothing should be written on refusal");

            // One byte under the limit still writes, header included.
            let mut ok = Vec::new();
            let at_limit = vec![0_u8; 0x00FF_FFFF];
            write_record_frame(&mut ok, &at_limit, 0).expect("the limit itself is writable");
            assert_eq!(ok.len(), at_limit.len() + 4);
        }

        #[test]
        fn pin_coordinates_past_the_i16_range_are_refused_by_name() {
            // Coordinates are stored as i16. A cast would wrap a far-off-sheet
            // pin round to the opposite side of the symbol rather than failing.
            for (field, mutate) in [
                ("pin.x", (|p: &mut Pin| p.x = 40_000) as fn(&mut Pin)),
                ("pin.y", |p: &mut Pin| p.y = -40_000),
                ("pin.length", |p: &mut Pin| p.length = 40_000),
                ("pin.owner_part_id", |p: &mut Pin| {
                    p.owner_part_id = 40_000;
                }),
            ] {
                let mut subject = pin();
                mutate(&mut subject);
                rejects(write_binary_pin(&mut Vec::new(), &subject), field);
            }
        }

        #[test]
        fn pin_strings_longer_than_the_length_prefix_are_refused_by_field() {
            // Each string is a Pascal short string: one byte of length. Over
            // 255 the prefix wraps and the reader takes the wrong byte count,
            // desyncing the rest of the record.
            let long = "x".repeat(256);
            for (field, mutate) in [
                (
                    "pin.name",
                    (|p: &mut Pin, s: String| p.name = s) as fn(&mut Pin, String),
                ),
                ("pin.designator", |p: &mut Pin, s: String| p.designator = s),
                ("pin.description", |p: &mut Pin, s: String| {
                    p.description = s;
                }),
                ("pin.swap_id_group", |p: &mut Pin, s: String| {
                    p.swap_id_group = s;
                }),
                ("pin.part_and_sequence", |p: &mut Pin, s: String| {
                    p.part_and_sequence = s;
                }),
                ("pin.default_value", |p: &mut Pin, s: String| {
                    p.default_value = s;
                }),
            ] {
                let mut subject = pin();
                mutate(&mut subject, long.clone());
                rejects(write_binary_pin(&mut Vec::new(), &subject), field);
            }
        }

        #[test]
        fn the_limit_is_encoded_bytes_not_characters() {
            // The description is Windows-1252 (single-byte), so 255 non-ASCII
            // characters fit where a UTF-8 length check would have wrongly
            // rejected them.
            let mut subject = pin();
            subject.description = "\u{b5}".repeat(255); // 255 x MICRO SIGN
            write_binary_pin(&mut Vec::new(), &subject)
                .expect("255 encoded bytes is within the prefix");
            subject.description = "\u{b5}".repeat(256);
            rejects(
                write_binary_pin(&mut Vec::new(), &subject),
                "pin.description",
            );

            // The name is stored as UTF-8 bytes once it leaves ASCII, as
            // Altium stores it, so its limit is the UTF-8 length: 127 micro
            // signs are 254 bytes, 128 are 256.
            let mut subject = pin();
            subject.name = "\u{b5}".repeat(127);
            write_binary_pin(&mut Vec::new(), &subject).expect("254 UTF-8 bytes fit");
            subject.name = "\u{b5}".repeat(128);
            rejects(write_binary_pin(&mut Vec::new(), &subject), "pin.name");
        }

        /// A non-ASCII pin name is stored as its UTF-8 bytes — every one of
        /// the golden's 52 such pins, `Résistance` included — never the code
        /// page; an ASCII name stays single-byte.
        #[test]
        fn a_non_ascii_pin_name_is_stored_as_utf8_bytes() {
            let mut subject = pin();
            subject.name = "R\u{e9}sistance".to_string();
            let mut data = Vec::new();
            write_binary_pin(&mut data, &subject).unwrap();
            let needle = b"\x0bR\xc3\xa9sistance";
            assert!(
                data.windows(needle.len()).any(|w| w == needle),
                "UTF-8 bytes with an 11-byte length prefix: {data:?}"
            );
        }

        #[test]
        fn display_flag_keys_are_pushed_only_when_they_differ_from_the_default() {
            // These ride in a `|`-joined list, so a key emitted at its default
            // value is not harmless: it changes the bytes of every shape that
            // carries it and breaks byte-identity against Altium's own output.
            let mut parts = Vec::new();
            push_display_flags(&mut parts, ShapeDisplayFlags::default());
            assert!(parts.is_empty(), "defaults emit nothing: {parts:?}");

            let flags = ShapeDisplayFlags {
                owner_part_display_mode: 1,
                graphically_locked: true,
                disabled: true,
                dimmed: true,
            };
            let mut parts = Vec::new();
            push_display_flags(&mut parts, flags);
            assert_eq!(
                parts,
                vec![
                    "OwnerPartDisplayMode=1",
                    "GraphicallyLocked=T",
                    "Disabled=T",
                    "Dimmed=T",
                ]
            );
        }

        #[test]
        fn every_justification_maps_to_its_own_altium_id() {
            // Nine anchors, nine ids: a duplicated arm would silently re-anchor
            // a label to a different corner on save.
            let ids: Vec<u8> = [
                TextJustification::BottomLeft,
                TextJustification::BottomCenter,
                TextJustification::BottomRight,
                TextJustification::MiddleLeft,
                TextJustification::MiddleCenter,
                TextJustification::MiddleRight,
                TextJustification::TopLeft,
                TextJustification::TopCenter,
                TextJustification::TopRight,
            ]
            .into_iter()
            .map(justification_to_id)
            .collect();
            assert_eq!(ids, (0..=8).collect::<Vec<u8>>());
        }

        #[test]
        fn pin_display_flags_ride_in_their_own_bits() {
            // The flag byte packs four independent booleans; a shared or
            // swapped bit would turn "locked" into "hidden" on the next save.
            let encoded = |mutate: fn(&mut Pin)| {
                let mut subject = pin();
                mutate(&mut subject);
                let mut data = Vec::new();
                write_binary_pin(&mut data, &subject).expect("pin should encode");
                data
            };

            let plain = encoded(|_| {});
            for mutate in [
                (|p: &mut Pin| p.graphically_locked = true) as fn(&mut Pin),
                |p: &mut Pin| p.is_not_accessible = true,
            ] {
                assert_ne!(
                    encoded(mutate),
                    plain,
                    "a display flag left no trace in the record"
                );
            }
        }
    }

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
        let s = encode_text_frame(&frame, 0);
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

    /// Splits an encoded Data stream into per-record text (binary pin records
    /// come back as `"<PIN>"` markers) for token-order assertions.
    fn stream_records(data: &[u8]) -> Vec<String> {
        let mut records = Vec::new();
        let mut off = 0;
        while off + 4 <= data.len() {
            let len = data[off] as usize
                | ((data[off + 1] as usize) << 8)
                | ((data[off + 2] as usize) << 16);
            let flags = data[off + 3];
            if flags == 1 {
                records.push("<PIN>".to_string());
            } else {
                records.push(
                    String::from_utf8_lossy(&data[off + 4..off + 4 + len])
                        .trim_end_matches('\0')
                        .to_string(),
                );
            }
            off += 4 + len;
        }
        records
    }

    #[test]
    fn index_in_sheet_golden_sequence_across_shapes() {
        // The golden rule (LINES / SHAPESTYLE fixtures): all content records
        // share ONE 0-based counter in stream order, slot 0's token is omitted,
        // and the token sits right after IsNotAccesible (before OwnerPartId).
        let mut symbol = Symbol::new("SEQ");
        symbol.add_line(Line::new(-10, 0, 10, 0));
        symbol.add_line(Line::new(0, -10, 0, 10));
        symbol.add_line(Line::new(-10, -10, 10, 10));

        let data = encode_data_stream(&symbol).expect("encode");
        let records = stream_records(&data);
        let lines: Vec<&String> = records
            .iter()
            .filter(|t| t.starts_with("|RECORD=13"))
            .collect();
        assert_eq!(lines.len(), 3);
        assert!(
            !lines[0].contains("IndexInSheet"),
            "first content record (slot 0) omits the token: {}",
            lines[0]
        );
        assert!(
            lines[1].starts_with("|RECORD=13|IsNotAccesible=T|IndexInSheet=1|OwnerPartId=1|"),
            "second record carries =1 right after IsNotAccesible: {}",
            lines[1]
        );
        assert!(
            lines[2].starts_with("|RECORD=13|IsNotAccesible=T|IndexInSheet=2|OwnerPartId=1|"),
            "third record carries =2: {}",
            lines[2]
        );
    }

    #[test]
    fn index_in_sheet_pins_consume_counter_slots() {
        // Golden-confirmed against real Altium-authored libraries: binary pins
        // store no IndexInSheet token but DO consume counter slots (a real
        // symbol with parameters 0..2, two pins, then a rectangle stores
        // IndexInSheet=5 on the rectangle). The records go out in the order
        // they were added — parameter (slot 0, token omitted), rectangle (1),
        // two pins (2, 3), line (4) — because `add_*` records the order, the
        // same way reading a symbol records the file's.
        let mut symbol = Symbol::new("PINSLOTS");
        symbol.add_parameter(Parameter::new("Value", "10k"));
        symbol.add_rectangle(Rectangle::new(-5, -5, 5, 5));
        symbol.add_pin(Pin::new("A", "1", -10, 0, 5, PinOrientation::Left));
        symbol.add_pin(Pin::new("B", "2", 10, 0, 5, PinOrientation::Right));
        symbol.add_line(Line::new(-5, 0, 5, 0));

        let data = encode_data_stream(&symbol).expect("encode");
        let records = stream_records(&data);
        let param = records
            .iter()
            .find(|t| t.starts_with("|RECORD=41") && !t.contains("OwnerPartId=-1"))
            .expect("user parameter present");
        assert!(
            !param.contains("IndexInSheet"),
            "the parameter was added first, so it holds slot 0: {param}"
        );
        let rect = records
            .iter()
            .find(|t| t.starts_with("|RECORD=14"))
            .expect("rectangle present");
        assert!(
            rect.contains("|IndexInSheet=1|"),
            "rectangle takes slot 1: {rect}"
        );
        let line = records
            .iter()
            .find(|t| t.starts_with("|RECORD=13"))
            .expect("line present");
        assert!(
            line.contains("|IndexInSheet=4|"),
            "line after two pins takes slot 4 (pins consumed 2 and 3): {line}"
        );
    }

    /// A UI-authored header comes back segment for segment: the twin-first
    /// `%UTF8%Key=<UTF-8>|||Key=<Windows-1252>` description, the omitted
    /// `LibraryPath`/`SheetPartFileName`, the unmodelled
    /// `COMPONENTKINDVERSION2`, the stale `AllPinCount` — and an edited field
    /// takes the canonical form in its own slot.
    #[test]
    fn a_read_header_is_replayed_verbatim_until_a_field_is_edited() {
        use super::super::reader;

        // Exactly what a UI-drawn 32-pin MCU stores (AllPinCount=1 and all).
        let record = concat!(
            "|RECORD=1|LibReference=STM32G0C1KET6N",
            "|%UTF8%ComponentDescription=STM32G0, Arm\u{c2}\u{ae} Cortex\u{c2}\u{ae}-M0+",
            "|||ComponentDescription=STM32G0, Arm\u{ae} Cortex\u{ae}-M0+",
            "|PartCount=2|DisplayModeCount=1|IndexInSheet=-1|OwnerPartId=-1|CurrentPartId=1",
            "|SourceLibraryName=*|TargetFileName=*|AllPinCount=1|AreaColor=11599871|Color=128",
            "|PartIDLocked=F|COMPONENTKINDVERSION2=5"
        );
        let mut symbol = Symbol::new("placeholder");
        reader::parse_text_record_from_string_for_test(&mut symbol, record);
        assert_eq!(symbol.name, "STM32G0C1KET6N");
        assert_eq!(symbol.description, "STM32G0, Arm\u{ae} Cortex\u{ae}-M0+");
        assert_eq!(symbol.all_pin_count, Some(1));
        assert_eq!(symbol.header_params.len(), 18, "{:?}", symbol.header_params);
        for _ in 0..32 {
            symbol.add_pin(Pin::new("P", "1", 0, 0, 10, PinOrientation::Left));
        }

        // Unchanged: byte-identical, stale count and all.
        assert_eq!(encode_component_header(&symbol), record);

        // An edit to the description replaces its two segments with the
        // canonical form (ASCII here, so no twin) at the plain key's slot.
        symbol.description = "CAN transceiver".to_string();
        let edited = encode_component_header(&symbol);
        assert!(
            edited.contains(
                "|LibReference=STM32G0C1KET6N|||ComponentDescription=CAN transceiver|PartCount=2|"
            ),
            "{edited}"
        );
        assert!(!edited.contains("%UTF8%"), "{edited}");
        assert!(
            edited.ends_with("|PartIDLocked=F|COMPONENTKINDVERSION2=5"),
            "{edited}"
        );

        // A field the header never carried is appended when set; the
        // constant keys it omitted stay omitted.
        symbol.part_id_locked = true;
        let locked = encode_component_header(&symbol);
        assert!(locked.contains("|PartIDLocked=T|"), "{locked}");
        assert!(!locked.contains("LibraryPath"), "{locked}");
    }

    /// From scratch the canonical header is unchanged: every constant key,
    /// the pin count, and the scripted `%UTF8%` form for a non-ASCII value.
    #[test]
    fn a_fresh_header_is_canonical() {
        let mut symbol = Symbol::new("R\u{e9}sistance");
        symbol.add_pin(Pin::new("1", "1", 0, 0, 10, PinOrientation::Left));
        let header = encode_component_header(&symbol);
        let bytes = crate::altium::encode_utf8_param_value("R\u{e9}sistance");
        assert!(
            header.starts_with(&format!(
                "|RECORD=1|LibReference={bytes}|%UTF8%LibReference={bytes}|ComponentDescription=|"
            )),
            "{header}"
        );
        assert!(header.contains("|LibraryPath=*|SourceLibraryName=*|SheetPartFileName=*|TargetFileName=*|AllPinCount=1|"), "{header}");
        symbol.all_pin_count = Some(7);
        assert!(encode_component_header(&symbol).contains("|AllPinCount=7|"));
    }

    /// Only Altium's own `Comment` is the system parameter: a user parameter
    /// stored with `OwnerPartId=-1` — the UI's habit — is a content record
    /// with a counter slot, in authoring order before the graphics.
    #[test]
    fn user_parameters_with_owner_minus_one_keep_their_slots_and_order() {
        let mut symbol = Symbol::new("PESD1CAN");
        symbol.designator = "U?".to_string();
        for (name, value) in [("Value", "PESD1CAN"), ("Part Number", "PESD1CAN")] {
            let mut p = Parameter::new(name, value);
            p.owner_part_id = -1;
            symbol.add_parameter(p);
        }
        symbol.add_rectangle(Rectangle::new(-20, -10, 30, 20));
        let mut comment = Parameter::new("Comment", "=VALUE");
        comment.owner_part_id = -1;
        symbol.add_parameter(comment);

        let data = encode_data_stream(&symbol).expect("encode");
        let records = stream_records(&data);
        let kinds: Vec<String> = records
            .iter()
            .map(|r| {
                let kind = r.split('|').nth(1).unwrap_or("").to_string();
                let index = r
                    .split('|')
                    .find_map(|t| t.strip_prefix("IndexInSheet="))
                    .unwrap_or("(absent)")
                    .to_string();
                format!("{kind} {index}")
            })
            .collect();
        assert_eq!(
            &kinds[..5],
            [
                "RECORD=1 -1",
                "RECORD=41 (absent)", // Value: slot 0, key omitted
                "RECORD=41 1",        // Part Number
                "RECORD=14 2",        // the rectangle
                "RECORD=34 -1",       // the designator
            ],
            "{kinds:?}"
        );
        assert_eq!(
            kinds[5], "RECORD=41 -1",
            "the system Comment, after the designator"
        );
    }

    /// A footprint model that is not the current one carries no `IsCurrent`
    /// key at all, like every false boolean.
    #[test]
    fn is_current_is_written_only_when_true() {
        let model = FootprintModel::new("SOT-23");
        assert!(encode_footprint_model(&model, 1, true).contains("|IsCurrent=T|UniqueID="));
        let other = encode_footprint_model(&model, 1, false);
        assert!(!other.contains("IsCurrent"), "{other}");
        assert!(
            other.contains("|ModelDatafileKind0=PCBLib|UniqueID="),
            "{other}"
        );
    }

    /// Every key any golden record carries is one an encoder can emit (or a
    /// vertex key), so the replay's "a read key the canonical form lacks was
    /// edited to its default" rule never mistakes an Altium key for one of
    /// ours — and `MODELLED_RECORD_KEYS` cannot silently fall behind.
    #[test]
    fn every_golden_record_key_is_modelled() {
        let lib = crate::altium::SchLib::open("scripts/samples/symbols.SchLib").unwrap();
        let mut unmodelled = std::collections::BTreeSet::new();
        for symbol in lib.iter() {
            let records: Vec<&Vec<(String, String)>> = symbol
                .rectangles
                .iter()
                .map(|r| &r.raw_params)
                .chain(symbol.lines.iter().map(|r| &r.raw_params))
                .chain(symbol.polylines.iter().map(|r| &r.raw_params))
                .chain(symbol.polygons.iter().map(|r| &r.raw_params))
                .chain(symbol.arcs.iter().map(|r| &r.raw_params))
                .chain(symbol.pies.iter().map(|r| &r.raw_params))
                .chain(symbol.images.iter().map(|r| &r.raw_params))
                .chain(symbol.text_frames.iter().map(|r| &r.raw_params))
                .chain(symbol.beziers.iter().map(|r| &r.raw_params))
                .chain(symbol.ellipses.iter().map(|r| &r.raw_params))
                .chain(symbol.round_rects.iter().map(|r| &r.raw_params))
                .chain(symbol.elliptical_arcs.iter().map(|r| &r.raw_params))
                .chain(symbol.labels.iter().map(|r| &r.raw_params))
                .chain(symbol.ieee_symbols.iter().map(|r| &r.raw_params))
                .chain(symbol.parameters.iter().map(|r| &r.raw_params))
                .collect();
            assert!(
                !records.is_empty(),
                "{}: every record carries its raw segments",
                symbol.name
            );
            for raw in records {
                assert!(
                    !raw.is_empty(),
                    "{}: a read record has raw segments",
                    symbol.name
                );
                for (key, _) in raw {
                    if !key.is_empty() && !is_modelled_record_key(key) {
                        unmodelled.insert(key.clone());
                    }
                }
            }
        }
        assert!(
            unmodelled.is_empty(),
            "golden keys no encoder emits: {unmodelled:?}"
        );
    }

    /// The replay's four rules on one rectangle read from a UI-authored
    /// library: verbatim while unchanged (no `LineWidth` invented), the
    /// canonical value for an edited field, a cleared flag's key dropped, an
    /// unmodelled Altium key kept in place, and the positional index renumbered.
    #[test]
    fn a_read_record_is_replayed_verbatim_until_edited() {
        // As the UI stores it: no LineWidth, an unmodelled key in the middle.
        let raw: Vec<(String, String)> = [
            ("RECORD", "14"),
            ("IsNotAccesible", "T"),
            ("IndexInSheet", "3"),
            ("OwnerPartId", "1"),
            ("Location.X", "-20"),
            ("Location.Y", "-10"),
            ("Corner.X", "30"),
            ("Corner.Y", "20"),
            ("FUTUREKEY", "7"),
            ("Color", "128"),
            ("AreaColor", "11599871"),
            ("IsSolid", "T"),
            ("UniqueID", "GOHQXBJE"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
        let mut rect = Rectangle::new(-20, -10, 30, 20);
        rect.filled = true;
        rect.unique_id = Some("GOHQXBJE".to_string());
        rect.raw_params = raw;

        let verbatim = replay_record(&encode_rectangle(&rect, 3), &rect.raw_params);
        assert_eq!(
            verbatim,
            "|RECORD=14|IsNotAccesible=T|IndexInSheet=3|OwnerPartId=1|Location.X=-20|Location.Y=-10|Corner.X=30|Corner.Y=20|FUTUREKEY=7|Color=128|AreaColor=11599871|IsSolid=T|UniqueID=GOHQXBJE"
        );

        // Moved to slot 0, corner edited, fill cleared, line width set.
        rect.x2 = 35.0;
        rect.filled = false;
        rect.line_width = 2;
        let edited = replay_record(&encode_rectangle(&rect, 0), &rect.raw_params);
        assert_eq!(
            edited,
            "|RECORD=14|IsNotAccesible=T|OwnerPartId=1|Location.X=-20|Location.Y=-10|Corner.X=35|Corner.Y=20|FUTUREKEY=7|Color=128|AreaColor=11599871|UniqueID=GOHQXBJE|LineWidth=2"
        );

        // No raw segments: the canonical form, LineWidth and all.
        rect.raw_params.clear();
        assert!(
            replay_record(&encode_rectangle(&rect, 0), &rect.raw_params).contains("|LineWidth=2|")
        );
        rect.line_width = 1;
        assert!(
            replay_record(&encode_rectangle(&rect, 0), &rect.raw_params).contains("|LineWidth=1|")
        );
    }

    /// A `%UTF8%` twin goes back with the bytes it was read with — a locale
    /// artefact of the writing machine — for as long as its plain key is
    /// unchanged, and in the canonical form once the text is edited.
    #[test]
    fn a_utf8_twin_follows_its_plain_key() {
        let raw: Vec<(String, String)> = [
            ("RECORD", "4"),
            ("OwnerPartId", "1"),
            ("FontID", "1"),
            ("%UTF8%Text", "R\u{c4}\u{82}\u{c2}\u{a9}sistance"),
            ("", ""),
            ("", ""),
            ("Text", "R\u{c3}\u{a9}sistance"),
            ("UniqueID", "ADKXLEQV"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
        let mut label = Label {
            x: 0.0,
            y: 0.0,
            text: "R\u{e9}sistance".to_string(),
            font_id: 1,
            color: 0,
            justification: TextJustification::BottomLeft,
            rotation: 0.0,
            is_mirrored: false,
            is_hidden: false,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: Some("ADKXLEQV".to_string()),
            raw_params: raw,
        };
        let replayed = replay_record(&encode_label(&label, 0), &label.raw_params);
        assert!(
            replayed.contains(
                "|%UTF8%Text=R\u{c4}\u{82}\u{c2}\u{a9}sistance|||Text=R\u{c3}\u{a9}sistance|"
            ),
            "{replayed}"
        );

        label.text = "R2".to_string();
        let edited = replay_record(&encode_label(&label, 0), &label.raw_params);
        assert!(edited.contains("|||Text=R2|UniqueID=ADKXLEQV"), "{edited}");
        assert!(
            !edited.contains("%UTF8%"),
            "an ASCII edit drops the twin: {edited}"
        );
    }

    #[test]
    fn a_symbol_with_no_recorded_order_leads_with_the_rectangles() {
        // Populating the lists directly leaves `primitive_order` empty, which is
        // what a symbol deserialised from a `write_schlib` call looks like. The
        // canonical order applies, and it puts the rectangles first so a
        // solid-filled body sits behind the pins rather than painting over the
        // pin names inside it.
        let symbol = Symbol {
            pins: vec![Pin::new("A", "1", -10, 0, 5, PinOrientation::Left)],
            rectangles: vec![Rectangle::new(-5, -5, 5, 5)],
            lines: vec![Line::new(-5, 0, 5, 0)],
            ..Symbol::new("CANONICAL")
        };
        assert!(
            symbol.primitive_order.is_empty(),
            "direct construction records no order"
        );

        let data = encode_data_stream(&symbol).expect("encode");
        let records = stream_records(&data);
        let kinds: Vec<&str> = records
            .iter()
            .filter_map(|t| t.strip_prefix("|RECORD="))
            .filter_map(|t| t.split('|').next())
            .collect();
        // 1 = header, 14 = rectangle, 13 = line; the pin is binary and does not
        // appear among the text records.
        assert_eq!(
            kinds.iter().position(|k| *k == "14"),
            Some(1),
            "the rectangle is the first content record: {kinds:?}"
        );
        assert!(
            kinds.iter().position(|k| *k == "13") > kinds.iter().position(|k| *k == "14"),
            "the line follows it: {kinds:?}"
        );
    }

    #[test]
    fn system_parameter_keeps_minus_one_and_consumes_no_slot() {
        // F1 regression test, pinned to the golden DISPMODE sequence exactly:
        // the system Comment (owner_part_id == -1) carries the IndexInSheet=-1
        // sentinel and does NOT consume a content-counter slot — the first
        // rectangle stays at slot 0 (token omitted) and the second at =1, as
        // the golden stores. Feeding system parameters through the shared
        // counter destroyed the -1 and shifted every content index by one on
        // read-modify-write.
        let mut symbol = Symbol::new("DISPMODE");
        symbol.designator = "U?".to_string();
        let mut comment = Parameter::new("Comment", "*");
        comment.owner_part_id = -1;
        comment.x = -5.0;
        comment.y = -15.0;
        comment.unique_id = Some("SBJHPTML".to_string());
        symbol.add_parameter(comment);
        let mut rect1 = Rectangle::new(-5.0, -2.5, 5.0, 2.5);
        rect1.line_color = 0; // the golden rectangles omit Color (0)
        rect1.unique_id = Some("ODNTDFPU".to_string());
        symbol.add_rectangle(rect1);
        let mut rect2 = Rectangle::new(-6, -3, 6, 3);
        rect2.line_color = 0;
        rect2.display_flags.owner_part_display_mode = 1;
        rect2.unique_id = Some("IELVGVKJ".to_string());
        symbol.add_rectangle(rect2);

        let data = encode_data_stream(&symbol).expect("encode");
        let records = stream_records(&data);
        // Golden DISPMODE record text, byte for byte.
        assert!(
            records.iter().any(|t| t
                == "|RECORD=14|IsNotAccesible=T|OwnerPartId=1|Location.X=-5|Location.Y=-2\
                    |Location.Y_Frac=-50000|Corner.X=5|Corner.Y=2|Corner.Y_Frac=50000\
                    |LineWidth=1|AreaColor=11599871|IsSolid=T|UniqueID=ODNTDFPU"),
            "first rectangle (slot 0, token omitted) must match the golden exactly: {records:#?}"
        );
        assert!(
            records.iter().any(|t| t
                == "|RECORD=14|IsNotAccesible=T|IndexInSheet=1|OwnerPartId=1\
                    |OwnerPartDisplayMode=1|Location.X=-6|Location.Y=-3|Corner.X=6|Corner.Y=3\
                    |LineWidth=1|AreaColor=11599871|IsSolid=T|UniqueID=IELVGVKJ"),
            "second rectangle (slot 1) must match the golden exactly: {records:#?}"
        );
        assert!(
            records.iter().any(|t| t
                == "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15\
                    |Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=SBJHPTML"),
            "system Comment keeps the -1 sentinel and the golden token order: {records:#?}"
        );
    }

    #[test]
    fn index_in_sheet_single_shape_symbol_emits_no_token() {
        // Byte-identity gate: a from-scratch single-shape symbol emits NO
        // IndexInSheet on the shape (slot 0 omitted), so its output bytes are
        // unchanged; only the header keeps its -1.
        let mut symbol = Symbol::new("ONESHAPE");
        symbol.add_rectangle(Rectangle::new(-5, -5, 5, 5));

        let data = encode_data_stream(&symbol).expect("encode");
        let records = stream_records(&data);
        let rect = records
            .iter()
            .find(|t| t.starts_with("|RECORD=14"))
            .expect("rectangle present");
        assert!(
            !rect.contains("IndexInSheet"),
            "single content record omits the token: {rect}"
        );
        let header = &records[0];
        assert!(
            header.contains("|IndexInSheet=-1|"),
            "component header keeps -1: {header}"
        );
    }

    #[test]
    fn index_in_sheet_system_designator_keeps_minus_one() {
        // The trailing system Designator record (RECORD=34) stays at -1
        // regardless of how many content slots precede it.
        let mut symbol = Symbol::new("SYSREC");
        symbol.designator = "U?".to_string();
        symbol.add_rectangle(Rectangle::new(-5, -5, 5, 5));
        symbol.add_rectangle(Rectangle::new(-3, -3, 3, 3));

        let data = encode_data_stream(&symbol).expect("encode");
        let records = stream_records(&data);
        let designator = records
            .iter()
            .find(|t| t.starts_with("|RECORD=34"))
            .expect("designator present");
        assert!(
            designator.contains("|IndexInSheet=-1|"),
            "system designator keeps -1: {designator}"
        );
    }

    #[test]
    fn index_in_sheet_symbol_round_trips_through_cursor() {
        // An in-RAM Cursor round-trip still parses every primitive with the
        // positional IndexInSheet tokens present (the reader ignores them).
        let mut symbol = Symbol::new("RT");
        symbol.designator = "U?".to_string();
        symbol.add_parameter(Parameter::new("Value", "10k"));
        symbol.add_rectangle(Rectangle::new(-10, -10, 10, 10));
        symbol.add_pin(Pin::new("A", "1", -15, 0, 5, PinOrientation::Left));
        symbol.add_pin(Pin::new("B", "2", 15, 0, 5, PinOrientation::Right));
        symbol.add_line(Line::new(-10, 0, 10, 0));
        symbol.add_label(Label {
            raw_params: Vec::new(),
            x: 0.0,
            y: 12.0,
            text: "hello".to_string(),
            font_id: 1,
            color: 0,
            justification: TextJustification::BottomLeft,
            rotation: 0.0,
            is_mirrored: false,
            is_hidden: false,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        });

        let mut lib = crate::altium::schlib::SchLib::new();
        lib.add(symbol);
        let mut buf = std::io::Cursor::new(Vec::new());
        lib.write(&mut buf).expect("library should serialise");
        buf.set_position(0);
        let back_lib =
            crate::altium::schlib::SchLib::read(buf).expect("library should deserialise");
        let sym = back_lib.get("RT").expect("symbol RT round-trips");
        assert_eq!(sym.parameters.len(), 1);
        assert_eq!(sym.rectangles.len(), 1);
        assert_eq!(sym.pins.len(), 2);
        assert_eq!(sym.lines.len(), 1);
        assert_eq!(sym.labels.len(), 1);
        assert_eq!(sym.designator, "U?");
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

        let data = encode_file_header(&symbols, &ole_names, "ABCDEFGH");

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
        // LibRef carries the REAL name however long — the golden stores the
        // full 33-byte Khmer name here against a 31-unit storage. The map from
        // name to truncated storage lives in the root SectionKeys stream, not
        // in this list.
        let long_name = "A".repeat(64);
        let symbol = Symbol::new(&long_name);
        let symbols = vec![&symbol];
        let ole_names = vec!["A".repeat(31)];

        let data = encode_file_header(&symbols, &ole_names, "ABCDEFGH");

        let text = String::from_utf8_lossy(&data[4..]);
        assert!(
            text.contains(&format!("LibRef0={long_name}")),
            "LibRef must hold the untruncated name"
        );
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

        // Display properties: emitted only when non-default, and each with the
        // Altium key spelling the reader matches case-insensitively.
        let mut d = Parameter::new("Rule", "Width");
        assert!(!encode_parameter(&d, 1).contains("NotAutoPosition"));
        assert!(!encode_parameter(&d, 1).contains("IsRule"));
        d.auto_position = false;
        d.is_rule = true;
        d.is_system_parameter = true;
        d.text_horz_anchor = 2;
        d.text_vert_anchor = 1;
        let ds = encode_parameter(&d, 1);
        for key in [
            "|NotAutoPosition=T",
            "|IsRule=T",
            "|IsSystemParameter=T",
            "|TextHorzAnchor=2",
            "|TextVertAnchor=1",
        ] {
            assert!(ds.contains(key), "emit {key}: {ds}");
        }

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
            raw_params: Vec::new(),
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
            raw_params: Vec::new(),
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
            raw_params: Vec::new(),
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

        // An IEEE symbol as the IEEESYM golden stores a plain dot: no
        // Orientation, Mirror or Color at their defaults, and no UniqueID ever.
        let s = encode_ieee_symbol(&IeeeSymbol::new(1, -10.0, 0.0), 0);
        assert_eq!(
            s,
            "|RECORD=3|IsNotAccesible=T|OwnerPartId=1|Symbol=1|Location.X=-10|ScaleFactor=10|LineWidth=1"
        );
        let mut clock = IeeeSymbol::new(3, 0.0, 0.0);
        clock.rotation = 90.0;
        clock.is_mirrored = true;
        assert_eq!(
            encode_ieee_symbol(&clock, 1),
            "|RECORD=3|IsNotAccesible=T|IndexInSheet=1|OwnerPartId=1|Symbol=3|ScaleFactor=10|Orientation=1|LineWidth=1|Mirror=T"
        );
        let mut locked = IeeeSymbol::new(4, 10.0, 0.0);
        locked.scale_factor = 20.0;
        locked.color = 16_711_680;
        locked.display_flags.graphically_locked = true;
        assert_eq!(
            encode_ieee_symbol(&locked, 2),
            "|RECORD=3|IsNotAccesible=T|IndexInSheet=2|OwnerPartId=1|GraphicallyLocked=T|Symbol=4|Location.X=10|ScaleFactor=20|LineWidth=1|Color=16711680"
        );
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
                raw_params: Vec::new(),
                points: vec![(0.0, 0.0), (5.0, 5.0)],
                line_width: 1,
                color: 0,
                line_style: 0,
                start_line_shape: 0,
                end_line_shape: 0,
                line_shape_size: 0,
                transparent: false,
                is_not_accessible: true,
                owner_part_id: 1,
                display_flags: ShapeDisplayFlags::default(),
                unique_id: Some("ABCD1234".to_string()),
            },
            1,
        );
        let poly = encode_polygon(
            &Polygon {
                raw_params: Vec::new(),
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
                raw_params: Vec::new(),
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
                raw_params: Vec::new(),
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
            raw_params: Vec::new(),
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
    fn ascii_text_stays_plain_and_non_ascii_promotes() {
        // The promotion gate is ASCII, not Windows-1252-representability. The
        // golden's `Résistance_L1` record stores its LibReference and labels as
        // raw UTF-8 bytes with a `%UTF8%` twin even though `é` has a
        // single-byte Windows-1252 form, so `µ`/`é` values promote too; only a
        // pure-ASCII value keeps the bare single key.
        let mut p = Parameter::new("Value", "10\u{00B5}F"); // "10µF"
        p.unique_id = Some("ABCD1234".to_string());
        let s = encode_parameter(&p, 1);
        let expected = crate::altium::encode_utf8_param_value("10\u{00B5}F");
        assert!(
            s.contains(&format!("|Text={expected}|")),
            "plain Text carries the UTF-8 bytes: {s}"
        );
        assert!(
            s.contains(&format!("%UTF8%Text={expected}")),
            "non-ASCII value gets the %UTF8% twin: {s}"
        );

        let mut label = Label {
            raw_params: Vec::new(),
            x: 0.0,
            y: 0.0,
            text: "caf\u{00E9}".to_string(), // "café" — non-ASCII, so promoted
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
        assert!(s.contains("%UTF8%Text="), "café promotes: {s}");

        // An ASCII label is byte-identical to the historical output.
        label.text = "R".to_string();
        let s = encode_label(&label, 1);
        assert!(s.contains("|Text=R|"), "plain ASCII Text: {s}");
        assert!(!s.contains("%UTF8%"), "no %UTF8% key for ASCII: {s}");
    }

    #[test]
    fn non_win1252_text_emits_both_keys_carrying_utf8_bytes() {
        // Greek omega (U+03A9) is NOT in Windows-1252. Altium writes such a value
        // twice — the plain key holding its raw UTF-8 bytes, plus a `%UTF8%`
        // companion — and reads the plain one, so emitting only the companion
        // leaves the value `?`-mangled in Altium.
        let mut p = Parameter::new("Value", "10k\u{03A9}");
        p.unique_id = Some("ABCD1234".to_string());
        let s = encode_parameter(&p, 1);

        // Both keys, carrying the same UTF-8 bytes mapped one char per byte.
        let expected = crate::altium::encode_utf8_param_value("10k\u{03A9}");
        assert!(
            s.contains(&format!("|Text={expected}|")),
            "plain Text must carry the UTF-8 bytes: {s}"
        );
        assert!(
            s.contains(&format!("|%UTF8%Text={expected}|")),
            "%UTF8%Text companion: {s}"
        );
        assert!(!s.contains("10k?"), "no `?`-mangled value anywhere: {s}");
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
                raw_params: Vec::new(),
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

    #[test]
    fn a_read_footprint_link_is_replayed_verbatim() {
        // As the UI stores it: IntegratedModel and DatabaseModel, which this
        // crate does not model, and no Description while it is empty.
        let raw: Vec<(String, String)> = [
            ("RECORD", "45"),
            ("OwnerIndex", "1"),
            ("IndexInSheet", "-1"),
            ("ModelName", "MOUNTING_HOLE"),
            ("ModelType", "PCBLIB"),
            ("DatafileCount", "1"),
            ("ModelDatafileEntity0", "MOUNTING_HOLE"),
            ("ModelDatafileKind0", "PCBLib"),
            ("IsCurrent", "T"),
            ("IntegratedModel", "T"),
            ("DatabaseModel", "T"),
            ("UniqueID", "ABCDEFGH"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
        let mut model = FootprintModel::new("MOUNTING_HOLE");
        model.unique_id = Some("ABCDEFGH".to_string());
        model.raw_params = raw;

        let verbatim = replay_record(&encode_footprint_model(&model, 1, true), &model.raw_params);
        assert_eq!(
            verbatim,
            "|RECORD=45|OwnerIndex=1|IndexInSheet=-1|ModelName=MOUNTING_HOLE|ModelType=PCBLIB|DatafileCount=1|ModelDatafileEntity0=MOUNTING_HOLE|ModelDatafileKind0=PCBLib|IsCurrent=T|IntegratedModel=T|DatabaseModel=T|UniqueID=ABCDEFGH"
        );

        // A name-only link as Altium stores it — no datafile group — keeps
        // that shape; giving it a path adds the whole group.
        let raw: Vec<(String, String)> = [
            ("RECORD", "45"),
            ("OwnerIndex", "4"),
            ("IndexInSheet", "-1"),
            ("ModelName", "SOIC-8-WIDE"),
            ("ModelType", "PCBLIB"),
            ("UniqueID", "IQFACTUZ"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
        let mut plain = FootprintModel::new("SOIC-8-WIDE");
        plain.unique_id = Some("IQFACTUZ".to_string());
        plain.raw_params = raw;
        assert_eq!(
            replay_record(&encode_footprint_model(&plain, 4, false), &plain.raw_params),
            "|RECORD=45|OwnerIndex=4|IndexInSheet=-1|ModelName=SOIC-8-WIDE|ModelType=PCBLIB|UniqueID=IQFACTUZ"
        );
        plain.library_path = Some("Lib.PcbLib".to_string());
        assert_eq!(
            replay_record(&encode_footprint_model(&plain, 4, false), &plain.raw_params),
            "|RECORD=45|OwnerIndex=4|IndexInSheet=-1|ModelName=SOIC-8-WIDE|ModelType=PCBLIB|UniqueID=IQFACTUZ|DatafileCount=1|ModelDatafile0=Lib.PcbLib|ModelDatafileEntity0=SOIC-8-WIDE|ModelDatafileKind0=PCBLib"
        );

        // No longer current, a description given: the unmodelled keys stay,
        // IsCurrent goes, the description is appended.
        model.description = "M3".to_string();
        let edited = replay_record(&encode_footprint_model(&model, 1, false), &model.raw_params);
        assert_eq!(
            edited,
            "|RECORD=45|OwnerIndex=1|IndexInSheet=-1|ModelName=MOUNTING_HOLE|ModelType=PCBLIB|DatafileCount=1|ModelDatafileEntity0=MOUNTING_HOLE|ModelDatafileKind0=PCBLib|IntegratedModel=T|DatabaseModel=T|UniqueID=ABCDEFGH|Description=M3"
        );
    }
    /// A replayed header that names only some keys still writes every
    /// canonical key: the ones the replay list omits are appended, never
    /// dropped.
    #[test]
    fn a_partial_header_replay_still_writes_every_canonical_key() {
        let mut symbol = Symbol::new("PARTIAL_HEADER");
        symbol.header_params = vec![("LibReference".to_string(), "PARTIAL_HEADER".to_string())];
        let data = encode_data_stream(&symbol).expect("encode");
        let text = String::from_utf8_lossy(&data);
        assert!(
            text.contains("PartCount="),
            "canonical keys the replay omits are appended: {text}"
        );
    }
}
