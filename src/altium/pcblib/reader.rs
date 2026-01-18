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
    Arc, ComponentBody, Fill, Layer, Pad, PadShape, Region, Text, Track, Vertex, Via,
};
use super::Footprint;

/// A lookup table for WideStrings text content.
///
/// Maps index (e.g., 0, 1, 2) to decoded text content.
/// The `/WideStrings` stream stores text as `|ENCODEDTEXT{N}=c1,c2,c3,...|`
/// where c1,c2,c3 are ASCII character codes.
pub type WideStrings = HashMap<usize, String>;

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
    let text = match String::from_utf8(data.to_vec()) {
        Ok(s) => s,
        Err(_) => {
            tracing::debug!("WideStrings stream is not valid UTF-8");
            return strings;
        }
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
fn decode_ascii_codes(encoded: &str) -> String {
    encoded
        .split(',')
        .filter_map(|s| s.trim().parse::<u8>().ok())
        .map(|c| c as char)
        .collect()
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
    // Use Windows-1252 encoding (common in Altium files)
    String::from_utf8_lossy(&block[1..=str_len]).to_string()
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
        32 => Layer::BottomLayer,
        33 => Layer::TopOverlay,
        34 => Layer::BottomOverlay,
        35 => Layer::TopPaste,
        36 => Layer::BottomPaste,
        37 => Layer::TopSolder,
        38 => Layer::BottomSolder,
        56 => Layer::KeepOut,
        57 => Layer::Mechanical1,
        // Component layer pairs (from sample library)
        58 => Layer::TopAssembly,
        59 => Layer::BottomAssembly,
        60 => Layer::TopCourtyard,
        61 => Layer::BottomCourtyard,
        62 => Layer::Top3DBody,
        63 => Layer::Bottom3DBody,
        // Remaining mechanical layers
        64..=68 => Layer::Mechanical2, // Mechanical 8-12
        69 | 70 => Layer::Mechanical13,
        71 | 72 => Layer::Mechanical15,
        // 74 = Multi-Layer and all other unknown layers
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

/// Parses primitives from a `PcbLib` Data stream.
///
/// # Arguments
///
/// * `footprint` - The footprint to populate with parsed primitives
/// * `data` - The raw Data stream bytes
/// * `wide_strings` - Optional WideStrings lookup for text content
pub fn parse_data_stream(footprint: &mut Footprint, data: &[u8], wide_strings: Option<&WideStrings>) {
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
                if let Some((arc, new_offset)) = parse_arc(data, offset) {
                    footprint.add_arc(arc);
                    offset = new_offset;
                } else {
                    tracing::debug!("Failed to parse Arc at offset {offset:#x}");
                    break;
                }
            }
            0x02 => {
                // Pad
                if let Some((pad, new_offset)) = parse_pad(data, offset) {
                    footprint.add_pad(pad);
                    offset = new_offset;
                } else {
                    tracing::debug!("Failed to parse Pad at offset {offset:#x}");
                    break;
                }
            }
            0x04 => {
                // Track
                if let Some((track, new_offset)) = parse_track(data, offset) {
                    footprint.add_track(track);
                    offset = new_offset;
                } else {
                    tracing::debug!("Failed to parse Track at offset {offset:#x}");
                    break;
                }
            }
            0x05 => {
                // Text
                if let Some((text, new_offset)) = parse_text(data, offset, wide_strings) {
                    footprint.add_text(text);
                    offset = new_offset;
                } else {
                    tracing::debug!("Failed to parse Text at offset {offset:#x}");
                    break;
                }
            }
            0x0B => {
                // Region (filled polygon)
                if let Some((region, new_offset)) = parse_region(data, offset) {
                    footprint.add_region(region);
                    offset = new_offset;
                } else {
                    tracing::debug!("Failed to parse Region at offset {offset:#x}");
                    break;
                }
            }
            0x06 => {
                // Fill (filled rectangle)
                if let Some((fill, new_offset)) = parse_fill(data, offset) {
                    footprint.add_fill(fill);
                    offset = new_offset;
                } else {
                    tracing::debug!("Failed to parse Fill at offset {offset:#x}");
                    break;
                }
            }
            0x0C => {
                // ComponentBody (3D model reference)
                if let Some((body, new_offset)) = parse_component_body(data, offset) {
                    footprint.add_component_body(body);
                    offset = new_offset;
                } else {
                    tracing::debug!("Failed to parse ComponentBody at offset {offset:#x}");
                    break;
                }
            }
            0x03 => {
                // Via
                if let Some((via, new_offset)) = parse_via(data, offset) {
                    footprint.add_via(via);
                    offset = new_offset;
                } else {
                    tracing::debug!("Failed to parse Via at offset {offset:#x}");
                    break;
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
fn parse_pad(data: &[u8], offset: usize) -> Option<(Pad, usize)> {
    let mut current = offset;

    // Block 0: Designator string
    let (block0, next) = read_block(data, current)?;
    let designator = read_string_from_block(block0);
    current = next;

    // Block 1: Unknown (skip)
    let (_, next) = read_block(data, current)?;
    current = next;

    // Block 2: Unknown string ("|&|0")
    let (_, next) = read_block(data, current)?;
    current = next;

    // Block 3: Unknown (skip)
    let (_, next) = read_block(data, current)?;
    current = next;

    // Block 4: Geometry data
    let (geometry, next) = read_block(data, current)?;
    current = next;

    // Block 5: Per-layer data (optional)
    if let Some((_, next)) = read_block(data, current) {
        current = next;
    }

    // Parse geometry block
    if geometry.len() < 52 {
        return None;
    }

    // Common header (13 bytes)
    let layer_id = geometry[0];
    let layer = layer_from_id(layer_id);

    // Location (X, Y) - offsets 13-20
    let x = to_mm(read_i32(geometry, 13)?);
    let y = to_mm(read_i32(geometry, 17)?);

    // Size top (X, Y) - offsets 21-28
    let size_top_x = to_mm(read_i32(geometry, 21)?);
    let size_top_y = to_mm(read_i32(geometry, 25)?);

    // Use top size for width/height
    let width = size_top_x;
    let height = size_top_y;

    // Hole size - offset 45
    let hole_size = if geometry.len() > 48 {
        let hole = to_mm(read_i32(geometry, 45)?);
        if hole > 0.001 {
            Some(hole)
        } else {
            None
        }
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

    let pad = Pad {
        designator,
        x,
        y,
        width,
        height,
        shape,
        layer,
        hole_size,
        rotation,
    };

    Some((pad, current))
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
fn parse_via(data: &[u8], offset: usize) -> Option<(Via, usize)> {
    let mut current = offset;

    // Block 0: Name/designator (typically empty for vias)
    let (_, next) = read_block(data, current)?;
    current = next;

    // Block 1: Layer stack data (skip)
    let (_, next) = read_block(data, current)?;
    current = next;

    // Block 2: Marker string ("|&|0")
    let (_, next) = read_block(data, current)?;
    current = next;

    // Block 3: Net/connectivity data (skip)
    let (_, next) = read_block(data, current)?;
    current = next;

    // Block 4: Geometry data
    let (geometry, next) = read_block(data, current)?;
    current = next;

    // Block 5: Per-layer data (optional)
    if let Some((_, next)) = read_block(data, current) {
        current = next;
    }

    // Parse geometry block
    // Minimum size: 13 (header) + 4 (x) + 4 (y) + 4 (diameter) + 4 (hole) + 2 (layers) = 31 bytes
    if geometry.len() < 31 {
        tracing::trace!("Via geometry block too short: {} bytes", geometry.len());
        return None;
    }

    // Common header (13 bytes) - layer ID at offset 0
    // Note: Via layer is typically MultiLayer (74), but we read from/to layers separately

    // Location (X, Y) - offsets 13-20
    let x = to_mm(read_i32(geometry, 13)?);
    let y = to_mm(read_i32(geometry, 17)?);

    // Diameter - offset 21
    let diameter = to_mm(read_i32(geometry, 21)?);

    // Hole size - offset 25
    let hole_size = to_mm(read_i32(geometry, 25)?);

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

    // Solder mask expansion - offset 40
    let solder_mask_expansion = if geometry.len() > 43 {
        to_mm(read_i32(geometry, 40).unwrap_or(0))
    } else {
        0.0
    };

    // Solder mask expansion manual flag - offset 44
    let solder_mask_expansion_manual = geometry.len() > 44 && geometry[44] != 0;

    let via = Via {
        x,
        y,
        diameter,
        hole_size,
        from_layer,
        to_layer,
        solder_mask_expansion,
        solder_mask_expansion_manual,
    };

    Some((via, current))
}

/// Parses a Track primitive.
/// Returns the parsed `Track` and the new offset on success.
fn parse_track(data: &[u8], offset: usize) -> Option<(Track, usize)> {
    // Track has a single block with geometry data
    let (block, next) = read_block(data, offset)?;

    if block.len() < 33 {
        return None;
    }

    // Common header (13 bytes)
    let layer_id = block[0];
    let layer = layer_from_id(layer_id);

    // Start coordinates (X, Y) - offsets 13-20
    let x1 = to_mm(read_i32(block, 13)?);
    let y1 = to_mm(read_i32(block, 17)?);

    // End coordinates (X, Y) - offsets 21-28
    let x2 = to_mm(read_i32(block, 21)?);
    let y2 = to_mm(read_i32(block, 25)?);

    // Width - offset 29
    let width = to_mm(read_i32(block, 29)?);

    let track = Track::new(x1, y1, x2, y2, width, layer);

    Some((track, next))
}

/// Parses an Arc primitive.
/// Returns the parsed `Arc` and the new offset on success.
fn parse_arc(data: &[u8], offset: usize) -> Option<(Arc, usize)> {
    // Arc has a single block with geometry data
    let (block, next) = read_block(data, offset)?;

    if block.len() < 45 {
        return None;
    }

    // Common header (13 bytes)
    let layer_id = block[0];
    let layer = layer_from_id(layer_id);

    // Centre coordinates (X, Y) - offsets 13-20
    let x = to_mm(read_i32(block, 13)?);
    let y = to_mm(read_i32(block, 17)?);

    // Radius - offset 21
    let radius = to_mm(read_i32(block, 21)?);

    // Angles (doubles) - offsets 25-40
    let start_angle = read_f64(block, 25).unwrap_or(0.0);
    let end_angle = read_f64(block, 33).unwrap_or(360.0);

    // Width - offset 41
    let width = to_mm(read_i32(block, 41)?);

    let arc = Arc {
        x,
        y,
        radius,
        start_angle,
        end_angle,
        width,
        layer,
    };

    Some((arc, next))
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
fn parse_text(data: &[u8], offset: usize, wide_strings: Option<&WideStrings>) -> Option<(Text, usize)> {
    // Text has 2 blocks:
    // - Block 0: Geometry/metadata (layer, position, height, rotation, font, etc.)
    // - Block 1: Text content (length-prefixed string, or reference to WideStrings)

    // Block 0: Geometry
    let (geometry_block, mut current) = read_block(data, offset)?;

    if geometry_block.len() < 25 {
        tracing::trace!(
            "Text geometry block too short: {} bytes",
            geometry_block.len()
        );
        return None;
    }

    // Common header (13 bytes)
    let layer_id = geometry_block[0];
    let layer = layer_from_id(layer_id);

    // Position (X, Y) - offsets 13-20
    let x = to_mm(read_i32(geometry_block, 13)?);
    let y = to_mm(read_i32(geometry_block, 17)?);

    // Height - offset 21
    let height = to_mm(read_i32(geometry_block, 21)?);

    // Rotation - offset 27 (8-byte double)
    // Altium stores rotation in degrees (0-360)
    let rotation = if geometry_block.len() > 35 {
        read_f64(geometry_block, 27).unwrap_or(0.0)
    } else {
        0.0
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
    };

    Some((text, current))
}

/// Resolves text content, looking up WideStrings if needed.
///
/// If the content looks like a WideStrings index (numeric), attempts to look it up.
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
/// - A WideStrings index that needs to be looked up
///
/// # Arguments
///
/// * `block` - The geometry block data
/// * `wide_strings` - Optional WideStrings lookup table
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
    // The index is typically stored as a small integer near the end of the block
    if let Some(ws) = wide_strings {
        // Try extracting a potential index from the block
        // The WideStringsIndex is typically a u16 or u32 near offset 95+
        if block.len() > 97 {
            // Try reading as u16 at common offsets
            for offset in [95, 96, 97] {
                if let Some(index) = read_u16(block, offset) {
                    if let Some(resolved) = ws.get(&(index as usize)) {
                        tracing::trace!(offset, index, resolved = %resolved, "Resolved WideStrings from block");
                        return resolved.clone();
                    }
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
fn parse_region(data: &[u8], offset: usize) -> Option<(Region, usize)> {
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
    let (props_block, mut current) = read_block(data, offset)?;

    if props_block.len() < 22 {
        tracing::trace!(
            "Region properties block too short: {} bytes",
            props_block.len()
        );
        return None;
    }

    // Common header (13 bytes)
    let layer_id = props_block[0];
    let layer = layer_from_id(layer_id);

    // Skip unknown bytes (5 bytes after header)
    // Read parameter string length at offset 18
    let param_len = read_u32(props_block, 18)? as usize;

    // Skip parameter string, vertex data follows
    let vertex_offset = 22 + param_len;

    if props_block.len() < vertex_offset + 4 {
        tracing::trace!(
            "Region block too short for vertex count at offset {}",
            vertex_offset
        );
        return None;
    }

    // Read vertex count
    let vertex_count = read_u32(props_block, vertex_offset)? as usize;

    // Each vertex is 2 doubles (16 bytes)
    let vertex_data_offset = vertex_offset + 4;
    let expected_size = vertex_data_offset + vertex_count * 16;

    if props_block.len() < expected_size {
        tracing::trace!(
            "Region block too short for {} vertices: {} < {}",
            vertex_count,
            props_block.len(),
            expected_size
        );
        return None;
    }

    // Parse vertices
    let mut vertices = Vec::with_capacity(vertex_count);
    for i in 0..vertex_count {
        let base = vertex_data_offset + i * 16;
        // Coordinates stored as doubles in internal units
        let x_internal = read_f64(props_block, base)?;
        let y_internal = read_f64(props_block, base + 8)?;

        // Convert from internal units to mm
        let x = to_mm(x_internal.round() as i32);
        let y = to_mm(y_internal.round() as i32);

        vertices.push(Vertex { x, y });
    }

    // Block 1: Usually empty, but still need to read it
    if let Some((_, next)) = read_block(data, current) {
        current = next;
    }

    let region = Region { vertices, layer };

    Some((region, current))
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
fn parse_fill(data: &[u8], offset: usize) -> Option<(Fill, usize)> {
    // Fill has a single block
    let (block, current) = read_block(data, offset)?;

    // Minimum size: 13 (header) + 16 (coordinates) + 8 (rotation) = 37 bytes
    if block.len() < 37 {
        tracing::trace!("Fill block too short: {} bytes", block.len());
        return None;
    }

    // Common header (13 bytes)
    let layer_id = block[0];
    let layer = layer_from_id(layer_id);

    // Coordinates at offset 13
    let x1 = to_mm(read_i32(block, 13)?);
    let y1 = to_mm(read_i32(block, 17)?);
    let x2 = to_mm(read_i32(block, 21)?);
    let y2 = to_mm(read_i32(block, 25)?);

    // Rotation at offset 29
    let rotation = read_f64(block, 29)?;

    let fill = Fill {
        x1,
        y1,
        x2,
        y2,
        layer,
        rotation,
    };

    Some((fill, current))
}

/// Parses a `ComponentBody` primitive (3D model reference).
/// Returns the parsed `ComponentBody` and the new offset on success.
///
/// `ComponentBody` has 3 blocks:
/// - Block 0: Properties (layer, parameters as key=value string)
/// - Block 1: Usually empty
/// - Block 2: Usually empty
fn parse_component_body(data: &[u8], offset: usize) -> Option<(ComponentBody, usize)> {
    let mut current = offset;

    // Block 0: Properties with parameter string (required)
    let (block0, next) = read_block(data, current)?;
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
    };

    Some((body, current))
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
        assert_eq!(layer_from_id(1), Layer::TopLayer);
        assert_eq!(layer_from_id(32), Layer::BottomLayer);
        assert_eq!(layer_from_id(74), Layer::MultiLayer);
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
}
