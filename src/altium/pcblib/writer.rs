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

use super::primitives::{
    Arc, ComponentBody, Fill, HoleShape, Layer, Pad, PadShape, PadStackMode, PcbFlags, Region,
    StrokeFont, Text, TextKind, Track, Via, ViaStackMode,
};
use super::Footprint;

use super::units::{from_mm, mm_to_mil, MM_TO_INTERNAL_UNITS};

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

// Shared byte frames live in crate::altium::framing so PcbLib and SchLib use
// one implementation each (see that module).
use crate::altium::framing::{write_block, write_cstring_param_block, write_pascal_string};

/// Writes a length-prefixed string block: outer `[u32 len]` wrapping a Pascal
/// short string `[u8 len][bytes]`.
///
/// # Errors
///
/// Returns an error if the string exceeds 255 bytes.
fn write_string_block(
    data: &mut Vec<u8>,
    s: &str,
    field_name: &str,
) -> crate::altium::error::AltiumResult<()> {
    use crate::altium::error::AltiumError;

    // Altium stores strings as Windows-1252, not UTF-8; the Pascal length
    // prefix is the encoded byte count.
    let bytes = crate::altium::encode_windows1252(s);
    if bytes.len() > 255 {
        return Err(AltiumError::InvalidParameter {
            name: field_name.to_string(),
            message: format!(
                "String '{}...' length {} exceeds maximum of 255 bytes",
                s.chars().take(20).collect::<String>(),
                bytes.len()
            ),
        });
    }

    let mut block = Vec::with_capacity(1 + bytes.len());
    write_pascal_string(&mut block, &bytes);
    write_block(data, &block);
    Ok(())
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
#[allow(clippy::too_many_lines)] // Layer-to-ID lookup for all layer types
const fn layer_to_id(layer: Layer) -> u8 {
    match layer {
        Layer::TopLayer => 1,
        // Mid layers (IDs 2-31)
        Layer::MidLayer1 => 2,
        Layer::MidLayer2 => 3,
        Layer::MidLayer3 => 4,
        Layer::MidLayer4 => 5,
        Layer::MidLayer5 => 6,
        Layer::MidLayer6 => 7,
        Layer::MidLayer7 => 8,
        Layer::MidLayer8 => 9,
        Layer::MidLayer9 => 10,
        Layer::MidLayer10 => 11,
        Layer::MidLayer11 => 12,
        Layer::MidLayer12 => 13,
        Layer::MidLayer13 => 14,
        Layer::MidLayer14 => 15,
        Layer::MidLayer15 => 16,
        Layer::MidLayer16 => 17,
        Layer::MidLayer17 => 18,
        Layer::MidLayer18 => 19,
        Layer::MidLayer19 => 20,
        Layer::MidLayer20 => 21,
        Layer::MidLayer21 => 22,
        Layer::MidLayer22 => 23,
        Layer::MidLayer23 => 24,
        Layer::MidLayer24 => 25,
        Layer::MidLayer25 => 26,
        Layer::MidLayer26 => 27,
        Layer::MidLayer27 => 28,
        Layer::MidLayer28 => 29,
        Layer::MidLayer29 => 30,
        Layer::MidLayer30 => 31,
        Layer::BottomLayer => 32,
        Layer::TopOverlay => 33,
        Layer::BottomOverlay => 34,
        Layer::TopPaste => 35,
        Layer::BottomPaste => 36,
        Layer::TopSolder => 37,
        Layer::BottomSolder => 38,
        // Internal planes (IDs 39-54)
        Layer::InternalPlane1 => 39,
        Layer::InternalPlane2 => 40,
        Layer::InternalPlane3 => 41,
        Layer::InternalPlane4 => 42,
        Layer::InternalPlane5 => 43,
        Layer::InternalPlane6 => 44,
        Layer::InternalPlane7 => 45,
        Layer::InternalPlane8 => 46,
        Layer::InternalPlane9 => 47,
        Layer::InternalPlane10 => 48,
        Layer::InternalPlane11 => 49,
        Layer::InternalPlane12 => 50,
        Layer::InternalPlane13 => 51,
        Layer::InternalPlane14 => 52,
        Layer::InternalPlane15 => 53,
        Layer::InternalPlane16 => 54,
        // Drill layers
        Layer::DrillGuide => 55,
        Layer::DrillDrawing => 73,
        Layer::KeepOut => 56,
        Layer::Mechanical1 => 57,
        // Component layer pairs (from sample library)
        Layer::TopAssembly | Layer::Mechanical2 => 58,
        Layer::BottomAssembly | Layer::Mechanical3 => 59,
        Layer::TopCourtyard | Layer::Mechanical4 => 60,
        Layer::BottomCourtyard | Layer::Mechanical5 => 61,
        Layer::Top3DBody | Layer::Mechanical6 => 62,
        Layer::Bottom3DBody | Layer::Mechanical7 => 63,
        // Remaining mechanical layers (IDs 64-72)
        Layer::Mechanical8 => 64,
        Layer::Mechanical9 => 65,
        Layer::Mechanical10 => 66,
        Layer::Mechanical11 => 67,
        Layer::Mechanical12 => 68,
        Layer::Mechanical13 => 69,
        Layer::Mechanical14 => 70,
        Layer::Mechanical15 => 71,
        Layer::Mechanical16 => 72,
        // Special layers (IDs 75-85)
        Layer::ConnectLayer => 75,
        Layer::BackgroundLayer => 76,
        Layer::DRCErrorLayer => 77,
        Layer::HighlightLayer => 78,
        Layer::GridColor1 => 79,
        Layer::GridColor10 => 80,
        Layer::PadHoleLayer => 81,
        Layer::ViaHoleLayer => 82,
        Layer::TopPadMaster => 83,
        Layer::BottomPadMaster => 84,
        Layer::DRCDetailLayer => 85,
        // Extended mechanical layers (Altium Designer 18+)
        Layer::Mechanical17 => 186,
        Layer::Mechanical18 => 187,
        Layer::Mechanical19 => 188,
        Layer::Mechanical20 => 189,
        Layer::Mechanical21 => 190,
        Layer::Mechanical22 => 191,
        Layer::Mechanical23 => 192,
        Layer::Mechanical24 => 193,
        Layer::Mechanical25 => 194,
        Layer::Mechanical26 => 195,
        Layer::Mechanical27 => 196,
        Layer::Mechanical28 => 197,
        Layer::Mechanical29 => 198,
        Layer::Mechanical30 => 199,
        Layer::Mechanical31 => 200,
        Layer::Mechanical32 => 201,
        Layer::MultiLayer => 74,
    }
}

/// Converts our `PadShape` enum to the Altium pad shape ID.
///
/// Altium shape ids (`PcbPad`): Round=1, Rectangular=2, Octagonal=3,
/// RoundedRectangle=9. We keep `Oval`→3 so it round-trips through our own
/// reader (Altium itself has no oval shape — it draws a Round pad with unequal
/// X/Y sizes as an oblong).
const fn pad_shape_to_id(shape: PadShape) -> u8 {
    match shape {
        PadShape::Round => 1,
        PadShape::Rectangle => 2,
        PadShape::Oval => 3,
        PadShape::RoundedRectangle => 9,
    }
}

// Note: non-round hole shapes (Square/Slot) live in the 596-byte size/shape
// block (offset 262), which simple from-scratch pads do not emit; supporting
// them is a follow-up. Offset 61 of the main block is reserved (0x00).

// Altium primitive flag bits (PcbBinaryConstants), distinct from our internal
// `PcbFlags` bit layout — shared with the reader via `super::flags`.
use super::flags::{
    ALT_FLAG_KEEPOUT, ALT_FLAG_SAVED, ALT_FLAG_TENTING_BOTTOM, ALT_FLAG_TENTING_TOP,
    ALT_FLAG_UNLOCKED,
};

/// Encodes our internal `PcbFlags` into Altium's on-disk flag word.
///
/// `FlagSaved` (bit 3) is always set on a saved primitive and `FlagUnlocked`
/// (bit 2) is set unless the primitive is locked — a normal pad is therefore
/// `0x000C`, not `0x0000`. `read_flags` in the reader performs the inverse.
const fn encode_altium_flags(flags: PcbFlags) -> u16 {
    let mut f = ALT_FLAG_SAVED;
    if !flags.contains(PcbFlags::LOCKED) {
        f |= ALT_FLAG_UNLOCKED;
    }
    if flags.contains(PcbFlags::TENTING_TOP) {
        f |= ALT_FLAG_TENTING_TOP;
    }
    if flags.contains(PcbFlags::TENTING_BOTTOM) {
        f |= ALT_FLAG_TENTING_BOTTOM;
    }
    if flags.contains(PcbFlags::KEEPOUT) {
        f |= ALT_FLAG_KEEPOUT;
    }
    f
}

/// Writes the common 13-byte header for primitives.
fn write_common_header(data: &mut Vec<u8>, layer: Layer, flags: PcbFlags) {
    // Byte 0: Layer ID
    data.push(layer_to_id(layer));
    // Bytes 1-2: Altium flag word (saved/unlocked/tenting/keepout)
    data.extend_from_slice(&encode_altium_flags(flags).to_le_bytes());
    // Bytes 3-12: net index / polygon index / component index / reserved, all
    // 0xFF (none) for a free primitive.
    data.extend_from_slice(&[0xFF; 10]);
}

/// Encodes footprint primitives to binary format.
///
/// # Errors
///
/// Returns an error if any string (footprint name, pad designator, text) exceeds 255 bytes.
pub fn encode_data_stream(footprint: &Footprint) -> crate::altium::error::AltiumResult<Vec<u8>> {
    let mut data = Vec::new();

    // Write name block: [block_len:4][str_len:1][name:str_len]
    write_string_block(&mut data, &footprint.name, "footprint.name")?;

    // Write primitives
    // Order: Arcs, Pads, Tracks (following typical Altium ordering)

    for arc in &footprint.arcs {
        data.push(0x01); // Arc record type
        encode_arc(&mut data, arc);
    }

    for pad in &footprint.pads {
        data.push(0x02); // Pad record type
        encode_pad(&mut data, pad)?;
    }

    for via in &footprint.vias {
        data.push(0x03); // Via record type
        encode_via(&mut data, via)?;
    }

    for track in &footprint.tracks {
        data.push(0x04); // Track record type
        encode_track(&mut data, track);
    }

    for text in &footprint.text {
        data.push(0x05); // Text record type
        encode_text(&mut data, text)?;
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
        let outline = resolve_body_outline(body, footprint);
        encode_component_body(&mut data, body, &outline);
    }

    // No end marker: Altium reads exactly the primitive count from the component
    // Header. AltiumSharp writes none, and a trailing 0x00 is mis-read as a
    // record with object-id 0 (issue #68).

    Ok(data)
}

/// Encodes per-layer data for a Pad (Block 5).
///
/// Per-layer data is required when stack mode is not Simple. The format is:
/// - 32 size entries (`CoordPoint`, 8 bytes each) = 256 bytes
/// - 32 shape entries (1 byte each) = 32 bytes
/// - 32 corner radius percentages (1 byte each, 0-100) = 32 bytes
/// - 32 offset entries (`CoordPoint`, 8 bytes each) = 256 bytes (optional)
///
/// Total: 320 bytes minimum, 576 bytes with offsets
fn encode_pad_per_layer_data(pad: &Pad) -> Vec<u8> {
    let has_offsets = pad.per_layer_offsets.is_some();
    let capacity = if has_offsets { 576 } else { 320 };
    let mut block = Vec::with_capacity(capacity);

    // 32 size entries (width, height for each layer) - 256 bytes
    for i in 0..32 {
        let (width, height) = pad
            .per_layer_sizes
            .as_ref()
            .and_then(|sizes| sizes.get(i).copied())
            .unwrap_or((pad.width, pad.height));
        write_i32(&mut block, from_mm(width));
        write_i32(&mut block, from_mm(height));
    }

    // 32 shape entries - 32 bytes
    for i in 0..32 {
        let shape = pad
            .per_layer_shapes
            .as_ref()
            .and_then(|shapes| shapes.get(i).copied())
            .unwrap_or(pad.shape);
        block.push(pad_shape_to_id(shape));
    }

    // 32 corner radius percentages - 32 bytes
    // Default corner radius: 50% for RoundedRectangle, 0% otherwise
    // This ensures RoundedRectangle pads round-trip correctly (they share shape ID 1 with Round)
    let default_radius = pad.corner_radius_percent.unwrap_or_else(|| {
        if pad.shape == PadShape::RoundedRectangle {
            50
        } else {
            0
        }
    });
    for i in 0..32 {
        // Get per-layer corner radius, or calculate default based on per-layer shape
        let radius = pad
            .per_layer_corner_radii
            .as_ref()
            .and_then(|radii| radii.get(i).copied())
            .unwrap_or_else(|| {
                // If per-layer shape is specified and is RoundedRectangle, use 50%
                let layer_shape = pad
                    .per_layer_shapes
                    .as_ref()
                    .and_then(|shapes| shapes.get(i).copied())
                    .unwrap_or(pad.shape);
                if layer_shape == PadShape::RoundedRectangle && default_radius == 0 {
                    50
                } else {
                    default_radius
                }
            });
        block.push(radius);
    }

    // 32 offset entries (x, y for each layer) - 256 bytes (optional)
    if let Some(ref offsets) = pad.per_layer_offsets {
        for i in 0..32 {
            let (x, y) = offsets.get(i).copied().unwrap_or((0.0, 0.0));
            write_i32(&mut block, from_mm(x));
            write_i32(&mut block, from_mm(y));
        }
    }

    block
}

/// Encodes a Pad primitive.
fn encode_pad(data: &mut Vec<u8>, pad: &Pad) -> crate::altium::error::AltiumResult<()> {
    // Block 0: Designator string
    write_string_block(data, &pad.designator, "pad.designator")?;

    // Block 1: SubRecord 2 (empty string block = 1-byte 0x00, matching Altium).
    write_block(data, &[0u8]);

    // Block 2: "|&|0" string (standard marker)
    write_string_block(data, "|&|0", "pad.marker")?;

    // Block 3: SubRecord 4 (1-byte 0x00 block, matching Altium).
    write_block(data, &[0u8]);

    // Block 4: Geometry data (202 bytes)
    let geometry = encode_pad_geometry(pad);
    write_block(data, &geometry);

    // Block 5: the size/shape block. Three cases:
    //  - genuine per-layer (full-stack) data  -> legacy per-layer block;
    //  - a non-round hole or an explicit corner radius on a simple pad
    //    -> the canonical 596-byte size/shape block (carries hole type @262 and
    //       the rounded-rect corner radius @564, which the main block cannot);
    //  - otherwise (plain simple pad) -> EMPTY block (matches Altium).
    let needs_per_layer_data = pad.stack_mode != PadStackMode::Simple
        || pad.per_layer_sizes.is_some()
        || pad.per_layer_shapes.is_some()
        || pad.per_layer_corner_radii.is_some()
        || pad.per_layer_offsets.is_some();
    let needs_size_shape =
        pad.hole_shape != HoleShape::Round || pad.corner_radius_percent.is_some();

    if needs_per_layer_data {
        write_block(data, &encode_pad_per_layer_data(pad));
    } else if needs_size_shape {
        write_block(data, &encode_pad_size_shape_block(pad));
    } else {
        write_block(data, &[]);
    }

    Ok(())
}

/// Converts our `HoleShape` enum to the Altium hole-type id.
const fn hole_shape_to_id(shape: HoleShape) -> u8 {
    match shape {
        HoleShape::Round => 0,
        HoleShape::Square => 1,
        HoleShape::Slot => 2,
    }
}

/// Encodes the canonical 596-byte pad size/shape block (Block 5) for a simple
/// pad that needs a non-round hole or an explicit corner radius. Layout matches
/// `AltiumSharp` `WritePad` (values uniform across layers); the reader pairs
/// with this via [`super::reader`] when the block is >= 596 bytes.
fn encode_pad_size_shape_block(pad: &Pad) -> Vec<u8> {
    let mut b = Vec::with_capacity(596);
    let w = from_mm(pad.width);
    let h = from_mm(pad.height);
    let shape_id = pad_shape_to_id(pad.shape);
    let radius = pad
        .corner_radius_percent
        .unwrap_or(if pad.shape == PadShape::RoundedRectangle {
            50
        } else {
            0
        });

    for _ in 0..29 {
        write_i32(&mut b, w); // 0-115: internal-layer X sizes
    }
    for _ in 0..29 {
        write_i32(&mut b, h); // 116-231: internal-layer Y sizes
    }
    for _ in 0..29 {
        b.push(shape_id); // 232-260: internal-layer shapes
    }
    b.push(0); // 261: reserved
    b.push(hole_shape_to_id(pad.hole_shape)); // 262: hole type
    write_i32(&mut b, 0); // 263-266: hole slot length (not modelled)
    write_f64(&mut b, 0.0); // 267-274: hole rotation
    for _ in 0..32 {
        write_i32(&mut b, 0); // 275-402: per-layer X offsets
    }
    for _ in 0..32 {
        write_i32(&mut b, 0); // 403-530: per-layer Y offsets
    }
    b.push(u8::from(pad.shape == PadShape::RoundedRectangle)); // 531: has-rounded-rect
    for _ in 0..32 {
        b.push(shape_id); // 532-563: per-layer shapes
    }
    for _ in 0..32 {
        b.push(radius); // 564-595: per-layer corner radii (%)
    }
    debug_assert_eq!(b.len(), 596);
    b
}

/// Total length of the pad main geometry block (`SubRecord-5`) in a `PcbLib`:
/// 61 bytes of typed geometry plus a 141-byte extended tail = 202 bytes.
/// Altium Designer rejects pads whose main block is shorter (issue #68).
const PAD_MAIN_BLOCK_LEN: usize = 202;

/// First main-block offset of the pad extended tail.
const PAD_EXTENDED_TAIL_START: usize = 61;

/// Canonical 141-byte pad extended tail (main-block offsets 61-201), captured
/// from a standard Altium pad (`AltiumSharp` `PadExtendedTailTemplate`). The
/// typed/semantic fields are overlaid in [`build_pad_extended_tail`]; the
/// remaining bytes are reserved / cache / identity values reproduced verbatim
/// so the record matches Altium's 202-byte layout exactly.
#[rustfmt::skip]
const PAD_EXTENDED_TAIL_TEMPLATE: [u8; 141] = [
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,0xA0,0x86,0x01,0x00,0x04,0x00,0xA0,0x86,0x01, // 61-76
    0x00,0x40,0x0D,0x03,0x00,0x40,0x0D,0x03,0x00,0x00,0x00,0x00,0x00,0x40,0x9C,0x00, // 77-92
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x01,0x01,0x00,0x00,0x00,0x00,0x00,0x00, // 93-108
    0x00,0x00,0x00,0x00,0x00,0x0F,0x00,0x03,0x01,0x00,0x00,0x00,0x40,0x9C,0x00,0x00, // 109-124
    0x00,0x64,0x9A,0x92,0x26,0x10,0xC7,0xE4,0x41,0xA3,0x2B,0x29,0x17,0xA5,0x35,0x2E, // 125-140
    0x67,0x7F,0xAB,0x21,0x20,0xC3,0x0B,0x32,0x47,0xAD,0xCE,0x6C,0xB7,0xB8,0xC9,0x7E, // 141-156
    0x68,0x00,0x00,0x00,0x00,0xFF,0xFF,0xFF,0x7F,0xFF,0xFF,0xFF,0x7F,0x00,0x01,0x1A, // 157-172
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x01,0x01,0x00,0x00,0x00, // 173-188
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,                // 189-201
];

/// Derives the "v7 saved layer id" from an Altium layer number (must match the
/// primitive's layer). Ported from `AltiumSharp` `V7LayerId`.
fn v7_layer_id(layer: u8) -> u32 {
    let l = u32::from(layer);
    if layer == 32 {
        return 0x0100_FFFF; // bottom signal sentinel
    }
    if (1..=31).contains(&layer) {
        return 0x0100_0000 + l; // signal (top/mid)
    }
    if (39..=54).contains(&layer) {
        return 0x0101_0000 + (l - 38); // internal plane 1-16
    }
    if (57..=72).contains(&layer) {
        return 0x0102_0000 + (l - 56); // mechanical 1-16
    }
    match layer {
        33 => 0x0103_0006, // top overlay
        34 => 0x0103_0007, // bottom overlay
        35 => 0x0103_0008, // top paste
        36 => 0x0103_0009, // bottom paste
        37 => 0x0103_000A, // top solder
        38 => 0x0103_000B, // bottom solder
        55 => 0x0103_000C, // drill guide
        56 => 0x0103_000D, // keepout
        73 => 0x0103_000E, // drill drawing
        _ => 0x0103_000F,  // 74 multi-layer + fallback
    }
}

/// Builds the 141-byte pad extended tail by overlaying typed fields onto the
/// canonical template (matching `AltiumSharp` `BuildPadExtendedTail`).
fn build_pad_extended_tail(pad: &Pad) -> [u8; 141] {
    const START: usize = PAD_EXTENDED_TAIL_START;
    let mut tail = PAD_EXTENDED_TAIL_TEMPLATE;

    // 62: pad stack mode
    tail[62 - START] = pad_stack_mode_to_id(pad.stack_mode);
    // 86-89 / 90-93: paste & solder mask expansion
    tail[86 - START..90 - START]
        .copy_from_slice(&from_mm(pad.paste_mask_expansion.unwrap_or(0.0)).to_le_bytes());
    tail[90 - START..94 - START]
        .copy_from_slice(&from_mm(pad.solder_mask_expansion.unwrap_or(0.0)).to_le_bytes());
    // 101 / 102: paste & solder mask manual flags
    tail[101 - START] = u8::from(pad.paste_mask_expansion_manual);
    tail[102 - START] = u8::from(pad.solder_mask_expansion_manual);
    // 114-117: v7 layer id (derived from the pad's layer)
    tail[114 - START..118 - START]
        .copy_from_slice(&v7_layer_id(layer_to_id(pad.layer)).to_le_bytes());
    // 126-141 / 142-157: two per-pad identity GUIDs
    tail[126 - START..142 - START].copy_from_slice(&generate_guid());
    tail[142 - START..158 - START].copy_from_slice(&generate_guid());
    // 185: reserved marker (0x03 for a standard PcbLib pad)
    tail[185 - START] = 0x03;

    tail
}

/// Encodes the 202-byte geometry block (`SubRecord-5`) for a pad.
///
/// Offsets 0-60 are typed geometry (common header, location, sizes, hole,
/// shapes, rotation, plating); offsets 61-201 are the extended tail. See
/// [`build_pad_extended_tail`].
fn encode_pad_geometry(pad: &Pad) -> Vec<u8> {
    let mut block = Vec::with_capacity(PAD_MAIN_BLOCK_LEN);

    // Common header (13 bytes) - offsets 0-12
    write_common_header(&mut block, pad.layer, pad.flags);

    // Location (X, Y) - offsets 13-20
    write_i32(&mut block, from_mm(pad.x));
    write_i32(&mut block, from_mm(pad.y));

    // Size top/middle/bottom (X, Y) - offsets 21-44 (identical for simple pads)
    for _ in 0..3 {
        write_i32(&mut block, from_mm(pad.width));
        write_i32(&mut block, from_mm(pad.height));
    }

    // Hole size - offsets 45-48
    write_i32(&mut block, from_mm(pad.hole_size.unwrap_or(0.0)));

    // Shapes (top, middle, bottom) - offsets 49-51
    let shape_id = pad_shape_to_id(pad.shape);
    block.push(shape_id);
    block.push(shape_id);
    block.push(shape_id);

    // Rotation - offsets 52-59 (8-byte double)
    write_f64(&mut block, pad.rotation);

    // Is plated - offset 60
    block.push(u8::from(pad.hole_size.is_some()));

    // Extended tail - offsets 61-201 (141 bytes)
    block.extend_from_slice(&build_pad_extended_tail(pad));

    debug_assert_eq!(block.len(), PAD_MAIN_BLOCK_LEN);
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
fn encode_via(data: &mut Vec<u8>, via: &Via) -> crate::altium::error::AltiumResult<()> {
    // Block 0: Name/designator (empty for vias)
    write_block(data, &[0u8; 1]); // Single null byte for empty string

    // Block 1: Layer stack data (empty)
    write_block(data, &[]);

    // Block 2: "|&|0" marker string
    write_string_block(data, "|&|0", "via.marker")?;

    // Block 3: Net/connectivity data (empty for library vias)
    write_block(data, &[]);

    // Block 4: Geometry data
    let geometry = encode_via_geometry(via);
    write_block(data, &geometry);

    // Block 5: Per-layer data (empty for simple vias)
    write_block(data, &[]);

    Ok(())
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
    let mut block = Vec::with_capacity(256); // Larger for potential per-layer data

    // Common header (13 bytes) - use MultiLayer for vias
    // Note: Via primitive doesn't have flags field (different binary structure)
    write_common_header(&mut block, Layer::MultiLayer, PcbFlags::empty());

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

    // Thermal relief air gap width - offset 31
    write_i32(&mut block, from_mm(via.thermal_relief_gap));

    // Thermal relief conductors count - offset 35
    block.push(via.thermal_relief_conductors);

    // Thermal relief conductors width - offset 36
    write_i32(&mut block, from_mm(via.thermal_relief_width));

    // Solder mask expansion - offset 40
    write_i32(&mut block, from_mm(via.solder_mask_expansion));

    // Solder mask expansion manual flag - offset 44
    block.push(u8::from(via.solder_mask_expansion_manual));

    // Diameter stack mode - offset 45
    block.push(via_stack_mode_to_id(via.diameter_stack_mode));

    // Per-layer diameters - offset 46+ (32 × 4 bytes = 128 bytes)
    // Only write if stack mode is not Simple
    if via.diameter_stack_mode != ViaStackMode::Simple {
        if let Some(ref diameters) = via.per_layer_diameters {
            for i in 0..32 {
                let diameter = diameters.get(i).copied().unwrap_or(via.diameter);
                write_i32(&mut block, from_mm(diameter));
            }
        } else {
            // No per-layer data provided, use main diameter for all layers
            for _ in 0..32 {
                write_i32(&mut block, from_mm(via.diameter));
            }
        }
    }

    // Pad to minimum expected size
    while block.len() < 46 {
        block.push(0x00);
    }

    block
}

/// Converts a `ViaStackMode` to its binary ID.
const fn via_stack_mode_to_id(mode: ViaStackMode) -> u8 {
    match mode {
        ViaStackMode::Simple => 0,
        ViaStackMode::TopMiddleBottom => 1,
        ViaStackMode::FullStack => 2,
    }
}

/// Converts a `PadStackMode` to its binary ID.
const fn pad_stack_mode_to_id(mode: PadStackMode) -> u8 {
    match mode {
        PadStackMode::Simple => 0,
        PadStackMode::TopMiddleBottom => 1,
        PadStackMode::FullStack => 2,
    }
}

/// Converts a `TextKind` to its binary ID.
const fn text_kind_to_id(kind: TextKind) -> u8 {
    match kind {
        TextKind::Stroke => 0,
        TextKind::TrueType => 1,
        TextKind::BarCode => 2,
    }
}

/// Converts a `StrokeFont` to its binary font-table ID. Altium's default
/// stroke font is index 1, so the ids are 1-based.
const fn stroke_font_to_id(font: StrokeFont) -> u16 {
    match font {
        StrokeFont::Default => 1,
        StrokeFont::SansSerif => 2,
        StrokeFont::Serif => 3,
    }
}

/// Encodes a Track primitive.
fn encode_track(data: &mut Vec<u8>, track: &Track) {
    let mut block = Vec::with_capacity(64);

    // Common header (13 bytes)
    write_common_header(&mut block, track.layer, track.flags);

    // Start coordinates (X, Y) - offsets 13-20
    write_i32(&mut block, from_mm(track.x1));
    write_i32(&mut block, from_mm(track.y1));

    // End coordinates (X, Y) - offsets 21-28
    write_i32(&mut block, from_mm(track.x2));
    write_i32(&mut block, from_mm(track.y2));

    // Width - offset 29-32
    write_i32(&mut block, from_mm(track.width));

    // Extended tail (offsets 33-48) — every Altium-authored track carries it.
    // Ported from `AltiumSharp` `WriteTrack`.
    block.extend_from_slice(&0i16.to_le_bytes()); // 33-34 subpoly index
    write_i32(&mut block, 0); // 35-38 solder mask expansion
    block.extend_from_slice(&0i16.to_le_bytes()); // 39-40 paste mask expansion
    write_u32(&mut block, v7_layer_id(layer_to_id(track.layer))); // 41-44 v7 layer id
    block.push(0); // 45 keepout restrictions
    block.extend_from_slice(&[0u8; 3]); // 46-48 reserved

    write_block(data, &block);
}

/// Encodes an Arc primitive.
fn encode_arc(data: &mut Vec<u8>, arc: &Arc) {
    let mut block = Vec::with_capacity(64);

    // Common header (13 bytes)
    write_common_header(&mut block, arc.layer, arc.flags);

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

    // Extended tail (offsets 45-59) — every Altium-authored arc carries it.
    // Ported from `AltiumSharp` `WriteArc` (note: 1-byte paste-mask field,
    // versus the track's 2-byte field).
    block.extend_from_slice(&0i16.to_le_bytes()); // 45-46 subpoly index
    write_i32(&mut block, 0); // 47-50 solder mask expansion
    block.push(0); // 51 paste mask expansion
    write_u32(&mut block, v7_layer_id(layer_to_id(arc.layer))); // 52-55 v7 layer id
    block.push(0); // 56 keepout restrictions
    block.extend_from_slice(&[0u8; 3]); // 57-59 reserved

    write_block(data, &block);
}

/// Encodes a Text primitive.
///
/// Text has 2 blocks:
/// - Block 0: Geometry/metadata (layer, position, height, rotation, font info)
/// - Block 1: Text content (length-prefixed string)
fn encode_text(data: &mut Vec<u8>, text: &Text) -> crate::altium::error::AltiumResult<()> {
    // Block 0: Geometry
    let geometry = encode_text_geometry(text);
    write_block(data, &geometry);

    // Block 1: Text content
    write_string_block(data, &text.text, "text.text")?;

    Ok(())
}

/// Canonical 252-byte text SubRecord-1 (offsets 0-251), ported from
/// `AltiumSharp` `TextSr1Template`. Offsets 0-12 are the common header
/// (overwritten per-text), 13-251 carry the geometry/font/text-box/frame
/// fields; the reserved bytes are replayed verbatim and the typed fields
/// overlaid at their offsets. The default font field is "Arial" (UTF-16).
#[rustfmt::skip]
const TEXT_SR1_TEMPLATE: [u8; 252] = [
    0x21, 0x0C, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
    0x00, 0x50, 0x8E, 0xF4, 0xFF, 0x80, 0x1A, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x80, 0x46, 0x40, 0x00, 0x40, 0x9C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x41, 0x00,
    0x72, 0x00, 0x69, 0x00, 0x61, 0x00, 0x6C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xCE, 0xE5, 0x29, 0x00,
    0x7F, 0x52, 0x07, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0xA0, 0x37, 0xA0, 0x00, 0x20, 0x0B, 0x20,
    0x00, 0x40, 0x0D, 0x03, 0x00, 0x40, 0x0D, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x00, 0x41, 0x00, 0x72, 0x00, 0x69, 0x00, 0x61, 0x00, 0x6C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x01, 0x06, 0x00, 0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x80,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x50, 0x8E, 0xF4, 0xFF,
];

/// Encodes the 252-byte geometry block for a text primitive, overlaying the
/// typed fields onto the canonical template. Mirrors `AltiumSharp`
/// `BuildTextExtended`: the common header occupies offsets 0-12 and every
/// varying field is written at its fixed offset. Real Altium text records are
/// always this fixed 252-byte block — the previous ~80-byte guessed layout put
/// the kind, stroke width, font id and v7 layer id at the wrong places.
fn encode_text_geometry(text: &Text) -> Vec<u8> {
    let mut block = TEXT_SR1_TEMPLATE;

    // Common header (offsets 0-12): layer + Altium flag word + 0xFF net/poly/comp.
    let mut header = Vec::with_capacity(13);
    write_common_header(&mut header, text.layer, text.flags);
    block[..13].copy_from_slice(&header);

    // Position and height (offsets 13-24).
    block[13..17].copy_from_slice(&from_mm(text.x).to_le_bytes());
    block[17..21].copy_from_slice(&from_mm(text.y).to_le_bytes());
    block[21..25].copy_from_slice(&from_mm(text.height).to_le_bytes());

    // Font-table index (offsets 25-26); the default stroke font is index 1.
    let font_id = text.stroke_font.map_or(1, stroke_font_to_id);
    block[25..27].copy_from_slice(&font_id.to_le_bytes());

    // Rotation (offsets 27-34, f64 degrees).
    block[27..35].copy_from_slice(&text.rotation.to_le_bytes());

    // Authoritative text kind (offset 160).
    block[160] = text_kind_to_id(text.kind);

    // v7 layer id (offsets 226-229), derived from the layer.
    block[226..230].copy_from_slice(&v7_layer_id(layer_to_id(text.layer)).to_le_bytes());

    block.to_vec()
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

    // Build parameter string (leading pipe, matching Altium).
    let layer_name = region.layer.as_str().replace(' ', "").to_uppercase();
    let params = format!("|V7_LAYER={layer_name}|NAME=|KIND=0");
    let params_bytes = params.as_bytes();

    let mut block = Vec::with_capacity(22 + params_bytes.len() + 4 + vertex_count * 16);

    // Common header (13 bytes)
    write_common_header(&mut block, region.layer, region.flags);

    // Unknown bytes (5 bytes)
    block.extend_from_slice(&[0x00; 5]);

    // C-string parameter block (length includes the null terminator).
    write_cstring_param_block(&mut block, params_bytes);

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
    write_common_header(&mut block, fill.layer, fill.flags);

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
///
/// Altium writes exactly ONE size-prefixed block per body (verified against
/// `AltiumSharp` and the `BODY_3D`/`BODY_3D_STEP` golden libraries) — the outline
/// lives inside that block. Emitting extra empty blocks would be read back as a
/// bogus object-id-0 primitive and desynchronise the record stream (the same
/// class of bug as the trailing-`0x00` end marker removed for #68).
fn encode_component_body(data: &mut Vec<u8>, body: &ComponentBody, outline: &[(f64, f64)]) {
    let block = encode_component_body_block(body, outline);
    write_block(data, &block);
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
/// [vertex_count:4]             // Outline vertex count
/// [vertices...]                // Outline vertices: f64 x, f64 y (internal units)
/// ```
#[allow(clippy::cast_possible_truncation)] // Parameter strings + outlines are always small
fn encode_component_body_block(body: &ComponentBody, outline: &[(f64, f64)]) -> Vec<u8> {
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

    // Parameter string as a C-string block (length includes the null).
    let param_str = build_component_body_params(body);
    write_cstring_param_block(&mut block, param_str.as_bytes());

    // Outline polygon: vertex count then (f64 x, f64 y) per vertex, in Altium
    // internal units (1 mil = 10000). Altium needs a non-empty outline to place
    // and render the body.
    write_u32(&mut block, outline.len() as u32);
    for &(x, y) in outline {
        write_f64(&mut block, x * MM_TO_INTERNAL_UNITS);
        write_f64(&mut block, y * MM_TO_INTERNAL_UNITS);
    }

    block
}

/// Resolves the outline to write for a `ComponentBody`.
///
/// Uses the body's explicit outline when present (e.g. preserved from a file we
/// read); otherwise synthesises a rectangle from the footprint's pad extent so
/// the body is never written with a degenerate (empty) outline. Falls back to a
/// ±1 mm square when the footprint has no pads. Vertices are wound to match
/// Altium's convention: top-left, bottom-left, bottom-right, top-right.
fn resolve_body_outline(body: &ComponentBody, footprint: &Footprint) -> Vec<(f64, f64)> {
    if !body.outline.is_empty() {
        return body.outline.clone();
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for pad in &footprint.pads {
        min_x = min_x.min(pad.x - pad.width / 2.0);
        max_x = max_x.max(pad.x + pad.width / 2.0);
        min_y = min_y.min(pad.y - pad.height / 2.0);
        max_y = max_y.max(pad.y + pad.height / 2.0);
    }
    if !min_x.is_finite() {
        // No pads to bound — use a small default square.
        min_x = -1.0;
        min_y = -1.0;
        max_x = 1.0;
        max_y = 1.0;
    }

    vec![
        (min_x, max_y),
        (min_x, min_y),
        (max_x, min_y),
        (max_x, max_y),
    ]
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
    // Altium repeats ARCRESOLUTION after BODYPROJECTION (verbatim shape from the
    // BODY_3D golden files).
    params.push("ARCRESOLUTION=0.5mil".to_string());
    params.push("BODYCOLOR3D=8421504".to_string());
    params.push("BODYOPACITY3D=1.000".to_string());
    params.push("IDENTIFIER=".to_string());
    params.push("TEXTURE=".to_string());
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

// =============================================================================
// 3D Model Writing
// =============================================================================

use super::primitives::EmbeddedModel;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write as IoWrite;

/// Compresses model data using zlib.
///
/// # Arguments
///
/// * `data` - The uncompressed STEP file data
///
/// # Returns
///
/// Zlib-compressed data, or an error if compression fails.
///
/// # Errors
///
/// Returns `AltiumError::CompressionError` if the data cannot be compressed.
pub fn compress_model_data(data: &[u8]) -> crate::altium::error::AltiumResult<Vec<u8>> {
    use crate::altium::error::AltiumError;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).map_err(|e| {
        AltiumError::compression_error("Failed to write data to zlib encoder", Some(e))
    })?;
    encoder
        .finish()
        .map_err(|e| AltiumError::compression_error("Failed to finish zlib compression", Some(e)))
}

/// Encodes the `/Library/Models/Header` stream.
///
/// # Format
///
/// The Header stream is a 4-byte little-endian unsigned integer containing
/// the number of embedded models in the library.
#[allow(clippy::cast_possible_truncation)] // Model count always fits in u32
pub fn encode_model_header_stream(model_count: usize) -> Vec<u8> {
    (model_count as u32).to_le_bytes().to_vec()
}

/// Encodes the `/Library/Models/Data` stream.
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
/// Each record contains pipe-delimited key=value pairs:
/// - `EMBED=TRUE` - Indicates model is embedded
/// - `MODELSOURCE=Undefined` - Model source
/// - `ID={GUID}` - The model's unique identifier
/// - `ROTX=0.000|ROTY=0.000|ROTZ=0.000` - Rotation values
/// - `DZ=0` - Z offset
/// - `CHECKSUM={value}` - Model checksum
/// - `NAME=filename.step` - The model filename
#[allow(clippy::cast_possible_truncation)] // Record lengths are always small enough for u32
pub fn encode_model_data_stream(models: &[EmbeddedModel]) -> Vec<u8> {
    let mut output = Vec::new();

    for model in models {
        // Pipe-delimited parameters, NO leading pipe (matches AltiumSharp's
        // string.Join and every BODY_3D golden, whose record starts at EMBED=).
        let record = format!(
            "EMBED=TRUE|MODELSOURCE=Undefined|ID={}|ROTX=0.000|ROTY=0.000|ROTZ=0.000|DZ=0|CHECKSUM=0|NAME={}",
            model.id, model.name
        );
        // C-string parameter block (length includes the null terminator).
        write_cstring_param_block(&mut output, record.as_bytes());
    }

    output
}

/// Prepares models for writing by compressing and indexing them.
///
/// # Returns
///
/// A vector of (index, `compressed_data`) tuples, or an error if compression fails.
///
/// # Errors
///
/// Returns `AltiumError::CompressionError` if any model data cannot be compressed.
pub fn prepare_models_for_writing(
    models: &[EmbeddedModel],
) -> crate::altium::error::AltiumResult<Vec<(usize, Vec<u8>)>> {
    models
        .iter()
        .enumerate()
        .map(|(idx, model)| Ok((idx, compress_model_data(&model.data)?)))
        .collect()
}

// =============================================================================
// UniqueIDPrimitiveInformation Writing
// =============================================================================

/// Encodes the `UniqueIDPrimitiveInformation/Data` stream for a footprint.
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
/// * `footprint` - The footprint containing primitives with unique IDs
///
/// # Returns
///
/// The encoded stream data, or `None` if no primitives have unique IDs.
#[allow(clippy::cast_possible_truncation)]
pub fn encode_unique_id_stream(footprint: &Footprint) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    let mut has_any_id = false;

    // Encode each primitive type with its unique IDs
    // Index is 0-based (AltiumSharp convention)

    // Pads
    for (i, pad) in footprint.pads.iter().enumerate() {
        if let Some(ref uid) = pad.unique_id {
            encode_unique_id_record(&mut data, i, "Pad", uid);
            has_any_id = true;
        }
    }

    // Vias
    for (i, via) in footprint.vias.iter().enumerate() {
        if let Some(ref uid) = via.unique_id {
            encode_unique_id_record(&mut data, i, "Via", uid);
            has_any_id = true;
        }
    }

    // Tracks
    for (i, track) in footprint.tracks.iter().enumerate() {
        if let Some(ref uid) = track.unique_id {
            encode_unique_id_record(&mut data, i, "Track", uid);
            has_any_id = true;
        }
    }

    // Arcs
    for (i, arc) in footprint.arcs.iter().enumerate() {
        if let Some(ref uid) = arc.unique_id {
            encode_unique_id_record(&mut data, i, "Arc", uid);
            has_any_id = true;
        }
    }

    // Regions
    for (i, region) in footprint.regions.iter().enumerate() {
        if let Some(ref uid) = region.unique_id {
            encode_unique_id_record(&mut data, i, "Region", uid);
            has_any_id = true;
        }
    }

    // Text
    for (i, text) in footprint.text.iter().enumerate() {
        if let Some(ref uid) = text.unique_id {
            encode_unique_id_record(&mut data, i, "Text", uid);
            has_any_id = true;
        }
    }

    // Fills
    for (i, fill) in footprint.fills.iter().enumerate() {
        if let Some(ref uid) = fill.unique_id {
            encode_unique_id_record(&mut data, i, "Fill", uid);
            has_any_id = true;
        }
    }

    // ComponentBodies
    for (i, body) in footprint.component_bodies.iter().enumerate() {
        if let Some(ref uid) = body.unique_id {
            encode_unique_id_record(&mut data, i, "ComponentBody", uid);
            has_any_id = true;
        }
    }

    if has_any_id {
        Some(data)
    } else {
        None
    }
}

/// Encodes a single unique ID record.
///
/// # Format
///
/// ```text
/// [block_len:4 LE u32]["|PRIMITIVEINDEX=...|PRIMITIVEOBJECTID=...|UNIQUEID=..." + \x00]
/// ```
///
/// Block length includes the null terminator.
#[allow(clippy::cast_possible_truncation)]
fn encode_unique_id_record(
    data: &mut Vec<u8>,
    index: usize,
    primitive_type: &str,
    unique_id: &str,
) {
    let record =
        format!("|PRIMITIVEINDEX={index}|PRIMITIVEOBJECTID={primitive_type}|UNIQUEID={unique_id}");
    // C-string parameter block (length includes the null terminator).
    write_cstring_param_block(data, record.as_bytes());
}

// =============================================================================
// Per-Component Header Writing
// =============================================================================

/// Encodes the per-component `Header` stream.
///
/// # Format
///
/// 4-byte little-endian unsigned integer containing the exact primitive count.
#[allow(clippy::cast_possible_truncation)]
pub fn encode_component_header(footprint: &Footprint) -> Vec<u8> {
    let count = footprint.arcs.len()
        + footprint.pads.len()
        + footprint.vias.len()
        + footprint.tracks.len()
        + footprint.text.len()
        + footprint.regions.len()
        + footprint.fills.len()
        + footprint.component_bodies.len();

    (count as u32).to_le_bytes().to_vec()
}

/// Generates a random GUID as 16 bytes (little-endian UUID format).
fn generate_guid() -> [u8; 16] {
    use uuid::Uuid;
    *Uuid::new_v4().as_bytes()
}

// =============================================================================
// Per-Component WideStrings Writing
// =============================================================================

/// Encodes the per-component `WideStrings` stream.
///
/// # Format
///
/// ```text
/// [length:4 LE u32][content with null terminator]
/// ```
///
/// Format: `[block_len:4]["|ENCODEDTEXT0=...|..." + \x00]`
///
/// Empty: `[block_len:4]["|" + \x00]` (`block_len` = 2)
pub fn encode_component_wide_strings(footprint: &Footprint) -> Vec<u8> {
    use std::fmt::Write;

    // Collect text content from this footprint
    let mut texts: Vec<&str> = Vec::new();

    for text in &footprint.text {
        if !text.text.starts_with('.') && !text.text.is_empty() {
            texts.push(&text.text);
        }
    }

    // Build encoded text parameter string (always starts with "|")
    let mut content = String::from("|");
    for (index, text) in texts.iter().enumerate() {
        let encoded: String = text
            .bytes()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let _ = write!(content, "ENCODEDTEXT{index}={encoded}|");
    }

    // Block format: [block_len:4][content + \x00] (length includes the null).
    let content_bytes = content.as_bytes();
    let mut data = Vec::with_capacity(4 + content_bytes.len() + 1);
    write_cstring_param_block(&mut data, content_bytes);

    data
}

// =============================================================================
// UniqueIDPrimitiveInformation Header
// =============================================================================

/// Encodes the `UniqueIDPrimitiveInformation/Header` stream.
///
/// # Format
///
/// 4-byte little-endian unsigned integer containing the record count.
#[allow(clippy::cast_possible_truncation)]
pub fn encode_unique_id_header(footprint: &Footprint) -> Vec<u8> {
    let count = count_unique_ids(footprint);
    (count as u32).to_le_bytes().to_vec()
}

/// Counts primitives with unique IDs.
fn count_unique_ids(footprint: &Footprint) -> usize {
    let mut count = 0;

    for pad in &footprint.pads {
        if pad.unique_id.is_some() {
            count += 1;
        }
    }
    for via in &footprint.vias {
        if via.unique_id.is_some() {
            count += 1;
        }
    }
    for track in &footprint.tracks {
        if track.unique_id.is_some() {
            count += 1;
        }
    }
    for arc in &footprint.arcs {
        if arc.unique_id.is_some() {
            count += 1;
        }
    }
    for text in &footprint.text {
        if text.unique_id.is_some() {
            count += 1;
        }
    }
    for region in &footprint.regions {
        if region.unique_id.is_some() {
            count += 1;
        }
    }
    for fill in &footprint.fills {
        if fill.unique_id.is_some() {
            count += 1;
        }
    }
    for body in &footprint.component_bodies {
        if body.unique_id.is_some() {
            count += 1;
        }
    }

    count
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
        write_string_block(&mut data, "TEST", "test_field").expect("should succeed");
        // Block length (5) + string length (4) + "TEST"
        assert_eq!(
            data,
            vec![0x05, 0x00, 0x00, 0x00, 0x04, b'T', b'E', b'S', b'T']
        );
    }

    #[test]
    fn test_write_string_block_too_long() {
        let mut data = Vec::new();
        let long_string = "A".repeat(256);
        let result = write_string_block(&mut data, &long_string, "test_field");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("exceeds maximum of 255 bytes"));
    }

    #[test]
    fn test_layer_to_id() {
        // Copper layers
        assert_eq!(layer_to_id(Layer::TopLayer), 1);
        assert_eq!(layer_to_id(Layer::BottomLayer), 32);
        assert_eq!(layer_to_id(Layer::MultiLayer), 74);

        // Mid layers (2-31)
        assert_eq!(layer_to_id(Layer::MidLayer1), 2);
        assert_eq!(layer_to_id(Layer::MidLayer2), 3);
        assert_eq!(layer_to_id(Layer::MidLayer15), 16);
        assert_eq!(layer_to_id(Layer::MidLayer30), 31);

        // Silkscreen and mask layers
        assert_eq!(layer_to_id(Layer::TopOverlay), 33);
        assert_eq!(layer_to_id(Layer::BottomOverlay), 34);
        assert_eq!(layer_to_id(Layer::TopPaste), 35);
        assert_eq!(layer_to_id(Layer::BottomPaste), 36);
        assert_eq!(layer_to_id(Layer::TopSolder), 37);
        assert_eq!(layer_to_id(Layer::BottomSolder), 38);

        // Internal planes (39-54)
        assert_eq!(layer_to_id(Layer::InternalPlane1), 39);
        assert_eq!(layer_to_id(Layer::InternalPlane2), 40);
        assert_eq!(layer_to_id(Layer::InternalPlane16), 54);

        // Drill layers
        assert_eq!(layer_to_id(Layer::DrillGuide), 55);
        assert_eq!(layer_to_id(Layer::KeepOut), 56);
        assert_eq!(layer_to_id(Layer::DrillDrawing), 73);

        // Mechanical layers (57-72)
        assert_eq!(layer_to_id(Layer::Mechanical1), 57);
        // Component layer pairs (aliased to mechanical 2-7)
        assert_eq!(layer_to_id(Layer::TopAssembly), 58);
        assert_eq!(layer_to_id(Layer::BottomAssembly), 59);
        assert_eq!(layer_to_id(Layer::TopCourtyard), 60);
        assert_eq!(layer_to_id(Layer::BottomCourtyard), 61);
        assert_eq!(layer_to_id(Layer::Top3DBody), 62);
        assert_eq!(layer_to_id(Layer::Bottom3DBody), 63);
        // Mechanical aliases for the same layers
        assert_eq!(layer_to_id(Layer::Mechanical2), 58);
        assert_eq!(layer_to_id(Layer::Mechanical3), 59);
        assert_eq!(layer_to_id(Layer::Mechanical4), 60);
        assert_eq!(layer_to_id(Layer::Mechanical5), 61);
        assert_eq!(layer_to_id(Layer::Mechanical6), 62);
        assert_eq!(layer_to_id(Layer::Mechanical7), 63);
        assert_eq!(layer_to_id(Layer::Mechanical8), 64);
        assert_eq!(layer_to_id(Layer::Mechanical16), 72);

        // Special layers (75-85)
        assert_eq!(layer_to_id(Layer::ConnectLayer), 75);
        assert_eq!(layer_to_id(Layer::BackgroundLayer), 76);
        assert_eq!(layer_to_id(Layer::DRCErrorLayer), 77);
        assert_eq!(layer_to_id(Layer::HighlightLayer), 78);
        assert_eq!(layer_to_id(Layer::GridColor1), 79);
        assert_eq!(layer_to_id(Layer::GridColor10), 80);
        assert_eq!(layer_to_id(Layer::PadHoleLayer), 81);
        assert_eq!(layer_to_id(Layer::ViaHoleLayer), 82);
        assert_eq!(layer_to_id(Layer::TopPadMaster), 83);
        assert_eq!(layer_to_id(Layer::BottomPadMaster), 84);
        assert_eq!(layer_to_id(Layer::DRCDetailLayer), 85);
    }

    #[test]
    fn test_encode_simple_footprint() {
        let mut fp = Footprint::new("TEST_FP");
        fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));

        let data = encode_data_stream(&fp).expect("encoding should succeed");

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

    // =============================================================================
    // 3D Model Writing Tests
    // =============================================================================

    #[test]
    fn test_compress_model_data() {
        use flate2::read::ZlibDecoder;
        use std::io::Read;

        let original = b"ISO-10303-21; HEADER; FILE_DESCRIPTION...";
        let compressed = compress_model_data(original).expect("compression should succeed");

        // Verify it's actually compressed (should be smaller for larger data)
        assert!(!compressed.is_empty());

        // Verify we can decompress it
        let mut decoder = ZlibDecoder::new(&compressed[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();

        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_encode_model_header_stream() {
        // Header is a 4-byte LE u32
        let data = encode_model_header_stream(5);
        assert_eq!(data.len(), 4);
        assert_eq!(data, [0x05, 0x00, 0x00, 0x00]);

        let data = encode_model_header_stream(13);
        assert_eq!(data, [0x0d, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_encode_model_data_stream() {
        let models = vec![
            EmbeddedModel::new("{GUID-1}", "model1.step", vec![]),
            EmbeddedModel::new("{GUID-2}", "model2.step", vec![]),
        ];

        let data = encode_model_data_stream(&models);

        // Verify we can parse it back with our reader
        let parsed = super::super::reader::parse_model_data_stream(&data);
        assert_eq!(parsed.len(), 2);

        // Check GUID-1 maps to stream index 0
        let (idx1, name1) = parsed.get("{GUID-1}").expect("Should have GUID-1");
        assert_eq!(*idx1, 0);
        assert_eq!(name1, "model1.step");

        // Check GUID-2 maps to stream index 1
        let (idx2, name2) = parsed.get("{GUID-2}").expect("Should have GUID-2");
        assert_eq!(*idx2, 1);
        assert_eq!(name2, "model2.step");
    }

    #[test]
    fn test_encode_model_data_stream_empty() {
        let models: Vec<EmbeddedModel> = vec![];
        let data = encode_model_data_stream(&models);
        assert!(data.is_empty());
    }

    #[test]
    fn test_prepare_models_for_writing() {
        let models = vec![
            EmbeddedModel::new("{A}", "a.step", b"STEP A".to_vec()),
            EmbeddedModel::new("{B}", "b.step", b"STEP B".to_vec()),
        ];

        let prepared = prepare_models_for_writing(&models).expect("compression should succeed");

        assert_eq!(prepared.len(), 2);
        assert_eq!(prepared[0].0, 0);
        assert_eq!(prepared[1].0, 1);

        // Verify each is compressed
        assert!(!prepared[0].1.is_empty());
        assert!(!prepared[1].1.is_empty());
    }
}
