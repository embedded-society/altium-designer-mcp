//! `PcbLib` reader: per-primitive binary parsers (pad/via/track/arc/text/region/fill/component-body).
//!
//! # Truncation is caught per field, not by an upfront length guard
//!
//! Each parser reads its fields through bounds-checked accessors and reports the
//! first one that runs off the end of the block, naming that field. There is no
//! "block too short" pre-check, and deliberately so: a guard that asserted the
//! block was long enough for every field made each read's own error arm
//! unreachable, so the checks that actually protect the parse could never be
//! exercised — and a reader whose error handling cannot be tested is a reader
//! whose error handling is assumed rather than known.
//!
//! Rejection behaviour is unchanged: the same truncated blocks are still
//! refused, and the message now names the field that could not be read instead
//! of quoting a byte count the caller has to map back to a field themselves.
//!
//! The consequence for anyone editing a parser: **read every field through
//! `read_*`, `.first()` or `.get()`**. A bare `block[n]` panics on a truncated
//! block where the old guard would have returned an error. Length checks that
//! are *data-driven* rather than constant — a vertex count or a parameter-string
//! length read out of the block itself — are a different thing and stay.

#[allow(clippy::wildcard_imports)] // tightly-coupled reader split
use super::*;

/// Reads the common-header connectivity indices from a primitive block/header:
/// net index (u16 @3-4), polygon index (u16 @5-6) and component index (u16 @7-8,
/// exposed as `i32` with the `0xFFFF` sentinel mapped to `-1`).
///
/// These live in the `CommonPrimitiveData` header shared by every `PcbLib`
/// primitive. A missing byte (short header) or the `0xFFFF` sentinel reads back as the
/// from-scratch "none" default (`0xFFFF` / `0xFFFF` / `-1`). Inverse of
/// [`super::super::writer`]'s `write_common_indices`; factored so every parser
/// shares one implementation (mirrors how `parse_region` already reads @3/@5/@7).
fn read_common_indices(header: &[u8]) -> (u16, u16, i32) {
    let net_index = read_u16(header, 3).unwrap_or(0xFFFF);
    let polygon_index = read_u16(header, 5).unwrap_or(0xFFFF);
    let component_index = match read_u16(header, 7).unwrap_or(0xFFFF) {
        0xFFFF => -1,
        ci => i32::from(ci),
    };
    (net_index, polygon_index, component_index)
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
pub(super) fn parse_pad(data: &[u8], offset: usize) -> ParseResult<Pad> {
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

    // Common header (13 bytes). The layer byte opens it, so an empty geometry
    // block fails here.
    let layer_id = *geometry
        .first()
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Pad layer"))?;
    let (layer, raw_layer_id) = resolve_layer(layer_id, geometry, 114);
    let flags = read_flags(geometry);
    // Common-header connectivity indices @3-8 (net/polygon/component).
    let (net_index, polygon_index, component_index) = read_common_indices(geometry);

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

    // Hole size @45 and shape @49. Both are required: a land with no shape byte
    // is not a pad, and defaulting it to a rounded rectangle with no drill would
    // hand back a plausible-looking pad that solders to nothing. `None` still
    // means "no hole" — an SMD land reads a zero here — but the bytes have to be
    // present to say so.
    let drill =
        to_mm(read_i32(geometry, 45).ok_or_else(|| {
            AltiumError::parse_error(offset + 45, "failed to read Pad hole size")
        })?);
    let hole_size = (drill > 0.001).then_some(drill);

    let shape = pad_shape_from_id(
        *geometry
            .get(49)
            .ok_or_else(|| AltiumError::parse_error(offset + 49, "failed to read Pad shape"))?,
    );

    // Rotation - offset 52 (8-byte double)
    let rotation = if geometry.len() > 59 {
        read_f64(geometry, 52).unwrap_or(0.0)
    } else {
        0.0
    };

    // Is plated - offset 60 (bool). An independent flag Altium defaults to 1
    // for every pad, SMD included (verified against AltiumSharp ReadPad and
    // the golden fixture), so an absent byte reads back as `true`.
    let is_plated = geometry.get(60).is_none_or(|&b| b != 0);
    let solder_mask_expansion_from_hole_edge = geometry.get(125).is_some_and(|&b| b != 0);
    let jumper_id = read_i16(geometry, 110).unwrap_or(0);

    // Per-pad identity GUIDs — extended-tail 16-byte fields @126 (GUID-A) and
    // @142 (GUID-B), read back verbatim (including the golden's nil GUIDs) so
    // a loaded pad re-encodes byte-identically. Absent (short block) -> None,
    // which makes the writer generate a fresh GUID (the from-scratch default).
    let identity_guid = geometry.get(126..142).map(guid_string_from_bytes);
    let identity_guid_b = geometry.get(142..158).map(guid_string_from_bytes);

    // Hole shape comes from the 596-byte size/shape block (offset 262) when
    // present; a plain simple pad (empty Block 5) has a round hole. Main-block
    // offset 61 is reserved in Altium's layout, so it is not used here.
    let hole_shape = per_layer_data
        .filter(|d| d.len() >= 596)
        .map_or(HoleShape::Round, |d| hole_shape_from_id(d[262]));

    // Slot length @263 (i32) and hole rotation @267 (f64) live in the same
    // size/shape block; absent (plain simple pad) they default to 0.
    let hole_slot_length = per_layer_data
        .filter(|d| d.len() >= 596)
        .and_then(|d| read_i32(d, 263))
        .map_or(0.0, to_mm);
    let hole_rotation = per_layer_data
        .filter(|d| d.len() >= 596)
        .and_then(|d| read_f64(d, 267))
        .unwrap_or(0.0);

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

    // Paste/solder mask expansion modes - offsets 101/102 (tri-state byte).
    let paste_mask_expansion_mode = geometry
        .get(101)
        .map_or(MaskExpansionMode::None, |&b| MaskExpansionMode::from_id(b));
    let solder_mask_expansion_mode = geometry
        .get(102)
        .map_or(MaskExpansionMode::None, |&b| MaskExpansionMode::from_id(b));

    // Thermal-relief / power-plane connection fields (extended tail). Each
    // falls back to the from-scratch default (= Altium's pad template constant)
    // when the byte is absent, so a short or older pad round-trips faithfully.
    // 67: connection style; 68-71/74-77/78-81/82-85: i32 coords; 72-73: i16 count.
    let power_plane_connect_style = geometry
        .get(67)
        .map_or(PowerPlaneConnectStyle::Relief, |&b| {
            PowerPlaneConnectStyle::from_id(b)
        });
    let relief_conductor_width = read_i32(geometry, 68).map_or(0.254, to_mm);
    let relief_entries = read_i16(geometry, 72).unwrap_or(4);
    let relief_air_gap = read_i32(geometry, 74).map_or(0.254, to_mm);
    let power_plane_relief_expansion = read_i32(geometry, 78).map_or(0.508, to_mm);
    let power_plane_clearance = read_i32(geometry, 82).map_or(0.508, to_mm);

    // Drill tolerances @162 / @166 (i32). The 0x7FFFFFFF ("unset") sentinel and
    // any absent (short pad) value read back as None.
    let hole_positive_tolerance = read_i32(geometry, 162)
        .filter(|&t| t != i32::MAX)
        .map(to_mm);
    let hole_negative_tolerance = read_i32(geometry, 166)
        .filter(|&t| t != i32::MAX)
        .map(to_mm);

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
        // Corner radius from the size/shape block: offset 564 in the canonical
        // 596-byte layout, or offset 288 in the legacy block (back-compat).
        let corner_radius = per_layer_data.and_then(|data| {
            let radius = if data.len() >= 596 {
                data[564]
            } else if data.len() > 288 {
                data[288]
            } else {
                return None;
            };
            (radius > 0 && radius <= 100).then_some(radius)
        });
        (corner_radius, None, None, None, None)
    } else if stack_mode == PadStackMode::TopMiddleBottom {
        // For a TopMiddleBottom (LocalStack) pad the top/mid/bottom sizes and
        // shapes live in the MAIN geometry block (Block 5 is empty); they are
        // NOT in the 32-entry per-layer block. Surface them as a 3-entry
        // [top, mid, bottom] vector (mirrors AltiumSharp's Size/Shape Top/Mid/Bottom).
        // Top X/Y @21/25 are already decoded as width/height; mid @29/33, bot @37/41.
        let mid_x = read_i32(geometry, 29).map_or(width, to_mm);
        let mid_y = read_i32(geometry, 33).map_or(height, to_mm);
        let bot_x = read_i32(geometry, 37).map_or(width, to_mm);
        let bot_y = read_i32(geometry, 41).map_or(height, to_mm);
        let sizes = vec![(width, height), (mid_x, mid_y), (bot_x, bot_y)];

        // Shapes: top @49 (already decoded as `shape`), mid @50, bottom @51.
        let mid_shape = geometry.get(50).map_or(shape, |&b| pad_shape_from_id(b));
        let bot_shape = geometry.get(51).map_or(shape, |&b| pad_shape_from_id(b));
        let shapes = vec![shape, mid_shape, bot_shape];

        (None, Some(sizes), Some(shapes), None, None)
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
        raw_layer_id,
        hole_size,
        is_plated,
        jumper_id,
        solder_mask_expansion_from_hole_edge,
        hole_shape,
        hole_slot_length,
        hole_rotation,
        hole_positive_tolerance,
        hole_negative_tolerance,
        rotation,
        paste_mask_expansion,
        solder_mask_expansion,
        paste_mask_expansion_mode,
        solder_mask_expansion_mode,
        power_plane_connect_style,
        relief_conductor_width,
        relief_entries,
        relief_air_gap,
        power_plane_relief_expansion,
        power_plane_clearance,
        corner_radius_percent,
        stack_mode,
        per_layer_sizes,
        per_layer_shapes,
        per_layer_corner_radii,
        per_layer_offsets,
        net_index,
        polygon_index,
        component_index,
        flags,
        unique_id: None,
        guid: None,
        // The extended tail as read, replayed verbatim as the write base so
        // unmodelled tail bytes (and the AD-version-specific tail LENGTH —
        // AD24 writes 133 where the template is 141) survive a rewrite.
        raw_tail: (geometry.len() > 61).then(|| geometry[61..].to_vec()),
        identity_guid,
        identity_guid_b,
    };

    Ok((pad, current))
}

/// Formats a 16-byte on-disk identity GUID as a braced uppercase GUID string
/// (e.g. `{A5172B29-…}`). Altium stores GUIDs in the Windows little-endian
/// mixed layout, matching `AltiumSharp`'s `new Guid(byte[16])`; the writer's
/// `guid_bytes_from_string` is the exact inverse, so a read value re-encodes
/// byte-identically.
///
/// # Panics
///
/// The caller must pass exactly 16 bytes (a checked slice from `get(a..b)`).
fn guid_string_from_bytes(bytes: &[u8]) -> String {
    let array: [u8; 16] = bytes.try_into().expect("identity GUID is 16 bytes");
    let uuid = uuid::Uuid::from_bytes_le(array);
    format!("{{{}}}", uuid.hyphenated().to_string().to_uppercase())
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
pub(super) fn parse_per_layer_data(
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

    // Parse 32 size entries (256 bytes). The 320-byte guard above already proves
    // every pair is present, so these are read as fixed 8-byte chunks rather
    // than through fallible offsets whose `(0.0, 0.0)` fallback could never run.
    let sizes: Vec<(f64, f64)> = data[..256]
        .chunks_exact(8)
        .map(|e| {
            (
                to_mm(i32::from_le_bytes([e[0], e[1], e[2], e[3]])),
                to_mm(i32::from_le_bytes([e[4], e[5], e[6], e[7]])),
            )
        })
        .collect();

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

    // Parse 32 offset entries (256 bytes, starting at offset 320) if available.
    // `get` is both the length check and the read, so — as with the sizes above
    // — there is no unreachable per-entry fallback beneath it.
    let offsets = data.get(320..576).map(|entries| {
        entries
            .chunks_exact(8)
            .map(|e| {
                (
                    to_mm(i32::from_le_bytes([e[0], e[1], e[2], e[3]])),
                    to_mm(i32::from_le_bytes([e[4], e[5], e[6], e[7]])),
                )
            })
            .collect()
    });

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
pub(super) fn parse_via(data: &[u8], offset: usize) -> ParseResult<Via> {
    // Altium writes a via as a single block: the 13-byte common header followed
    // by the 321-byte via SubRecord-1 (offsets 13-320). Mirror of `encode_via`
    // (#113). It is one block, not the six pad-style blocks a via resembles.
    let (block, next) = read_block(data, offset)
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Via block"))?;

    let x = to_mm(
        read_i32(block, 13)
            .ok_or_else(|| AltiumError::parse_error(offset + 13, "failed to read Via x"))?,
    );
    let y = to_mm(
        read_i32(block, 17)
            .ok_or_else(|| AltiumError::parse_error(offset + 17, "failed to read Via y"))?,
    );
    let diameter = to_mm(
        read_i32(block, 21)
            .ok_or_else(|| AltiumError::parse_error(offset + 21, "failed to read Via diameter"))?,
    );
    let hole_size =
        to_mm(read_i32(block, 25).ok_or_else(|| {
            AltiumError::parse_error(offset + 25, "failed to read Via hole size")
        })?);
    // The layer span is what makes a via a via: without both ends it stitches
    // nothing, so a block that stops short of them is refused rather than
    // defaulted to "layer 0 to layer 0".
    let from_layer =
        layer_from_id(*block.get(29).ok_or_else(|| {
            AltiumError::parse_error(offset + 29, "failed to read Via from layer")
        })?);
    let to_layer = layer_from_id(
        *block
            .get(30)
            .ok_or_else(|| AltiumError::parse_error(offset + 30, "failed to read Via to layer"))?,
    );

    // Common-header flag word @1-2 (locked/keepout/tenting top+bottom). Tenting a
    // via is the highest-value property here — it covers the pad with solder mask.
    let flags = read_flags(block);
    // Common-header connectivity indices @3-8 (net/polygon/component). Read-only
    // surface for footprint vias (a free via keeps the 0xFFFF/none defaults).
    let (net_index, polygon_index, component_index) = read_common_indices(block);

    // Extended SubRecord-1 fields (offsets 31-74). A short block falls back to
    // the same defaults the Via struct uses.
    // Power-plane connection style @31 (0=Relief, 1=Direct, 2=NoConnect).
    let power_plane_connect_style = block.get(31).map_or(PowerPlaneConnectStyle::Relief, |&b| {
        PowerPlaneConnectStyle::from_id(b)
    });
    let thermal_relief_gap = read_i32(block, 32).map_or(0.254, to_mm);
    let thermal_relief_conductors = block.get(36).copied().unwrap_or(4);
    let thermal_relief_width = read_i32(block, 38).map_or(0.254, to_mm);
    // Power-plane relief expansion @42, plane clearance @46 (i32 -> mm).
    let power_plane_relief_expansion = read_i32(block, 42).map_or(0.508, to_mm);
    let power_plane_clearance = read_i32(block, 46).map_or(0.508, to_mm);
    // Paste-mask expansion @50 (i32 -> mm).
    let paste_mask_expansion = read_i32(block, 50).map_or(0.0, to_mm);
    let solder_mask_expansion = read_i32(block, 54).map_or(0.0, to_mm);
    // Offset 66 is a tri-state mode byte (0=None, 1=FromRule, 2=Manual), not a bool.
    let solder_mask_expansion_mode = block
        .get(66)
        .map_or(MaskExpansionMode::None, |&b| MaskExpansionMode::from_id(b));
    // Bottom-face solder-mask expansion @242. Only surfaced when it differs from the
    // front @54, so a template-default via (both faces equal) reads back as `None`
    // and re-emits byte-identically.
    // @258 bool: measure mask expansion from the hole edge; @312 byte: drill-pair
    // classification. Both sit past the per-layer diameter table.
    let solder_mask_expansion_from_hole_edge = block.get(258).is_some_and(|&b| b != 0);
    let drill_layer_pair_type = DrillLayerPairType::from_id(block.get(312).copied().unwrap_or(0));
    let solder_mask_expansion_back = match (read_i32(block, 242), read_i32(block, 54)) {
        (Some(back), Some(front)) if back != front => Some(to_mm(back)),
        _ => None,
    };
    let diameter_stack_mode = block
        .get(74)
        .map_or(ViaStackMode::Simple, |&b| via_stack_mode_from_id(b));

    // Drill tolerances @291 / @295 (i32). The 0x7FFFFFFF ("unset") sentinel and
    // any absent (short block) value read back as None.
    let hole_positive_tolerance = read_i32(block, 291).filter(|&t| t != i32::MAX).map(to_mm);
    let hole_negative_tolerance = read_i32(block, 295).filter(|&t| t != i32::MAX).map(to_mm);

    // Per-layer diameters: 32 x i32 from offset 75, only for a non-simple stack.
    let per_layer_diameters =
        if diameter_stack_mode != ViaStackMode::Simple && block.len() >= 75 + 32 * 4 {
            Some(
                (0..32)
                    .map(|i| read_i32(block, 75 + i * 4).map_or(diameter, to_mm))
                    .collect(),
            )
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
        solder_mask_expansion_mode,
        solder_mask_expansion_back,
        solder_mask_expansion_from_hole_edge,
        drill_layer_pair_type,
        hole_positive_tolerance,
        hole_negative_tolerance,
        paste_mask_expansion,
        power_plane_connect_style,
        power_plane_relief_expansion,
        power_plane_clearance,
        net_index,
        polygon_index,
        component_index,
        thermal_relief_gap,
        thermal_relief_conductors,
        thermal_relief_width,
        diameter_stack_mode,
        per_layer_diameters,
        flags,
        unique_id: None,
        guid: None,
        // The whole record block as read: the write base, so the in-record
        // identity GUID slots (zeros in every AD-authored library via) and any
        // unmodelled cache bytes survive a rewrite.
        raw_block: Some(block.to_vec()),
    };

    Ok((via, next))
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
pub(super) fn parse_track(data: &[u8], offset: usize) -> ParseResult<Track> {
    // Track has a single block with geometry data
    let (block, next) = read_block(data, offset)
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Track block"))?;

    // Common header (13 bytes). The layer byte opens it, so an empty block
    // fails here.
    let layer_id = *block
        .first()
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Track layer"))?;
    let (layer, raw_layer_id) = resolve_layer(layer_id, block, 41);
    let flags = read_flags(block);
    // Common-header connectivity indices @3-8 (net/polygon/component).
    let (net_index, polygon_index, component_index) = read_common_indices(block);

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

    // Extended tail (round-trip fidelity, #113): solder-mask expansion @35-38,
    // keepout restrictions @45. Kept `None` when absent or zero so a from-scratch
    // track (which writes 0) round-trips without gaining these keys.
    let solder_mask_expansion = read_i32(block, 35).map(to_mm).filter(|v| v.abs() > 1e-4);
    let keepout_restrictions = block.get(45).copied().filter(|&b| b != 0);

    let track = Track {
        x1,
        y1,
        x2,
        y2,
        width,
        layer,
        raw_layer_id,
        flags,
        net_index,
        polygon_index,
        component_index,
        unique_id: None,
        guid: None,
        solder_mask_expansion,
        keepout_restrictions,
    };

    Ok((track, next))
}

/// Parses an Arc primitive.
/// Returns the parsed `Arc` and the new offset on success.
pub(super) fn parse_arc(data: &[u8], offset: usize) -> ParseResult<Arc> {
    // Arc has a single block with geometry data
    let (block, next) = read_block(data, offset)
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Arc block"))?;

    // Common header (13 bytes). The layer byte opens it, so an empty block
    // fails here.
    let layer_id = *block
        .first()
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Arc layer"))?;
    let (layer, raw_layer_id) = resolve_layer(layer_id, block, 52);
    let flags = read_flags(block);
    // Common-header connectivity indices @3-8 (net/polygon/component).
    let (net_index, polygon_index, component_index) = read_common_indices(block);

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

    // Extended tail (round-trip fidelity, #113): solder-mask @47-50, keepout @56.
    let solder_mask_expansion = read_i32(block, 47).map(to_mm).filter(|v| v.abs() > 1e-4);
    let keepout_restrictions = block.get(56).copied().filter(|&b| b != 0);

    let arc = Arc {
        x,
        y,
        radius,
        start_angle,
        end_angle,
        width,
        layer,
        raw_layer_id,
        flags,
        net_index,
        polygon_index,
        component_index,
        unique_id: None,
        guid: None,
        solder_mask_expansion,
        keepout_restrictions,
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
/// [rotation:8 f64]              // Rotation angle (at offset 27)
/// [font_name:varies]            // Font name in UTF-16 (null-terminated)
/// [text_content:varies]         // Text content in UTF-16 or reference
/// ```
#[allow(clippy::too_many_lines)] // fixed 252-byte layout: many fields read at their offsets
pub(super) fn parse_text(
    data: &[u8],
    offset: usize,
    wide_strings: Option<&WideStrings>,
) -> ParseResult<Text> {
    // Text has 2 blocks:
    // - Block 0: Geometry/metadata (layer, position, height, rotation, font, etc.)
    // - Block 1: Text content (length-prefixed string, or reference to WideStrings)

    // Block 0: Geometry
    let (geometry_block, mut current) = read_block(data, offset)
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Text geometry block"))?;

    // Common header (13 bytes): layer at 0, Altium flag word at offsets 1-2.
    // Common header (13 bytes). The layer byte opens it, so an empty geometry
    // block fails here.
    let layer_id = *geometry_block
        .first()
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Text layer"))?;
    let (layer, raw_layer_id) = resolve_layer(layer_id, geometry_block, 226);
    // Decode the lock/tenting/keepout flag word like every other primitive does,
    // rather than discarding it (the write side already encodes these correctly).
    let flags = read_flags(geometry_block);
    // Common-header connectivity indices @3-8 (net/polygon/component).
    let (net_index, polygon_index, component_index) = read_common_indices(geometry_block);

    // The authoritative text kind lives at offset 160 in the 252-byte record
    // (0 = Stroke, 1 = TrueType, 2 = BarCode).
    let kind = if geometry_block.len() > 160 {
        text_kind_from_id(geometry_block[160])
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
        let font_id = read_u16(geometry_block, 25).unwrap_or(1);
        // The default stroke font is index 1; only a non-default selection is
        // surfaced as an explicit `StrokeFont`.
        if font_id > 1 {
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

    // Mirror flag - offset 35 (bool, IsMirrored). Absent/short blocks default to false.
    let mirror = geometry_block.get(35).is_some_and(|&b| b != 0);

    // Stroke line width - offset 36 (i32, internal units; Altium reads I32(36)).
    // A positive value is surfaced explicitly; 0/absent leaves it as the default.
    let stroke_width = read_i32(geometry_block, 36).filter(|&w| w > 0).map(to_mm);

    // Comment/Designator field markers - offsets 40/41 (bool, IsComment /
    // IsDesignator; verified against AltiumSharp ReadText B(40)/B(41)).
    // Absent/short blocks default to false (the template bytes).
    let is_comment = geometry_block.get(40).is_some_and(|&b| b != 0);
    let is_designator = geometry_block.get(41).is_some_and(|&b| b != 0);

    // Bold/italic styles - offsets 44/45 (bool). Absent/short blocks default to false.
    // baseFontType@43 is not read: it is fully derived from `kind` (offset 160).
    let bold = geometry_block.get(44).is_some_and(|&b| b != 0);
    let italic = geometry_block.get(45).is_some_and(|&b| b != 0);

    // Font name - offsets 46-109 (UTF-16, 64-byte field). Empty/short blocks
    // default to "Arial" (the template font).
    let font_name = read_text_font_name(geometry_block);

    // Text-box justification - offset 132 (Altium column-major encoding). Absent
    // blocks default to `BottomLeft` (the template's 0x03 anchor).
    let justification = geometry_block
        .get(132)
        .map_or(TextJustification::BottomLeft, |&b| {
            pcb_justification_from_id(b)
        });

    // Inverted (knockout) text-box descriptor.
    //   @110 IsInverted (bool), @111 InvertedBorder (i32 coord),
    //   @123 UseInvertedRectangle (bool), @124 InvertedRectWidth (i32 coord),
    //   @128 InvertedRectHeight (i32 coord), @133 InvertedRectTextOffset (i32 coord).
    // Offsets verified against AltiumSharp `PcbLibReader.ReadText`.
    let is_inverted = geometry_block.get(110).is_some_and(|&b| b != 0);
    // Border/text-offset default to 0 in the template, so a zero reads back as
    // `None` (round-trips to the same template bytes).
    let inverted_border = read_i32(geometry_block, 111).filter(|&v| v != 0).map(to_mm);
    let use_inverted_rectangle = geometry_block.get(123).is_some_and(|&b| b != 0);
    // The rect width/height template bytes are non-zero (a precomputed text-box
    // size), so only surface them when the framed rectangle is actually in use;
    // otherwise leave them `None` and let the writer replay the template bytes
    // (byte-identity for plain text).
    let (inverted_rect_width, inverted_rect_height) = if use_inverted_rectangle {
        (
            read_i32(geometry_block, 124).map(to_mm),
            read_i32(geometry_block, 128).map(to_mm),
        )
    } else {
        (None, None)
    };
    let inverted_rect_text_offset = read_i32(geometry_block, 133).filter(|&v| v != 0).map(to_mm);

    // Block 1: Text content
    let text_content = if let Some((text_block, next)) = read_block(data, current) {
        current = next;
        // Text block is a length-prefixed string
        let content = read_string_from_block(text_block);

        // /WideStrings takes precedence when this primitive has an entry: block 1
        // is a Pascal SHORT string, so Altium truncates it at 255 bytes and the
        // full text lives out of line.
        //
        // The entry is named by the index at geometry offset 115 — never
        // inferred from the content, which would make numeric text ambiguous
        // with an index. This call also resolves the special
        // `.Designator`/`.Comment` markers held in the geometry block.
        // See docs/PCBLIB_FORMAT.md § /{component}/WideStrings.
        let out_of_line = extract_text_from_block(geometry_block, wide_strings);
        if out_of_line.is_empty() {
            content
        } else {
            out_of_line
        }
    } else {
        // Fallback: check geometry block
        extract_text_from_block(geometry_block, wide_strings)
    };

    // Barcode block. Offsets verified by authoring two barcodes whose sizing
    // differs and diffing the records: @137/@141 carried the authored 400/600 mil
    // widths and 100/150 mil heights, @145/@149 the 20/30 and 20/40 mil margins,
    // @157 the symbology, and @161 the font name as UTF-16LE. Only surfaced for a
    // barcode, so a plain text keeps replaying the template bytes untouched.
    let is_barcode = kind == TextKind::BarCode;
    let barcode_coord = |o: usize| {
        if is_barcode {
            read_i32(geometry_block, o).map(to_mm)
        } else {
            None
        }
    };
    let barcode_full_width = barcode_coord(137);
    let barcode_full_height = barcode_coord(141);
    let barcode_margin_x = barcode_coord(145);
    let barcode_margin_y = barcode_coord(149);
    let barcode_kind = geometry_block.get(157).copied().unwrap_or(0);
    let barcode_inverted = is_barcode && geometry_block.get(159).is_some_and(|&b| b != 0);
    let barcode_show_text = is_barcode && geometry_block.get(225).is_some_and(|&b| b != 0);
    // UTF-16LE and null-padded, unlike every other string in this record.
    let barcode_font_name = if is_barcode {
        let raw = geometry_block.get(161..225).unwrap_or_default();
        let units: Vec<u16> = raw
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0)
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::new()
    };

    let text = Text {
        x,
        y,
        text: text_content,
        height,
        layer,
        raw_layer_id,
        rotation,
        kind,
        stroke_font,
        stroke_width,
        italic,
        bold,
        mirror,
        is_comment,
        is_designator,
        font_name,
        justification,
        is_inverted,
        inverted_border,
        use_inverted_rectangle,
        inverted_rect_width,
        inverted_rect_height,
        inverted_rect_text_offset,
        flags,
        net_index,
        polygon_index,
        component_index,
        unique_id: None,
        guid: None,
        // The geometry block as read: the write base, so AD's cached render
        // metrics (bytes we do not model, zeroed or filled differently per AD
        // version) survive a rewrite.
        raw_geometry: Some(geometry_block.to_vec()),
        barcode_full_width,
        barcode_full_height,
        barcode_x_margin: barcode_margin_x,
        barcode_y_margin: barcode_margin_y,
        barcode_kind,
        barcode_font_name,
        barcode_inverted,
        barcode_show_text,
    };

    Ok((text, current))
}

/// Reads the 64-byte UTF-16 font-name field at offset 46 of a text geometry
/// block. Decodes little-endian UTF-16 up to the first null pair and defaults to
/// `"Arial"` (the template font) when the field is absent or empty.
fn read_text_font_name(block: &[u8]) -> String {
    let Some(field) = block.get(46..110) else {
        return "Arial".to_string();
    };
    let mut units = Vec::with_capacity(32);
    for pair in field.chunks_exact(2) {
        let unit = u16::from_le_bytes([pair[0], pair[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    if units.is_empty() {
        return "Arial".to_string();
    }
    String::from_utf16_lossy(&units)
}

/// Converts an Altium PCB text-box justification byte (offset 132) to a
/// [`TextJustification`]. The inverse of the writer's `pcb_justification_to_id`:
/// Altium's column-major (1-based) encoding is mapped back onto the shared 3x3
/// grid. The template's `0x03` (= Altium `LeftBottom`) round-trips to the default
/// `BottomLeft` anchor; the manual byte `0` (no member in the shared grid) also
/// decodes to `BottomLeft`.
const fn pcb_justification_from_id(id: u8) -> TextJustification {
    match id {
        1 => TextJustification::TopLeft,
        2 => TextJustification::MiddleLeft,
        4 => TextJustification::TopCenter,
        5 => TextJustification::MiddleCenter,
        6 => TextJustification::BottomCenter,
        7 => TextJustification::TopRight,
        8 => TextJustification::MiddleRight,
        9 => TextJustification::BottomRight,
        // 3 (LeftBottom, the template default) and 0 (manual) → BottomLeft.
        _ => TextJustification::BottomLeft,
    }
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
pub(super) fn extract_text_from_block(block: &[u8], wide_strings: Option<&WideStrings>) -> String {
    // Check for special designator/comment text inline
    for pattern in [".Designator", ".Comment"] {
        if find_ascii_in_block(block, pattern).is_some() {
            return pattern.to_string();
        }
    }

    // Try to find a WideStrings index in the block
    // The WideStringsIndex is a u16 at offset 115 in the geometry block
    // Verified by reverse-engineering an Altium-authored library with Text primitives
    if let Some(ws) = wide_strings {
        // The u16 at offset 115 needs bytes 115..117, i.e. len >= 117; `read_u16`
        // already bounds-checks, so no redundant length guard (the old `> 117`
        // wrongly rejected a block of exactly 117 bytes).
        if let Some(index) = read_u16(block, 115) {
            if let Some(resolved) = ws.get(&(index as usize)) {
                tracing::trace!(index, resolved = %resolved, "Resolved WideStrings from offset 115");
                return resolved.clone();
            }
        }
    }

    // No text content found
    String::new()
}

/// Finds an ASCII pattern within a block (for special text like ".Designator").
pub(super) fn find_ascii_in_block(block: &[u8], pattern: &str) -> Option<usize> {
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
/// Reads one count-prefixed vertex contour (`[u32 count][count x 16-byte (x, y)
/// doubles]`) from `props_block` starting at `at`. Returns the vertices and the
/// offset just past the contour. `label` names the contour in error messages and
/// `offset` is the record's absolute base for error reporting.
#[allow(clippy::cast_possible_truncation)] // Altium coords fit in i32
fn read_region_contour(
    props_block: &[u8],
    at: usize,
    offset: usize,
    label: &str,
) -> Result<(Vec<Vertex>, usize), AltiumError> {
    let count = read_u32(props_block, at).ok_or_else(|| {
        AltiumError::parse_error(offset + at, format!("failed to read {label} count"))
    })? as usize;
    let data_offset = at + 4;
    let end = data_offset + count * 16;
    // The data-driven length check IS the read: taking the whole contour as one
    // slice reports a truncated block exactly as before, and leaves the
    // per-vertex reads below infallible instead of guarded by arms no input
    // could reach.
    let vertex_bytes = props_block.get(data_offset..end).ok_or_else(|| {
        AltiumError::parse_error(
            offset + at,
            format!(
                "Region block too short for {label} with {count} vertices: {} bytes, expected {end}",
                props_block.len()
            ),
        )
    })?;

    // Coordinates are doubles in internal units; quantise to mm.
    let contour = vertex_bytes
        .chunks_exact(16)
        .map(|v| {
            let x = f64::from_le_bytes([v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]]);
            let y = f64::from_le_bytes([v[8], v[9], v[10], v[11], v[12], v[13], v[14], v[15]]);
            Vertex {
                x: to_mm(x.round() as i32),
                y: to_mm(y.round() as i32),
            }
        })
        .collect();
    Ok((contour, end))
}

#[allow(clippy::cast_possible_truncation)] // Altium coords fit in i32
pub(super) fn parse_region(data: &[u8], offset: usize) -> ParseResult<Region> {
    // Region format (observed from Altium files): a single block containing:
    //   - Common header (13 bytes): layer, flags, padding
    //   - Unknown data (5 bytes)
    //   - Parameter string length (4 bytes)
    //   - Parameter string (ASCII key=value pairs)
    //   - Vertex count (4 bytes)
    //   - Vertices (count * 16 bytes, each as 2 doubles)
    // A region is a single block: common header, parameter string, and the
    // vertex outline embedded within it.
    let (props_block, current) = read_block(data, offset).ok_or_else(|| {
        AltiumError::parse_error(offset, "failed to read Region properties block")
    })?;

    // Common header (13 bytes): @0 layer, @1-2 flags, @3-4 net index (u16),
    // @5-6 polygon index (u16), @7-8 component index (u16, 0xFFFF -> -1), @9-12 reserved.
    // The layer byte opens it, so an empty properties block fails here.
    let layer_id = *props_block
        .first()
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Region layer"))?;
    let flags = read_flags(props_block);
    let (net_index, polygon_index, component_index) = read_common_indices(props_block);

    // @13 reserved | @14-15 hole_count (u16) | @16-17 reserved. The trailing hole
    // contours (if any) follow the outline. A no-hole region reports 0 here.
    let hole_count = read_u16(props_block, 14).unwrap_or(0) as usize;

    // Read parameter string length at offset 18
    let param_len = read_u32(props_block, 18).ok_or_else(|| {
        AltiumError::parse_error(offset + 18, "failed to read Region parameter string length")
    })? as usize;

    // Parse the nested C-string parameter block (offsets 22..22+param_len). It carries
    // KIND, NAME, ARCRESOLUTION, CAVITYHEIGHT, etc. in the canonical `KEY=VALUE|...`
    // form (no leading pipe, Windows-1252, null-terminated). Historically skipped by
    // length; now decoded into the region's typed fields.
    let param_end = 22 + param_len;
    if props_block.len() < param_end {
        return Err(AltiumError::parse_error(
            offset + 22,
            format!("Region parameter block truncated: needs {param_end} bytes"),
        ));
    }
    let params_str = crate::altium::decode_ansi(
        &props_block[22..param_end],
        crate::altium::current_ansi_encoding(),
    );
    let params = crate::altium::parse_pipe_params_raw(&params_str);

    // The header byte names the layer, except that a mechanical byte defers
    // to a `V7_LAYER` token past the sixteen it can hold (see `resolve_layer`).
    let layer = mechanical_past_sixteen(layer_id, v7_token_mechanical(params.get("V7_LAYER")))
        .unwrap_or_else(|| layer_from_id(layer_id));

    // Capture every key the typed model does NOT consume, in read order, so a
    // read-modify-write round-trips the board-region keys (LAYER, KEEPOUT, ...)
    // Altium writes but we do not model. The writer re-emits these verbatim after
    // its canonical key set. The modelled keys below are exactly those backed by a
    // Region struct field (and thus re-emitted from that field); everything else
    // is "additional".
    let additional_parameters = capture_additional_params(&params_str, REGION_MODELLED_PARAM_KEYS);
    let param_key_order: Vec<String> = crate::altium::parse_pipe_params_ordered(&params_str)
        .into_iter()
        .map(|(key, _)| key)
        .collect();

    // Keep `V7_LAYER` only when it disagrees with the layer byte, which is what
    // a board cutout does — see `Region::v7_layer`. Deriving it back from
    // `layer` covers every other region, and leaving it `None` there means a
    // caller that moves a region to another layer still gets the right token.
    let v7_layer = params
        .get("V7_LAYER")
        .filter(|token| **token != crate::altium::pcblib::writer::v7_layer_token(layer))
        .cloned();

    // Vertex data follows the parameter string.
    let vertex_offset = param_end;

    if props_block.len() < vertex_offset + 4 {
        return Err(AltiumError::parse_error(
            offset + vertex_offset,
            format!("Region block too short for vertex count at offset {vertex_offset}"),
        ));
    }

    // Outline contour: count-prefixed vertices immediately after the param string.
    let (vertices, mut next_offset) =
        read_region_contour(props_block, vertex_offset, offset, "Region vertex")?;

    // Trailing hole contours follow the outline, each as `[u32 count][count*16B]`.
    // `hole_count` (read from @14) bounds the loop; the helper length-guards each
    // contour so a truncated block fails cleanly instead of over-reading.
    let mut holes = Vec::with_capacity(hole_count);
    for h in 0..hole_count {
        let label = format!("Region hole {h}");
        let (contour, end) = read_region_contour(props_block, next_offset, offset, &label)?;
        holes.push(contour);
        next_offset = end;
    }

    // A region is a single block — there is no trailing empty "Block 1". Altium
    // places the next record's type byte immediately after this block, so `current`
    // already points at the next record. Reading a second block here would
    // mis-read the next record's bytes against a real Altium region.
    // Extract typed properties from the parsed parameter block. Missing keys fall
    // back to the from-scratch defaults so a minimal region still round-trips.
    let kind = params
        .get("KIND")
        .and_then(|v| v.parse::<i32>().ok())
        .map_or(RegionKind::Copper, RegionKind::from_id);
    let name = params.get("NAME").cloned().unwrap_or_default();
    let sub_poly_index = params
        .get("SUBPOLYINDEX")
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(-1);
    let union_index = params
        .get("UNIONINDEX")
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    let is_shape_based = params
        .get("ISSHAPEBASED")
        .is_some_and(|v| v.eq_ignore_ascii_case("TRUE"));
    let arc_resolution = parse_mil_value(params.get("ARCRESOLUTION").map(String::as_str));
    let cavity_height = parse_mil_value(params.get("CAVITYHEIGHT").map(String::as_str));
    // The `NET` param, when present, carries the numeric net index; otherwise the
    // common-header index (@3) is authoritative.
    let net_index = params
        .get("NET")
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(net_index);

    let region = Region {
        vertices,
        holes,
        layer,
        v7_layer,
        flags,
        kind,
        param_key_order,
        name,
        net_index,
        polygon_index,
        component_index,
        arc_resolution,
        cavity_height,
        sub_poly_index,
        union_index,
        is_shape_based,
        unique_id: None,
        guid: None,
        additional_parameters,
    };

    Ok((region, current))
}

/// Region parameter keys the typed [`Region`] model consumes (each backed by a
/// struct field). Every OTHER key in the nested block is captured verbatim into
/// [`Region::additional_parameters`] so a read-modify-write does not drop it.
const REGION_MODELLED_PARAM_KEYS: &[&str] = &[
    "V7_LAYER",
    "NAME",
    "KIND",
    "SUBPOLYINDEX",
    "UNIONINDEX",
    "ARCRESOLUTION",
    "ISSHAPEBASED",
    "CAVITYHEIGHT",
    "NET",
];

/// Captures the parameters NOT in `modelled`, preserving read order and every
/// occurrence. Shared by the Region and `ComponentBody` parsers to build their
/// `additional_parameters` catch-all.
fn capture_additional_params(params_str: &str, modelled: &[&str]) -> Vec<(String, String)> {
    crate::altium::parse_pipe_params_ordered(params_str)
        .into_iter()
        .filter(|(key, _)| !modelled.contains(&key.as_str()))
        .collect()
}

/// Resolves a record's layer from its header byte and the V7 layer id at
/// `v7_at`, and returns the byte to carry when the layer cannot reproduce it.
///
/// The header byte holds sixteen mechanical layers; Altium stores a
/// primitive on Mechanical 17-32 under byte 72 (the last the byte can hold)
/// and names the real layer in the V7 id (`0x0102_0014` is Mechanical 20),
/// so a mechanical byte defers to a V7 id past sixteen. A byte the table does
/// not map lands on the `MultiLayer` catch-all and is carried as is, so the
/// rewrite puts the byte back rather than `74` (see `raw_layer_id`).
fn resolve_layer(byte: u8, block: &[u8], v7_at: usize) -> (Layer, Option<u8>) {
    let v7_mechanical = read_u32(block, v7_at)
        .filter(|v7| v7 >> 16 == 0x0102)
        .map(|v7| v7 & 0xFFFF);
    let layer = mechanical_past_sixteen(byte, v7_mechanical).unwrap_or_else(|| layer_from_id(byte));
    (layer, unmapped_layer_byte(byte, layer))
}

/// The layer a mechanical header byte and a V7 mechanical index past
/// sixteen name together, when they do.
fn mechanical_past_sixteen(byte: u8, v7_mechanical: Option<u32>) -> Option<Layer> {
    let index = v7_mechanical.filter(|index| (17..=32).contains(index))?;
    (57..=72).contains(&byte).then(|| {
        // Mechanical 17-32 sit at 186-201 in the layer table.
        #[allow(clippy::cast_possible_truncation)]
        let id = (169 + index) as u8;
        layer_from_id(id)
    })
}

/// The V7 mechanical index a `V7_LAYER` token names (`MECHANICAL20` is 20).
fn v7_token_mechanical(token: Option<&String>) -> Option<u32> {
    token?.strip_prefix("MECHANICAL")?.parse().ok()
}

/// The header byte to carry: the byte as read when the table does not map
/// it and the primitive therefore sits on the `MultiLayer` catch-all (#391);
/// nothing for a byte the layer reproduces, `74` included.
fn unmapped_layer_byte(byte: u8, layer: Layer) -> Option<u8> {
    (layer == Layer::MultiLayer && byte != crate::altium::pcblib::writer::layer_to_id(layer))
        .then_some(byte)
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
pub(super) fn parse_fill(data: &[u8], offset: usize) -> ParseResult<Fill> {
    // Fill has a single block
    let (block, current) = read_block(data, offset)
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Fill block"))?;

    // Common header (13 bytes). The layer byte opens it, so an empty block
    // fails here.
    let layer_id = *block
        .first()
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Fill layer"))?;
    let (layer, raw_layer_id) = resolve_layer(layer_id, block, 42);
    let flags = read_flags(block);
    // Common-header connectivity indices @3-8 (net/polygon/component).
    let (net_index, polygon_index, component_index) = read_common_indices(block);

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

    // Extended tail (round-trip fidelity): solder-mask expansion @37-40, keepout @46.
    // Kept `None` when absent/zero so a from-scratch fill round-trips unchanged.
    let solder_mask_expansion = read_i32(block, 37).map(to_mm).filter(|v| v.abs() > 1e-4);
    let keepout_restrictions = block.get(46).copied().filter(|&b| b != 0);

    let fill = Fill {
        x1,
        y1,
        x2,
        y2,
        layer,
        raw_layer_id,
        rotation,
        flags,
        net_index,
        polygon_index,
        component_index,
        solder_mask_expansion,
        keepout_restrictions,
        unique_id: None,
        guid: None,
    };

    Ok((fill, current))
}

/// Parses a `ComponentBody` primitive (3D model reference).
/// Returns the parsed `ComponentBody` and the new offset on success.
///
/// A `ComponentBody` is a single size-prefixed block (matching `AltiumSharp` and
/// the `BODY_3D` golden libraries): the layer/flags header, a C-string
/// parameter block, then the 2D outline polygon — all within the one block.
/// The body's decoded `IDENTIFIER` plus its four verbatim texture values
/// (centre X/Y, size X/Y). IDENTIFIER is a comma-separated list of decimal
/// Unicode code points (settled by `manual/identifier.PcbLib`: `µΩ电` =
/// `181,937,30005`), decoded here and re-encoded symmetrically by the writer.
/// The texture values round-trip verbatim: the UI writes
/// `TEXTURESIZEX=0.0001mil` where a scripted body carries `0mil`, so they
/// cannot be derived; `None` (absent key) lets the writer emit the
/// scripted-body default.
fn parse_body_identity_params(
    params: &std::collections::HashMap<String, String>,
) -> (String, [Option<String>; 5]) {
    let identifier = params
        .get("IDENTIFIER")
        .map(|v| decode_identifier(v))
        .unwrap_or_default();
    let texture = |key: &str| params.get(key).cloned();
    (
        identifier,
        [
            texture("TEXTURECENTERX"),
            texture("TEXTURECENTERY"),
            texture("TEXTURESIZEX"),
            texture("TEXTURESIZEY"),
            texture("TEXTUREROTATION"),
        ],
    )
}

/// Decodes an `IDENTIFIER` value — comma-separated decimal Unicode code
/// points — into the string it names. Empty input or any unparsable entry
/// yields an empty identifier (never a half-decoded one).
fn decode_identifier(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    value
        .split(',')
        .map(|part| {
            part.trim()
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .ok_or(())
        })
        .collect::<Result<String, ()>>()
        .unwrap_or_default()
}

/// The `MODEL.CHECKSUM` value, round-tripped verbatim (0 when absent).
fn parse_model_checksum(params: &std::collections::HashMap<String, String>) -> i64 {
    params
        .get("MODEL.CHECKSUM")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
}

#[allow(clippy::too_many_lines)] // one straight-line read per body field, like encode_data_stream
pub(super) fn parse_component_body(data: &[u8], offset: usize) -> ParseResult<ComponentBody> {
    // The single block holds the header, parameters and outline.
    let (block0, current) = read_block(data, offset).ok_or_else(|| {
        AltiumError::parse_error(offset, "failed to read ComponentBody block (properties)")
    })?;

    // Parse the outline polygon that follows the parameter block.
    let outline = parse_component_body_outline(block0);

    // Parse block 0 to extract parameters
    // Format: [header bytes][parameter_string]
    // Parameter string is pipe-separated key=value pairs starting with V7_LAYER=
    // Altium stores these as Windows-1252, not UTF-8 (#68).
    let block_str = crate::altium::decode_ansi(block0, crate::altium::current_ansi_encoding());

    // Find the parameter string (starts with V7_LAYER= or similar key)
    let params = parse_component_body_params(&block_str);

    // Capture every key the typed model does NOT consume, in read order, so a
    // read-modify-write round-trips the body keys Altium writes but we do not model
    // (TEXTURE*, MODEL.2D.X/Y, IDENTIFIER, MODEL.MODELTYPE, MODEL.MODELSOURCE, the
    // extrusion range, the repeated ARCRESOLUTION, CAVITYHEIGHT, ...). The writer
    // re-emits these verbatim after its canonical key set. The modelled keys are
    // exactly those backed by a ComponentBody struct field.
    let additional_parameters = block_str.find("V7_LAYER").map_or_else(Vec::new, |start| {
        capture_additional_params(&block_str[start..], BODY_MODELLED_PARAM_KEYS)
    });
    // Every key in read order, so the writer can put the unmodelled ones
    // back where Altium had them.
    let param_key_order: Vec<String> = block_str.find("V7_LAYER").map_or_else(Vec::new, |start| {
        crate::altium::parse_pipe_params_ordered(&block_str[start..])
            .into_iter()
            .map(|(key, _)| key)
            .collect()
    });

    // Extract key values
    let model_id = params.get("MODELID").cloned().unwrap_or_default();
    let model_name = params.get("MODEL.NAME").cloned().unwrap_or_default();
    let embedded = params.get("MODEL.EMBED").is_some_and(|v| v == "TRUE");

    // Rotations are plain decimal strings like "0.000"; heights carry a unit
    // suffix ("0mil", "15.748mil") and go through `parse_mil_value` below.
    let rotation = |key: &str| {
        params
            .get(key)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    let [rotation_x, rotation_y, rotation_z] =
        ["MODEL.3D.ROTX", "MODEL.3D.ROTY", "MODEL.3D.ROTZ"].map(rotation);

    let height = |key: &str| parse_mil_value(params.get(key).map(String::as_str));
    let [z_offset, standoff_height, cavity_height, overall_height] = [
        "MODEL.3D.DZ",
        "STANDOFFHEIGHT",
        "CAVITYHEIGHT",
        "OVERALLHEIGHT",
    ]
    .map(height);

    // MODEL.CHECKSUM is a plain integer. Round-trip it verbatim
    // (0 = default/valid) — it is not recomputed from the model bytes here.
    let model_checksum = parse_model_checksum(&params);

    // The body's layer is the CommonPrimitiveData layer byte at block offset 0 — the
    // authoritative source, exactly as AltiumSharp does (`result.Layer = layer`,
    // PcbLibReader.ReadComponentBody) — except that a mechanical byte defers to
    // a `V7_LAYER` token past the sixteen it can hold (see `resolve_layer`).
    // Decoding only the `V7_LAYER` parameter string and falling back to
    // `Top3DBody` would collapse every mechanical layer that fallback does not
    // cover (e.g. MECHANICAL13 / id 69) to the top 3D-body layer on read.
    //
    // There is deliberately no `V7_LAYER` fallback for a missing header byte:
    // `params` is decoded from this same block, so the only block that lacks the
    // byte — an empty one — has no parameter string to fall back to either.
    let layer = block0.first().map_or(Layer::Top3DBody, |&id| {
        mechanical_past_sixteen(id, v7_token_mechanical(params.get("V7_LAYER")))
            .unwrap_or_else(|| layer_from_id(id))
    });

    // #391's one-byte replay base: when the byte is an id `layer_from_id`
    // does not map, the decode above collapses it to the `MultiLayer`
    // catch-all and re-deriving the byte from `layer` would rewrite it. Keep
    // the byte (and, below, its `V7_LAYER` token) verbatim so the pair
    // round-trips; a genuine `MultiLayer` body (the canonical id itself)
    // keeps `None`.
    let raw_layer_id = block0
        .first()
        .and_then(|&id| unmapped_layer_byte(id, layer));
    let v7_layer = params
        .get("V7_LAYER")
        .filter(|token| **token != crate::altium::pcblib::writer::v7_layer_token(layer))
        .cloned();

    // Common-header connectivity indices @3-8 (net/polygon/component). The body's
    // block starts with the layer byte @0, the 0x0C/0x00 record-type marker @1-2,
    // then the net/polygon/component words @3-8 (0xFF padding for a free body).
    let (net_index, polygon_index, component_index) = read_common_indices(block0);

    // Additive fields. Each default matches the writer's
    // hard-coded literal default so a default body round-trips byte-identically.
    let name = params
        .get("NAME")
        .cloned()
        .unwrap_or_else(|| " ".to_string());
    let kind = params.get("KIND").and_then(|v| v.parse().ok()).unwrap_or(0);
    let sub_poly_index = params
        .get("SUBPOLYINDEX")
        .and_then(|v| v.parse().ok())
        .unwrap_or(-1);
    let union_index = params
        .get("UNIONINDEX")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let is_shape_based = params.get("ISSHAPEBASED").is_some_and(|v| v == "TRUE");
    let body_projection = params
        .get("BODYPROJECTION")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let body_color_3d = params
        .get("BODYCOLOR3D")
        .and_then(|v| v.parse().ok())
        .unwrap_or(8_421_504);
    let body_opacity_3d = params
        .get("BODYOPACITY3D")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    let model_2d_rotation = params
        .get("MODEL.2D.ROTATION")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    // MODEL.2D.X/Y are in BODY_MODELLED_PARAM_KEYS, so the additional_parameters
    // passthrough deliberately skips them — they have to be parsed here or the
    // offset is simply lost.
    let model_2d_x = parse_mil_value(params.get("MODEL.2D.X").map(String::as_str));
    let model_2d_y = parse_mil_value(params.get("MODEL.2D.Y").map(String::as_str));

    let (identifier, textures) = parse_body_identity_params(&params);

    let body = ComponentBody {
        identifier,
        texture_center_x: textures[0].clone(),
        texture_center_y: textures[1].clone(),
        texture_size_x: textures[2].clone(),
        texture_size_y: textures[3].clone(),
        texture_rotation: textures[4].clone(),
        model_id,
        model_name,
        embedded,
        rotation_x,
        rotation_y,
        rotation_z,
        z_offset,
        overall_height,
        standoff_height,
        cavity_height,
        layer,
        outline,
        unique_id: None,
        guid: None,
        model_checksum,
        name,
        kind,
        sub_poly_index,
        union_index,
        is_shape_based,
        body_projection,
        body_color_3d,
        body_opacity_3d,
        model_2d_rotation,
        model_2d_x,
        model_2d_y,
        raw_layer_id,
        v7_layer,
        net_index,
        polygon_index,
        component_index,
        additional_parameters,
        param_key_order,
    };

    Ok((body, current))
}

/// `ComponentBody` parameter keys excluded from `additional_parameters` capture:
/// each is either backed by a typed [`ComponentBody`] field (`IDENTIFIER`,
/// `TEXTURECENTERX/Y`, `TEXTURESIZEX/Y`, `MODEL.2D.X/Y`, `MODEL.MODELTYPE`,
/// `MODEL.EXTRUDED.*`, …) or a literal the writer re-emits unconditionally
/// (`ARCRESOLUTION`, `TEXTURE`). Every OTHER key in the block is captured
/// verbatim into [`ComponentBody::additional_parameters`] so a
/// read-modify-write does not drop it.
// Every key `build_component_body_params` (writer) emits unconditionally belongs
// here, or the reader captures it into `additional_parameters` and it round-trips
// as a spurious extra entry — and for the deliberately-repeated ARCRESOLUTION,
// captured TWICE. (The writer also dedupes captured canonical keys as a safety
// net, but excluding them here keeps `additional_parameters` clean. Region's
// `REGION_MODELLED_PARAM_KEYS` follows the same discipline.)
const BODY_MODELLED_PARAM_KEYS: &[&str] = &[
    "V7_LAYER",
    "NAME",
    "KIND",
    "SUBPOLYINDEX",
    "UNIONINDEX",
    "ARCRESOLUTION",
    "ISSHAPEBASED",
    "CAVITYHEIGHT",
    "STANDOFFHEIGHT",
    "OVERALLHEIGHT",
    "BODYPROJECTION",
    "BODYCOLOR3D",
    "BODYOPACITY3D",
    "IDENTIFIER",
    "TEXTURE",
    "TEXTURECENTERX",
    "TEXTURECENTERY",
    "TEXTURESIZEX",
    "TEXTURESIZEY",
    "TEXTUREROTATION",
    "MODELID",
    "MODEL.CHECKSUM",
    "MODEL.EMBED",
    "MODEL.NAME",
    "MODEL.2D.X",
    "MODEL.2D.Y",
    "MODEL.2D.ROTATION",
    "MODEL.3D.ROTX",
    "MODEL.3D.ROTY",
    "MODEL.3D.ROTZ",
    "MODEL.3D.DZ",
    "MODEL.MODELTYPE",
    "MODEL.MODELSOURCE",
    "MODEL.EXTRUDED.MINZ",
    "MODEL.EXTRUDED.MAXZ",
];

/// Parses the 2D outline polygon from a `ComponentBody` block.
///
/// Layout within the block: an 18-byte layer/flags header, the C-string
/// parameter block (`[u32 len incl. NUL][bytes][NUL]`), then `[u32 count]`
/// followed by `count` `(f64 x, f64 y)` vertices in Altium internal units.
/// Returns the vertices in mm, or empty if the block is malformed/truncated.
pub(super) fn parse_component_body_outline(block0: &[u8]) -> Vec<(f64, f64)> {
    const HEADER_LEN: usize = 18;

    // Skip the header + the C-string parameter block (its u32 prefix already
    // counts the bytes-plus-NUL that follow it).
    let Some(param_len) = read_u32(block0, HEADER_LEN) else {
        return Vec::new();
    };
    let mut off = HEADER_LEN + 4 + param_len as usize;

    let Some(count) = read_u32(block0, off) else {
        return Vec::new();
    };
    off += 4;

    let mut outline = Vec::new();
    for _ in 0..count {
        let (Some(x), Some(y)) = (read_f64(block0, off), read_f64(block0, off + 8)) else {
            break;
        };
        off += 16;
        outline.push((x * INTERNAL_UNITS_TO_MM, y * INTERNAL_UNITS_TO_MM));
    }
    outline
}

/// Parses key=value parameters from a `ComponentBody` block string.
pub(super) fn parse_component_body_params(s: &str) -> std::collections::HashMap<String, String> {
    // Parameters begin at the first `V7_LAYER=` key (after the binary header)
    // and end at the NUL terminator: the outline polygon follows it in the
    // same block, and without the cut the last key's value (TEXTUREROTATION on
    // an Altium-authored body) would carry the outline bytes.
    s.find("V7_LAYER")
        .map(|start| {
            let text = &s[start..];
            let end = text.find('\0').unwrap_or(text.len());
            crate::altium::parse_pipe_params_raw(&text[..end])
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod body_param_tests {
    use super::parse_component_body_params;

    /// The parameter text ends at its NUL; the outline bytes after it must
    /// not leak into the last key's value.
    #[test]
    fn body_params_stop_at_the_nul_terminator() {
        let block = "\u{1}\u{2}V7_LAYER=MECHANICAL13|NAME= |TEXTUREROTATION= 0.00000000000000E+0000\0\u{4}\0\0\0\u{80}binary";
        let params = parse_component_body_params(block);
        assert_eq!(
            params.get("TEXTUREROTATION").map(String::as_str),
            Some(" 0.00000000000000E+0000")
        );
        assert_eq!(params.get("NAME").map(String::as_str), Some(" "));
        assert!(parse_component_body_params("no parameters here").is_empty());
    }
}

/// Parses a value in mils (e.g., "15.748mil") to mm.
pub fn parse_mil_value(s: Option<&str>) -> f64 {
    let Some(s) = s else {
        return 0.0;
    };

    // Remove "mil" suffix if present
    let numeric = s.trim_end_matches("mil").trim();
    numeric.parse::<f64>().map_or(0.0, |v| v * MM_PER_MIL) // Convert mils to mm
}

// =============================================================================
// 3D Model Parsing
// =============================================================================

/// A mapping of model GUID to stream index.
///
/// The `/Library/Models/Data` stream contains entries that map GUIDs to
/// the numeric index of the model stream (e.g., `/Library/Models/0`) and the model name.
///
/// The value is a tuple of (`stream_index`, `model_name`).
pub type ModelIndex = HashMap<String, (usize, String)>;

// =============================================================================
// Tests
// =============================================================================
//
// These drive the per-primitive parsers from hand-built byte buffers rather
// than through the writer: the writer only ever emits the canonical, fully
// populated record, so a round-trip cannot reach the short-buffer guards, the
// "trailing field absent" fallbacks, or the unknown-discriminant arms. Those
// are exactly the branches that decide whether a malformed or older file is
// rejected loudly or loaded as a silently wrong primitive.

#[cfg(test)]
mod tests {
    use super::*;

    /// Millimetre comparison tolerance — coordinates are quantised to 1 nm.
    const EPS: f64 = 1e-9;

    #[test]
    fn common_indices_decode_values_sentinels_and_short_headers() {
        // Real associations at @3/@5/@7 survive as themselves.
        let mut header = vec![0u8; 9];
        header[3..5].copy_from_slice(&7u16.to_le_bytes());
        header[5..7].copy_from_slice(&9u16.to_le_bytes());
        header[7..9].copy_from_slice(&4u16.to_le_bytes());
        assert_eq!(read_common_indices(&header), (7, 9, 4));

        // The `0xFFFF` sentinel is "none": the indices keep it, the component
        // index maps to `-1` because that is its from-scratch default.
        assert_eq!(read_common_indices(&[0xFF; 9]), (0xFFFF, 0xFFFF, -1));

        // A header too short for a field falls back to the same "none" default
        // rather than panicking, at every truncation point.
        for len in 0..9 {
            assert_eq!(
                read_common_indices(&vec![0u8; len]),
                match len {
                    0..=4 => (0xFFFF, 0xFFFF, -1),
                    5..=6 => (0, 0xFFFF, -1),
                    _ => (0, 0, -1),
                },
                "truncated to {len} bytes"
            );
        }
    }

    /// Wraps `payload` in the `[u32 len][payload]` framing every `PcbLib` record
    /// block uses, so a test can hand-build a record without the writer.
    /// A Pascal short string as the text content block holds it:
    /// `[u8 len][Windows-1252 bytes]`.
    fn pascal(s: &str) -> Vec<u8> {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len() + 1);
        out.push(u8::try_from(bytes.len()).expect("fixture fits"));
        out.extend_from_slice(bytes);
        out
    }

    fn block(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(payload.len() + 4);
        out.extend_from_slice(
            &u32::try_from(payload.len())
                .expect("fixture fits")
                .to_le_bytes(),
        );
        out.extend_from_slice(payload);
        out
    }

    /// Assembles a Pad record: the four leading blocks `parse_pad` reads before
    /// the geometry block, the geometry block itself, and the optional Block 5
    /// (the per-layer / size-shape block).
    fn pad_record(geometry: &[u8], per_layer: Option<&[u8]>) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&block(&[0x01, b'1'])); // Block 0: designator "1"
        data.extend_from_slice(&block(&[])); // Block 1
        data.extend_from_slice(&block(&[0x04, b'|', b'&', b'|', b'0'])); // Block 2
        data.extend_from_slice(&block(&[])); // Block 3
        data.extend_from_slice(&block(geometry)); // Block 4: geometry
        if let Some(per_layer) = per_layer {
            data.extend_from_slice(&block(per_layer)); // Block 5
        }
        data
    }

    /// Writes the 13-byte `CommonPrimitiveData` header shared by every
    /// primitive: layer @0, the unlocked flag word @1-2, and the `0xFFFF`
    /// "no net / no polygon / no component" sentinels @3-8.
    fn write_common_header(buf: &mut [u8], layer_id: u8) {
        buf[0] = layer_id;
        buf[1..3].copy_from_slice(&0x0004_u16.to_le_bytes()); // FlagUnlocked
        buf[3..5].copy_from_slice(&0xFFFF_u16.to_le_bytes());
        buf[5..7].copy_from_slice(&0xFFFF_u16.to_le_bytes());
        buf[7..9].copy_from_slice(&0xFFFF_u16.to_le_bytes());
    }

    /// A well-formed pad geometry block padded to exactly `len` bytes, so a test
    /// can choose which trailing fields exist and drive their fallbacks.
    fn pad_geometry(len: usize) -> Vec<u8> {
        assert!(len >= 52, "a parseable pad geometry block is >= 52 bytes");
        let mut g = vec![0_u8; len];
        write_common_header(&mut g, 33); // TopOverlay
        g[13..17].copy_from_slice(&100_000_i32.to_le_bytes()); // x = 10 mil
        g[17..21].copy_from_slice(&200_000_i32.to_le_bytes()); // y = 20 mil
        g[21..25].copy_from_slice(&400_000_i32.to_le_bytes()); // width = 40 mil
        g[25..29].copy_from_slice(&300_000_i32.to_le_bytes()); // height = 30 mil
        g[49] = 2; // Rectangle
        g
    }

    /// A well-formed via block padded to exactly `len` bytes.
    fn via_block(len: usize) -> Vec<u8> {
        assert!(len >= 31, "a parseable via block is >= 31 bytes");
        let mut b = vec![0_u8; len];
        write_common_header(&mut b, 74); // MultiLayer
        b[13..17].copy_from_slice(&100_000_i32.to_le_bytes());
        b[17..21].copy_from_slice(&200_000_i32.to_le_bytes());
        b[21..25].copy_from_slice(&240_000_i32.to_le_bytes()); // 24 mil diameter
        b[25..29].copy_from_slice(&120_000_i32.to_le_bytes()); // 12 mil hole
        b[29] = 1; // from TopLayer
        b[30] = 32; // to BottomLayer
        b
    }

    /// A well-formed text geometry block padded to exactly `len` bytes.
    fn text_geometry(len: usize) -> Vec<u8> {
        assert!(len >= 25, "a parseable text geometry block is >= 25 bytes");
        let mut b = vec![0_u8; len];
        write_common_header(&mut b, 33); // TopOverlay
        b[13..17].copy_from_slice(&100_000_i32.to_le_bytes());
        b[17..21].copy_from_slice(&200_000_i32.to_le_bytes());
        b[21..25].copy_from_slice(&300_000_i32.to_le_bytes()); // height
        b
    }

    /// A region properties block: the 22-byte header (hole count @14, parameter
    /// length @18), the parameter string, then each count-prefixed contour.
    fn region_block(params: &str, hole_count: u16, contours: &[&[(i32, i32)]]) -> Vec<u8> {
        let mut b = vec![0_u8; 22];
        write_common_header(&mut b, 33);
        b[14..16].copy_from_slice(&hole_count.to_le_bytes());
        let params = params.as_bytes();
        b[18..22].copy_from_slice(
            &u32::try_from(params.len())
                .expect("fixture fits")
                .to_le_bytes(),
        );
        b.extend_from_slice(params);
        for contour in contours {
            b.extend_from_slice(
                &u32::try_from(contour.len())
                    .expect("fixture fits")
                    .to_le_bytes(),
            );
            for &(x, y) in *contour {
                b.extend_from_slice(&f64::from(x).to_le_bytes());
                b.extend_from_slice(&f64::from(y).to_le_bytes());
            }
        }
        b
    }

    // -------------------------------------------------------------------------
    // Pad
    // -------------------------------------------------------------------------

    #[test]
    fn pad_geometry_missing_a_named_field_is_rejected_by_that_field() {
        // A truncated pad record must fail loudly. Accepted silently, its size
        // and hole bytes would read as zeros and the footprint would load with a
        // 0 x 0 mm land — a pad that solders to nothing, with no error anywhere
        // to explain why the assembled board is open-circuit.
        //
        // Each field is refused by name, so the message says which byte range
        // was missing rather than quoting a total the reader has to map back.
        for (len, expected) in [
            (0, "failed to read Pad layer"),
            (14, "failed to read Pad x coordinate"),
            (18, "failed to read Pad y coordinate"),
            (22, "failed to read Pad width"),
            (26, "failed to read Pad height"),
            (46, "failed to read Pad hole size"),
            (49, "failed to read Pad shape"),
        ] {
            let data = pad_record(&vec![0_u8; len], None);
            let err = parse_pad(&data, 0).unwrap_err();
            assert!(
                err.to_string().contains(expected),
                "{len} bytes: expected {expected:?}, got: {err}"
            );
        }
    }

    /// Runs `body` with a TRACE-level subscriber installed on this thread, so
    /// the diagnostics these parsers emit on a damaged block are actually
    /// formatted rather than skipped by the level check.
    fn with_tracing<T>(body: impl FnOnce() -> T) -> T {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_test_writer()
            .finish();
        tracing::subscriber::with_default(subscriber, body)
    }

    #[test]
    fn a_pad_record_that_stops_between_blocks_names_the_block() {
        // Before any geometry field is read, five blocks are framed
        // `[u32 len][payload]`. A Data stream that ends inside that framing
        // never reaches the named fields the test above covers, so each block
        // reports itself — otherwise a record truncated at block 2 would be
        // indistinguishable from one carrying a malformed layer byte.
        let whole = pad_record(&pad_geometry(52), None);
        for (len, expected) in [
            (0, "failed to read Pad block 0 (designator)"),
            (6, "failed to read Pad block 1"),
            (10, "failed to read Pad block 2"),
            (19, "failed to read Pad block 3"),
            (23, "failed to read Pad block 4 (geometry)"),
            // The length prefix is present, but the payload it promises is not.
            (30, "failed to read Pad block 4 (geometry)"),
        ] {
            let err = parse_pad(&whole[..len], 0).unwrap_err();
            assert!(
                err.to_string().contains(expected),
                "{len} bytes: expected {expected:?}, got: {err}"
            );
        }
    }

    #[test]
    fn per_layer_data_shorter_than_the_table_is_declined_whole() {
        // The 32-entry table is all-or-nothing: 320 bytes of sizes, shapes and
        // corner radii. A block one byte short is refused entirely rather than
        // half-decoded, because a partial table would hand back per-layer sizes
        // for some layers and silent zeros for the rest — a pad that looks
        // stacked but lands on nothing below the layer the data ran out at.
        with_tracing(|| {
            for len in [0, 1, 319] {
                assert_eq!(
                    parse_per_layer_data(Some(&vec![0_u8; len])),
                    (None, None, None, None, None),
                    "{len} bytes"
                );
            }
        });

        // Absent entirely (a pad with no Block 5) is the same answer.
        assert_eq!(parse_per_layer_data(None), (None, None, None, None, None));
    }

    #[test]
    fn per_layer_offsets_appear_only_with_the_full_576_byte_block() {
        // Sizes/shapes/radii occupy 320 bytes; the 32 offset pairs that follow
        // take it to 576. Between the two the offsets are absent, not zeroed,
        // so a caller can tell "no offsets stored" from "offsets of zero".
        let mut data = vec![0_u8; 576];
        // Layer 0 size 40 x 30 mil, and a non-zero offset pair to prove the
        // table is read from 320 rather than defaulted.
        data[0..4].copy_from_slice(&400_000_i32.to_le_bytes());
        data[4..8].copy_from_slice(&300_000_i32.to_le_bytes());
        data[320..324].copy_from_slice(&10_000_i32.to_le_bytes());
        data[324..328].copy_from_slice(&(-20_000_i32).to_le_bytes());

        let (_, sizes, _, _, offsets) = parse_per_layer_data(Some(&data));
        let sizes = sizes.expect("a 576-byte block carries sizes");
        assert_eq!(sizes.len(), 32);
        assert!((sizes[0].0 - 1.016).abs() < 1e-6, "{}", sizes[0].0);
        assert!((sizes[0].1 - 0.762).abs() < 1e-6, "{}", sizes[0].1);
        assert!((sizes[31].0).abs() < EPS, "trailing entries read as zero");

        let offsets = offsets.expect("a 576-byte block carries offsets");
        assert_eq!(offsets.len(), 32);
        assert!((offsets[0].0 - 0.0254).abs() < 1e-6, "{}", offsets[0].0);
        assert!((offsets[0].1 + 0.0508).abs() < 1e-6, "{}", offsets[0].1);

        // One byte short of the offset table: sizes still parse, offsets do not.
        let (_, sizes, _, _, offsets) = parse_per_layer_data(Some(&data[..575]));
        assert!(sizes.is_some(), "the 320-byte table is still complete");
        assert!(offsets.is_none(), "a partial offset table is not reported");
    }

    #[test]
    fn a_component_body_with_no_header_or_parameters_takes_its_defaults() {
        // An empty properties block carries neither the layer byte nor the
        // parameter string, so every field falls back. The name matters most:
        // the writer emits a single space for a nameless body, and defaulting
        // to the empty string here would add a `NAME=` key on the next save
        // and break byte-identity for a body Altium wrote without one.
        let (body, next) =
            parse_component_body(&block(&[]), 0).expect("an empty block still parses");

        assert_eq!(next, 4, "the 4-byte length prefix is consumed");
        assert_eq!(body.layer, Layer::Top3DBody);
        assert_eq!(body.name, " ");
        assert!(body.outline.is_empty());
    }

    #[test]
    fn a_pad_is_accepted_once_every_named_field_is_present() {
        // The boundary is now the last named field (shape @49) rather than the
        // old hand-picked 52, which sat two bytes past anything the parser
        // reads. A 50-byte block carries a complete pad — position, size, hole
        // and shape — so it is parsed, with the optional tail taking its
        // template defaults.
        let mut geometry = vec![0_u8; 50];
        write_common_header(&mut geometry, 33);
        geometry[21..25].copy_from_slice(&400_000_i32.to_le_bytes());
        geometry[25..29].copy_from_slice(&300_000_i32.to_le_bytes());
        geometry[49] = 2; // Rectangle

        let (pad, _) = parse_pad(&pad_record(&geometry, None), 0).expect("50 bytes is a whole pad");
        assert!((pad.width - 1.016).abs() < 1e-6, "{}", pad.width);
        assert_eq!(pad.shape, PadShape::Rectangle);
        assert_eq!(pad.hole_size, None, "a zero drill means an SMD land");
        assert!((pad.rotation - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pad_with_only_the_mandatory_52_bytes_uses_template_defaults() {
        // Hand-built and older pads stop after the mandatory 52 bytes. Every
        // trailing field must fall back to Altium's pad-template constant, not
        // to zero: a 0 mm thermal-relief conductor width or a 0 mm plane
        // clearance would make the pad flood-connect straight into an internal
        // plane, and the board would be unsolderable (infinite heat sink).
        let data = pad_record(&pad_geometry(52), None);
        let (pad, next) = parse_pad(&data, 0).unwrap();

        assert_eq!(next, data.len(), "the parser must consume the whole record");
        assert_eq!(pad.designator, "1");
        assert_eq!(pad.layer, Layer::TopOverlay);
        assert!(pad.rotation.abs() < EPS);
        assert_eq!(pad.stack_mode, PadStackMode::Simple);
        assert!(pad.paste_mask_expansion.is_none());
        assert!(pad.solder_mask_expansion.is_none());
        assert_eq!(pad.paste_mask_expansion_mode, MaskExpansionMode::None);
        assert_eq!(pad.solder_mask_expansion_mode, MaskExpansionMode::None);
        assert_eq!(
            pad.power_plane_connect_style,
            PowerPlaneConnectStyle::Relief
        );
        assert!((pad.relief_conductor_width - 0.254).abs() < EPS);
        assert_eq!(pad.relief_entries, 4);
        assert!((pad.relief_air_gap - 0.254).abs() < EPS);
        assert!((pad.power_plane_relief_expansion - 0.508).abs() < EPS);
        assert!((pad.power_plane_clearance - 0.508).abs() < EPS);
        assert!(pad.is_plated, "an absent @60 byte means plated, not bare");
        assert!(pad.identity_guid.is_none());
        assert!(pad.identity_guid_b.is_none());
        assert!(pad.hole_positive_tolerance.is_none());
        assert!(pad.hole_negative_tolerance.is_none());
        assert_eq!(pad.hole_shape, HoleShape::Round);
        assert!(pad.hole_slot_length.abs() < EPS);
        assert!(pad.hole_rotation.abs() < EPS);
    }

    #[test]
    fn pad_unknown_layer_shape_and_stack_ids_degrade_to_documented_defaults() {
        // Altium keeps adding layer/shape ids, and a corrupt byte can hold any
        // value. Each lookup must degrade to its documented default; a table
        // index would panic instead, aborting the read of an entire library
        // because of one exotic pad.
        let mut g = pad_geometry(63);
        g[0] = 0xFE; // no such layer id
        g[49] = 99; // no such pad shape id
        g[62] = 0x7F; // no such pad stack mode id
        let data = pad_record(&g, None);
        let (pad, _) = parse_pad(&data, 0).unwrap();

        assert_eq!(pad.layer, Layer::MultiLayer);
        assert_eq!(pad.shape, PadShape::RoundedRectangle);
        assert_eq!(pad.stack_mode, PadStackMode::Simple);
    }

    #[test]
    fn pad_block5_too_short_to_hold_a_corner_radius_reports_none() {
        // A simple pad whose Block 5 exists but is shorter than both the legacy
        // (289-byte) and canonical (596-byte) layouts carries no corner radius.
        // Reading a byte anyway would invent a rounding percentage and turn a
        // plain rectangular land into a rounded one on the next save.
        let data = pad_record(&pad_geometry(52), Some(&[0_u8; 100]));
        let (pad, _) = parse_pad(&data, 0).unwrap();

        assert!(pad.corner_radius_percent.is_none());
        assert_eq!(pad.hole_shape, HoleShape::Round);
        assert!(pad.hole_slot_length.abs() < EPS);
    }

    #[test]
    fn pad_legacy_block5_reads_the_corner_radius_at_offset_288() {
        // Pre-596-byte libraries keep the corner radius at Block 5 offset 288.
        // Losing that path squares off every rounded-rectangle land in an older
        // library — the land grows into its neighbour's clearance and the board
        // fails DRC only after the change has been saved.
        let mut per_layer = vec![0_u8; 289];
        per_layer[288] = 25;
        let mut g = pad_geometry(52);
        g[49] = 1; // shape id 1 is Round AND RoundedRectangle on disk
        let data = pad_record(&g, Some(&per_layer));
        let (pad, _) = parse_pad(&data, 0).unwrap();

        assert_eq!(pad.corner_radius_percent, Some(25));
        assert_eq!(
            pad.shape,
            PadShape::RoundedRectangle,
            "a 1-99% corner radius disambiguates shape id 1"
        );
    }

    #[test]
    fn pad_size_shape_block_supplies_hole_shape_slot_length_and_corner_radius() {
        // The 596-byte size/shape block is where a slotted hole lives (@262
        // shape, @263 length, @267 rotation) and where the modern corner radius
        // sits (@564). Dropping these turns a milled slot back into a round
        // drill: the fab drills a hole the connector's pin physically cannot
        // enter, and the error is only discovered at assembly.
        let mut per_layer = vec![0_u8; 596];
        per_layer[262] = 2; // Slot
        per_layer[263..267].copy_from_slice(&30_000_i32.to_le_bytes());
        per_layer[267..275].copy_from_slice(&45.0_f64.to_le_bytes());
        per_layer[564] = 40;
        let mut g = pad_geometry(52);
        g[49] = 1;
        let data = pad_record(&g, Some(&per_layer));
        let (pad, _) = parse_pad(&data, 0).unwrap();

        assert_eq!(pad.hole_shape, HoleShape::Slot);
        assert!((pad.hole_slot_length - to_mm(30_000)).abs() < EPS);
        assert!((pad.hole_rotation - 45.0).abs() < EPS);
        assert_eq!(pad.corner_radius_percent, Some(40));
    }

    #[test]
    fn pad_unknown_hole_shape_id_falls_back_to_round() {
        // @262 is a raw byte from the file. An unrecognised value must read as a
        // round hole rather than panicking or being carried through as a slot of
        // unknown length.
        let mut per_layer = vec![0_u8; 596];
        per_layer[262] = 0xAA;
        let data = pad_record(&pad_geometry(52), Some(&per_layer));
        let (pad, _) = parse_pad(&data, 0).unwrap();

        assert_eq!(pad.hole_shape, HoleShape::Round);
    }

    #[test]
    fn full_stack_pad_reinterprets_round_plus_corner_radius_as_rounded_rectangle() {
        // Shape id 1 means BOTH Round and RoundedRectangle on disk; the
        // per-layer corner radius is the only discriminator. Regressing this
        // rewrites every rounded per-layer land as a full circle, shrinking the
        // land area below the IPC pattern and starving the joint of solder.
        let mut per_layer = vec![0_u8; 576];
        for i in 0..32_usize {
            per_layer[i * 8..i * 8 + 4].copy_from_slice(&500_000_i32.to_le_bytes());
            per_layer[i * 8 + 4..i * 8 + 8].copy_from_slice(&300_000_i32.to_le_bytes());
            per_layer[256 + i] = 1; // Round on disk
            per_layer[320 + i * 8..320 + i * 8 + 4].copy_from_slice(&1_000_i32.to_le_bytes());
            per_layer[320 + i * 8 + 4..320 + i * 8 + 8].copy_from_slice(&2_000_i32.to_le_bytes());
        }
        per_layer[288] = 50; // layer 0: rounded 50%
        per_layer[289] = 0; // layer 1: a true circle
        per_layer[290] = 200; // layer 2: out of range, must clamp

        let mut g = pad_geometry(63);
        g[62] = 2; // FullStack
        let data = pad_record(&g, Some(&per_layer));
        let (pad, _) = parse_pad(&data, 0).unwrap();

        assert_eq!(pad.stack_mode, PadStackMode::FullStack);
        let shapes = pad.per_layer_shapes.expect("full stack carries shapes");
        assert_eq!(shapes[0], PadShape::RoundedRectangle);
        assert_eq!(shapes[1], PadShape::Round);
        assert_eq!(
            shapes[2],
            PadShape::Round,
            "a radius clamped to 100% is a full round, not a rounded rectangle"
        );
        let radii = pad
            .per_layer_corner_radii
            .expect("full stack carries radii");
        assert_eq!(
            radii[2], 100,
            "an out-of-range radius clamps to 100, not 200"
        );
        assert_eq!(pad.corner_radius_percent, Some(50));

        let sizes = pad.per_layer_sizes.expect("full stack carries sizes");
        assert_eq!(sizes.len(), 32);
        assert!((sizes[0].0 - to_mm(500_000)).abs() < EPS);
        assert!((sizes[31].1 - to_mm(300_000)).abs() < EPS);

        let offsets = pad
            .per_layer_offsets
            .expect("a 576-byte block carries offsets");
        assert_eq!(offsets.len(), 32);
        assert!((offsets[3].1 - to_mm(2_000)).abs() < EPS);
    }

    #[test]
    fn per_layer_data_absent_or_short_reports_no_layers_at_all() {
        // A full-stack pad whose Block 5 is missing or truncated must report no
        // per-layer geometry rather than a half-decoded stack. A partly-read
        // table would silently resize the inner-layer lands, which nothing in
        // the 2D footprint view would reveal.
        let (radius, sizes, shapes, radii, offsets) = parse_per_layer_data(None);
        assert!(radius.is_none() && sizes.is_none());
        assert!(shapes.is_none() && radii.is_none() && offsets.is_none());

        // One byte below the 320-byte minimum is still "no data", not a partial
        // parse of the 39 size entries that happen to fit.
        let short = vec![0_u8; 319];
        let (radius, sizes, shapes, radii, offsets) = parse_per_layer_data(Some(&short));
        assert!(radius.is_none() && sizes.is_none());
        assert!(shapes.is_none() && radii.is_none() && offsets.is_none());
    }

    #[test]
    fn per_layer_data_without_the_offset_table_still_decodes_sizes_and_shapes() {
        // The 32 x/y offset entries at @320 are optional. A 320-byte block must
        // yield sizes/shapes with `offsets = None` — inventing zero offsets
        // would re-centre every per-layer land on the next write.
        let mut data = vec![0_u8; 320];
        data[0..4].copy_from_slice(&500_000_i32.to_le_bytes());
        data[4..8].copy_from_slice(&300_000_i32.to_le_bytes());
        data[256] = 2; // Rectangle
        let (radius, sizes, shapes, radii, offsets) = parse_per_layer_data(Some(&data));

        assert!(radius.is_none(), "a zero corner radius is not a rounding");
        assert_eq!(sizes.expect("sizes").len(), 32);
        assert_eq!(shapes.expect("shapes")[0], PadShape::Rectangle);
        assert_eq!(radii.expect("radii").len(), 32);
        assert!(offsets.is_none(), "no offset table below 576 bytes");
    }

    // -------------------------------------------------------------------------
    // Via
    // -------------------------------------------------------------------------

    #[test]
    fn via_missing_a_named_field_is_rejected_by_that_field() {
        // Below 31 bytes the from/to layer bytes are missing. Accepted silently,
        // the via would span "layer 0 to layer 0" — a via that stitches nothing,
        // leaving the net it was placed for open on the finished board.
        for (len, expected) in [
            (14, "failed to read Via x"),
            (18, "failed to read Via y"),
            (22, "failed to read Via diameter"),
            (26, "failed to read Via hole size"),
            (29, "failed to read Via from layer"),
            (30, "failed to read Via to layer"),
        ] {
            let data = block(&vec![0_u8; len]);
            let err = parse_via(&data, 0).unwrap_err();
            assert!(
                err.to_string().contains(expected),
                "{len} bytes: expected {expected:?}, got: {err}"
            );
        }
    }

    #[test]
    fn via_with_only_the_mandatory_31_bytes_uses_template_defaults() {
        // A via record that stops after the layer span must fall back to the
        // Altium template's thermal/plane constants. Zeros here mean a 0 mm
        // relief gap and 0 mm plane clearance: the via would short to every
        // internal plane it passes through.
        let data = block(&via_block(31));
        let (via, next) = parse_via(&data, 0).unwrap();

        assert_eq!(next, data.len());
        assert_eq!(via.from_layer, Layer::TopLayer);
        assert_eq!(via.to_layer, Layer::BottomLayer);
        assert!((via.diameter - to_mm(240_000)).abs() < EPS);
        assert!((via.hole_size - to_mm(120_000)).abs() < EPS);
        assert_eq!(
            via.power_plane_connect_style,
            PowerPlaneConnectStyle::Relief
        );
        assert!((via.thermal_relief_gap - 0.254).abs() < EPS);
        assert_eq!(via.thermal_relief_conductors, 4);
        assert!((via.thermal_relief_width - 0.254).abs() < EPS);
        assert!((via.power_plane_relief_expansion - 0.508).abs() < EPS);
        assert!((via.power_plane_clearance - 0.508).abs() < EPS);
        assert!(via.paste_mask_expansion.abs() < EPS);
        assert!(via.solder_mask_expansion.abs() < EPS);
        assert_eq!(via.solder_mask_expansion_mode, MaskExpansionMode::None);
        assert!(via.solder_mask_expansion_back.is_none());
        assert_eq!(via.diameter_stack_mode, ViaStackMode::Simple);
        assert!(via.per_layer_diameters.is_none());
        assert!(via.hole_positive_tolerance.is_none());
        assert!(via.hole_negative_tolerance.is_none());
    }

    #[test]
    fn via_stack_modes_decode_and_carry_their_per_layer_diameters() {
        // A TopMiddleBottom / FullStack via takes its inner-layer pads from the
        // 32-entry table at @75. Collapsing to Simple flattens every inner
        // annular ring to the outer diameter, so drill/annular-ring DRC passes
        // on a board that is actually out of spec at the fab.
        for (id, mode) in [
            (1_u8, ViaStackMode::TopMiddleBottom),
            (2_u8, ViaStackMode::FullStack),
        ] {
            let mut b = via_block(203);
            b[74] = id;
            for i in 0..32_usize {
                let value = 300_000 + i32::try_from(i).expect("index fits");
                b[75 + i * 4..79 + i * 4].copy_from_slice(&value.to_le_bytes());
            }
            let (via, _) = parse_via(&block(&b), 0).unwrap();

            assert_eq!(via.diameter_stack_mode, mode);
            let diameters = via.per_layer_diameters.expect("non-simple stack");
            assert_eq!(diameters.len(), 32);
            assert!((diameters[0] - to_mm(300_000)).abs() < EPS);
            assert!((diameters[31] - to_mm(300_031)).abs() < EPS);
        }

        // An unrecognised stack id degrades to Simple instead of indexing off
        // the end of the mode table.
        let mut b = via_block(203);
        b[74] = 0x5A;
        let (via, _) = parse_via(&block(&b), 0).unwrap();
        assert_eq!(via.diameter_stack_mode, ViaStackMode::Simple);
        assert!(via.per_layer_diameters.is_none());
    }

    #[test]
    fn non_simple_via_without_the_per_layer_table_reports_no_diameters() {
        // The stack-mode byte and the 128-byte diameter table can disagree in a
        // truncated file. Without the table there is nothing to report — reading
        // past the block would hand the caller neighbouring record bytes as
        // annular diameters.
        let mut b = via_block(80); // has @74 but not the full 75..203 table
        b[74] = 2; // FullStack
        let (via, _) = parse_via(&block(&b), 0).unwrap();

        assert_eq!(via.diameter_stack_mode, ViaStackMode::FullStack);
        assert!(via.per_layer_diameters.is_none());
    }

    // -------------------------------------------------------------------------
    // Track / Arc / Fill
    // -------------------------------------------------------------------------

    #[test]
    fn track_missing_a_named_field_is_rejected_by_that_field() {
        // Below 33 bytes the width field is missing. A silently-accepted track
        // would be 0 mm wide: invisible in the library editor and absent from
        // the Gerber output, even though the footprint "loaded fine".
        for (len, expected) in [
            (0, "failed to read Track layer"),
            (14, "failed to read Track x1 coordinate"),
            (18, "failed to read Track y1 coordinate"),
            (22, "failed to read Track x2 coordinate"),
            (26, "failed to read Track y2 coordinate"),
            (32, "failed to read Track width"),
        ] {
            let data = block(&vec![0_u8; len]);
            let err = parse_track(&data, 0).unwrap_err();
            assert!(
                err.to_string().contains(expected),
                "{len} bytes: expected {expected:?}, got: {err}"
            );
        }
    }

    #[test]
    fn track_without_its_extended_tail_reports_no_mask_or_keepout_overrides() {
        // A 33-byte track has no solder-mask expansion (@35) or keepout byte
        // (@45). Surfacing zeros as real values would write an explicit 0 mm
        // mask expansion back, overriding the design rule and tenting the
        // track's neighbours on the next save.
        let mut b = vec![0_u8; 33];
        write_common_header(&mut b, 33);
        b[13..17].copy_from_slice(&100_000_i32.to_le_bytes());
        b[17..21].copy_from_slice(&200_000_i32.to_le_bytes());
        b[21..25].copy_from_slice(&300_000_i32.to_le_bytes());
        b[25..29].copy_from_slice(&400_000_i32.to_le_bytes());
        b[29..33].copy_from_slice(&50_000_i32.to_le_bytes());
        let (track, next) = parse_track(&block(&b), 0).unwrap();

        assert_eq!(next, block(&b).len());
        assert!((track.x1 - to_mm(100_000)).abs() < EPS);
        assert!((track.width - to_mm(50_000)).abs() < EPS);
        assert!(track.solder_mask_expansion.is_none());
        assert!(track.keepout_restrictions.is_none());
        assert_eq!(track.layer, Layer::TopOverlay);
    }

    #[test]
    fn arc_missing_a_named_field_is_rejected_by_that_field() {
        // Below 45 bytes the width at @41 is missing, so a silently-accepted arc
        // draws with zero width — a courtyard or polarity arc that disappears
        // from the silkscreen without any load error.
        // The two angle doubles at @25 and @33 are not in this list on purpose:
        // they carry `unwrap_or` defaults (0 and 360, i.e. a full circle) rather
        // than failing, and the required width at @41 sits past both, so any
        // block long enough to reach the width has already supplied them.
        for (len, expected) in [
            (0, "failed to read Arc layer"),
            (14, "failed to read Arc x coordinate"),
            (18, "failed to read Arc y coordinate"),
            (22, "failed to read Arc radius"),
            (44, "failed to read Arc width"),
        ] {
            let data = block(&vec![0_u8; len]);
            let err = parse_arc(&data, 0).unwrap_err();
            assert!(
                err.to_string().contains(expected),
                "{len} bytes: expected {expected:?}, got: {err}"
            );
        }
    }

    #[test]
    fn arc_without_its_extended_tail_reports_no_mask_or_keepout_overrides() {
        // Same contract as Track: absent @47 / @56 must stay `None` so a
        // from-scratch arc round-trips without gaining an explicit 0 mm mask
        // expansion that overrides the board's design rule.
        let mut b = vec![0_u8; 45];
        write_common_header(&mut b, 33);
        b[13..17].copy_from_slice(&100_000_i32.to_le_bytes());
        b[17..21].copy_from_slice(&200_000_i32.to_le_bytes());
        b[21..25].copy_from_slice(&250_000_i32.to_le_bytes());
        b[25..33].copy_from_slice(&30.0_f64.to_le_bytes());
        b[33..41].copy_from_slice(&300.0_f64.to_le_bytes());
        b[41..45].copy_from_slice(&50_000_i32.to_le_bytes());
        let (arc, _) = parse_arc(&block(&b), 0).unwrap();

        assert!((arc.radius - to_mm(250_000)).abs() < EPS);
        assert!((arc.start_angle - 30.0).abs() < EPS);
        assert!((arc.end_angle - 300.0).abs() < EPS);
        assert!(arc.solder_mask_expansion.is_none());
        assert!(arc.keepout_restrictions.is_none());
    }

    #[test]
    fn fill_missing_a_named_field_is_rejected_by_that_field() {
        // Below 37 bytes the rotation double at @29 is missing. A silently
        // accepted fill would be un-rotated: a rotated copper or keepout
        // rectangle would land axis-aligned, covering a different set of pads
        // than the designer drew.
        for (len, expected) in [
            (0, "failed to read Fill layer"),
            (14, "failed to read Fill x1 coordinate"),
            (18, "failed to read Fill y1 coordinate"),
            (22, "failed to read Fill x2 coordinate"),
            (26, "failed to read Fill y2 coordinate"),
            (36, "failed to read Fill rotation"),
        ] {
            let data = block(&vec![0_u8; len]);
            let err = parse_fill(&data, 0).unwrap_err();
            assert!(
                err.to_string().contains(expected),
                "{len} bytes: expected {expected:?}, got: {err}"
            );
        }
    }

    // -------------------------------------------------------------------------
    // Text
    // -------------------------------------------------------------------------

    #[test]
    fn text_missing_a_named_field_is_rejected_by_that_field() {
        // Below 25 bytes the text height at @21 is missing. A zero-height
        // silkscreen string is invisible in the editor and in the fab output,
        // so the part would ship with no designator or polarity marking.
        for (len, expected) in [
            (0, "failed to read Text layer"),
            (14, "failed to read Text x coordinate"),
            (18, "failed to read Text y coordinate"),
            (24, "failed to read Text height"),
        ] {
            let data = block(&vec![0_u8; len]);
            let err = parse_text(&data, 0, None).unwrap_err();
            assert!(
                err.to_string().contains(expected),
                "{len} bytes: expected {expected:?}, got: {err}"
            );
        }
    }

    #[test]
    fn text_with_only_the_mandatory_25_bytes_uses_template_defaults() {
        // A legacy/truncated text record must land on the template defaults:
        // stroke font, 0 degrees, Arial, bottom-left anchor. Guessing TrueType
        // or a non-zero rotation here would rotate and re-typeset every
        // silkscreen designator in an old library the moment it is re-saved.
        let data = block(&text_geometry(25));
        let (text, next) = parse_text(&data, 0, None).unwrap();

        assert_eq!(next, data.len());
        assert_eq!(text.kind, TextKind::Stroke);
        assert!(text.rotation.abs() < EPS);
        assert!(text.stroke_font.is_none());
        assert!(text.stroke_width.is_none());
        assert_eq!(text.font_name, "Arial");
        assert_eq!(text.justification, TextJustification::BottomLeft);
        assert!(!text.mirror);
        assert!(!text.bold);
        assert!(!text.italic);
        assert!(!text.is_comment);
        assert!(!text.is_designator);
        assert!(!text.is_inverted);
        assert!(text.inverted_border.is_none());
        assert!(!text.use_inverted_rectangle);
        assert!(text.inverted_rect_width.is_none());
        assert!(text.inverted_rect_height.is_none());
        assert!(
            text.text.is_empty(),
            "no content block and no inline marker means empty text"
        );
    }

    #[test]
    fn text_kind_at_offset_160_governs_whether_a_stroke_font_is_surfaced() {
        // @160 is the authoritative text kind and @25 the stroke-font index. A
        // non-default stroke font must survive the read; losing it silently
        // swaps the silkscreen typeface, and the different glyph widths push
        // the string outside the courtyard it was fitted to.
        let mut b = text_geometry(161);
        b[25..27].copy_from_slice(&3_u16.to_le_bytes()); // Serif
        let (text, _) = parse_text(&block(&b), 0, None).unwrap();
        assert_eq!(text.kind, TextKind::Stroke);
        assert_eq!(text.stroke_font, Some(StrokeFont::Serif));

        // The default stroke index (1) stays implicit so the record round-trips.
        let mut b = text_geometry(161);
        b[25..27].copy_from_slice(&1_u16.to_le_bytes());
        let (text, _) = parse_text(&block(&b), 0, None).unwrap();
        assert!(text.stroke_font.is_none());

        // A TrueType text has no stroke font at all, whatever @25 holds.
        let mut b = text_geometry(161);
        b[25..27].copy_from_slice(&3_u16.to_le_bytes());
        b[160] = 1;
        let (text, _) = parse_text(&block(&b), 0, None).unwrap();
        assert_eq!(text.kind, TextKind::TrueType);
        assert!(text.stroke_font.is_none());

        // BarCode is the third documented kind.
        let mut b = text_geometry(161);
        b[160] = 2;
        let (text, _) = parse_text(&block(&b), 0, None).unwrap();
        assert_eq!(text.kind, TextKind::BarCode);

        // An unrecognised kind byte degrades to Stroke rather than panicking.
        let mut b = text_geometry(161);
        b[160] = 0x5A;
        let (text, _) = parse_text(&block(&b), 0, None).unwrap();
        assert_eq!(text.kind, TextKind::Stroke);
    }

    #[test]
    fn text_font_name_field_decodes_utf16_and_falls_back_to_arial() {
        // The @46 font-name field is 64 bytes of little-endian UTF-16. An
        // all-zero or absent field means "template default": decoding it as an
        // empty name writes an empty font back, and Altium then renders the
        // string in a substitute face at a different size.
        let mut b = text_geometry(161);
        for (i, unit) in "Times".encode_utf16().enumerate() {
            b[46 + i * 2..48 + i * 2].copy_from_slice(&unit.to_le_bytes());
        }
        let (text, _) = parse_text(&block(&b), 0, None).unwrap();
        assert_eq!(text.font_name, "Times");

        let (text, _) = parse_text(&block(&text_geometry(161)), 0, None).unwrap();
        assert_eq!(text.font_name, "Arial", "an all-zero field is the default");

        assert_eq!(
            read_text_font_name(&text_geometry(46)),
            "Arial",
            "a block that ends before @110 has no font field at all"
        );
    }

    #[test]
    fn justification_byte_maps_onto_the_shared_three_by_three_anchor_grid() {
        // Altium's column-major anchor byte decides where the text box hangs off
        // its (x, y). Mis-mapping one value shifts a designator by up to a full
        // text box — silkscreen printed straight over a pad, which the fab will
        // happily manufacture.
        for (id, expected) in [
            (1_u8, TextJustification::TopLeft),
            (2, TextJustification::MiddleLeft),
            (3, TextJustification::BottomLeft),
            (4, TextJustification::TopCenter),
            (5, TextJustification::MiddleCenter),
            (6, TextJustification::BottomCenter),
            (7, TextJustification::TopRight),
            (8, TextJustification::MiddleRight),
            (9, TextJustification::BottomRight),
            (0, TextJustification::BottomLeft), // manual: no member in the grid
            (0xFF, TextJustification::BottomLeft), // corrupt byte
        ] {
            assert_eq!(pcb_justification_from_id(id), expected, "anchor id {id}");
        }
    }

    #[test]
    fn empty_text_content_block_falls_back_to_the_inline_designator_marker() {
        // Designator and comment texts carry their content as an inline
        // ".Designator" / ".Comment" marker inside the geometry block, with an
        // EMPTY content block. If the empty block won, every footprint would
        // lose its designator text and the board would assemble unlabelled.
        let mut b = text_geometry(200);
        b[170..181].copy_from_slice(b".Designator");
        let mut data = block(&b);
        data.extend_from_slice(&block(&[0x00])); // zero-length Pascal string
        let (text, next) = parse_text(&data, 0, None).unwrap();
        assert_eq!(text.text, ".Designator");
        assert_eq!(
            next,
            data.len(),
            "the empty content block is still consumed"
        );

        let mut b = text_geometry(200);
        b[170..178].copy_from_slice(b".Comment");
        let (text, _) = parse_text(&block(&b), 0, None).unwrap();
        assert_eq!(text.text, ".Comment");
    }

    #[test]
    fn text_content_resolves_a_wide_strings_index_from_geometry_offset_115() {
        // Altium stores long/unicode text out of line in /WideStrings, leaving
        // only a u16 index at @115. Missing it leaves the primitive blank, so
        // the part marking silently vanishes from the silkscreen.
        let mut ws = WideStrings::new();
        ws.insert(7, "PLACE COMPONENT HERE".to_string());

        let mut b = text_geometry(200);
        b[115..117].copy_from_slice(&7_u16.to_le_bytes());
        let (text, _) = parse_text(&block(&b), 0, Some(&ws)).unwrap();
        assert_eq!(text.text, "PLACE COMPONENT HERE");

        // An index with no matching entry yields empty text, not a panic.
        let mut b = text_geometry(200);
        b[115..117].copy_from_slice(&99_u16.to_le_bytes());
        let (text, _) = parse_text(&block(&b), 0, Some(&ws)).unwrap();
        assert!(text.text.is_empty());

        // A block too short to hold @115 is handled by the bounds-checked read.
        assert!(extract_text_from_block(&text_geometry(116), Some(&ws)).is_empty());
    }

    #[test]
    fn find_ascii_in_block_never_reads_past_a_block_shorter_than_the_pattern() {
        // The inline-marker scan runs over every text record, including
        // truncated ones. A missing length guard here is an out-of-bounds slice
        // — an outright panic while opening a library.
        assert_eq!(find_ascii_in_block(b"xx.Comment", ".Comment"), Some(2));
        assert_eq!(find_ascii_in_block(b".Comment", ".Comment"), Some(0));
        assert_eq!(find_ascii_in_block(b"short", ".Designator"), None);
        assert_eq!(find_ascii_in_block(&[], ".Comment"), None);
    }

    #[test]
    fn text_content_block_is_literal_never_a_wide_strings_index() {
        // Regression for #309. Block 1 holds the text itself, so all-digit
        // content — pin-1 markers, numeric value legends — must survive
        // verbatim even when the component has a /WideStrings table. The entry
        // to use is named by the index at geometry offset 115 (see
        // docs/PCBLIB_FORMAT.md), never inferred from the content.
        let mut ws = WideStrings::new();
        ws.insert(1, "TOTALLY UNRELATED".to_string());
        ws.insert(2, "ALSO UNRELATED".to_string());

        // Numeric silkscreen survives verbatim, with and without a table.
        for (content, table) in [
            ("1", Some(&ws)),
            ("2", Some(&ws)),
            ("42", Some(&ws)),
            ("1", None),
        ] {
            let mut record = block(&text_geometry(200));
            record.extend_from_slice(&block(&pascal(content)));
            let (text, _) = parse_text(&record, 0, table).expect("text parses");
            assert_eq!(
                text.text, content,
                "content {content:?} must be taken literally"
            );
        }

        // Ordinary and special content are unaffected.
        for content in ["R1", "10uF", ".Designator", ".Comment"] {
            let mut record = block(&text_geometry(200));
            record.extend_from_slice(&block(&pascal(content)));
            let (text, _) = parse_text(&record, 0, Some(&ws)).expect("text parses");
            assert_eq!(text.text, content);
        }
    }

    #[test]
    fn wide_strings_still_resolve_when_the_content_block_is_empty() {
        // The out-of-line path must keep working: an empty block 1 means the
        // text lives in /WideStrings under the index at geometry offset 115.
        // Removing the content heuristic must not disturb this.
        let mut ws = WideStrings::new();
        ws.insert(3, "PLACE COMPONENT HERE".to_string());

        let mut geom = text_geometry(200);
        geom[115..117].copy_from_slice(&3_u16.to_le_bytes());
        let mut record = block(&geom);
        record.extend_from_slice(&block(&pascal("")));

        let (text, _) = parse_text(&record, 0, Some(&ws)).expect("text parses");
        assert_eq!(text.text, "PLACE COMPONENT HERE");
    }

    // -------------------------------------------------------------------------
    // Region
    // -------------------------------------------------------------------------

    #[test]
    fn region_with_an_unreadable_outer_block_is_rejected() {
        // The block length prefix comes straight off disk. One that runs past
        // the end of the stream must fail before any field is decoded, rather
        // than yielding a region built from whatever bytes happen to follow.
        let data = 1000_u32.to_le_bytes();
        let err = parse_region(&data, 0).unwrap_err();
        assert!(
            err.to_string()
                .contains("failed to read Region properties block"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn region_missing_a_named_field_is_rejected_by_that_field() {
        // Below 22 bytes there is no parameter-length field, so the vertex
        // outline cannot be located at all. Accepting it would produce a region
        // with no vertices — a copper pour that silently disappears from the
        // footprint, leaving the net it fed unconnected.
        for (len, expected) in [
            (0, "failed to read Region layer"),
            (21, "failed to read Region parameter string length"),
        ] {
            let data = block(&vec![0_u8; len]);
            let err = parse_region(&data, 0).unwrap_err();
            assert!(
                err.to_string().contains(expected),
                "{len} bytes: expected {expected:?}, got: {err}"
            );
        }
    }

    #[test]
    fn region_parameter_length_past_the_end_of_the_block_is_rejected() {
        // The declared parameter length is file-controlled. Trusting one that
        // overruns the block would read the vertex outline from unrelated
        // bytes: a copper region drawn somewhere else entirely, shorting nets
        // on the manufactured board.
        let mut b = region_block("", 0, &[&[]]);
        b[18..22].copy_from_slice(&5000_u32.to_le_bytes());
        let err = parse_region(&block(&b), 0).unwrap_err();
        assert!(
            err.to_string().contains("Region parameter block truncated"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn region_without_a_vertex_count_is_rejected() {
        // A block that ends exactly at the parameter string has no outline. The
        // guard must fire here rather than letting the count read fall through
        // to the next primitive's bytes.
        let b = region_block("KIND=0", 0, &[]);
        let err = parse_region(&block(&b), 0).unwrap_err();
        assert!(
            err.to_string().contains("too short for vertex count"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn region_whose_declared_vertex_count_overruns_the_block_is_rejected() {
        // A vertex count larger than the data behind it is the classic
        // corrupt-record shape. It must be a clean parse error — never a
        // partially-read polygon (a mis-shaped pour) and never an oversized
        // allocation driven by an attacker-chosen count.
        let mut b = region_block("", 0, &[]);
        b.extend_from_slice(&5_u32.to_le_bytes()); // claims 5, supplies none
        let err = parse_region(&block(&b), 0).unwrap_err();
        assert!(
            err.to_string()
                .contains("Region block too short for Region vertex with 5 vertices"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn region_hole_contour_running_past_the_end_of_the_block_is_rejected() {
        // @14 declares how many hole contours follow the outline. A count that
        // outruns the data must error instead of reading the NEXT record's
        // bytes as a cutout — a phantom hole punched through a copper pour,
        // which manufactures as a real void in the plane.
        let b = region_block("", 1, &[&[]]); // one hole promised, none present
        let err = parse_region(&block(&b), 0).unwrap_err();
        assert!(
            err.to_string()
                .contains("failed to read Region hole 0 count"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn region_decodes_its_outline_holes_and_typed_parameters() {
        // The happy path for a region WITH a cutout, plus the typed parameter
        // keys. A dropped hole contour fills in a deliberate void (shorting to
        // whatever the void was clearing); a dropped KIND turns a board cutout
        // into a copper pour.
        let outline: &[(i32, i32)] = &[(0, 0), (100_000, 0), (100_000, 100_000)];
        let hole: &[(i32, i32)] = &[(10_000, 10_000), (20_000, 10_000), (20_000, 20_000)];
        let b = region_block(
            "KIND=1|NAME=cutout|SUBPOLYINDEX=2|UNIONINDEX=3|ISSHAPEBASED=TRUE|NET=5|KEEPOUT=TRUE",
            1,
            &[outline, hole],
        );
        let (region, next) = parse_region(&block(&b), 0).unwrap();

        assert_eq!(next, block(&b).len());
        assert_eq!(region.kind, RegionKind::Cutout);
        assert_eq!(region.name, "cutout");
        assert_eq!(region.sub_poly_index, 2);
        assert_eq!(region.union_index, 3);
        assert!(region.is_shape_based);
        assert_eq!(
            region.net_index, 5,
            "the NET parameter overrides the header word"
        );
        assert_eq!(region.vertices.len(), 3);
        assert_eq!(region.holes.len(), 1);
        assert_eq!(region.holes[0].len(), 3);
        assert!((region.vertices[1].x - to_mm(100_000)).abs() < EPS);
        assert!(
            region
                .additional_parameters
                .iter()
                .any(|(k, v)| k == "KEEPOUT" && v == "TRUE"),
            "unmodelled keys must survive a read-modify-write"
        );
    }

    #[test]
    fn region_with_an_unknown_kind_preserves_the_raw_value() {
        // Altium may write a KIND we do not model. Collapsing it to Copper would
        // silently convert, say, a cavity definition into a plated pour on the
        // next save.
        let b = region_block("KIND=7", 0, &[&[]]);
        let (region, _) = parse_region(&block(&b), 0).unwrap();
        assert_eq!(region.kind, RegionKind::Other(7));
    }

    // -------------------------------------------------------------------------
    // ComponentBody
    // -------------------------------------------------------------------------

    #[test]
    fn component_body_with_an_unreadable_block_is_rejected() {
        // Same contract as Region: a length prefix past the end of the stream
        // must fail immediately, not produce a body whose 3D model reference is
        // assembled from unrelated bytes.
        let data = 1000_u32.to_le_bytes();
        let err = parse_component_body(&data, 0).unwrap_err();
        assert!(
            err.to_string()
                .contains("failed to read ComponentBody block"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn component_body_outline_stops_cleanly_on_a_truncated_block() {
        // The 2D outline is `[u32 count][count x (f64, f64)]` behind the
        // parameter block. A truncated tail must yield only the vertices
        // actually present; reading past the record would draw this body's
        // outline over a neighbouring primitive's bytes.
        assert!(
            parse_component_body_outline(&[0_u8; 21]).is_empty(),
            "no parameter-length field at all"
        );

        let mut b = vec![0_u8; 22];
        b[18..22].copy_from_slice(&0_u32.to_le_bytes());
        assert!(
            parse_component_body_outline(&b).is_empty(),
            "parameter length present but no vertex count behind it"
        );

        let mut b = vec![0_u8; 22];
        b[18..22].copy_from_slice(&0_u32.to_le_bytes());
        b.extend_from_slice(&2_u32.to_le_bytes()); // claims two vertices
        b.extend_from_slice(&100_000.0_f64.to_le_bytes());
        b.extend_from_slice(&200_000.0_f64.to_le_bytes());
        b.extend_from_slice(&300_000.0_f64.to_le_bytes()); // x only; y is missing
        let outline = parse_component_body_outline(&b);
        assert_eq!(outline.len(), 1, "the half-present vertex is dropped");
        let expected_x = 100_000.0 * INTERNAL_UNITS_TO_MM;
        let expected_y = 200_000.0 * INTERNAL_UNITS_TO_MM;
        assert!((outline[0].0 - expected_x).abs() < EPS);
        assert!((outline[0].1 - expected_y).abs() < EPS);
    }

    #[test]
    fn a_component_body_reads_its_layer_from_the_header_byte() {
        // The header byte is the only layer source, so it must cover the whole
        // mechanical range on its own — a 3D body silently jumping layers breaks
        // the enclosure clearance checks that layer feeds. `V7_LAYER` in the
        // parameter string is written but not read back: it always agrees, and
        // the byte reaches layers the token's old MECHANICAL2-7 mapping did not.
        for (id, expected, token) in [
            (57_u8, Layer::Mechanical1, "MECHANICAL1"),
            (62, Layer::Top3DBody, "MECHANICAL6"),
            (69, Layer::Mechanical13, "MECHANICAL13"),
            (201, Layer::Mechanical32, "MECHANICAL32"),
        ] {
            let mut props = vec![0_u8; 22];
            write_common_header(&mut props, id);
            let params = format!("V7_LAYER={token}|NAME=body\0");
            props[18..22].copy_from_slice(
                &u32::try_from(params.len())
                    .expect("fixture fits")
                    .to_le_bytes(),
            );
            props.extend_from_slice(params.as_bytes());

            let (body, _) = parse_component_body(&block(&props), 0).expect("a whole body");
            assert_eq!(body.layer, expected, "layer id {id}");
            assert_eq!(body.name, "body");
        }
    }

    #[test]
    fn mil_values_parse_with_and_without_their_suffix() {
        // Body heights and arc resolutions are stored as mil strings. A failed
        // parse must read as 0 mm rather than propagating a NaN into the
        // component's 3D extrusion range.
        let expected = 15.748 * MM_PER_MIL;
        assert!((parse_mil_value(Some("15.748mil")) - expected).abs() < 1e-9);
        assert!(parse_mil_value(Some("0mil")).abs() < EPS);
        let expected = 100.0 * MM_PER_MIL;
        assert!((parse_mil_value(Some(" 100 mil")) - expected).abs() < 1e-9);
        assert!(parse_mil_value(None).abs() < EPS);
        assert!(parse_mil_value(Some("not-a-number")).abs() < EPS);
    }

    #[test]
    fn the_v7_layer_id_names_a_mechanical_layer_past_the_legacy_sixteen() {
        // Byte 72 is Mechanical 16, the last the header byte can hold; a
        // track on Mechanical 20 carries 72 there and `0x0102_0014` in the
        // V7 id at @41, which decides. Nothing is carried: the writer stores
        // Mechanical 20 the same way.
        let mut b = vec![0_u8; 46];
        write_common_header(&mut b, 72);
        b[29..33].copy_from_slice(&50_000_i32.to_le_bytes());
        b[41..45].copy_from_slice(&0x0102_0014_u32.to_le_bytes());
        let (track, _) = parse_track(&block(&b), 0).expect("a whole track");
        assert_eq!(track.layer, Layer::Mechanical20);
        assert_eq!(track.raw_layer_id, None);

        // A V7 id within the sixteen leaves the byte in charge.
        b[41..45].copy_from_slice(&0x0102_0010_u32.to_le_bytes());
        let (track, _) = parse_track(&block(&b), 0).expect("a whole track");
        assert_eq!(track.layer, Layer::Mechanical16);

        // A legacy-only record (no V7 id) is its byte.
        let (track, _) = parse_track(&block(&b[..33]), 0).expect("a whole track");
        assert_eq!(track.layer, Layer::Mechanical16);

        // The deferral is for mechanical bytes only: a copper byte with a
        // stray mechanical V7 id stays copper.
        write_common_header(&mut b, 1);
        b[41..45].copy_from_slice(&0x0102_0014_u32.to_le_bytes());
        let (track, _) = parse_track(&block(&b), 0).expect("a whole track");
        assert_eq!(track.layer, Layer::TopLayer);

        // A byte the table does not map is carried for the rewrite; the
        // Multi-Layer byte itself is not.
        for (byte, carried) in [(100, Some(100)), (74, None)] {
            write_common_header(&mut b, byte);
            let (track, _) = parse_track(&block(&b), 0).expect("a whole track");
            assert_eq!(track.layer, Layer::MultiLayer);
            assert_eq!(track.raw_layer_id, carried, "byte {byte}");
        }
    }

    #[test]
    fn a_v7_layer_token_names_a_region_or_body_layer_past_the_legacy_sixteen() {
        // Regions and bodies carry the layer as a `V7_LAYER` token instead of
        // an id; under byte 72 a `MECHANICAL20` token is the layer, and the
        // token is then derived, not carried.
        let mut b = vec![0_u8; 22];
        write_common_header(&mut b, 72);
        let params = b"V7_LAYER=MECHANICAL20|NAME=\0";
        b[18..22].copy_from_slice(&u32::try_from(params.len()).expect("len").to_le_bytes());
        b.extend_from_slice(params);
        b.extend_from_slice(&0_u32.to_le_bytes()); // no vertices
        let (region, _) = parse_region(&block(&b), 0).expect("a whole region");
        assert_eq!(region.layer, Layer::Mechanical20);
        assert_eq!(region.v7_layer, None, "the token follows from the layer");

        let mut props = b"V7_LAYER=MECHANICAL20|NAME=body\0".to_vec();
        let mut block0 = vec![72_u8, 0x0C, 0x00];
        block0.extend_from_slice(&[0xFF; 10]);
        block0.extend_from_slice(&[0; 5]);
        block0.extend_from_slice(&u32::try_from(props.len()).expect("len").to_le_bytes());
        block0.append(&mut props);
        block0.extend_from_slice(&0_u32.to_le_bytes());
        let (body, _) = parse_component_body(&block(&block0), 0).expect("a whole body");
        assert_eq!(body.layer, Layer::Mechanical20);
        assert_eq!(body.v7_layer, None);
        assert_eq!(body.raw_layer_id, None);
    }
}
