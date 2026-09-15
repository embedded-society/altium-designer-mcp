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
use super::{Footprint, PrimitiveKind};

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
pub(super) const fn layer_to_id(layer: Layer) -> u8 {
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
    ALT_FLAG_TESTPOINT_BOTTOM, ALT_FLAG_TESTPOINT_TOP, ALT_FLAG_UNLOCKED,
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
    // Altium clears the unlocked bit on a primitive it marks as a test point, so
    // mirror that rather than emitting a combination Altium never writes.
    if flags.contains(PcbFlags::TESTPOINT_TOP) {
        f |= ALT_FLAG_TESTPOINT_TOP;
        f &= !ALT_FLAG_UNLOCKED;
    }
    if flags.contains(PcbFlags::TESTPOINT_BOTTOM) {
        f |= ALT_FLAG_TESTPOINT_BOTTOM;
        f &= !ALT_FLAG_UNLOCKED;
    }
    // The bits read verbatim go back where they came from.
    let mut i = 0;
    while i < PcbFlags::DISK_BITS.len() {
        let (disk_bit, carrier) = PcbFlags::DISK_BITS[i];
        if flags.contains(carrier) {
            f |= disk_bit;
        }
        i += 1;
    }
    f
}

/// Writes the common 13-byte header for primitives.
fn write_common_header(data: &mut Vec<u8>, layer: Layer, flags: PcbFlags) {
    write_common_header_with_byte(data, disk_layer_byte(layer), flags);
}

/// The header layer byte Altium stores for a layer. Mechanical 17-32 have
/// no byte of their own: Altium writes 72 (Mechanical 16, the last the byte
/// can hold) and names the real layer in the V7 layer id or `V7_LAYER`
/// token — an AD-authored `Mechanical 20` track is `72` + `0x0102_0014` —
/// so that is what a from-scratch primitive gets too. Every other layer is
/// its [`layer_to_id`] byte.
pub(super) const fn disk_layer_byte(layer: Layer) -> u8 {
    match layer_to_id(layer) {
        186..=201 => 72,
        id => id,
    }
}

/// The layer byte a read primitive goes back out with: the byte as read
/// while the primitive still sits on the `MultiLayer` catch-all an unmapped
/// byte decodes to (see `raw_layer_id` on every layered primitive), else
/// the byte Altium stores for the layer it now has.
fn layer_byte(raw: Option<u8>, layer: Layer) -> u8 {
    match raw {
        Some(byte) if layer == Layer::MultiLayer => byte,
        _ => disk_layer_byte(layer),
    }
}

/// [`write_common_header`] with an explicit layer byte.
fn write_common_header_with_byte(data: &mut Vec<u8>, layer_byte: u8, flags: PcbFlags) {
    // Byte 0: Layer ID
    data.push(layer_byte);
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

    // Write name block: [block_len:4][str_len:1][name:str_len] — the same
    // bytes as PATTERN and the library component list.
    write_string_block(
        &mut data,
        &footprint_pattern_text(footprint),
        "footprint.name",
    )?;

    // A font face name past 31 UTF-16 units cannot fit the text record's
    // 64-byte field beside its terminator; refuse it here rather than write
    // it cut short. (The MCP layer refuses earlier with its own wording —
    // this covers direct library-API callers.)
    for (index, text) in footprint.text.iter().enumerate() {
        for (field, name) in [
            ("font_name", &text.font_name),
            ("barcode_font_name", &text.barcode_font_name),
        ] {
            let units = name.encode_utf16().count();
            if units > 31 {
                return Err(crate::altium::error::AltiumError::InvalidParameter {
                    name: format!("text[{index}].{field}"),
                    message: format!(
                        "'{name}' is {units} UTF-16 units long; a font face name has at most 31"
                    ),
                });
            }
        }
    }

    // A rounded rectangle's rounding lives in per-layer corner-radius bytes,
    // which only a full stack's size/shape block stores. A top_middle_bottom
    // slot would be written as shape id 1 and read back plain round, so it is
    // refused rather than degraded in silence.
    for (index, pad) in footprint.pads.iter().enumerate() {
        if pad.stack_mode == PadStackMode::TopMiddleBottom {
            if let Some(shapes) = &pad.per_layer_shapes {
                if shapes.contains(&PadShape::RoundedRectangle) {
                    return Err(crate::altium::error::AltiumError::InvalidParameter {
                        name: format!("pads[{index}].per_layer_shapes"),
                        message: format!(
                            "pad '{}': rounded_rectangle needs a per-layer corner radius, \
                             which only a full_stack pad stores",
                            pad.designator
                        ),
                    });
                }
            }
        }
    }

    // Primitives go out in the footprint's own order, which is Altium's
    // authoring order for anything read from a file (see
    // `Footprint::primitive_order`) and the canonical kind order otherwise.
    //
    // `WideStrings` is indexed over `footprint.text` alone, so the index each
    // text record carries is resolved up front rather than counted along the
    // way — the sequence below may put other kinds between two texts, but it
    // never reorders the texts themselves.
    let wide_indices = wide_string_indices(footprint);

    for (kind, index) in footprint.write_sequence() {
        match kind {
            PrimitiveKind::Arc => {
                data.push(0x01);
                encode_arc(&mut data, &footprint.arcs[index]);
            }
            PrimitiveKind::Pad => {
                data.push(0x02);
                encode_pad(&mut data, &footprint.pads[index])?;
            }
            PrimitiveKind::Via => {
                data.push(0x03);
                encode_via(&mut data, &footprint.vias[index]);
            }
            PrimitiveKind::Track => {
                data.push(0x04);
                encode_track(&mut data, &footprint.tracks[index]);
            }
            PrimitiveKind::Text => {
                data.push(0x05);
                encode_text(&mut data, &footprint.text[index], wide_indices[index]);
            }
            PrimitiveKind::Region => {
                data.push(0x0B);
                encode_region(&mut data, &footprint.regions[index]);
            }
            PrimitiveKind::Fill => {
                data.push(0x06);
                encode_fill(&mut data, &footprint.fills[index]);
            }
            PrimitiveKind::ComponentBody => {
                let body = &footprint.component_bodies[index];
                data.push(0x0C);
                let outline = resolve_body_outline(body, footprint);
                encode_component_body(&mut data, body, &outline);
            }
        }
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
        // In the per-layer table a rounded rectangle is stored as Round (1):
        // the corner-radius bytes below are what distinguish it, exactly as
        // the reader decodes it — the golden's rounded-rect pads carry 1 here,
        // and 9 was our invention.
        block.push(match shape {
            PadShape::RoundedRectangle => 1,
            other => pad_shape_to_id(other),
        });
    }

    // 32 corner radius percentages - 32 bytes. On disk a rounded rectangle
    // is shape id 1 (Round) plus a radius in 1..=99, so the radius byte is
    // what tells the two apart: a rounded-rectangle layer gets the pad's
    // radius (50% when none is set) and every other layer gets 0, or a
    // round layer would read back as a rounded rectangle.
    let default_radius = match pad.corner_radius_percent {
        Some(radius) if radius > 0 => radius,
        _ => 50,
    };
    for i in 0..32 {
        let radius = pad
            .per_layer_corner_radii
            .as_ref()
            .and_then(|radii| radii.get(i).copied())
            .unwrap_or_else(|| {
                let layer_shape = pad
                    .per_layer_shapes
                    .as_ref()
                    .and_then(|shapes| shapes.get(i).copied())
                    .unwrap_or(pad.shape);
                if layer_shape == PadShape::RoundedRectangle {
                    default_radius
                } else {
                    0
                }
            });
        // Corner radius is a 0-100 percentage; clamp on write to mirror the
        // reader's `.min(100)`, so an out-of-range value round-trips symmetrically
        // rather than emitting a byte the reader would then clamp.
        block.push(radius.min(100));
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
    // A rounded rectangle NEEDS the block even with a default radius: the
    // geometry byte @49 stores 1 (Round) for it — the golden's rounded-rect
    // pads prove the shape lives in the radius bytes, not the id — so without
    // this block the rounding would be unreadable.
    let needs_size_shape = pad.hole_shape != HoleShape::Round
        || pad.corner_radius_percent.is_some()
        || pad.shape == PadShape::RoundedRectangle;

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

/// Encodes the canonical 651-byte pad size/shape block (Block 5) for a simple
/// pad that needs a non-round hole or an explicit corner radius. Layout matches
/// `AltiumSharp` `WritePad` (values uniform across layers); the reader pairs
/// with this via [`super::reader`] when the block is >= 596 bytes.
fn encode_pad_size_shape_block(pad: &Pad) -> Vec<u8> {
    let mut b = Vec::with_capacity(596);
    let w = from_mm(pad.width);
    let h = from_mm(pad.height);
    // In the size/shape block a rounded rectangle is stored as Round (1) with
    // the radius bytes carrying the rounding — the golden's rounded-rect pads
    // carry 1 in every per-layer slot; 9 lives only in the single @49 byte of
    // the main geometry block.
    let shape_id = match pad.shape {
        PadShape::RoundedRectangle => 1,
        other => pad_shape_to_id(other),
    };
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
                                                               // 532-563: per-layer shapes. THIS table keeps the full shape id — the
                                                               // golden's rounded-rect pad stores 9 here while the internal-layer table
                                                               // @232-260 stores 1 (with the radius bytes carrying the rounding).
    let full_shape_id = pad_shape_to_id(pad.shape);
    for _ in 0..32 {
        b.push(full_shape_id); // 532-563: per-layer shapes
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
    b.push(full_shape_id); // 640: flag4 = full pad shape id (9 for rounded-rect)
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
    if (186..=201).contains(&layer) {
        return 0x0102_0000 + (l - 169); // mechanical 17-32 (Altium Designer 18+)
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
fn build_pad_extended_tail(pad: &Pad) -> Vec<u8> {
    const START: usize = PAD_EXTENDED_TAIL_START;
    // Every overlay below writes inside the first 125 tail bytes, which both
    // the 141-byte AltiumSharp template and AD24's own 133-byte tails cover.
    // A read tail is replayed verbatim as the base — including its LENGTH, so
    // an AD24 pad stays 194 bytes overall instead of growing the template's
    // surplus — and a shorter foreign tail falls back to the template rather
    // than panicking an overlay.
    const MIN_REPLAY_LEN: usize = 125;
    let mut tail: Vec<u8> = match pad.raw_tail.as_deref() {
        Some(raw) if raw.len() >= MIN_REPLAY_LEN => raw.to_vec(),
        _ => PAD_EXTENDED_TAIL_TEMPLATE.to_vec(),
    };
    let replaying = pad
        .raw_tail
        .as_deref()
        .is_some_and(|r| r.len() >= MIN_REPLAY_LEN);

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

    // Solder-mask expansion measured from the hole edge rather than the pad edge
    // (bool @125). Sits just past the solder-mask cache mirror at @121-124.
    tail[125 - START] = u8::from(pad.solder_mask_expansion_from_hole_edge);

    // Jumper group id (i16 @110-111); 0 leaves the template bytes as they are.
    tail[110 - START..112 - START].copy_from_slice(&pad.jumper_id.to_le_bytes());
    // 114-117: v7 layer id (derived from the pad's layer)
    tail[114 - START..118 - START]
        .copy_from_slice(&v7_layer_id(layer_to_id(pad.layer)).to_le_bytes());
    // 126-141 / 142-157: the two per-pad identity GUIDs (GUID-A / GUID-B). A
    // read-back value (braced GUID string) is replayed verbatim so a loaded pad
    // round-trips its exact bytes — including the golden's nil GUIDs; `None`
    // (from scratch) generates a fresh random GUID per pad, the historical
    // behaviour (mirrors AltiumSharp's PadBuilder Guid.NewGuid defaults).
    tail[126 - START..142 - START].copy_from_slice(
        &pad.identity_guid
            .as_deref()
            .and_then(guid_bytes_from_string)
            .unwrap_or_else(generate_guid),
    );
    tail[142 - START..158 - START].copy_from_slice(
        &pad.identity_guid_b
            .as_deref()
            .and_then(guid_bytes_from_string)
            .unwrap_or_else(generate_guid),
    );
    // 162-165 / 166-169: drill tolerances. `None` leaves the template's
    // 0x7FFFFFFF "unset" sentinel (byte-identical); `Some(mm)` writes the raw.
    if let Some(tol) = pad.hole_positive_tolerance {
        tail[162 - START..166 - START].copy_from_slice(&from_mm(tol).to_le_bytes());
    }
    if let Some(tol) = pad.hole_negative_tolerance {
        tail[166 - START..170 - START].copy_from_slice(&from_mm(tol).to_le_bytes());
    }
    // 185: reserved marker. The AltiumSharp-derived template value is 0x03;
    // AD24's own pads carry 0x01 — a replayed tail keeps whatever was read,
    // and only the from-scratch template gets the historical stamp.
    if !replaying {
        tail[185 - START] = 0x03;
    }

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
    write_common_header_with_byte(
        &mut block,
        layer_byte(pad.raw_layer_id, pad.layer),
        pad.flags,
    );
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
    // Rounded rectangle stores as 1 in every per-layer slot (radius bytes
    // carry the rounding); see encode_pad_size_shape_block.
    let shape_id = match pad.shape {
        PadShape::RoundedRectangle => 1,
        other => pad_shape_to_id(other),
    };
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
                .map_or(shape_id, |s| match s {
                    PadShape::RoundedRectangle => 1,
                    other => pad_shape_to_id(other),
                })
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

    // Is plated - offset 60. Altium stores this as an independent bool that
    // defaults to 1 for every pad, SMD included (the golden fixture's SMD pads
    // all carry 1; AltiumSharp's PcbPad.IsPlated defaults to true). The writer
    // It is independent of hole_size; deriving it from that emits 0 for SMD pads.
    block.push(u8::from(pad.is_plated));

    // Extended tail — offsets 61.. . Length follows the tail: a replayed AD24
    // pad stays 194 bytes (133-byte tail), a from-scratch pad keeps the
    // 202-byte template layout.
    block.extend_from_slice(&build_pad_extended_tail(pad));

    debug_assert!(
        block.len() == PAD_MAIN_BLOCK_LEN || pad.raw_tail.is_some(),
        "from-scratch pad main block must stay {PAD_MAIN_BLOCK_LEN} bytes"
    );
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
    // Base = the block as read whenever it is at least as long as the
    // template the overlays below assume — length included, so a library's
    // 351-byte vias keep the thirty bytes past the template this crate does
    // not model and every other unmodelled byte round-trips; else the
    // template with its identity-GUID slots zeroed, which is what AD24 writes
    // there for every library via (the golden).
    let mut block: Vec<u8> = via
        .raw_block
        .as_deref()
        .filter(|raw| raw.len() >= VIA_SR1_TEMPLATE.len())
        .map_or_else(
            || {
                let mut b = VIA_SR1_TEMPLATE;
                b[259..291].fill(0);
                b.to_vec()
            },
            <[u8]>::to_vec,
        );

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

    // @258 mask-from-hole-edge bool and @312 drill-pair classification. Both are 0 in
    // the template, so a default via stays byte-identical.
    block[258] = u8::from(via.solder_mask_expansion_from_hole_edge);
    block[312] = via.drill_layer_pair_type.to_id();
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
    // Write the name's UTF-16 units plus ONE null terminator, leaving the rest
    // of the field as the base provides it. Altium reads the field as a
    // null-terminated string and leaves whatever lay beyond the terminator in
    // place — the golden's fields carry repeating junk there — so zero-filling
    // was rewriting bytes AD itself preserves. A from-scratch text's template
    // base is all zeros, keeping the historical output byte-identical.
    let mut i = 0;
    for unit in name.encode_utf16() {
        if i + 2 > 62 {
            break; // leave room for the terminator
        }
        dst[i..i + 2].copy_from_slice(&unit.to_le_bytes());
        i += 2;
    }
    dst[i..i + 2].copy_from_slice(&[0, 0]);
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
    write_common_header_with_byte(
        &mut block,
        layer_byte(track.raw_layer_id, track.layer),
        track.flags,
    );
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
    write_common_header_with_byte(
        &mut block,
        layer_byte(arc.raw_layer_id, arc.layer),
        arc.flags,
    );
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
fn encode_text(data: &mut Vec<u8>, text: &Text, wide_index: Option<u32>) {
    // Block 0: Geometry
    let geometry = encode_text_geometry(text, wide_index);
    write_block(data, &geometry);

    // Block 1: Text content, a Pascal short string capped at 255 bytes.
    //
    // Longer text is truncated here rather than rejected, because the full
    // value is carried by `/WideStrings` and addressed by the index stamped in
    // the geometry block above — which is how Altium itself stores it, and the
    // only way a text that Altium can author survives a read-modify-write.
    // Windows-1252 is single-byte, so the cut cannot split a character.
    let encoded = crate::altium::encode_windows1252(&text.text);
    let truncated = &encoded[..encoded.len().min(255)];
    crate::altium::framing::write_string_block(data, truncated);
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
/// `wide_index` is this text's `/WideStrings` entry number, stamped as an `i32`
/// at offset 115 — the field a reader uses to find the primitive's out-of-line
/// content. It must match the enumeration in
/// [`encode_component_wide_strings`]. `None` writes Altium's `-1`, meaning the
/// text has no entry (special or empty).
pub fn encode_text_geometry(text: &Text, wide_index: Option<u32>) -> Vec<u8> {
    // Base = the block as read when its length matches the template layout the
    // overlays assume, so AD's cached render metrics (bytes we do not model)
    // round-trip; else the template.
    let mut block = text
        .raw_geometry
        .as_deref()
        .and_then(|raw| <[u8; TEXT_SR1_TEMPLATE.len()]>::try_from(raw).ok())
        .unwrap_or(TEXT_SR1_TEMPLATE);

    // i32 at 115: the entry number, or -1 for "no entry".
    let index_field: i32 = wide_index.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1);
    block[115..119].copy_from_slice(&index_field.to_le_bytes());

    // Common header (offsets 0-12): layer + Altium flag word + 0xFF net/poly/comp.
    let mut header = Vec::with_capacity(13);
    write_common_header_with_byte(
        &mut header,
        layer_byte(text.raw_layer_id, text.layer),
        text.flags,
    );
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

    // Comment/Designator field markers (offsets 40/41, IsComment/IsDesignator).
    // Defaults false reproduce the template's 0x00 bytes (every golden text
    // carries 0x00 at both offsets); offsets verified against AltiumSharp
    // ReadText (B(40)/B(41)) and its writer (b[40]/b[41]).
    block[40] = u8::from(text.is_comment);
    block[41] = u8::from(text.is_designator);

    // Font bold (offset 44, FontBold — twin of italic@45). Default false
    // reproduces the template's 0x00.
    block[44] = u8::from(text.bold);

    // Authoritative text kind (offset 160).
    block[160] = text_kind_to_id(text.kind);

    // Base font type (offset 43) is derived from the text kind: Stroke -> 0,
    // TrueType -> 1. The template default is 0, so stroke text stays
    // byte-identical; the TrueType record
    // (kind@160=1 with base@43=0). BarCode is a deferred kind and not modelled here.
    // @43 (base font type) is stamped only from scratch: the golden's special
    // strings carry 0 here despite a TrueType kind, so the byte is not a pure
    // function of the kind — a replayed base keeps what AD wrote, and the
    // authoritative kind byte @160 above is still always overlaid.
    if text.raw_geometry.is_none() {
        block[43] = u8::from(!matches!(text.kind, TextKind::Stroke));
    }
    // Italic style (offset 45). Default false reproduces the template's 0x00.
    block[45] = u8::from(text.italic);

    // Font name (offsets 46-109, UTF-16, 64-byte field). The default "Arial"
    // reproduces the template's exact bytes: the UTF-16 name (max 62 bytes) then
    // zero fill — byte-identical to the "Arial\0…" template for a from-scratch text.
    encode_font_name_field(&mut block[46..110], &text.font_name);

    // Text-box justification (offset 132). The from-scratch default `BottomLeft`
    // encodes to 0x03 (the template byte at offset 132); other anchors map onto
    // Altium's column-major text-box encoding.
    block[132] = pcb_justification_to_id(text.justification);

    // Barcode block (offsets verified by diffing two authored barcodes). Each field
    // is written only when set, so a plain text — and a barcode that does not
    // override a value — replays the template bytes unchanged.
    for (offset, value) in [
        (137, text.barcode_full_width),
        (141, text.barcode_full_height),
        (145, text.barcode_x_margin),
        (149, text.barcode_y_margin),
    ] {
        if let Some(mm) = value {
            block[offset..offset + 4].copy_from_slice(&from_mm(mm).to_le_bytes());
        }
    }
    if text.barcode_kind != 0 {
        block[157] = text.barcode_kind;
    }
    if text.barcode_inverted {
        block[159] = 1;
    }
    if text.barcode_show_text {
        block[225] = 1;
    }
    // UTF-16LE, null-padded into the fixed 64-byte field at @161-224.
    if !text.barcode_font_name.is_empty() {
        // Pad-preserving, like encode_font_name_field: write the units plus
        // one terminator and leave the base's bytes beyond it — AD keeps junk
        // there and reads only to the null.
        encode_font_name_field(&mut block[161..225], &text.barcode_font_name);
    }

    // Inverted (knockout) text-box descriptor. Defaults reproduce the template
    // bytes exactly (@110/123 = 0x00, @111/133 = 0, @124/128 = the template's
    // precomputed text-box size), so a from-scratch plain text stays byte-identical.
    //   @110 IsInverted (bool)   @111 InvertedBorder (i32 coord)
    //   @123 UseInvertedRectangle (bool)   @124 InvertedRectWidth (i32 coord)
    //   @128 InvertedRectHeight (i32 coord)   @133 InvertedRectTextOffset (i32 coord)
    block[110] = u8::from(text.is_inverted);
    if let Some(border) = text.inverted_border {
        block[111..115].copy_from_slice(&from_mm(border).to_le_bytes());
    }
    block[123] = u8::from(text.use_inverted_rectangle);
    // `None` leaves the template's precomputed width/height in place (byte-identity
    // for plain text); a framed inverted text overlays its explicit dimensions.
    if let Some(width) = text.inverted_rect_width {
        block[124..128].copy_from_slice(&from_mm(width).to_le_bytes());
    }
    if let Some(height) = text.inverted_rect_height {
        block[128..132].copy_from_slice(&from_mm(height).to_le_bytes());
    }
    if let Some(offset) = text.inverted_rect_text_offset {
        block[133..137].copy_from_slice(&from_mm(offset).to_le_bytes());
    }

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
/// Altium identifies a Region's or `ComponentBody`'s layer by this parameter
/// string, NOT the common-header layer byte, and it is a fixed vocabulary
/// rather than the display name: `TOP`, not `TOPLAYER`; `PLANE3`, not
/// `INTERNALPLANE3`. A token Altium cannot resolve leaves the primitive on Top
/// Layer (copper), so `BOTTOMLAYER` — which is what stripping the spaces out of
/// the display name produces — silently moves a bottom-side region to the top.
///
/// The vocabulary is `Advpcb.dll`'s own, held there as UTF-16LE strings:
/// `MECHANICAL`, `MID` and `PLANE` are prefixes taking a number, alongside
/// `TOP`, `BOTTOM`, `TOPOVERLAY`, `BOTTOMOVERLAY`, `TOPPASTE`, `BOTTOMPASTE`,
/// `TOPSOLDER`, `BOTTOMSOLDER`, `DRILLGUIDE`, `DRILLDRAWING`, `KEEPOUT`,
/// `MULTILAYER` and `CONNECT`. The golden confirms `TOP`, `MECHANICAL1` and
/// `MECHANICAL13`.
pub(super) fn v7_layer_token(layer: Layer) -> String {
    match layer_to_id(layer) {
        1 => "TOP".to_string(),
        id @ 2..=31 => format!("MID{}", id - 1),
        32 => "BOTTOM".to_string(),
        33 => "TOPOVERLAY".to_string(),
        34 => "BOTTOMOVERLAY".to_string(),
        35 => "TOPPASTE".to_string(),
        36 => "BOTTOMPASTE".to_string(),
        37 => "TOPSOLDER".to_string(),
        38 => "BOTTOMSOLDER".to_string(),
        id @ 39..=54 => format!("PLANE{}", id - 38),
        55 => "DRILLGUIDE".to_string(),
        56 => "KEEPOUT".to_string(),
        id @ 57..=72 => format!("MECHANICAL{}", id - 56),
        73 => "DRILLDRAWING".to_string(),
        74 => "MULTILAYER".to_string(),
        75 => "CONNECT".to_string(),
        76 => "BACKGROUND".to_string(),
        77 => "DRCERRORS".to_string(),
        78 => "SELECTIONS".to_string(),
        79 => "VISIBLEGRID1X".to_string(),
        80 => "VISIBLEGRID10X".to_string(),
        81 => "PADHOLES".to_string(),
        82 => "VIAHOLES".to_string(),
        id @ 186..=201 => format!("MECHANICAL{}", id - 169),
        // The pad masters and the DRC detail layer have no token in the
        // vocabulary. Nothing a library can hold sits on them, so the display
        // name stands in rather than a guess dressed up as a fact.
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
/// The canonical region parameter keys, in from-scratch emission order.
const REGION_CANONICAL_KEYS: [&str; 8] = [
    "V7_LAYER",
    "NAME",
    "KIND",
    "SUBPOLYINDEX",
    "UNIONINDEX",
    "ARCRESOLUTION",
    "ISSHAPEBASED",
    "CAVITYHEIGHT",
];

/// One canonical region key's value, from the typed field that models it.
fn region_canonical_value(region: &Region, key: &str) -> String {
    match key {
        "V7_LAYER" => region
            .v7_layer
            .clone()
            .unwrap_or_else(|| v7_layer_token(region.layer)),
        "NAME" => region.name.clone(),
        "KIND" => region.kind.to_id().to_string(),
        "SUBPOLYINDEX" => region.sub_poly_index.to_string(),
        "UNIONINDEX" => region.union_index.to_string(),
        "ARCRESOLUTION" => format_mil_coord(region.arc_resolution),
        "ISSHAPEBASED" => if region.is_shape_based {
            "TRUE"
        } else {
            "FALSE"
        }
        .to_string(),
        "CAVITYHEIGHT" => format_mil_coord(region.cavity_height),
        _ => unreachable!("not a canonical region key"),
    }
}

/// Builds a region's nested parameter string (no leading pipe).
///
/// A region read from a file replays its own key ORDER (`param_key_order`):
/// Altium interleaves unmodelled keys with the canonical set — a board cutout
/// stores `LAYER`/`KEEPOUT`/`ISBOARDCUTOUT` right after `NAME`, not appended —
/// so canonical keys are emitted from their typed fields at their original
/// positions and everything else comes from `additional_parameters`, consumed
/// in read order. Canonical keys the original block lacked are appended so a
/// typed edit is never dropped. A from-scratch region (empty order) emits the
/// canonical set in canonical order, byte-identical to the historical output.
fn build_region_param_text(region: &Region) -> String {
    if region.param_key_order.is_empty() {
        let params = REGION_CANONICAL_KEYS
            .iter()
            .map(|key| format!("{key}={}", region_canonical_value(region, key)))
            .collect::<Vec<_>>()
            .join("|");
        return append_additional_params(params, &region.additional_parameters);
    }

    let mut additional = region.additional_parameters.iter();
    let mut parts: Vec<String> = Vec::with_capacity(region.param_key_order.len());
    for key in &region.param_key_order {
        if REGION_CANONICAL_KEYS.contains(&key.as_str()) {
            parts.push(format!("{key}={}", region_canonical_value(region, key)));
        } else if let Some((k, v)) = additional.next() {
            parts.push(format!("{k}={v}"));
        }
    }
    for key in REGION_CANONICAL_KEYS {
        if !region.param_key_order.iter().any(|k| k == key) {
            parts.push(format!("{key}={}", region_canonical_value(region, key)));
        }
    }
    parts.join("|")
}

#[allow(clippy::cast_possible_truncation)] // Vertex/hole count and param length fit in u32/u16
fn encode_region_properties(region: &Region) -> Vec<u8> {
    let vertex_count = region.vertices.len();

    // Parameter string in Altium's canonical key order. Unlike other Altium param
    // blocks, a region's nested block has NO leading pipe and carries the full
    // canonical key set (matching AltiumSharp `BuildRegionParamText`). Each value is
    // now taken from the typed field; a default region reproduces the historical
    // hard-coded string byte-for-byte (KIND=0, NAME=, ARCRESOLUTION=0mil, ...).
    let params = build_region_param_text(region);
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
    write_common_header_with_byte(
        &mut block,
        layer_byte(fill.raw_layer_id, fill.layer),
        fill.flags,
    );
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

    // Layer ID (1 byte). An unmapped byte the reader could not decode is
    // replayed verbatim while the body still sits on the `MultiLayer`
    // catch-all its decode produced (#391); a retargeted body gets the
    // byte Altium stores for its new layer.
    block.push(layer_byte(body.raw_layer_id, body.layer));

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

/// Encodes a body identifier as its on-disk form: a comma-separated list of
/// decimal Unicode code points (empty stays empty). Inverse of the reader's
/// `decode_identifier`.
fn encode_identifier(identifier: &str) -> String {
    identifier
        .chars()
        .map(|c| (c as u32).to_string())
        .collect::<Vec<_>>()
        .join(",")
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
    // A captured token replays under the same condition as the raw layer
    // byte it pairs with (#391), so byte and token can never re-emit
    // mismatched against each other.
    let v7_token = match &body.v7_layer {
        Some(token) if body.layer == Layer::MultiLayer => token.clone(),
        _ => v7_layer_token(body.layer),
    };
    params.push(format!("V7_LAYER={v7_token}"));

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
    params.push(format!(
        "CAVITYHEIGHT={}",
        format_mil_coord(body.cavity_height)
    ));
    // Use the canonical trimmed mil formatting (as the region encoder does) rather
    // than raw {} float formatting, so body heights match Altium's own output shape.
    params.push(format!(
        "STANDOFFHEIGHT={}",
        format_mil_coord(body.standoff_height)
    ));
    params.push(format!(
        "OVERALLHEIGHT={}",
        format_mil_coord(body.overall_height)
    ));
    params.push(format!("BODYPROJECTION={}", body.body_projection));
    // Altium repeats ARCRESOLUTION after BODYPROJECTION (verbatim shape from the
    // BODY_3D golden files).
    params.push("ARCRESOLUTION=0.5mil".to_string());
    params.push(format!("BODYCOLOR3D={}", body.body_color_3d));
    params.push(format!("BODYOPACITY3D={:.3}", body.body_opacity_3d));
    // IDENTIFIER is a comma-separated list of decimal Unicode code points
    // (manual/identifier.PcbLib: `µΩ电` = `181,937,30005`); an empty
    // identifier emits the bare key, as both authoring routes do.
    params.push(format!(
        "IDENTIFIER={}",
        encode_identifier(&body.identifier)
    ));
    params.push("TEXTURE=".to_string());
    // The texture values round-trip verbatim when read (the UI writes
    // 0.0001mil sizes where a scripted body carries 0mil); a from-scratch
    // body emits the scripted-body defaults.
    let texture = |value: &Option<String>, default: &str| {
        value.clone().unwrap_or_else(|| default.to_string())
    };
    params.push(format!(
        "TEXTURECENTERX={}",
        texture(&body.texture_center_x, "0mil")
    ));
    params.push(format!(
        "TEXTURECENTERY={}",
        texture(&body.texture_center_y, "0mil")
    ));
    params.push(format!(
        "TEXTURESIZEX={}",
        texture(&body.texture_size_x, "0mil")
    ));
    params.push(format!(
        "TEXTURESIZEY={}",
        texture(&body.texture_size_y, "0mil")
    ));
    params.push(format!(
        "TEXTUREROTATION={}",
        texture(&body.texture_rotation, " 0.00000000000000E+0000")
    ));

    // Model reference — present exactly when the body HAS a model identity
    // or a model file. Both authoring routes are golden-pinned: a
    // script-authored extruded body (BODY3D, PRIMPROPS) has no MODELID and
    // ends at TEXTUREROTATION with no MODEL keys at all, while a UI-authored
    // extruded body (manual/identifier.PcbLib) carries a MODELID and the full
    // group with MODEL.MODELTYPE=0 plus the EXTRUDED Z range
    // (standoff..overall) and no MODELSOURCE. A model-backed body (EMBSTEP)
    // uses MODELTYPE=1 plus MODEL.MODELSOURCE=Undefined and no EXTRUDED
    // range. A body that references a STEP file it does not embed carries the
    // same group under an EMPTY MODELID (`MODELID=|MODEL.CHECKSUM=0|
    // MODEL.EMBED=FALSE|MODEL.NAME=test_0805.step`, a UI-authored library), so
    // the group is keyed on the file name too, or the reference is lost.
    // Inventing a MODELID for a body that has none is what #377 removed; a
    // body that has one keeps its group whatever its type.
    if !body.model_id.is_empty() || !body.model_name.is_empty() || body.embedded {
        params.push(format!("MODELID={}", body.model_id));
        // Round-trip the stored checksum verbatim.
        params.push(format!("MODEL.CHECKSUM={}", body.model_checksum));
        params.push(format!(
            "MODEL.EMBED={}",
            if body.embedded { "TRUE" } else { "FALSE" }
        ));
        params.push(format!("MODEL.NAME={}", body.model_name));
        params.push(format!("MODEL.2D.X={}", format_mil_coord(body.model_2d_x)));
        params.push(format!("MODEL.2D.Y={}", format_mil_coord(body.model_2d_y)));
        params.push(format!("MODEL.2D.ROTATION={:.3}", body.model_2d_rotation));
        params.push(format!("MODEL.3D.ROTX={:.3}", body.rotation_x));
        params.push(format!("MODEL.3D.ROTY={:.3}", body.rotation_y));
        params.push(format!("MODEL.3D.ROTZ={:.3}", body.rotation_z));
        params.push(format!("MODEL.3D.DZ={}", format_mil_coord(body.z_offset)));
        if extruded {
            params.push("MODEL.MODELTYPE=0".to_string());
            params.push(format!(
                "MODEL.EXTRUDED.MINZ={}",
                format_mil_coord(body.standoff_height)
            ));
            params.push(format!(
                "MODEL.EXTRUDED.MAXZ={}",
                format_mil_coord(body.overall_height)
            ));
        } else {
            params.push("MODEL.MODELTYPE=1".to_string());
            params.push("MODEL.MODELSOURCE=Undefined".to_string());
        }
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
    let additional: Vec<&(String, String)> = body
        .additional_parameters
        .iter()
        .filter(|(key, _)| !emitted.contains(key))
        .collect();

    replay_body_key_order(&params, &additional, &body.param_key_order)
}

/// Emits a body's canonical `KEY=VALUE` tokens and the unmodelled ones it
/// carried in the order they were read.
///
/// A body read from a file replays its own key order: Altium interleaves
/// unmodelled keys with the canonical set (`BODYOVERRIDECOLOR=TRUE` sits right
/// after `BODYOPACITY3D`), so each canonical key goes at its read position and
/// the unmodelled ones fill theirs in read order. Canonical keys the original
/// lacked — a typed edit, or a model group the body gained — are appended; a
/// from-scratch body has no order and emits the canonical set followed by the
/// unmodelled keys. A canonical key can repeat (Altium writes ARCRESOLUTION
/// twice), so each key holds a queue of its values in emission order.
fn replay_body_key_order(
    params: &[String],
    additional: &[&(String, String)],
    order: &[String],
) -> String {
    if order.is_empty() {
        let mut all: Vec<String> = params.to_vec();
        all.extend(
            additional
                .iter()
                .map(|(key, value)| format!("{key}={value}")),
        );
        return all.join("|");
    }
    let mut canonical: std::collections::HashMap<&str, std::collections::VecDeque<&str>> =
        std::collections::HashMap::new();
    for p in params {
        if let Some((key, value)) = p.split_once('=') {
            canonical.entry(key).or_default().push_back(value);
        }
    }
    let mut ordered: Vec<String> = Vec::with_capacity(params.len() + additional.len());
    let mut extra = additional.iter();
    for key in order {
        if let Some(value) = canonical
            .get_mut(key.as_str())
            .and_then(std::collections::VecDeque::pop_front)
        {
            ordered.push(format!("{key}={value}"));
        } else if let Some((k, v)) = extra.next() {
            ordered.push(format!("{k}={v}"));
        }
    }
    for p in params {
        if let Some((key, _)) = p.split_once('=') {
            if canonical
                .get_mut(key)
                .and_then(std::collections::VecDeque::pop_front)
                .is_some()
            {
                ordered.push(p.clone());
            }
        }
    }
    ordered.extend(extra.map(|(key, value)| format!("{key}={value}")));
    ordered.join("|")
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
// Footprint Parameters stream
// =============================================================================

/// The value the footprint carries for a `Parameters` key, if any.
fn carried_param<'a>(footprint: &'a Footprint, key: &str) -> Option<&'a str> {
    footprint
        .additional_parameters
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

/// Whether the carried `PATTERN` and `UNICODE__PATTERN` entries still describe
/// the footprint's name. They are re-emitted verbatim then — a widened twin
/// included, since Altium derives the storage from the name it reads there —
/// and rebuilt from the name otherwise.
fn carried_name_is_current(footprint: &Footprint) -> bool {
    carried_param(footprint, "PATTERN").is_some_and(|plain| {
        crate::altium::unicode_field_text(carried_param(footprint, "UNICODE__PATTERN"), plain)
            == footprint.name
    })
}

/// Whether the carried `DESCRIPTION` and `UNICODE__DESCRIPTION` entries still
/// describe the footprint's description.
fn carried_description_is_current(footprint: &Footprint) -> bool {
    let plain = carried_param(footprint, "DESCRIPTION").unwrap_or_default();
    crate::altium::unicode_field_text(carried_param(footprint, "UNICODE__DESCRIPTION"), plain)
        == footprint.description
}

/// The text the footprint's `PATTERN`, `DESCRIPTION` and `UNICODE` twins
/// will hold: carried entries verbatim while they still describe the name and
/// description, rebuilt from the field otherwise.
struct ParamsPlan {
    pattern: String,
    description: String,
    name_twin: Option<String>,
    description_twin: Option<String>,
}

impl ParamsPlan {
    fn of(footprint: &Footprint) -> Self {
        let name_current = carried_name_is_current(footprint);
        let description_current = carried_description_is_current(footprint);
        let plain = |current: bool, key: &str, value: &str| {
            if current {
                carried_param(footprint, key)
                    .unwrap_or_default()
                    .to_string()
            } else {
                crate::altium::to_wire_text(value)
            }
        };
        let twin = |current: bool, key: &str, value: &str| -> Option<String> {
            if current {
                carried_param(footprint, key).map(str::to_string)
            } else if value.is_ascii() {
                None
            } else {
                Some(crate::altium::utf16_units_decimal(value))
            }
        };
        Self {
            pattern: plain(name_current, "PATTERN", &footprint.name),
            description: plain(description_current, "DESCRIPTION", &footprint.description),
            name_twin: twin(name_current, "UNICODE__PATTERN", &footprint.name),
            description_twin: twin(
                description_current,
                "UNICODE__DESCRIPTION",
                &footprint.description,
            ),
        }
    }
}

/// The footprint's `PATTERN` text, which `Library/Data`, the Data stream's
/// name block and `SectionKeys` carry in the same bytes: the carried bytes
/// while they still describe the name, else the name's wire form.
pub(super) fn footprint_pattern_text(footprint: &Footprint) -> String {
    ParamsPlan::of(footprint).pattern
}

/// The name Altium will read for the footprint, which is the name it derives
/// a short footprint's storage from: the twin's code units when the block
/// carries or gains one — unfolded, since Altium does not fold a widened
/// twin — else the `PATTERN` text as Windows-1252 characters.
pub(super) fn footprint_name_as_altium_reads_it(footprint: &Footprint) -> String {
    let plan = ParamsPlan::of(footprint);
    plan.name_twin
        .as_deref()
        .and_then(crate::altium::text_from_utf16_units)
        .unwrap_or(plan.pattern)
}

/// Builds the footprint's `Parameters` block text (leading `|`, no trailing
/// pipe, not yet NUL-terminated) in the shape Altium writes.
///
/// From scratch the block is `PATTERN`, `HEIGHT`, `DESCRIPTION`, `ITEMGUID`,
/// `REVISIONGUID`; a name or description outside ASCII adds its UTF-16 code
/// units in a `UNICODE__PATTERN` / `UNICODE__DESCRIPTION` twin after the other
/// keys and a `UNICODE=EXISTS` at both ends, as `manual/i18n4.PcbLib` shows. A
/// footprint read from a file replays its `param_key_order`: `HEIGHT` from its
/// field, `PATTERN`, `DESCRIPTION` and the twins verbatim while they still
/// describe the name and description and rebuilt from the field when they do
/// not, every other carried key verbatim. A twin the carried order lacks — a
/// name that left ASCII — puts the block back in canonical order, since
/// replaying could not be byte-faithful anyway.
#[allow(clippy::too_many_lines)] // one block, one shape decision after another
pub(super) fn build_footprint_params(footprint: &Footprint) -> String {
    let ParamsPlan {
        pattern,
        description,
        name_twin,
        description_twin,
    } = ParamsPlan::of(footprint);
    let unicode = name_twin.is_some() || description_twin.is_some();
    let height = format_mil_coord(footprint.height);

    // A key the writer computes: `Some(None)` drops it from the block.
    let computed = |key: &str| -> Option<Option<String>> {
        match key.to_ascii_uppercase().as_str() {
            "PATTERN" => Some(Some(pattern.clone())),
            "HEIGHT" => Some(Some(height.clone())),
            "DESCRIPTION" => Some(Some(description.clone())),
            "UNICODE" => Some(unicode.then(|| "EXISTS".to_string())),
            "UNICODE__PATTERN" => Some(name_twin.clone()),
            "UNICODE__DESCRIPTION" => Some(description_twin.clone()),
            _ => None,
        }
    };
    let push = |out: &mut String, key: &str, value: &str| {
        out.push('|');
        out.push_str(key);
        out.push('=');
        out.push_str(value);
    };
    let order_has = |key: &str| {
        footprint
            .param_key_order
            .iter()
            .any(|k| k.eq_ignore_ascii_case(key))
    };
    let replay = !footprint.param_key_order.is_empty()
        && (name_twin.is_none() || order_has("UNICODE__PATTERN"))
        && (description_twin.is_none() || order_has("UNICODE__DESCRIPTION"));

    let mut out = String::new();
    if replay {
        let mut carried: Vec<Option<&(String, String)>> =
            footprint.additional_parameters.iter().map(Some).collect();
        let mut take = |key: &str| -> Option<&(String, String)> {
            carried
                .iter_mut()
                .find(|slot| slot.is_some_and(|(k, _)| k.eq_ignore_ascii_case(key)))
                .and_then(Option::take)
        };
        for key in &footprint.param_key_order {
            if let Some(value) = computed(key) {
                // Consumed so it cannot resurface as a leftover below.
                take(key);
                if let Some(value) = value {
                    push(&mut out, key, &value);
                }
            } else if let Some((k, v)) = take(key) {
                push(&mut out, k, v);
            }
            // A key the order names but nothing carries — dropped through
            // JSON — is simply gone.
        }
        // Pairs supplied without a matching order entry follow verbatim.
        for (k, v) in carried.into_iter().flatten() {
            if computed(k).is_none() {
                push(&mut out, k, v);
            }
        }
        // The keys every block has, should the order have lacked them.
        for key in ["PATTERN", "HEIGHT", "DESCRIPTION"] {
            if !order_has(key) {
                push(&mut out, key, &computed(key).flatten().unwrap_or_default());
            }
        }
    } else {
        if unicode {
            push(&mut out, "UNICODE", "EXISTS");
        }
        push(&mut out, "PATTERN", &pattern);
        push(&mut out, "HEIGHT", &height);
        push(&mut out, "DESCRIPTION", &description);
        let others: Vec<&(String, String)> = footprint
            .additional_parameters
            .iter()
            .filter(|(k, _)| computed(k).is_none())
            .collect();
        // Altium always writes the two GUID keys, empty for a footprint that
        // belongs to no managed item.
        for key in ["ITEMGUID", "REVISIONGUID"] {
            if !others.iter().any(|(k, _)| k.eq_ignore_ascii_case(key)) {
                push(&mut out, key, "");
            }
        }
        for (k, v) in others {
            push(&mut out, k, v);
        }
        if let Some(twin) = &description_twin {
            push(&mut out, "UNICODE__DESCRIPTION", twin);
        }
        if let Some(twin) = &name_twin {
            push(&mut out, "UNICODE__PATTERN", twin);
        }
        if unicode {
            push(&mut out, "UNICODE", "EXISTS");
        }
    }
    out
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

    // `PRIMITIVEINDEX` is a single global 0-based ordinal over ALL of the
    // footprint's primitives, in the order the `Data` stream stores them — the
    // same ordinal a `PrimitiveGuids` record carries. Every primitive consumes
    // one whether or not it has a record here, so the sequence has to be the
    // one `encode_data_stream` writes, not a per-kind walk.
    for (ordinal, (kind, index)) in footprint.write_sequence().into_iter().enumerate() {
        if let Some(uid) = unique_id_of(footprint, kind, index) {
            encode_unique_id_record(&mut data, ordinal, kind.object_id(), uid);
            has_any_id = true;
        }
    }

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

/// Encodes a footprint's `PrimitiveGuids/Data` stream, or `None` when it has no
/// identities to write.
///
/// The inverse of `reader::parse_primitive_guids`: 24-byte records of
/// `[object_kind: u32][ordinal: u32][guid: 16 bytes]`, the GUID's
/// first three fields little-endian. A malformed GUID string is skipped rather
/// than written as zeroes, which would claim an identity Altium never issued.
///
/// Rebuilt from the primitives themselves: each carries its own `guid`, and
/// its ordinal is its position in [`Footprint::write_sequence`] — so a
/// structural edit (delete a pad, insert a region) moves every identity WITH
/// its primitive instead of re-pointing the survivors. The footprint record's
/// own identity (kind 85) leads, then the primitives in ordinal order; Altium
/// scrambles its record order, but the records are keyed, not positional.
///
/// Nothing is emitted for a from-scratch footprint: it has no identities to
/// preserve, and inventing them would make every save produce different bytes.
pub fn encode_primitive_guids(footprint: &Footprint) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut push = |kind: u32, ordinal: usize, guid: &str| {
        if let Some(bytes) = parse_guid(guid) {
            out.extend_from_slice(&kind.to_le_bytes());
            #[allow(clippy::cast_possible_truncation)] // primitive counts are small
            out.extend_from_slice(&(ordinal as u32).to_le_bytes());
            out.extend_from_slice(&bytes);
        }
    };

    if let Some(guid) = &footprint.guid {
        push(85, 0, guid);
    }
    for (ordinal, (kind, index)) in footprint.write_sequence().into_iter().enumerate() {
        let guid = match kind {
            PrimitiveKind::Arc => &footprint.arcs[index].guid,
            PrimitiveKind::Pad => &footprint.pads[index].guid,
            PrimitiveKind::Via => &footprint.vias[index].guid,
            PrimitiveKind::Track => &footprint.tracks[index].guid,
            PrimitiveKind::Text => &footprint.text[index].guid,
            PrimitiveKind::Region => &footprint.regions[index].guid,
            PrimitiveKind::Fill => &footprint.fills[index].guid,
            PrimitiveKind::ComponentBody => &footprint.component_bodies[index].guid,
        };
        if let Some(guid) = guid {
            push(kind.altium_object_id(), ordinal, guid);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Parses `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` back into its 16 bytes, with
/// the first three fields little-endian.
fn parse_guid(text: &str) -> Option<[u8; 16]> {
    let digits: Vec<u8> = text.bytes().filter(u8::is_ascii_hexdigit).collect();
    if digits.len() != 32 {
        return None;
    }
    let mut hex = [0u8; 16];
    for (slot, pair) in hex.iter_mut().zip(digits.chunks_exact(2)) {
        let text = std::str::from_utf8(pair).ok()?;
        *slot = u8::from_str_radix(text, 16).ok()?;
    }
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&[hex[3], hex[2], hex[1], hex[0]]);
    out[4..6].copy_from_slice(&[hex[5], hex[4]]);
    out[6..8].copy_from_slice(&[hex[7], hex[6]]);
    out[8..16].copy_from_slice(&hex[8..16]);
    Some(out)
}

/// Encodes the per-component `Header` stream.
///
/// # Format
///
/// 4-byte little-endian unsigned integer containing the exact primitive count.
#[allow(clippy::cast_possible_truncation)]
pub fn encode_component_header(footprint: &Footprint) -> Vec<u8> {
    (footprint.primitive_count() as u32).to_le_bytes().to_vec()
}

/// Generates a random GUID as 16 bytes (little-endian UUID format).
fn generate_guid() -> [u8; 16] {
    use uuid::Uuid;
    *Uuid::new_v4().as_bytes()
}

/// Converts a braced GUID string (as read back by the pad parser, e.g.
/// `{A5172B29-…}`) to Altium's on-disk 16-byte Windows little-endian layout —
/// the inverse of the reader's `guid_string_from_bytes`, so a read value
/// re-encodes byte-identically. Returns `None` for an unparseable string (the
/// caller falls back to a fresh GUID, like the `None` field default).
fn guid_bytes_from_string(guid: &str) -> Option<[u8; 16]> {
    use uuid::Uuid;
    let trimmed = guid.trim_start_matches('{').trim_end_matches('}');
    Uuid::parse_str(trimmed).ok().map(|uuid| uuid.to_bytes_le())
}

// =============================================================================
// Per-Component WideStrings Writing
// =============================================================================

/// Whether a text primitive gets a `WideStrings` entry of its own.
///
/// A special string (`.Designator` and friends) is resolved by Altium at draw
/// time and an empty one has nothing to carry, so neither is encoded.
fn carries_wide_string(text: &Text) -> bool {
    !text.text.starts_with('.') && !text.text.is_empty()
}

/// The `WideStrings` entry index for each text primitive, positionally.
///
/// `None` where the text carries no entry. Kept separate from the emit loop so
/// the indices stay tied to `footprint.text`'s own order, which is what
/// [`encode_component_wide_strings`] numbers, however the primitives are
/// interleaved in the `Data` stream.
fn wide_string_indices(footprint: &Footprint) -> Vec<Option<u32>> {
    let mut next = 0u32;
    footprint
        .text
        .iter()
        .map(|text| {
            carries_wide_string(text).then(|| {
                let current = next;
                next += 1;
                current
            })
        })
        .collect()
}

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
    let texts: Vec<&str> = footprint
        .text
        .iter()
        .filter(|t| carries_wide_string(t))
        .map(|t| t.text.as_str())
        .collect();

    // Build the parameter string: `|ENCODEDTEXT0=...|ENCODEDTEXT1=...` — a leading
    // pipe per entry and NO trailing pipe (matching AltiumSharp). With no entries the
    // string is empty, so the stream is just `[01 00 00 00][00]` — AltiumSharp's empty
    // form — rather than the spurious `[02 00 00 00][7C 00]` (leading-pipe) we emitted.
    // Each value is the text's UTF-16 code units in decimal — `10µF` is
    // `49,48,181,70`, `Ω` is `937` — which is what lets the stream carry text
    // the Windows-1252 `Data` block cannot.
    let mut content = String::new();
    for (index, text) in texts.iter().enumerate() {
        let encoded: String = text
            .encode_utf16()
            .map(|unit| unit.to_string())
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

/// The unique id one primitive carries, addressed the way
/// [`Footprint::write_sequence`] names it.
fn unique_id_of(footprint: &Footprint, kind: PrimitiveKind, index: usize) -> Option<&str> {
    match kind {
        PrimitiveKind::Arc => footprint.arcs[index].unique_id.as_deref(),
        PrimitiveKind::Pad => footprint.pads[index].unique_id.as_deref(),
        PrimitiveKind::Via => footprint.vias[index].unique_id.as_deref(),
        PrimitiveKind::Track => footprint.tracks[index].unique_id.as_deref(),
        PrimitiveKind::Text => footprint.text[index].unique_id.as_deref(),
        PrimitiveKind::Region => footprint.regions[index].unique_id.as_deref(),
        PrimitiveKind::Fill => footprint.fills[index].unique_id.as_deref(),
        PrimitiveKind::ComponentBody => footprint.component_bodies[index].unique_id.as_deref(),
    }
}

/// Counts the primitives that carry a unique id, so the header can never
/// disagree with the records [`encode_unique_id_stream`] writes.
fn count_unique_ids(footprint: &Footprint) -> usize {
    footprint
        .write_sequence()
        .into_iter()
        .filter(|&(kind, index)| unique_id_of(footprint, kind, index).is_some())
        .count()
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
    fn pad_is_plated_byte_defaults_to_one() {
        // @60 is an independent bool Altium defaults to 1 for EVERY pad, SMD
        // included (golden fixture + AltiumSharp `IsPlated = true`). The writer
        // It is independent of hole_size; deriving it from that emits 0 for SMD pads.
        let smd = Pad::smd("1", 0.0, 0.0, 1.0, 0.6);
        assert_eq!(encode_pad_geometry(&smd)[60], 1, "SMD pad plated @60");

        let th = Pad::through_hole("2", 0.0, 0.0, 1.6, 1.6, 0.8);
        assert_eq!(encode_pad_geometry(&th)[60], 1, "TH pad plated @60");

        let mut unplated = Pad::through_hole("3", 0.0, 0.0, 1.6, 1.6, 0.8);
        unplated.is_plated = false;
        assert_eq!(encode_pad_geometry(&unplated)[60], 0, "unplated pad @60");
    }

    #[test]
    fn pad_explicit_identity_guids_encode_verbatim() {
        // A read-back identity GUID (braced string) must re-encode to its exact
        // on-disk bytes @126-141/@142-157 (Windows little-endian layout, the
        // inverse of the reader's guid_string_from_bytes). The golden's nil
        // GUIDs must round-trip as zeros, not be regenerated.
        let mut pad = Pad::smd("1", 0.0, 0.0, 1.0, 0.6);
        pad.identity_guid = Some("{A5172B29-10E4-C726-929A-64E441352E67}".to_string());
        pad.identity_guid_b = Some("{00000000-0000-0000-0000-000000000000}".to_string());
        let geom = encode_pad_geometry(&pad);
        assert_eq!(
            &geom[126..142],
            &guid_bytes_from_string("{A5172B29-10E4-C726-929A-64E441352E67}").unwrap(),
            "explicit GUID-A must be replayed verbatim @126"
        );
        assert_eq!(
            &geom[142..158],
            &[0u8; 16],
            "the nil GUID-B must round-trip as zeros @142"
        );

        // `None` keeps the historical fresh-per-pad behaviour: two independent
        // non-zero GUIDs that differ between encodes.
        let fresh = Pad::smd("2", 0.0, 0.0, 1.0, 0.6);
        let g1 = encode_pad_geometry(&fresh);
        let g2 = encode_pad_geometry(&fresh);
        assert_ne!(&g1[126..142], &[0u8; 16], "fresh GUID-A is non-zero");
        assert_ne!(&g1[126..142], &g1[142..158], "GUID-A differs from GUID-B");
        assert_ne!(&g1[126..142], &g2[126..142], "fresh GUIDs are per-encode");
    }

    #[test]
    fn text_comment_designator_flags_encode_at_offsets() {
        use crate::altium::TextJustification;
        // IsComment@40 / IsDesignator@41 overlay the template's 0x00 bytes
        // (offsets verified against AltiumSharp b[40]/b[41]).
        let text = Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 0.0,
            y: 0.0,
            text: "C1".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            is_comment: true,
            is_designator: true,
            font_name: "Arial".to_string(),
            justification: TextJustification::BottomLeft,
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        };
        let geom = encode_text_geometry(&text, None);
        assert_eq!(geom[40], 0x01, "IsComment @40");
        assert_eq!(geom[41], 0x01, "IsDesignator @41");
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
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
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
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::BottomLeft,
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        };
        let geom = encode_text_geometry(&text, None);
        assert_eq!(
            &geom[3..9],
            &[0xFF; 6],
            "from-scratch text must keep the 0xFF net/polygon/component bytes"
        );

        // The inverted (knockout) descriptor stays at the template bytes for a
        // from-scratch plain text: @110/@123 booleans are 0x00, the border/
        // text-offset i32s are 0, and the width/height keep the template's
        // precomputed text-box size (verified against AltiumSharp offsets).
        assert_eq!(geom[110], 0x00, "IsInverted @110 default 0");
        assert_eq!(&geom[111..115], &[0x00; 4], "InvertedBorder @111 default 0");
        assert_eq!(geom[123], 0x00, "UseInvertedRectangle @123 default 0");
        assert_eq!(
            &geom[124..128],
            &TEXT_SR1_TEMPLATE[124..128],
            "InvertedRectWidth @124 keeps the template bytes"
        );
        assert_eq!(
            &geom[128..132],
            &TEXT_SR1_TEMPLATE[128..132],
            "InvertedRectHeight @128 keeps the template bytes"
        );
        assert_eq!(
            &geom[133..137],
            &[0x00; 4],
            "InvertedRectTextOffset @133 default 0"
        );
    }

    #[test]
    fn text_inverted_rect_fields_encode_at_offsets() {
        use crate::altium::TextJustification;
        // A framed inverted text overlays every descriptor field at its offset.
        let text = Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
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
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::BottomLeft,
            is_inverted: true,
            inverted_border: Some(0.0254), // 1 mil = 10000 units
            use_inverted_rectangle: true,
            inverted_rect_width: Some(0.254), // 10 mil = 100000 units
            inverted_rect_height: Some(0.127),
            inverted_rect_text_offset: Some(0.0508),
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        };
        let geom = encode_text_geometry(&text, None);
        assert_eq!(geom[110], 0x01, "IsInverted @110");
        assert_eq!(
            &geom[111..115],
            &from_mm(0.0254).to_le_bytes(),
            "border @111"
        );
        assert_eq!(geom[123], 0x01, "UseInvertedRectangle @123");
        assert_eq!(&geom[124..128], &from_mm(0.254).to_le_bytes(), "width @124");
        assert_eq!(
            &geom[128..132],
            &from_mm(0.127).to_le_bytes(),
            "height @128"
        );
        assert_eq!(
            &geom[133..137],
            &from_mm(0.0508).to_le_bytes(),
            "text offset @133"
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

    /// Every `Layer` variant paired with the Altium layer id it must serialise to.
    ///
    /// The ids are Altium's own numbering, mirrored in the doc comment on each
    /// [`Layer`] variant; this table pins the writer side of that mapping. It is
    /// deliberately exhaustive rather than a sample: #282 was Mechanical 17-32
    /// silently taking a fallback arm in code that already looked finished, and only
    /// a full walk catches that class of bug.
    const ALL_LAYERS: [(Layer, u8); 107] = [
        // Signal: top, mid 1-30, bottom (ids 1-32)
        (Layer::TopLayer, 1),
        (Layer::MidLayer1, 2),
        (Layer::MidLayer2, 3),
        (Layer::MidLayer3, 4),
        (Layer::MidLayer4, 5),
        (Layer::MidLayer5, 6),
        (Layer::MidLayer6, 7),
        (Layer::MidLayer7, 8),
        (Layer::MidLayer8, 9),
        (Layer::MidLayer9, 10),
        (Layer::MidLayer10, 11),
        (Layer::MidLayer11, 12),
        (Layer::MidLayer12, 13),
        (Layer::MidLayer13, 14),
        (Layer::MidLayer14, 15),
        (Layer::MidLayer15, 16),
        (Layer::MidLayer16, 17),
        (Layer::MidLayer17, 18),
        (Layer::MidLayer18, 19),
        (Layer::MidLayer19, 20),
        (Layer::MidLayer20, 21),
        (Layer::MidLayer21, 22),
        (Layer::MidLayer22, 23),
        (Layer::MidLayer23, 24),
        (Layer::MidLayer24, 25),
        (Layer::MidLayer25, 26),
        (Layer::MidLayer26, 27),
        (Layer::MidLayer27, 28),
        (Layer::MidLayer28, 29),
        (Layer::MidLayer29, 30),
        (Layer::MidLayer30, 31),
        (Layer::BottomLayer, 32),
        // Multi-layer (74) - where through-hole pads live
        (Layer::MultiLayer, 74),
        // Silkscreen, solder mask and paste (33-38)
        (Layer::TopOverlay, 33),
        (Layer::BottomOverlay, 34),
        (Layer::TopPaste, 35),
        (Layer::BottomPaste, 36),
        (Layer::TopSolder, 37),
        (Layer::BottomSolder, 38),
        // Internal planes (39-54)
        (Layer::InternalPlane1, 39),
        (Layer::InternalPlane2, 40),
        (Layer::InternalPlane3, 41),
        (Layer::InternalPlane4, 42),
        (Layer::InternalPlane5, 43),
        (Layer::InternalPlane6, 44),
        (Layer::InternalPlane7, 45),
        (Layer::InternalPlane8, 46),
        (Layer::InternalPlane9, 47),
        (Layer::InternalPlane10, 48),
        (Layer::InternalPlane11, 49),
        (Layer::InternalPlane12, 50),
        (Layer::InternalPlane13, 51),
        (Layer::InternalPlane14, 52),
        (Layer::InternalPlane15, 53),
        (Layer::InternalPlane16, 54),
        // Drill and keep-out (55, 56, 73)
        (Layer::DrillGuide, 55),
        (Layer::KeepOut, 56),
        (Layer::DrillDrawing, 73),
        // Named component layers - aliases of Mechanical 2-7 (58-63)
        (Layer::TopAssembly, 58),
        (Layer::BottomAssembly, 59),
        (Layer::TopCourtyard, 60),
        (Layer::BottomCourtyard, 61),
        (Layer::Top3DBody, 62),
        (Layer::Bottom3DBody, 63),
        // Mechanical 1-16 (57-72)
        (Layer::Mechanical1, 57),
        (Layer::Mechanical2, 58),
        (Layer::Mechanical3, 59),
        (Layer::Mechanical4, 60),
        (Layer::Mechanical5, 61),
        (Layer::Mechanical6, 62),
        (Layer::Mechanical7, 63),
        (Layer::Mechanical8, 64),
        (Layer::Mechanical9, 65),
        (Layer::Mechanical10, 66),
        (Layer::Mechanical11, 67),
        (Layer::Mechanical12, 68),
        (Layer::Mechanical13, 69),
        (Layer::Mechanical14, 70),
        (Layer::Mechanical15, 71),
        (Layer::Mechanical16, 72),
        // Mechanical 17-32 (186-201, Altium Designer 18+) - the #282 range
        (Layer::Mechanical17, 186),
        (Layer::Mechanical18, 187),
        (Layer::Mechanical19, 188),
        (Layer::Mechanical20, 189),
        (Layer::Mechanical21, 190),
        (Layer::Mechanical22, 191),
        (Layer::Mechanical23, 192),
        (Layer::Mechanical24, 193),
        (Layer::Mechanical25, 194),
        (Layer::Mechanical26, 195),
        (Layer::Mechanical27, 196),
        (Layer::Mechanical28, 197),
        (Layer::Mechanical29, 198),
        (Layer::Mechanical30, 199),
        (Layer::Mechanical31, 200),
        (Layer::Mechanical32, 201),
        // System and UI layers (75-85) - never carry library primitives
        (Layer::ConnectLayer, 75),
        (Layer::BackgroundLayer, 76),
        (Layer::DRCErrorLayer, 77),
        (Layer::HighlightLayer, 78),
        (Layer::GridColor1, 79),
        (Layer::GridColor10, 80),
        (Layer::PadHoleLayer, 81),
        (Layer::ViaHoleLayer, 82),
        (Layer::TopPadMaster, 83),
        (Layer::BottomPadMaster, 84),
        (Layer::DRCDetailLayer, 85),
    ];

    /// The v7 catch-all id. Only [`EXPECTED_V7_FALLBACK`] may legitimately reach it.
    const V7_FALLBACK: u32 = 0x0103_000F;

    /// The only layers allowed to serialise to [`V7_FALLBACK`]: `MultiLayer`, whose
    /// fallback *is* its encoding, and the system/UI layers, which never carry a
    /// library primitive. Any other layer landing here has silently lost its
    /// identity on write.
    const EXPECTED_V7_FALLBACK: [Layer; 12] = [
        Layer::MultiLayer,
        Layer::ConnectLayer,
        Layer::BackgroundLayer,
        Layer::DRCErrorLayer,
        Layer::HighlightLayer,
        Layer::GridColor1,
        Layer::GridColor10,
        Layer::PadHoleLayer,
        Layer::ViaHoleLayer,
        Layer::TopPadMaster,
        Layer::BottomPadMaster,
        Layer::DRCDetailLayer,
    ];

    #[test]
    fn layer_to_id_maps_every_variant() {
        for (layer, want) in ALL_LAYERS {
            assert_eq!(
                layer_to_id(layer),
                want,
                "{layer:?} must serialise to Altium layer id {want}"
            );
        }
    }

    /// Compile-time completeness guard for [`ALL_LAYERS`].
    ///
    /// A new `Layer` variant makes this match non-exhaustive, so the build fails
    /// here rather than the variant silently never being exercised — which is
    /// exactly how Mechanical 17-32 went unnoticed in #282.
    #[test]
    #[allow(clippy::too_many_lines)] // One arm per Layer variant is the point
    fn every_layer_variant_is_listed_in_all_layers() {
        for (layer, _) in ALL_LAYERS {
            match layer {
                Layer::TopLayer
                | Layer::MidLayer1
                | Layer::MidLayer2
                | Layer::MidLayer3
                | Layer::MidLayer4
                | Layer::MidLayer5
                | Layer::MidLayer6
                | Layer::MidLayer7
                | Layer::MidLayer8
                | Layer::MidLayer9
                | Layer::MidLayer10
                | Layer::MidLayer11
                | Layer::MidLayer12
                | Layer::MidLayer13
                | Layer::MidLayer14
                | Layer::MidLayer15
                | Layer::MidLayer16
                | Layer::MidLayer17
                | Layer::MidLayer18
                | Layer::MidLayer19
                | Layer::MidLayer20
                | Layer::MidLayer21
                | Layer::MidLayer22
                | Layer::MidLayer23
                | Layer::MidLayer24
                | Layer::MidLayer25
                | Layer::MidLayer26
                | Layer::MidLayer27
                | Layer::MidLayer28
                | Layer::MidLayer29
                | Layer::MidLayer30
                | Layer::BottomLayer
                | Layer::MultiLayer
                | Layer::TopOverlay
                | Layer::BottomOverlay
                | Layer::TopSolder
                | Layer::BottomSolder
                | Layer::TopPaste
                | Layer::BottomPaste
                | Layer::InternalPlane1
                | Layer::InternalPlane2
                | Layer::InternalPlane3
                | Layer::InternalPlane4
                | Layer::InternalPlane5
                | Layer::InternalPlane6
                | Layer::InternalPlane7
                | Layer::InternalPlane8
                | Layer::InternalPlane9
                | Layer::InternalPlane10
                | Layer::InternalPlane11
                | Layer::InternalPlane12
                | Layer::InternalPlane13
                | Layer::InternalPlane14
                | Layer::InternalPlane15
                | Layer::InternalPlane16
                | Layer::DrillGuide
                | Layer::DrillDrawing
                | Layer::KeepOut
                | Layer::TopAssembly
                | Layer::BottomAssembly
                | Layer::TopCourtyard
                | Layer::BottomCourtyard
                | Layer::Top3DBody
                | Layer::Bottom3DBody
                | Layer::Mechanical1
                | Layer::Mechanical2
                | Layer::Mechanical3
                | Layer::Mechanical4
                | Layer::Mechanical5
                | Layer::Mechanical6
                | Layer::Mechanical7
                | Layer::Mechanical8
                | Layer::Mechanical9
                | Layer::Mechanical10
                | Layer::Mechanical11
                | Layer::Mechanical12
                | Layer::Mechanical13
                | Layer::Mechanical14
                | Layer::Mechanical15
                | Layer::Mechanical16
                | Layer::Mechanical17
                | Layer::Mechanical18
                | Layer::Mechanical19
                | Layer::Mechanical20
                | Layer::Mechanical21
                | Layer::Mechanical22
                | Layer::Mechanical23
                | Layer::Mechanical24
                | Layer::Mechanical25
                | Layer::Mechanical26
                | Layer::Mechanical27
                | Layer::Mechanical28
                | Layer::Mechanical29
                | Layer::Mechanical30
                | Layer::Mechanical31
                | Layer::Mechanical32
                | Layer::ConnectLayer
                | Layer::BackgroundLayer
                | Layer::DRCErrorLayer
                | Layer::HighlightLayer
                | Layer::GridColor1
                | Layer::GridColor10
                | Layer::PadHoleLayer
                | Layer::ViaHoleLayer
                | Layer::TopPadMaster
                | Layer::BottomPadMaster
                | Layer::DRCDetailLayer => {}
            }
        }
    }

    #[test]
    fn all_layers_table_lists_each_variant_once() {
        // Catches a copy-paste duplicate masking a missing variant: the table would
        // still have 107 rows and the exhaustiveness guard would still compile.
        let mut seen = std::collections::HashSet::new();
        for (layer, _) in ALL_LAYERS {
            assert!(
                seen.insert(format!("{layer:?}")),
                "{layer:?} appears twice in ALL_LAYERS"
            );
        }
        assert_eq!(seen.len(), ALL_LAYERS.len());
    }

    #[test]
    fn v7_layer_id_falls_back_only_for_system_layers() {
        // The #282 failure mode was a real layer reaching the `_` arm and being
        // written as multi-layer. Pinning the exact fallback set means any future
        // layer that starts falling through fails here.
        for (layer, id) in ALL_LAYERS {
            let allowed = EXPECTED_V7_FALLBACK.contains(&layer);
            assert_eq!(
                v7_layer_id(id) == V7_FALLBACK,
                allowed,
                "{layer:?} (id {id}) must {} the v7 fallback",
                if allowed { "use" } else { "not use" }
            );
        }
    }

    #[test]
    fn v7_layer_ids_collide_only_between_documented_aliases() {
        // Two layers may share a v7 id only when they share an Altium layer id -
        // i.e. they are the documented alias pairs (TopAssembly/Mechanical2, ...).
        // Any other collision is two distinct layers writing as the same layer.
        let mut seen: std::collections::HashMap<u32, (Layer, u8)> =
            std::collections::HashMap::new();
        for (layer, id) in ALL_LAYERS {
            let v7 = v7_layer_id(id);
            if v7 == V7_FALLBACK {
                continue; // the system layers legitimately share the catch-all
            }
            if let Some((prev_layer, prev_id)) = seen.insert(v7, (layer, id)) {
                assert_eq!(
                    prev_id, id,
                    "{layer:?} and {prev_layer:?} share v7 id {v7:#010X} \
                     without sharing an Altium layer id"
                );
            }
        }
    }

    #[test]
    fn pad_shape_ids_match_altium_numbering() {
        // Altium `PcbPad` shape ids. `Oval` is deliberately not its own id: Altium
        // draws an oblong as a Round pad with unequal X/Y sizes.
        for (shape, want) in [
            (PadShape::Round, 1),
            (PadShape::Oval, 1),
            (PadShape::Rectangle, 2),
            (PadShape::Octagonal, 3),
            (PadShape::RoundedRectangle, 9),
        ] {
            assert_eq!(
                pad_shape_to_id(shape),
                want,
                "{shape:?} must encode as {want}"
            );
        }
    }

    #[test]
    fn hole_shape_ids_match_altium_numbering() {
        for (shape, want) in [
            (HoleShape::Round, 0),
            (HoleShape::Square, 1),
            (HoleShape::Slot, 2),
        ] {
            assert_eq!(
                hole_shape_to_id(shape),
                want,
                "{shape:?} must encode as {want}"
            );
        }
    }

    #[test]
    fn stack_mode_ids_match_altium_numbering() {
        // Pad and via stack modes share Altium's ordinal encoding.
        for (mode, want) in [
            (PadStackMode::Simple, 0),
            (PadStackMode::TopMiddleBottom, 1),
            (PadStackMode::FullStack, 2),
        ] {
            assert_eq!(
                pad_stack_mode_to_id(mode),
                want,
                "{mode:?} must encode as {want}"
            );
        }
        for (mode, want) in [
            (ViaStackMode::Simple, 0),
            (ViaStackMode::TopMiddleBottom, 1),
            (ViaStackMode::FullStack, 2),
        ] {
            assert_eq!(
                via_stack_mode_to_id(mode),
                want,
                "{mode:?} must encode as {want}"
            );
        }
    }

    #[test]
    fn text_kind_and_stroke_font_ids_match_altium_numbering() {
        for (kind, want) in [
            (TextKind::Stroke, 0),
            (TextKind::TrueType, 1),
            (TextKind::BarCode, 2),
        ] {
            assert_eq!(
                text_kind_to_id(kind),
                want,
                "{kind:?} must encode as {want}"
            );
        }
        // Stroke font ids are 1-based: Altium's default stroke font is index 1.
        for (font, want) in [
            (StrokeFont::Default, 1),
            (StrokeFont::SansSerif, 2),
            (StrokeFont::Serif, 3),
        ] {
            assert_eq!(
                stroke_font_to_id(font),
                want,
                "{font:?} must encode as {want}"
            );
        }
    }

    #[test]
    fn pcb_justification_ids_are_column_major() {
        // Altium numbers the 3x3 anchor grid down each column, 1-based:
        // LeftTop=1..LeftBottom=3, CenterTop=4..CenterBottom=6, RightTop=7..=9.
        for (justification, want) in [
            (TextJustification::TopLeft, 1),
            (TextJustification::MiddleLeft, 2),
            (TextJustification::BottomLeft, 3),
            (TextJustification::TopCenter, 4),
            (TextJustification::MiddleCenter, 5),
            (TextJustification::BottomCenter, 6),
            (TextJustification::TopRight, 7),
            (TextJustification::MiddleRight, 8),
            (TextJustification::BottomRight, 9),
        ] {
            assert_eq!(
                pcb_justification_to_id(justification),
                want,
                "{justification:?} must encode as {want}"
            );
        }
        // The from-scratch default must stay byte-identical to the geometry
        // template's justification byte at offset 132.
        assert_eq!(
            pcb_justification_to_id(TextJustification::BottomLeft),
            0x03,
            "the default anchor must match the template byte"
        );
    }

    #[test]
    fn v7_layer_id_handles_extended_mechanical_layers() {
        // Regression for #282. Byte IDs 186-201 are Mechanical 17-32 (Altium
        // Designer 18+). Without an explicit arm they fall through to `_` and are
        // written as the multi-layer fallback, so a pad/track/arc/text/fill on
        // M17-M32 silently lost its layer.
        for (layer, want) in [
            (Layer::Mechanical17, 0x0102_0011_u32),
            (Layer::Mechanical18, 0x0102_0012),
            (Layer::Mechanical20, 0x0102_0014),
            (Layer::Mechanical22, 0x0102_0016),
            (Layer::Mechanical28, 0x0102_001C),
            (Layer::Mechanical32, 0x0102_0020),
        ] {
            assert_eq!(
                v7_layer_id(layer_to_id(layer)),
                want,
                "{layer:?} must serialize its own V7 id"
            );
        }

        // The whole extended range is contiguous with M1-M16 and collision-free:
        // mechanical N (1..=32) is always 0x0102_0000 + N.
        let mut seen = std::collections::HashSet::new();
        for (n, layer) in [
            Layer::Mechanical1,
            Layer::Mechanical2,
            Layer::Mechanical3,
            Layer::Mechanical4,
            Layer::Mechanical5,
            Layer::Mechanical6,
            Layer::Mechanical7,
            Layer::Mechanical8,
            Layer::Mechanical9,
            Layer::Mechanical10,
            Layer::Mechanical11,
            Layer::Mechanical12,
            Layer::Mechanical13,
            Layer::Mechanical14,
            Layer::Mechanical15,
            Layer::Mechanical16,
            Layer::Mechanical17,
            Layer::Mechanical18,
            Layer::Mechanical19,
            Layer::Mechanical20,
            Layer::Mechanical21,
            Layer::Mechanical22,
            Layer::Mechanical23,
            Layer::Mechanical24,
            Layer::Mechanical25,
            Layer::Mechanical26,
            Layer::Mechanical27,
            Layer::Mechanical28,
            Layer::Mechanical29,
            Layer::Mechanical30,
            Layer::Mechanical31,
            Layer::Mechanical32,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, l)| (u32::from(u8::try_from(i).unwrap()) + 1, l))
        {
            let id = v7_layer_id(layer_to_id(layer));
            assert_eq!(id, 0x0102_0000 + n, "mechanical {n} V7 id");
            assert!(seen.insert(id), "duplicate V7 id for mechanical {n}");
            // ...and never the multi-layer fallback that caused the bug.
            assert_ne!(id, v7_layer_id(74), "mechanical {n} must not fall back");
        }

        // The numeric id and the string token must agree on the mechanical index,
        // since regions/bodies use the token while pads/tracks use the id.
        for layer in [
            Layer::Mechanical17,
            Layer::Mechanical32,
            Layer::TopCourtyard,
        ] {
            let n = v7_layer_id(layer_to_id(layer)) - 0x0102_0000;
            assert_eq!(
                v7_layer_token(layer),
                format!("MECHANICAL{n}"),
                "{layer:?}: id and token must reference the same mechanical layer"
            );
        }
    }

    #[test]
    fn test_v7_layer_token() {
        use crate::altium::pcblib::primitives::Vertex;
        // Component-pair / mechanical layers must use the MECHANICAL{n} token,
        // not the display name (which Altium can't resolve -> falls back to Top Layer).
        assert_eq!(v7_layer_token(Layer::TopCourtyard), "MECHANICAL4");
        assert_eq!(v7_layer_token(Layer::TopAssembly), "MECHANICAL2");
        assert_eq!(v7_layer_token(Layer::Mechanical1), "MECHANICAL1");
        assert_eq!(v7_layer_token(Layer::Mechanical17), "MECHANICAL17");
        assert_eq!(v7_layer_token(Layer::Mechanical32), "MECHANICAL32");
        // The rest of the vocabulary is equally its own: the token is not the
        // display name with the spaces taken out. `TOPLAYER` and `BOTTOMLAYER`
        // resolve to nothing, and an unresolved token leaves the primitive on
        // Top Layer — so the bottom-side case is a silent side swap.
        assert_eq!(v7_layer_token(Layer::TopLayer), "TOP");
        assert_eq!(v7_layer_token(Layer::BottomLayer), "BOTTOM");
        assert_eq!(v7_layer_token(Layer::MidLayer1), "MID1");
        assert_eq!(v7_layer_token(Layer::MidLayer30), "MID30");
        assert_eq!(v7_layer_token(Layer::InternalPlane3), "PLANE3");
        assert_eq!(v7_layer_token(Layer::KeepOut), "KEEPOUT");
        assert_eq!(v7_layer_token(Layer::MultiLayer), "MULTILAYER");
        assert_eq!(v7_layer_token(Layer::DrillDrawing), "DRILLDRAWING");
        assert_eq!(v7_layer_token(Layer::DrillGuide), "DRILLGUIDE");
        // These two do keep their display spelling, because that is what the
        // vocabulary happens to use.
        assert_eq!(v7_layer_token(Layer::TopOverlay), "TOPOVERLAY");
        assert_eq!(v7_layer_token(Layer::BottomSolder), "BOTTOMSOLDER");
        // Every layer has a token, in the vocabulary's shape — including the
        // ones it has no word for, which take their display name squeezed.
        for layer in Layer::ALL {
            let token = v7_layer_token(layer);
            assert!(!token.is_empty(), "{layer:?}");
            assert_eq!(
                token,
                token.replace(' ', "").to_uppercase(),
                "{layer:?}: {token}"
            );
        }

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
            TEXTURESIZEX=0mil|TEXTURESIZEY=0mil|TEXTUREROTATION= 0.00000000000000E+0000|\
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
        // Generic extruded body: no model name, not embedded. The golden's own
        // extruded bodies (BODY3D, PRIMPROPS) end at TEXTUREROTATION with NO
        // MODEL keys at all — the extrusion derives from STANDOFFHEIGHT /
        // OVERALLHEIGHT, and inventing a MODELID meant a fresh GUID per save.
        let mut extruded = ComponentBody::new("", "");
        extruded.embedded = false;
        extruded.overall_height = 1.0;
        extruded.standoff_height = 0.0;
        let s = build_component_body_params(&extruded);
        assert!(s.contains("ISSHAPEBASED=FALSE"), "got: {s}");
        assert!(
            s.ends_with("TEXTUREROTATION= 0.00000000000000E+0000"),
            "got: {s}"
        );
        assert!(
            !s.contains("MODEL"),
            "no MODEL keys on an extruded body: {s}"
        );

        // Model-backed body (STEP): exactly the EMBSTEP golden's group.
        let mut model = ComponentBody::new("{GUID}", "part.step");
        model.embedded = true;
        let s = build_component_body_params(&model);
        assert!(s.contains("ISSHAPEBASED=FALSE"), "got: {s}");
        assert!(s.contains("MODELID={GUID}"), "got: {s}");
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
            TEXTURESIZEX=0mil|TEXTURESIZEY=0mil|TEXTUREROTATION= 0.00000000000000E+0000|\
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

    /// #391: a header layer byte `layer_from_id` does not map decodes to the
    /// `MultiLayer` catch-all but must round-trip verbatim, together with its
    /// `V7_LAYER` token — the pair is the body's one-byte replay base.
    #[test]
    fn component_body_unmapped_layer_byte_roundtrips_verbatim() {
        use super::super::reader;

        let mut original = Footprint::new("RT_BODY_RAWLAYER");
        let mut body = ComponentBody::new("{G-1234}", "p.step");
        body.layer = Layer::MultiLayer;
        body.raw_layer_id = Some(150); // unmapped: 86-185 has no Layer variant
        body.v7_layer = Some("MECHANICAL22".to_string());
        original.add_component_body(body);

        let data = encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("RT_BODY_RAWLAYER");
        reader::parse_data_stream(&mut decoded, &data, None);

        let b = &decoded.component_bodies[0];
        assert_eq!(b.layer, Layer::MultiLayer, "catch-all decode is unchanged");
        assert_eq!(b.raw_layer_id, Some(150), "the authored byte survives");
        assert_eq!(
            b.v7_layer.as_deref(),
            Some("MECHANICAL22"),
            "the authored token survives"
        );

        // And the second write re-emits the identical pair.
        let data2 = encode_data_stream(&decoded).expect("re-encode");
        assert_eq!(data, data2, "unmapped-layer body is byte-stable");
    }

    /// A STEP reference the library does not embed is stored by Altium with
    /// an EMPTY `MODELID` and the full `MODEL.*` group (`test_0805.step` in a
    /// UI-authored library); the group is keyed on the file name, so the
    /// reference survives a write and a read.
    #[test]
    fn external_model_reference_keeps_its_model_group_under_an_empty_model_id() {
        use super::super::reader;

        let mut body = ComponentBody::new("", "models/test_0805.step");
        body.embedded = false;
        let params = build_component_body_params(&body);
        assert!(
            params.contains(
                "|MODELID=|MODEL.CHECKSUM=0|MODEL.EMBED=FALSE|MODEL.NAME=models/test_0805.step|"
            ),
            "{params}"
        );
        assert!(
            params.ends_with("|MODEL.MODELTYPE=1|MODEL.MODELSOURCE=Undefined"),
            "{params}"
        );

        let mut fp = Footprint::new("EXT");
        fp.add_component_body(body);
        let data = encode_data_stream(&fp).expect("encode");
        let mut decoded = Footprint::new("EXT");
        reader::parse_data_stream(&mut decoded, &data, None);
        let back = &decoded.component_bodies[0];
        assert_eq!(back.model_name, "models/test_0805.step");
        assert!(!back.embedded && back.model_id.is_empty());
        assert_eq!(encode_data_stream(&decoded).unwrap(), data, "byte-stable");

        // An extruded body — no file, not embedded — still carries no group.
        let mut extruded = ComponentBody::new("", "");
        extruded.embedded = false;
        assert!(!build_component_body_params(&extruded).contains("MODELID"));
    }

    /// `TEXTUREROTATION` is carried verbatim (a UI-authored terminal block
    /// rotates its texture by 90°); a from-scratch body emits the zero form.
    #[test]
    fn texture_rotation_round_trips_verbatim() {
        let mut body = ComponentBody::new("", "");
        assert!(
            build_component_body_params(&body).contains("|TEXTUREROTATION= 0.00000000000000E+0000")
        );
        body.texture_rotation = Some(" 9.00000000000000E+0001".to_string());
        assert!(
            build_component_body_params(&body).contains("|TEXTUREROTATION= 9.00000000000000E+0001")
        );
    }

    /// A body read from a file replays its key order — an unmodelled key
    /// goes back between the canonical ones it sat between, and a canonical
    /// key Altium writes twice (ARCRESOLUTION) keeps both positions.
    #[test]
    fn body_params_replay_the_read_key_order() {
        let mut body = ComponentBody::new("", "");
        body.additional_parameters = vec![("BODYOVERRIDECOLOR".to_string(), "TRUE".to_string())];
        let canonical = build_component_body_params(&body);
        let appended_last = canonical.ends_with("|BODYOVERRIDECOLOR=TRUE");
        assert!(
            appended_last,
            "without an order the unmodelled key is appended: {canonical}"
        );

        // The order Altium wrote: the override colour right after the opacity.
        let mut order: Vec<String> = canonical
            .split('|')
            .filter_map(|kv| kv.split_once('=').map(|(k, _)| k.to_string()))
            .filter(|k| k != "BODYOVERRIDECOLOR")
            .collect();
        let at = order.iter().position(|k| k == "BODYOPACITY3D").unwrap() + 1;
        order.insert(at, "BODYOVERRIDECOLOR".to_string());
        assert_eq!(order.iter().filter(|k| *k == "ARCRESOLUTION").count(), 2);
        body.param_key_order = order;

        let replayed = build_component_body_params(&body);
        assert!(
            replayed.contains("|BODYOPACITY3D=1.000|BODYOVERRIDECOLOR=TRUE|IDENTIFIER="),
            "{replayed}"
        );
        assert_eq!(replayed.matches("ARCRESOLUTION=").count(), 2, "{replayed}");
        assert_eq!(
            replayed.len(),
            canonical.len(),
            "same tokens, different order"
        );

        // A canonical key the order lacks (a typed edit) is still emitted.
        body.param_key_order.retain(|k| k != "BODYCOLOR3D");
        assert!(build_component_body_params(&body).contains("|BODYCOLOR3D="));
    }

    /// Retargeting such a body to a real layer discards the stale pair and
    /// emits the canonical byte + token for the new layer.
    #[test]
    fn component_body_retarget_discards_stale_raw_layer_pair() {
        use super::super::reader;

        let mut original = Footprint::new("RT_BODY_RETARGET");
        let mut body = ComponentBody::new("{G-1234}", "p.step");
        body.layer = Layer::Mechanical13; // retargeted away from the catch-all
        body.raw_layer_id = Some(150); // stale echo from a previous read
        body.v7_layer = Some("MECHANICAL22".to_string());
        original.add_component_body(body);

        let data = encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("RT_BODY_RETARGET");
        reader::parse_data_stream(&mut decoded, &data, None);

        let b = &decoded.component_bodies[0];
        assert_eq!(b.layer, Layer::Mechanical13, "typed layer wins");
        assert_eq!(b.raw_layer_id, None, "stale byte was not written");
        assert_eq!(b.v7_layer, None, "canonical token was written");
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
        // 42-45, rather than a blanket [0x00; 13] that would leave it zeroed.
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
        // `[01 00 00 00][00]`, not a spurious leading-pipe `[02 00 00 00][7C 00]`.
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
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
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
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::MiddleCenter,
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
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

    // ==================== V7_LAYER vocabulary ================================

    #[test]
    fn every_layer_maps_to_a_v7_token_and_no_two_share_one() {
        // Altium resolves a Region's or ComponentBody's layer from this token,
        // not from the common-header byte, and a token it cannot resolve leaves
        // the primitive on Top Layer. So a wrong or duplicated entry does not
        // fail the save — it silently moves copper to the other side of the
        // board. The vocabulary is a fixed one from Advpcb.dll, and stripping
        // spaces out of the display name is exactly the mistake that produces
        // the unresolvable "BOTTOMLAYER".
        use std::collections::HashSet;

        // The named vocabulary, spot-checked against the golden.
        let vocabulary = [
            (Layer::TopLayer, "TOP"),
            (Layer::BottomLayer, "BOTTOM"),
            (Layer::MidLayer1, "MID1"),
            (Layer::MidLayer30, "MID30"),
            (Layer::TopOverlay, "TOPOVERLAY"),
            (Layer::BottomOverlay, "BOTTOMOVERLAY"),
            (Layer::TopPaste, "TOPPASTE"),
            (Layer::BottomPaste, "BOTTOMPASTE"),
            (Layer::TopSolder, "TOPSOLDER"),
            (Layer::BottomSolder, "BOTTOMSOLDER"),
            (Layer::InternalPlane1, "PLANE1"),
            (Layer::InternalPlane16, "PLANE16"),
            (Layer::DrillGuide, "DRILLGUIDE"),
            (Layer::KeepOut, "KEEPOUT"),
            (Layer::Mechanical1, "MECHANICAL1"),
            (Layer::Mechanical16, "MECHANICAL16"),
            (Layer::DrillDrawing, "DRILLDRAWING"),
            (Layer::MultiLayer, "MULTILAYER"),
            (Layer::ConnectLayer, "CONNECT"),
            (Layer::BackgroundLayer, "BACKGROUND"),
            (Layer::DRCErrorLayer, "DRCERRORS"),
            (Layer::HighlightLayer, "SELECTIONS"),
            (Layer::GridColor1, "VISIBLEGRID1X"),
            (Layer::GridColor10, "VISIBLEGRID10X"),
            (Layer::PadHoleLayer, "PADHOLES"),
            (Layer::ViaHoleLayer, "VIAHOLES"),
            // The extended mechanical range keeps counting from 17, not from 1.
            (Layer::Mechanical17, "MECHANICAL17"),
            (Layer::Mechanical32, "MECHANICAL32"),
        ];
        for (layer, token) in vocabulary {
            assert_eq!(v7_layer_token(layer), token, "{layer:?}");
        }

        // Two layers sharing a token would merge on read-back, so within the
        // vocabulary every token has to be its own.
        let distinct: HashSet<&str> = vocabulary.iter().map(|&(_, token)| token).collect();
        assert_eq!(
            distinct.len(),
            vocabulary.len(),
            "two layers share a V7_LAYER token"
        );

        // The component-layer aliases deliberately do share their mechanical
        // layer's token, because that is the layer Altium stores them on.
        assert_eq!(v7_layer_token(Layer::TopAssembly), "MECHANICAL2");
        assert_eq!(v7_layer_token(Layer::Top3DBody), "MECHANICAL6");
    }

    #[test]
    fn a_full_stack_via_writes_its_per_layer_diameters() {
        // A stacked via carries one diameter per layer. Falling back to the
        // simple diameter for a layer the caller supplied would quietly resize
        // the barrel on that layer.
        let mut via = Via::new(0.0, 0.0, 0.6, 0.3);
        via.diameter_stack_mode = ViaStackMode::FullStack;
        via.per_layer_diameters = Some(vec![0.9; 32]);

        let mut stacked = Vec::new();
        encode_via(&mut stacked, &via);

        let mut simple = Via::new(0.0, 0.0, 0.6, 0.3);
        simple.diameter_stack_mode = ViaStackMode::Simple;
        let mut plain = Vec::new();
        encode_via(&mut plain, &simple);

        assert_ne!(
            stacked, plain,
            "the per-layer diameters left no trace in the record"
        );

        // A short list falls back to the primary diameter for the layers it
        // does not cover, rather than writing a zero-width barrel.
        let mut partial = Via::new(0.0, 0.0, 0.6, 0.3);
        partial.diameter_stack_mode = ViaStackMode::FullStack;
        partial.per_layer_diameters = Some(vec![0.9; 4]);
        let mut short = Vec::new();
        encode_via(&mut short, &partial);
        assert_eq!(short.len(), stacked.len());
    }

    /// A flag word with bits nothing models — a hand-authored pin header's
    /// tracks carry `0x001C` — comes back exactly, and so does every word
    /// Altium can write: all sixteen bits, modelled or not.
    #[test]
    fn unmodelled_flag_bits_round_trip_verbatim() {
        use crate::altium::pcblib::reader::read_flags_for_test;
        assert_eq!(encode_altium_flags(read_flags_for_test(0x001C)), 0x001C);
        // Every saved word (bit 3 set) whose unlocked/test-point combination
        // Altium itself writes round-trips bit for bit.
        for word in 0u16..=0xFFFF {
            let saved = word & ALT_FLAG_SAVED != 0;
            let testpoint = word & (ALT_FLAG_TESTPOINT_TOP | ALT_FLAG_TESTPOINT_BOTTOM) != 0;
            let unlocked = word & ALT_FLAG_UNLOCKED != 0;
            if saved && !(testpoint && unlocked) {
                assert_eq!(
                    encode_altium_flags(read_flags_for_test(word)),
                    word,
                    "{word:#06x}"
                );
            }
        }
    }

    /// On disk a rounded rectangle is shape id 1 plus a radius in 1..=99, so
    /// a per-layer round, rectangular or octagonal land must carry radius 0
    /// whatever the pad's own corner radius is — the alternative reads back
    /// as a rounded rectangle on every such layer.
    #[test]
    fn per_layer_corner_radius_follows_the_layer_shape() {
        let mut pad = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        pad.shape = PadShape::RoundedRectangle;
        pad.corner_radius_percent = Some(30);
        pad.stack_mode = PadStackMode::FullStack;
        let mut shapes = vec![PadShape::Round; 32];
        shapes[1] = PadShape::Rectangle;
        shapes[2] = PadShape::Octagonal;
        shapes[3] = PadShape::RoundedRectangle;
        pad.per_layer_shapes = Some(shapes);

        let block = encode_pad_per_layer_data(&pad);
        let (ids, radii) = (&block[256..288], &block[288..320]);
        assert_eq!(&ids[..4], &[1, 2, 3, 1]);
        assert_eq!(
            &radii[..4],
            &[0, 0, 0, 30],
            "only the rounded layer carries the radius"
        );

        // Without a pad radius the rounded layer gets the 50% default; an
        // explicit per-layer table wins over both.
        pad.corner_radius_percent = None;
        assert_eq!(encode_pad_per_layer_data(&pad)[288 + 3], 50);
        pad.per_layer_corner_radii = Some(vec![7; 32]);
        assert_eq!(&encode_pad_per_layer_data(&pad)[288..292], &[7, 7, 7, 7]);
    }

    #[test]
    fn a_via_block_longer_than_the_template_keeps_its_length() {
        // An older library stores 351-byte vias: thirty bytes past the
        // 321-byte template, unmodelled here, that a rewrite must not cut off.
        let mut raw = vec![0_u8; 351];
        raw[..VIA_SR1_TEMPLATE.len()].copy_from_slice(&VIA_SR1_TEMPLATE);
        for (i, byte) in raw[VIA_SR1_TEMPLATE.len()..].iter_mut().enumerate() {
            *byte = u8::try_from(0xA0 + i).expect("fits");
        }
        let mut via = Via::new(0.0, 0.0, 0.6, 0.3);
        via.raw_block = Some(raw.clone());

        let mut data = Vec::new();
        encode_via(&mut data, &via);
        let len = u32::from_le_bytes(data[..4].try_into().expect("prefix")) as usize;
        assert_eq!(len, raw.len(), "the read length is the written length");
        assert_eq!(
            &data[4 + VIA_SR1_TEMPLATE.len()..],
            &raw[VIA_SR1_TEMPLATE.len()..],
            "the bytes past the template go back verbatim"
        );

        // A block shorter than the template cannot take the overlays: the
        // template is the base instead.
        via.raw_block = Some(raw[..300].to_vec());
        let mut data = Vec::new();
        encode_via(&mut data, &via);
        let len = u32::from_le_bytes(data[..4].try_into().expect("prefix")) as usize;
        assert_eq!(len, VIA_SR1_TEMPLATE.len());
    }

    #[test]
    fn a_mechanical_layer_past_sixteen_is_stored_the_way_altium_stores_it() {
        // Altium keeps a Mechanical 20 track under legacy byte 72 with the
        // real layer in the V7 id at @41; a track placed there from scratch
        // gets the same pair, and so does one read from such a file.
        let mut track = Track::new(0.0, 0.0, 1.0, 0.0, 0.2, Layer::Mechanical20);
        let mut data = Vec::new();
        encode_track(&mut data, &track);
        assert_eq!(data[4], 72, "the legacy byte Altium writes");
        assert_eq!(&data[4 + 41..4 + 45], &0x0102_0014_u32.to_le_bytes());
        for layer in [
            Layer::Mechanical17,
            Layer::Mechanical32,
            Layer::Mechanical16,
        ] {
            assert_eq!(disk_layer_byte(layer), 72, "{layer:?}");
        }
        assert_eq!(disk_layer_byte(Layer::Mechanical15), 71);
        assert_eq!(disk_layer_byte(Layer::TopOverlay), 33);

        // A byte outside every documented range reads as Multi-Layer and, the
        // primitive unmoved, goes back as the byte it was rather than as 74;
        // moved to a layer the model can name, it gets that layer's byte.
        track.layer = Layer::MultiLayer;
        track.raw_layer_id = Some(100);
        let mut data = Vec::new();
        encode_track(&mut data, &track);
        assert_eq!(data[4], 100);
        track.layer = Layer::TopOverlay;
        let mut data = Vec::new();
        encode_track(&mut data, &track);
        assert_eq!(data[4], 33);
        assert_eq!(layer_byte(None, Layer::MultiLayer), 74);
    }

    /// A GUID string is its 32 hex digits, however dashed or braced; any
    /// other count of digits is no GUID.
    #[test]
    fn parse_guid_needs_exactly_32_hex_digits() {
        assert!(parse_guid("{01234567-89AB-CDEF-0123-456789ABCDEF}").is_some());
        assert!(parse_guid("0123456789ABCDEF0123456789ABCDEF").is_some());
        assert!(parse_guid("not-a-guid").is_none());
        assert!(parse_guid("{01234567-89AB-CDEF-0123-456789ABCDE}").is_none());
    }

    /// The name block is a Pascal string: a name over 255 bytes is refused
    /// by the writer, naming the field, rather than written wrapped.
    #[test]
    fn encode_data_stream_refuses_a_name_longer_than_a_pascal_string() {
        let footprint = Footprint::new("N".repeat(256));
        let err = encode_data_stream(&footprint).expect_err("a 256-byte name must be refused");
        let text = err.to_string();
        assert!(text.contains("footprint.name"), "{text}");
        assert!(text.contains("exceeds maximum of 255 bytes"), "{text}");
    }
    /// A font face name past 31 UTF-16 units is refused by the writer — the
    /// record's 64-byte field holds no more beside its terminator — rather
    /// than written cut short.
    #[test]
    fn encode_data_stream_refuses_a_font_face_name_past_31_units() {
        let with = |field: fn(&mut Text, String), name: String| {
            let mut footprint = Footprint::new("F");
            let mut text = Text::new(0.0, 0.0, "T", 1.0, Layer::TopOverlay);
            field(&mut text, name);
            footprint.add_text(text);
            encode_data_stream(&footprint)
        };
        let longest = "F".repeat(31);
        assert!(with(|t, n| t.font_name = n, longest.clone()).is_ok());
        assert!(with(|t, n| t.barcode_font_name = n, longest).is_ok());

        let err = with(|t, n| t.font_name = n, "F".repeat(32)).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("text[0].font_name"), "{text}");
        assert!(text.contains("32 UTF-16 units"), "{text}");
        let err = with(|t, n| t.barcode_font_name = n, "\u{1F600}".repeat(16)).unwrap_err();
        assert!(
            err.to_string().contains("text[0].barcode_font_name"),
            "{err}"
        );
    }

    /// A `top_middle_bottom` stack cannot hold a per-layer rounded
    /// rectangle — the main block has no per-layer radius bytes — so the
    /// writer refuses it rather than storing plain round.
    #[test]
    fn encode_data_stream_refuses_rounded_rectangle_in_a_tmb_stack() {
        let mut footprint = Footprint::new("TMB");
        let mut pad = Pad::through_hole("7", 0.0, 0.0, 1.6, 1.6, 0.8);
        pad.stack_mode = PadStackMode::TopMiddleBottom;
        pad.per_layer_shapes = Some(vec![
            PadShape::Round,
            PadShape::RoundedRectangle,
            PadShape::Rectangle,
        ]);
        footprint.add_pad(pad);
        let err = encode_data_stream(&footprint).expect_err("TMB rounded_rectangle");
        let text = err.to_string();
        assert!(text.contains("pads[0].per_layer_shapes"), "{text}");
        assert!(text.contains("pad '7'"), "{text}");
        assert!(text.contains("full_stack"), "{text}");
    }

    /// From scratch the block is Altium's five keys, with HEIGHT as a mil
    /// string — byte-identical to the block the golden carries.
    #[test]
    fn footprint_params_from_scratch_is_the_five_key_block() {
        let mut fp = Footprint::new("R0402");
        fp.description = "Chip resistor".to_string();
        assert_eq!(
            build_footprint_params(&fp),
            "|PATTERN=R0402|HEIGHT=0mil|DESCRIPTION=Chip resistor|ITEMGUID=|REVISIONGUID="
        );
        fp.height = 1.0;
        assert!(
            build_footprint_params(&fp).contains("|HEIGHT=39.370079mil|"),
            "1 mm is 39.370079 mil in Altium's 0.###### form"
        );
    }

    /// A name outside ASCII gets its UTF-16 code units in a twin, bracketed by
    /// UNICODE=EXISTS at both ends, after the other keys — #507's canary, as
    /// Altium would write it — and that twin is the name Altium will read.
    #[test]
    fn footprint_params_add_a_unicode_twin_for_a_non_ascii_name() {
        let mut fp = Footprint::new("CANARY*X/\u{FF08}0402\u{FF09}\u{D7}"); // CANARY*X/（0402）×
        fp.description = "Canary".to_string();
        let wire = crate::altium::to_wire_text(&fp.name);
        assert_eq!(
            build_footprint_params(&fp),
            format!(
                "|UNICODE=EXISTS|PATTERN={wire}|HEIGHT=0mil|DESCRIPTION=Canary|ITEMGUID=|REVISIONGUID=\
                 |UNICODE__PATTERN=67,65,78,65,82,89,42,88,47,65288,48,52,48,50,65289,215|UNICODE=EXISTS"
            )
        );
        assert_eq!(footprint_pattern_text(&fp), wire);
        assert_eq!(footprint_name_as_altium_reads_it(&fp), fp.name);
    }

    /// A carried block is replayed verbatim while PATTERN and the twins still
    /// describe the name and description — the i18n4 shape, AREA included —
    /// with only HEIGHT coming from its field.
    #[test]
    fn footprint_params_replay_a_carried_block_verbatim() {
        let mut fp = Footprint::new("\u{13E3}\u{13B3}\u{13A9}_CR_0402"); // ᏣᎳᎩ_CR_0402
        fp.description.clone_from(&fp.name);
        let units = "5091,5043,5033,95,67,82,95,48,52,48,50";
        let carried = [
            ("UNICODE", "EXISTS"),
            ("PATTERN", "???_CR_0402"),
            ("DESCRIPTION", "???_CR_0402"),
            ("ITEMGUID", ""),
            ("REVISIONGUID", ""),
            ("AREA", "462399999999.999936"),
            ("UNICODE__DESCRIPTION", units),
            ("UNICODE__PATTERN", units),
            ("UNICODE", "EXISTS"),
        ];
        fp.additional_parameters = carried
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        fp.param_key_order = [
            "UNICODE",
            "PATTERN",
            "HEIGHT",
            "DESCRIPTION",
            "ITEMGUID",
            "REVISIONGUID",
            "AREA",
            "UNICODE__DESCRIPTION",
            "UNICODE__PATTERN",
            "UNICODE",
        ]
        .iter()
        .map(|k| (*k).to_string())
        .collect();
        assert_eq!(
            build_footprint_params(&fp),
            format!(
                "|UNICODE=EXISTS|PATTERN=???_CR_0402|HEIGHT=0mil|DESCRIPTION=???_CR_0402|ITEMGUID=\
                 |REVISIONGUID=|AREA=462399999999.999936|UNICODE__DESCRIPTION={units}\
                 |UNICODE__PATTERN={units}|UNICODE=EXISTS"
            )
        );
        assert_eq!(
            footprint_pattern_text(&fp),
            "???_CR_0402",
            "the husk Altium wrote"
        );
        assert_eq!(footprint_name_as_altium_reads_it(&fp), fp.name);
    }

    /// A rename leaves the carried PATTERN and twin stale, so both are rebuilt
    /// from the new name in place; a name back inside ASCII drops the twin
    /// and its UNICODE markers.
    #[test]
    fn footprint_params_rebuild_stale_entries_after_a_rename() {
        let mut fp = Footprint::new("\u{13E3}\u{13B3}\u{13A9}_CR_0402");
        fp.additional_parameters = [
            ("UNICODE", "EXISTS"),
            ("PATTERN", "???_CR_0402"),
            ("DESCRIPTION", ""),
            ("ITEMGUID", ""),
            ("REVISIONGUID", ""),
            ("UNICODE__PATTERN", "5091,5043,5033,95,67,82,95,48,52,48,50"),
            ("UNICODE", "EXISTS"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
        fp.param_key_order = [
            "UNICODE",
            "PATTERN",
            "HEIGHT",
            "DESCRIPTION",
            "ITEMGUID",
            "REVISIONGUID",
            "UNICODE__PATTERN",
            "UNICODE",
        ]
        .iter()
        .map(|k| (*k).to_string())
        .collect();

        fp.name = "\u{3A9}_0402".to_string(); // Ω_0402
        assert_eq!(
            build_footprint_params(&fp),
            format!(
                "|UNICODE=EXISTS|PATTERN={}|HEIGHT=0mil|DESCRIPTION=|ITEMGUID=|REVISIONGUID=\
                 |UNICODE__PATTERN=937,95,48,52,48,50|UNICODE=EXISTS",
                crate::altium::to_wire_text(&fp.name)
            )
        );

        fp.name = "R0402".to_string();
        assert_eq!(
            build_footprint_params(&fp),
            "|PATTERN=R0402|HEIGHT=0mil|DESCRIPTION=|ITEMGUID=|REVISIONGUID="
        );
    }

    /// A name that leaves ASCII after being read without a twin cannot replay
    /// the old order: the block is rebuilt in canonical order with the twin,
    /// the carried GUIDs kept.
    #[test]
    fn footprint_params_fall_back_to_canonical_order_when_a_twin_is_new() {
        let mut fp = Footprint::new("R0402");
        fp.additional_parameters = [
            ("PATTERN", "R0402"),
            ("DESCRIPTION", ""),
            ("ITEMGUID", "{ITEM}"),
            ("REVISIONGUID", "{REV}"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
        fp.param_key_order = [
            "PATTERN",
            "HEIGHT",
            "DESCRIPTION",
            "ITEMGUID",
            "REVISIONGUID",
        ]
        .iter()
        .map(|k| (*k).to_string())
        .collect();
        fp.name = "R0402\u{3A9}".to_string();
        assert_eq!(
            build_footprint_params(&fp),
            format!(
                "|UNICODE=EXISTS|PATTERN={}|HEIGHT=0mil|DESCRIPTION=|ITEMGUID={{ITEM}}\
                 |REVISIONGUID={{REV}}|UNICODE__PATTERN=82,48,52,48,50,937|UNICODE=EXISTS",
                crate::altium::to_wire_text(&fp.name)
            )
        );
    }

    /// The golden's script-authored Cyrillic footprint: PATTERN holds the
    /// name's UTF-8 bytes and the twin those bytes widened through
    /// Windows-1250 — the name Altium reads and derives the storage from.
    /// Both are replayed verbatim, and the Altium-side name is the widened
    /// one, not the real one the reader folds it back to.
    #[test]
    fn footprint_params_keep_a_widened_twin_and_report_altiums_view() {
        let real = "\u{420}\u{435}\u{437}\u{438}\u{441}\u{442}\u{43E}\u{440}_0402"; // Резистор_0402
        let wire = crate::altium::to_wire_text(real);
        let widened_units =
            "272,160,272,181,272,183,272,184,323,129,323,8218,272,318,323,8364,95,48,52,48,50";
        let mut fp = Footprint::new(real);
        fp.additional_parameters = [
            ("UNICODE", "EXISTS"),
            ("PATTERN", wire.as_str()),
            ("DESCRIPTION", ""),
            ("ITEMGUID", ""),
            ("REVISIONGUID", ""),
            ("UNICODE__PATTERN", widened_units),
            ("UNICODE", "EXISTS"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
        fp.param_key_order = [
            "UNICODE",
            "PATTERN",
            "HEIGHT",
            "DESCRIPTION",
            "ITEMGUID",
            "REVISIONGUID",
            "UNICODE__PATTERN",
            "UNICODE",
        ]
        .iter()
        .map(|k| (*k).to_string())
        .collect();
        assert_eq!(
            build_footprint_params(&fp),
            format!(
                "|UNICODE=EXISTS|PATTERN={wire}|HEIGHT=0mil|DESCRIPTION=|ITEMGUID=|REVISIONGUID=\
                 |UNICODE__PATTERN={widened_units}|UNICODE=EXISTS"
            )
        );
        let widened = crate::altium::text_from_utf16_units(widened_units).expect("units");
        assert_ne!(widened, real);
        assert_eq!(footprint_name_as_altium_reads_it(&fp), widened);
    }
}
