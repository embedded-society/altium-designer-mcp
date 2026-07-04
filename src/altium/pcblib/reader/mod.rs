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
//! ...
//! [0x00]                                       // End marker
//! ```
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
    Arc, ComponentBody, Fill, HoleShape, Layer, MaskExpansionMode, Pad, PadShape, PadStackMode,
    PcbFlags, PowerPlaneConnectStyle, Region, RegionKind, StrokeFont, Text, TextJustification,
    TextKind, Track, Vertex, Via, ViaStackMode,
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
#[allow(clippy::wildcard_imports)] // tightly-coupled reader split
use parsers::*;

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

    // WideStrings is pipe-delimited key=value pairs
    let Ok(text) = String::from_utf8(data.to_vec()) else {
        tracing::debug!("WideStrings stream is not valid UTF-8");
        return strings;
    };

    for pair in text.split('|') {
        if pair.is_empty() {
            continue;
        }

        // Look for ENCODEDTEXT{N}=...
        if let Some(rest) = pair.strip_prefix("ENCODEDTEXT") {
            if let Some((index_str, encoded)) = rest.split_once('=') {
                if let Ok(index) = index_str.parse::<usize>() {
                    // Decode comma-separated ASCII codes
                    let decoded = decode_ascii_codes(encoded);
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

/// Decodes comma-separated ASCII codes to a string.
///
/// # Example
///
/// `"84,69,83,84"` → `"TEST"`
///
/// # Non-ASCII Handling
///
/// Values 0-127 are valid ASCII and converted directly.
/// Values 128-255 are replaced with the Unicode replacement character (U+FFFD)
/// since Altium's ENCODEDTEXT format should only contain ASCII.
fn decode_ascii_codes(encoded: &str) -> String {
    encoded
        .split(',')
        .filter_map(|s| s.trim().parse::<u8>().ok())
        .map(|c| {
            if c.is_ascii() {
                c as char
            } else {
                // Non-ASCII byte - use replacement character and log warning
                tracing::warn!(
                    byte = c,
                    "Non-ASCII byte in ENCODEDTEXT, replacing with U+FFFD"
                );
                '\u{FFFD}'
            }
        })
        .collect()
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

    while offset + 4 <= data.len() {
        // Read 4-byte little-endian length
        let Some(record_len) = read_u32(data, offset) else {
            break;
        };
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
    let unique_id = params.get("UNIQUEID")?.clone();
    // Only return if all required fields are present and the id is non-empty.
    (!unique_id.is_empty()).then_some(UniqueIdEntry {
        primitive_index,
        primitive_type,
        unique_id,
    })
}

/// Applies unique IDs from the `UniqueIDPrimitiveInformation` stream to footprint primitives.
///
/// `PRIMITIVEINDEX` is a single global 0-based ordinal over all primitives in
/// `Data`-stream emit order (Arc, Pad, Via, Track, Text, Region, Fill,
/// `ComponentBody`) — NOT a per-type index. This walks that exact order, mirroring
/// `encode_unique_id_stream`, so a written-then-read footprint round-trips.
///
/// # Arguments
///
/// * `footprint` - The footprint to update with unique IDs
/// * `unique_ids` - The parsed unique ID map from `parse_unique_id_stream`
pub fn apply_unique_ids(footprint: &mut Footprint, unique_ids: &UniqueIdMap) {
    // Map global ordinal -> (type, uid). Type is kept only to disambiguate a foreign
    // file whose ordinal base doesn't line up: we skip rather than mis-attach.
    let mut by_ordinal: HashMap<usize, (&str, &str)> = HashMap::new();
    for entry in unique_ids {
        by_ordinal.insert(
            entry.primitive_index,
            (entry.primitive_type.as_str(), entry.unique_id.as_str()),
        );
    }

    let mut ordinal = 0usize;
    macro_rules! apply {
        ($iter:expr, $ty:literal) => {
            for prim in $iter {
                if let Some(&(ty, uid)) = by_ordinal.get(&ordinal) {
                    if ty == $ty {
                        prim.unique_id = Some(uid.to_string());
                    }
                }
                ordinal += 1;
            }
        };
    }
    apply!(footprint.arcs.iter_mut(), "Arc");
    apply!(footprint.pads.iter_mut(), "Pad");
    apply!(footprint.vias.iter_mut(), "Via");
    apply!(footprint.tracks.iter_mut(), "Track");
    apply!(footprint.text.iter_mut(), "Text");
    apply!(footprint.regions.iter_mut(), "Region");
    apply!(footprint.fills.iter_mut(), "Fill");
    apply!(footprint.component_bodies.iter_mut(), "ComponentBody");

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
    crate::altium::framing::read_pascal_string(block, 0).0
}

// Flag bits shared with the writer via `super::flags`.
use super::flags::{
    ALT_FLAG_KEEPOUT, ALT_FLAG_TENTING_BOTTOM, ALT_FLAG_TENTING_TOP, ALT_FLAG_UNLOCKED,
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
    if bits & ALT_FLAG_KEEPOUT != 0 {
        flags |= PcbFlags::KEEPOUT;
    }
    flags
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
const fn layer_from_id(id: u8) -> Layer {
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
    if data.len() < 5 {
        tracing::warn!("Data stream too short");
        return;
    }

    // Read name block: [block_len:4][str_len:1][name:str_len]
    let Some(name_block_len) = read_u32(data, 0) else {
        tracing::warn!("Failed to read name block length");
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
    fn test_decode_ascii_codes() {
        assert_eq!(decode_ascii_codes("84,69,83,84"), "TEST");
        assert_eq!(decode_ascii_codes("72,69,76,76,79"), "HELLO");
        assert_eq!(decode_ascii_codes("65"), "A");
        assert_eq!(decode_ascii_codes(""), "");
    }

    #[test]
    fn test_decode_ascii_codes_non_ascii() {
        // Non-ASCII bytes (128-255) should be replaced with U+FFFD
        assert_eq!(decode_ascii_codes("65,200,66"), "A\u{FFFD}B");
        assert_eq!(decode_ascii_codes("255"), "\u{FFFD}");
        // Boundary: 127 is still ASCII
        assert_eq!(decode_ascii_codes("127"), "\x7F");
        // Boundary: 128 is non-ASCII
        assert_eq!(decode_ascii_codes("128"), "\u{FFFD}");
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
