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
    Arc, ComponentBody, Fill, HoleShape, Layer, Pad, PadShape, PadStackMode, PcbFlags, Region,
    StrokeFont, Text, TextJustification, TextKind, Track, Vertex, Via, ViaStackMode,
};
use super::Footprint;
use crate::altium::error::AltiumError;

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
    /// Primitive index (1-indexed, as stored in Altium files).
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

        // Parse the pipe-delimited record
        if let Ok(record_str) = String::from_utf8(record_data.to_vec()) {
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
    let mut primitive_index: Option<usize> = None;
    let mut primitive_type: Option<String> = None;
    let mut unique_id: Option<String> = None;

    for pair in record.split('|') {
        if pair.is_empty() {
            continue;
        }

        if let Some((key, value)) = pair.split_once('=') {
            match key {
                "PRIMITIVEINDEX" => {
                    primitive_index = value.parse().ok();
                }
                "PRIMITIVEOBJECTID" => {
                    primitive_type = Some(value.to_string());
                }
                "UNIQUEID" => {
                    unique_id = Some(value.to_string());
                }
                _ => {}
            }
        }
    }

    // Only return if we have all required fields
    match (primitive_index, primitive_type, unique_id) {
        (Some(index), Some(ptype), Some(uid)) if !uid.is_empty() => Some(UniqueIdEntry {
            primitive_index: index,
            primitive_type: ptype,
            unique_id: uid,
        }),
        _ => None,
    }
}

/// Applies unique IDs from the `UniqueIDPrimitiveInformation` stream to footprint primitives.
///
/// This function assigns unique IDs to primitives based on their type and index.
/// The index is 1-based in the Altium format, and represents the order of primitives
/// within each type (e.g., the 3rd Pad is index 3, regardless of Tracks in between).
///
/// # Arguments
///
/// * `footprint` - The footprint to update with unique IDs
/// * `unique_ids` - The parsed unique ID map from `parse_unique_id_stream`
pub fn apply_unique_ids(footprint: &mut Footprint, unique_ids: &UniqueIdMap) {
    // Build lookup by (type, index) for efficient assignment
    let mut lookup: HashMap<(&str, usize), &str> = HashMap::new();
    for entry in unique_ids {
        lookup.insert(
            (entry.primitive_type.as_str(), entry.primitive_index),
            entry.unique_id.as_str(),
        );
    }

    // The PRIMITIVEINDEX appears to be 1-indexed and sequential within each primitive type
    // Apply unique IDs to each primitive type

    // Pads
    for (i, pad) in footprint.pads.iter_mut().enumerate() {
        if let Some(&uid) = lookup.get(&("Pad", i + 1)) {
            pad.unique_id = Some(uid.to_string());
        }
    }

    // Vias
    for (i, via) in footprint.vias.iter_mut().enumerate() {
        if let Some(&uid) = lookup.get(&("Via", i + 1)) {
            via.unique_id = Some(uid.to_string());
        }
    }

    // Tracks
    for (i, track) in footprint.tracks.iter_mut().enumerate() {
        if let Some(&uid) = lookup.get(&("Track", i + 1)) {
            track.unique_id = Some(uid.to_string());
        }
    }

    // Arcs
    for (i, arc) in footprint.arcs.iter_mut().enumerate() {
        if let Some(&uid) = lookup.get(&("Arc", i + 1)) {
            arc.unique_id = Some(uid.to_string());
        }
    }

    // Regions
    for (i, region) in footprint.regions.iter_mut().enumerate() {
        if let Some(&uid) = lookup.get(&("Region", i + 1)) {
            region.unique_id = Some(uid.to_string());
        }
    }

    // Text
    for (i, text) in footprint.text.iter_mut().enumerate() {
        if let Some(&uid) = lookup.get(&("Text", i + 1)) {
            text.unique_id = Some(uid.to_string());
        }
    }

    // Fills
    for (i, fill) in footprint.fills.iter_mut().enumerate() {
        if let Some(&uid) = lookup.get(&("Fill", i + 1)) {
            fill.unique_id = Some(uid.to_string());
        }
    }

    // ComponentBodies
    for (i, body) in footprint.component_bodies.iter_mut().enumerate() {
        if let Some(&uid) = lookup.get(&("ComponentBody", i + 1)) {
            body.unique_id = Some(uid.to_string());
        }
    }

    tracing::trace!(
        footprint = %footprint.name,
        "Applied unique IDs to primitives"
    );
}

/// Conversion factor from Altium internal units to millimetres.
/// Internal units: 10000 = 1 mil = 0.0254 mm
const INTERNAL_UNITS_TO_MM: f64 = 0.0254 / 10000.0;

/// Converts Altium internal units to millimetres.
/// Rounds to 6 decimal places (1nm resolution) to avoid floating-point noise.
fn to_mm(internal: i32) -> f64 {
    let raw = f64::from(internal) * INTERNAL_UNITS_TO_MM;
    // Round to 6 decimal places (1nm = 0.000001mm) to avoid precision artifacts
    (raw * 1_000_000.0).round() / 1_000_000.0
}

/// Reads a 4-byte little-endian unsigned integer.
fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    if offset + 4 > data.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

/// Reads a 4-byte little-endian signed integer.
fn read_i32(data: &[u8], offset: usize) -> Option<i32> {
    if offset + 4 > data.len() {
        return None;
    }
    Some(i32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

/// Reads an 8-byte little-endian double (IEEE 754).
fn read_f64(data: &[u8], offset: usize) -> Option<f64> {
    if offset + 8 > data.len() {
        return None;
    }
    Some(f64::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ]))
}

/// Reads a length-prefixed block from data.
/// Returns the block data and the new offset.
fn read_block(data: &[u8], offset: usize) -> Option<(&[u8], usize)> {
    let block_len = read_u32(data, offset)? as usize;
    if block_len > 100_000 || offset + 4 + block_len > data.len() {
        return None;
    }
    Some((
        &data[offset + 4..offset + 4 + block_len],
        offset + 4 + block_len,
    ))
}

/// Reads a length-prefixed string from block data.
fn read_string_from_block(block: &[u8]) -> String {
    if block.is_empty() {
        return String::new();
    }
    let str_len = block[0] as usize;
    if str_len + 1 > block.len() {
        return String::new();
    }
    // Decode as UTF-8 with lossy replacement for invalid sequences
    String::from_utf8_lossy(&block[1..=str_len]).to_string()
}

/// Reads PCB flags from the common header bytes 1-2.
fn read_flags(data: &[u8]) -> PcbFlags {
    if data.len() < 3 {
        return PcbFlags::empty();
    }
    let bits = u16::from_le_bytes([data[1], data[2]]);
    PcbFlags::from_bits_truncate(bits)
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
        // Unknown layers default to MultiLayer
        _ => Layer::MultiLayer,
    }
}

/// Converts Altium pad shape ID to our `PadShape` enum.
const fn pad_shape_from_id(id: u8) -> PadShape {
    match id {
        1 => PadShape::Round,
        2 => PadShape::Rectangle,
        3 => PadShape::Oval, // Octagon maps to Oval as closest match
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

/// Converts Altium stroke font ID to our `StrokeFont` enum.
///
/// Stroke font IDs (from geometry block bytes 25-26 as u16):
/// - 0: Default
/// - 1: Sans-Serif
/// - 2: Serif
const fn stroke_font_from_id(id: u16) -> StrokeFont {
    match id {
        1 => StrokeFont::SansSerif,
        2 => StrokeFont::Serif,
        _ => StrokeFont::Default, // Default and ID 0
    }
}

/// Converts Altium text justification ID to our `TextJustification` enum.
///
/// Justification IDs form a 3x3 grid:
/// - 0: `BottomLeft`
/// - 1: `BottomCenter`
/// - 2: `BottomRight`
/// - 3: `MiddleLeft`
/// - 4: `MiddleCenter`
/// - 5: `MiddleRight`
/// - 6: `TopLeft`
/// - 7: `TopCenter`
/// - 8: `TopRight`
const fn justification_from_id(id: u8) -> TextJustification {
    match id {
        0 => TextJustification::BottomLeft,
        1 => TextJustification::BottomCenter,
        2 => TextJustification::BottomRight,
        3 => TextJustification::MiddleLeft,
        5 => TextJustification::MiddleRight,
        6 => TextJustification::TopLeft,
        7 => TextJustification::TopCenter,
        8 => TextJustification::TopRight,
        _ => TextJustification::MiddleCenter, // Default and ID 4
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

/// Parses a Pad primitive.
/// Returns the parsed `Pad` and the new offset on success.
///
/// # Geometry Block Offsets
///
/// | Offset | Size | Field |
/// |--------|------|-------|
/// | 0-12 | 13 | Common header (layer, flags, padding) |
/// | 13-16 | 4 | X position |
/// | 17-20 | 4 | Y position |
/// | 21-24 | 4 | Width (top) |
/// | 25-28 | 4 | Height (top) |
/// | 29-32 | 4 | Width (mid) |
/// | 33-36 | 4 | Height (mid) |
/// | 37-40 | 4 | Width (bottom) |
/// | 41-44 | 4 | Height (bottom) |
/// | 45-48 | 4 | Hole size |
/// | 49 | 1 | Shape (top) |
/// | 50 | 1 | Shape (mid) |
/// | 51 | 1 | Shape (bottom) |
/// | 52-59 | 8 | Rotation (double) |
/// | 60 | 1 | Is plated |
/// | 61 | 1 | Hole shape |
/// | 62 | 1 | Stack mode |
/// | 86-89 | 4 | Paste mask expansion |
/// | 90-93 | 4 | Solder mask expansion |
/// | 101 | 1 | Paste mask expansion manual |
/// | 102 | 1 | Solder mask expansion manual |
#[allow(clippy::too_many_lines)] // Complex binary format requires detailed parsing
fn parse_pad(data: &[u8], offset: usize) -> ParseResult<Pad> {
    let mut current = offset;

    // Block 0: Designator string
    let (block0, next) = read_block(data, current).ok_or_else(|| {
        AltiumError::parse_error(offset, "failed to read Pad block 0 (designator)")
    })?;
    let designator = read_string_from_block(block0);
    current = next;

    // Block 1: Unknown (skip)
    let (_, next) = read_block(data, current)
        .ok_or_else(|| AltiumError::parse_error(current, "failed to read Pad block 1"))?;
    current = next;

    // Block 2: Unknown string ("|&|0")
    let (_, next) = read_block(data, current)
        .ok_or_else(|| AltiumError::parse_error(current, "failed to read Pad block 2"))?;
    current = next;

    // Block 3: Unknown (skip)
    let (_, next) = read_block(data, current)
        .ok_or_else(|| AltiumError::parse_error(current, "failed to read Pad block 3"))?;
    current = next;

    // Block 4: Geometry data
    let (geometry, next) = read_block(data, current).ok_or_else(|| {
        AltiumError::parse_error(current, "failed to read Pad block 4 (geometry)")
    })?;
    current = next;

    // Block 5: Per-layer data (optional, may contain corner radius)
    let per_layer_data = if let Some((block, next)) = read_block(data, current) {
        current = next;
        Some(block)
    } else {
        None
    };

    // Parse geometry block
    if geometry.len() < 52 {
        return Err(AltiumError::parse_error(
            offset,
            format!(
                "Pad geometry block too short: {} bytes, expected at least 52",
                geometry.len()
            ),
        ));
    }

    // Common header (13 bytes)
    let layer_id = geometry[0];
    let layer = layer_from_id(layer_id);
    let flags = read_flags(geometry);

    // Location (X, Y) - offsets 13-20
    let x =
        to_mm(read_i32(geometry, 13).ok_or_else(|| {
            AltiumError::parse_error(offset + 13, "failed to read Pad x coordinate")
        })?);
    let y =
        to_mm(read_i32(geometry, 17).ok_or_else(|| {
            AltiumError::parse_error(offset + 17, "failed to read Pad y coordinate")
        })?);

    // Size top (X, Y) - offsets 21-28
    let size_top_x = to_mm(
        read_i32(geometry, 21)
            .ok_or_else(|| AltiumError::parse_error(offset + 21, "failed to read Pad width"))?,
    );
    let size_top_y = to_mm(
        read_i32(geometry, 25)
            .ok_or_else(|| AltiumError::parse_error(offset + 25, "failed to read Pad height"))?,
    );

    // Use top size for width/height
    let width = size_top_x;
    let height = size_top_y;

    // Hole size - offset 45
    let hole_size = if geometry.len() > 48 {
        read_i32(geometry, 45)
            .map(to_mm)
            .filter(|&hole| hole > 0.001)
    } else {
        None
    };

    // Shape - offset 49
    let shape = if geometry.len() > 49 {
        pad_shape_from_id(geometry[49])
    } else {
        PadShape::RoundedRectangle
    };

    // Rotation - offset 52 (8-byte double)
    let rotation = if geometry.len() > 59 {
        read_f64(geometry, 52).unwrap_or(0.0)
    } else {
        0.0
    };

    // Hole shape - offset 61
    let hole_shape = if geometry.len() > 61 {
        hole_shape_from_id(geometry[61])
    } else {
        HoleShape::Round
    };

    // Stack mode - offset 62
    let stack_mode = if geometry.len() > 62 {
        pad_stack_mode_from_id(geometry[62])
    } else {
        PadStackMode::Simple
    };

    // Paste mask expansion - offset 86-89
    let paste_mask_expansion = if geometry.len() > 89 {
        read_i32(geometry, 86)
            .map(to_mm)
            .filter(|&expansion| expansion.abs() > 0.0001)
    } else {
        None
    };

    // Solder mask expansion - offset 90-93
    let solder_mask_expansion = if geometry.len() > 93 {
        read_i32(geometry, 90)
            .map(to_mm)
            .filter(|&expansion| expansion.abs() > 0.0001)
    } else {
        None
    };

    // Paste mask expansion manual flag - offset 101
    let paste_mask_expansion_manual = geometry.len() > 101 && geometry[101] != 0;

    // Solder mask expansion manual flag - offset 102
    let solder_mask_expansion_manual = geometry.len() > 102 && geometry[102] != 0;

    // Parse per-layer data when stack mode is not Simple
    // Per-layer data format:
    // - 32 size entries (width, height as i32 pairs) = 256 bytes
    // - 32 shape entries (1 byte each) = 32 bytes
    // - 32 corner radius percentages (1 byte each) = 32 bytes
    // - 32 offset entries (x, y as i32 pairs) = 256 bytes (optional)
    // Total: 320 bytes minimum, 576 bytes with offsets
    let (
        corner_radius_percent,
        per_layer_sizes,
        per_layer_shapes,
        per_layer_corner_radii,
        per_layer_offsets,
    ) = if stack_mode == PadStackMode::Simple {
        // For Simple mode, extract corner radius if available (backwards compatibility)
        let corner_radius = per_layer_data.and_then(|data| {
            if data.len() > 288 {
                let radius = data[288];
                if radius > 0 && radius <= 100 {
                    Some(radius)
                } else {
                    None
                }
            } else {
                None
            }
        });
        (corner_radius, None, None, None, None)
    } else {
        parse_per_layer_data(per_layer_data)
    };

    // Adjust shape based on corner radius: if shape is Round but corner_radius is set,
    // it's actually RoundedRectangle (both use shape ID 1 in Altium's binary format)
    let adjusted_shape =
        if shape == PadShape::Round && corner_radius_percent.is_some_and(|r| r > 0 && r < 100) {
            PadShape::RoundedRectangle
        } else {
            shape
        };

    let pad = Pad {
        designator,
        x,
        y,
        width,
        height,
        shape: adjusted_shape,
        layer,
        hole_size,
        hole_shape,
        rotation,
        paste_mask_expansion,
        solder_mask_expansion,
        paste_mask_expansion_manual,
        solder_mask_expansion_manual,
        corner_radius_percent,
        stack_mode,
        per_layer_sizes,
        per_layer_shapes,
        per_layer_corner_radii,
        per_layer_offsets,
        flags,
        unique_id: None,
    };

    Ok((pad, current))
}

/// Parses per-layer pad data from Block 5.
///
/// # Format
///
/// ```text
/// [sizes: 32 × 8 bytes]         // 32 width/height pairs as i32
/// [shapes: 32 × 1 byte]         // 32 shape IDs
/// [corner_radii: 32 × 1 byte]   // 32 corner radius percentages (0-100)
/// [offsets: 32 × 8 bytes]       // 32 x/y offset pairs as i32 (optional)
/// ```
///
/// # Returns
///
/// Tuple of (`corner_radius_percent`, sizes, shapes, `corner_radii`, offsets).
#[allow(clippy::type_complexity)]
fn parse_per_layer_data(
    data: Option<&[u8]>,
) -> (
    Option<u8>,
    Option<Vec<(f64, f64)>>,
    Option<Vec<PadShape>>,
    Option<Vec<u8>>,
    Option<Vec<(f64, f64)>>,
) {
    let Some(data) = data else {
        return (None, None, None, None, None);
    };

    // Minimum size: 256 (sizes) + 32 (shapes) + 32 (corner radii) = 320 bytes
    if data.len() < 320 {
        tracing::trace!(
            "Per-layer data block too short: {} bytes (expected >= 320)",
            data.len()
        );
        return (None, None, None, None, None);
    }

    // Parse 32 size entries (256 bytes)
    let mut sizes = Vec::with_capacity(32);
    for i in 0..32 {
        let offset = i * 8;
        if let (Some(width), Some(height)) = (read_i32(data, offset), read_i32(data, offset + 4)) {
            sizes.push((to_mm(width), to_mm(height)));
        } else {
            sizes.push((0.0, 0.0));
        }
    }

    // Parse 32 corner radius entries (32 bytes, starting at offset 288)
    // Parse corner radii first so we can use them to determine shapes
    let mut corner_radii = Vec::with_capacity(32);
    for i in 0..32 {
        let radius = data[288 + i];
        corner_radii.push(radius.min(100)); // Clamp to 0-100
    }

    // Parse 32 shape entries (32 bytes, starting at offset 256)
    // Use corner radius to distinguish between Round and RoundedRectangle
    // since both use shape ID 1 in Altium's binary format
    let mut shapes = Vec::with_capacity(32);
    for i in 0..32 {
        let shape_id = data[256 + i];
        let shape = pad_shape_from_id(shape_id);
        // If shape ID is 1 (Round) but corner radius is < 100%, it's RoundedRectangle
        let adjusted_shape =
            if shape == PadShape::Round && corner_radii[i] > 0 && corner_radii[i] < 100 {
                PadShape::RoundedRectangle
            } else {
                shape
            };
        shapes.push(adjusted_shape);
    }

    // Extract corner radius percent from first layer (top layer, index 0)
    let corner_radius_percent = if corner_radii[0] > 0 && corner_radii[0] <= 100 {
        Some(corner_radii[0])
    } else {
        None
    };

    // Parse 32 offset entries (256 bytes, starting at offset 320) if available
    let offsets = if data.len() >= 576 {
        let mut offs = Vec::with_capacity(32);
        for i in 0..32 {
            let offset = 320 + i * 8;
            if let (Some(x), Some(y)) = (read_i32(data, offset), read_i32(data, offset + 4)) {
                offs.push((to_mm(x), to_mm(y)));
            } else {
                offs.push((0.0, 0.0));
            }
        }
        Some(offs)
    } else {
        None
    };

    (
        corner_radius_percent,
        Some(sizes),
        Some(shapes),
        Some(corner_radii),
        offsets,
    )
}

/// Converts a pad stack mode ID to `PadStackMode`.
const fn pad_stack_mode_from_id(id: u8) -> PadStackMode {
    match id {
        1 => PadStackMode::TopMiddleBottom,
        2 => PadStackMode::FullStack,
        _ => PadStackMode::Simple, // 0 and any unknown value default to Simple
    }
}

/// Parses a Via primitive.
/// Returns the parsed `Via` and the new offset on success.
///
/// Via has 6 blocks (similar to Pad):
/// - Block 0: Name/designator (typically empty)
/// - Block 1: Layer stack data
/// - Block 2: Marker string ("|&|0")
/// - Block 3: Net/connectivity data
/// - Block 4: Geometry data
/// - Block 5: Per-layer data
#[allow(clippy::too_many_lines)]
fn parse_via(data: &[u8], offset: usize) -> ParseResult<Via> {
    let mut current = offset;

    // Block 0: Name/designator (typically empty for vias)
    let (_, next) = read_block(data, current)
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Via block 0 (name)"))?;
    current = next;

    // Block 1: Layer stack data (skip)
    let (_, next) = read_block(data, current).ok_or_else(|| {
        AltiumError::parse_error(current, "failed to read Via block 1 (layer stack)")
    })?;
    current = next;

    // Block 2: Marker string ("|&|0")
    let (_, next) = read_block(data, current)
        .ok_or_else(|| AltiumError::parse_error(current, "failed to read Via block 2 (marker)"))?;
    current = next;

    // Block 3: Net/connectivity data (skip)
    let (_, next) = read_block(data, current).ok_or_else(|| {
        AltiumError::parse_error(current, "failed to read Via block 3 (net data)")
    })?;
    current = next;

    // Block 4: Geometry data
    let (geometry, next) = read_block(data, current).ok_or_else(|| {
        AltiumError::parse_error(current, "failed to read Via block 4 (geometry)")
    })?;
    current = next;

    // Block 5: Per-layer data (optional)
    if let Some((_, next)) = read_block(data, current) {
        current = next;
    }

    // Parse geometry block
    // Minimum size: 13 (header) + 4 (x) + 4 (y) + 4 (diameter) + 4 (hole) + 2 (layers) = 31 bytes
    if geometry.len() < 31 {
        return Err(AltiumError::parse_error(
            offset,
            format!(
                "Via geometry block too short: {} bytes, expected at least 31",
                geometry.len()
            ),
        ));
    }

    // Common header (13 bytes) - layer ID at offset 0
    // Note: Via layer is typically MultiLayer (74), but we read from/to layers separately

    // Location (X, Y) - offsets 13-20
    let x =
        to_mm(read_i32(geometry, 13).ok_or_else(|| {
            AltiumError::parse_error(offset + 13, "failed to read Via x coordinate")
        })?);
    let y =
        to_mm(read_i32(geometry, 17).ok_or_else(|| {
            AltiumError::parse_error(offset + 17, "failed to read Via y coordinate")
        })?);

    // Diameter - offset 21
    let diameter = to_mm(
        read_i32(geometry, 21)
            .ok_or_else(|| AltiumError::parse_error(offset + 21, "failed to read Via diameter"))?,
    );

    // Hole size - offset 25
    let hole_size =
        to_mm(read_i32(geometry, 25).ok_or_else(|| {
            AltiumError::parse_error(offset + 25, "failed to read Via hole size")
        })?);

    // From/To layers - offsets 29-30
    let from_layer = if geometry.len() > 29 {
        layer_from_id(geometry[29])
    } else {
        Layer::TopLayer
    };

    let to_layer = if geometry.len() > 30 {
        layer_from_id(geometry[30])
    } else {
        Layer::BottomLayer
    };

    // Thermal relief settings - offsets 31-39
    let thermal_relief_gap = if geometry.len() > 34 {
        to_mm(read_i32(geometry, 31).unwrap_or(2540)) // Default: 10 mils = 2540 internal units
    } else {
        0.254 // Default: 10 mils
    };

    let thermal_relief_conductors = if geometry.len() > 35 {
        geometry[35]
    } else {
        4 // Default: 4 conductors
    };

    let thermal_relief_width = if geometry.len() > 39 {
        to_mm(read_i32(geometry, 36).unwrap_or(2540)) // Default: 10 mils = 2540 internal units
    } else {
        0.254 // Default: 10 mils
    };

    // Solder mask expansion - offset 40
    let solder_mask_expansion = if geometry.len() > 43 {
        to_mm(read_i32(geometry, 40).unwrap_or(0))
    } else {
        0.0
    };

    // Solder mask expansion manual flag - offset 44
    let solder_mask_expansion_manual = geometry.len() > 44 && geometry[44] != 0;

    // Diameter stack mode - offset 45
    let diameter_stack_mode = if geometry.len() > 45 {
        via_stack_mode_from_id(geometry[45])
    } else {
        ViaStackMode::Simple
    };

    // Per-layer diameters - offset 46+ (32 × 4 bytes = 128 bytes)
    let per_layer_diameters =
        if diameter_stack_mode != ViaStackMode::Simple && geometry.len() > 45 + 128 {
            let mut diameters = Vec::with_capacity(32);
            for i in 0..32 {
                let layer_offset = 46 + (i * 4);
                if let Some(val) = read_i32(geometry, layer_offset) {
                    diameters.push(to_mm(val));
                } else {
                    diameters.push(diameter); // Fallback to main diameter
                }
            }
            Some(diameters)
        } else {
            None
        };

    let via = Via {
        x,
        y,
        diameter,
        hole_size,
        from_layer,
        to_layer,
        solder_mask_expansion,
        solder_mask_expansion_manual,
        thermal_relief_gap,
        thermal_relief_conductors,
        thermal_relief_width,
        diameter_stack_mode,
        per_layer_diameters,
        unique_id: None,
    };

    Ok((via, current))
}

/// Converts a via stack mode ID to `ViaStackMode`.
const fn via_stack_mode_from_id(id: u8) -> ViaStackMode {
    match id {
        1 => ViaStackMode::TopMiddleBottom,
        2 => ViaStackMode::FullStack,
        _ => ViaStackMode::Simple, // 0 and any unknown value default to Simple
    }
}

/// Parses a Track primitive.
/// Returns the parsed `Track` and the new offset on success.
fn parse_track(data: &[u8], offset: usize) -> ParseResult<Track> {
    // Track has a single block with geometry data
    let (block, next) = read_block(data, offset)
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Track block"))?;

    if block.len() < 33 {
        return Err(AltiumError::parse_error(
            offset,
            format!(
                "Track block too short: {} bytes, expected at least 33",
                block.len()
            ),
        ));
    }

    // Common header (13 bytes)
    let layer_id = block[0];
    let layer = layer_from_id(layer_id);
    let flags = read_flags(block);

    // Start coordinates (X, Y) - offsets 13-20
    let x1 = to_mm(read_i32(block, 13).ok_or_else(|| {
        AltiumError::parse_error(offset + 13, "failed to read Track x1 coordinate")
    })?);
    let y1 = to_mm(read_i32(block, 17).ok_or_else(|| {
        AltiumError::parse_error(offset + 17, "failed to read Track y1 coordinate")
    })?);

    // End coordinates (X, Y) - offsets 21-28
    let x2 = to_mm(read_i32(block, 21).ok_or_else(|| {
        AltiumError::parse_error(offset + 21, "failed to read Track x2 coordinate")
    })?);
    let y2 = to_mm(read_i32(block, 25).ok_or_else(|| {
        AltiumError::parse_error(offset + 25, "failed to read Track y2 coordinate")
    })?);

    // Width - offset 29
    let width = to_mm(
        read_i32(block, 29)
            .ok_or_else(|| AltiumError::parse_error(offset + 29, "failed to read Track width"))?,
    );

    let track = Track {
        x1,
        y1,
        x2,
        y2,
        width,
        layer,
        flags,
        unique_id: None,
    };

    Ok((track, next))
}

/// Parses an Arc primitive.
/// Returns the parsed `Arc` and the new offset on success.
fn parse_arc(data: &[u8], offset: usize) -> ParseResult<Arc> {
    // Arc has a single block with geometry data
    let (block, next) = read_block(data, offset)
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Arc block"))?;

    if block.len() < 45 {
        return Err(AltiumError::parse_error(
            offset,
            format!(
                "Arc block too short: {} bytes, expected at least 45",
                block.len()
            ),
        ));
    }

    // Common header (13 bytes)
    let layer_id = block[0];
    let layer = layer_from_id(layer_id);
    let flags = read_flags(block);

    // Centre coordinates (X, Y) - offsets 13-20
    let x =
        to_mm(read_i32(block, 13).ok_or_else(|| {
            AltiumError::parse_error(offset + 13, "failed to read Arc x coordinate")
        })?);
    let y =
        to_mm(read_i32(block, 17).ok_or_else(|| {
            AltiumError::parse_error(offset + 17, "failed to read Arc y coordinate")
        })?);

    // Radius - offset 21
    let radius = to_mm(
        read_i32(block, 21)
            .ok_or_else(|| AltiumError::parse_error(offset + 21, "failed to read Arc radius"))?,
    );

    // Angles (doubles) - offsets 25-40
    let start_angle = read_f64(block, 25).unwrap_or(0.0);
    let end_angle = read_f64(block, 33).unwrap_or(360.0);

    // Width - offset 41
    let width = to_mm(
        read_i32(block, 41)
            .ok_or_else(|| AltiumError::parse_error(offset + 41, "failed to read Arc width"))?,
    );

    let arc = Arc {
        x,
        y,
        radius,
        start_angle,
        end_angle,
        width,
        layer,
        flags,
        unique_id: None,
    };

    Ok((arc, next))
}

/// Parses a Text primitive.
/// Returns the parsed `Text` and the new offset on success.
///
/// # Text Block Format (observed from sample files)
///
/// ```text
/// [block_len:4][block_data:block_len]
///
/// Block data:
/// [layer:1][flags:12]           // 13-byte common header
/// [x:4 i32]                     // X position
/// [y:4 i32]                     // Y position
/// [height:4 i32]                // Text height
/// ...                           // Additional fields (font, style)
/// [rotation:8 f64]              // Rotation angle (at offset 37)
/// [font_name:varies]            // Font name in UTF-16 (null-terminated)
/// [text_content:varies]         // Text content in UTF-16 or reference
/// ```
fn parse_text(data: &[u8], offset: usize, wide_strings: Option<&WideStrings>) -> ParseResult<Text> {
    // Text has 2 blocks:
    // - Block 0: Geometry/metadata (layer, position, height, rotation, font, etc.)
    // - Block 1: Text content (length-prefixed string, or reference to WideStrings)

    // Block 0: Geometry
    let (geometry_block, mut current) = read_block(data, offset)
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Text geometry block"))?;

    if geometry_block.len() < 25 {
        return Err(AltiumError::parse_error(
            offset,
            format!(
                "Text geometry block too short: {} bytes, expected at least 25",
                geometry_block.len()
            ),
        ));
    }

    // Common header (13 bytes)
    let layer_id = geometry_block[0];
    let layer = layer_from_id(layer_id);
    // Note: Text doesn't use standard flags format - byte 1 is text kind, not flags
    let flags = PcbFlags::empty();

    // Text kind at offset 1 (where flags low byte normally is)
    // 0 = Stroke, 1 = TrueType, 2 = BarCode
    let kind = if geometry_block.len() > 1 {
        text_kind_from_id(geometry_block[1])
    } else {
        TextKind::Stroke
    };

    // Position (X, Y) - offsets 13-20
    let x = to_mm(read_i32(geometry_block, 13).ok_or_else(|| {
        AltiumError::parse_error(offset + 13, "failed to read Text x coordinate")
    })?);
    let y = to_mm(read_i32(geometry_block, 17).ok_or_else(|| {
        AltiumError::parse_error(offset + 17, "failed to read Text y coordinate")
    })?);

    // Height - offset 21
    let height = to_mm(
        read_i32(geometry_block, 21)
            .ok_or_else(|| AltiumError::parse_error(offset + 21, "failed to read Text height"))?,
    );

    // Stroke font ID - offset 25-26 (u16)
    // Only meaningful when kind is Stroke
    let stroke_font = if geometry_block.len() > 26 && kind == TextKind::Stroke {
        let font_id = read_u16(geometry_block, 25).unwrap_or(0);
        if font_id > 0 {
            Some(stroke_font_from_id(font_id))
        } else {
            None
        }
    } else {
        None
    };

    // Rotation - offset 27 (8-byte double)
    // Altium stores rotation in degrees (0-360)
    let rotation = if geometry_block.len() > 35 {
        read_f64(geometry_block, 27).unwrap_or(0.0)
    } else {
        0.0
    };

    // Justification - offset 67 (assuming "Arial" font name)
    // The offset varies based on font name length, but 67 works for typical cases.
    // Format: line_spacing (4 bytes at 63-66) then justification (1 byte at 67)
    let justification = if geometry_block.len() > 67 {
        justification_from_id(geometry_block[67])
    } else {
        TextJustification::MiddleCenter
    };

    // Block 1: Text content
    let text_content = if let Some((text_block, next)) = read_block(data, current) {
        current = next;
        // Text block is a length-prefixed string
        let content = read_string_from_block(text_block);
        if content.is_empty() {
            // Check for special designator/comment text in geometry block
            extract_text_from_block(geometry_block, wide_strings)
        } else {
            // Check if content is a WideStrings index reference
            resolve_text_content(&content, wide_strings)
        }
    } else {
        // Fallback: check geometry block
        extract_text_from_block(geometry_block, wide_strings)
    };

    let text = Text {
        x,
        y,
        text: text_content,
        height,
        layer,
        rotation,
        kind,
        stroke_font,
        justification,
        flags,
        unique_id: None,
    };

    Ok((text, current))
}

/// Resolves text content, looking up `WideStrings` if needed.
///
/// If the content looks like a `WideStrings` index (numeric), attempts to look it up.
/// Otherwise returns the content as-is.
fn resolve_text_content(content: &str, wide_strings: Option<&WideStrings>) -> String {
    // Special text values are returned as-is
    if content.starts_with('.') {
        return content.to_string();
    }

    // Try to parse as a WideStrings index
    if let Some(ws) = wide_strings {
        if let Ok(index) = content.parse::<usize>() {
            if let Some(resolved) = ws.get(&index) {
                tracing::trace!(index, resolved = %resolved, "Resolved WideStrings text");
                return resolved.clone();
            }
        }
    }

    // Return content as-is if not a WideStrings reference
    content.to_string()
}

/// Extracts the text content from a Text geometry block.
///
/// Text content may be:
/// - Special inline text like `.Designator` or `.Comment`
/// - A `WideStrings` index that needs to be looked up
///
/// # Arguments
///
/// * `block` - The geometry block data
/// * `wide_strings` - Optional `WideStrings` lookup table
///
/// # Returns
///
/// The resolved text content, or empty string if not found.
fn extract_text_from_block(block: &[u8], wide_strings: Option<&WideStrings>) -> String {
    // Check for special designator/comment text inline
    for pattern in [".Designator", ".Comment"] {
        if find_ascii_in_block(block, pattern).is_some() {
            return pattern.to_string();
        }
    }

    // Try to find a WideStrings index in the block
    // The WideStringsIndex is a u16 at offset 115 in the geometry block
    // Verified by reverse-engineering sample.PcbLib with Text primitives
    if let Some(ws) = wide_strings {
        if block.len() > 117 {
            if let Some(index) = read_u16(block, 115) {
                if let Some(resolved) = ws.get(&(index as usize)) {
                    tracing::trace!(index, resolved = %resolved, "Resolved WideStrings from offset 115");
                    return resolved.clone();
                }
            }
        }
    }

    // No text content found
    String::new()
}

/// Reads a 2-byte little-endian unsigned integer.
fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    if offset + 2 > data.len() {
        return None;
    }
    Some(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

/// Finds an ASCII pattern within a block (for special text like ".Designator").
fn find_ascii_in_block(block: &[u8], pattern: &str) -> Option<usize> {
    let pattern_bytes = pattern.as_bytes();
    if pattern_bytes.len() > block.len() {
        return None;
    }

    (0..=(block.len() - pattern_bytes.len()))
        .find(|&i| &block[i..i + pattern_bytes.len()] == pattern_bytes)
}

/// Parses a Region primitive (filled polygon).
/// Returns the parsed `Region` and the new offset on success.
///
/// # Region Block Format (from `AltiumSharp` analysis)
///
/// Region has 2 blocks:
/// - Block 0: Properties (common header + metadata)
/// - Block 1: Vertices (count + coordinate pairs)
///
/// Block 0:
/// ```text
/// [layer:1][flags:12]      // 13-byte common header
/// [unknown:4 u32]          // Unknown data
/// [unknown:1]              // Unknown byte
/// ...                      // Additional properties
/// ```
///
/// Block 1 (vertices):
/// ```text
/// [count:4 u32]            // Number of vertices
/// [x:8 f64][y:8 f64]       // Vertex 1 (doubles in internal units)
/// [x:8 f64][y:8 f64]       // Vertex 2
/// ...
/// ```
#[allow(clippy::cast_possible_truncation)] // Altium coords fit in i32
fn parse_region(data: &[u8], offset: usize) -> ParseResult<Region> {
    // Region format (observed from Altium files):
    // Block 0: Properties block containing:
    //   - Common header (13 bytes): layer, flags, padding
    //   - Unknown data (5 bytes)
    //   - Parameter string length (4 bytes)
    //   - Parameter string (ASCII key=value pairs)
    //   - Vertex count (4 bytes)
    //   - Vertices (count * 16 bytes, each as 2 doubles)
    // Block 1: Usually empty (0 bytes)

    // Block 0: Properties with embedded vertices
    let (props_block, mut current) = read_block(data, offset).ok_or_else(|| {
        AltiumError::parse_error(offset, "failed to read Region properties block")
    })?;

    if props_block.len() < 22 {
        return Err(AltiumError::parse_error(
            offset,
            format!(
                "Region properties block too short: {} bytes, expected at least 22",
                props_block.len()
            ),
        ));
    }

    // Common header (13 bytes)
    let layer_id = props_block[0];
    let layer = layer_from_id(layer_id);
    let flags = read_flags(props_block);

    // Skip unknown bytes (5 bytes after header)
    // Read parameter string length at offset 18
    let param_len = read_u32(props_block, 18).ok_or_else(|| {
        AltiumError::parse_error(offset + 18, "failed to read Region parameter string length")
    })? as usize;

    // Skip parameter string, vertex data follows
    let vertex_offset = 22 + param_len;

    if props_block.len() < vertex_offset + 4 {
        return Err(AltiumError::parse_error(
            offset + vertex_offset,
            format!("Region block too short for vertex count at offset {vertex_offset}"),
        ));
    }

    // Read vertex count
    let vertex_count = read_u32(props_block, vertex_offset).ok_or_else(|| {
        AltiumError::parse_error(offset + vertex_offset, "failed to read Region vertex count")
    })? as usize;

    // Each vertex is 2 doubles (16 bytes)
    let vertex_data_offset = vertex_offset + 4;
    let expected_size = vertex_data_offset + vertex_count * 16;

    if props_block.len() < expected_size {
        return Err(AltiumError::parse_error(
            offset,
            format!(
                "Region block too short for {vertex_count} vertices: {} bytes, expected {expected_size}",
                props_block.len()
            ),
        ));
    }

    // Parse vertices
    let mut vertices = Vec::with_capacity(vertex_count);
    for i in 0..vertex_count {
        let base = vertex_data_offset + i * 16;
        // Coordinates stored as doubles in internal units
        let x_internal = read_f64(props_block, base).ok_or_else(|| {
            AltiumError::parse_error(
                offset + base,
                format!("failed to read Region vertex {i} x coordinate"),
            )
        })?;
        let y_internal = read_f64(props_block, base + 8).ok_or_else(|| {
            AltiumError::parse_error(
                offset + base + 8,
                format!("failed to read Region vertex {i} y coordinate"),
            )
        })?;

        // Convert from internal units to mm
        let x = to_mm(x_internal.round() as i32);
        let y = to_mm(y_internal.round() as i32);

        vertices.push(Vertex { x, y });
    }

    // Block 1: Usually empty, but still need to read it
    if let Some((_, next)) = read_block(data, current) {
        current = next;
    }

    let region = Region {
        vertices,
        layer,
        flags,
        unique_id: None,
    };

    Ok((region, current))
}

/// Parses a Fill primitive (filled rectangle).
/// Returns the parsed `Fill` and the new offset on success.
///
/// # Fill Block Format
///
/// Fill has 1 block:
/// ```text
/// [layer:1]                 // Layer ID
/// [flags:12]                // Flags and padding
/// [x1:4 i32]                // First corner X (internal units)
/// [y1:4 i32]                // First corner Y (internal units)
/// [x2:4 i32]                // Second corner X (internal units)
/// [y2:4 i32]                // Second corner Y (internal units)
/// [rotation:8 f64]          // Rotation angle in degrees
/// [unknown:...]             // Additional data
/// ```
fn parse_fill(data: &[u8], offset: usize) -> ParseResult<Fill> {
    // Fill has a single block
    let (block, current) = read_block(data, offset)
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Fill block"))?;

    // Minimum size: 13 (header) + 16 (coordinates) + 8 (rotation) = 37 bytes
    if block.len() < 37 {
        return Err(AltiumError::parse_error(
            offset,
            format!(
                "Fill block too short: {} bytes, expected at least 37",
                block.len()
            ),
        ));
    }

    // Common header (13 bytes)
    let layer_id = block[0];
    let layer = layer_from_id(layer_id);
    let flags = read_flags(block);

    // Coordinates at offset 13
    let x1 = to_mm(read_i32(block, 13).ok_or_else(|| {
        AltiumError::parse_error(offset + 13, "failed to read Fill x1 coordinate")
    })?);
    let y1 = to_mm(read_i32(block, 17).ok_or_else(|| {
        AltiumError::parse_error(offset + 17, "failed to read Fill y1 coordinate")
    })?);
    let x2 = to_mm(read_i32(block, 21).ok_or_else(|| {
        AltiumError::parse_error(offset + 21, "failed to read Fill x2 coordinate")
    })?);
    let y2 = to_mm(read_i32(block, 25).ok_or_else(|| {
        AltiumError::parse_error(offset + 25, "failed to read Fill y2 coordinate")
    })?);

    // Rotation at offset 29
    let rotation = read_f64(block, 29)
        .ok_or_else(|| AltiumError::parse_error(offset + 29, "failed to read Fill rotation"))?;

    let fill = Fill {
        x1,
        y1,
        x2,
        y2,
        layer,
        rotation,
        flags,
        unique_id: None,
    };

    Ok((fill, current))
}

/// Parses a `ComponentBody` primitive (3D model reference).
/// Returns the parsed `ComponentBody` and the new offset on success.
///
/// `ComponentBody` has 3 blocks:
/// - Block 0: Properties (layer, parameters as key=value string)
/// - Block 1: Usually empty
/// - Block 2: Usually empty
fn parse_component_body(data: &[u8], offset: usize) -> ParseResult<ComponentBody> {
    let mut current = offset;

    // Block 0: Properties with parameter string (required)
    let (block0, next) = read_block(data, current).ok_or_else(|| {
        AltiumError::parse_error(offset, "failed to read ComponentBody block 0 (properties)")
    })?;
    current = next;

    // Block 1: Usually empty (optional - may not exist at end of file)
    if let Some((_, next)) = read_block(data, current) {
        current = next;

        // Block 2: Usually empty (optional)
        if let Some((_, next)) = read_block(data, current) {
            current = next;
        }
    }

    // Parse block 0 to extract parameters
    // Format: [header bytes][parameter_string]
    // Parameter string is pipe-separated key=value pairs starting with V7_LAYER=
    let block_str = String::from_utf8_lossy(block0);

    // Find the parameter string (starts with V7_LAYER= or similar key)
    let params = parse_component_body_params(&block_str);

    // Extract key values
    let model_id = params.get("MODELID").cloned().unwrap_or_default();
    let model_name = params.get("MODEL.NAME").cloned().unwrap_or_default();
    let embedded = params.get("MODEL.EMBED").is_some_and(|v| v == "TRUE");

    // Parse rotations (stored as strings like "0.000")
    let rotation_x = params
        .get("MODEL.3D.ROTX")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let rotation_y = params
        .get("MODEL.3D.ROTY")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let rotation_z = params
        .get("MODEL.3D.ROTZ")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);

    // Parse heights (stored as strings like "0mil" or "15.748mil")
    let z_offset = parse_mil_value(params.get("MODEL.3D.DZ").map(String::as_str));
    let standoff_height = parse_mil_value(params.get("STANDOFFHEIGHT").map(String::as_str));
    let overall_height = parse_mil_value(params.get("OVERALLHEIGHT").map(String::as_str));

    // Parse layer from V7_LAYER (e.g., "MECHANICAL6")
    let layer = params
        .get("V7_LAYER")
        .and_then(|v| parse_v7_layer(v))
        .unwrap_or(Layer::Top3DBody);

    let body = ComponentBody {
        model_id,
        model_name,
        embedded,
        rotation_x,
        rotation_y,
        rotation_z,
        z_offset,
        overall_height,
        standoff_height,
        layer,
        unique_id: None,
    };

    Ok((body, current))
}

/// Parses key=value parameters from a `ComponentBody` block string.
fn parse_component_body_params(s: &str) -> std::collections::HashMap<String, String> {
    let mut params = std::collections::HashMap::new();

    // Find the start of parameters (look for V7_LAYER=)
    if let Some(start) = s.find("V7_LAYER") {
        let params_str = &s[start..];
        for pair in params_str.split('|') {
            if let Some((key, val)) = pair.split_once('=') {
                // Clean up null bytes and whitespace
                let val = val.trim_end_matches('\0').trim();
                params.insert(key.to_string(), val.to_string());
            }
        }
    }

    params
}

/// Parses a value in mils (e.g., "15.748mil") to mm.
fn parse_mil_value(s: Option<&str>) -> f64 {
    let Some(s) = s else {
        return 0.0;
    };

    // Remove "mil" suffix if present
    let numeric = s.trim_end_matches("mil").trim();
    numeric
        .parse::<f64>()
        .map(|v| v * 0.0254) // Convert mils to mm
        .unwrap_or(0.0)
}

/// Parses `V7_LAYER` string (e.g., "MECHANICAL6") to Layer enum.
fn parse_v7_layer(s: &str) -> Option<Layer> {
    match s {
        "MECHANICAL6" => Some(Layer::Top3DBody),
        "MECHANICAL7" => Some(Layer::Bottom3DBody),
        "MECHANICAL2" => Some(Layer::TopAssembly),
        "MECHANICAL3" => Some(Layer::BottomAssembly),
        "MECHANICAL4" => Some(Layer::TopCourtyard),
        "MECHANICAL5" => Some(Layer::BottomCourtyard),
        _ => None,
    }
}

// =============================================================================
// 3D Model Parsing
// =============================================================================

use super::primitives::EmbeddedModel;
use flate2::read::ZlibDecoder;
use std::io::Read as IoRead;

/// A mapping of model GUID to stream index.
///
/// The `/Library/Models/Data` stream contains entries that map GUIDs to
/// the numeric index of the model stream (e.g., `/Library/Models/0`) and the model name.
///
/// The value is a tuple of (`stream_index`, `model_name`).
pub type ModelIndex = HashMap<String, (usize, String)>;

/// Parses the `/Library/Models/Data` stream to extract GUID-to-index mapping.
///
/// # Format
///
/// The Data stream contains a sequence of length-prefixed records:
/// ```text
/// [record_len:4 LE][pipe-delimited params][null:1]
/// [record_len:4 LE][pipe-delimited params][null:1]
/// ...
/// ```
///
/// Each record contains pipe-delimited key=value pairs including:
/// - `ID={GUID}` - The model's unique identifier
/// - `NAME=filename.step` - The model filename
/// - `EMBED=TRUE|FALSE` - Whether the model is embedded
/// - `CHECKSUM=...` - Model checksum
///
/// The record's position (0, 1, 2, ...) corresponds to the model stream index
/// (`/Library/Models/0`, `/Library/Models/1`, etc.).
///
/// # Returns
///
/// A `HashMap` mapping GUID strings to their stream index and filename.
pub fn parse_model_data_stream(data: &[u8]) -> ModelIndex {
    let mut index = ModelIndex::new();
    let mut offset = 0usize;
    let mut stream_index = 0usize;

    while offset + 4 <= data.len() {
        // Read 4-byte little-endian record length
        let record_len = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        if record_len == 0 || offset + record_len > data.len() {
            tracing::debug!(
                offset,
                record_len,
                data_len = data.len(),
                "Invalid record length in Models/Data stream"
            );
            break;
        }

        // Parse the record content as UTF-8 (or Latin-1 fallback)
        let record_data = &data[offset..offset + record_len];
        let record_text = String::from_utf8(record_data.to_vec())
            .unwrap_or_else(|_| record_data.iter().map(|&b| b as char).collect());

        // Extract ID (GUID) and NAME from the record
        let mut guid = String::new();
        let mut name = String::new();

        for pair in record_text.split('|') {
            if pair.is_empty() {
                continue;
            }

            if let Some((key, value)) = pair.split_once('=') {
                match key {
                    "ID" => guid = value.trim_end_matches('\0').to_string(),
                    "NAME" => name = value.trim_end_matches('\0').to_string(),
                    _ => {}
                }
            }
        }

        if !guid.is_empty() {
            tracing::trace!(
                stream_index,
                guid = %guid,
                name = %name,
                "Parsed model record from Data stream"
            );
            index.insert(guid, (stream_index, name));
        }

        // Move past record content and null terminator
        offset += record_len;
        if offset < data.len() && data[offset] == 0 {
            offset += 1;
        }

        stream_index += 1;
    }

    tracing::debug!(count = index.len(), "Parsed model index from Data stream");
    index
}

/// Parses the `/Library/Models/Header` stream to get the model count.
///
/// # Format
///
/// The Header stream is a 4-byte little-endian unsigned integer containing
/// the number of embedded models in the library.
///
/// # Returns
///
/// The number of models in the library, or 0 if parsing fails.
pub fn parse_model_header_stream(data: &[u8]) -> usize {
    if data.len() < 4 {
        tracing::debug!(
            len = data.len(),
            "Models/Header stream too short (expected 4 bytes)"
        );
        return 0;
    }

    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    tracing::debug!(count, "Parsed model count from Header stream");
    count
}

/// Decompresses a zlib-compressed model stream.
///
/// Models in `/Library/Models/{N}` streams are zlib-compressed STEP files.
///
/// # Arguments
///
/// * `data` - The compressed model data
///
/// # Returns
///
/// The decompressed STEP file data, or an empty vector on error.
pub fn decompress_model_data(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();

    match decoder.read_to_end(&mut decompressed) {
        Ok(size) => {
            tracing::trace!(
                compressed = data.len(),
                decompressed = size,
                "Decompressed model data"
            );
            decompressed
        }
        Err(e) => {
            tracing::debug!(error = %e, "Failed to decompress model data");
            Vec::new()
        }
    }
}

/// Parses embedded models from the `/Library/Models/` storage.
///
/// This function reads the Header and Data streams to understand the model
/// structure, then extracts and decompresses each model.
///
/// # Arguments
///
/// * `model_index` - Mapping of GUID to stream index
/// * `model_data` - Vector of (index, `compressed_data`) pairs
///
/// # Returns
///
/// A vector of `EmbeddedModel` structs with decompressed STEP data.
pub fn parse_embedded_models(
    model_index: &ModelIndex,
    model_data: &[(usize, Vec<u8>)],
) -> Vec<EmbeddedModel> {
    let mut models = Vec::new();

    // Create reverse mapping: index -> (GUID, name)
    let index_to_info: HashMap<usize, (&String, &String)> = model_index
        .iter()
        .map(|(guid, (idx, name))| (*idx, (guid, name)))
        .collect();

    for (idx, compressed) in model_data {
        let Some((guid, name)) = index_to_info.get(idx) else {
            tracing::debug!(index = idx, "Model stream has no GUID mapping");
            continue;
        };

        let decompressed = decompress_model_data(compressed);
        if decompressed.is_empty() {
            tracing::warn!(
                guid = %guid,
                name = %name,
                compressed_size = compressed.len(),
                "Failed to decompress embedded 3D model — model will be missing from library"
            );
            continue;
        }

        let model = EmbeddedModel {
            id: (*guid).clone(),
            name: (*name).clone(),
            data: decompressed,
            compressed_size: compressed.len(),
        };

        tracing::debug!(
            guid = %guid,
            name = %name,
            size = model.data.len(),
            "Parsed embedded model"
        );
        models.push(model);
    }

    models
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
        let decompressed = decompress_model_data(&compressed);

        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_decompress_model_data_empty() {
        let data = b"";
        let result = decompress_model_data(data);
        assert!(result.is_empty());
    }

    #[test]
    fn test_decompress_model_data_invalid() {
        let data = b"not valid zlib data";
        let result = decompress_model_data(data);
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
