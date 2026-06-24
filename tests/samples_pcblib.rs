//! Sample-library tests for `PcbLib`.
//!
//! Like `samples_schlib.rs`, these tests open a *real*, Altium-authored sample
//! library from `scripts/samples/` with our reader and assert the parsed values
//! against the file's authored intent (rather than round-tripping our own
//! writer's output, as `file_io_roundtrip.rs` does).

use altium_designer_mcp::altium::pcblib::{Layer, PadShape, PcbLib};
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
    let path = sample("pads.PcbLib");
    assert!(
        path.exists(),
        "missing sample fixture: {} — the samples_pcblib tests read a real \
         Altium-authored library that must be present on disk",
        path.display()
    );
}

#[test]
fn samples_pcblib_pad_shapes() {
    let lib = PcbLib::open(sample("pads.PcbLib")).expect("failed to open pads.PcbLib");

    // The library contains exactly one footprint, PAD_SHAPES.
    assert_eq!(lib.len(), 1, "expected exactly one footprint");
    assert_eq!(
        lib.names(),
        vec!["PAD_SHAPES"],
        "unexpected footprint name(s)"
    );

    let footprint = lib
        .get("PAD_SHAPES")
        .expect("footprint PAD_SHAPES not found");
    assert_eq!(footprint.name, "PAD_SHAPES");
    assert_eq!(footprint.pads.len(), 4, "PAD_SHAPES has 4 pads");

    // Authored pads: four 60x40 mil (1.524 x 1.016 mm) SMD pads on the Top Layer,
    // one per shape. We look pads up by designator because the on-disk order is
    // not guaranteed to be 1..4. The shape column is the variant our reader
    // reports, which differs from the authored Altium shape for pad "3": Altium's
    // octagon has no `PadShape` variant, so the reader maps it to `Oval` (the
    // closest match; see `pad_shape_from_id` in src/altium/pcblib/reader/mod.rs).
    let expected: [(&str, PadShape); 4] = [
        ("1", PadShape::Round),            // authored Rounded
        ("2", PadShape::Rectangle),        // authored Rectangular
        ("3", PadShape::Oval), // authored Octagonal — reads as Oval (no Octagonal variant)
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
