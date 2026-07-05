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
    StrokeFont, Text, TextJustification, TextKind, Track, Via, ViaStackMode,
};
use super::Footprint;

use super::units::{from_mm, mm_to_mil};

/// Writes a 4-byte little-endian unsigned integer.
fn write_u32(data: &mut Vec<u8>, value: u32) {
    data.extend_from_slice(&value.to_le_bytes());
}

/// Writes a 2-byte little-endian unsigned integer.
fn write_u16(data: &mut Vec<u8>, value: u16) {
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
use crate::altium::framing::{write_block, write_cstring_param_block};

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

    crate::altium::framing::write_string_block(data, &bytes);
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
/// RoundedRectangle=9.
const fn pad_shape_to_id(shape: PadShape) -> u8 {
    match shape {
        // Altium has no oval shape: an oval pad is a Round pad with width≠height,
        // so `Oval` also serialises as Round (id 1).
        PadShape::Round | PadShape::Oval => 1,
        PadShape::Rectangle => 2,
        PadShape::Octagonal => 3,
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

/// Overlays the common-header connectivity indices onto the `0xFF` fill
/// [`write_common_header`] writes: net index (u16 @3-4), polygon index
/// (u16 @5-6) and component index (i32 modelled, stored as u16 @7-8 with
/// `-1` -> the `0xFFFF` sentinel).
///
/// The from-scratch "none" defaults — `net = 0xFFFF`, `polygon = 0xFFFF`,
/// `component = -1` (-> `0xFFFF`) — reproduce the header's `0xFF FF` bytes exactly,
/// so a default primitive stays byte-identical to the previous hard-coded output
/// (the oracle depends on this). `block` must be at least 9 bytes long.
///
/// Mirrors how [`encode_region_properties`] / [`encode_via`] already overlay these
/// bytes; factored so every primitive encoder shares one implementation.
fn write_common_indices(
    block: &mut [u8],
    net_index: u16,
    polygon_index: u16,
    component_index: i32,
) {
    block[3..5].copy_from_slice(&net_index.to_le_bytes());
    block[5..7].copy_from_slice(&polygon_index.to_le_bytes());
    // -1 (free primitive) and any out-of-range value store as the 0xFFFF sentinel.
    let component_word = u16::try_from(component_index).unwrap_or(0xFFFF);
    block[7..9].copy_from_slice(&component_word.to_le_bytes());
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
        encode_via(&mut data, via);
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
    //  - otherwise (plain simple/TopMiddleBottom pad) -> EMPTY block (matches Altium).
    //
    // Only FullStack emits the 32-entry per-layer block. A TopMiddleBottom pad
    // carries its top/mid/bottom sizes+shapes in the MAIN geometry block (see
    // `encode_pad_geometry`) and keeps Block 5 empty, matching the golden.
    let needs_per_layer_data = pad.stack_mode == PadStackMode::FullStack;
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
    write_i32(&mut b, from_mm(pad.hole_slot_length)); // 263-266: hole slot length
    write_f64(&mut b, pad.hole_rotation); // 267-274: hole rotation
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

    // Full-stack tail. Altium NEVER emits a bare 596-byte size/shape block — every
    // non-empty block in a golden .PcbLib is 651 (one entry) or 696 (four). It is
    // `[32 reserved][i32 count][i32 stride=15]` then `count × 15`-byte entries; a
    // 596-byte block is under-length and Altium rejects the pad (issue #68/#113
    // class). We emit the single-entry (count=1) form; the multi-entry full-stack
    // form (PadStackMode::FullStack) is deferred. The entry's corner byte is a fixed
    // `50` in every golden (not the body radius), and layer code 4 = top signal.
    b.extend_from_slice(&[0u8; 32]); // 596-627: reserved
    write_i32(&mut b, 1); // 628-631: entry count
    write_i32(&mut b, 15); // 632-635: entry stride
    b.push(4); // 636: layer code (top signal)
    b.push(0); // 637: flag1
    b.push(0x80); // 638: flag2
    b.push(1); // 639: flag3
    b.push(shape_id); // 640: flag4 = pad shape id
    write_i32(&mut b, w); // 641-644: entry size X
    write_i32(&mut b, h); // 645-648: entry size Y
    b.push(50); // 649: corner radius % (fixed 50 across all goldens)
    b.push(0); // 650: trailing
    debug_assert_eq!(b.len(), 651);
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
    // Thermal-relief / power-plane connection fields. Each default equals the
    // template constant at its offset (style 0; conductor width / air gap
    // 100000 = 0.254mm; entries 4; relief expansion / clearance 200000 =
    // 0.508mm), so a default pad stays byte-identical. See
    // PAD_EXTENDED_TAIL_TEMPLATE.
    // 67: power-plane connection style (0=Relief, 1=Direct, 2=NoConnect)
    tail[67 - START] = pad.power_plane_connect_style.to_id();
    // 68-71: thermal-relief conductor (spoke) width
    tail[68 - START..72 - START]
        .copy_from_slice(&from_mm(pad.relief_conductor_width).to_le_bytes());
    // 72-73: thermal-relief spoke count (i16)
    tail[72 - START..74 - START].copy_from_slice(&pad.relief_entries.to_le_bytes());
    // 74-77: thermal-relief air gap
    tail[74 - START..78 - START].copy_from_slice(&from_mm(pad.relief_air_gap).to_le_bytes());
    // 78-81: power-plane relief expansion
    tail[78 - START..82 - START]
        .copy_from_slice(&from_mm(pad.power_plane_relief_expansion).to_le_bytes());
    // 82-85: power-plane (anti-pad) clearance
    tail[82 - START..86 - START].copy_from_slice(&from_mm(pad.power_plane_clearance).to_le_bytes());
    // 86-89 / 90-93: paste & solder mask expansion
    tail[86 - START..90 - START]
        .copy_from_slice(&from_mm(pad.paste_mask_expansion.unwrap_or(0.0)).to_le_bytes());
    tail[90 - START..94 - START]
        .copy_from_slice(&from_mm(pad.solder_mask_expansion.unwrap_or(0.0)).to_le_bytes());
    // 101 / 102: paste & solder mask expansion modes (tri-state, 0/1/2)
    tail[101 - START] = pad.paste_mask_expansion_mode.to_id();
    tail[102 - START] = pad.solder_mask_expansion_mode.to_id();
    // 114-117: v7 layer id (derived from the pad's layer)
    tail[114 - START..118 - START]
        .copy_from_slice(&v7_layer_id(layer_to_id(pad.layer)).to_le_bytes());
    // 126-141 / 142-157: two per-pad identity GUIDs
    tail[126 - START..142 - START].copy_from_slice(&generate_guid());
    tail[142 - START..158 - START].copy_from_slice(&generate_guid());
    // 162-165 / 166-169: drill tolerances. `None` leaves the template's
    // 0x7FFFFFFF "unset" sentinel (byte-identical); `Some(mm)` writes the raw.
    if let Some(tol) = pad.hole_positive_tolerance {
        tail[162 - START..166 - START].copy_from_slice(&from_mm(tol).to_le_bytes());
    }
    if let Some(tol) = pad.hole_negative_tolerance {
        tail[166 - START..170 - START].copy_from_slice(&from_mm(tol).to_le_bytes());
    }
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

    // Common header (13 bytes) - offsets 0-12 + connectivity indices @3-8.
    write_common_header(&mut block, pad.layer, pad.flags);
    write_common_indices(
        &mut block,
        pad.net_index,
        pad.polygon_index,
        pad.component_index,
    );

    // Location (X, Y) - offsets 13-20
    write_i32(&mut block, from_mm(pad.x));
    write_i32(&mut block, from_mm(pad.y));

    // Size top/middle/bottom (X, Y) - offsets 21-44.
    //
    // For a TopMiddleBottom (LocalStack) pad the mid/bottom sizes and shapes
    // live in THIS main block (Block 5 stays empty), so we pull the distinct
    // mid/bottom values from `per_layer_sizes`/`per_layer_shapes` ([top, mid,
    // bottom]). For Simple/FullStack pads all three slots are the top size and
    // shape (FullStack carries its per-layer data in Block 5 instead).
    let is_tmb = pad.stack_mode == PadStackMode::TopMiddleBottom;
    let shape_id = pad_shape_to_id(pad.shape);
    let tmb_size = |index: usize| -> (f64, f64) {
        if is_tmb {
            pad.per_layer_sizes
                .as_ref()
                .and_then(|sizes| sizes.get(index).copied())
                .unwrap_or((pad.width, pad.height))
        } else {
            (pad.width, pad.height)
        }
    };
    let tmb_shape_id = |index: usize| -> u8 {
        if is_tmb {
            pad.per_layer_shapes
                .as_ref()
                .and_then(|shapes| shapes.get(index).copied())
                .map_or(shape_id, pad_shape_to_id)
        } else {
            shape_id
        }
    };

    // Top @21/25, mid @29/33, bottom @37/41.
    for index in 0..3 {
        let (w, h) = tmb_size(index);
        write_i32(&mut block, from_mm(w));
        write_i32(&mut block, from_mm(h));
    }

    // Hole size - offsets 45-48
    write_i32(&mut block, from_mm(pad.hole_size.unwrap_or(0.0)));

    // Shapes (top @49, middle @50, bottom @51)
    block.push(tmb_shape_id(0));
    block.push(tmb_shape_id(1));
    block.push(tmb_shape_id(2));

    // Rotation - offsets 52-59 (8-byte double)
    write_f64(&mut block, pad.rotation);

    // Is plated - offset 60
    block.push(u8::from(pad.hole_size.is_some()));

    // Extended tail - offsets 61-201 (141 bytes)
    block.extend_from_slice(&build_pad_extended_tail(pad));

    debug_assert_eq!(block.len(), PAD_MAIN_BLOCK_LEN);
    block
}

/// Canonical 321-byte via `SubRecord-1` (offsets 0-320) captured from a standard
/// Altium via. [`encode_via`] clones it and overlays the typed fields we model;
/// the reserved/cache regions keep their template defaults so the via stays
/// Altium-readable (matches `PcbLibWriter.BuildViaExtended`). The two identity
/// GUIDs @259/@275 are overwritten per-via with fresh unique values (see
/// [`encode_via`]) — the template's GUID bytes are placeholders only.
const VIA_SR1_TEMPLATE: [u8; 321] = [
    0x4A, 0x0C, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xF0, 0x49, 0x02, 0x00, 0x02, 0x20, 0x00,
    0xA0, 0x86, 0x01, 0x00, 0x04, 0x00, 0xA0, 0x86, 0x01, 0x00, 0x40, 0x0D, 0x03, 0x00, 0x40, 0x0D,
    0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x9C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0,
    0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0,
    0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0,
    0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0,
    0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0,
    0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0,
    0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0,
    0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0,
    0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0xE0, 0x93, 0x04, 0x00, 0x0F, 0x00, 0x03, 0x01, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x40, 0x9C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2A, 0x00,
    0x00, 0x00, 0x00, 0x80, 0x63, 0xD4, 0xE4, 0x65, 0xC4, 0xF4, 0x4E, 0x8B, 0xAD, 0xA7, 0xCE, 0x97,
    0xDC, 0x40, 0xDA, 0xA5, 0xB1, 0xE3, 0xB2, 0x84, 0x25, 0x11, 0x43, 0x83, 0xDB, 0x2B, 0x6A, 0x87,
    0x7C, 0xB1, 0x74, 0xFF, 0xFF, 0xFF, 0x7F, 0xFF, 0xFF, 0xFF, 0x7F, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x1E, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01,
];

/// Encodes a Via primitive as Altium's single-block via record.
///
/// Altium writes a via as **one** block: the 13-byte common header (offsets
/// 0-12) followed by the 321-byte via `SubRecord-1` (offsets 13-320) — see
/// [`VIA_SR1_TEMPLATE`]. Our previous 6-block layout (copied from the pad
/// encoder) was misread by Altium; this matches `PcbLibWriter.WriteVia` (#113).
fn encode_via(data: &mut Vec<u8>, via: &Via) {
    let mut block = VIA_SR1_TEMPLATE;

    // Common header (offsets 0-12): MultiLayer + the via's flag word
    // (locked/keepout/tenting top+bottom).
    let mut header = Vec::with_capacity(13);
    write_common_header(&mut header, Layer::MultiLayer, via.flags);
    block[0..13].copy_from_slice(&header);

    // Connectivity indices @3-8 (net/polygon/component). Overlays the header's
    // 0xFF bytes; a default via keeps 0xFFFF/none so the template bytes are
    // reproduced unchanged (byte-identity).
    write_common_indices(
        &mut block,
        via.net_index,
        via.polygon_index,
        via.component_index,
    );

    // Geometry (offsets 13-30).
    block[13..17].copy_from_slice(&from_mm(via.x).to_le_bytes());
    block[17..21].copy_from_slice(&from_mm(via.y).to_le_bytes());
    block[21..25].copy_from_slice(&from_mm(via.diameter).to_le_bytes());
    block[25..29].copy_from_slice(&from_mm(via.hole_size).to_le_bytes());
    block[29] = layer_to_id(via.from_layer);
    block[30] = layer_to_id(via.to_layer);

    // Power-plane connection style @31 (0=Relief, 1=Direct, 2=NoConnect).
    block[31] = via.power_plane_connect_style.to_id();

    // Thermal relief (air gap @32, conductor count @36, conductor width @38).
    block[32..36].copy_from_slice(&from_mm(via.thermal_relief_gap).to_le_bytes());
    block[36] = via.thermal_relief_conductors;
    block[38..42].copy_from_slice(&from_mm(via.thermal_relief_width).to_le_bytes());

    // Power-plane relief expansion @42, plane clearance @46, paste-mask @50.
    block[42..46].copy_from_slice(&from_mm(via.power_plane_relief_expansion).to_le_bytes());
    block[46..50].copy_from_slice(&from_mm(via.power_plane_clearance).to_le_bytes());
    block[50..54].copy_from_slice(&from_mm(via.paste_mask_expansion).to_le_bytes());

    // Solder mask expansion @54, its mode @66, diameter stack mode @74.
    block[54..58].copy_from_slice(&from_mm(via.solder_mask_expansion).to_le_bytes());
    block[66] = via.solder_mask_expansion_mode.to_id();
    block[74] = via_stack_mode_to_id(via.diameter_stack_mode);

    // Bottom-face solder-mask expansion @242. `None` mirrors the front face, so a
    // default via reproduces the front bytes (preserving round-trip identity).
    let back = via
        .solder_mask_expansion_back
        .unwrap_or(via.solder_mask_expansion);
    block[242..246].copy_from_slice(&from_mm(back).to_le_bytes());

    // Two per-via identity GUIDs @259 (IdentityGuid / GUID-A) and @275
    // (IdentityGuidB / GUID-B). Altium expects each primitive to carry its OWN
    // identity; the template's fixed GUID bytes were reused for every via, so
    // multiple vias in one footprint collided on a single GUID. Mirror the pad
    // encoder (`build_pad_extended_tail`), which writes two independent fresh
    // GUIDs per primitive. The reader never reads these back, so they are a pure
    // write-side identity (distinct from the UniqueIDPrimitiveInformation stream).
    block[259..275].copy_from_slice(&generate_guid());
    block[275..291].copy_from_slice(&generate_guid());

    // Drill tolerances @291 / @295. `None` leaves the template's 0x7FFFFFFF
    // "unset" sentinel (byte-identical); `Some(mm)` writes the raw tolerance.
    if let Some(tol) = via.hole_positive_tolerance {
        block[291..295].copy_from_slice(&from_mm(tol).to_le_bytes());
    }
    if let Some(tol) = via.hole_negative_tolerance {
        block[295..299].copy_from_slice(&from_mm(tol).to_le_bytes());
    }

    // Per-layer diameters: 32 x i32 from offset 75. A real stack uses the
    // per-layer array; a simple via repeats its diameter on every layer so it
    // never reads back as zero-diameter per layer.
    for i in 0..32 {
        let d = if via.diameter_stack_mode == ViaStackMode::Simple {
            via.diameter
        } else {
            via.per_layer_diameters
                .as_ref()
                .and_then(|v| v.get(i).copied())
                .unwrap_or(via.diameter)
        };
        let off = 75 + i * 4;
        block[off..off + 4].copy_from_slice(&from_mm(d).to_le_bytes());
    }

    write_block(data, &block);
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

/// Writes a text font name into a fixed 64-byte UTF-16 field (`dst.len() == 64`).
///
/// The name is encoded UTF-16 little-endian, truncated to at most 62 bytes (31
/// UTF-16 code units) so the field always ends in at least one null pair, and the
/// remainder is zero-filled. Mirrors `AltiumSharp`'s modeled emit: for the default
/// "Arial" this reproduces the template's exact bytes (`41 00 72 00 69 00 61 00
/// 6C 00 00 00 …`), keeping a from-scratch text byte-identical.
fn encode_font_name_field(dst: &mut [u8], name: &str) {
    debug_assert_eq!(dst.len(), 64);
    dst.fill(0);
    let mut i = 0;
    for unit in name.encode_utf16() {
        if i + 2 > 62 {
            break; // leave the final 2 bytes as a null terminator
        }
        dst[i..i + 2].copy_from_slice(&unit.to_le_bytes());
        i += 2;
    }
}

/// Converts a [`TextJustification`] to the Altium PCB text-box justification byte
/// (geometry offset 132). Altium encodes this column-major (1-based):
/// `LeftTop=1, LeftCenter=2, LeftBottom=3, CenterTop=4, CenterCenter=5,
/// CenterBottom=6, RightTop=7, RightCenter=8, RightBottom=9`. The shared 3x3 grid
/// maps onto it cell-for-cell, so the field's from-scratch default (`BottomLeft`
/// = `LeftBottom`) yields `0x03`, matching the template.
const fn pcb_justification_to_id(j: TextJustification) -> u8 {
    match j {
        TextJustification::TopLeft => 1,
        TextJustification::MiddleLeft => 2,
        TextJustification::BottomLeft => 3,
        TextJustification::TopCenter => 4,
        TextJustification::MiddleCenter => 5,
        TextJustification::BottomCenter => 6,
        TextJustification::TopRight => 7,
        TextJustification::MiddleRight => 8,
        TextJustification::BottomRight => 9,
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

    // Common header (13 bytes) + connectivity indices @3-8 (net/polygon/component).
    write_common_header(&mut block, track.layer, track.flags);
    write_common_indices(
        &mut block,
        track.net_index,
        track.polygon_index,
        track.component_index,
    );

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
    write_i32(
        &mut block,
        from_mm(track.solder_mask_expansion.unwrap_or(0.0)),
    ); // 35-38 solder mask expansion
    block.extend_from_slice(&0i16.to_le_bytes()); // 39-40 paste mask expansion
    write_u32(&mut block, v7_layer_id(layer_to_id(track.layer))); // 41-44 v7 layer id
    block.push(track.keepout_restrictions.unwrap_or(0)); // 45 keepout restrictions
    block.extend_from_slice(&[0u8; 3]); // 46-48 reserved

    write_block(data, &block);
}

/// Encodes an Arc primitive.
fn encode_arc(data: &mut Vec<u8>, arc: &Arc) {
    let mut block = Vec::with_capacity(64);

    // Common header (13 bytes) + connectivity indices @3-8 (net/polygon/component).
    write_common_header(&mut block, arc.layer, arc.flags);
    write_common_indices(
        &mut block,
        arc.net_index,
        arc.polygon_index,
        arc.component_index,
    );

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
    write_i32(
        &mut block,
        from_mm(arc.solder_mask_expansion.unwrap_or(0.0)),
    ); // 47-50 solder mask expansion
    block.push(0); // 51 paste mask expansion
    write_u32(&mut block, v7_layer_id(layer_to_id(arc.layer))); // 52-55 v7 layer id
    block.push(arc.keepout_restrictions.unwrap_or(0)); // 56 keepout restrictions
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
pub fn encode_text_geometry(text: &Text) -> Vec<u8> {
    let mut block = TEXT_SR1_TEMPLATE;

    // Common header (offsets 0-12): layer + Altium flag word + 0xFF net/poly/comp.
    let mut header = Vec::with_capacity(13);
    write_common_header(&mut header, text.layer, text.flags);
    block[..13].copy_from_slice(&header);

    // Connectivity indices @3-8 (net/polygon/component). Overlays the header's
    // 0xFF bytes; defaults keep 0xFFFF/none so a from-scratch text stays
    // byte-identical to the template.
    write_common_indices(
        &mut block,
        text.net_index,
        text.polygon_index,
        text.component_index,
    );

    // Position and height (offsets 13-24).
    block[13..17].copy_from_slice(&from_mm(text.x).to_le_bytes());
    block[17..21].copy_from_slice(&from_mm(text.y).to_le_bytes());
    block[21..25].copy_from_slice(&from_mm(text.height).to_le_bytes());

    // Font-table index (offsets 25-26); the default stroke font is index 1.
    let font_id = text.stroke_font.map_or(1, stroke_font_to_id);
    block[25..27].copy_from_slice(&font_id.to_le_bytes());

    // Rotation (offsets 27-34, f64 degrees).
    block[27..35].copy_from_slice(&text.rotation.to_le_bytes());

    // Mirror flag (offset 35, IsMirrored). Default false reproduces the template's
    // 0x00; true marks a bottom-side (mirrored) silkscreen text.
    block[35] = u8::from(text.mirror);

    // Stroke line width (offset 36, i32). `None` keeps the template default (4 mil).
    if let Some(width) = text.stroke_width {
        block[36..40].copy_from_slice(&from_mm(width).to_le_bytes());
    }

    // Font bold (offset 44, FontBold — twin of italic@45). Default false
    // reproduces the template's 0x00.
    block[44] = u8::from(text.bold);

    // Authoritative text kind (offset 160).
    block[160] = text_kind_to_id(text.kind);

    // Base font type (offset 43) is derived from the text kind: Stroke -> 0,
    // TrueType -> 1. The template default is 0, so stroke text stays
    // byte-identical; the only change is the previously malformed TrueType record
    // (kind@160=1 with base@43=0). BarCode is a deferred kind and not modelled here.
    block[43] = u8::from(!matches!(text.kind, TextKind::Stroke));
    // Italic style (offset 45). Default false reproduces the template's 0x00.
    block[45] = u8::from(text.italic);

    // Font name (offsets 46-109, UTF-16, 64-byte field). The default "Arial"
    // reproduces the template's exact bytes: the UTF-16 name (max 62 bytes) then
    // zero fill — byte-identical to the "Arial\0…" template for a from-scratch text.
    encode_font_name_field(&mut block[46..110], &text.font_name);

    // Text-box justification (offset 132). The from-scratch default `MiddleCenter`
    // encodes to 0x00 (the template byte); other anchors map onto the Altium
    // column-major text-box encoding.
    block[132] = pcb_justification_to_id(text.justification);

    // v7 layer id (offsets 226-229), derived from the layer.
    block[226..230].copy_from_slice(&v7_layer_id(layer_to_id(text.layer)).to_le_bytes());

    block.to_vec()
}

/// Encodes a Region primitive (filled polygon).
///
/// Region format (matching Altium):
/// - A single block: common header, parameter string, and the vertex outline.
///
/// Altium's `WriteRegion` emits exactly one block. A spurious empty second block
/// leaves a stray `00 00 00 00` after the region; when another primitive follows,
/// Altium reads it as an invalid record type and silently drops every primitive
/// after the region (e.g. a trailing `ComponentBody` never renders).
fn encode_region(data: &mut Vec<u8>, region: &Region) {
    let props = encode_region_properties(region);
    write_block(data, &props);
}

/// Returns the canonical Altium `V7_LAYER` token for a layer.
///
/// Altium identifies a Region's layer by this parameter string, NOT the
/// common-header layer byte. Mechanical and component-pair layers must use their
/// `MECHANICAL{n}` token (e.g. Top Courtyard -> `MECHANICAL4`). The display name
/// with spaces stripped (`TOPCOURTYARD`) is not a valid token, so Altium fails to
/// resolve the layer and silently drops the region onto Top Layer (copper). The
/// 3D-body encoder already hardcodes `MECHANICAL6`/`MECHANICAL7` for this reason.
fn region_v7_layer_token(layer: Layer) -> String {
    match layer_to_id(layer) {
        id @ 57..=72 => format!("MECHANICAL{}", id - 56),
        id @ 186..=201 => format!("MECHANICAL{}", id - 169),
        _ => layer.as_str().replace(' ', "").to_uppercase(),
    }
}

/// Formats a length (mm) as an Altium mil-suffixed string with trailing zeros
/// trimmed (e.g. `0mil`, `0.5mil`, `19.685mil`). Mirrors `AltiumSharp`
/// `FormatMilCoord` (`ToMils().ToString("0.######") + "mil"`). A `0.0` input
/// yields exactly `0mil`, keeping the from-scratch region byte-identical.
fn format_mil_coord(mm: f64) -> String {
    let mils = mm_to_mil(mm);
    // Round to 6 decimals and strip trailing zeros / a lone trailing dot.
    let mut s = format!("{mils:.6}");
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    format!("{s}mil")
}

/// Encodes the properties block for a region.
///
/// Format (matching Altium):
/// ```text
/// [common_header:13]       // Layer, flags, net/poly/comp indices
/// [reserved:1]             // @13 reserved (0)
/// [hole_count:2 u16]       // @14-15 number of interior hole contours
/// [reserved:2]             // @16-17 reserved (0)
/// [param_len:4 u32]        // Parameter string length
/// [params:param_len]       // Parameter string (ASCII)
/// [vertex_count:4 u32]     // Number of outline vertices
/// [vertices:count*16]      // Outline vertices as doubles
/// [hole:...]               // hole_count x [u32 count][count*16] hole contours
/// ```
#[allow(clippy::cast_possible_truncation)] // Vertex/hole count and param length fit in u32/u16
fn encode_region_properties(region: &Region) -> Vec<u8> {
    let vertex_count = region.vertices.len();

    // Parameter string in Altium's canonical key order. Unlike other Altium param
    // blocks, a region's nested block has NO leading pipe and carries the full
    // canonical key set (matching AltiumSharp `BuildRegionParamText`). Each value is
    // now taken from the typed field; a default region reproduces the historical
    // hard-coded string byte-for-byte (KIND=0, NAME=, ARCRESOLUTION=0mil, ...).
    let layer_name = region_v7_layer_token(region.layer);
    let params = format!(
        "V7_LAYER={layer_name}|NAME={name}|KIND={kind}|SUBPOLYINDEX={spi}|UNIONINDEX={uix}\
         |ARCRESOLUTION={arc}|ISSHAPEBASED={shape}|CAVITYHEIGHT={cav}",
        name = region.name,
        kind = region.kind.to_id(),
        spi = region.sub_poly_index,
        uix = region.union_index,
        arc = format_mil_coord(region.arc_resolution),
        shape = if region.is_shape_based {
            "TRUE"
        } else {
            "FALSE"
        },
        cav = format_mil_coord(region.cavity_height),
    );
    // Re-emit any unmodelled keys captured on read (board-region keys like LAYER /
    // KEEPOUT / ISBOARDCUTOUT, etc.), verbatim and in read order, so a
    // read-modify-write does not drop them. Empty for a from-scratch region, so
    // nothing is appended and the output stays byte-identical to the canonical form.
    let params = append_additional_params(params, &region.additional_parameters);
    let params_bytes = crate::altium::encode_windows1252(&params);

    let mut block = Vec::with_capacity(22 + params_bytes.len() + 4 + vertex_count * 16);

    // Common header (13 bytes): layer + flag word, then the net/polygon/component
    // indices. `write_common_header` fills bytes 3-12 with 0xFF (a free primitive);
    // `write_common_indices` overlays the modelled indices. Defaults (net=0xFFFF,
    // poly=0xFFFF, component=-1 -> 0xFFFF) leave the 0xFF bytes untouched, so a
    // from-scratch region stays byte-identical.
    write_common_header(&mut block, region.layer, region.flags);
    write_common_indices(
        &mut block,
        region.net_index,
        region.polygon_index,
        region.component_index,
    );

    // @13 reserved | @14-15 hole_count (u16 LE) | @16-17 reserved. With no holes
    // this collapses to `00 00 00 00 00`, byte-identical to the previous output.
    block.push(0x00);
    write_u16(&mut block, region.holes.len() as u16);
    block.extend_from_slice(&[0x00; 2]);

    // C-string parameter block (length includes the null terminator).
    write_cstring_param_block(&mut block, &params_bytes);

    // Outline vertex count
    write_u32(&mut block, vertex_count as u32);

    // Outline vertices as doubles in internal units
    for vertex in &region.vertices {
        let x_internal = f64::from(from_mm(vertex.x));
        let y_internal = f64::from(from_mm(vertex.y));
        write_f64(&mut block, x_internal);
        write_f64(&mut block, y_internal);
    }

    // Trailing hole contours, each count-prefixed exactly like the outline. With
    // no holes nothing is appended, so the output is unchanged.
    for hole in &region.holes {
        write_u32(&mut block, hole.len() as u32);
        for vertex in hole {
            write_f64(&mut block, f64::from(from_mm(vertex.x)));
            write_f64(&mut block, f64::from(from_mm(vertex.y)));
        }
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

    // Common header (13 bytes) + connectivity indices @3-8 (net/polygon/component).
    write_common_header(&mut block, fill.layer, fill.flags);
    write_common_indices(
        &mut block,
        fill.net_index,
        fill.polygon_index,
        fill.component_index,
    );

    // Corner coordinates (16 bytes)
    write_i32(&mut block, from_mm(fill.x1));
    write_i32(&mut block, from_mm(fill.y1));
    write_i32(&mut block, from_mm(fill.x2));
    write_i32(&mut block, from_mm(fill.y2));

    // Rotation (8 bytes)
    write_f64(&mut block, fill.rotation);

    // Tail (13 bytes, offsets 37-49), ported from AltiumSharp `WriteFill`:
    // solder-mask expansion i32 @37-40, paste-mask byte @41 (0), v7 layer id @42-45,
    // keepout byte @46, reserved @47-49. Both modelled fields default to 0, so a
    // from-scratch fill emits the same bytes as before (byte-identical).
    let mut tail = [0x00u8; 13];
    tail[0..4].copy_from_slice(&from_mm(fill.solder_mask_expansion.unwrap_or(0.0)).to_le_bytes());
    tail[5..9].copy_from_slice(&v7_layer_id(layer_to_id(fill.layer)).to_le_bytes());
    tail[9] = fill.keepout_restrictions.unwrap_or(0);
    block.extend_from_slice(&tail);

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

    // 0xFF padding (10 bytes) @3-12: net/polygon/component indices + reserved.
    block.extend_from_slice(&[0xFF; 10]);

    // Connectivity indices @3-8 (net/polygon/component). Overlays the 0xFF
    // padding; defaults keep 0xFFFF/none so a from-scratch body's header bytes
    // are reproduced unchanged (byte-identity).
    write_common_indices(
        &mut block,
        body.net_index,
        body.polygon_index,
        body.component_index,
    );

    // Zeros (5 bytes)
    block.extend_from_slice(&[0x00; 5]);

    // Parameter string as a C-string block (length includes the null).
    let param_str = build_component_body_params(body);
    write_cstring_param_block(&mut block, &crate::altium::encode_windows1252(&param_str));

    // Outline polygon: vertex count then (f64 x, f64 y) per vertex, in Altium
    // internal units. Coordinates MUST be whole internal units (like every other
    // primitive — via from_mm): Altium silently drops a body whose outline has
    // fractional internal coordinates. Real Altium-authored bodies are always
    // integer-valued here. (Writing mm*scale directly produced fractional values
    // for non-mil-aligned dimensions and the body never rendered.)
    write_u32(&mut block, outline.len() as u32);
    for &(x, y) in outline {
        write_f64(&mut block, f64::from(from_mm(x)));
        write_f64(&mut block, f64::from(from_mm(y)));
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
    // A body with no STEP model (no filename, not embedded) is a generic
    // *extruded* body: Altium defines it by its 2D outline polygon plus a Z
    // extent (MODEL.EXTRUDED.MINZ/MAXZ) and MODELTYPE=0, with no model file.
    // Matched against a real Altium-authored extruded body: ISSHAPEBASED stays
    // FALSE (same as STEP bodies); the extrusion comes from the EXTRUDED.MIN/MAXZ
    // pair, NOT from ISSHAPEBASED. Model-backed bodies use MODELTYPE=1 and a
    // MODELSOURCE instead.
    let extruded = body.model_name.is_empty() && !body.embedded;

    let mut params = Vec::new();

    // V7_LAYER must match the body's actual layer byte. Use the canonical
    // MECHANICAL{n} token for any mechanical layer (Top3DBody=MECHANICAL6,
    // Mechanical1=MECHANICAL1, etc.) instead of hardcoding one — a mismatch
    // between the param string and the layer byte makes Altium drop the body.
    params.push(format!("V7_LAYER={}", region_v7_layer_token(body.layer)));

    // Standard parameters. Each field's default reproduces the prior hard-coded
    // literal exactly, so a template-default body stays byte-identical (the oracle
    // depends on this).
    params.push(format!("NAME={}", body.name));
    params.push(format!("KIND={}", body.kind));
    params.push(format!("SUBPOLYINDEX={}", body.sub_poly_index));
    params.push(format!("UNIONINDEX={}", body.union_index));
    params.push("ARCRESOLUTION=0.5mil".to_string());
    params.push(format!(
        "ISSHAPEBASED={}",
        if body.is_shape_based { "TRUE" } else { "FALSE" }
    ));
    params.push("CAVITYHEIGHT=0mil".to_string());
    params.push(format!(
        "STANDOFFHEIGHT={}mil",
        mm_to_mil(body.standoff_height)
    ));
    params.push(format!(
        "OVERALLHEIGHT={}mil",
        mm_to_mil(body.overall_height)
    ));
    params.push(format!("BODYPROJECTION={}", body.body_projection));
    // Altium repeats ARCRESOLUTION after BODYPROJECTION (verbatim shape from the
    // BODY_3D golden files).
    params.push("ARCRESOLUTION=0.5mil".to_string());
    params.push(format!("BODYCOLOR3D={}", body.body_color_3d));
    params.push(format!("BODYOPACITY3D={:.3}", body.body_opacity_3d));
    // IDENTIFIER is deferred: AltiumSharp stores it as a comma-separated codepoint
    // list, so a plain-string round-trip would write a file Altium misreads. Keep
    // the empty literal hard-coded (empty -> empty is oracle-safe).
    params.push("IDENTIFIER=".to_string());
    params.push("TEXTURE=".to_string());
    params.push("TEXTURECENTERX=0mil".to_string());
    params.push("TEXTURECENTERY=0mil".to_string());
    params.push("TEXTURESIZEX=0.0001mil".to_string());
    params.push("TEXTURESIZEY=0.0001mil".to_string());
    params.push("TEXTUREROTATION= 0.00000000000000E+0000".to_string());

    // Model reference. Extruded bodies have no model file but still need a model
    // GUID, so synthesize one when the caller didn't supply it.
    let model_id = if extruded && body.model_id.is_empty() {
        format!("{{{}}}", uuid::Uuid::new_v4().to_string().to_uppercase())
    } else {
        body.model_id.clone()
    };
    params.push(format!("MODELID={model_id}"));
    // Round-trip the stored checksum verbatim (default 0 keeps fresh output identical).
    params.push(format!("MODEL.CHECKSUM={}", body.model_checksum));
    params.push(format!(
        "MODEL.EMBED={}",
        if body.embedded { "TRUE" } else { "FALSE" }
    ));
    params.push(format!("MODEL.NAME={}", body.model_name));
    params.push("MODEL.2D.X=0mil".to_string());
    params.push("MODEL.2D.Y=0mil".to_string());
    params.push(format!("MODEL.2D.ROTATION={:.3}", body.model_2d_rotation));
    params.push(format!("MODEL.3D.ROTX={:.3}", body.rotation_x));
    params.push(format!("MODEL.3D.ROTY={:.3}", body.rotation_y));
    params.push(format!("MODEL.3D.ROTZ={:.3}", body.rotation_z));
    params.push(format!("MODEL.3D.DZ={}mil", mm_to_mil(body.z_offset)));
    // MODELTYPE 0 = Extruded (no model file); 1 = generic/STEP model.
    let model_type = if extruded { "0" } else { "1" };
    params.push(format!("MODEL.MODELTYPE={model_type}"));
    if extruded {
        // The extrusion itself: Z range from standoff (MINZ) to overall (MAXZ).
        // This is what Altium actually extrudes the outline between; without it
        // the body has no volume and is discarded on load.
        params.push(format!(
            "MODEL.EXTRUDED.MINZ={}mil",
            mm_to_mil(body.standoff_height)
        ));
        params.push(format!(
            "MODEL.EXTRUDED.MAXZ={}mil",
            mm_to_mil(body.overall_height)
        ));
    } else {
        params.push("MODEL.MODELSOURCE=Undefined".to_string());
    }

    // Re-emit any unmodelled keys captured on read, verbatim and in read order, so
    // a read-modify-write does not drop them. Empty for a from-scratch body, so
    // nothing is appended and the output stays byte-identical.
    //
    // Skip any captured key we already emitted above: the writer unconditionally
    // emits several canonical keys (ARCRESOLUTION, CAVITYHEIGHT, IDENTIFIER,
    // TEXTURE*, MODEL.2D.X/Y, MODEL.MODELTYPE, the extrusion range, ...) that are
    // NOT in BODY_MODELLED_PARAM_KEYS, so the reader captures them too. Appending
    // them again produced a DUPLICATE token (e.g. two CAVITYHEIGHT=) on every
    // read-modify-write. Our canonical emission wins; the captured copy is dropped.
    let emitted: std::collections::HashSet<String> = params
        .iter()
        .filter_map(|p| p.split_once('=').map(|(k, _)| k.to_string()))
        .collect();
    for (key, value) in &body.additional_parameters {
        if emitted.contains(key) {
            continue;
        }
        params.push(format!("{key}={value}"));
    }

    params.join("|")
}

/// Appends `additional` `KEY=VALUE` pairs to an already-built `|`-joined parameter
/// string, verbatim and in order. Returns `params` unchanged when `additional` is
/// empty (the from-scratch case), so the output stays byte-identical.
fn append_additional_params(mut params: String, additional: &[(String, String)]) -> String {
    for (key, value) in additional {
        params.push('|');
        params.push_str(key);
        params.push('=');
        params.push_str(value);
    }
    params
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

    // `PRIMITIVEINDEX` is a single global 0-based ordinal over ALL primitives in
    // `Data`-stream emit order (see `encode_data_stream`): Arc, Pad, Via, Track,
    // Text, Region, Fill, ComponentBody. Every primitive consumes an ordinal whether
    // or not it carries a unique id (a record is emitted only when it does) — so e.g.
    // the first pad behind two silkscreen arcs is `PRIMITIVEINDEX=2`, matching Altium.
    // `apply_unique_ids` MUST walk this exact order to round-trip.
    let mut ordinal: usize = 0;
    macro_rules! emit {
        ($iter:expr, $ty:literal) => {
            for prim in $iter {
                if let Some(ref uid) = prim.unique_id {
                    encode_unique_id_record(&mut data, ordinal, $ty, uid);
                    has_any_id = true;
                }
                ordinal += 1;
            }
        };
    }
    emit!(&footprint.arcs, "Arc");
    emit!(&footprint.pads, "Pad");
    emit!(&footprint.vias, "Via");
    emit!(&footprint.tracks, "Track");
    emit!(&footprint.text, "Text");
    emit!(&footprint.regions, "Region");
    emit!(&footprint.fills, "Fill");
    emit!(&footprint.component_bodies, "ComponentBody");

    has_any_id.then_some(data)
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
/// Format: `[block_len:4]["|ENCODEDTEXT0=...|ENCODEDTEXT1=..." + \x00]` — a leading
/// pipe per entry and NO trailing pipe, matching `AltiumSharp`'s `ParametersToString`.
///
/// Empty (no wide-text entries): `[block_len:4][\x00]` (`block_len` = 1).
pub fn encode_component_wide_strings(footprint: &Footprint) -> Vec<u8> {
    use std::fmt::Write;

    // Collect text content from this footprint
    let mut texts: Vec<&str> = Vec::new();

    for text in &footprint.text {
        if !text.text.starts_with('.') && !text.text.is_empty() {
            texts.push(&text.text);
        }
    }

    // Build the parameter string: `|ENCODEDTEXT0=...|ENCODEDTEXT1=...` — a leading
    // pipe per entry and NO trailing pipe (matching AltiumSharp). With no entries the
    // string is empty, so the stream is just `[01 00 00 00][00]` — AltiumSharp's empty
    // form — rather than the spurious `[02 00 00 00][7C 00]` (leading-pipe) we emitted.
    let mut content = String::new();
    for (index, text) in texts.iter().enumerate() {
        let encoded: String = text
            .bytes()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let _ = write!(content, "|ENCODEDTEXT{index}={encoded}");
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
    fn size_shape_block_carries_fullstack_tail() {
        // A non-Simple pad's size/shape block must be 651 bytes (596 body + the
        // 40-byte tail), never a bare 596 — Altium rejects the under-length form.
        let mut pad = Pad::smd("1", 0.0, 0.0, 1.0, 0.6);
        pad.corner_radius_percent = Some(25); // routes to encode_pad_size_shape_block
        let b = encode_pad_size_shape_block(&pad);
        assert_eq!(
            b.len(),
            651,
            "block must carry the single-entry full-stack tail"
        );
        assert_eq!(&b[628..632], &1i32.to_le_bytes(), "tail entry count = 1");
        assert_eq!(&b[632..636], &15i32.to_le_bytes(), "tail entry stride = 15");
        assert_eq!(b[649], 50, "tail entry corner is a fixed 50");
    }

    #[test]
    fn common_indices_default_to_ff_bytes() {
        // Byte-identity guard (oracle): a from-scratch primitive's connectivity
        // indices default to "none" (net=0xFFFF, polygon=0xFFFF, component=-1 ->
        // 0xFFFF), which must reproduce the header fill's `0xFF FF` bytes @3-8
        // exactly. Any drift here re-introduces a byte diff the oracle would flag.
        let mut block = vec![0u8; 13];
        write_common_header(&mut block, Layer::TopLayer, PcbFlags::empty());
        // A from-scratch track/arc/etc. uses these defaults.
        write_common_indices(&mut block, 0xFFFF, 0xFFFF, -1);
        assert_eq!(
            &block[3..9],
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            "default indices must reproduce the 0xFF header fill @3-8"
        );
    }

    #[test]
    fn common_indices_overlay_modelled_values() {
        // A set net/polygon/component overlays the header fill at the right offsets
        // in LE, with component `-1` mapping to the 0xFFFF sentinel.
        let mut block = vec![0u8; 13];
        write_common_header(&mut block, Layer::TopLayer, PcbFlags::empty());
        write_common_indices(&mut block, 0x1234, 0x5678, 42);
        assert_eq!(&block[3..5], &0x1234u16.to_le_bytes(), "net @3-4");
        assert_eq!(&block[5..7], &0x5678u16.to_le_bytes(), "polygon @5-6");
        assert_eq!(&block[7..9], &42u16.to_le_bytes(), "component @7-8");
    }

    #[test]
    fn track_from_scratch_header_bytes_byte_identical() {
        // A default Track encodes the same 0xFF header bytes @3-8 as before the
        // indices were modelled (byte-identity for the oracle).
        let track = Track::new(0.0, 0.0, 1.0, 0.0, 0.2, Layer::TopOverlay);
        let mut data = Vec::new();
        encode_track(&mut data, &track);
        // Skip the 4-byte block length prefix; header is @0 of the block body.
        let block = &data[4..];
        assert_eq!(
            &block[3..9],
            &[0xFF; 6],
            "from-scratch track must keep the 0xFF net/polygon/component bytes"
        );
    }

    #[test]
    fn text_from_scratch_header_bytes_byte_identical() {
        use crate::altium::TextJustification;
        // A default Text's geometry block keeps the template's 0xFF header bytes @3-8.
        let text = Text {
            x: 0.0,
            y: 0.0,
            text: "X".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::BottomLeft,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
        };
        let geom = encode_text_geometry(&text);
        assert_eq!(
            &geom[3..9],
            &[0xFF; 6],
            "from-scratch text must keep the 0xFF net/polygon/component bytes"
        );
    }

    #[test]
    fn track_indices_encode_into_header() {
        // A track carrying a net/component association writes those indices into
        // the common header @3-8 (round-trip fidelity for a board-context primitive).
        let mut track = Track::new(0.0, 0.0, 1.0, 0.0, 0.2, Layer::TopLayer);
        track.net_index = 7;
        track.component_index = 3;
        let mut data = Vec::new();
        encode_track(&mut data, &track);
        let block = &data[4..];
        assert_eq!(&block[3..5], &7u16.to_le_bytes(), "net @3-4");
        assert_eq!(&block[5..7], &0xFFFFu16.to_le_bytes(), "polygon stays none");
        assert_eq!(&block[7..9], &3u16.to_le_bytes(), "component @7-8");
    }

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
    fn test_region_v7_layer_token() {
        use crate::altium::pcblib::primitives::Vertex;
        // Component-pair / mechanical layers must use the MECHANICAL{n} token,
        // not the display name (which Altium can't resolve -> falls back to Top Layer).
        assert_eq!(region_v7_layer_token(Layer::TopCourtyard), "MECHANICAL4");
        assert_eq!(region_v7_layer_token(Layer::TopAssembly), "MECHANICAL2");
        assert_eq!(region_v7_layer_token(Layer::Mechanical1), "MECHANICAL1");
        assert_eq!(region_v7_layer_token(Layer::Mechanical17), "MECHANICAL17");
        assert_eq!(region_v7_layer_token(Layer::Mechanical32), "MECHANICAL32");
        // Non-mechanical layers keep the stripped/uppercased display token.
        assert_eq!(region_v7_layer_token(Layer::TopLayer), "TOPLAYER");
        assert_eq!(region_v7_layer_token(Layer::TopOverlay), "TOPOVERLAY");

        // A region on Top Courtyard must serialize V7_LAYER=MECHANICAL4.
        let region = Region {
            vertices: vec![
                Vertex { x: -1.0, y: -1.0 },
                Vertex { x: 1.0, y: -1.0 },
                Vertex { x: 1.0, y: 1.0 },
                Vertex { x: -1.0, y: 1.0 },
            ],
            layer: Layer::TopCourtyard,
            ..Region::default()
        };
        let props = encode_region_properties(&region);
        let s = String::from_utf8_lossy(&props);
        assert!(s.contains("V7_LAYER=MECHANICAL4"), "got: {s}");
        assert!(!s.contains("TOPCOURTYARD"));
    }

    #[test]
    fn encode_region_emits_exactly_one_block() {
        use crate::altium::pcblib::primitives::Vertex;
        // A Region must serialize as a single length-prefixed block. A trailing
        // empty block (`00 00 00 00`) makes Altium treat the next primitive's
        // record-type byte region as an invalid record and silently drop every
        // primitive after the region (e.g. a following ComponentBody never renders).
        let region = Region {
            vertices: vec![
                Vertex { x: -1.0, y: -1.0 },
                Vertex { x: 1.0, y: -1.0 },
                Vertex { x: 1.0, y: 1.0 },
                Vertex { x: -1.0, y: 1.0 },
            ],
            layer: Layer::TopCourtyard,
            ..Region::default()
        };
        let mut data = Vec::new();
        encode_region(&mut data, &region);
        let block_len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        assert_eq!(
            data.len(),
            4 + block_len,
            "region must be a single block (4-byte length prefix + payload); \
             trailing bytes indicate a spurious empty block"
        );
    }

    #[test]
    fn encode_region_no_holes_keeps_reserved_bytes() {
        use crate::altium::pcblib::primitives::Vertex;
        // Oracle-safety: a region with no holes must emit hole_count=0 and no trailing
        // arrays, leaving the @13-17 reserved slot as `00 00 00 00 00` (byte-identical
        // to the pre-holes output). The header is 13 bytes, so the slot is props[13..18].
        let region = Region {
            vertices: vec![
                Vertex { x: -1.0, y: -1.0 },
                Vertex { x: 1.0, y: -1.0 },
                Vertex { x: 1.0, y: 1.0 },
                Vertex { x: -1.0, y: 1.0 },
            ],
            layer: Layer::TopCourtyard,
            ..Region::default()
        };
        let props = encode_region_properties(&region);
        assert_eq!(
            &props[13..18],
            &[0x00, 0x00, 0x00, 0x00, 0x00],
            "no-hole region must keep the reserved @13-17 slot zeroed (hole_count=0)"
        );
    }

    #[test]
    fn default_region_param_string_is_byte_identical() {
        use crate::altium::pcblib::primitives::Vertex;
        // Oracle-safety: a from-scratch region must serialize the exact historical
        // canonical parameter string, and the common-header net/polygon/component
        // index bytes (@3-8) must all be 0xFF (a free primitive, no net).
        let region = Region {
            vertices: vec![
                Vertex { x: -1.0, y: -1.0 },
                Vertex { x: 1.0, y: -1.0 },
                Vertex { x: 1.0, y: 1.0 },
                Vertex { x: -1.0, y: 1.0 },
            ],
            layer: Layer::TopCourtyard,
            ..Region::default()
        };
        let props = encode_region_properties(&region);

        // Header bytes 3-8 (net + polygon + component indices) are all 0xFF.
        assert_eq!(
            &props[3..9],
            &[0xFF; 6],
            "default region header net/polygon/component indices must be 0xFF"
        );

        // The nested parameter string must match the historical hard-coded string
        // exactly (only V7_LAYER varies with the layer).
        let param_len = u32::from_le_bytes(props[18..22].try_into().unwrap()) as usize;
        let params = &props[22..22 + param_len];
        let expected = b"V7_LAYER=MECHANICAL4|NAME=|KIND=0|SUBPOLYINDEX=-1|UNIONINDEX=0\
                         |ARCRESOLUTION=0mil|ISSHAPEBASED=FALSE|CAVITYHEIGHT=0mil\0";
        assert_eq!(
            params,
            expected.as_slice(),
            "default region param string drifted from the byte-identical canonical form: {}",
            String::from_utf8_lossy(params)
        );
    }

    #[test]
    fn non_default_region_survives_roundtrip() {
        use super::super::reader::parse_data_stream;
        use crate::altium::pcblib::primitives::{RegionKind, Vertex};
        // A region with non-default kind/name/cavity/arc values must survive an
        // encode -> decode round-trip.
        let mut fp = Footprint::new("R");
        fp.add_region(Region {
            vertices: vec![
                Vertex { x: -1.0, y: -1.0 },
                Vertex { x: 1.0, y: -1.0 },
                Vertex { x: 1.0, y: 1.0 },
                Vertex { x: -1.0, y: 1.0 },
            ],
            layer: Layer::TopCourtyard,
            kind: RegionKind::Cutout,
            name: "POUR1".to_string(),
            cavity_height: 0.0254 * 10.0, // 10 mil in mm
            arc_resolution: 0.0254 * 0.5, // 0.5 mil in mm
            union_index: 3,
            sub_poly_index: 2,
            is_shape_based: true,
            ..Region::default()
        });

        let data = encode_data_stream(&fp).expect("encode should succeed");
        let mut decoded = Footprint::new("R");
        parse_data_stream(&mut decoded, &data, None);
        assert_eq!(decoded.regions.len(), 1);
        let r = &decoded.regions[0];
        assert_eq!(r.kind, RegionKind::Cutout);
        assert_eq!(r.name, "POUR1");
        assert!(
            (r.cavity_height - 0.254).abs() < 1e-6,
            "cav: {}",
            r.cavity_height
        );
        assert!(
            (r.arc_resolution - 0.0127).abs() < 1e-6,
            "arc: {}",
            r.arc_resolution
        );
        assert_eq!(r.union_index, 3);
        assert_eq!(r.sub_poly_index, 2);
        assert!(r.is_shape_based);
    }

    #[test]
    fn region_additional_params_are_reemitted() {
        use crate::altium::pcblib::primitives::Vertex;
        // A region carrying unmodelled board-region keys must re-emit them verbatim
        // after the canonical key set (round-trip fidelity — the reader captures them
        // into `additional_parameters`).
        let region = Region {
            vertices: vec![
                Vertex { x: -1.0, y: -1.0 },
                Vertex { x: 1.0, y: -1.0 },
                Vertex { x: 1.0, y: 1.0 },
                Vertex { x: -1.0, y: 1.0 },
            ],
            layer: Layer::TopCourtyard,
            additional_parameters: vec![
                ("LAYER".to_string(), "TOP".to_string()),
                ("KEEPOUT".to_string(), "TRUE".to_string()),
                ("ISBOARDCUTOUT".to_string(), "FALSE".to_string()),
            ],
            ..Region::default()
        };
        let props = encode_region_properties(&region);
        let param_len = u32::from_le_bytes(props[18..22].try_into().unwrap()) as usize;
        let params = String::from_utf8_lossy(&props[22..22 + param_len]);
        let params = params.trim_end_matches('\0');
        // Canonical keys still present, and the extra keys appended (in order) after them.
        assert!(params.contains("CAVITYHEIGHT=0mil"), "got: {params}");
        assert!(
            params.ends_with("|LAYER=TOP|KEEPOUT=TRUE|ISBOARDCUTOUT=FALSE"),
            "extra keys must be appended verbatim after the canonical set: {params}"
        );
    }

    #[test]
    fn region_empty_additional_params_is_byte_identical() {
        use crate::altium::pcblib::primitives::Vertex;
        // The load-bearing property: a from-scratch region (empty additional_parameters)
        // emits the EXACT canonical param string — the writer appends nothing.
        let region = Region {
            vertices: vec![
                Vertex { x: -1.0, y: -1.0 },
                Vertex { x: 1.0, y: -1.0 },
                Vertex { x: 1.0, y: 1.0 },
                Vertex { x: -1.0, y: 1.0 },
            ],
            layer: Layer::TopCourtyard,
            ..Region::default()
        };
        assert!(region.additional_parameters.is_empty());
        let props = encode_region_properties(&region);
        let param_len = u32::from_le_bytes(props[18..22].try_into().unwrap()) as usize;
        let params = &props[22..22 + param_len];
        let expected = b"V7_LAYER=MECHANICAL4|NAME=|KIND=0|SUBPOLYINDEX=-1|UNIONINDEX=0\
                         |ARCRESOLUTION=0mil|ISSHAPEBASED=FALSE|CAVITYHEIGHT=0mil\0";
        assert_eq!(
            params,
            expected.as_slice(),
            "empty additional_parameters must not change the canonical param string: {}",
            String::from_utf8_lossy(params)
        );
    }

    #[test]
    fn region_additional_params_survive_roundtrip() {
        use super::super::reader::parse_data_stream;
        use crate::altium::pcblib::primitives::Vertex;
        // An unmodelled key captured on read must survive encode -> decode.
        let mut fp = Footprint::new("R");
        fp.add_region(Region {
            vertices: vec![
                Vertex { x: -1.0, y: -1.0 },
                Vertex { x: 1.0, y: -1.0 },
                Vertex { x: 1.0, y: 1.0 },
                Vertex { x: -1.0, y: 1.0 },
            ],
            layer: Layer::TopCourtyard,
            additional_parameters: vec![
                ("LAYER".to_string(), "TOP".to_string()),
                ("LAYERSTACKID".to_string(), "7".to_string()),
            ],
            ..Region::default()
        });
        let data = encode_data_stream(&fp).expect("encode should succeed");
        let mut decoded = Footprint::new("R");
        parse_data_stream(&mut decoded, &data, None);
        assert_eq!(decoded.regions.len(), 1);
        assert_eq!(
            decoded.regions[0].additional_parameters,
            vec![
                ("LAYER".to_string(), "TOP".to_string()),
                ("LAYERSTACKID".to_string(), "7".to_string()),
            ],
        );
    }

    #[test]
    fn body_additional_params_are_reemitted_and_roundtrip() {
        use super::super::reader;
        // A body carrying a genuinely UNMODELLED key (one the writer does not emit
        // itself) must re-emit it and survive encode -> decode into
        // additional_parameters. TEXTURE / MODEL.2D.X are NOT valid here: the writer
        // emits them canonically, so a captured copy is (correctly) deduped away.
        let mut model = ComponentBody::new("{G}", "part.step");
        model.embedded = true;
        model.additional_parameters = vec![("WELDINGSPOT".to_string(), "42".to_string())];
        let s = build_component_body_params(&model);
        assert!(s.ends_with("|WELDINGSPOT=42"), "got: {s}");

        let mut fp = Footprint::new("B");
        fp.add_component_body(model);
        let data = reader_encode_decode(&fp);
        let mut decoded = Footprint::new("B");
        reader::parse_data_stream(&mut decoded, &data, None);
        let extra = &decoded.component_bodies[0].additional_parameters;
        assert!(
            extra.contains(&("WELDINGSPOT".to_string(), "42".to_string())),
            "an unmodelled key must round-trip, got: {extra:?}"
        );
    }

    #[test]
    fn body_canonical_key_captured_on_read_is_not_duplicated_on_write() {
        // Regression (bug sweep 2026-07): the writer emits ARCRESOLUTION,
        // CAVITYHEIGHT, IDENTIFIER, TEXTURE*, MODEL.2D.X/Y, MODEL.MODELTYPE and the
        // extrusion range unconditionally, yet none are in BODY_MODELLED_PARAM_KEYS,
        // so the reader ALSO captures them into additional_parameters. Appending them
        // again produced a duplicate token on every read-modify-write. The writer now
        // dedupes: its canonical emission wins and the captured copy is dropped.
        let mut model = ComponentBody::new("", "");
        // Simulate what the reader captures from a real Altium body.
        model.additional_parameters = vec![
            ("CAVITYHEIGHT".to_string(), "0mil".to_string()),
            ("TEXTURE".to_string(), String::new()),
            ("MODEL.2D.X".to_string(), "0mil".to_string()),
        ];
        let s = build_component_body_params(&model);
        for key in ["CAVITYHEIGHT", "TEXTURE", "MODEL.2D.X"] {
            let hits = s
                .split('|')
                .filter(|t| t.starts_with(&format!("{key}=")))
                .count();
            assert_eq!(hits, 1, "canonical key {key} must appear exactly once: {s}");
        }
    }

    #[test]
    fn body_empty_additional_params_is_byte_identical() {
        // A from-scratch body (empty additional_parameters) must emit the exact
        // canonical param string — the writer appends nothing.
        let mut model = ComponentBody::new("{G}", "part.step");
        model.embedded = true;
        assert!(model.additional_parameters.is_empty());
        let s = build_component_body_params(&model);
        let expected = "V7_LAYER=MECHANICAL6|NAME= |KIND=0|SUBPOLYINDEX=-1|UNIONINDEX=0|\
            ARCRESOLUTION=0.5mil|ISSHAPEBASED=FALSE|CAVITYHEIGHT=0mil|STANDOFFHEIGHT=0mil|\
            OVERALLHEIGHT=0mil|BODYPROJECTION=0|ARCRESOLUTION=0.5mil|BODYCOLOR3D=8421504|\
            BODYOPACITY3D=1.000|IDENTIFIER=|TEXTURE=|TEXTURECENTERX=0mil|TEXTURECENTERY=0mil|\
            TEXTURESIZEX=0.0001mil|TEXTURESIZEY=0.0001mil|TEXTUREROTATION= 0.00000000000000E+0000|\
            MODELID={G}|MODEL.CHECKSUM=0|MODEL.EMBED=TRUE|MODEL.NAME=part.step|MODEL.2D.X=0mil|\
            MODEL.2D.Y=0mil|MODEL.2D.ROTATION=0.000|MODEL.3D.ROTX=0.000|MODEL.3D.ROTY=0.000|\
            MODEL.3D.ROTZ=0.000|MODEL.3D.DZ=0mil|MODEL.MODELTYPE=1|MODEL.MODELSOURCE=Undefined";
        assert_eq!(s, expected);
    }

    /// Encodes then returns the data stream for a footprint (test helper).
    fn reader_encode_decode(fp: &Footprint) -> Vec<u8> {
        encode_data_stream(fp).expect("encode should succeed")
    }

    #[test]
    fn test_component_body_extruded_vs_model_params() {
        // Generic extruded body: no model name, not embedded. Matches a real
        // Altium-authored extruded body: ISSHAPEBASED=FALSE, MODELTYPE=0, a
        // synthesized MODELID GUID, and the extrusion Z range via EXTRUDED.MIN/MAXZ.
        let mut extruded = ComponentBody::new("", "");
        extruded.embedded = false;
        extruded.overall_height = 1.0;
        extruded.standoff_height = 0.0;
        let s = build_component_body_params(&extruded);
        assert!(s.contains("ISSHAPEBASED=FALSE"), "got: {s}");
        assert!(s.contains("MODEL.MODELTYPE=0"), "got: {s}");
        assert!(s.contains("MODEL.EXTRUDED.MAXZ="), "got: {s}");
        assert!(
            s.contains("MODELID={") && !s.contains("MODELID=|"),
            "got: {s}"
        );
        assert!(!s.contains("MODELSOURCE"), "got: {s}");

        // Model-backed body (STEP) keeps the legacy shape/type, no extrusion range.
        let mut model = ComponentBody::new("{GUID}", "part.step");
        model.embedded = true;
        let s = build_component_body_params(&model);
        assert!(s.contains("ISSHAPEBASED=FALSE"), "got: {s}");
        assert!(s.contains("MODEL.MODELTYPE=1"), "got: {s}");
        assert!(s.contains("MODEL.MODELSOURCE=Undefined"), "got: {s}");
        assert!(!s.contains("EXTRUDED"), "got: {s}");
    }

    #[test]
    fn component_body_default_params_byte_identical() {
        // Locks the template-default param string. If a new field's default or the
        // key order drifts, this fails *before* the pyaltiumlib oracle would. The
        // model-backed (STEP) path with embedded=true emits MODELID verbatim, so
        // the literal is stable.
        let mut model = ComponentBody::new("{G}", "part.step");
        model.embedded = true;
        let s = build_component_body_params(&model);
        let expected = "V7_LAYER=MECHANICAL6|NAME= |KIND=0|SUBPOLYINDEX=-1|UNIONINDEX=0|\
            ARCRESOLUTION=0.5mil|ISSHAPEBASED=FALSE|CAVITYHEIGHT=0mil|STANDOFFHEIGHT=0mil|\
            OVERALLHEIGHT=0mil|BODYPROJECTION=0|ARCRESOLUTION=0.5mil|BODYCOLOR3D=8421504|\
            BODYOPACITY3D=1.000|IDENTIFIER=|TEXTURE=|TEXTURECENTERX=0mil|TEXTURECENTERY=0mil|\
            TEXTURESIZEX=0.0001mil|TEXTURESIZEY=0.0001mil|TEXTUREROTATION= 0.00000000000000E+0000|\
            MODELID={G}|MODEL.CHECKSUM=0|MODEL.EMBED=TRUE|MODEL.NAME=part.step|MODEL.2D.X=0mil|\
            MODEL.2D.Y=0mil|MODEL.2D.ROTATION=0.000|MODEL.3D.ROTX=0.000|MODEL.3D.ROTY=0.000|\
            MODEL.3D.ROTZ=0.000|MODEL.3D.DZ=0mil|MODEL.MODELTYPE=1|MODEL.MODELSOURCE=Undefined";
        assert_eq!(s, expected);
        // Explicit guards for the two field-promoted literals callers most care about.
        assert!(s.contains("|BODYCOLOR3D=8421504|"), "got: {s}");
        assert!(s.contains("|BODYOPACITY3D=1.000|"), "got: {s}");
    }

    #[test]
    fn component_body_additive_fields_roundtrip() {
        use super::super::reader;
        let mut original = Footprint::new("RT_BODY_FIELDS");
        let mut body = ComponentBody::new("{G-1234}", "p.step");
        body.embedded = true;
        body.body_color_3d = 0x00FF_0000; // non-default red
        body.body_opacity_3d = 0.5;
        body.kind = 2;
        body.sub_poly_index = 3;
        body.union_index = 4;
        body.body_projection = 1;
        body.is_shape_based = true;
        body.model_2d_rotation = 90.0;
        body.name = "BODY_A".into();
        // Author on Mechanical 13 so the layer-reader fix (read the header layer byte,
        // not just the incomplete V7_LAYER map) is exercised through encode -> decode.
        body.layer = Layer::Mechanical13;
        original.add_component_body(body);

        let data = encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("RT_BODY_FIELDS");
        reader::parse_data_stream(&mut decoded, &data, None);

        let b = &decoded.component_bodies[0];
        assert_eq!(b.body_color_3d, 0x00FF_0000);
        assert!((b.body_opacity_3d - 0.5).abs() < 1e-9);
        assert_eq!(b.kind, 2);
        assert_eq!(b.sub_poly_index, 3);
        assert_eq!(b.union_index, 4);
        assert_eq!(b.body_projection, 1);
        assert!(b.is_shape_based);
        assert!((b.model_2d_rotation - 90.0).abs() < 1e-9);
        assert_eq!(b.name, "BODY_A");
        assert_eq!(b.layer, Layer::Mechanical13, "body layer round-trips");
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

    #[test]
    fn fill_block_writes_v7_layer_id() {
        // The fill tail (offsets 37-49) carries the layer-derived v7 layer id at
        // 42-45 — previously a blanket [0x00; 13] that left it zeroed.
        let block = encode_fill_block(&Fill::new(-1.0, -1.0, 1.0, 1.0, Layer::TopPaste));
        assert_eq!(block.len(), 50);
        let v7 = u32::from_le_bytes([block[42], block[43], block[44], block[45]]);
        assert_eq!(v7, v7_layer_id(layer_to_id(Layer::TopPaste)));
        assert_ne!(v7, 0, "a real layer must yield a non-zero v7 id");
    }

    #[test]
    fn region_param_string_is_canonical() {
        use crate::altium::pcblib::primitives::Vertex;
        let region = Region {
            vertices: vec![
                Vertex { x: -1.0, y: -1.0 },
                Vertex { x: 1.0, y: -1.0 },
                Vertex { x: 1.0, y: 1.0 },
                Vertex { x: -1.0, y: 1.0 },
            ],
            layer: Layer::TopCourtyard,
            ..Region::default()
        };
        let block = encode_region_properties(&region);
        let param_len = u32::from_le_bytes([block[18], block[19], block[20], block[21]]) as usize;
        let params = String::from_utf8_lossy(&block[22..22 + param_len]);
        let params = params.trim_end_matches('\0');
        // No leading pipe (region blocks are special), and the full canonical key set.
        assert!(!params.starts_with('|'), "no leading pipe: {params}");
        for key in [
            "V7_LAYER=MECHANICAL4",
            "NAME=",
            "KIND=0",
            "SUBPOLYINDEX=-1",
            "UNIONINDEX=0",
            "ARCRESOLUTION=0mil",
            "ISSHAPEBASED=FALSE",
            "CAVITYHEIGHT=0mil",
        ] {
            assert!(params.contains(key), "missing '{key}' in: {params}");
        }
    }

    #[test]
    fn wide_strings_empty_matches_altiumsharp_5_bytes() {
        // A footprint with no qualifying wide text emits AltiumSharp's empty form
        // `[01 00 00 00][00]`, not the spurious `[02 00 00 00][7C 00]` we used to write.
        let fp = Footprint::new("WS_EMPTY");
        assert_eq!(
            encode_component_wide_strings(&fp),
            vec![0x01, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn wide_strings_nonempty_has_no_trailing_pipe() {
        use crate::altium::TextJustification;
        let mk = |s: &str| Text {
            x: 0.0,
            y: 0.0,
            text: s.to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::MiddleCenter,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
        };
        let mut fp = Footprint::new("WS");
        fp.add_text(mk("AB")); // bytes 65, 66
        fp.add_text(mk("C")); //  byte 67
        let bytes = encode_component_wide_strings(&fp);

        // Leading pipe per entry, NO trailing pipe, null-terminated; len includes null.
        let payload = b"|ENCODEDTEXT0=65,66|ENCODEDTEXT1=67";
        let mut expected = u32::try_from(payload.len() + 1)
            .unwrap()
            .to_le_bytes()
            .to_vec();
        expected.extend_from_slice(payload);
        expected.push(0x00);
        assert_eq!(bytes, expected);
        assert!(
            !bytes.ends_with(&[b'|', 0x00]),
            "must not have a trailing pipe before the null"
        );
    }
}
