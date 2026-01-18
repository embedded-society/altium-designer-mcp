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

use super::primitives::{Arc, ComponentBody, Fill, Layer, Pad, PadShape, Region, Text, Track, Via};
use super::Footprint;

/// Encodes text content for the `WideStrings` stream.
///
/// # Format
///
/// ```text
/// |ENCODEDTEXT0=84,69,83,84|ENCODEDTEXT1=72,69,76,76,79|
/// ```
///
/// Where values are comma-separated ASCII codes.
///
/// # Arguments
///
/// * `texts` - Slice of text strings to encode
///
/// # Returns
///
/// Encoded `WideStrings` stream content as bytes.
pub fn encode_wide_strings(texts: &[&str]) -> Vec<u8> {
    use std::fmt::Write;

    let mut output = String::new();

    for (index, text) in texts.iter().enumerate() {
        // Skip special text like .Designator and .Comment
        if text.starts_with('.') {
            continue;
        }

        // Encode text as comma-separated ASCII codes
        let encoded: String = text
            .bytes()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let _ = write!(output, "|ENCODEDTEXT{index}={encoded}");
    }

    if !output.is_empty() {
        output.push('|');
    }

    output.into_bytes()
}

/// Collects all text content from footprints that needs to be stored in `WideStrings`.
///
/// Returns a vector of unique text strings (excluding special values like `.Designator`).
pub fn collect_wide_strings_content(footprints: &[Footprint]) -> Vec<String> {
    let mut texts = Vec::new();

    for footprint in footprints {
        for text in &footprint.text {
            // Skip special text values
            if !text.text.starts_with('.') && !text.text.is_empty() && !texts.contains(&text.text) {
                texts.push(text.text.clone());
            }
        }
    }

    texts
}

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

    for via in &footprint.vias {
        data.push(0x03); // Via record type
        encode_via(&mut data, via);
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

    for body in &footprint.component_bodies {
        data.push(0x0C); // ComponentBody record type
        encode_component_body(&mut data, body);
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

/// Encodes a Via primitive.
///
/// Via has 6 blocks (similar to Pad):
/// - Block 0: Name/designator (empty for vias)
/// - Block 1: Layer stack data (empty)
/// - Block 2: Marker string ("|&|0")
/// - Block 3: Net/connectivity data (empty)
/// - Block 4: Geometry data
/// - Block 5: Per-layer data (empty for simple vias)
fn encode_via(data: &mut Vec<u8>, via: &Via) {
    // Block 0: Name/designator (empty for vias)
    write_block(data, &[0u8; 1]); // Single null byte for empty string

    // Block 1: Layer stack data (empty)
    write_block(data, &[]);

    // Block 2: "|&|0" marker string
    write_string_block(data, "|&|0");

    // Block 3: Net/connectivity data (empty for library vias)
    write_block(data, &[]);

    // Block 4: Geometry data
    let geometry = encode_via_geometry(via);
    write_block(data, &geometry);

    // Block 5: Per-layer data (empty for simple vias)
    write_block(data, &[]);
}

/// Encodes the geometry block for a via.
///
/// # Format
///
/// ```text
/// [layer:1]                 // Layer ID (typically MultiLayer = 74)
/// [flags:12]                // Flags and padding
/// [x:4 i32]                 // X position
/// [y:4 i32]                 // Y position
/// [diameter:4 i32]          // Via diameter
/// [hole_size:4 i32]         // Hole diameter
/// [from_layer:1]            // Starting layer ID
/// [to_layer:1]              // Ending layer ID
/// [thermal_gap:4 i32]       // Thermal relief air gap width
/// [thermal_count:1]         // Thermal relief conductors count
/// [thermal_width:4 i32]     // Thermal relief conductors width
/// [solder_expansion:4 i32]  // Solder mask expansion
/// [solder_manual:1]         // Solder mask expansion manual flag
/// [stack_mode:1]            // Diameter stack mode (0 = Simple)
/// ```
fn encode_via_geometry(via: &Via) -> Vec<u8> {
    let mut block = Vec::with_capacity(64);

    // Common header (13 bytes) - use MultiLayer for vias
    write_common_header(&mut block, Layer::MultiLayer);

    // Location (X, Y) - offsets 13-20
    write_i32(&mut block, from_mm(via.x));
    write_i32(&mut block, from_mm(via.y));

    // Diameter - offset 21
    write_i32(&mut block, from_mm(via.diameter));

    // Hole size - offset 25
    write_i32(&mut block, from_mm(via.hole_size));

    // From/To layers - offsets 29-30
    block.push(layer_to_id(via.from_layer));
    block.push(layer_to_id(via.to_layer));

    // Thermal relief air gap width - offset 31 (default: 10 mils = 0.254mm)
    write_i32(&mut block, from_mm(0.254));

    // Thermal relief conductors count - offset 35 (default: 4)
    block.push(4);

    // Thermal relief conductors width - offset 36 (default: 10 mils = 0.254mm)
    write_i32(&mut block, from_mm(0.254));

    // Solder mask expansion - offset 40
    write_i32(&mut block, from_mm(via.solder_mask_expansion));

    // Solder mask expansion manual flag - offset 44
    block.push(u8::from(via.solder_mask_expansion_manual));

    // Diameter stack mode - offset 45 (0 = Simple)
    block.push(0x00);

    // Pad to minimum expected size
    while block.len() < 46 {
        block.push(0x00);
    }

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

    // Centre coordinates (X, Y) - offsets 13-20
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

/// Encodes a `ComponentBody` primitive (3D model reference).
fn encode_component_body(data: &mut Vec<u8>, body: &ComponentBody) {
    let block = encode_component_body_block(body);
    write_block(data, &block);

    // Blocks 1 and 2 are optional/empty
    write_block(data, &[]);
    write_block(data, &[]);
}

/// Encodes the `ComponentBody` block 0.
///
/// Format:
/// ```text
/// [layer:1]                    // Layer ID (e.g., 62 for Top 3D Body)
/// [record_type:2]              // Record type (0x0C, 0x00)
/// [ff_padding:10]              // 0xFF padding
/// [zeros:5]                    // Zeros
/// [param_len:4]                // Parameter string length (including null)
/// [param_string:param_len]     // Key=value pairs separated by |
/// [vertex_count:4]             // Outline vertex count (usually 0 or 4)
/// [vertices...]                // Optional outline vertices
/// ```
#[allow(clippy::cast_possible_truncation)] // Parameter strings are always small
fn encode_component_body_block(body: &ComponentBody) -> Vec<u8> {
    let mut block = Vec::with_capacity(128);

    // Layer ID (1 byte)
    block.push(layer_to_id(body.layer));

    // Record type marker (2 bytes): 0x0C 0x00
    block.push(0x0C);
    block.push(0x00);

    // 0xFF padding (10 bytes)
    block.extend_from_slice(&[0xFF; 10]);

    // Zeros (5 bytes)
    block.extend_from_slice(&[0x00; 5]);

    // Build parameter string
    let param_str = build_component_body_params(body);

    // Parameter string length including null terminator (4 bytes)
    let param_len = param_str.len() + 1; // +1 for null
    write_u32(&mut block, param_len as u32);

    // Parameter string (null-terminated)
    block.extend_from_slice(param_str.as_bytes());
    block.push(0x00); // Null terminator

    // No outline vertices (4 bytes = count of 0)
    write_u32(&mut block, 0);

    block
}

/// Builds the parameter string for a `ComponentBody`.
fn build_component_body_params(body: &ComponentBody) -> String {
    let mut params = Vec::new();

    // V7_LAYER (Top3DBody is MECHANICAL6, Bottom3DBody is MECHANICAL7)
    let layer_name = match body.layer {
        Layer::Bottom3DBody => "MECHANICAL7",
        _ => "MECHANICAL6",
    };
    params.push(format!("V7_LAYER={layer_name}"));

    // Standard parameters
    params.push("NAME= ".to_string());
    params.push("KIND=0".to_string());
    params.push("SUBPOLYINDEX=-1".to_string());
    params.push("UNIONINDEX=0".to_string());
    params.push("ARCRESOLUTION=0.5mil".to_string());
    params.push("ISSHAPEBASED=FALSE".to_string());
    params.push("CAVITYHEIGHT=0mil".to_string());
    params.push(format!(
        "STANDOFFHEIGHT={}mil",
        mm_to_mil(body.standoff_height)
    ));
    params.push(format!(
        "OVERALLHEIGHT={}mil",
        mm_to_mil(body.overall_height)
    ));
    params.push("BODYPROJECTION=0".to_string());
    params.push("ARCRESOLUTION=0.5mil".to_string());
    params.push("BODYCOLOR3D=8421504".to_string());
    params.push("BODYOPACITY3D=1.000".to_string());
    params.push("TEXTURECENTERX=0mil".to_string());
    params.push("TEXTURECENTERY=0mil".to_string());
    params.push("TEXTURESIZEX=0mil".to_string());
    params.push("TEXTURESIZEY=0mil".to_string());
    params.push("TEXTUREROTATION= 0.00000000000000E+0000".to_string());

    // Model reference
    params.push(format!("MODELID={}", body.model_id));
    params.push("MODEL.CHECKSUM=0".to_string());
    params.push(format!(
        "MODEL.EMBED={}",
        if body.embedded { "TRUE" } else { "FALSE" }
    ));
    params.push(format!("MODEL.NAME={}", body.model_name));
    params.push("MODEL.2D.X=0mil".to_string());
    params.push("MODEL.2D.Y=0mil".to_string());
    params.push("MODEL.2D.ROTATION=0.000".to_string());
    params.push(format!("MODEL.3D.ROTX={:.3}", body.rotation_x));
    params.push(format!("MODEL.3D.ROTY={:.3}", body.rotation_y));
    params.push(format!("MODEL.3D.ROTZ={:.3}", body.rotation_z));
    params.push(format!("MODEL.3D.DZ={}mil", mm_to_mil(body.z_offset)));
    params.push("MODEL.MODELTYPE=1".to_string());
    params.push("MODEL.MODELSOURCE=Undefined".to_string());

    params.join("|")
}

/// Converts mm to mils for parameter strings.
fn mm_to_mil(mm: f64) -> f64 {
    mm / 0.0254
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

    #[test]
    fn test_encode_wide_strings() {
        let texts = ["TEST", "HELLO"];
        let data = encode_wide_strings(&texts);
        let output = String::from_utf8(data).unwrap();

        // Should contain ENCODEDTEXT entries with ASCII codes
        assert!(output.contains("|ENCODEDTEXT0=84,69,83,84")); // TEST
        assert!(output.contains("|ENCODEDTEXT1=72,69,76,76,79")); // HELLO
    }

    #[test]
    fn test_encode_wide_strings_empty() {
        let texts: [&str; 0] = [];
        let data = encode_wide_strings(&texts);
        assert!(data.is_empty());
    }

    #[test]
    fn test_encode_wide_strings_skips_special() {
        let texts = [".Designator", ".Comment", "ACTUAL"];
        let data = encode_wide_strings(&texts);
        let output = String::from_utf8(data).unwrap();

        // Should only contain ACTUAL, not special text
        assert!(!output.contains("Designator"));
        assert!(!output.contains("Comment"));
        // Index is 2 because .Designator (0) and .Comment (1) are skipped but preserve index
        assert!(output.contains("|ENCODEDTEXT2=65,67,84,85,65,76")); // ACTUAL
    }

    #[test]
    fn test_collect_wide_strings_content() {
        use super::{Layer, Text};

        let mut fp = Footprint::new("TEST");
        fp.add_text(Text {
            x: 0.0,
            y: 0.0,
            text: ".Designator".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
        });
        fp.add_text(Text {
            x: 1.0,
            y: 0.0,
            text: "CUSTOM_TEXT".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
        });

        let texts = collect_wide_strings_content(&[fp]);

        // Should only contain non-special text
        assert_eq!(texts.len(), 1);
        assert!(texts.contains(&"CUSTOM_TEXT".to_string()));
        assert!(!texts.iter().any(|t| t.starts_with('.')));
    }
}
