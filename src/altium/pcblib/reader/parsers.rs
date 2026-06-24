//! `PcbLib` reader: per-primitive binary parsers (pad/via/track/arc/text/region/fill/component-body).

#[allow(clippy::wildcard_imports)] // tightly-coupled reader split
use super::*;

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

    // Hole shape comes from the 596-byte size/shape block (offset 262) when
    // present; a plain simple pad (empty Block 5) has a round hole. Main-block
    // offset 61 is reserved in Altium's layout, so it is not used here.
    let hole_shape = per_layer_data
        .filter(|d| d.len() >= 596)
        .map_or(HoleShape::Round, |d| hole_shape_from_id(d[262]));

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
pub(super) fn parse_via(data: &[u8], offset: usize) -> ParseResult<Via> {
    // Altium writes a via as a single block: the 13-byte common header followed
    // by the 321-byte via SubRecord-1 (offsets 13-320). Mirror of `encode_via`
    // (#113); the old reader expected six pad-style blocks.
    let (block, next) = read_block(data, offset)
        .ok_or_else(|| AltiumError::parse_error(offset, "failed to read Via block"))?;

    if block.len() < 31 {
        return Err(AltiumError::parse_error(
            offset,
            format!(
                "Via block too short: {} bytes, expected at least 31",
                block.len()
            ),
        ));
    }

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
    let from_layer = layer_from_id(block[29]);
    let to_layer = layer_from_id(block[30]);

    // Extended SubRecord-1 fields (offsets 32-74). A short block falls back to
    // the same defaults the Via struct uses.
    let thermal_relief_gap = read_i32(block, 32).map_or(0.254, to_mm);
    let thermal_relief_conductors = block.get(36).copied().unwrap_or(4);
    let thermal_relief_width = read_i32(block, 38).map_or(0.254, to_mm);
    let solder_mask_expansion = read_i32(block, 54).map_or(0.0, to_mm);
    let solder_mask_expansion_manual = block.get(66).is_some_and(|&b| b != 0);
    let diameter_stack_mode = block
        .get(74)
        .map_or(ViaStackMode::Simple, |&b| via_stack_mode_from_id(b));

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
        solder_mask_expansion_manual,
        thermal_relief_gap,
        thermal_relief_conductors,
        thermal_relief_width,
        diameter_stack_mode,
        per_layer_diameters,
        unique_id: None,
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
        flags,
        unique_id: None,
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
        flags,
        unique_id: None,
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
/// [rotation:8 f64]              // Rotation angle (at offset 37)
/// [font_name:varies]            // Font name in UTF-16 (null-terminated)
/// [text_content:varies]         // Text content in UTF-16 or reference
/// ```
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

    if geometry_block.len() < 25 {
        return Err(AltiumError::parse_error(
            offset,
            format!(
                "Text geometry block too short: {} bytes, expected at least 25",
                geometry_block.len()
            ),
        ));
    }

    // Common header (13 bytes): layer at 0, Altium flag word at offsets 1-2.
    let layer_id = geometry_block[0];
    let layer = layer_from_id(layer_id);
    // Decode the lock/tenting/keepout flag word like every other primitive does,
    // rather than discarding it (the write side already encodes these correctly).
    let flags = read_flags(geometry_block);

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

    // Normal (non-inverted) text does not carry a justification field in this
    // record — it only exists inside the inverted-rectangle sub-block — so
    // default it rather than mis-read a byte inside the font-name field.
    let justification = TextJustification::default();

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
pub(super) fn resolve_text_content(content: &str, wide_strings: Option<&WideStrings>) -> String {
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
pub(super) fn extract_text_from_block(block: &[u8], wide_strings: Option<&WideStrings>) -> String {
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

    // A region is a single block — there is no trailing empty "Block 1". Altium
    // places the next record's type byte immediately after this block, so `current`
    // already points at the next record. (We previously read a spurious second block,
    // which against a real Altium region would mis-read the next record's bytes.)
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
pub(super) fn parse_fill(data: &[u8], offset: usize) -> ParseResult<Fill> {
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
/// A `ComponentBody` is a single size-prefixed block (matching `AltiumSharp` and
/// the `BODY_3D` golden libraries): the layer/flags header, a C-string
/// parameter block, then the 2D outline polygon — all within the one block.
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
    let block_str = crate::altium::decode_windows1252(block0);

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
        outline,
        unique_id: None,
    };

    Ok((body, current))
}

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
    // Parameters begin at the first `V7_LAYER=` key (after the binary header).
    s.find("V7_LAYER")
        .map(|start| crate::altium::parse_pipe_params_raw(&s[start..]))
        .unwrap_or_default()
}

/// Parses a value in mils (e.g., "15.748mil") to mm.
pub(super) fn parse_mil_value(s: Option<&str>) -> f64 {
    let Some(s) = s else {
        return 0.0;
    };

    // Remove "mil" suffix if present
    let numeric = s.trim_end_matches("mil").trim();
    numeric.parse::<f64>().map_or(0.0, |v| v * MM_PER_MIL) // Convert mils to mm
}

/// Parses `V7_LAYER` string (e.g., "MECHANICAL6") to Layer enum.
pub(super) fn parse_v7_layer(s: &str) -> Option<Layer> {
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

/// A mapping of model GUID to stream index.
///
/// The `/Library/Models/Data` stream contains entries that map GUIDs to
/// the numeric index of the model stream (e.g., `/Library/Models/0`) and the model name.
///
/// The value is a tuple of (`stream_index`, `model_name`).
pub type ModelIndex = HashMap<String, (usize, String)>;
