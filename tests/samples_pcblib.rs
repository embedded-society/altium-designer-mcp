//! Sample-library tests for `PcbLib`.
//!
//! Like `samples_schlib.rs`, these tests open a *real*, Altium-authored sample
//! library from `scripts/samples/` with our reader and assert the parsed values
//! against the file's authored intent (rather than round-tripping our own
//! writer's output, as `file_io_roundtrip.rs` does).

use altium_designer_mcp::altium::pcblib::{
    DrillLayerPairType, HoleShape, Layer, MaskExpansionMode, PadPolygonConnect, PadShape,
    PadStackMode, PcbFlags, PcbLib, PowerPlaneConnectStyle, PrimitiveKind, RegionKind, StrokeFont,
    TextKind,
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

    // Twelve per-primitive-family footprints plus the coverage-enrichment ones
    // (TEXT_STYLE, REGION_CUTOUT, TEXT_SPECIAL, MULTILAYER, EMBSTEP, PRIMPROPS,
    // batch 5's STEP_REF, MECH20, TEXT_WIDE_ONLY, and BODYPREC).
    // Note: PAD_THERMAL remains a documented negative — the thermal-relief /
    // power-plane setters crash AD24's scripting engine on a fresh library pad in
    // every sequence tried (batch 4b final bisect); see GenerateSamples.pas.
    assert_eq!(lib.len(), 26, "expected exactly twenty-six footprints");
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
        "TEXT_LONG",
        "Резистор_0402",
        "PADMASK",
        "LOCKFLAGS_PCB",
        "MULTILAYER",
        "EMBSTEP",
        "PRIMPROPS",
        "STEP_REF",
        "MECH20",
        "TEXT_WIDE_ONLY",
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
        // A single space, not empty: Altium writes `NAME= ` on a default region.
        // This read blank until the parameter reader stopped trimming values,
        // and the writer then emitted `NAME=`, altering the record on every
        // read-modify-write.
        assert_eq!(
            region.name, " ",
            "golden region NAME is a single space, got {:?}",
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
fn samples_pcblib_text_long_comes_from_wide_strings() {
    // The out-of-line text path, against a real Altium file (#314, #309).
    //
    // A Text record's block 1 is a Pascal SHORT string, so Altium truncates it
    // at 255 bytes and writes the full text to /{component}/WideStrings. This
    // footprint's first string is 264 characters, which is what makes that path
    // observable: every other sample text is short enough for Altium to
    // duplicate inline, where block 1 alone is sufficient.
    //
    // The short string alongside it is the last entry in the stream, so it
    // covers the null terminator that ends the final value.
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");

    let footprint = lib.get("TEXT_LONG").expect("footprint TEXT_LONG not found");
    assert_eq!(footprint.text.len(), 2, "TEXT_LONG has 2 strings");

    let long = footprint
        .text
        .iter()
        .find(|t| t.text.len() > 255)
        .expect("the long string must survive block 1's 255-byte limit");
    let expected: String = "A".repeat(260) + "_END";
    assert_eq!(long.text.len(), 264, "264 characters, not truncated to 255");
    assert_eq!(long.text, expected, "the full authored text, verbatim");

    // Last entry in the stream, so its final character sits against the
    // terminator.
    assert!(
        footprint.text.iter().any(|t| t.text == "SHORT"),
        "the final entry keeps its last character: {:?}",
        footprint.text.iter().map(|t| &t.text).collect::<Vec<_>>()
    );
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
    // The body is authored on Mechanical 13 (layer id 69). The reader must
    // take the layer from the CommonPrimitiveData header byte: decoding only
    // the V7_LAYER string collapses any layer outside that map to Top3DBody.
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
// Coverage-enrichment tests (scripts/samples/COVERAGE.md): non-default PcbLib
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
    // real-Altium ground truth for these IPCB_Text style fields, beyond what a
    // self-round-trip or an oracle default can show.
    assert_eq!(fp.text.len(), 1, "TEXT_STYLE has one text");
    let t = &fp.text[0];
    assert_eq!(t.kind, TextKind::TrueType, "text kind is TrueType");
    assert!(t.bold, "text is bold");
    assert!(t.italic, "text is italic");
    assert!(t.mirror, "text is mirrored");
    assert_eq!(t.font_name, "Arial", "TrueType font name round-trips");
}

#[test]
fn samples_pcblib_via_manual_mask_expansion() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib.get("PRIMPROPS").expect("PRIMPROPS footprint not found");

    // The counterpart to samples_pcblib_via_mask_state_is_altium_factory_default:
    // that one pins a via left on the rule, this one a via switched to manual.
    // Both expansions had to be set through TPadCache — the direct
    // Via.SolderMaskExpansion* / .PasteMaskExpansion* setters compile and then
    // bring AD24 down with a native access violation, so no golden can ever be
    // authored that way (see GenerateSamples.pas).
    assert_eq!(fp.vias.len(), 1, "PRIMPROPS has one via");
    let v = &fp.vias[0];
    assert_eq!(
        v.solder_mask_expansion_mode,
        MaskExpansionMode::Manual,
        "the cache marks the expansion manual"
    );
    assert!(
        approx_eq(v.solder_mask_expansion, 0.1778, 1e-4),
        "7 mil solder-mask expansion, got {}",
        v.solder_mask_expansion
    );
    assert!(
        approx_eq(v.paste_mask_expansion, 0.0762, 1e-4),
        "3 mil paste-mask expansion, got {}",
        v.paste_mask_expansion
    );
}

#[test]
fn samples_pcblib_component_body_placement_and_appearance() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib.get("PRIMPROPS").expect("PRIMPROPS footprint not found");

    // The only body in the library that is not flat, grey and opaque. Without it
    // the standoff, cavity, colour and opacity arms read nothing but defaults.
    assert_eq!(fp.component_bodies.len(), 1, "PRIMPROPS has one body");
    let b = &fp.component_bodies[0];

    // 10 mil standoff, 50 mil overall, 5 mil cavity.
    assert!(
        approx_eq(b.standoff_height, 0.254, 1e-6),
        "standoff height, got {}",
        b.standoff_height
    );
    assert!(
        approx_eq(b.overall_height, 1.27, 1e-6),
        "overall height, got {}",
        b.overall_height
    );
    // CAVITYHEIGHT was on the reader's modelled-keys list — and so excluded from
    // the unknown-key passthrough — while no field parsed it, and the writer
    // emitted a hard-coded `CAVITYHEIGHT=0mil`. A non-zero value was therefore
    // lost in both directions until this fixture caught it.
    assert!(
        approx_eq(b.cavity_height, 0.127, 1e-6),
        "cavity height, got {}",
        b.cavity_height
    );

    // Red at half opacity, against the grey opaque default every other body has.
    assert_eq!(b.body_color_3d, 255, "3D body colour (BGR red)");
    assert!(
        approx_eq(b.body_opacity_3d, 0.5, 1e-6),
        "3D body opacity, got {}",
        b.body_opacity_3d
    );
}

#[test]
fn samples_pcblib_rotated_unplated_slot() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib.get("PRIMPROPS").expect("PRIMPROPS footprint not found");

    // Every other hole in the library is unrotated and plated, so both arms read
    // nothing but defaults. The slot is 20 mil across and 40 mil long, turned 30
    // degrees, and unplated.
    let pad = fp
        .pads
        .iter()
        .find(|p| p.designator == "S1")
        .expect("PRIMPROPS pad S1 not found");
    assert_eq!(pad.hole_shape, HoleShape::Slot, "slotted hole");
    assert!(
        pad.hole_size.is_some_and(|h| approx_eq(h, 0.508, 1e-6)),
        "20 mil slot width, got {:?}",
        pad.hole_size
    );
    assert!(
        approx_eq(pad.hole_slot_length, 1.016, 1e-6),
        "40 mil slot length, got {}",
        pad.hole_slot_length
    );
    assert!(
        approx_eq(pad.hole_rotation, 30.0, 1e-9),
        "slot rotated 30 degrees, got {}",
        pad.hole_rotation
    );
    assert!(!pad.is_plated, "authored unplated");
}

#[test]
fn samples_pcblib_text_stroke_fonts() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib.get("PRIMPROPS").expect("PRIMPROPS footprint not found");

    // The reader only surfaces a stroke font when the geometry's font id at @25
    // is above 1, so a library of default-font texts never reaches that arm.
    // FontID 2 = Sans Serif, 3 = Serif; both also carry a non-default 12 mil
    // stroke width, which no other text in the library sets.
    let by_text = |t: &str| {
        fp.text
            .iter()
            .find(|x| x.text == t)
            .unwrap_or_else(|| panic!("PRIMPROPS text {t:?} not found"))
    };

    let sans = by_text("SANS");
    assert_eq!(
        sans.kind,
        TextKind::Stroke,
        "authored as stroke, not TrueType"
    );
    assert_eq!(sans.stroke_font, Some(StrokeFont::SansSerif), "FontID 2");
    assert!(
        sans.stroke_width
            .is_some_and(|w| approx_eq(w, 0.3048, 1e-6)),
        "12 mil stroke width, got {:?}",
        sans.stroke_width
    );

    let serif = by_text("SERIF");
    assert_eq!(serif.kind, TextKind::Stroke);
    assert_eq!(serif.stroke_font, Some(StrokeFont::Serif), "FontID 3");
}

/// A borrowed list of per-primitive identity GUIDs, one entry per primitive.
type GuidList<'a> = Vec<&'a Option<String>>;

#[test]
fn samples_pcblib_primitive_guids_survive_a_rewrite() {
    let src = sample("footprints.PcbLib");
    let mut lib = PcbLib::open(&src).expect("failed to open footprints.PcbLib");

    // Identities ride on the primitives themselves, so a structural edit moves
    // them with their primitive instead of re-pointing the survivors. ARCS: two
    // arcs, each with its own GUID, plus the footprint record's own (kind 85).
    let arcs = lib.get("ARCS").expect("ARCS footprint not found");
    let (a, b) = (
        arcs.arcs[0].guid.as_deref().expect("arc 0 carries a GUID"),
        arcs.arcs[1].guid.as_deref().expect("arc 1 carries a GUID"),
    );
    assert_ne!(a, b, "each arc has its own identity");
    for guid in [a, b] {
        assert!(
            guid.starts_with('{') && guid.ends_with('}') && guid.len() == 38,
            "GUID is brace-wrapped and 36 characters wide, got {guid:?}"
        );
    }
    assert!(
        arcs.guid.is_some(),
        "the footprint record has a GUID of its own"
    );

    // The point of reading them is writing them back unchanged — on every
    // primitive of every footprint.
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("rewritten.PcbLib");
    lib.save(&out).expect("write the library back");
    let reread = PcbLib::open(&out).expect("reopen the rewritten library");

    for footprint in lib.iter() {
        let after = reread
            .get(&footprint.name)
            .unwrap_or_else(|| panic!("{} missing after rewrite", footprint.name));
        assert_eq!(
            after.guid, footprint.guid,
            "{}: footprint GUID",
            footprint.name
        );
        let pairs: [(&str, GuidList, GuidList); 8] = [
            (
                "arcs",
                footprint.arcs.iter().map(|p| &p.guid).collect(),
                after.arcs.iter().map(|p| &p.guid).collect(),
            ),
            (
                "pads",
                footprint.pads.iter().map(|p| &p.guid).collect(),
                after.pads.iter().map(|p| &p.guid).collect(),
            ),
            (
                "vias",
                footprint.vias.iter().map(|p| &p.guid).collect(),
                after.vias.iter().map(|p| &p.guid).collect(),
            ),
            (
                "tracks",
                footprint.tracks.iter().map(|p| &p.guid).collect(),
                after.tracks.iter().map(|p| &p.guid).collect(),
            ),
            (
                "text",
                footprint.text.iter().map(|p| &p.guid).collect(),
                after.text.iter().map(|p| &p.guid).collect(),
            ),
            (
                "regions",
                footprint.regions.iter().map(|p| &p.guid).collect(),
                after.regions.iter().map(|p| &p.guid).collect(),
            ),
            (
                "fills",
                footprint.fills.iter().map(|p| &p.guid).collect(),
                after.fills.iter().map(|p| &p.guid).collect(),
            ),
            (
                "bodies",
                footprint.component_bodies.iter().map(|p| &p.guid).collect(),
                after.component_bodies.iter().map(|p| &p.guid).collect(),
            ),
        ];
        for (kind, before_guids, after_guids) in pairs {
            assert_eq!(
                after_guids, before_guids,
                "{}: {kind} GUIDs changed across a rewrite",
                footprint.name
            );
        }
    }
}

#[test]
fn samples_pcblib_region_kinds_and_naming() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib.get("PRIMPROPS").expect("PRIMPROPS footprint not found");

    // One region per TRegionKind AD24 offers. Authoring all four is the only way
    // to pin the integer behind each name: nothing documents them, and a region
    // whose kind we misread is indistinguishable from a plain copper pour.
    let by_name = |n: &str| {
        fp.regions
            .iter()
            .find(|r| r.name == n)
            .unwrap_or_else(|| panic!("PRIMPROPS region {n:?} not found"))
    };

    // KIND=2. Also proves Name and UnionIndex survive: every other region in the
    // library is unnamed and in no union.
    let named = by_name("NamedPour");
    assert_eq!(named.kind, RegionKind::NamedRegion, "KIND=2");
    assert_eq!(named.union_index, 7, "authored union index");

    // KIND=4.
    assert_eq!(by_name("CavityRgn").kind, RegionKind::Cavity, "KIND=4");

    // KIND=1, the plain polygon cutout.
    assert_eq!(by_name("PlainCut").kind, RegionKind::Cutout, "KIND=1");

    // eRegionKind_BoardCutout is NOT a kind on disk: AD24 rewrites it as copper
    // on the keep-out layer with ISBOARDCUTOUT=TRUE, the same representation
    // samples_pcblib_region_cutout asserts. Named here so the four authored
    // kinds account for only three KIND integers.
    let cut = by_name("BoardCut");
    assert_eq!(cut.kind, RegionKind::Copper, "a board cutout stores KIND=0");
    assert_eq!(
        cut.layer,
        Layer::KeepOut,
        "and is moved to the keep-out layer"
    );
    assert!(
        cut.additional_parameters
            .iter()
            .any(|(k, v)| k == "ISBOARDCUTOUT" && v == "TRUE"),
        "the board-cutout flag is preserved, got {:?}",
        cut.additional_parameters
    );
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
    // Altium also MOVED the authored eTopLayer region onto the keep-out layer
    // (LAYER=KEEPOUT + KEEPOUT=TRUE in the same param block). Assert the real
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
    // The layer it was drawn on survives after all, in V7_LAYER: the layer byte
    // says keep-out, the token still says TOP. Nothing else in the record
    // records it, so deriving the token from `layer` would lose it.
    assert_eq!(
        r.v7_layer.as_deref(),
        Some("TOP"),
        "V7_LAYER keeps the authored layer even though the layer byte does not"
    );
}

#[test]
fn samples_pcblib_cutout_v7_layer_survives_a_read_modify_write() {
    use std::io::Cursor;

    let mut lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open golden");
    let mut buffer = Cursor::new(Vec::new());
    lib.write(&mut buffer).expect("write the golden back");
    buffer.set_position(0);
    let after = PcbLib::read(&mut buffer).expect("read back");

    let r = &after
        .get("REGION_CUTOUT")
        .expect("REGION_CUTOUT survives")
        .regions[0];
    assert_eq!(r.layer, Layer::KeepOut);
    assert_eq!(
        r.v7_layer.as_deref(),
        Some("TOP"),
        "re-deriving the token from the layer byte would write KEEPOUT and lose \
         the layer the cutout was drawn on"
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

    // Altium mints a fresh model GUID every time the samples are authored, so the
    // link is asserted rather than the literal: the body must reference a model the
    // library actually carries, in Altium's brace-wrapped GUID form. Pinning the
    // value would break on every regeneration for no added coverage.
    assert!(
        body.model_id.starts_with('{') && body.model_id.ends_with('}') && body.model_id.len() == 38,
        "model_id is a brace-wrapped GUID, got {:?}",
        body.model_id
    );
    assert!(
        lib.models()
            .any(|m| m.id.eq_ignore_ascii_case(&body.model_id)),
        "the body must reference one of the library's embedded models, got {:?}",
        body.model_id
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

    // The referenced model must actually exist in the library's model store,
    // resolved through the same lookup the reader uses (case-insensitive). The
    // store holds two entries: this embedded one and STEP_REF's referenced one.
    assert_eq!(lib.model_count(), 2, "EMBSTEP's model and STEP_REF's");
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
    // Real-Altium ground truth for TextKind::BarCode and the inverted
    // text-box descriptor (offsets 110-133), beyond a
    // self-round-trip. Matched by content; on-disk order is not
    // guaranteed.
    assert_eq!(fp.text.len(), 5, "TEXT_SPECIAL has five text items");
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
    // InvertedTTTextBorder = 10 mil, with an EXPLICIT 120 x 70 mil rectangle.
    // The size used to be left to Altium, but AD computes it lazily from the
    // rendered text extent and a headless save can catch it at 0 (the
    // 2026-09-01 regeneration did) — so the generator authors it.
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
    assert!(
        approx_eq(
            inv.inverted_rect_width.expect("INV has a rect width"),
            3.048,
            1e-6
        ),
        "INV inverted_rect_width: expected 120 mil = 3.048 mm, got {:?}",
        inv.inverted_rect_width,
    );
    assert!(
        approx_eq(
            inv.inverted_rect_height.expect("INV has a rect height"),
            1.778,
            1e-6
        ),
        "INV inverted_rect_height: expected 70 mil = 1.778 mm, got {:?}",
        inv.inverted_rect_height,
    );
    assert_eq!(
        inv.inverted_rect_text_offset, None,
        "INV text offset was not authored (zero on disk reads back None)"
    );

    // Barcode sizing block. Two barcodes were authored with every field different,
    // and each offset identified by the authored value appearing there: @137/@141
    // the 400x100 and 600x150 mil sizes, @145/@149 the margins, @157 the symbology,
    // @161 the font as UTF-16LE. BC2 is what pins them; BC128 is the control for
    // the margins it never set.
    let bc2 = by_content("BC2");
    assert_eq!(bc2.kind, TextKind::BarCode);
    assert_eq!(bc2.barcode_kind, 1, "Code128");
    assert!(approx_eq(
        bc2.barcode_full_width.expect("width"),
        15.24,
        1e-3
    ));
    assert!(approx_eq(
        bc2.barcode_full_height.expect("height"),
        3.81,
        1e-3
    ));
    assert!(approx_eq(
        bc2.barcode_x_margin.expect("x margin"),
        0.762,
        1e-3
    ));
    assert!(approx_eq(
        bc2.barcode_y_margin.expect("y margin"),
        1.016,
        1e-3
    ));
    assert_eq!(bc2.barcode_font_name, "Courier New");
    assert!(bc2.barcode_inverted, "BC2 is authored inverted");
    assert!(bc2.barcode_show_text, "and with its readable line shown");

    // BC3 and BC4 differ from BC2 in exactly one field each, which is what pins
    // @159 and @225 to the right flag rather than to each other.
    let bc3 = by_content("BC3");
    assert!(!bc3.barcode_inverted, "BC3 turns Inverted off");
    assert!(bc3.barcode_show_text, "and leaves ShowText alone");
    let bc4 = by_content("BC4");
    assert!(bc4.barcode_inverted, "BC4 leaves Inverted alone");
    assert!(!bc4.barcode_show_text, "and turns ShowText off");

    let bc1 = by_content("BC128");
    assert!(approx_eq(
        bc1.barcode_full_width.expect("width"),
        10.16,
        1e-3
    ));
    assert_eq!(bc1.barcode_font_name, "Arial", "the default barcode font");

    // A non-barcode text carries none of it, so the block cannot be read
    // unconditionally out of the template bytes every text record has.
    let inv = by_content("INV");
    assert_eq!(inv.barcode_kind, 0);
    assert!(inv.barcode_full_width.is_none());
    assert!(inv.barcode_font_name.is_empty());
}

#[test]
fn samples_pcblib_golden_survives_a_read_modify_write() {
    // The whole library is read from the Altium-authored golden and written
    // back through our writer, in memory. A footprint Altium can author must be
    // saveable: TEXT_LONG's 264-character string exceeds the 255-byte Pascal
    // short string in block 1, so the writer has to truncate that block and let
    // /WideStrings carry the full value, exactly as Altium does.
    use std::io::Cursor;

    let mut lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open golden");
    let before = lib.len();

    let mut buffer = Cursor::new(Vec::new());
    lib.write(&mut buffer)
        .expect("the golden library must be writable");
    buffer.set_position(0);
    let round_tripped = PcbLib::read(&mut buffer).expect("read back");

    assert_eq!(round_tripped.len(), before, "no footprint lost");

    let long = round_tripped
        .get("TEXT_LONG")
        .expect("TEXT_LONG survives")
        .text
        .iter()
        .find(|t| t.text.len() > 255)
        .expect("the 264-character text keeps its full length");
    assert_eq!(long.text, "A".repeat(260) + "_END");
}

#[test]
fn samples_pcblib_every_guid_record_attaches_to_its_primitive() {
    use std::io::Read as _;

    // The stream keys each GUID by the primitive's ordinal among ALL of the
    // footprint's primitives. The reader attaches each record to the primitive
    // at that ordinal (kind cross-checked), so for the golden — whose ordinal
    // space is known-good — every non-85 record must land: the multiset of
    // attached GUIDs equals the multiset in the raw stream.
    let path = sample("footprints.PcbLib");
    let lib = PcbLib::open(&path).expect("failed to open footprints.PcbLib");
    let file = std::fs::File::open(&path).expect("open");
    let mut cfb = cfb::CompoundFile::open(file).expect("parse OLE");

    let mut checked = 0;
    for footprint in lib.iter() {
        let stream = format!("/{}/PrimitiveGuids/Data", footprint.name);
        let Ok(mut s) = cfb.open_stream(&stream) else {
            continue;
        };
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).expect("read stream");

        // Raw records: [kind: u32][ordinal: u32][guid: 16 bytes].
        let mut stream_guids: Vec<(u32, [u8; 16])> = raw
            .chunks_exact(24)
            .map(|r| {
                let kind = u32::from_le_bytes([r[0], r[1], r[2], r[3]]);
                let mut g = [0u8; 16];
                g.copy_from_slice(&r[8..24]);
                (kind, g)
            })
            .collect();
        stream_guids.retain(|&(kind, _)| kind != 85);

        let mut attached: Vec<&str> = footprint
            .arcs
            .iter()
            .map(|p| p.guid.as_deref())
            .chain(footprint.pads.iter().map(|p| p.guid.as_deref()))
            .chain(footprint.vias.iter().map(|p| p.guid.as_deref()))
            .chain(footprint.tracks.iter().map(|p| p.guid.as_deref()))
            .chain(footprint.text.iter().map(|p| p.guid.as_deref()))
            .chain(footprint.regions.iter().map(|p| p.guid.as_deref()))
            .chain(footprint.fills.iter().map(|p| p.guid.as_deref()))
            .chain(footprint.component_bodies.iter().map(|p| p.guid.as_deref()))
            .flatten()
            .collect();
        attached.sort_unstable();

        assert_eq!(
            attached.len(),
            stream_guids.len(),
            "{}: every non-85 record must attach to exactly one primitive",
            footprint.name
        );
        assert!(
            footprint.guid.is_some(),
            "{}: the kind-85 record becomes the footprint's own GUID",
            footprint.name
        );
        checked += 1;
    }
    assert!(
        checked >= 20,
        "only {checked} footprints carried GUID streams"
    );
}

#[test]
fn samples_pcblib_primitive_order_survives_a_read_modify_write() {
    use std::io::Cursor;

    let mut lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open golden");
    let before: Vec<(String, Vec<PrimitiveKind>)> = lib
        .iter()
        .map(|f| (f.name.clone(), f.primitive_order.clone()))
        .collect();

    let mut buffer = Cursor::new(Vec::new());
    lib.write(&mut buffer).expect("write the golden back");
    buffer.set_position(0);
    let after = PcbLib::read(&mut buffer).expect("read back");

    for (name, order) in before {
        let rewritten = after
            .get(&name)
            .unwrap_or_else(|| panic!("{name} missing after rewrite"));
        assert_eq!(
            rewritten.primitive_order, order,
            "{name}: primitives came back in a different order, which \
             renumbers every PrimitiveGuids ordinal and PRIMITIVEINDEX"
        );
    }
}

#[test]
fn samples_pcblib_layer_stack_survives_a_read_modify_write() {
    use std::io::Cursor;

    /// The value of one key in a raw `|KEY=VALUE|` parameter block.
    fn param(block: &[u8], key: &str) -> Option<String> {
        block.split(|&b| b == b'|').find_map(|seg| {
            let eq = seg.iter().position(|&b| b == b'=')?;
            (seg[..eq] == *key.as_bytes())
                .then(|| String::from_utf8_lossy(&seg[eq + 1..]).into_owned())
        })
    }

    /// The block minus the three keys that describe one particular save.
    fn stack_only(block: &[u8]) -> Vec<Vec<u8>> {
        block
            .split(|&b| b == b'|')
            .filter(|seg| {
                let key = seg.split(|&b| b == b'=').next().unwrap_or_default();
                !["FILENAME", "DATE", "TIME"]
                    .iter()
                    .any(|k| key == k.as_bytes())
            })
            .map(<[u8]>::to_vec)
            .collect()
    }

    let mut lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open golden");
    let before = lib
        .metadata()
        .library_params
        .clone()
        .expect("the golden carries a layer stack");

    // Altium named this layer; our template stack calls slot 12 "Top Assembly".
    // Keeping the authored name is the point — a designer's "Mechanical 15 =
    // Assembly Top" must not be renamed by a read-modify-write.
    assert_eq!(
        param(&before, "LAYER_V8_12NAME").as_deref(),
        Some("Mechanical 2"),
        "the golden's own name for mechanical layer 2"
    );

    let mut buffer = Cursor::new(Vec::new());
    lib.write(&mut buffer).expect("write the golden back");
    buffer.set_position(0);
    let after = PcbLib::read(&mut buffer)
        .expect("read back")
        .metadata()
        .library_params
        .clone()
        .expect("the rewritten library carries a layer stack");

    assert_eq!(
        stack_only(&after),
        stack_only(&before),
        "the library's own board configuration must come back byte-for-byte: \
         layer names, enabled flags, USEDBYPRIMS, layer sets and per-layer ids"
    );

    // A library with no stack of its own falls back to the template, which is
    // what makes the assertion above meaningful rather than tautological.
    let mut fresh = PcbLib::new();
    let mut buffer = Cursor::new(Vec::new());
    fresh.write(&mut buffer).expect("write an empty library");
    buffer.set_position(0);
    let template = PcbLib::read(&mut buffer)
        .expect("read back")
        .metadata()
        .library_params
        .clone()
        .expect("a written library always has a stack");
    assert_eq!(
        param(&template, "LAYER_V8_12NAME").as_deref(),
        Some("Top Assembly"),
        "the template stack names slot 12 differently from the golden"
    );
}

#[test]
fn samples_pcblib_padmask_expansions() {
    // Manual paste / solder-mask expansion, authored by Altium.
    //
    // These live behind the pad's cache record rather than as direct scripting
    // properties, so until now they were only ever exercised by a self-round-trip
    // — our writer and reader agreeing with each other proves nothing about the
    // on-disk values. Pad 2 is the control: an untouched pad keeps the rule-driven
    // default, so a reader that hard-coded Manual would fail here.
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib.get("PADMASK").expect("footprint PADMASK not found");

    let pad = |d: &str| {
        fp.pads
            .iter()
            .find(|p| p.designator == d)
            .unwrap_or_else(|| panic!("pad {d} not found"))
    };

    // Authored as 7 mil solder / 3 mil paste.
    let manual = pad("1");
    assert_eq!(manual.solder_mask_expansion_mode, MaskExpansionMode::Manual);
    assert_eq!(manual.paste_mask_expansion_mode, MaskExpansionMode::Manual);
    assert!(
        approx_eq(manual.solder_mask_expansion.expect("solder"), 0.1778, 1e-4),
        "solder mask 7 mil, got {:?}",
        manual.solder_mask_expansion
    );
    assert!(
        approx_eq(manual.paste_mask_expansion.expect("paste"), 0.0762, 1e-4),
        "paste mask 3 mil, got {:?}",
        manual.paste_mask_expansion
    );

    let default_pad = pad("2");
    assert_eq!(
        default_pad.solder_mask_expansion_mode,
        MaskExpansionMode::None,
        "an untouched pad stays rule-driven"
    );
    assert_eq!(
        default_pad.paste_mask_expansion_mode,
        MaskExpansionMode::None
    );

    // Pad 3 measures its solder-mask expansion from the hole edge (main-block bool
    // @125). The offset was derived by diffing this pad against pad 1 byte by byte —
    // every other difference between them is geometry — so pads 1 and 2 double as the
    // controls that keep the flag from being read unconditionally.
    assert!(
        pad("3").solder_mask_expansion_from_hole_edge,
        "pad 3 measures mask expansion from the hole edge"
    );
    for d in ["1", "2"] {
        assert!(
            !pad(d).solder_mask_expansion_from_hole_edge,
            "pad {d} measures from the pad edge"
        );
    }
}

#[test]
fn samples_pcblib_via_mask_state_is_altium_factory_default() {
    // Ground truth for the mask-expansion tri-state on a via, which is Altium's
    // `TCacheState = (eCacheInvalid, eCacheValid, eCacheManual)`.
    //
    // An Altium-authored via carries byte @66 = 0 (`eCacheInvalid`), meaning the
    // stored expansion is stale. Our from-scratch via writes the same byte, so
    // this pins the two together. Altium recomputes a rule-driven expansion on
    // load either way (`scripts/Verify-MaskCacheState.ps1`), so what this
    // protects is fidelity with a factory via, not the resulting mask.
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib.get("VIAS").expect("footprint VIAS not found");
    assert_eq!(fp.vias.len(), 2);

    for (i, via) in fp.vias.iter().enumerate() {
        assert_eq!(
            via.solder_mask_expansion_mode,
            MaskExpansionMode::None,
            "via {i} keeps the factory cache state"
        );
        assert!(
            approx_eq(via.solder_mask_expansion, 0.1016, 1e-4),
            "via {i} carries Altium's 4 mil template expansion, got {}",
            via.solder_mask_expansion
        );
        assert!(
            via.paste_mask_expansion.abs() < 1e-9,
            "a via has no paste by default, got {}",
            via.paste_mask_expansion
        );
        // @258 / @312 are 0 in Altium's via template. Asserting the decoded defaults
        // catches a reader that picked the wrong offsets in a 321-byte block, which a
        // self-round-trip could not.
        assert!(
            !via.solder_mask_expansion_from_hole_edge,
            "via {i} measures mask expansion from the pad edge"
        );
        assert_eq!(
            via.drill_layer_pair_type,
            DrillLayerPairType::Through,
            "via {i} is a through via"
        );
    }
}

#[test]
fn samples_pcblib_locked_flag() {
    // The LOCKED bit of the shared common-header flag word, authored by Altium
    // (`Moveable := False`). A pad and a track cover the two record shapes that
    // carry it; the unlocked pad is the control, so a reader that sets LOCKED
    // unconditionally fails here rather than passing.
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib
        .get("LOCKFLAGS_PCB")
        .expect("footprint LOCKFLAGS_PCB not found");

    let pad = |d: &str| {
        fp.pads
            .iter()
            .find(|p| p.designator == d)
            .unwrap_or_else(|| panic!("pad {d} not found"))
    };
    assert!(
        pad("1").flags.contains(PcbFlags::LOCKED),
        "pad 1 is authored locked"
    );
    assert!(
        !pad("2").flags.contains(PcbFlags::LOCKED),
        "pad 2 is the unlocked control"
    );

    // KEEPOUT is the other bit of the same word, and must not bleed into LOCKED.
    let keepout = pad("3");
    assert!(
        keepout.flags.contains(PcbFlags::KEEPOUT),
        "pad 3 is authored as a keepout"
    );
    assert!(
        !keepout.flags.contains(PcbFlags::LOCKED),
        "keepout must not imply locked"
    );
    assert!(
        !pad("1").flags.contains(PcbFlags::KEEPOUT),
        "and locked must not imply keepout"
    );

    // Every record shape that carries the shared flag word is authored with each
    // bit, so no shape is left resting on a self-round-trip. Each is matched by the
    // flag rather than by index, and the opposite bit is asserted absent, so a
    // reader that decoded the word into the wrong shape's field would fail here.
    assert_eq!(fp.tracks.len(), 2, "LOCKFLAGS_PCB has two tracks");
    assert_eq!(fp.arcs.len(), 2, "LOCKFLAGS_PCB has two arcs");
    assert_eq!(fp.fills.len(), 2, "LOCKFLAGS_PCB has two fills");

    let locked_track = fp
        .tracks
        .iter()
        .find(|t| t.flags.contains(PcbFlags::LOCKED))
        .expect("a locked track");
    assert!(
        !locked_track.flags.contains(PcbFlags::KEEPOUT),
        "the locked track is not also a keepout"
    );
    let keepout_track = fp
        .tracks
        .iter()
        .find(|t| t.flags.contains(PcbFlags::KEEPOUT))
        .expect("a keepout track");
    assert!(
        !keepout_track.flags.contains(PcbFlags::LOCKED),
        "the keepout track is not also locked"
    );

    let locked_arc = fp
        .arcs
        .iter()
        .find(|a| a.flags.contains(PcbFlags::LOCKED))
        .expect("a locked arc");
    assert!(
        !locked_arc.flags.contains(PcbFlags::KEEPOUT),
        "the locked arc is not also a keepout"
    );
    let keepout_arc = fp
        .arcs
        .iter()
        .find(|a| a.flags.contains(PcbFlags::KEEPOUT))
        .expect("a keepout arc");
    assert!(
        !keepout_arc.flags.contains(PcbFlags::LOCKED),
        "the keepout arc is not also locked"
    );

    let locked_fill = fp
        .fills
        .iter()
        .find(|f| f.flags.contains(PcbFlags::LOCKED))
        .expect("a locked fill");
    assert!(
        !locked_fill.flags.contains(PcbFlags::KEEPOUT),
        "the locked fill is not also a keepout"
    );
    let keepout_fill = fp
        .fills
        .iter()
        .find(|f| f.flags.contains(PcbFlags::KEEPOUT))
        .expect("a keepout fill");
    assert!(
        !keepout_fill.flags.contains(PcbFlags::LOCKED),
        "the keepout fill is not also locked"
    );

    // The plain arcs and fills elsewhere in the library are the controls: an
    // unconditional LOCKED or KEEPOUT would light up here.
    let arcs = lib.get("ARCS").expect("footprint ARCS not found");
    assert!(
        arcs.arcs.iter().all(|a| a.flags.is_empty()),
        "the ARCS arcs carry no flags"
    );
    let fills = lib.get("FILLS").expect("footprint FILLS not found");
    assert!(
        fills.fills.iter().all(|f| f.flags.is_empty()),
        "the FILLS fills carry no flags"
    );
}

#[test]
fn samples_pcblib_jumper_group() {
    // Jumper group (main-block i16 @110-111), authored by Altium: pads 7 and 8 share
    // id 4 while every other pad keeps 0, so a reader that hard-coded either value
    // fails rather than passes.
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib
        .get("LOCKFLAGS_PCB")
        .expect("footprint LOCKFLAGS_PCB not found");
    let pad = |d: &str| {
        fp.pads
            .iter()
            .find(|p| p.designator == d)
            .unwrap_or_else(|| panic!("pad {d} not found"))
    };
    assert_eq!(pad("7").jumper_id, 4, "pad 7 is in jumper group 4");
    assert_eq!(pad("8").jumper_id, 4, "and so is pad 8");
    for d in ["1", "2", "3", "4", "5", "6"] {
        assert_eq!(pad(d).jumper_id, 0, "pad {d} is in no jumper group");
    }
}

#[test]
fn samples_pcblib_testpoint_flags() {
    // Issue #334: a pad authored with only IsTestPoint_Top read back as LOCKED and
    // reported nothing for the flag that was set. Both halves are explained here.
    //
    // The LOCKED was real — Altium clears a primitive's unlocked bit when it marks
    // it as a test point, confirmed against the raw flag word (0x0088 top / 0x0108
    // bottom, against a control pad's 0x000C). The silence was ours: PcbFlags did
    // not model the test-point bits at all, so they were dropped on read.
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib
        .get("LOCKFLAGS_PCB")
        .expect("footprint LOCKFLAGS_PCB not found");
    let pad = |d: &str| {
        fp.pads
            .iter()
            .find(|p| p.designator == d)
            .unwrap_or_else(|| panic!("pad {d} not found"))
    };
    // Fabrication test points (issue #334). Altium clears the primitive's unlocked
    // bit when it marks a test point, so these read LOCKED as well — that is
    // Altium's own behaviour, verified against the raw flag word (0x0088 / 0x0108
    // against a control pad's 0x000C), not a decode defect.
    let tp_top = pad("5");
    assert!(
        tp_top.flags.contains(PcbFlags::TESTPOINT_TOP),
        "pad 5 is a top-side fabrication test point"
    );
    assert!(
        !tp_top.flags.contains(PcbFlags::TESTPOINT_BOTTOM),
        "and not a bottom-side one"
    );
    assert!(
        tp_top.flags.contains(PcbFlags::LOCKED),
        "Altium locks a pad it marks as a test point"
    );
    let tp_bottom = pad("6");
    assert!(
        tp_bottom.flags.contains(PcbFlags::TESTPOINT_BOTTOM),
        "pad 6 is a bottom-side fabrication test point"
    );
    assert!(
        !tp_bottom.flags.contains(PcbFlags::TESTPOINT_TOP),
        "and not a top-side one"
    );
    // The control pads prove the bits are not set unconditionally.
    for d in ["1", "2", "3", "4"] {
        let p = pad(d);
        assert!(
            !p.flags
                .intersects(PcbFlags::TESTPOINT_TOP | PcbFlags::TESTPOINT_BOTTOM),
            "pad {d} carries no test-point flag"
        );
    }
}

#[test]
fn samples_pcblib_drill_tolerances() {
    // Positive / negative drill tolerance (extended-tail i32 @162/@166), authored
    // by Altium as +3 mil / -2 mil. The other pads in the footprint are the
    // control: an absent tolerance must read as None, not as a zero, because the
    // writer distinguishes the two and a zero would be written back as an
    // explicit tolerance.
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib
        .get("LOCKFLAGS_PCB")
        .expect("footprint LOCKFLAGS_PCB not found");

    let pad = |d: &str| {
        fp.pads
            .iter()
            .find(|p| p.designator == d)
            .unwrap_or_else(|| panic!("pad {d} not found"))
    };

    let toleranced = pad("4");
    assert!(
        approx_eq(
            toleranced.hole_positive_tolerance.expect("+tolerance"),
            0.0762,
            1e-4
        ),
        "+3 mil, got {:?}",
        toleranced.hole_positive_tolerance
    );
    assert!(
        approx_eq(
            toleranced.hole_negative_tolerance.expect("-tolerance"),
            0.0508,
            1e-4
        ),
        "-2 mil, got {:?}",
        toleranced.hole_negative_tolerance
    );

    for d in ["1", "2", "3"] {
        assert_eq!(
            pad(d).hole_positive_tolerance,
            None,
            "pad {d} has no authored tolerance"
        );
        assert_eq!(pad(d).hole_negative_tolerance, None, "pad {d}");
    }
}

/// The hand-authored `manual/identifier.PcbLib` (AD24 UI, 2026-08-16): one
/// footprint, two extruded bodies whose `IDENTIFIER` values settle the on-disk
/// encoding — comma-separated decimal Unicode code points (`µΩ电` =
/// `181,937,30005`) — and whose `MODEL.*` groups prove a UI-authored extruded
/// body carries a stable `MODELID`, real `MODEL.2D.X/Y` placement, and
/// `MODEL.EXTRUDED.MINZ/MAXZ`, unlike the script-authored golden's bodies.
///
/// Bodies are matched by identifier, never by index: a twin-save experiment
/// showed AD reorders bodies between saves of the same in-memory state.
#[test]
fn samples_manual_identifier_bodies_read_exactly() {
    let lib = PcbLib::open(sample("manual/identifier.PcbLib"))
        .expect("failed to open manual/identifier.PcbLib");
    let fp = lib.get("BODY_IDENT").expect("BODY_IDENT footprint");
    assert_eq!(fp.component_bodies.len(), 2, "two bodies");

    let by_ident = |ident: &str| {
        fp.component_bodies
            .iter()
            .find(|b| b.identifier == ident)
            .unwrap_or_else(|| panic!("body {ident:?} not found"))
    };

    let ascii = by_ident("BodyA");
    assert!(
        approx_eq(ascii.overall_height, 1.0, 1e-4),
        "BodyA is 1 mm tall"
    );
    let uni = by_ident("\u{00B5}\u{03A9}\u{7535}"); // µΩ电
    assert!(
        approx_eq(uni.overall_height, 0.5, 1e-4),
        "µΩ电 is 0.5 mm tall"
    );

    for body in [ascii, uni] {
        assert!(
            body.model_id.starts_with('{') && body.model_id.len() == 38,
            "a UI-authored extruded body carries a MODELID: {:?}",
            body.model_id
        );
        assert_ne!(body.model_checksum, 0, "and a checksum");
        assert_eq!(
            body.texture_size_x.as_deref(),
            Some("0.0001mil"),
            "the UI's texture size is 0.0001mil, not the scripted 0mil"
        );
    }
    // Real 2D placement, from dragging the bodies in the editor.
    assert!(
        approx_eq(uni.model_2d_x, -0.5715, 1e-4),
        "µΩ电 MODEL.2D.X (-22.5 mil)"
    );
    assert!(
        approx_eq(uni.model_2d_y, -2.159, 1e-4),
        "µΩ电 MODEL.2D.Y (-85 mil)"
    );
    assert!(
        approx_eq(ascii.model_2d_x, -1.905, 1e-4),
        "BodyA MODEL.2D.X (-75 mil)"
    );
    assert!(
        approx_eq(ascii.model_2d_y, 1.016, 1e-4),
        "BodyA MODEL.2D.Y (40 mil)"
    );
}

/// Every body field of the manual identifier fixture survives our own
/// write -> read, including the fields only this fixture exercises.
#[test]
fn samples_manual_identifier_survives_a_write_read_cycle() {
    use std::io::Cursor;

    let mut lib =
        PcbLib::open(sample("manual/identifier.PcbLib")).expect("open manual/identifier.PcbLib");
    let mut buffer = Cursor::new(Vec::new());
    lib.write(&mut buffer).expect("write");
    buffer.set_position(0);
    let reread = PcbLib::read(&mut buffer).expect("read back");

    let before = lib.get("BODY_IDENT").expect("before");
    let after = reread.get("BODY_IDENT").expect("after");
    for b in &before.component_bodies {
        let a = after
            .component_bodies
            .iter()
            .find(|a| a.identifier == b.identifier)
            .unwrap_or_else(|| panic!("body {:?} lost", b.identifier));
        assert_eq!(a.model_id, b.model_id, "{}: MODELID", b.identifier);
        assert_eq!(
            a.model_checksum, b.model_checksum,
            "{}: checksum",
            b.identifier
        );
        assert_eq!(
            a.texture_size_x, b.texture_size_x,
            "{}: texture size",
            b.identifier
        );
        assert!(
            approx_eq(a.model_2d_x, b.model_2d_x, 1e-6),
            "{}: 2D X",
            b.identifier
        );
        assert!(
            approx_eq(a.model_2d_y, b.model_2d_y, 1e-6),
            "{}: 2D Y",
            b.identifier
        );
        assert!(
            approx_eq(a.overall_height, b.overall_height, 1e-6),
            "{}: height",
            b.identifier
        );
    }
}

/// `STEP_REF` (batch 5): the same STEP model attached with `IPCB_Model.Embed`
/// cleared. A script-authored non-embedded model is NOT the form a UI-authored
/// library showed (an empty `MODELID` and no store entry): Altium still gives
/// the body a `MODELID`, lists the model in `/Library/Models/Data` with
/// `EMBED=FALSE`, and stores its bytes all the same. Both forms exist on disk
/// and both read; the writer reproduces whichever it read.
#[test]
fn samples_pcblib_step_ref_is_a_referenced_model_with_a_store_entry() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib.get("STEP_REF").expect("STEP_REF footprint not found");
    assert_eq!(fp.component_bodies.len(), 1, "one body");
    let body = &fp.component_bodies[0];
    assert!(!body.embedded, "MODEL.EMBED=FALSE reads as not embedded");
    assert!(
        body.model_id.starts_with('{') && body.model_id.len() == 38,
        "a referenced model still carries a MODELID: {:?}",
        body.model_id
    );
    assert_eq!(
        body.model_name, "minimal.step",
        "MODEL.NAME is the file name"
    );
    assert_eq!(body.model_checksum, 1_975_055, "the same STEP as EMBSTEP's");
    assert_eq!(body.layer, Layer::Mechanical1);

    let model = lib
        .get_model(&body.model_id)
        .expect("the referenced model has a store entry");
    assert_eq!(model.name, "minimal.step");
    assert_eq!(
        model.compressed_size, 189,
        "its bytes are stored regardless"
    );
    assert_eq!(model.data.len(), 267);
    // EMBSTEP's embedded copy is a distinct entry with its own id.
    let embedded = &lib.get("EMBSTEP").expect("EMBSTEP").component_bodies[0];
    assert_ne!(embedded.model_id, body.model_id);
}

/// `MECH20` (batch 5): one primitive of every layered kind on Mechanical 20.
/// Altium stores each under header byte 72 (the last the legacy byte can
/// name) with the real layer in the V7 id (`0x01020014`) or, for the region
/// and the body, the `V7_LAYER=MECHANICAL20` token — the pair hand-authored
/// tracks had shown, now seen on every kind. Every one reads as Mechanical 20
/// with nothing carried: the writer stores the layer the same way.
#[test]
fn samples_pcblib_mech20_every_kind_resolves_past_the_legacy_sixteen() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib.get("MECH20").expect("MECH20 footprint not found");

    assert_eq!(fp.tracks.len(), 1);
    assert_eq!(fp.tracks[0].layer, Layer::Mechanical20, "track");
    assert_eq!(fp.tracks[0].raw_layer_id, None, "nothing to carry");
    assert_eq!(fp.arcs.len(), 1);
    assert_eq!(fp.arcs[0].layer, Layer::Mechanical20, "arc");
    assert_eq!(fp.arcs[0].raw_layer_id, None);
    assert_eq!(fp.text.len(), 1);
    assert_eq!(fp.text[0].layer, Layer::Mechanical20, "text");
    assert_eq!(fp.text[0].text, "M20");
    assert_eq!(fp.text[0].raw_layer_id, None);
    assert_eq!(fp.fills.len(), 1);
    assert_eq!(fp.fills[0].layer, Layer::Mechanical20, "fill");
    assert_eq!(fp.fills[0].raw_layer_id, None);
    assert_eq!(fp.regions.len(), 1);
    assert_eq!(fp.regions[0].layer, Layer::Mechanical20, "region");
    assert_eq!(
        fp.regions[0].v7_layer, None,
        "the MECHANICAL20 token follows from the layer"
    );
    assert_eq!(fp.pads.len(), 1);
    assert_eq!(fp.pads[0].layer, Layer::Mechanical20, "pad");
    assert_eq!(fp.pads[0].designator, "M");
    assert_eq!(fp.pads[0].raw_layer_id, None);
    assert_eq!(fp.component_bodies.len(), 1);
    assert_eq!(fp.component_bodies[0].layer, Layer::Mechanical20, "body");
    assert_eq!(fp.component_bodies[0].raw_layer_id, None);
    assert_eq!(fp.component_bodies[0].v7_layer, None);
}

/// `TEXT_WIDE_ONLY` (batch 5): `WideStrings` is the authoritative form of a
/// text and `ENCODEDTEXT` holds its UTF-16 code units. The second text has a
/// character the Data stream cannot hold — U+0094, which Altium narrowed to
/// `?` — so only the wide form carries it, the shape a UI-typed Ω has.
#[test]
fn samples_pcblib_text_wide_only_reads_the_wide_form() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let fp = lib
        .get("TEXT_WIDE_ONLY")
        .expect("TEXT_WIDE_ONLY footprint not found");
    assert_eq!(fp.text.len(), 2, "two texts");
    let by_y = |y: f64| {
        fp.text
            .iter()
            .find(|t| approx_eq(t.y, y, 1e-6))
            .unwrap_or_else(|| panic!("no text at y = {y} mm"))
    };
    // Latin-1 characters narrow and widen losslessly.
    assert_eq!(by_y(0.0).text, "10 \u{ce}\u{a9}");
    // U+0094 is `?` in the Data stream and 148 in ENCODEDTEXT1; the wide form wins.
    assert_eq!(by_y(2.54).text, "\u{94}Q\u{bb}");
}

/// `manual/pipe.PcbLib` (AD24, scripted 2026-08-30): a footprint whose
/// description and region name were given a `|` through Altium's own API.
/// Altium writes them raw (`DESCRIPTION=A|B=C`) and, opening the file again,
/// reads the description back as `A` (its `description_lengths` reports 1
/// through `scripts/Verify-Libraries.ps1`) — so does this reader, and the
/// writer refuses a `|` rather than write text that comes back cut.
#[test]
fn manual_pipe_fixture_shows_altium_cuts_pcb_text_at_the_pipe() {
    let lib = PcbLib::open(sample("manual/pipe.PcbLib")).expect("open manual/pipe.PcbLib");
    let fp = lib.get("PIPEFP").expect("footprint PIPEFP not found");
    assert_eq!(fp.description, "A");
    assert_eq!(fp.regions[0].name, "R");
}

/// BODYPREC: an extruded body whose outline was authored OFF-GRID by raw
/// internal units (1 unit = 0.0001 mil) — `MilsToCoord(-50) + 1` and friends.
/// AD24 keeps the exact units through its save, so this golden pins the
/// reader's full outline precision: each coordinate converts back to the
/// authored unit count exactly, with no rounding to a coarser grid.
#[test]
fn samples_pcblib_bodyprec() {
    let lib = PcbLib::open(sample("footprints.PcbLib")).expect("failed to open footprints.PcbLib");
    let footprint = lib.get("BODYPREC").expect("footprint BODYPREC not found");
    assert_eq!(footprint.component_bodies.len(), 1, "BODYPREC has one body");

    let body = &footprint.component_bodies[0];
    assert_eq!(body.outline.len(), 4, "outline is a 4-vertex box");

    // The authored vertices in internal units (10_000 units = 1 mil; one unit
    // is 2.54e-6 mm). Altium re-anchors the contour cycle, so match as a set.
    let authored: [(i64, i64); 4] = [
        (-499_999, -249_997),
        (500_007, -250_001),
        (499_997, 250_009),
        (-500_009, 249_993),
    ];
    // A rounded ±0.5 mm coordinate is far inside i64 range.
    #[allow(clippy::cast_possible_truncation)]
    let mut read_units: Vec<(i64, i64)> = body
        .outline
        .iter()
        .map(|&(x, y)| ((x / 2.54e-6).round() as i64, (y / 2.54e-6).round() as i64))
        .collect();
    read_units.sort_unstable();
    let mut expected = authored.to_vec();
    expected.sort_unstable();
    assert_eq!(
        read_units, expected,
        "every off-grid outline unit survives exactly (got {:?})",
        body.outline
    );

    // And the conversion itself is sub-unit accurate: no coordinate is off by
    // more than a billionth of a millimetre from its unit-exact position.
    for &(x, y) in &body.outline {
        let xu = (x / 2.54e-6).round() * 2.54e-6;
        let yu = (y / 2.54e-6).round() * 2.54e-6;
        assert!(
            (x - xu).abs() < 1e-9 && (y - yu).abs() < 1e-9,
            "outline point ({x}, {y}) sits on an exact unit"
        );
    }
}

// ---------------------------------------------------------------------------
// manual/i18n4.PcbLib — Unicode names in a PcbLib, as the AD24 UI writes them
// ---------------------------------------------------------------------------

/// The four footprints of `manual/i18n4.PcbLib` (AD24 UI on a Windows-1250
/// machine, 2026-09-15), each with one pad and its name pasted as the
/// description: the name, the CFB storage name, and the description.
///
/// The reader takes the name and description from the `UNICODE__PATTERN` and
/// `UNICODE__DESCRIPTION` twins, so each reads as its real text although the
/// plain keys hold Altium's `?` husks and, for `ČĐŽ`, the Windows-1250 bytes
/// `C8 D0 8E`. The storage name is the real Unicode name within the 31-unit
/// cap and the ANSI form cut at 31 beyond it.
const I18N4_FOOTPRINTS: [(&str, &str, &str); 4] = [
    // ᏣᎳᎩ_CR_0402: Cherokee, outside every legacy code page.
    (
        "\u{13E3}\u{13B3}\u{13A9}_CR_0402",
        "\u{13E3}\u{13B3}\u{13A9}_CR_0402",
        "\u{13E3}\u{13B3}\u{13A9}_CR_0402",
    ),
    // ČĐŽ_SL_0402: inside Windows-1250, outside Windows-1252.
    (
        "\u{10C}\u{110}\u{17D}_SL_0402",
        "\u{10C}\u{110}\u{17D}_SL_0402",
        "\u{10C}\u{110}\u{17D}_SL_0402",
    ),
    // 36 units: stored under the ANSI form cut at 31.
    (
        "\u{13E3}\u{13B3}\u{13A9}_CR_LONG_NAME_ABCDEFGHIJKLMNOPQRS",
        "???_CR_LONG_NAME_ABCDEFGHIJKLMN",
        "\u{13E3}\u{13B3}\u{13A9}_CR_LONG_NAME_ABCDEFGHIJKLMNOPQRS",
    ),
    // 33 units ending in 𠮷野: the surrogate pair is `??` in the ANSI form, so
    // the cut at 31 never splits it.
    (
        "SURROGATE_AT_THE_CAP_012345678\u{20BB7}\u{91CE}",
        "SURROGATE_AT_THE_CAP_012345678?",
        "SURROGATE_AT_THE_CAP_012345678\u{20BB7}\u{91CE}",
    ),
];

/// Reads one stream of a compound document as raw bytes.
fn raw_stream(path: &std::path::Path, stream: &str) -> Vec<u8> {
    use std::io::Read;
    let mut doc = cfb::open(path).expect("open compound document");
    let mut bytes = Vec::new();
    doc.open_stream(stream)
        .unwrap_or_else(|e| panic!("stream {stream}: {e}"))
        .read_to_end(&mut bytes)
        .expect("read stream");
    bytes
}

/// Renders bytes one character per byte, so a mismatch reads as text.
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// Unframes a `[u32 len][text NUL]` parameter block, asserting the framing.
fn param_block(raw: &[u8], what: &str) -> Vec<u8> {
    let len = usize::try_from(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
        .expect("block length fits");
    assert_eq!(raw.len(), 4 + len, "{what}: one block");
    assert_eq!(raw[raw.len() - 1], 0, "{what}: NUL-terminated");
    raw[4..raw.len() - 1].to_vec()
}

/// Reads the `WriteStringBlock` (`[u32 len][u8 str_len][bytes]`) at `pos` and
/// advances past it.
fn string_block(bytes: &[u8], pos: &mut usize) -> Vec<u8> {
    let at = *pos;
    let len = usize::try_from(u32::from_le_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
    ]))
    .expect("block length fits");
    let str_len = usize::from(bytes[at + 4]);
    assert_eq!(len, str_len + 1, "a string block is str_len + 1 long");
    *pos = at + 4 + len;
    bytes[at + 5..at + 5 + str_len].to_vec()
}

/// The reader's view of the fixture: names in `Library/Data` order, the
/// storage each footprint was read from, its description and its one pad.
#[test]
fn samples_manual_i18n4_names_storages_and_descriptions_read_exactly() {
    let lib = PcbLib::open(sample("manual/i18n4.PcbLib")).expect("open manual/i18n4.PcbLib");
    let names: Vec<String> = I18N4_FOOTPRINTS
        .iter()
        .map(|(name, _, _)| (*name).to_string())
        .collect();
    assert_eq!(lib.names(), names, "four footprints, in Library/Data order");
    for (name, storage, description) in I18N4_FOOTPRINTS {
        let fp = lib
            .get(name)
            .unwrap_or_else(|| panic!("footprint {name:?}"));
        assert_eq!(
            fp.storage_name.as_deref(),
            Some(storage),
            "{name}: storage name"
        );
        assert_eq!(fp.description, description, "{name}: description");
        assert_eq!(fp.pads.len(), 1, "{name}: one pad");
    }
}

/// Every Parameters block of the fixture, byte for byte and in Altium's key
/// order: `UNICODE=EXISTS` opens and closes it, the plain keys hold the ANSI
/// form (`?` for a unit the code page cannot hold, `??` for a surrogate pair)
/// and the `UNICODE__` twins hold the UTF-16 code units in decimal. A `PcbLib`
/// has no `%UTF8%` twin.
#[test]
fn samples_manual_i18n4_parameters_carry_utf16_twins() {
    let path = sample("manual/i18n4.PcbLib");
    let cherokee = "5091,5043,5033,95,67,82,95,48,52,48,50";
    let slovene = "268,272,381,95,83,76,95,48,52,48,50";
    let long = "5091,5043,5033,95,67,82,95,76,79,78,71,95,78,65,77,69,95,\
                65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83";
    let surrogate = "83,85,82,82,79,71,65,84,69,95,65,84,95,84,72,69,95,67,65,80,95,\
                     48,49,50,51,52,53,54,55,56,55362,57271,37326";
    let area = "|AREA=462399999999.999936";
    let twins = |units: &str| {
        format!("|UNICODE__DESCRIPTION={units}|UNICODE__PATTERN={units}|UNICODE=EXISTS")
    };

    let mut slovene_block =
        b"|UNICODE=EXISTS|PATTERN=\xC8\xD0\x8E_SL_0402|HEIGHT=0mil|DESCRIPTION=\xC8\xD0\x8E_SL_0402|ITEMGUID=|REVISIONGUID="
            .to_vec();
    slovene_block.extend_from_slice(area.as_bytes());
    slovene_block.extend_from_slice(twins(slovene).as_bytes());

    let expected = [
        (
            "/\u{13E3}\u{13B3}\u{13A9}_CR_0402/Parameters",
            format!(
                "|UNICODE=EXISTS|PATTERN=???_CR_0402|HEIGHT=0mil|DESCRIPTION=???_CR_0402|ITEMGUID=|REVISIONGUID={}",
                twins(cherokee)
            )
            .into_bytes(),
        ),
        ("/\u{10C}\u{110}\u{17D}_SL_0402/Parameters", slovene_block),
        (
            "/???_CR_LONG_NAME_ABCDEFGHIJKLMN/Parameters",
            format!(
                "|UNICODE=EXISTS|PATTERN=???_CR_LONG_NAME_ABCDEFGHIJKLMNOPQRS|HEIGHT=0mil\
                 |DESCRIPTION=???_CR_LONG_NAME_ABCDEFGHIJKLMNOPQRS|ITEMGUID=|REVISIONGUID={area}{}",
                twins(long)
            )
            .into_bytes(),
        ),
        (
            "/SURROGATE_AT_THE_CAP_012345678?/Parameters",
            format!(
                "|UNICODE=EXISTS|PATTERN=SURROGATE_AT_THE_CAP_012345678???|HEIGHT=0mil\
                 |DESCRIPTION=SURROGATE_AT_THE_CAP_012345678???|ITEMGUID=|REVISIONGUID={area}{}",
                twins(surrogate)
            )
            .into_bytes(),
        ),
    ];
    for (stream, want) in &expected {
        let got = param_block(&raw_stream(&path, stream), stream);
        assert_eq!(latin1(&got), latin1(want), "{stream}");
        assert!(
            !got.windows(6).any(|w| w == b"%UTF8%"),
            "{stream}: a PcbLib carries no %UTF8% twin"
        );
    }
}

/// `Library/Data`, each `Data` stream's leading name block and `SectionKeys`
/// all hold the ANSI forms; only a short name's storage keeps its real
/// characters.
#[test]
fn samples_manual_i18n4_lists_and_section_keys_hold_ansi_forms() {
    let path = sample("manual/i18n4.PcbLib");
    let ansi = [
        b"???_CR_0402".to_vec(),
        b"\xC8\xD0\x8E_SL_0402".to_vec(),
        b"???_CR_LONG_NAME_ABCDEFGHIJKLMNOPQRS".to_vec(),
        b"SURROGATE_AT_THE_CAP_012345678???".to_vec(),
    ];

    // Library/Data: the parameter block, then the count and the full names.
    let library = raw_stream(&path, "/Library/Data");
    let block_len = usize::try_from(u32::from_le_bytes([
        library[0], library[1], library[2], library[3],
    ]))
    .expect("block length fits");
    let mut pos = 4 + block_len;
    let count = u32::from_le_bytes([
        library[pos],
        library[pos + 1],
        library[pos + 2],
        library[pos + 3],
    ]);
    assert_eq!(count, 4, "component count");
    pos += 4;
    let listed: Vec<Vec<u8>> = (0..4).map(|_| string_block(&library, &mut pos)).collect();
    assert_eq!(listed, ansi, "Library/Data names");

    // Each Data stream opens with the same ANSI form.
    for (storage, want) in [
        ("\u{13E3}\u{13B3}\u{13A9}_CR_0402", &ansi[0]),
        ("\u{10C}\u{110}\u{17D}_SL_0402", &ansi[1]),
        ("???_CR_LONG_NAME_ABCDEFGHIJKLMN", &ansi[2]),
        ("SURROGATE_AT_THE_CAP_012345678?", &ansi[3]),
    ] {
        let data = raw_stream(&path, &format!("/{storage}/Data"));
        let mut at = 0;
        assert_eq!(
            &string_block(&data, &mut at),
            want,
            "{storage}/Data name block"
        );
    }

    // SectionKeys: the two over-cap names, ANSI form to ANSI form cut at 31.
    let keys = raw_stream(&path, "/SectionKeys");
    assert_eq!(
        u32::from_le_bytes([keys[0], keys[1], keys[2], keys[3]]),
        2,
        "two pairs"
    );
    let mut pos = 4;
    let pairs: Vec<(Vec<u8>, Vec<u8>)> = (0..2)
        .map(|_| (string_block(&keys, &mut pos), string_block(&keys, &mut pos)))
        .collect();
    assert_eq!(pos, keys.len(), "nothing follows the pairs");
    assert_eq!(
        pairs,
        vec![
            (ansi[2].clone(), b"???_CR_LONG_NAME_ABCDEFGHIJKLMN".to_vec()),
            (ansi[3].clone(), b"SURROGATE_AT_THE_CAP_012345678?".to_vec()),
        ]
    );
}

/// Names, storage names, descriptions and pads of the fixture survive our own
/// write -> read; the carried storage names in particular (#507).
#[test]
fn samples_manual_i18n4_survives_a_write_read_cycle() {
    use std::io::Cursor;

    let mut lib = PcbLib::open(sample("manual/i18n4.PcbLib")).expect("open manual/i18n4.PcbLib");
    let mut buffer = Cursor::new(Vec::new());
    lib.write(&mut buffer).expect("write");
    buffer.set_position(0);
    let reread = PcbLib::read(&mut buffer).expect("read back");

    assert_eq!(reread.names(), lib.names(), "names and their order");
    for (name, storage, description) in I18N4_FOOTPRINTS {
        let fp = reread
            .get(name)
            .unwrap_or_else(|| panic!("footprint {name:?} lost"));
        assert_eq!(
            fp.storage_name.as_deref(),
            Some(storage),
            "{name}: storage kept"
        );
        assert_eq!(fp.description, description, "{name}: description");
        assert_eq!(fp.pads.len(), 1, "{name}: pad");
    }
}

/// `manual/region_hole.PcbLib` (AD24, scripted 2026-09-21 by
/// `scripts/altium/probe/RegionHoleProbe.pas`): a copper region with a
/// 200 mil square outline and a 80 mil square hole, the hole added through
/// `GeometricPolygon.AddContourIsHole`. The reader returns the outline as the
/// region's vertices and the hole as its one `holes` contour, both exact; the
/// byte-identical rewrite is `manual_pcblibs_survive_a_round_trip`.
#[test]
fn samples_manual_region_hole_reads_exactly() {
    let lib =
        PcbLib::open(sample("manual/region_hole.PcbLib")).expect("open manual/region_hole.PcbLib");
    let fp = lib.get("REGION_HOLE").expect("footprint REGION_HOLE");
    assert_eq!(fp.regions.len(), 1, "one region");
    let region = &fp.regions[0];
    assert_eq!(region.layer, Layer::TopLayer);

    // The four corners of a square of the given half-width, in any order.
    let corners = |points: &[(f64, f64)], half: f64| {
        let want = [(-half, half), (-half, -half), (half, -half), (half, half)];
        points.len() == want.len()
            && want.iter().all(|w| {
                points
                    .iter()
                    .any(|p| approx_eq(p.0, w.0, 1e-6) && approx_eq(p.1, w.1, 1e-6))
            })
    };
    let outline: Vec<(f64, f64)> = region.vertices.iter().map(|v| (v.x, v.y)).collect();
    assert!(
        corners(&outline, 2.54),
        "100 mil half-width outline: {outline:?}"
    );

    assert_eq!(region.holes.len(), 1, "one hole contour");
    let hole: Vec<(f64, f64)> = region.holes[0].iter().map(|v| (v.x, v.y)).collect();
    assert!(corners(&hole, 1.016), "40 mil half-width hole: {hole:?}");
}

/// `manual/wide.PcbLib` (AD24, scripted 2026-09-21 by
/// `scripts/altium/probe/WideProbe.pas`): text and body identifiers beyond
/// U+00FF, built from character literals (`#937`, `#55362#57271`), which the
/// script engine keeps as UTF-16 where a string literal would not. The texts
/// read from `WideStrings` and the identifiers from their UTF-16 code units;
/// the byte-identical rewrite is `manual_pcblibs_survive_a_round_trip`.
#[test]
fn samples_manual_wide_text_and_identifiers_read_exactly() {
    let lib = PcbLib::open(sample("manual/wide.PcbLib")).expect("open manual/wide.PcbLib");
    let fp = lib.get("WIDE").expect("footprint WIDE");
    let bmp = "\u{B5}\u{3A9}\u{7535}";
    let astral = "\u{20BB7}";

    let mut texts: Vec<&str> = fp.text.iter().map(|t| t.text.as_str()).collect();
    texts.sort_unstable();
    let mut want = vec![bmp, astral];
    want.sort_unstable();
    assert_eq!(texts, want, "both texts, beyond U+00FF and beyond the BMP");

    let mut idents: Vec<&str> = fp
        .component_bodies
        .iter()
        .map(|b| b.identifier.as_str())
        .collect();
    idents.sort_unstable();
    assert_eq!(idents, want, "both body identifiers");
}

/// `manual/thermal_relief.PcbLib` (AD24 UI, 2026-09-21): six through-hole
/// pads, each with a different Pad Stack → Thermal Relief setting. Pad 4
/// leaves the box unticked; the others tick it and change one or two
/// settings of the "Edit Polygon Connect Style" dialog each, so every field
/// of the override is pinned to its byte.
#[test]
fn samples_manual_pad_polygon_connect() {
    let lib = PcbLib::open(sample("manual/thermal_relief.PcbLib"))
        .expect("failed to open manual/thermal_relief.PcbLib");
    let fp = lib.iter().next().expect("one footprint");
    let pad = |designator: &str| {
        fp.pads
            .iter()
            .find(|p| p.designator == designator)
            .unwrap_or_else(|| panic!("pad {designator}"))
    };
    let mil = |m: f64| m * 0.0254;
    let expect = |designator: &str, want: PadPolygonConnect| {
        let got = pad(designator)
            .polygon_connect
            .unwrap_or_else(|| panic!("pad {designator} has an override"));
        assert_eq!(got.style, want.style, "pad {designator} style");
        assert!(
            approx_eq(got.air_gap, want.air_gap, 1e-6),
            "pad {designator} air gap"
        );
        assert!(
            approx_eq(got.conductor_width, want.conductor_width, 1e-6),
            "pad {designator} conductor width"
        );
        assert_eq!(
            got.conductors, want.conductors,
            "pad {designator} conductors"
        );
        assert_eq!(
            got.auto_conductors, want.auto_conductors,
            "pad {designator} auto"
        );
        assert_eq!(got.rotation, want.rotation, "pad {designator} rotation");
        assert!(
            approx_eq(got.min_distance, want.min_distance, 1e-6),
            "pad {designator} min distance"
        );
    };

    assert_eq!(
        pad("4").polygon_connect,
        None,
        "an unticked pad follows the rules"
    );
    expect(
        "3",
        PadPolygonConnect {
            air_gap: mil(9.0),
            conductor_width: mil(11.0),
            ..PadPolygonConnect::default()
        },
    );
    expect(
        "5",
        PadPolygonConnect {
            air_gap: mil(6.0),
            conductor_width: mil(14.0),
            conductors: 2,
            rotation: 45,
            ..PadPolygonConnect::default()
        },
    );
    expect(
        "6",
        PadPolygonConnect {
            style: PowerPlaneConnectStyle::Direct,
            ..PadPolygonConnect::default()
        },
    );
    expect(
        "7",
        PadPolygonConnect {
            auto_conductors: true,
            ..PadPolygonConnect::default()
        },
    );
    expect(
        "8",
        PadPolygonConnect {
            style: PowerPlaneConnectStyle::NoConnect,
            ..PadPolygonConnect::default()
        },
    );
}

/// Setting, changing and clearing a pad's override writes what AD24 writes:
/// an override added to the unticked pad matches the bytes of an Altium pad
/// ticked with the same settings, and clearing it gives back the unticked
/// pad's 194-byte block.
#[test]
fn samples_manual_pad_polygon_connect_edits() {
    use std::io::Cursor;

    let mut lib = PcbLib::open(sample("manual/thermal_relief.PcbLib"))
        .expect("failed to open manual/thermal_relief.PcbLib");
    let name = lib.names()[0].clone();
    let reread = |lib: &mut PcbLib| {
        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("write");
        buffer.set_position(0);
        PcbLib::read(&mut buffer).expect("read back")
    };

    let wanted = PadPolygonConnect {
        style: PowerPlaneConnectStyle::NoConnect,
        ..PadPolygonConnect::default()
    };
    {
        let fp = lib.get_mut(&name).expect("footprint");
        for pad in &mut fp.pads {
            match pad.designator.as_str() {
                "4" => pad.polygon_connect = Some(wanted),
                "8" => pad.polygon_connect = None,
                _ => {}
            }
        }
    }
    let back = reread(&mut lib);
    let fp = back.get(&name).expect("footprint");
    let pad = |d: &str| fp.pads.iter().find(|p| p.designator == d).expect("pad");
    assert_eq!(pad("4").polygon_connect, Some(wanted));
    assert_eq!(pad("8").polygon_connect, None);
    let tail = |d: &str| pad(d).raw_tail.clone().expect("raw tail");
    assert_eq!(
        tail("8").len(),
        194 - 61,
        "a cleared override leaves AD24's plain block"
    );
    // Pad 4 and Altium's own pad 8 now carry the same override bytes.
    assert_eq!(tail("4")[133..], tail_of_original("8")[133..]);
}

/// Pad `designator`'s extended tail as Altium wrote it.
fn tail_of_original(designator: &str) -> Vec<u8> {
    let lib = PcbLib::open(sample("manual/thermal_relief.PcbLib")).expect("open");
    let tail = lib
        .iter()
        .next()
        .expect("one footprint")
        .pads
        .iter()
        .find(|p| p.designator == designator)
        .and_then(|p| p.raw_tail.clone())
        .expect("raw tail");
    tail
}
