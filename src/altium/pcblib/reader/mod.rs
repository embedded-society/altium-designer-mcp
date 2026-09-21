//! Binary reader for `PcbLib` Data streams.
//!
//! This module handles parsing the binary format of Altium `PcbLib` Data streams,
//! which contain the primitives (pads, tracks, arcs, etc.) that make up footprints.
//!
//! # Data Stream Format
//!
//! ```text
//! [name_block_len:4][str_len:1][name:str_len]  // Component name
//! [record_type:1][blocks...]                   // First primitive
//! [record_type:1][blocks...]                   // Second primitive
//! ...                                          // exactly the primitive count from the component header
//! ```
//!
//! There is NO trailing end marker — the writer deliberately omits any final
//! `0x00` (a stray one is mis-read as a zero-length record; see issue #68).
//!
//! # Record Types
//!
//! - `0x01`: Arc
//! - `0x02`: Pad
//! - `0x03`: Via
//! - `0x04`: Track
//! - `0x05`: Text
//! - `0x06`: Fill
//! - `0x0B`: Region
//! - `0x0C`: `ComponentBody`

use std::collections::HashMap;

use super::primitives::{
    Arc, ComponentBody, DrillLayerPairType, Fill, HoleShape, Layer, MaskExpansionMode, Pad,
    PadPolygonConnect, PadShape, PadStackMode, PcbFlags, PowerPlaneConnectStyle, Region,
    RegionKind, StrokeFont, Text, TextJustification, TextKind, Track, Vertex, Via, ViaStackMode,
};
use super::Footprint;
use crate::altium::bytes::{
    read_f64_le as read_f64, read_i16_le as read_i16, read_i32_le as read_i32,
    read_u16_le as read_u16, read_u32_le as read_u32,
};
use crate::altium::error::AltiumError;

mod models;
mod parsers;

pub use models::{parse_embedded_models, parse_model_data_stream, parse_model_header_stream};
pub use parsers::parse_mil_value;
#[allow(clippy::wildcard_imports)] // tightly-coupled reader split
use parsers::*;
pub use parsers::{PAD_POLYGON_CONNECT_AT, PAD_POLYGON_CONNECT_LEN};

/// Result type for internal parse functions.
///
/// Returns the parsed primitive along with the new offset on success,
/// or an [`AltiumError::ParseError`] with offset and message on failure.
type ParseResult<T> = Result<(T, usize), AltiumError>;

/// A lookup table for `WideStrings` text content.
///
/// Maps index (e.g., 0, 1, 2) to decoded text content.
/// The `/WideStrings` stream stores text as `|ENCODEDTEXT{N}=c1,c2,c3,...|`
/// where c1,c2,c3 are ASCII character codes.
pub type WideStrings = HashMap<usize, String>;

/// A unique ID entry parsed from the `UniqueIDPrimitiveInformation` stream.
///
/// Each entry maps a primitive (by index and type) to its unique ID.
#[derive(Debug, Clone)]
pub struct UniqueIdEntry {
    /// `PRIMITIVEINDEX`: a single global 0-based ordinal over all primitives in
    /// `Data`-stream emit order (not a per-type index).
    pub primitive_index: usize,
    /// Primitive object type (e.g., "Pad", "Track", "Arc").
    pub primitive_type: String,
    /// Unique ID (8-character alphanumeric string).
    pub unique_id: String,
}

/// A list of unique ID entries for primitives in a footprint.
pub type UniqueIdMap = Vec<UniqueIdEntry>;

/// Parses the `/WideStrings` stream content.
///
/// # Format
///
/// ```text
/// |ENCODEDTEXT0=84,69,83,84|ENCODEDTEXT1=72,69,76,76,79|
/// ```
///
/// Where `84,69,83,84` = "TEST" (ASCII codes: T=84, E=69, S=83, T=84).
///
/// # Returns
///
/// A `HashMap` mapping index to decoded text content.
pub fn parse_wide_strings(data: &[u8]) -> WideStrings {
    let mut strings = WideStrings::new();

    // Pipe-delimited key=value pairs in Windows-1252, null-terminated. Decoding
    // is lossless for any byte, unlike UTF-8, which cannot represent an
    // arbitrary Windows-1252 stream. The terminator must be trimmed before
    // splitting or it stays glued to the final value ("84\0"), which then fails
    // to parse as a byte.
    let text = crate::altium::decode_windows1252(data);
    let text = text.trim_end_matches('\u{0}');

    for pair in text.split('|') {
        if pair.is_empty() {
            continue;
        }

        // Look for ENCODEDTEXT{N}=...
        if let Some(rest) = pair.strip_prefix("ENCODEDTEXT") {
            if let Some((index_str, encoded)) = rest.split_once('=') {
                if let Ok(index) = index_str.parse::<usize>() {
                    let decoded = decode_wide_string(encoded);
                    if !decoded.is_empty() {
                        tracing::trace!(index, text = %decoded, "Decoded WideStrings entry");
                        strings.insert(index, decoded);
                    }
                }
            }
        }
    }

    tracing::debug!(count = strings.len(), "Parsed WideStrings stream");
    strings
}

/// Decodes an `ENCODEDTEXT` value — comma-separated UTF-16 code units in
/// decimal — to a string: `"84,69,83,84"` → `"TEST"`, `"49,48,181,70"` →
/// `"10µF"`, `"937"` → `"Ω"`.
///
/// The units are what `WideStrings` exists to carry: text the Windows-1252
/// `Data` stream cannot hold. A unit that does not parse is skipped; an
/// unpaired surrogate becomes U+FFFD.
fn decode_wide_string(encoded: &str) -> String {
    let units: Vec<u16> = encoded
        .split(',')
        .filter_map(|s| s.trim().parse::<u16>().ok())
        .collect();
    String::from_utf16_lossy(&units)
}

/// Parses the `UniqueIDPrimitiveInformation/Data` stream content.
///
/// # Format
///
/// The stream contains length-prefixed records:
/// ```text
/// [length:4 LE u32][record_content:length]
/// [length:4 LE u32][record_content:length]
/// ...
/// ```
///
/// Each record content is a pipe-delimited key=value string:
/// ```text
/// |PRIMITIVEINDEX=1|PRIMITIVEOBJECTID=Pad|UNIQUEID=QHHMRSCB
/// ```
///
/// # Arguments
///
/// * `data` - The raw `UniqueIDPrimitiveInformation/Data` stream bytes
///
/// # Returns
///
/// A vector of `UniqueIdEntry` structs mapping primitives to their unique IDs.
pub fn parse_unique_id_stream(data: &[u8]) -> UniqueIdMap {
    let mut entries = UniqueIdMap::new();
    let mut offset = 0;

    // Each record is `[len:4][len bytes]`; the scan ends at the first offset
    // without a whole length prefix.
    while let Some(record_len) = read_u32(data, offset) {
        let record_len = record_len as usize;
        offset += 4;

        // Sanity check on record length
        if record_len == 0 || record_len > 10000 || offset + record_len > data.len() {
            tracing::debug!(
                offset,
                record_len,
                "Invalid UniqueID record length, stopping parse"
            );
            break;
        }

        // Read record content as string
        let record_data = &data[offset..offset + record_len];
        offset += record_len;

        // Parse the pipe-delimited record (strip trailing null terminators)
        let trimmed = record_data
            .iter()
            .copied()
            .take_while(|&b| b != 0x00)
            .collect::<Vec<u8>>();
        if let Ok(record_str) = String::from_utf8(trimmed) {
            if let Some(entry) = parse_unique_id_record(&record_str) {
                tracing::trace!(
                    index = entry.primitive_index,
                    primitive_type = %entry.primitive_type,
                    unique_id = %entry.unique_id,
                    "Parsed UniqueID entry"
                );
                entries.push(entry);
            }
        }
    }

    tracing::debug!(
        count = entries.len(),
        "Parsed UniqueIDPrimitiveInformation stream"
    );
    entries
}

/// Parses a single unique ID record string.
///
/// # Format
///
/// ```text
/// |PRIMITIVEINDEX=1|PRIMITIVEOBJECTID=Pad|UNIQUEID=QHHMRSCB
/// ```
fn parse_unique_id_record(record: &str) -> Option<UniqueIdEntry> {
    let params = crate::altium::parse_pipe_params_raw(record);
    let primitive_index: usize = params.get("PRIMITIVEINDEX")?.parse().ok()?;
    let primitive_type = params.get("PRIMITIVEOBJECTID")?.clone();
    // An empty `UNIQUEID` is not a missing record. Every pad in the golden has
    // one, value and all, and Altium writes the stream regardless — the record
    // is what marks the primitive as tracked, so dropping the empty ones threw
    // the whole stream away.
    Some(UniqueIdEntry {
        primitive_index,
        primitive_type,
        unique_id: params.get("UNIQUEID")?.clone(),
    })
}

/// Parses a footprint's `PrimitiveGuids/Data` stream.
///
/// Layout: 24-byte records of `[object_kind: u32][ordinal: u32][guid:
/// 16 bytes]`, packed with no header of their own — the record count lives in
/// the sibling `PrimitiveGuids/Header` stream, so the data is chunked instead.
/// The GUID's first three fields are little-endian, the Windows `GUID` struct
/// layout Altium follows, so it is reassembled rather than printed byte for
/// byte.
///
/// A trailing partial record is ignored: its identity is unrecoverable, and a
/// partial read beats refusing the whole footprint.
#[must_use]
pub fn parse_primitive_guids(data: &[u8]) -> Vec<crate::altium::pcblib::PrimitiveGuid> {
    const RECORD: usize = 24;
    data.chunks_exact(RECORD)
        .map(|rec| crate::altium::pcblib::PrimitiveGuid {
            object_kind: u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]),
            index: u32::from_le_bytes([rec[4], rec[5], rec[6], rec[7]]),
            guid: format_guid(&rec[8..24]),
        })
        .collect()
}

/// Attaches parsed `PrimitiveGuids` records to the primitives they name.
///
/// A record's `index` is the primitive's ordinal among all the footprint's
/// primitives in Data-stream order; called right after parsing, that order is
/// exactly what [`Footprint::write_sequence`] yields. The record's kind must
/// agree with the primitive found at the ordinal or the record is skipped —
/// a foreign file whose ordinal base does not line up mis-attaches nothing.
/// Kind 85 names the footprint record itself.
pub fn apply_primitive_guids(
    footprint: &mut Footprint,
    records: &[crate::altium::pcblib::PrimitiveGuid],
) {
    use crate::altium::pcblib::PrimitiveKind;

    let sequence = footprint.write_sequence();
    for record in records {
        if record.object_kind == 85 {
            footprint.guid = Some(record.guid.clone());
            continue;
        }
        let Some(kind) = PrimitiveKind::from_altium_object_id(record.object_kind) else {
            continue;
        };
        let Some(&(seq_kind, index)) = sequence.get(record.index as usize) else {
            continue;
        };
        if seq_kind != kind {
            continue;
        }
        let guid = Some(record.guid.clone());
        match kind {
            PrimitiveKind::Arc => footprint.arcs[index].guid = guid,
            PrimitiveKind::Pad => footprint.pads[index].guid = guid,
            PrimitiveKind::Via => footprint.vias[index].guid = guid,
            PrimitiveKind::Track => footprint.tracks[index].guid = guid,
            PrimitiveKind::Text => footprint.text[index].guid = guid,
            PrimitiveKind::Region => footprint.regions[index].guid = guid,
            PrimitiveKind::Fill => footprint.fills[index].guid = guid,
            PrimitiveKind::ComponentBody => footprint.component_bodies[index].guid = guid,
        }
    }
}

/// Formats 16 GUID bytes as `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
///
/// The first three fields are little-endian, the last two are byte order as
/// stored — the Microsoft `GUID` convention Altium follows.
fn format_guid(b: &[u8]) -> String {
    use std::fmt::Write as _;
    debug_assert_eq!(b.len(), 16);
    let mut tail = String::with_capacity(12);
    for byte in &b[10..16] {
        let _ = write!(tail, "{byte:02X}");
    }
    format!(
        "{{{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{tail}}}",
        b[3], b[2], b[1], b[0], b[5], b[4], b[7], b[6], b[8], b[9]
    )
}

/// Applies unique IDs from the `UniqueIDPrimitiveInformation` stream to footprint primitives.
///
/// `PRIMITIVEINDEX` is a single global 0-based ordinal over all of the
/// footprint's primitives, in the order the `Data` stream stores them — NOT a
/// per-type index. This walks `Footprint::write_sequence`, the same order
/// `encode_unique_id_stream` writes, so a footprint round-trips.
///
/// # Arguments
///
/// * `footprint` - The footprint to update with unique IDs
/// * `unique_ids` - The parsed unique ID map from `parse_unique_id_stream`
pub fn apply_unique_ids(footprint: &mut Footprint, unique_ids: &UniqueIdMap) {
    use crate::altium::pcblib::PrimitiveKind;

    // Map global ordinal -> (type, uid). Type is kept only to disambiguate a foreign
    // file whose ordinal base doesn't line up: we skip rather than mis-attach.
    let by_ordinal: HashMap<usize, (&str, &str)> = unique_ids
        .iter()
        .map(|entry| {
            (
                entry.primitive_index,
                (entry.primitive_type.as_str(), entry.unique_id.as_str()),
            )
        })
        .collect();

    let assignments: Vec<(PrimitiveKind, usize, String)> = footprint
        .write_sequence()
        .into_iter()
        .enumerate()
        .filter_map(|(ordinal, (kind, index))| {
            let &(ty, uid) = by_ordinal.get(&ordinal)?;
            (ty == kind.object_id()).then(|| (kind, index, uid.to_string()))
        })
        .collect();

    for (kind, index, uid) in assignments {
        let slot = match kind {
            PrimitiveKind::Arc => &mut footprint.arcs[index].unique_id,
            PrimitiveKind::Pad => &mut footprint.pads[index].unique_id,
            PrimitiveKind::Via => &mut footprint.vias[index].unique_id,
            PrimitiveKind::Track => &mut footprint.tracks[index].unique_id,
            PrimitiveKind::Text => &mut footprint.text[index].unique_id,
            PrimitiveKind::Region => &mut footprint.regions[index].unique_id,
            PrimitiveKind::Fill => &mut footprint.fills[index].unique_id,
            PrimitiveKind::ComponentBody => &mut footprint.component_bodies[index].unique_id,
        };
        *slot = Some(uid);
    }

    tracing::trace!(
        footprint = %footprint.name,
        "Applied unique IDs to primitives"
    );
}

// Unit conversions live in `super::units` so the writer and reader share one
// definition of the PcbLib scale (10000 = 1 mil = 0.0254 mm).
use super::units::{to_mm, INTERNAL_UNITS_TO_MM, MM_PER_MIL};

/// Reads a length-prefixed block from data.
/// Returns the block data and the new offset.
///
/// Wraps the shared [`crate::altium::framing::read_block`] frame with a
/// `PcbLib`-side 100 kB sanity cap to reject corrupt/oversized length prefixes.
fn read_block(data: &[u8], offset: usize) -> Option<(&[u8], usize)> {
    let (block, next) = crate::altium::framing::read_block(data, offset)?;
    if block.len() > 100_000 {
        return None;
    }
    Some((block, next))
}

/// Reads a length-prefixed string from block data.
fn read_string_from_block(block: &[u8]) -> String {
    // Pascal short string at the start of the block; Altium stores strings as
    // Windows-1252 (pairs with `write_string_block`).
    let Some(&len) = block.first() else {
        return String::new();
    };
    let end = (1 + usize::from(len)).min(block.len());
    crate::altium::decode_altium_text_in(&block[1..end], crate::altium::current_ansi_encoding())
}

// Flag bits shared with the writer via `super::flags`.
use super::flags::{
    ALT_FLAG_KEEPOUT, ALT_FLAG_TENTING_BOTTOM, ALT_FLAG_TENTING_TOP, ALT_FLAG_TESTPOINT_BOTTOM,
    ALT_FLAG_TESTPOINT_TOP, ALT_FLAG_UNLOCKED,
};

/// Reads PCB flags from the common header bytes 1-2.
///
/// Decodes Altium's on-disk flag word (`FlagSaved`/`FlagUnlocked`/tenting/keepout)
/// into our internal `PcbFlags` — the inverse of `writer::encode_altium_flags`.
/// `FlagUnlocked` is inverted (a clear unlocked bit means the primitive is
/// locked).
fn read_flags(data: &[u8]) -> PcbFlags {
    if data.len() < 3 {
        return PcbFlags::empty();
    }
    let bits = u16::from_le_bytes([data[1], data[2]]);
    let mut flags = PcbFlags::empty();
    if bits & ALT_FLAG_UNLOCKED == 0 {
        flags |= PcbFlags::LOCKED;
    }
    if bits & ALT_FLAG_TENTING_TOP != 0 {
        flags |= PcbFlags::TENTING_TOP;
    }
    if bits & ALT_FLAG_TENTING_BOTTOM != 0 {
        flags |= PcbFlags::TENTING_BOTTOM;
    }
    if bits & ALT_FLAG_TESTPOINT_TOP != 0 {
        flags |= PcbFlags::TESTPOINT_TOP;
    }
    if bits & ALT_FLAG_TESTPOINT_BOTTOM != 0 {
        flags |= PcbFlags::TESTPOINT_BOTTOM;
    }
    if bits & ALT_FLAG_KEEPOUT != 0 {
        flags |= PcbFlags::KEEPOUT;
    }
    // Bits nothing models are carried verbatim, not dropped.
    for (disk_bit, carrier) in PcbFlags::DISK_BITS {
        if bits & disk_bit != 0 {
            flags |= carrier;
        }
    }
    flags
}

/// [`read_flags`] on a bare flag word, for the writer's round-trip test.
#[cfg(test)]
pub fn read_flags_for_test(word: u16) -> PcbFlags {
    let [lo, hi] = word.to_le_bytes();
    read_flags(&[0, lo, hi])
}

/// Converts Altium layer ID to our Layer enum.
///
/// Layer IDs from Altium (based on `pyAltiumLib` and sample files):
/// - 1: Top Layer, 32: Bottom Layer, 74: Multi-Layer
/// - 33: Top Overlay, 34: Bottom Overlay
/// - 35: Top Paste, 36: Bottom Paste
/// - 37: Top Solder, 38: Bottom Solder
/// - 56: Keep-Out Layer
/// - 57-72: Mechanical 1-16
///
/// Component layer pairs (from sample library analysis):
/// - 58 (Mech 2): Top Assembly
/// - 59 (Mech 3): Bottom Assembly
/// - 60 (Mech 4): Top Courtyard
/// - 61 (Mech 5): Bottom Courtyard
/// - 62 (Mech 6): Top 3D Body
/// - 63 (Mech 7): Bottom 3D Body
#[allow(clippy::too_many_lines)] // ID-to-layer lookup for all layer types
pub(super) const fn layer_from_id(id: u8) -> Layer {
    match id {
        1 => Layer::TopLayer,
        // Mid layers (IDs 2-31)
        2 => Layer::MidLayer1,
        3 => Layer::MidLayer2,
        4 => Layer::MidLayer3,
        5 => Layer::MidLayer4,
        6 => Layer::MidLayer5,
        7 => Layer::MidLayer6,
        8 => Layer::MidLayer7,
        9 => Layer::MidLayer8,
        10 => Layer::MidLayer9,
        11 => Layer::MidLayer10,
        12 => Layer::MidLayer11,
        13 => Layer::MidLayer12,
        14 => Layer::MidLayer13,
        15 => Layer::MidLayer14,
        16 => Layer::MidLayer15,
        17 => Layer::MidLayer16,
        18 => Layer::MidLayer17,
        19 => Layer::MidLayer18,
        20 => Layer::MidLayer19,
        21 => Layer::MidLayer20,
        22 => Layer::MidLayer21,
        23 => Layer::MidLayer22,
        24 => Layer::MidLayer23,
        25 => Layer::MidLayer24,
        26 => Layer::MidLayer25,
        27 => Layer::MidLayer26,
        28 => Layer::MidLayer27,
        29 => Layer::MidLayer28,
        30 => Layer::MidLayer29,
        31 => Layer::MidLayer30,
        32 => Layer::BottomLayer,
        33 => Layer::TopOverlay,
        34 => Layer::BottomOverlay,
        35 => Layer::TopPaste,
        36 => Layer::BottomPaste,
        37 => Layer::TopSolder,
        38 => Layer::BottomSolder,
        // Internal planes (IDs 39-54)
        39 => Layer::InternalPlane1,
        40 => Layer::InternalPlane2,
        41 => Layer::InternalPlane3,
        42 => Layer::InternalPlane4,
        43 => Layer::InternalPlane5,
        44 => Layer::InternalPlane6,
        45 => Layer::InternalPlane7,
        46 => Layer::InternalPlane8,
        47 => Layer::InternalPlane9,
        48 => Layer::InternalPlane10,
        49 => Layer::InternalPlane11,
        50 => Layer::InternalPlane12,
        51 => Layer::InternalPlane13,
        52 => Layer::InternalPlane14,
        53 => Layer::InternalPlane15,
        54 => Layer::InternalPlane16,
        // Drill and keep-out layers
        55 => Layer::DrillGuide,
        56 => Layer::KeepOut,
        // Mechanical layers (IDs 57-72)
        57 => Layer::Mechanical1,
        // Component layer pairs (aliased to mechanical layers)
        58 => Layer::TopAssembly,     // Also Mechanical 2
        59 => Layer::BottomAssembly,  // Also Mechanical 3
        60 => Layer::TopCourtyard,    // Also Mechanical 4
        61 => Layer::BottomCourtyard, // Also Mechanical 5
        62 => Layer::Top3DBody,       // Also Mechanical 6
        63 => Layer::Bottom3DBody,    // Also Mechanical 7
        64 => Layer::Mechanical8,
        65 => Layer::Mechanical9,
        66 => Layer::Mechanical10,
        67 => Layer::Mechanical11,
        68 => Layer::Mechanical12,
        69 => Layer::Mechanical13,
        70 => Layer::Mechanical14,
        71 => Layer::Mechanical15,
        72 => Layer::Mechanical16,
        // Drill drawing
        73 => Layer::DrillDrawing,
        // Special layers (IDs 75-85)
        75 => Layer::ConnectLayer,
        76 => Layer::BackgroundLayer,
        77 => Layer::DRCErrorLayer,
        78 => Layer::HighlightLayer,
        79 => Layer::GridColor1,
        80 => Layer::GridColor10,
        81 => Layer::PadHoleLayer,
        82 => Layer::ViaHoleLayer,
        83 => Layer::TopPadMaster,
        84 => Layer::BottomPadMaster,
        85 => Layer::DRCDetailLayer,
        // Extended mechanical layers (Altium Designer 18+, IDs 186-201)
        186 => Layer::Mechanical17,
        187 => Layer::Mechanical18,
        188 => Layer::Mechanical19,
        189 => Layer::Mechanical20,
        190 => Layer::Mechanical21,
        191 => Layer::Mechanical22,
        192 => Layer::Mechanical23,
        193 => Layer::Mechanical24,
        194 => Layer::Mechanical25,
        195 => Layer::Mechanical26,
        196 => Layer::Mechanical27,
        197 => Layer::Mechanical28,
        198 => Layer::Mechanical29,
        199 => Layer::Mechanical30,
        200 => Layer::Mechanical31,
        201 => Layer::Mechanical32,
        // Unknown layers default to MultiLayer
        _ => Layer::MultiLayer,
    }
}

/// Converts Altium pad shape ID to our `PadShape` enum.
const fn pad_shape_from_id(id: u8) -> PadShape {
    match id {
        1 => PadShape::Round,
        2 => PadShape::Rectangle,
        3 => PadShape::Octagonal,
        _ => PadShape::RoundedRectangle,
    }
}

/// Converts Altium hole shape ID to our `HoleShape` enum.
///
/// Hole shape IDs:
/// - 0: Round
/// - 1: Square
/// - 2: Slot
const fn hole_shape_from_id(id: u8) -> HoleShape {
    match id {
        1 => HoleShape::Square,
        2 => HoleShape::Slot,
        _ => HoleShape::Round, // Default and ID 0
    }
}

/// Converts Altium text kind ID to our `TextKind` enum.
///
/// Text kind IDs:
/// - 0: Stroke (vector font)
/// - 1: TrueType
/// - 2: `BarCode`
const fn text_kind_from_id(id: u8) -> TextKind {
    match id {
        1 => TextKind::TrueType,
        2 => TextKind::BarCode,
        _ => TextKind::Stroke, // Default and ID 0
    }
}

/// Converts an Altium stroke font-table ID to our `StrokeFont` enum. The ids
/// are 1-based (1 = the default stroke font), pairing with `stroke_font_to_id`.
///
/// Stroke font IDs (from geometry block bytes 25-26 as u16):
/// - 1: Default
/// - 2: Sans-Serif
/// - 3: Serif
const fn stroke_font_from_id(id: u16) -> StrokeFont {
    match id {
        2 => StrokeFont::SansSerif,
        3 => StrokeFont::Serif,
        _ => StrokeFont::Default,
    }
}

/// Parses primitives from a `PcbLib` Data stream.
///
/// # Arguments
///
/// * `footprint` - The footprint to populate with parsed primitives
/// * `data` - The raw Data stream bytes
/// * `wide_strings` - Optional `WideStrings` lookup for text content
#[allow(clippy::too_many_lines)]
pub fn parse_data_stream(
    footprint: &mut Footprint,
    data: &[u8],
    wide_strings: Option<&WideStrings>,
) {
    // Read name block: [block_len:4][str_len:1][name:str_len]
    let Some(name_block_len) = read_u32(data, 0) else {
        tracing::warn!("Data stream too short for a name block");
        return;
    };

    let mut offset = 4 + name_block_len as usize;

    // Parse primitives until end marker (0x00) or end of data
    while offset < data.len() {
        let record_type = data[offset];

        if record_type == 0x00 {
            // End of records
            break;
        }

        offset += 1;

        match record_type {
            0x01 => {
                // Arc
                match parse_arc(data, offset) {
                    Ok((arc, new_offset)) => {
                        footprint.add_arc(arc);
                        offset = new_offset;
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse Arc: {e}");
                        break;
                    }
                }
            }
            0x02 => {
                // Pad
                match parse_pad(data, offset) {
                    Ok((pad, new_offset)) => {
                        footprint.add_pad(pad);
                        offset = new_offset;
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse Pad: {e}");
                        break;
                    }
                }
            }
            0x04 => {
                // Track
                match parse_track(data, offset) {
                    Ok((track, new_offset)) => {
                        footprint.add_track(track);
                        offset = new_offset;
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse Track: {e}");
                        break;
                    }
                }
            }
            0x05 => {
                // Text
                match parse_text(data, offset, wide_strings) {
                    Ok((text, new_offset)) => {
                        footprint.add_text(text);
                        offset = new_offset;
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse Text: {e}");
                        break;
                    }
                }
            }
            0x0B => {
                // Region (filled polygon)
                match parse_region(data, offset) {
                    Ok((region, new_offset)) => {
                        footprint.add_region(region);
                        offset = new_offset;
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse Region: {e}");
                        break;
                    }
                }
            }
            0x06 => {
                // Fill (filled rectangle)
                match parse_fill(data, offset) {
                    Ok((fill, new_offset)) => {
                        footprint.add_fill(fill);
                        offset = new_offset;
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse Fill: {e}");
                        break;
                    }
                }
            }
            0x0C => {
                // ComponentBody (3D model reference)
                match parse_component_body(data, offset) {
                    Ok((body, new_offset)) => {
                        footprint.add_component_body(body);
                        offset = new_offset;
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse ComponentBody: {e}");
                        break;
                    }
                }
            }
            0x03 => {
                // Via
                match parse_via(data, offset) {
                    Ok((via, new_offset)) => {
                        footprint.add_via(via);
                        offset = new_offset;
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse Via: {e}");
                        break;
                    }
                }
            }
            _ => {
                tracing::debug!("Unknown record type {record_type:#x} at offset {offset:#x}");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== record dispatch ====================================

    /// Builds a Data stream: the component-name block, then one record of
    /// `record_type` followed by `tail` verbatim.
    fn stream_with_record(record_type: u8, tail: &[u8]) -> Vec<u8> {
        let name = b"FP";
        let mut out = Vec::new();
        out.extend_from_slice(&u32::try_from(name.len() + 1).unwrap().to_le_bytes());
        out.push(u8::try_from(name.len()).unwrap());
        out.extend_from_slice(name);
        out.push(record_type);
        out.extend_from_slice(tail);
        out
    }

    /// A block header claiming far more bytes than the stream holds. Every
    /// parser starts by framing its first block, so this is the one truncation
    /// all of them reject regardless of how many blocks or fields they read.
    fn overrunning_block() -> [u8; 4] {
        999_u32.to_le_bytes()
    }

    /// Total primitives of every family on a footprint.
    fn primitive_count(fp: &Footprint) -> usize {
        fp.arcs.len()
            + fp.pads.len()
            + fp.vias.len()
            + fp.tracks.len()
            + fp.text.len()
            + fp.regions.len()
            + fp.fills.len()
            + fp.component_bodies.len()
    }

    #[test]
    fn a_record_that_fails_to_parse_stops_the_scan_for_every_type() {
        // Records are variable-length and read back to back, so a parser that
        // fails has also lost its place in the stream. Continuing would decode
        // the remaining bytes at the wrong offset and invent primitives that
        // were never in the file, which is worse than stopping short. Each
        // record type owns its own arm, so each needs a case.
        for record_type in [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x0B, 0x0C] {
            let data = stream_with_record(record_type, &overrunning_block());
            let mut fp = Footprint::new("FP");
            parse_data_stream(&mut fp, &data, None);
            assert_eq!(
                primitive_count(&fp),
                0,
                "record type {record_type:#x} kept a primitive it could not parse"
            );
        }
    }

    #[test]
    fn the_model_index_stops_at_a_corrupt_record_length() {
        // Model records are length-prefixed and read back to back, so a wrong
        // length has lost the reader's place. It stops rather than decoding the
        // rest at the wrong offset.
        let record = |body: &str| {
            let mut out = u32::try_from(body.len()).unwrap().to_le_bytes().to_vec();
            out.extend_from_slice(body.as_bytes());
            out.push(0); // null terminator
            out
        };

        let good = record("|ID=GUID-1|NAME=part.step");
        assert_eq!(parse_model_data_stream(&good).len(), 1);

        // A length running past the end of the stream.
        let mut overrun = good.clone();
        overrun[0] = 0xFF;
        assert!(parse_model_data_stream(&overrun).is_empty());

        // A zero length: no way to know how far to step.
        assert!(parse_model_data_stream(&0_u32.to_le_bytes()).is_empty());

        // A record with no ID key cannot be matched to a model stream, so it
        // is skipped — but the scan keeps its place and later records survive.
        let mut mixed = record("|NAME=orphan.step");
        mixed.extend_from_slice(&good);
        assert_eq!(parse_model_data_stream(&mixed).len(), 1);
    }

    #[test]
    fn a_model_stream_without_a_guid_or_a_readable_payload_is_dropped() {
        // Both skips lose a 3D body, which is why each logs: the library still
        // opens, and the footprint simply has no model, so a silent drop would
        // look like the model was never there.
        let mut index = ModelIndex::new();
        index.insert("GUID-1".to_string(), (0, "part.step".to_string()));

        // Stream index 7 has no entry in the index.
        let unmapped = parse_embedded_models(&index, &[(7, vec![1, 2, 3])]);
        assert!(unmapped.is_empty(), "{unmapped:?}");

        // Mapped, but the payload is not zlib.
        let corrupt = parse_embedded_models(&index, &[(0, b"not zlib at all".to_vec())]);
        assert!(corrupt.is_empty(), "{corrupt:?}");

        // Mapped and readable: kept, with its id and name from the index.
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut encoder, b"ISO-10303-21;").unwrap();
        let compressed = encoder.finish().unwrap();
        let models = parse_embedded_models(&index, &[(0, compressed)]);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "GUID-1");
        assert_eq!(models[0].name, "part.step");
        assert_eq!(models[0].data, b"ISO-10303-21;");
    }

    #[test]
    fn an_unknown_record_type_stops_the_scan_rather_than_guessing_its_length() {
        // An unrecognised type has no known block layout, so there is no way to
        // step past it to the next record.
        let data = stream_with_record(0x7F, &[0_u8; 44]);
        let mut fp = Footprint::new("FP");
        parse_data_stream(&mut fp, &data, None);
        assert_eq!(primitive_count(&fp), 0);
    }

    #[test]
    fn the_explicit_end_marker_and_a_runt_stream_both_stop_cleanly() {
        // A 0x00 where a record type would be is the end of the list.
        let mut ended = stream_with_record(0x00, &[]);
        ended.truncate(8);
        let mut fp = Footprint::new("FP");
        parse_data_stream(&mut fp, &ended, None);
        assert_eq!(primitive_count(&fp), 0);

        // Too short to hold even the name block: returns without reading.
        for runt in [&[0_u8; 4][..], &[0_u8; 3][..]] {
            let mut fp = Footprint::new("FP");
            parse_data_stream(&mut fp, runt, None);
            assert_eq!(primitive_count(&fp), 0);
        }
    }

    #[test]
    fn test_to_mm() {
        // 1 mil = 10000 internal units = 0.0254 mm
        assert!((to_mm(10000) - 0.0254).abs() < 1e-9);
        // 1 inch = 1000 mils = 10_000_000 internal = 25.4 mm
        assert!((to_mm(10_000_000) - 25.4).abs() < 1e-6);
    }

    #[test]
    fn test_read_block() {
        let data = [
            0x05, 0x00, 0x00, 0x00, // Length = 5
            0x04, 0x7c, 0x26, 0x7c, 0x30, // Content: "|&|0"
        ];
        let (block, offset) = read_block(&data, 0).unwrap();
        assert_eq!(block.len(), 5);
        assert_eq!(offset, 9);

        // The 100 kB sanity cap: a block claiming more is refused as corrupt.
        let mut over = 100_001_u32.to_le_bytes().to_vec();
        over.resize(4 + 100_001, 0);
        assert!(read_block(&over, 0).is_none());
        let mut at_cap = 100_000_u32.to_le_bytes().to_vec();
        at_cap.resize(4 + 100_000, 0);
        assert!(read_block(&at_cap, 0).is_some());
    }

    #[test]
    fn test_read_string_from_block() {
        let block = [0x04, 0x7c, 0x26, 0x7c, 0x30]; // "|&|0"
        let s = read_string_from_block(&block);
        assert_eq!(s, "|&|0");
    }

    #[test]
    fn test_layer_from_id() {
        // Copper layers
        assert_eq!(layer_from_id(1), Layer::TopLayer);
        assert_eq!(layer_from_id(32), Layer::BottomLayer);
        assert_eq!(layer_from_id(74), Layer::MultiLayer);

        // Mid layers (2-31)
        assert_eq!(layer_from_id(2), Layer::MidLayer1);
        assert_eq!(layer_from_id(3), Layer::MidLayer2);
        assert_eq!(layer_from_id(16), Layer::MidLayer15);
        assert_eq!(layer_from_id(31), Layer::MidLayer30);

        // Silkscreen and mask layers
        assert_eq!(layer_from_id(33), Layer::TopOverlay);
        assert_eq!(layer_from_id(34), Layer::BottomOverlay);
        assert_eq!(layer_from_id(35), Layer::TopPaste);
        assert_eq!(layer_from_id(36), Layer::BottomPaste);
        assert_eq!(layer_from_id(37), Layer::TopSolder);
        assert_eq!(layer_from_id(38), Layer::BottomSolder);

        // Internal planes (39-54)
        assert_eq!(layer_from_id(39), Layer::InternalPlane1);
        assert_eq!(layer_from_id(40), Layer::InternalPlane2);
        assert_eq!(layer_from_id(54), Layer::InternalPlane16);

        // Drill layers
        assert_eq!(layer_from_id(55), Layer::DrillGuide);
        assert_eq!(layer_from_id(56), Layer::KeepOut);
        assert_eq!(layer_from_id(73), Layer::DrillDrawing);

        // Mechanical layers (57-72)
        assert_eq!(layer_from_id(57), Layer::Mechanical1);
        // Component layer pairs (aliased to mechanical 2-7)
        assert_eq!(layer_from_id(58), Layer::TopAssembly);
        assert_eq!(layer_from_id(59), Layer::BottomAssembly);
        assert_eq!(layer_from_id(60), Layer::TopCourtyard);
        assert_eq!(layer_from_id(61), Layer::BottomCourtyard);
        assert_eq!(layer_from_id(62), Layer::Top3DBody);
        assert_eq!(layer_from_id(63), Layer::Bottom3DBody);
        assert_eq!(layer_from_id(64), Layer::Mechanical8);
        assert_eq!(layer_from_id(72), Layer::Mechanical16);

        // Special layers (75-85)
        assert_eq!(layer_from_id(75), Layer::ConnectLayer);
        assert_eq!(layer_from_id(76), Layer::BackgroundLayer);
        assert_eq!(layer_from_id(77), Layer::DRCErrorLayer);
        assert_eq!(layer_from_id(78), Layer::HighlightLayer);
        assert_eq!(layer_from_id(79), Layer::GridColor1);
        assert_eq!(layer_from_id(80), Layer::GridColor10);
        assert_eq!(layer_from_id(81), Layer::PadHoleLayer);
        assert_eq!(layer_from_id(82), Layer::ViaHoleLayer);
        assert_eq!(layer_from_id(83), Layer::TopPadMaster);
        assert_eq!(layer_from_id(84), Layer::BottomPadMaster);
        assert_eq!(layer_from_id(85), Layer::DRCDetailLayer);

        // Unknown IDs should default to MultiLayer
        assert_eq!(layer_from_id(0), Layer::MultiLayer);
        assert_eq!(layer_from_id(255), Layer::MultiLayer);
    }

    #[test]
    fn every_mapped_layer_id_resolves_to_a_distinct_layer() {
        // This table is the reader-side twin of the writer's, and that family
        // of table has been wrong before: a copy-pasted arm sent Mechanical
        // 17-32 to the wrong layer, and a bottom-side region silently changed
        // sides. Walking every id and demanding distinctness catches that
        // class outright, which the spot-checks above cannot.
        use std::collections::HashMap;

        // Every id the table names explicitly. 74 is deliberately absent: it
        // is Multi-Layer, which doubles as the fallback, so it cannot be
        // distinct from an unknown id.
        let mapped: Vec<u8> = (1..=73).chain(75..=85).chain(186..=201).collect();
        assert_eq!(mapped.len(), 100, "the documented id set changed");

        let mut seen: HashMap<String, u8> = HashMap::new();
        for id in mapped {
            let layer = layer_from_id(id);
            assert_ne!(
                layer,
                Layer::MultiLayer,
                "id {id} fell through to the fallback instead of naming a layer"
            );
            if let Some(previous) = seen.insert(format!("{layer:?}"), id) {
                panic!("ids {previous} and {id} both resolve to {layer:?}");
            }
        }

        // Multi-Layer's own id, and ids outside every documented range, land
        // on the fallback rather than on a neighbouring layer.
        for id in [0_u8, 74, 86, 100, 185, 202, 255] {
            assert_eq!(layer_from_id(id), Layer::MultiLayer, "id {id}");
        }
    }

    #[test]
    fn pad_text_and_stroke_font_ids_map_exhaustively() {
        // Small tables, but each unknown id has to reach the documented
        // default rather than the first arm.
        assert_eq!(pad_shape_from_id(1), PadShape::Round);
        assert_eq!(pad_shape_from_id(2), PadShape::Rectangle);
        assert_eq!(pad_shape_from_id(3), PadShape::Octagonal);
        assert_eq!(pad_shape_from_id(9), PadShape::RoundedRectangle);
        assert_eq!(pad_shape_from_id(0), PadShape::RoundedRectangle);

        assert_eq!(text_kind_from_id(0), TextKind::Stroke);
        assert_eq!(text_kind_from_id(1), TextKind::TrueType);
        assert_eq!(text_kind_from_id(2), TextKind::BarCode);
        assert_eq!(text_kind_from_id(200), TextKind::Stroke);

        // The stroke-font ids are 1-based, so 0 is not "the first font".
        assert_eq!(stroke_font_from_id(1), StrokeFont::Default);
        assert_eq!(stroke_font_from_id(2), StrokeFont::SansSerif);
        assert_eq!(stroke_font_from_id(3), StrokeFont::Serif);
        assert_eq!(stroke_font_from_id(0), StrokeFont::Default);
        assert_eq!(stroke_font_from_id(9999), StrokeFont::Default);
    }

    #[test]
    fn flag_word_decoding_inverts_the_unlocked_bit() {
        // A cleared "unlocked" bit means locked, so a primitive whose header
        // is too short to carry the word must not read as locked by accident.
        assert_eq!(read_flags(&[0, 0]), PcbFlags::empty());
        assert!(read_flags(&[0, 0, 0]).contains(PcbFlags::LOCKED));

        let with = |bits: u16| {
            let [lo, hi] = bits.to_le_bytes();
            read_flags(&[0, lo, hi])
        };
        assert!(!with(ALT_FLAG_UNLOCKED).contains(PcbFlags::LOCKED));
        assert!(with(ALT_FLAG_UNLOCKED | ALT_FLAG_KEEPOUT).contains(PcbFlags::KEEPOUT));
        assert!(with(ALT_FLAG_UNLOCKED | ALT_FLAG_TENTING_TOP).contains(PcbFlags::TENTING_TOP));
        assert!(
            with(ALT_FLAG_UNLOCKED | ALT_FLAG_TENTING_BOTTOM).contains(PcbFlags::TENTING_BOTTOM)
        );
        assert!(with(ALT_FLAG_UNLOCKED | ALT_FLAG_TESTPOINT_TOP).contains(PcbFlags::TESTPOINT_TOP));
        assert!(with(ALT_FLAG_UNLOCKED | ALT_FLAG_TESTPOINT_BOTTOM)
            .contains(PcbFlags::TESTPOINT_BOTTOM));
    }

    #[test]
    fn unique_id_stream_stops_at_a_corrupt_length_prefix() {
        // The stream is a run of length-prefixed records, so a wrong length
        // would run the parser off the end or into the next record. It stops
        // instead, keeping whatever it had already read.
        let record = |body: &str| {
            let len = u32::try_from(body.len()).expect("test record fits in u32");
            let mut out = len.to_le_bytes().to_vec();
            out.extend_from_slice(body.as_bytes());
            out
        };

        let good = "|PRIMITIVEINDEX=1|PRIMITIVEOBJECTID=Pad|UNIQUEID=QHHMRSCB";
        let parsed = parse_unique_id_stream(&record(good));
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].primitive_index, 1);
        assert_eq!(parsed[0].unique_id, "QHHMRSCB");

        // A length running past the end of the buffer.
        let mut overrun = record(good);
        overrun[0] = 0xFF;
        assert!(parse_unique_id_stream(&overrun).is_empty());

        // A zero length, and one past the 10 kB sanity cap.
        assert!(parse_unique_id_stream(&0_u32.to_le_bytes()).is_empty());
        assert!(parse_unique_id_stream(&20_000_u32.to_le_bytes()).is_empty());

        // A trailing partial prefix after a good record keeps the good one.
        let mut trailing = record(good);
        trailing.extend_from_slice(&[1, 2, 3]);
        assert_eq!(parse_unique_id_stream(&trailing).len(), 1);

        // A record whose content is not a usable record is skipped without
        // stopping the scan.
        let mut mixed = record("|NOTHING=USEFUL");
        mixed.extend_from_slice(&record(good));
        assert_eq!(parse_unique_id_stream(&mixed).len(), 1);

        // So is a record that is not UTF-8.
        let mut binary = 2_u32.to_le_bytes().to_vec();
        binary.extend_from_slice(&[0xFF, 0xFE]);
        binary.extend_from_slice(&record(good));
        assert_eq!(parse_unique_id_stream(&binary).len(), 1);
    }

    /// A GUID record is attached only when its kind is known, its ordinal
    /// lies within the footprint's write sequence and the kind there agrees;
    /// kind 85 names the footprint itself.
    #[test]
    fn primitive_guids_attach_only_where_kind_and_ordinal_agree() {
        use crate::altium::pcblib::{Pad, PrimitiveGuid, PrimitiveKind};

        let mut fp = Footprint::new("FP");
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        fp.add_pad(Pad::smd("2", 2.0, 0.0, 1.0, 1.0));
        let record = |object_kind: u32, index: u32, guid: &str| PrimitiveGuid {
            object_kind,
            index,
            guid: guid.to_string(),
        };
        let records = [
            record(85, 0, "{FOOTPRINT}"),
            record(99, 0, "{UNKNOWN-KIND}"),
            record(
                PrimitiveKind::Track.altium_object_id(),
                0,
                "{KIND-MISMATCH}",
            ),
            record(
                PrimitiveKind::Pad.altium_object_id(),
                7,
                "{BEYOND-SEQUENCE}",
            ),
            record(PrimitiveKind::Pad.altium_object_id(), 1, "{PAD-2}"),
        ];
        apply_primitive_guids(&mut fp, &records);
        assert_eq!(fp.guid.as_deref(), Some("{FOOTPRINT}"));
        assert_eq!(
            fp.pads[0].guid, None,
            "the mismatched and out-of-range records"
        );
        assert_eq!(fp.pads[1].guid.as_deref(), Some("{PAD-2}"));
    }

    /// A unique ID names a text primitive by its ordinal like any other.
    #[test]
    fn unique_ids_reach_text_primitives() {
        use crate::altium::pcblib::{Layer, Text};

        let mut fp = Footprint::new("FP");
        fp.add_text(Text::new(0.0, 0.0, "T", 1.0, Layer::TopOverlay));
        let ids = vec![UniqueIdEntry {
            primitive_index: 0,
            primitive_type: "Text".to_string(),
            unique_id: "TEXTUID1".to_string(),
        }];
        apply_unique_ids(&mut fp, &ids);
        assert_eq!(fp.text[0].unique_id.as_deref(), Some("TEXTUID1"));
    }

    #[test]
    fn wide_strings_skips_entries_it_cannot_decode() {
        // Malformed entries have to be dropped individually: one bad index
        // must not cost the caller the rest of the table.
        let stream =
            b"|ENCODEDTEXT0=84,69,83,84|ENCODEDTEXTx=65|ENCODEDTEXT2=|ENCODEDTEXT5|NOTENCODED=1|";
        let parsed = parse_wide_strings(stream);
        assert_eq!(parsed.get(&0).map(String::as_str), Some("TEST"));
        // A non-numeric index, an empty payload, an entry with no `=` and an
        // unrelated key all drop.
        assert_eq!(parsed.len(), 1, "{parsed:?}");
    }

    #[test]
    fn test_hole_shape_from_id() {
        assert_eq!(hole_shape_from_id(0), HoleShape::Round);
        assert_eq!(hole_shape_from_id(1), HoleShape::Square);
        assert_eq!(hole_shape_from_id(2), HoleShape::Slot);
        // Unknown IDs should default to Round
        assert_eq!(hole_shape_from_id(255), HoleShape::Round);
    }

    #[test]
    fn test_parse_wide_strings() {
        // Test basic WideStrings parsing
        let data = b"|ENCODEDTEXT0=84,69,83,84|ENCODEDTEXT1=72,69,76,76,79|";
        let strings = parse_wide_strings(data);

        assert_eq!(strings.len(), 2);
        assert_eq!(strings.get(&0), Some(&"TEST".to_string()));
        assert_eq!(strings.get(&1), Some(&"HELLO".to_string()));
    }

    #[test]
    fn test_parse_wide_strings_empty() {
        let data = b"";
        let strings = parse_wide_strings(data);
        assert!(strings.is_empty());
    }

    #[test]
    fn test_parse_wide_strings_single() {
        let data = b"|ENCODEDTEXT0=65,66,67|";
        let strings = parse_wide_strings(data);

        assert_eq!(strings.len(), 1);
        assert_eq!(strings.get(&0), Some(&"ABC".to_string()));
    }

    #[test]
    fn decode_wide_string_reads_ascii() {
        assert_eq!(decode_wide_string("84,69,83,84"), "TEST");
        assert_eq!(decode_wide_string("72,69,76,76,79"), "HELLO");
        assert_eq!(decode_wide_string("65"), "A");
        assert_eq!(decode_wide_string(""), "");
    }

    /// The values are UTF-16 code units, not bytes: Altium writes `10µF` as
    /// `49,48,181,70` (the golden `TEXT_WIN1252`), and a character beyond
    /// Latin-1 is one unit above 255 — which is the whole point of the stream.
    #[test]
    fn decode_wide_string_reads_utf16_code_units() {
        assert_eq!(decode_wide_string("49,48,181,70"), "10\u{B5}F");
        assert_eq!(decode_wide_string("177,53,37"), "\u{B1}5%");
        assert_eq!(decode_wide_string("937"), "\u{3A9}");
        assert_eq!(decode_wide_string("8364"), "\u{20AC}");
        assert_eq!(decode_wide_string("26085,26412"), "日本");
        // A surrogate pair is two units; a lone one is replaced, not dropped.
        assert_eq!(decode_wide_string("55357,56832"), "\u{1F600}");
        assert_eq!(decode_wide_string("55357"), "\u{FFFD}");
        // Unit 128 is U+0080 (a control), not the Windows-1252 Euro: the
        // Euro is 8364 here.
        assert_eq!(decode_wide_string("128"), "\u{80}");
        // A value that is not a unit is skipped rather than failing the text.
        assert_eq!(decode_wide_string("65,x,66,70000"), "AB");
    }

    // =============================================================================
    // 3D Model Parsing Tests
    // =============================================================================

    #[test]
    #[allow(clippy::cast_possible_truncation)] // Test data lengths always fit in u32
    fn test_parse_model_data_stream() {
        // Build test data in the actual Altium format:
        // [record_len:4 LE][pipe-delimited params][null:1]
        let record1 = b"EMBED=TRUE|ID={GUID-1234}|NAME=model1.step|CHECKSUM=123";
        let record2 = b"EMBED=TRUE|ID={GUID-5678}|NAME=model2.step|CHECKSUM=456";

        let mut data = Vec::new();

        // Record 1
        data.extend_from_slice(&(record1.len() as u32).to_le_bytes());
        data.extend_from_slice(record1);
        data.push(0x00); // null terminator

        // Record 2
        data.extend_from_slice(&(record2.len() as u32).to_le_bytes());
        data.extend_from_slice(record2);
        data.push(0x00); // null terminator

        let index = parse_model_data_stream(&data);

        assert_eq!(index.len(), 2);
        assert_eq!(
            index.get("{GUID-1234}"),
            Some(&(0, "model1.step".to_string()))
        );
        assert_eq!(
            index.get("{GUID-5678}"),
            Some(&(1, "model2.step".to_string()))
        );
    }

    #[test]
    fn test_parse_model_data_stream_empty() {
        let data: [u8; 0] = [];
        let index = parse_model_data_stream(&data);
        assert!(index.is_empty());
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)] // Test data lengths always fit in u32
    fn test_parse_model_data_stream_single() {
        // Single record with length prefix
        let record = b"ID={ABC-DEF}|NAME=test.step";

        let mut data = Vec::new();
        data.extend_from_slice(&(record.len() as u32).to_le_bytes());
        data.extend_from_slice(record);
        data.push(0x00);

        let index = parse_model_data_stream(&data);

        assert_eq!(index.len(), 1);
        assert_eq!(index.get("{ABC-DEF}"), Some(&(0, "test.step".to_string())));
    }

    #[test]
    fn test_parse_model_header_stream() {
        // Header is a 4-byte LE u32 containing the model count
        let data: [u8; 4] = [0x03, 0x00, 0x00, 0x00]; // 3 in little-endian
        let count = parse_model_header_stream(&data);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_parse_model_header_stream_empty() {
        let data: [u8; 0] = [];
        let count = parse_model_header_stream(&data);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_parse_model_header_stream_short() {
        // Data too short (less than 4 bytes)
        let data: [u8; 2] = [0x03, 0x00];
        let count = parse_model_header_stream(&data);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_decompress_model_data() {
        // Compress some test data
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let original = b"ISO-10303-21; HEADER; FILE_DESCRIPTION...";
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();

        // Decompress it
        let decompressed = super::models::decompress_model_data(&compressed);

        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_decompress_model_data_empty() {
        let data = b"";
        let result = super::models::decompress_model_data(data);
        assert!(result.is_empty());
    }

    #[test]
    fn test_decompress_capped_rejects_bomb() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        // Highly compressible data that decompresses well past a small cap: a
        // tiny compressed stream expanding to far more output (a bomb).
        let max = 1024;
        let huge = vec![0u8; max * 64];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&huge).unwrap();
        let compressed = encoder.finish().unwrap();
        assert!(compressed.len() < huge.len(), "test data should be a bomb");

        // Over the cap -> rejected (empty).
        assert!(super::models::decompress_capped(&compressed, max).is_empty());
    }

    #[test]
    fn test_decompress_capped_allows_within_limit() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let original = vec![0xABu8; 500];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&original).unwrap();
        let compressed = encoder.finish().unwrap();

        // Exactly at/under the cap -> returned intact.
        assert_eq!(
            super::models::decompress_capped(&compressed, 1024),
            original
        );
    }

    #[test]
    fn test_decompress_model_data_invalid() {
        let data = b"not valid zlib data";
        let result = super::models::decompress_model_data(data);
        assert!(result.is_empty()); // Should return empty on error
    }

    #[test]
    fn test_parse_embedded_models() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        // Create mock model index with (index, name) tuples
        let mut model_index = ModelIndex::new();
        model_index.insert("{GUID-A}".to_string(), (0, "model_a.step".to_string()));
        model_index.insert("{GUID-B}".to_string(), (1, "model_b.step".to_string()));

        // Create compressed model data
        let step_data_a = b"STEP model A content";
        let step_data_b = b"STEP model B content";

        let mut encoder_a = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder_a.write_all(step_data_a).unwrap();
        let compressed_a = encoder_a.finish().unwrap();

        let mut encoder_b = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder_b.write_all(step_data_b).unwrap();
        let compressed_b = encoder_b.finish().unwrap();

        let model_data = vec![(0, compressed_a), (1, compressed_b)];

        // Parse models
        let models = parse_embedded_models(&model_index, &model_data);

        assert_eq!(models.len(), 2);

        // Find model A
        let model_a = models.iter().find(|m| m.id == "{GUID-A}").unwrap();
        assert_eq!(model_a.data, step_data_a);
        assert_eq!(model_a.name, "model_a.step");

        // Find model B
        let model_b = models.iter().find(|m| m.id == "{GUID-B}").unwrap();
        assert_eq!(model_b.data, step_data_b);
        assert_eq!(model_b.name, "model_b.step");
    }
}
