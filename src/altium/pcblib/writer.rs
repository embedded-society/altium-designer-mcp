//! Binary writer for `PcbLib` Data streams.
//!
//! This module handles encoding footprint primitives to the binary format
//! used in Altium `PcbLib` Data streams.
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

use super::primitives::{Arc, Fill, Layer, Pad, PadShape, Region, Text, Track};
use super::Footprint;

/// Conversion factor from millimetres to Altium internal units.
/// Internal units: 10000 = 1 mil = 0.0254 mm
const MM_TO_INTERNAL_UNITS: f64 = 10000.0 / 0.0254;

/// Converts millimetres to Altium internal units.
#[allow(clippy::cast_possible_truncation)] // Intentional: PCB coordinates fit in i32
fn from_mm(mm: f64) -> i32 {
    (mm * MM_TO_INTERNAL_UNITS).round() as i32
}

/// Writes a 4-byte little-endian unsigned integer.
fn write_u32(data: &mut Vec<u8>, value: u32) {
    data.extend_from_slice(&value.to_le_bytes());
}

/// Writes a 4-byte little-endian signed integer.
fn write_i32(data: &mut Vec<u8>, value: i32) {
    data.extend_from_slice(&value.to_le_bytes());
}

/// Writes an 8-byte little-endian double (IEEE 754).
fn write_f64(data: &mut Vec<u8>, value: f64) {
    data.extend_from_slice(&value.to_le_bytes());
}

/// Writes a length-prefixed block.
#[allow(clippy::cast_possible_truncation)] // Blocks are always small
fn write_block(data: &mut Vec<u8>, block: &[u8]) {
    write_u32(data, block.len() as u32);
    data.extend_from_slice(block);
}

/// Writes a length-prefixed string block.
#[allow(clippy::cast_possible_truncation)] // String names are always < 256 chars
fn write_string_block(data: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let mut block = Vec::with_capacity(1 + bytes.len());
    block.push(bytes.len() as u8);
    block.extend_from_slice(bytes);
    write_block(data, &block);
}

/// Converts our Layer enum to Altium layer ID.
///
/// Component layer pairs (from sample library analysis):
/// - Top Assembly: 58 (Mech 2)
/// - Bottom Assembly: 59 (Mech 3)
/// - Top Courtyard: 60 (Mech 4)
/// - Bottom Courtyard: 61 (Mech 5)
/// - Top 3D Body: 62 (Mech 6)
/// - Bottom 3D Body: 63 (Mech 7)
const fn layer_to_id(layer: Layer) -> u8 {
    match layer {
        Layer::TopLayer => 1,
        Layer::BottomLayer => 32,
        Layer::TopOverlay => 33,
        Layer::BottomOverlay => 34,
        Layer::TopPaste => 35,
        Layer::BottomPaste => 36,
        Layer::TopSolder => 37,
        Layer::BottomSolder => 38,
        Layer::KeepOut => 56,
        Layer::Mechanical1 => 57,
        Layer::Mechanical2 => 64, // Mechanical 8 (to avoid conflict with component layers)
        // Component layer pairs (from sample library)
        Layer::TopAssembly => 58,
        Layer::BottomAssembly => 59,
        Layer::TopCourtyard => 60,
        Layer::BottomCourtyard => 61,
        Layer::Top3DBody => 62,
        Layer::Bottom3DBody => 63,
        // Remaining mechanical layers
        Layer::Mechanical13 => 69,
        Layer::Mechanical15 => 71,
        Layer::MultiLayer => 74,
    }
}

/// Converts our `PadShape` enum to Altium pad shape ID.
const fn pad_shape_to_id(shape: PadShape) -> u8 {
    match shape {
        // Round and RoundedRectangle both use ID 1 (Altium handles corner radius separately)
        PadShape::Round | PadShape::RoundedRectangle => 1,
        PadShape::Rectangle => 2,
        PadShape::Oval => 3, // Octagon in Altium
    }
}

/// Writes the common 13-byte header for primitives.
fn write_common_header(data: &mut Vec<u8>, layer: Layer) {
    // Byte 0: Layer ID
    data.push(layer_to_id(layer));
    // Byte 1: Flags (unlocked, tenting, etc.) - use reasonable defaults
    data.push(0x00);
    // Byte 2: More flags
    data.push(0x00);
    // Bytes 3-12: Padding (0xFF as per pyAltiumLib)
    data.extend_from_slice(&[0xFF; 10]);
}

/// Encodes footprint primitives to binary format.
pub fn encode_data_stream(footprint: &Footprint) -> Vec<u8> {
    let mut data = Vec::new();

    // Write name block: [block_len:4][str_len:1][name:str_len]
    write_string_block(&mut data, &footprint.name);

    // Write primitives
    // Order: Arcs, Pads, Tracks (following typical Altium ordering)

    for arc in &footprint.arcs {
        data.push(0x01); // Arc record type
        encode_arc(&mut data, arc);
    }

    for pad in &footprint.pads {
        data.push(0x02); // Pad record type
        encode_pad(&mut data, pad);
    }

    for track in &footprint.tracks {
        data.push(0x04); // Track record type
        encode_track(&mut data, track);
    }

    for text in &footprint.text {
        data.push(0x05); // Text record type
        encode_text(&mut data, text);
    }

    for region in &footprint.regions {
        data.push(0x0B); // Region record type
        encode_region(&mut data, region);
    }

    for fill in &footprint.fills {
        data.push(0x06); // Fill record type
        encode_fill(&mut data, fill);
    }

    // End marker
    data.push(0x00);

    data
}

/// Encodes a Pad primitive.
fn encode_pad(data: &mut Vec<u8>, pad: &Pad) {
    // Block 0: Designator string
    write_string_block(data, &pad.designator);

    // Block 1: Unknown (empty block)
    write_block(data, &[]);

    // Block 2: "|&|0" string (standard marker)
    write_string_block(data, "|&|0");

    // Block 3: Unknown (empty block)
    write_block(data, &[]);

    // Block 4: Geometry data
    let geometry = encode_pad_geometry(pad);
    write_block(data, &geometry);

    // Block 5: Per-layer data (empty for simple pads)
    write_block(data, &[]);
}

/// Encodes the geometry block for a pad.
fn encode_pad_geometry(pad: &Pad) -> Vec<u8> {
    let mut block = Vec::with_capacity(128);

    // Common header (13 bytes)
    write_common_header(&mut block, pad.layer);

    // Location (X, Y) - offsets 13-20
    write_i32(&mut block, from_mm(pad.x));
    write_i32(&mut block, from_mm(pad.y));

    // Size top (X, Y) - offsets 21-28
    write_i32(&mut block, from_mm(pad.width));
    write_i32(&mut block, from_mm(pad.height));

    // Size middle (X, Y) - offsets 29-36 (same as top for simple pads)
    write_i32(&mut block, from_mm(pad.width));
    write_i32(&mut block, from_mm(pad.height));

    // Size bottom (X, Y) - offsets 37-44 (same as top for simple pads)
    write_i32(&mut block, from_mm(pad.width));
    write_i32(&mut block, from_mm(pad.height));

    // Hole size - offset 45-48
    let hole = pad.hole_size.unwrap_or(0.0);
    write_i32(&mut block, from_mm(hole));

    // Shapes (top, middle, bottom) - offsets 49-51
    let shape_id = pad_shape_to_id(pad.shape);
    block.push(shape_id); // shape_top
    block.push(shape_id); // shape_middle
    block.push(shape_id); // shape_bottom

    // Rotation - offset 52-59 (8-byte double)
    write_f64(&mut block, pad.rotation);

    // Is plated - offset 60
    block.push(u8::from(pad.hole_size.is_some()));

    // Unknown byte
    block.push(0x00);

    // Stack mode - offset 62
    block.push(0x00); // Simple stack mode

    // Unknown byte
    block.push(0x00);

    // Unknown i32s (4 of them)
    write_i32(&mut block, 0);
    write_i32(&mut block, 0);

    // Unknown i16
    block.extend_from_slice(&[0u8; 2]);

    // More unknown i32s
    write_i32(&mut block, 0);
    write_i32(&mut block, 0);
    write_i32(&mut block, 0);

    // Paste/solder mask expansions (use defaults)
    write_i32(&mut block, 0); // expansion_paste_mask
    write_i32(&mut block, 0); // expansion_solder_mask

    // Unknown bytes (7)
    block.extend_from_slice(&[0u8; 7]);

    // Manual expansion flags
    block.push(0x00); // expansion_manual_paste_mask
    block.push(0x00); // expansion_manual_solder_mask

    // More unknown (7 bytes)
    block.extend_from_slice(&[0u8; 7]);

    // Jumper ID
    block.extend_from_slice(&[0u8; 2]);

    block
}

/// Encodes a Track primitive.
fn encode_track(data: &mut Vec<u8>, track: &Track) {
    let mut block = Vec::with_capacity(64);

    // Common header (13 bytes)
    write_common_header(&mut block, track.layer);

    // Start coordinates (X, Y) - offsets 13-20
    write_i32(&mut block, from_mm(track.x1));
    write_i32(&mut block, from_mm(track.y1));

    // End coordinates (X, Y) - offsets 21-28
    write_i32(&mut block, from_mm(track.x2));
    write_i32(&mut block, from_mm(track.y2));

    // Width - offset 29-32
    write_i32(&mut block, from_mm(track.width));

    write_block(data, &block);
}

/// Encodes an Arc primitive.
fn encode_arc(data: &mut Vec<u8>, arc: &Arc) {
    let mut block = Vec::with_capacity(64);

    // Common header (13 bytes)
    write_common_header(&mut block, arc.layer);

    // Center coordinates (X, Y) - offsets 13-20
    write_i32(&mut block, from_mm(arc.x));
    write_i32(&mut block, from_mm(arc.y));

    // Radius - offset 21-24
    write_i32(&mut block, from_mm(arc.radius));

    // Angles (doubles) - offsets 25-40
    write_f64(&mut block, arc.start_angle);
    write_f64(&mut block, arc.end_angle);

    // Width - offset 41-44
    write_i32(&mut block, from_mm(arc.width));

    write_block(data, &block);
}

/// Encodes a Text primitive.
///
/// Text has 2 blocks:
/// - Block 0: Geometry/metadata (layer, position, height, rotation, font info)
/// - Block 1: Text content (length-prefixed string)
fn encode_text(data: &mut Vec<u8>, text: &Text) {
    // Block 0: Geometry
    let geometry = encode_text_geometry(text);
    write_block(data, &geometry);

    // Block 1: Text content
    write_string_block(data, &text.text);
}

/// Encodes the geometry block for text.
///
/// # Format
///
/// ```text
/// [layer:1][flags:12]           // 13-byte common header
/// [x:4 i32]                     // X position
/// [y:4 i32]                     // Y position
/// [height:4 i32]                // Text height
/// [unknown:2]                   // Unknown bytes
/// [rotation:8 f64]              // Rotation angle
/// [font_size:4 i32]             // Font size (same as height)
/// [unknown:4]                   // Unknown
/// [font_name:varies]            // Font name in UTF-16 (null-terminated)
/// [padding...]                  // Padding to standard size
/// ```
fn encode_text_geometry(text: &Text) -> Vec<u8> {
    let mut block = Vec::with_capacity(128);

    // Common header (13 bytes)
    write_common_header(&mut block, text.layer);

    // Position (X, Y) - offsets 13-20
    write_i32(&mut block, from_mm(text.x));
    write_i32(&mut block, from_mm(text.y));

    // Height - offset 21-24
    write_i32(&mut block, from_mm(text.height));

    // Unknown bytes before rotation - offsets 25-26
    block.push(0x01); // Flag: visible?
    block.push(0x00);

    // Rotation - offset 27-34 (8-byte double)
    write_f64(&mut block, text.rotation);

    // Font size (same as height) - offset 35-38
    write_i32(&mut block, from_mm(text.height));

    // Unknown bytes
    block.extend_from_slice(&[0x00; 4]);

    // Font name in UTF-16LE (null-terminated)
    // Default font: "Arial"
    let font_name = "Arial";
    for c in font_name.encode_utf16() {
        block.extend_from_slice(&c.to_le_bytes());
    }
    // Null terminator (2 bytes for UTF-16)
    block.extend_from_slice(&[0x00, 0x00]);

    // Additional font/style settings
    // These are typical defaults based on sample analysis
    block.push(0x56); // Font style byte 1
    block.push(0x40); // Font style byte 2
    block.push(0x01); // Bold flag
    block.extend_from_slice(&[0x00; 5]); // Padding

    // More font/text settings (defaults)
    write_i32(&mut block, from_mm(text.height)); // line_spacing
    block.push(0x04); // justification
    block.push(0x00);
    write_i32(&mut block, from_mm(text.height)); // glyph_width

    // Additional padding to approximate typical size
    // Total block should be around 80-100 bytes minimum
    while block.len() < 80 {
        block.push(0x00);
    }

    block
}

/// Encodes a Region primitive (filled polygon).
///
/// Region format (matching Altium):
/// - Block 0: Properties block containing common header, parameter string, and vertices
/// - Block 1: Empty block
fn encode_region(data: &mut Vec<u8>, region: &Region) {
    // Block 0: Properties with embedded vertices
    let props = encode_region_properties(region);
    write_block(data, &props);

    // Block 1: Empty block (required by Altium format)
    write_block(data, &[]);
}

/// Encodes the properties block for a region.
///
/// Format (matching Altium):
/// ```text
/// [common_header:13]       // Layer, flags, padding
/// [unknown:5]              // Unknown bytes
/// [param_len:4 u32]        // Parameter string length
/// [params:param_len]       // Parameter string (ASCII)
/// [vertex_count:4 u32]     // Number of vertices
/// [vertices:count*16]      // Vertices as doubles
/// ```
#[allow(clippy::cast_possible_truncation)] // Vertex count and param length fit in u32
fn encode_region_properties(region: &Region) -> Vec<u8> {
    let vertex_count = region.vertices.len();

    // Build parameter string
    let layer_name = region.layer.as_str().replace(' ', "").to_uppercase();
    let params = format!("V7_LAYER={layer_name}|NAME= |KIND=0");
    let params_bytes = params.as_bytes();

    let mut block = Vec::with_capacity(22 + params_bytes.len() + 4 + vertex_count * 16);

    // Common header (13 bytes)
    write_common_header(&mut block, region.layer);

    // Unknown bytes (5 bytes)
    block.extend_from_slice(&[0x00; 5]);

    // Parameter string length
    write_u32(&mut block, params_bytes.len() as u32);

    // Parameter string
    block.extend_from_slice(params_bytes);

    // Vertex count
    write_u32(&mut block, vertex_count as u32);

    // Vertices as doubles in internal units
    for vertex in &region.vertices {
        let x_internal = f64::from(from_mm(vertex.x));
        let y_internal = f64::from(from_mm(vertex.y));
        write_f64(&mut block, x_internal);
        write_f64(&mut block, y_internal);
    }

    block
}

/// Encodes a Fill primitive (filled rectangle).
///
/// Fill format:
/// - Block 0: Properties (layer, coordinates, rotation)
fn encode_fill(data: &mut Vec<u8>, fill: &Fill) {
    let block = encode_fill_block(fill);
    write_block(data, &block);
}

/// Encodes the Fill block.
///
/// Format:
/// ```text
/// [layer:1]                 // Layer ID
/// [flags:12]                // Flags and padding
/// [x1:4 i32]                // First corner X (internal units)
/// [y1:4 i32]                // First corner Y (internal units)
/// [x2:4 i32]                // Second corner X (internal units)
/// [y2:4 i32]                // Second corner Y (internal units)
/// [rotation:8 f64]          // Rotation angle in degrees
/// [unknown:13]              // Additional data (zeros)
/// ```
fn encode_fill_block(fill: &Fill) -> Vec<u8> {
    // Total block size: 13 + 16 + 8 + 13 = 50 bytes
    let mut block = Vec::with_capacity(50);

    // Common header (13 bytes)
    write_common_header(&mut block, fill.layer);

    // Corner coordinates (16 bytes)
    write_i32(&mut block, from_mm(fill.x1));
    write_i32(&mut block, from_mm(fill.y1));
    write_i32(&mut block, from_mm(fill.x2));
    write_i32(&mut block, from_mm(fill.y2));

    // Rotation (8 bytes)
    write_f64(&mut block, fill.rotation);

    // Unknown padding (13 bytes to match Altium's 50-byte block)
    block.extend_from_slice(&[0x00; 13]);

    block
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_mm() {
        // 0.0254 mm = 1 mil = 10000 internal units
        assert_eq!(from_mm(0.0254), 10000);
        // 25.4 mm = 1 inch = 10,000,000 internal units
        assert_eq!(from_mm(25.4), 10_000_000);
    }

    #[test]
    fn test_write_block() {
        let mut data = Vec::new();
        write_block(&mut data, &[0x01, 0x02, 0x03]);
        assert_eq!(data, vec![0x03, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_write_string_block() {
        let mut data = Vec::new();
        write_string_block(&mut data, "TEST");
        // Block length (5) + string length (4) + "TEST"
        assert_eq!(
            data,
            vec![0x05, 0x00, 0x00, 0x00, 0x04, b'T', b'E', b'S', b'T']
        );
    }

    #[test]
    fn test_layer_to_id() {
        assert_eq!(layer_to_id(Layer::TopLayer), 1);
        assert_eq!(layer_to_id(Layer::BottomLayer), 32);
        assert_eq!(layer_to_id(Layer::MultiLayer), 74);
    }

    #[test]
    fn test_encode_simple_footprint() {
        let mut fp = Footprint::new("TEST_FP");
        fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));

        let data = encode_data_stream(&fp);

        // Should start with name block
        // Block length: 8 (1 + 7 for "TEST_FP")
        assert_eq!(&data[0..4], &[0x08, 0x00, 0x00, 0x00]);
        // String length: 7
        assert_eq!(data[4], 0x07);
        // Name: "TEST_FP"
        assert_eq!(&data[5..12], b"TEST_FP");

        // Should have two pad records (type 0x02)
        // After name block, first record type should be 0x02
        assert_eq!(data[12], 0x02);

        // Should end with 0x00
        assert_eq!(*data.last().unwrap(), 0x00);
    }
}
