//! Sample-library tests for `PcbLib`.
//!
//! Like `samples_schlib.rs`, these tests open a *real*, Altium-authored sample
//! library from `scripts/samples/` with our reader and assert the parsed values
//! against the file's authored intent (rather than round-tripping our own
//! writer's output, as `file_io_roundtrip.rs` does).

use altium_designer_mcp::altium::pcblib::{
    HoleShape, Layer, PadShape, PadStackMode, PcbLib, RegionKind, TextKind,
};
use std::path::PathBuf;

/// Resolves a sample fixture by name under `scripts/samples/`.
fn sample(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("samples")
        .join(name)
}

/// Compares two lengths (mm) with a tolerance generous enough to absorb the
/// mil -> mm -> Altium-internal-unit conversion (60 mil = 1.524 mm exactly).
fn approx_eq(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() < tolerance
}

#[test]
fn samples_exist() {
    let path = sample("footprints.PcbLib");
    assert!(
        path.exists(),
        "missing sample fixture: {} — the samples_pcblib tests read a real \
         Altium-authored library that must be present on disk",
        path.display()
    );
}

#[test]
fn samples_pcblib_pad_shapes() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    // Seventeen footprints: twelve per-primitive-family footprints plus the five
    // coverage-enrichment footprints (TEXT_STYLE, REGION_CUTOUT, TEXT_SPECIAL,
    // MULTILAYER, EMBSTEP). Note: PAD_THERMAL remains a documented negative — the
    // thermal-relief/power-plane setters crash AD24's scripting engine on a fresh
    // library pad in every sequence tried (batch 4b final bisect); see
    // GenerateSamples.pas.
    assert_eq!(lib.len(), 17, "expected exactly seventeen footprints");
    let names = lib.names();
    for expected in [
        "PAD_SHAPES",
        "PAD_HOLES",
        "PAD_STACK",
        "TRACKS",
        "ARCS",
        "REGIONS",
        "TEXT_STROKE",
        "VIAS",
        "FILLS",
        "TEXT_WIN1252",
        "BODY3D",
        "EDGE",
        "TEXT_STYLE",
        "REGION_CUTOUT",
        "TEXT_SPECIAL",
        "MULTILAYER",
        "EMBSTEP",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} missing from {names:?}",
        );
    }

    let footprint = lib
        .get("PAD_SHAPES")
        .expect("footprint PAD_SHAPES not found");
    assert_eq!(footprint.name, "PAD_SHAPES");
    assert_eq!(footprint.pads.len(), 4, "PAD_SHAPES has 4 pads");

    // Authored pads: four 60x40 mil (1.524 x 1.016 mm) SMD pads on the Top Layer,
    // one per shape. We look pads up by designator because the on-disk order is
    // not guaranteed to be 1..4. Each authored Altium shape maps faithfully to a
    // `PadShape` variant, so the library round-trips losslessly.
    let expected: [(&str, PadShape); 4] = [
        ("1", PadShape::Round),            // authored Rounded
        ("2", PadShape::Rectangle),        // authored Rectangular
        ("3", PadShape::Octagonal),        // authored Octagonal
        ("4", PadShape::RoundedRectangle), // authored RoundedRectangle
    ];

    for (designator, shape) in expected {
        let pad = footprint
            .pads
            .iter()
            .find(|p| p.designator == designator)
            .unwrap_or_else(|| panic!("pad {designator} not found"));

        assert_eq!(pad.shape, shape, "pad {designator} shape");
        assert_eq!(
            pad.hole_size, None,
            "pad {designator} is SMD and must have no hole",
        );
        // Altium authors @60 = 1 for every pad, SMD included (probed: all four
        // golden SMD pads carry 1) — the flag is independent of hole_size.
        assert!(pad.is_plated, "pad {designator} is_plated (golden @60 = 1)");
        assert_eq!(pad.layer, Layer::TopLayer, "pad {designator} layer");
        assert!(
            approx_eq(pad.width, 1.524, 1e-2),
            "pad {designator} width: expected ~1.524 mm, got {}",
            pad.width,
        );
        assert!(
            approx_eq(pad.height, 1.016, 1e-2),
            "pad {designator} height: expected ~1.016 mm, got {}",
            pad.height,
        );
    }
}

#[test]
fn samples_pcblib_edge() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    let footprint = lib.get("EDGE").expect("footprint EDGE not found");
    assert_eq!(footprint.name, "EDGE");
    assert_eq!(footprint.pads.len(), 3, "EDGE has 3 pads");

    // Boundary-case pads, matched by designator (the on-disk order is not
    // guaranteed). Pad 1 is the headline case: a rotated rectangular pad — no
    // MAIN sample exercises a non-zero pad rotation. Pads 2 and 3 push the
    // coordinate extremes (negative and large positions).
    let pad1 = footprint
        .pads
        .iter()
        .find(|p| p.designator == "1")
        .expect("pad 1 not found");
    assert!(
        approx_eq(pad1.rotation, 45.0, 1e-2),
        "pad 1 rotation: expected ~45 degrees, got {}",
        pad1.rotation,
    );
    assert_eq!(pad1.shape, PadShape::Rectangle, "pad 1 shape");
    assert!(
        approx_eq(pad1.width, 2.032, 1e-2),
        "pad 1 width: expected ~2.032 mm, got {}",
        pad1.width,
    );
    assert!(
        approx_eq(pad1.height, 1.016, 1e-2),
        "pad 1 height: expected ~1.016 mm, got {}",
        pad1.height,
    );
    assert!(
        approx_eq(pad1.x, 0.0, 1e-2) && approx_eq(pad1.y, 0.0, 1e-2),
        "pad 1 position: expected ~(0, 0), got ({}, {})",
        pad1.x,
        pad1.y,
    );

    // Pad 2 sits at negative coordinates.
    let pad2 = footprint
        .pads
        .iter()
        .find(|p| p.designator == "2")
        .expect("pad 2 not found");
    assert!(
        approx_eq(pad2.x, -1.27, 1e-2) && approx_eq(pad2.y, -0.762, 1e-2),
        "pad 2 position: expected ~(-1.27, -0.762), got ({}, {})",
        pad2.x,
        pad2.y,
    );
    assert_eq!(pad2.shape, PadShape::Round, "pad 2 shape");

    // Pad 3 sits at large coordinates.
    let pad3 = footprint
        .pads
        .iter()
        .find(|p| p.designator == "3")
        .expect("pad 3 not found");
    assert!(
        approx_eq(pad3.x, 5.08, 1e-2) && approx_eq(pad3.y, 3.81, 1e-2),
        "pad 3 position: expected ~(5.08, 3.81), got ({}, {})",
        pad3.x,
        pad3.y,
    );
    assert_eq!(pad3.shape, PadShape::Round, "pad 3 shape");
}

#[test]
fn samples_pcblib_pad_stack() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let footprint = lib.get("PAD_STACK").expect("footprint PAD_STACK not found");
    assert_eq!(footprint.pads.len(), 1, "PAD_STACK has 1 pad");
    let pad = &footprint.pads[0];

    // A multi-layer (LocalStack) through-hole pad authored with a 70-mil round top,
    // 60-mil round mid and 50-mil square bottom over a 30-mil round hole. The reader
    // recognises the stack MODE, surfaces the top layer + hole, and now also
    // round-trips the mid/bottom per-layer sizes and shapes (closing TODO.md A1,
    // "middle/bottom sizes (TopMiddleBottom)") from the main geometry block.
    assert_eq!(
        pad.stack_mode,
        PadStackMode::TopMiddleBottom,
        "authored ePadMode_LocalStack reads back as a TopMiddleBottom stack",
    );
    assert_eq!(
        pad.layer,
        Layer::MultiLayer,
        "through-hole pad spans all layers",
    );
    assert_eq!(pad.shape, PadShape::Round, "top-layer shape");
    assert!(
        approx_eq(pad.width, 1.778, 1e-2),
        "top width ~70 mil, got {}",
        pad.width,
    );
    assert!(
        approx_eq(pad.height, 1.778, 1e-2),
        "top height ~70 mil, got {}",
        pad.height,
    );
    assert_eq!(pad.hole_shape, HoleShape::Round, "round hole");
    assert!(
        pad.hole_size.is_some_and(|h| approx_eq(h, 0.762, 1e-2)),
        "hole ~30 mil, got {:?}",
        pad.hole_size,
    );
    assert!(pad.is_plated, "golden stack pad is plated (@60 = 1)");

    // Per-layer [top, mid, bottom]: 70 / 60 / 50 mil sizes.
    let sizes = pad
        .per_layer_sizes
        .as_ref()
        .expect("TopMiddleBottom pad now surfaces per_layer_sizes");
    let expected_sizes = [(1.778, 1.778), (1.524, 1.524), (1.27, 1.27)];
    assert_eq!(sizes.len(), 3, "TMB per_layer_sizes is [top, mid, bottom]");
    for (i, &(ew, eh)) in expected_sizes.iter().enumerate() {
        assert!(
            approx_eq(sizes[i].0, ew, 1e-2) && approx_eq(sizes[i].1, eh, 1e-2),
            "per-layer size {i}: expected ~({ew},{eh}), got {:?}",
            sizes[i],
        );
    }

    // Per-layer [top, mid, bottom] shapes: round top, round mid, square bottom.
    assert_eq!(
        pad.per_layer_shapes.as_deref(),
        Some([PadShape::Round, PadShape::Round, PadShape::Rectangle].as_slice()),
        "TMB per_layer_shapes is [top, mid, bottom]",
    );
}

#[test]
fn samples_pcblib_pad_holes() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    let footprint = lib.get("PAD_HOLES").expect("footprint PAD_HOLES not found");
    assert_eq!(footprint.name, "PAD_HOLES");
    assert_eq!(footprint.pads.len(), 3, "PAD_HOLES has 3 pads");

    // Authored pads: three ~70x70 mil (1.778 x 1.778 mm) Round through-hole pads on
    // the Multi-Layer, one per hole shape. We look pads up by designator because the
    // on-disk order is not guaranteed. A round drill reads back as `HoleShape::Round`
    // (the enum's default) — it has no dedicated 651 block on disk, hence it serialises
    // as the absent/default value rather than `None`.
    let expected: [(&str, f64, HoleShape); 3] = [
        ("1", 0.762, HoleShape::Round),  // round drill (no 651 block)
        ("2", 0.762, HoleShape::Square), // square hole
        ("3", 1.016, HoleShape::Slot),   // slot hole
    ];

    for (designator, hole_size, hole_shape) in expected {
        let pad = footprint
            .pads
            .iter()
            .find(|p| p.designator == designator)
            .unwrap_or_else(|| panic!("pad {designator} not found"));

        assert_eq!(pad.shape, PadShape::Round, "pad {designator} shape");
        assert_eq!(pad.layer, Layer::MultiLayer, "pad {designator} layer");

        let actual_hole = pad
            .hole_size
            .unwrap_or_else(|| panic!("pad {designator} is through-hole and must have a hole"));
        assert!(
            approx_eq(actual_hole, hole_size, 1e-2),
            "pad {designator} hole_size: expected ~{hole_size} mm, got {actual_hole}",
        );

        assert_eq!(pad.hole_shape, hole_shape, "pad {designator} hole_shape");
    }

    // The slot pad (designator "3") carries a non-zero slot length in its 651-byte
    // size/shape block (@263 = 200000 raw = 0.508 mm); PR-8 now reads it. Round/square
    // pads have a zero slot length.
    let slot_pad = footprint.pads.iter().find(|p| p.designator == "3").unwrap();
    assert!(
        approx_eq(slot_pad.hole_slot_length, 0.508, 1e-3),
        "slot pad hole_slot_length: expected ~0.508 mm, got {}",
        slot_pad.hole_slot_length
    );
    assert!(
        approx_eq(slot_pad.hole_rotation, 0.0, 1e-6),
        "slot pad hole_rotation should be 0"
    );
    // Golden pads leave both drill tolerances at the 0x7FFFFFFF sentinel -> None.
    for pad in &footprint.pads {
        assert_eq!(pad.hole_positive_tolerance, None);
        assert_eq!(pad.hole_negative_tolerance, None);
    }

    // Every golden through-hole pad is plated (@60 = 1, Altium's default), and
    // the scripting-authored pads carry the nil identity GUIDs @126/@142
    // (probed: all-zero bytes) — read back verbatim so a read-modify-write
    // preserves them instead of regenerating fresh GUIDs.
    let nil_guid = "{00000000-0000-0000-0000-000000000000}";
    for pad in &footprint.pads {
        assert!(pad.is_plated, "pad {} is_plated", pad.designator);
        assert_eq!(
            pad.identity_guid.as_deref(),
            Some(nil_guid),
            "pad {} GUID-A is the golden's nil GUID",
            pad.designator,
        );
        assert_eq!(
            pad.identity_guid_b.as_deref(),
            Some(nil_guid),
            "pad {} GUID-B is the golden's nil GUID",
            pad.designator,
        );
    }
}

#[test]
fn samples_pcblib_tracks() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    let footprint = lib.get("TRACKS").expect("footprint TRACKS not found");
    assert_eq!(footprint.name, "TRACKS");
    assert_eq!(footprint.tracks.len(), 5, "TRACKS has 5 tracks");

    // Authored: a 5.08 mm silk box (four 0.254 mm = 10 mil segments on Top Overlay)
    // plus one 0.508 mm = 20 mil copper track across the middle on Top Layer. We
    // identify each by endpoints (order on disk is not guaranteed).
    let silk_segments: [(f64, f64, f64, f64); 4] = [
        (-2.54, -2.54, 2.54, -2.54),
        (2.54, -2.54, 2.54, 2.54),
        (2.54, 2.54, -2.54, 2.54),
        (-2.54, 2.54, -2.54, -2.54),
    ];

    for (x1, y1, x2, y2) in silk_segments {
        let track = footprint
            .tracks
            .iter()
            .find(|t| {
                approx_eq(t.x1, x1, 1e-3)
                    && approx_eq(t.y1, y1, 1e-3)
                    && approx_eq(t.x2, x2, 1e-3)
                    && approx_eq(t.y2, y2, 1e-3)
            })
            .unwrap_or_else(|| panic!("silk track ({x1},{y1})->({x2},{y2}) not found"));

        assert_eq!(track.layer, Layer::TopOverlay, "silk track layer");
        assert!(
            approx_eq(track.width, 0.254, 1e-3),
            "silk track width: expected ~0.254 mm, got {}",
            track.width,
        );
    }

    // The lone copper track is the only 0.508 mm (20 mil) one.
    let copper = footprint
        .tracks
        .iter()
        .find(|t| approx_eq(t.width, 0.508, 1e-3))
        .expect("copper track (width ~0.508 mm) not found");
    assert_eq!(copper.layer, Layer::TopLayer, "copper track layer");
    assert!(
        approx_eq(copper.x1, -2.54, 1e-3)
            && approx_eq(copper.y1, 0.0, 1e-3)
            && approx_eq(copper.x2, 2.54, 1e-3)
            && approx_eq(copper.y2, 0.0, 1e-3),
        "copper track endpoints: got ({},{})->({},{})",
        copper.x1,
        copper.y1,
        copper.x2,
        copper.y2,
    );
}

#[test]
fn samples_pcblib_arcs() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    let footprint = lib.get("ARCS").expect("footprint ARCS not found");
    assert_eq!(footprint.name, "ARCS");
    assert_eq!(footprint.arcs.len(), 2, "ARCS has 2 arcs");

    // Two arcs on the Top Overlay: a full circle and a quarter arc. We identify
    // them by radius (1.270 mm = 50 mil vs 1.016 mm = 40 mil).
    let circle = footprint
        .arcs
        .iter()
        .find(|a| approx_eq(a.radius, 1.270, 1e-2))
        .expect("full-circle arc (radius ~1.270 mm) not found");
    assert_eq!(circle.layer, Layer::TopOverlay, "circle layer");
    assert!(
        approx_eq(circle.x, 0.0, 1e-3) && approx_eq(circle.y, 0.0, 1e-3),
        "circle centre: got ({},{})",
        circle.x,
        circle.y,
    );
    assert!(
        approx_eq(circle.start_angle, 0.0, 1e-2),
        "circle start angle: expected ~0, got {}",
        circle.start_angle,
    );
    assert!(
        approx_eq(circle.end_angle, 360.0, 1e-2),
        "circle end angle: expected ~360, got {}",
        circle.end_angle,
    );

    let quarter = footprint
        .arcs
        .iter()
        .find(|a| approx_eq(a.radius, 1.016, 1e-2))
        .expect("quarter arc (radius ~1.016 mm) not found");
    assert_eq!(quarter.layer, Layer::TopOverlay, "quarter arc layer");
    assert!(
        approx_eq(quarter.x, 5.08, 1e-3) && approx_eq(quarter.y, 0.0, 1e-3),
        "quarter arc centre: got ({},{})",
        quarter.x,
        quarter.y,
    );
    assert!(
        approx_eq(quarter.start_angle, 0.0, 1e-2),
        "quarter arc start angle: expected ~0, got {}",
        quarter.start_angle,
    );
    assert!(
        approx_eq(quarter.end_angle, 90.0, 1e-2),
        "quarter arc end angle: expected ~90, got {}",
        quarter.end_angle,
    );
}

#[test]
fn samples_pcblib_regions() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    let footprint = lib.get("REGIONS").expect("footprint REGIONS not found");
    assert_eq!(footprint.name, "REGIONS");
    assert_eq!(footprint.regions.len(), 2, "REGIONS has 2 regions");

    // Each region is a 4-vertex box; one is on the Top Layer, the other on
    // Mechanical 1. The reader now parses the nested parameter block, so KIND,
    // NAME, and ARCRESOLUTION are populated from the golden's real Altium values
    // (`KIND=0`, `NAME= ` -> empty, `ARCRESOLUTION=0.5mil`).
    for region in &footprint.regions {
        assert_eq!(
            region.vertices.len(),
            4,
            "each region is a 4-vertex box, got {} on {:?}",
            region.vertices.len(),
            region.layer,
        );
        assert_eq!(region.kind, RegionKind::Copper, "golden regions are copper");
        assert!(
            region.name.is_empty(),
            "golden region NAME is blank, got {:?}",
            region.name
        );
        // ARCRESOLUTION=0.5mil = 0.0127 mm.
        assert!(
            approx_eq(region.arc_resolution, 0.5 * 0.0254, 1e-6),
            "golden region ARCRESOLUTION should be 0.5mil (0.0127mm), got {}",
            region.arc_resolution
        );
        assert_eq!(region.sub_poly_index, -1, "golden SUBPOLYINDEX=-1");
        assert!(!region.is_shape_based, "golden ISSHAPEBASED=FALSE");
        // These simple copper regions carry only the modelled keys, so the
        // unmodelled-parameter catch-all is empty (PR-R5).
        assert!(
            region.additional_parameters.is_empty(),
            "golden copper region has no unmodelled keys, got {:?}",
            region.additional_parameters
        );
    }

    assert!(
        footprint.regions.iter().any(|r| r.layer == Layer::TopLayer),
        "expected a region on Top Layer, layers: {:?}",
        footprint
            .regions
            .iter()
            .map(|r| r.layer)
            .collect::<Vec<_>>(),
    );
    assert!(
        footprint
            .regions
            .iter()
            .any(|r| r.layer == Layer::Mechanical1),
        "expected a region on Mechanical 1, layers: {:?}",
        footprint
            .regions
            .iter()
            .map(|r| r.layer)
            .collect::<Vec<_>>(),
    );
}

#[test]
fn samples_pcblib_text_stroke() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    let footprint = lib
        .get("TEXT_STROKE")
        .expect("footprint TEXT_STROKE not found");
    assert_eq!(footprint.name, "TEXT_STROKE");
    assert_eq!(footprint.text.len(), 4, "TEXT_STROKE has 4 strings");

    // Four stroke-font strings on the Top Overlay. We look each up by content and
    // assert its height (mm), rotation, and that it is stroke (vector) text.
    let expected: [(&str, f64, f64); 4] = [
        ("REF", 1.524, 0.0),   // 60 mil, upright
        ("10uF", 1.270, 0.0),  // 50 mil, upright
        ("VERT", 1.524, 90.0), // 60 mil, rotated
        ("4u7", 1.270, 0.0),   // 50 mil, upright
    ];

    for (content, height, rotation) in expected {
        let text = footprint
            .text
            .iter()
            .find(|t| t.text == content)
            .unwrap_or_else(|| panic!("text {content:?} not found"));

        assert_eq!(text.layer, Layer::TopOverlay, "text {content:?} layer");
        assert_eq!(text.kind, TextKind::Stroke, "text {content:?} kind");
        // PR-10: the reader now populates mirror/bold/font_name from the geometry
        // block; a default Altium stroke text is top-side, non-bold, Arial.
        assert!(!text.mirror, "text {content:?} mirror");
        assert!(!text.bold, "text {content:?} bold");
        // Every golden text is a plain string: the IsComment@40 / IsDesignator@41
        // markers carry 0x00 on disk (probed) and read back false.
        assert!(!text.is_comment, "text {content:?} is_comment");
        assert!(!text.is_designator, "text {content:?} is_designator");
        assert_eq!(text.font_name, "Arial", "text {content:?} font_name");
        assert!(
            approx_eq(text.height, height, 1e-2),
            "text {content:?} height: expected ~{height} mm, got {}",
            text.height,
        );
        assert!(
            approx_eq(text.rotation, rotation, 1e-2),
            "text {content:?} rotation: expected ~{rotation}, got {}",
            text.rotation,
        );
    }
}

#[test]
fn samples_pcblib_vias() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    let footprint = lib.get("VIAS").expect("footprint VIAS not found");
    assert_eq!(footprint.name, "VIAS");
    assert_eq!(footprint.vias.len(), 2, "VIAS has 2 vias");

    // Two through-hole vias (Top -> Bottom). We identify each by its diameter
    // (24 mil vs 40 mil); the on-disk order is not guaranteed.
    let small = footprint
        .vias
        .iter()
        .find(|v| approx_eq(v.diameter, 0.6096, 1e-2))
        .expect("via with diameter ~0.6096 mm not found");
    assert!(
        approx_eq(small.x, 0.0, 1e-2) && approx_eq(small.y, 0.0, 1e-2),
        "small via position: got ({},{})",
        small.x,
        small.y,
    );
    assert!(
        approx_eq(small.hole_size, 0.3048, 1e-2),
        "small via hole_size: expected ~0.3048 mm, got {}",
        small.hole_size,
    );
    assert_eq!(small.from_layer, Layer::TopLayer, "small via from_layer");
    assert_eq!(small.to_layer, Layer::BottomLayer, "small via to_layer");

    let large = footprint
        .vias
        .iter()
        .find(|v| approx_eq(v.diameter, 1.016, 1e-2))
        .expect("via with diameter ~1.016 mm not found");
    assert!(
        approx_eq(large.x, 2.032, 1e-2) && approx_eq(large.y, 0.0, 1e-2),
        "large via position: got ({},{})",
        large.x,
        large.y,
    );
    assert!(
        approx_eq(large.hole_size, 0.508, 1e-2),
        "large via hole_size: expected ~0.508 mm, got {}",
        large.hole_size,
    );
    assert_eq!(large.from_layer, Layer::TopLayer, "large via from_layer");
    assert_eq!(large.to_layer, Layer::BottomLayer, "large via to_layer");
}

#[test]
fn samples_pcblib_fills() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    let footprint = lib.get("FILLS").expect("footprint FILLS not found");
    assert_eq!(footprint.name, "FILLS");
    assert_eq!(footprint.fills.len(), 2, "FILLS has 2 fills");

    // Two filled rectangles on the Top Layer; one upright, one rotated 45°. We
    // identify each by rotation (the on-disk order is not guaranteed).
    let upright = footprint
        .fills
        .iter()
        .find(|f| approx_eq(f.rotation, 0.0, 1e-2))
        .expect("fill with rotation ~0 not found");
    assert_eq!(upright.layer, Layer::TopLayer, "upright fill layer");
    assert!(
        approx_eq(upright.x1, 0.0, 1e-2)
            && approx_eq(upright.y1, 0.0, 1e-2)
            && approx_eq(upright.x2, 1.016, 1e-2)
            && approx_eq(upright.y2, 0.508, 1e-2),
        "upright fill corners: got ({},{})->({},{})",
        upright.x1,
        upright.y1,
        upright.x2,
        upright.y2,
    );

    let rotated = footprint
        .fills
        .iter()
        .find(|f| approx_eq(f.rotation, 45.0, 1e-2))
        .expect("fill with rotation ~45 not found");
    assert_eq!(rotated.layer, Layer::TopLayer, "rotated fill layer");
    assert!(
        approx_eq(rotated.x1, 1.524, 1e-2)
            && approx_eq(rotated.y1, 0.0, 1e-2)
            && approx_eq(rotated.x2, 2.54, 1e-2)
            && approx_eq(rotated.y2, 0.508, 1e-2),
        "rotated fill corners: got ({},{})->({},{})",
        rotated.x1,
        rotated.y1,
        rotated.x2,
        rotated.y2,
    );
}

#[test]
fn samples_pcblib_text_win1252() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    let footprint = lib
        .get("TEXT_WIN1252")
        .expect("footprint TEXT_WIN1252 not found");
    assert_eq!(footprint.name, "TEXT_WIN1252");
    assert_eq!(footprint.text.len(), 2, "TEXT_WIN1252 has 2 strings");

    // Two stroke-font strings whose content uses non-ASCII characters authored in
    // Windows-1252 (micro sign 0xB5 and plus-minus 0xB1). This asserts they survive
    // the cp1252 -> UTF-8 decode into correct Rust `String`s. We avoid pasting raw
    // non-ASCII into the source by matching on explicit Unicode escapes.
    for content in ["10\u{B5}F", "\u{B1}5%"] {
        let text = footprint
            .text
            .iter()
            .find(|t| t.text == content)
            .unwrap_or_else(|| panic!("text {content:?} not found"));

        assert_eq!(text.kind, TextKind::Stroke, "text {content:?} kind");
        assert_eq!(text.layer, Layer::TopOverlay, "text {content:?} layer");
    }
}

#[test]
fn samples_pcblib_body3d() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    let footprint = lib.get("BODY3D").expect("footprint BODY3D not found");
    assert_eq!(footprint.name, "BODY3D");
    assert_eq!(
        footprint.component_bodies.len(),
        1,
        "BODY3D has 1 component body",
    );

    // A simple extruded 3D component body: a 100x60 mil rectangle authored on
    // Mechanical 13 (LayerUtils.MechanicalLayer(13) in the sample generator), ~1 mm
    // (40 mil) tall, sitting flush on the board (standoff 0).
    let body = &footprint.component_bodies[0];
    assert!(
        approx_eq(body.overall_height, 1.016, 1e-2),
        "overall_height: expected ~1.016 mm (40 mil), got {}",
        body.overall_height,
    );
    assert!(
        approx_eq(body.standoff_height, 0.0, 1e-2),
        "standoff_height: expected ~0, got {}",
        body.standoff_height,
    );
    // Layer-reader regression (PR-11): the body is authored on Mechanical 13
    // (layer id 69). It was previously collapsed to Top3DBody because the reader
    // decoded only the V7_LAYER string via an incomplete map (MECHANICAL2-7) and
    // ignored the CommonPrimitiveData header layer byte. The reader now reads the
    // header byte, so the true layer survives.
    assert_eq!(body.layer, Layer::Mechanical13, "body layer");

    // Altium reorders the contour vertices on save, so we assert the vertex count
    // and the axis-aligned bounding box rather than an exact vertex order.
    assert_eq!(body.outline.len(), 4, "body outline is a 4-vertex box");

    let min_x = body
        .outline
        .iter()
        .map(|&(x, _)| x)
        .fold(f64::INFINITY, f64::min);
    let max_x = body
        .outline
        .iter()
        .map(|&(x, _)| x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = body
        .outline
        .iter()
        .map(|&(_, y)| y)
        .fold(f64::INFINITY, f64::min);
    let max_y = body
        .outline
        .iter()
        .map(|&(_, y)| y)
        .fold(f64::NEG_INFINITY, f64::max);

    assert!(
        approx_eq(min_x, -1.27, 1e-2),
        "outline min x: expected ~-1.27 mm, got {min_x}",
    );
    assert!(
        approx_eq(max_x, 1.27, 1e-2),
        "outline max x: expected ~1.27 mm, got {max_x}",
    );
    assert!(
        approx_eq(min_y, -0.762, 1e-2),
        "outline min y: expected ~-0.762 mm, got {min_y}",
    );
    assert!(
        approx_eq(max_y, 0.762, 1e-2),
        "outline max y: expected ~0.762 mm, got {max_y}",
    );

    // Every key `build_component_body_params` emits itself is now in
    // BODY_MODELLED_PARAM_KEYS, so the reader does NOT capture any of them into
    // `additional_parameters` (bug sweep 2026-07: capturing a writer-emitted key
    // duplicated it on read-modify-write, and the deliberately-repeated
    // ARCRESOLUTION was captured twice). Only genuinely-unmodelled keys survive
    // here — and no canonical key may.
    let keys: Vec<&str> = body
        .additional_parameters
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    for canonical in [
        "V7_LAYER",
        "NAME",
        "KIND",
        "OVERALLHEIGHT",
        "MODELID",
        "MODEL.NAME",
        "TEXTURE",
        "IDENTIFIER",
        "ARCRESOLUTION",
        "CAVITYHEIGHT",
        "MODEL.2D.X",
        "MODEL.MODELTYPE",
        "MODEL.EXTRUDED.MINZ",
    ] {
        assert!(
            !keys.contains(&canonical),
            "canonical key {canonical} must NOT be captured into additional_parameters \
             (the writer emits it), got {keys:?}"
        );
    }
    // No key may appear twice (the ARCRESOLUTION double-capture regression).
    for (k, _) in &body.additional_parameters {
        let count = keys.iter().filter(|&&x| x == k.as_str()).count();
        assert_eq!(
            count, 1,
            "additional_parameters key {k} captured more than once"
        );
    }
}

// ---------------------------------------------------------------------------
// Coverage-enrichment tests (docs/FIXTURE_COVERAGE.md): non-default PcbLib
// property values authored by GenerateSamples.pas, verified against the real
// Altium-regenerated fixture.
// ---------------------------------------------------------------------------

#[test]
fn samples_pcblib_text_style() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib
        .get("TEXT_STYLE")
        .expect("TEXT_STYLE footprint not found");

    // A single TrueType text authored with Bold + Italic + Mirror + FontName —
    // the first real-Altium ground truth for these IPCB_Text style fields (they
    // were previously exercised only by a self-round-trip / oracle default).
    assert_eq!(fp.text.len(), 1, "TEXT_STYLE has one text");
    let t = &fp.text[0];
    assert_eq!(t.kind, TextKind::TrueType, "text kind is TrueType");
    assert!(t.bold, "text is bold");
    assert!(t.italic, "text is italic");
    assert!(t.mirror, "text is mirrored");
    assert_eq!(t.font_name, "Arial", "TrueType font name round-trips");
}

#[test]
fn samples_pcblib_region_cutout() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib
        .get("REGION_CUTOUT")
        .expect("REGION_CUTOUT footprint not found");

    // Authored via eRegionKind_BoardCutout. Altium stores a board cutout NOT as a
    // distinct KIND integer but as a copper region (KIND=0) carrying
    // ISBOARDCUTOUT=TRUE in its parameter block. Our reader therefore reads it as
    // `Copper` and preserves the cutout state verbatim in `additional_parameters`
    // (round-trip-safe). This documents the real on-disk representation.
    assert_eq!(fp.regions.len(), 1, "REGION_CUTOUT has one region");
    let r = &fp.regions[0];
    assert_eq!(
        r.kind,
        RegionKind::Copper,
        "on disk a board cutout is KIND=0"
    );
    assert!(
        r.additional_parameters
            .iter()
            .any(|(k, v)| k == "ISBOARDCUTOUT" && v == "TRUE"),
        "the board-cutout flag is preserved in additional_parameters, got {:?}",
        r.additional_parameters
    );
    // Altium also silently MOVED the authored eTopLayer region onto the
    // keep-out layer (LAYER=KEEPOUT + KEEPOUT=TRUE in the same param block) —
    // the authored layer is not preserved for a board cutout. Assert the real
    // on-disk placement so the relocation is documented ground truth.
    assert_eq!(
        r.layer,
        Layer::KeepOut,
        "Altium relocates a board cutout to the keep-out layer"
    );
    assert!(
        r.additional_parameters
            .iter()
            .any(|(k, v)| k == "KEEPOUT" && v == "TRUE"),
        "the KEEPOUT flag is preserved in additional_parameters, got {:?}",
        r.additional_parameters
    );
}

#[test]
fn samples_pcblib_multilayer() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib
        .get("MULTILAYER")
        .expect("MULTILAYER footprint not found");

    // Six 10-mil tracks (x -50..50 mil) stacked at y = 0/20/40/60/80/100 mil, one
    // per exotic layer arm of `layer_from_id` — the first real-Altium golden for
    // the mechanical / mid-copper / drill / internal-plane / keep-out IDs (58, 6,
    // 55, 73, 39, 56). Byte-probe of MULTILAYER/Data confirmed the authored layer
    // IDs verbatim. Tracks are matched by their y position (mm), never by index.
    //
    // Note the first row: the track authored on eMechanical2 (layer ID 58) reads
    // back as `Layer::TopAssembly` — the reader's documented alias for ID 58 (the
    // component-layer-pair name for Mechanical 2). The mapping is lossless on the
    // wire: the writer maps both `TopAssembly` and `Mechanical2` back to ID 58.
    assert_eq!(fp.tracks.len(), 6, "MULTILAYER has 6 tracks");

    let expected: [(f64, Layer); 6] = [
        (0.0, Layer::TopAssembly), // authored eMechanical2 (ID 58)
        (0.508, Layer::MidLayer5),
        (1.016, Layer::DrillGuide),
        (1.524, Layer::DrillDrawing),
        (2.032, Layer::InternalPlane1),
        (2.54, Layer::KeepOut),
    ];
    for (y_mm, layer) in expected {
        let track = fp
            .tracks
            .iter()
            .find(|t| approx_eq(t.y1, y_mm, 1e-6) && approx_eq(t.y2, y_mm, 1e-6))
            .unwrap_or_else(|| panic!("no horizontal track at y = {y_mm} mm"));
        assert_eq!(track.layer, layer, "layer of the track at y = {y_mm} mm");
        assert!(
            approx_eq(track.x1, -1.27, 1e-6) && approx_eq(track.x2, 1.27, 1e-6),
            "track at y = {y_mm} mm spans -50..50 mil, got x1={} x2={}",
            track.x1,
            track.x2,
        );
        assert!(
            approx_eq(track.width, 0.254, 1e-6),
            "track at y = {y_mm} mm is 10 mil wide, got {}",
            track.width,
        );
    }
}

#[test]
fn samples_pcblib_embstep() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib.get("EMBSTEP").expect("EMBSTEP footprint not found");

    // A component body carrying an EMBEDDED minimal AP214 STEP model, authored via
    // ModelFactory_FromFilename -> SetState_FromModel -> .Model. This is the first
    // real-Altium golden for the embedded-model read path: the body's parameter
    // record carries MODELID/MODEL.CHECKSUM/MODEL.NAME, and the library-level
    // /Library/Models/Data + /Library/Models/0 streams carry the zlib-compressed
    // model bytes. All values below were byte-probed from the fixture.
    assert_eq!(fp.component_bodies.len(), 1, "EMBSTEP has 1 component body");
    let body = &fp.component_bodies[0];

    assert_eq!(
        body.model_id, "{A0448C65-C10D-4882-92F6-D6E5A5C55B3D}",
        "the body references the embedded model's GUID"
    );
    assert_eq!(body.model_name, "minimal.step", "MODEL.NAME");
    assert!(body.embedded, "MODEL.EMBED=TRUE reads as embedded");
    assert_eq!(
        body.model_checksum, 1_975_055,
        "MODEL.CHECKSUM round-trips verbatim"
    );
    // The model was imported unrotated and flush on the board.
    assert!(approx_eq(body.rotation_x, 0.0, 1e-9), "MODEL.3D.ROTX");
    assert!(approx_eq(body.rotation_y, 0.0, 1e-9), "MODEL.3D.ROTY");
    assert!(approx_eq(body.rotation_z, 0.0, 1e-9), "MODEL.3D.ROTZ");
    assert!(approx_eq(body.z_offset, 0.0, 1e-9), "MODEL.3D.DZ");
    assert!(approx_eq(body.standoff_height, 0.0, 1e-9), "STANDOFFHEIGHT");
    assert!(approx_eq(body.overall_height, 0.0, 1e-9), "OVERALLHEIGHT");
    // CommonPrimitiveData layer byte 57 (also V7_LAYER=MECHANICAL1).
    assert_eq!(body.layer, Layer::Mechanical1, "body layer");

    // The referenced model must actually exist in the library's embedded-model
    // store, resolved through the same lookup the reader uses (case-insensitive).
    assert_eq!(lib.model_count(), 1, "the library embeds exactly one model");
    let model = lib
        .get_model(&body.model_id)
        .expect("the body's MODELID resolves to an embedded model");
    assert!(
        model.id.eq_ignore_ascii_case(&body.model_id),
        "model GUID matches the body reference, got {}",
        model.id,
    );
    assert_eq!(model.name, "minimal.step", "embedded model name");
    assert!(
        std::path::Path::new(&model.name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("step")),
        "embedded model is a STEP file"
    );

    // /Library/Models/0 is zlib-compressed (189 bytes on disk); decompressed it
    // is the 267-byte minimal.step text, a valid ISO-10303-21 exchange file.
    assert_eq!(model.compressed_size, 189, "compressed stream size");
    assert_eq!(model.data.len(), 267, "decompressed STEP size");
    assert!(
        model.data.starts_with(b"ISO-10303-21"),
        "decompressed model data is STEP text"
    );
    assert!(
        model.data.ends_with(b"END-ISO-10303-21;\r\n"),
        "STEP terminator survives decompression"
    );

    // The backward-compatibility Model3D view resolves the filename through the
    // embedded-model index.
    let model_3d = fp
        .model_3d
        .as_ref()
        .expect("model_3d is populated from the component body");
    assert_eq!(model_3d.filepath, "minimal.step", "model_3d filepath");
}

#[test]
fn samples_pcblib_text_special() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib
        .get("TEXT_SPECIAL")
        .expect("TEXT_SPECIAL footprint not found");

    // Two special text items authored in batch 4a: a Code-128 barcode ("BC128")
    // and an inverted (knockout) TrueType text in a framed rectangle ("INV").
    // First real-Altium ground truth for TextKind::BarCode and the inverted
    // text-box descriptor (offsets 110-133), previously only
    // self-round-trip-tested. Matched by content; on-disk order is not
    // guaranteed.
    assert_eq!(fp.text.len(), 2, "TEXT_SPECIAL has two text items");
    let by_content = |content: &str| {
        fp.text
            .iter()
            .find(|t| t.text == content)
            .unwrap_or_else(|| panic!("text {content:?} not found"))
    };

    // Barcode: authored TextKind := eText_BarCode (+ eBarCode128 sizing, which
    // the Text struct does not model — `kind` is the modelled surface).
    let barcode = by_content("BC128");
    assert_eq!(barcode.kind, TextKind::BarCode, "BC128 kind is BarCode");
    assert_eq!(barcode.layer, Layer::TopOverlay, "BC128 layer");
    assert!(!barcode.is_inverted, "BC128 is not inverted");
    assert!(
        approx_eq(barcode.height, 1.524, 1e-3),
        "BC128 height: expected 60 mil = 1.524 mm, got {}",
        barcode.height,
    );

    // Inverted TrueType: authored Inverted + UseInvertedRectangle +
    // InvertedTTTextBorder = 10 mil. The rectangle dimensions themselves were
    // auto-computed by Altium on save (not authored), so only the border is
    // asserted exactly; the width/height must simply be present (Some) since
    // UseInvertedRectangle is set.
    let inv = by_content("INV");
    assert_eq!(inv.kind, TextKind::TrueType, "INV kind is TrueType");
    assert!(inv.is_inverted, "INV is inverted (knockout)");
    assert!(inv.use_inverted_rectangle, "INV uses a framed rectangle");
    assert_eq!(inv.font_name, "Arial", "INV TrueType font name");
    assert!(
        approx_eq(inv.inverted_border.expect("INV has a border"), 0.254, 1e-6),
        "INV inverted_border: expected 10 mil = 0.254 mm, got {:?}",
        inv.inverted_border,
    );
    // The auto-computed on-disk values (fixture ground truth): width 1,040,750
    // internal units = 104.075 mil = 2.643505 mm; height 584,375 units =
    // 58.4375 mil = 1.4843125 mm.
    assert!(
        approx_eq(
            inv.inverted_rect_width.expect("INV has a rect width"),
            2.643_505,
            1e-5
        ),
        "INV inverted_rect_width: got {:?}",
        inv.inverted_rect_width,
    );
    assert!(
        approx_eq(
            inv.inverted_rect_height.expect("INV has a rect height"),
            1.484_312_5,
            1e-5
        ),
        "INV inverted_rect_height: got {:?}",
        inv.inverted_rect_height,
    );
    assert_eq!(
        inv.inverted_rect_text_offset, None,
        "INV text offset was not authored (zero on disk reads back None)"
    );
}
