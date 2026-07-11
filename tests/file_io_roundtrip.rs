//! File I/O roundtrip tests for `PcbLib` and `SchLib`.
//!
//! These tests verify that libraries can be written to files and read back
//! with all data preserved through the full OLE compound document format.

use altium_designer_mcp::altium::pcblib::{
    Arc, Fill, Footprint, Layer, Pad, PadShape, PadStackMode, PcbFlags, PcbLib, Region, Text,
    TextJustification, TextKind, Track, Via,
};
use altium_designer_mcp::altium::schlib::{
    Pin, PinElectricalType, PinOrientation, PinSymbol, Rectangle, SchLib, Symbol,
};
use std::fs::File;
use tempfile::TempDir;

/// Creates a temporary directory inside `.tmp/` for test isolation.
/// The directory is automatically cleaned up when the returned `TempDir` is dropped.
///
/// Uses an absolute path to avoid issues with parallel test execution.
fn test_temp_dir() -> TempDir {
    // Use CARGO_MANIFEST_DIR instead of current_dir() to avoid issues with
    // parallel test execution or tests that change the working directory.
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tmp_root = manifest_dir.join(".tmp");
    std::fs::create_dir_all(&tmp_root).expect("Failed to create .tmp directory");
    tempfile::tempdir_in(&tmp_root).expect("Failed to create temp dir")
}

/// Helper to compare floats with tolerance.
fn approx_eq(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() < tolerance
}

// Geometry used in the simple footprint roundtrip test.
const SIMPLE_PAD_1_X: f64 = -0.5;
const SIMPLE_PAD_2_X: f64 = 0.5;
const SIMPLE_PAD_Y: f64 = 0.0;
const SIMPLE_PAD_WIDTH: f64 = 0.6;
const SIMPLE_PAD_HEIGHT: f64 = 0.5;
const COORD_TOLERANCE: f64 = 0.001;

// =============================================================================
// PcbLib File I/O Roundtrip Tests
// =============================================================================

#[test]
fn pcblib_file_roundtrip_simple_footprint() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_simple.PcbLib");

    // Create a simple footprint
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("CHIP_0402");
    fp.description = "Test 0402 chip component".to_string();
    fp.add_pad(Pad::smd(
        "1",
        SIMPLE_PAD_1_X,
        SIMPLE_PAD_Y,
        SIMPLE_PAD_WIDTH,
        SIMPLE_PAD_HEIGHT,
    ));
    fp.add_pad(Pad::smd(
        "2",
        SIMPLE_PAD_2_X,
        SIMPLE_PAD_Y,
        SIMPLE_PAD_WIDTH,
        SIMPLE_PAD_HEIGHT,
    ));
    lib.add(fp);

    // Write to file
    lib.save(&file_path).expect("Failed to write PcbLib");

    // Read back
    let read_lib = PcbLib::open(&file_path).expect("Failed to read PcbLib");

    // Verify
    assert_eq!(read_lib.len(), 1);
    let read_fp = read_lib.get("CHIP_0402").expect("Footprint not found");
    assert_eq!(read_fp.description, "Test 0402 chip component");
    assert_eq!(read_fp.pads.len(), 2);
    assert_eq!(read_fp.pads[0].designator, "1");
    assert_eq!(read_fp.pads[1].designator, "2");
    assert!(approx_eq(
        read_fp.pads[0].x,
        SIMPLE_PAD_1_X,
        COORD_TOLERANCE
    ));
    assert!(approx_eq(
        read_fp.pads[1].x,
        SIMPLE_PAD_2_X,
        COORD_TOLERANCE
    ));
}

#[test]
fn pcblib_file_roundtrip_multiple_footprints() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_multi.PcbLib");

    // Create multiple footprints
    let mut lib = PcbLib::new();

    for size in ["0402", "0603", "0805", "1206"] {
        let mut fp = Footprint::new(format!("CHIP_{size}"));
        fp.description = format!("Chip resistor {size}");
        fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
        lib.add(fp);
    }

    // Write and read back
    lib.save(&file_path).expect("Failed to write PcbLib");
    let read_lib = PcbLib::open(&file_path).expect("Failed to read PcbLib");

    // Verify all footprints
    assert_eq!(read_lib.len(), 4);
    assert!(read_lib.get("CHIP_0402").is_some());
    assert!(read_lib.get("CHIP_0603").is_some());
    assert!(read_lib.get("CHIP_0805").is_some());
    assert!(read_lib.get("CHIP_1206").is_some());
}

#[test]
fn pcblib_file_roundtrip_all_primitives() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_all_primitives.PcbLib");

    // Create footprint with all primitive types
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("ALL_PRIMITIVES");

    // Pads
    fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.8, 0.6));
    fp.add_pad(Pad::through_hole("2", 1.0, 0.0, 1.5, 1.5, 0.8));

    // Tracks
    fp.add_track(Track::new(-2.0, -1.0, 2.0, -1.0, 0.15, Layer::TopOverlay));
    fp.add_track(Track::new(-2.0, 1.0, 2.0, 1.0, 0.15, Layer::TopOverlay));

    // Arcs
    fp.add_arc(Arc::circle(0.0, 2.0, 0.5, 0.1, Layer::TopOverlay));

    // Regions
    fp.add_region(Region::rectangle(-2.5, -1.5, 2.5, 1.5, Layer::TopCourtyard));

    // Text
    let text = Text {
        x: 0.0,
        y: -2.0,
        text: ".Designator".to_string(),
        height: 1.0,
        layer: Layer::TopOverlay,
        kind: TextKind::Stroke,
        rotation: 0.0,
        stroke_font: None,
        stroke_width: None,
        italic: false,
        bold: false,
        mirror: false,
        is_comment: false,
        is_designator: false,
        font_name: "Arial".to_string(),
        justification: TextJustification::default(),
        is_inverted: false,
        inverted_border: None,
        use_inverted_rectangle: false,
        inverted_rect_width: None,
        inverted_rect_height: None,
        inverted_rect_text_offset: None,
        flags: PcbFlags::default(),
        net_index: 0xFFFF,
        polygon_index: 0xFFFF,
        component_index: -1,
        unique_id: None,
    };
    fp.add_text(text);

    // Fill
    fp.add_fill(Fill::new(-0.5, -0.5, 0.5, 0.5, Layer::TopOverlay));

    // Via
    fp.add_via(Via::new(0.0, 0.0, 0.6, 0.3));

    lib.add(fp);

    // Write and read back
    lib.save(&file_path).expect("Failed to write PcbLib");
    let read_lib = PcbLib::open(&file_path).expect("Failed to read PcbLib");

    // Verify all primitives
    let read_fp = read_lib.get("ALL_PRIMITIVES").expect("Footprint not found");
    assert_eq!(read_fp.pads.len(), 2);
    assert_eq!(read_fp.tracks.len(), 2);
    assert_eq!(read_fp.arcs.len(), 1);
    assert_eq!(read_fp.regions.len(), 1);
    assert_eq!(read_fp.text.len(), 1);
    assert_eq!(read_fp.fills.len(), 1);
    assert_eq!(read_fp.vias.len(), 1);
}

#[test]
fn pcblib_file_roundtrip_pad_shapes() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_pad_shapes.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("PAD_SHAPES");

    // Different pad shapes
    let mut pad1 = Pad::smd("1", -3.0, 0.0, 1.0, 1.0);
    pad1.shape = PadShape::Rectangle;
    fp.add_pad(pad1);

    let mut pad2 = Pad::smd("2", -1.0, 0.0, 1.0, 1.0);
    pad2.shape = PadShape::Round;
    fp.add_pad(pad2);

    // Octagonal is a first-class Altium shape (id 3) and round-trips faithfully.
    // (Oval is intentionally not tested here: Altium has no oval shape, so an oval
    // pad serialises as Round (id 1) and reads back as Round — see pad 2 above.)
    let mut pad3 = Pad::smd("3", 1.0, 0.0, 1.0, 1.0);
    pad3.shape = PadShape::Octagonal;
    fp.add_pad(pad3);

    // RoundedRectangle with corner radius requires FullStack mode for shape to be preserved
    let mut pad4 = Pad::smd("4", 3.0, 0.0, 1.0, 1.0);
    pad4.shape = PadShape::RoundedRectangle;
    pad4.corner_radius_percent = Some(25);
    pad4.stack_mode = PadStackMode::FullStack;
    fp.add_pad(pad4);

    lib.add(fp);

    // Write and read back
    lib.save(&file_path).expect("Failed to write PcbLib");
    let read_lib = PcbLib::open(&file_path).expect("Failed to read PcbLib");

    let read_fp = read_lib.get("PAD_SHAPES").expect("Footprint not found");
    assert_eq!(read_fp.pads.len(), 4);
    assert_eq!(read_fp.pads[0].shape, PadShape::Rectangle);
    assert_eq!(read_fp.pads[1].shape, PadShape::Round);
    assert_eq!(read_fp.pads[2].shape, PadShape::Octagonal);
    assert_eq!(read_fp.pads[3].shape, PadShape::RoundedRectangle);
}

#[test]
fn pcblib_file_roundtrip_pad_top_middle_bottom() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_pad_tmb.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("PAD_TMB");

    // A TopMiddleBottom (LocalStack) through-hole pad with distinct top/mid/bottom
    // sizes and shapes. The mid/bottom values live in the main geometry block, so
    // they must survive a write -> read cycle (Block 5 stays empty here).
    let mut pad = Pad::through_hole("1", 0.0, 0.0, 1.778, 1.778, 0.762);
    pad.layer = Layer::MultiLayer;
    pad.shape = PadShape::Round;
    pad.stack_mode = PadStackMode::TopMiddleBottom;
    pad.per_layer_sizes = Some(vec![(1.778, 1.778), (1.524, 1.524), (1.27, 1.27)]);
    pad.per_layer_shapes = Some(vec![PadShape::Round, PadShape::Round, PadShape::Rectangle]);
    fp.add_pad(pad);

    lib.add(fp);

    lib.save(&file_path).expect("Failed to write PcbLib");
    let read_lib = PcbLib::open(&file_path).expect("Failed to read PcbLib");

    let read_fp = read_lib.get("PAD_TMB").expect("Footprint not found");
    assert_eq!(read_fp.pads.len(), 1);
    let read_pad = &read_fp.pads[0];

    assert_eq!(read_pad.stack_mode, PadStackMode::TopMiddleBottom);
    assert_eq!(read_pad.layer, Layer::MultiLayer);

    // Top size/shape survive (already exercised elsewhere, asserted here for completeness).
    assert!(approx_eq(read_pad.width, 1.778, 1e-2));
    assert!(approx_eq(read_pad.height, 1.778, 1e-2));
    assert_eq!(read_pad.shape, PadShape::Round);

    // Mid/bottom per-layer sizes round-trip.
    let sizes = read_pad
        .per_layer_sizes
        .as_ref()
        .expect("per_layer_sizes preserved for TopMiddleBottom");
    assert_eq!(sizes.len(), 3, "per_layer_sizes is [top, mid, bottom]");
    let expected_sizes = [(1.778, 1.778), (1.524, 1.524), (1.27, 1.27)];
    for (i, &(ew, eh)) in expected_sizes.iter().enumerate() {
        assert!(
            approx_eq(sizes[i].0, ew, 1e-2) && approx_eq(sizes[i].1, eh, 1e-2),
            "per-layer size {i}: expected ~({ew},{eh}), got {:?}",
            sizes[i],
        );
    }

    // Mid/bottom per-layer shapes round-trip.
    assert_eq!(
        read_pad.per_layer_shapes.as_deref(),
        Some([PadShape::Round, PadShape::Round, PadShape::Rectangle].as_slice()),
        "per_layer_shapes is [top, mid, bottom]",
    );
}

#[test]
fn pcblib_file_roundtrip_layers() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_layers.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("LAYER_TEST");

    // Pads on different layers
    let mut pad_top = Pad::smd("T1", -1.0, 0.0, 1.0, 1.0);
    pad_top.layer = Layer::TopLayer;
    fp.add_pad(pad_top);

    let mut pad_bottom = Pad::smd("B1", 0.0, 0.0, 1.0, 1.0);
    pad_bottom.layer = Layer::BottomLayer;
    fp.add_pad(pad_bottom);

    let mut pad_multi = Pad::through_hole("M1", 1.0, 0.0, 1.5, 1.5, 0.8);
    pad_multi.layer = Layer::MultiLayer;
    fp.add_pad(pad_multi);

    // Tracks on different layers
    fp.add_track(Track::new(-2.0, 1.0, 2.0, 1.0, 0.15, Layer::TopOverlay));
    fp.add_track(Track::new(-2.0, -1.0, 2.0, -1.0, 0.15, Layer::TopAssembly));

    lib.add(fp);

    // Write and read back
    lib.save(&file_path).expect("Failed to write PcbLib");
    let read_lib = PcbLib::open(&file_path).expect("Failed to read PcbLib");

    let read_fp = read_lib.get("LAYER_TEST").expect("Footprint not found");
    assert_eq!(read_fp.pads[0].layer, Layer::TopLayer);
    assert_eq!(read_fp.pads[1].layer, Layer::BottomLayer);
    assert_eq!(read_fp.pads[2].layer, Layer::MultiLayer);
    assert_eq!(read_fp.tracks[0].layer, Layer::TopOverlay);
    assert_eq!(read_fp.tracks[1].layer, Layer::TopAssembly);
}

// =============================================================================
// SchLib File I/O Roundtrip Tests
// =============================================================================

#[test]
fn schlib_file_roundtrip_simple_symbol() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_simple.SchLib");

    // Create a simple resistor symbol
    let mut lib = SchLib::new();
    let mut sym = Symbol::new("RESISTOR");
    sym.description = "Test resistor".to_string();
    sym.designator = "R?".to_string();

    // Add pins
    sym.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
    sym.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));

    // Add body rectangle
    sym.add_rectangle(Rectangle::new(-10, -5, 10, 5));

    lib.add(sym);

    // Write to file using File I/O
    let file = File::create(&file_path).expect("Failed to create file");
    lib.write(file).expect("Failed to write SchLib");

    // Read back
    let read_lib = SchLib::open(&file_path).expect("Failed to read SchLib");

    // Verify
    assert_eq!(read_lib.len(), 1);
    let read_sym = read_lib.get("RESISTOR").expect("Symbol not found");
    assert_eq!(read_sym.description, "Test resistor");
    assert_eq!(read_sym.designator, "R?");
    assert_eq!(read_sym.pins.len(), 2);
    assert_eq!(read_sym.rectangles.len(), 1);
}

// =============================================================================
// Issue #68: on-disk strings must be Windows-1252, not UTF-8
// =============================================================================

/// Returns true if `haystack` contains `needle` as a contiguous byte run.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

/// Altium stores strings as Windows-1252. A footprint description with non-ASCII
/// characters must round-trip AND be written as single-byte Windows-1252 (e.g.
/// `µ` = `0xB5`), never as the two-byte UTF-8 sequence (`0xC2 0xB5`) that Altium
/// misreads. ASCII is identical in both encodings, so only non-ASCII exposes the
/// bug — which is why it survived an all-ASCII test suite.
#[test]
fn pcblib_non_ascii_description_is_windows1252() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_win1252.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("CAP_0402");
    fp.description = "cap 10µF ±5% é°".to_string();
    fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
    lib.add(fp);
    lib.save(&file_path).expect("Failed to write PcbLib");

    // Round-trip preserves the exact string.
    let read_lib = PcbLib::open(&file_path).expect("Failed to read PcbLib");
    let read_fp = read_lib.get("CAP_0402").expect("Footprint not found");
    assert_eq!(read_fp.description, "cap 10µF ±5% é°");

    // The on-disk bytes are Windows-1252, not UTF-8.
    let raw = std::fs::read(&file_path).expect("read raw file");
    assert!(
        contains_bytes(&raw, b"10\xb5F"),
        "description should be Windows-1252 (0xB5 for µ)"
    );
    assert!(
        !contains_bytes(&raw, b"10\xc2\xb5F"),
        "description must NOT be UTF-8 (0xC2 0xB5 for µ) — Altium would misread it"
    );
}

/// The `SchLib` `FileHeader` (component description) must likewise be Windows-1252.
#[test]
fn schlib_non_ascii_description_is_windows1252() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_win1252.SchLib");

    let mut lib = SchLib::new();
    let mut sym = Symbol::new("RES");
    sym.description = "résistance 10µF".to_string();
    sym.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
    lib.add(sym);
    lib.save(&file_path).expect("Failed to write SchLib");

    let read_lib = SchLib::open(&file_path).expect("Failed to read SchLib");
    let read_sym = read_lib.get("RES").expect("Symbol not found");
    assert_eq!(read_sym.description, "résistance 10µF");

    let raw = std::fs::read(&file_path).expect("read raw file");
    assert!(
        contains_bytes(&raw, b"10\xb5F"),
        "description should be Windows-1252 (0xB5 for µ)"
    );
    assert!(
        !contains_bytes(&raw, b"10\xc2\xb5F"),
        "description must NOT be UTF-8 (0xC2 0xB5 for µ)"
    );
}

#[test]
fn schlib_preserves_unique_id_and_pin_accessibility() {
    // #113: reading an Altium symbol and writing it back must preserve each
    // shape's UniqueID (object identity) and the pin IsNotAccessible flag,
    // rather than regenerating/dropping them.
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_fidelity.SchLib");

    let mut lib = SchLib::new();
    let mut sym = Symbol::new("RES");
    let mut pin = Pin::new("1", "1", -20, 0, 10, PinOrientation::Left);
    pin.is_not_accessible = true;
    sym.add_pin(pin);
    let mut rect = Rectangle::new(-20, -50, 20, 50);
    rect.unique_id = Some("ABCD1234".to_string());
    sym.add_rectangle(rect);
    lib.add(sym);
    lib.save(&file_path).expect("Failed to write SchLib");

    let read_lib = SchLib::open(&file_path).expect("Failed to read SchLib");
    let read_sym = read_lib.get("RES").expect("Symbol not found");
    assert_eq!(
        read_sym.rectangles[0].unique_id.as_deref(),
        Some("ABCD1234"),
        "rectangle UniqueID must survive read->write, not be regenerated"
    );
    assert!(
        read_sym.pins[0].is_not_accessible,
        "pin IsNotAccessible must survive read->write"
    );
}

/// Returns the set of OLE stream paths (lower-cased) in a written library file,
/// used to assert whether the optional pin auxiliary streams were emitted.
fn ole_stream_paths(path: &std::path::Path) -> Vec<String> {
    let file = File::open(path).expect("open written SchLib");
    let cfb = cfb::CompoundFile::open(file).expect("parse OLE");
    cfb.walk()
        .filter(cfb::Entry::is_stream)
        .map(|e| e.path().to_string_lossy().to_lowercase())
        .collect()
}

#[test]
fn schlib_pin_owner_part_display_mode_roundtrips() {
    // PR-R3 Part 1: the pin binary record's own OwnerPartDisplayMode byte (offset
    // 7) is preserved. A from-scratch pin defaults to 0 (byte-identical to
    // Altium); a non-default value survives write -> read.
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_pin_odm.SchLib");

    let mut lib = SchLib::new();
    let mut sym = Symbol::new("MODE");
    let mut pin = Pin::new("1", "1", -20, 0, 10, PinOrientation::Left);
    pin.owner_part_display_mode = 2;
    sym.add_pin(pin);
    sym.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right)); // default 0
    lib.add(sym);
    lib.save(&file_path).expect("write SchLib");

    let read_lib = SchLib::open(&file_path).expect("read SchLib");
    let read_sym = read_lib.get("MODE").expect("symbol not found");
    assert_eq!(
        read_sym.pins[0].owner_part_display_mode, 2,
        "non-default pin OwnerPartDisplayMode must survive round-trip"
    );
    assert_eq!(
        read_sym.pins[1].owner_part_display_mode, 0,
        "default pin OwnerPartDisplayMode stays 0"
    );
}

#[test]
fn schlib_pin_symbol_line_width_roundtrips_and_omits_when_default() {
    // PR-R3 Part 2: a non-default SymbolLineWidth survives a full library
    // write -> read via the per-component PinSymbolLineWidth stream, keyed by
    // pin ordinal. There is no golden for this stream, so this self round-trip
    // (we control both compress and decompress) is the verification.
    let temp_dir = test_temp_dir();

    // Non-default: pin[1] carries width 5.
    let path_nondefault = temp_dir.path().join("test_pin_slw.SchLib");
    let mut lib = SchLib::new();
    let mut sym = Symbol::new("SLW");
    sym.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left)); // default 0
    let mut pin = Pin::new("2", "2", 20, 0, 10, PinOrientation::Right);
    pin.symbol_line_width = 5;
    sym.add_pin(pin);
    lib.add(sym);
    lib.save(&path_nondefault).expect("write SchLib");

    let streams = ole_stream_paths(&path_nondefault);
    assert!(
        streams.iter().any(|s| s.ends_with("pinsymbollinewidth")),
        "a non-default symbol line width must emit a PinSymbolLineWidth stream; streams: {streams:?}"
    );

    let read_lib = SchLib::open(&path_nondefault).expect("read SchLib");
    let read_sym = read_lib.get("SLW").expect("symbol not found");
    assert_eq!(
        read_sym.pins[0].symbol_line_width, 0,
        "default pin[0] line width stays 0"
    );
    assert_eq!(
        read_sym.pins[1].symbol_line_width, 5,
        "non-default pin[1] line width survives round-trip, keyed by ordinal"
    );

    // Default: all pins width 0 -> no stream (byte-identity anchor).
    let path_default = temp_dir.path().join("test_pin_slw_default.SchLib");
    let mut lib = SchLib::new();
    let mut sym = Symbol::new("PLAIN");
    sym.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
    sym.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
    lib.add(sym);
    lib.save(&path_default).expect("write SchLib");
    let streams = ole_stream_paths(&path_default);
    assert!(
        !streams.iter().any(|s| s.ends_with("pinsymbollinewidth")),
        "all-default pins must write NO PinSymbolLineWidth stream; streams: {streams:?}"
    );
}

#[test]
fn schlib_pin_frac_roundtrips_and_omits_when_default() {
    // PR-R3 Part 3: a fractional (off-grid) pin survives a full library
    // write -> read via the per-component PinFrac stream, keyed by pin ordinal.
    // No golden exists for this stream, so this self round-trip is the check.
    use altium_designer_mcp::altium::schlib::PinFrac;
    let temp_dir = test_temp_dir();

    // Off-grid: pin[0] carries a fractional remainder.
    let path_frac = temp_dir.path().join("test_pin_frac.SchLib");
    let mut lib = SchLib::new();
    let mut sym = Symbol::new("FRAC");
    let mut pin = Pin::new("1", "1", -20, 0, 10, PinOrientation::Left);
    pin.frac = Some(PinFrac {
        x: 50_000,
        y: -25_000,
        length: 12_345,
    });
    sym.add_pin(pin);
    sym.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right)); // on-grid
    lib.add(sym);
    lib.save(&path_frac).expect("write SchLib");

    let streams = ole_stream_paths(&path_frac);
    assert!(
        streams.iter().any(|s| s.ends_with("pinfrac")),
        "a fractional pin must emit a PinFrac stream; streams: {streams:?}"
    );

    let read_lib = SchLib::open(&path_frac).expect("read SchLib");
    let read_sym = read_lib.get("FRAC").expect("symbol not found");
    assert_eq!(
        read_sym.pins[0].frac,
        Some(PinFrac {
            x: 50_000,
            y: -25_000,
            length: 12_345,
        }),
        "off-grid pin[0] fractional coords survive round-trip, keyed by ordinal"
    );
    assert_eq!(
        read_sym.pins[1].frac, None,
        "on-grid pin[1] carries no PinFrac remainder"
    );
    // The integer part of the coordinates is untouched by the PinFrac layer.
    assert_eq!(
        read_sym.pins[0].x, -20,
        "integer X preserved alongside frac"
    );

    // Default: all pins on-grid -> no stream (byte-identity anchor).
    let path_default = temp_dir.path().join("test_pin_frac_default.SchLib");
    let mut lib = SchLib::new();
    let mut sym = Symbol::new("GRID");
    sym.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
    lib.add(sym);
    lib.save(&path_default).expect("write SchLib");
    let streams = ole_stream_paths(&path_default);
    assert!(
        !streams.iter().any(|s| s.ends_with("pinfrac")),
        "all-on-grid pins must write NO PinFrac stream; streams: {streams:?}"
    );
}

#[test]
fn schlib_file_roundtrip_multiple_symbols() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_multi.SchLib");

    let mut lib = SchLib::new();

    // Create multiple symbols
    for (name, prefix) in [("RESISTOR", "R"), ("CAPACITOR", "C"), ("INDUCTOR", "L")] {
        let mut sym = Symbol::new(name);
        sym.designator = format!("{prefix}?");
        sym.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
        sym.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
        sym.add_rectangle(Rectangle::new(-10, -5, 10, 5));
        lib.add(sym);
    }

    // Write and read back
    let file = File::create(&file_path).expect("Failed to create file");
    lib.write(file).expect("Failed to write SchLib");
    let read_lib = SchLib::open(&file_path).expect("Failed to read SchLib");

    // Verify
    assert_eq!(read_lib.len(), 3);
    assert!(read_lib.get("RESISTOR").is_some());
    assert!(read_lib.get("CAPACITOR").is_some());
    assert!(read_lib.get("INDUCTOR").is_some());
}

#[test]
fn schlib_file_roundtrip_pin_types() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_pin_types.SchLib");

    let mut lib = SchLib::new();
    let mut sym = Symbol::new("IC");
    sym.designator = "U?".to_string();

    // Different pin electrical types
    let mut pin_passive = Pin::new("P1", "1", -40, 20, 10, PinOrientation::Left);
    pin_passive.electrical_type = PinElectricalType::Passive;
    sym.add_pin(pin_passive);

    let mut pin_input = Pin::new("IN", "2", -40, 0, 10, PinOrientation::Left);
    pin_input.electrical_type = PinElectricalType::Input;
    sym.add_pin(pin_input);

    let mut pin_output = Pin::new("OUT", "3", 40, 10, 10, PinOrientation::Right);
    pin_output.electrical_type = PinElectricalType::Output;
    sym.add_pin(pin_output);

    let mut pin_bidir = Pin::new("IO", "4", 40, -10, 10, PinOrientation::Right);
    pin_bidir.electrical_type = PinElectricalType::Bidirectional;
    sym.add_pin(pin_bidir);

    sym.add_rectangle(Rectangle::new(-30, -20, 30, 30));
    lib.add(sym);

    // Write and read back
    let file = File::create(&file_path).expect("Failed to create file");
    lib.write(file).expect("Failed to write SchLib");
    let read_lib = SchLib::open(&file_path).expect("Failed to read SchLib");

    let read_sym = read_lib.get("IC").expect("Symbol not found");
    assert_eq!(read_sym.pins.len(), 4);

    let p1 = read_sym.pins.iter().find(|p| p.designator == "1").unwrap();
    let p2 = read_sym.pins.iter().find(|p| p.designator == "2").unwrap();
    let p3 = read_sym.pins.iter().find(|p| p.designator == "3").unwrap();
    let p4 = read_sym.pins.iter().find(|p| p.designator == "4").unwrap();

    assert_eq!(p1.electrical_type, PinElectricalType::Passive);
    assert_eq!(p2.electrical_type, PinElectricalType::Input);
    assert_eq!(p3.electrical_type, PinElectricalType::Output);
    assert_eq!(p4.electrical_type, PinElectricalType::Bidirectional);
}

#[test]
fn schlib_file_roundtrip_pin_orientations() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_orientations.SchLib");

    let mut lib = SchLib::new();
    let mut sym = Symbol::new("QUAD_PIN");
    sym.designator = "U?".to_string();

    // Pins in all orientations
    sym.add_pin(Pin::new("L", "1", -20, 0, 10, PinOrientation::Left));
    sym.add_pin(Pin::new("R", "2", 20, 0, 10, PinOrientation::Right));
    sym.add_pin(Pin::new("U", "3", 0, 20, 10, PinOrientation::Up));
    sym.add_pin(Pin::new("D", "4", 0, -20, 10, PinOrientation::Down));

    sym.add_rectangle(Rectangle::new(-10, -10, 10, 10));
    lib.add(sym);

    // Write and read back
    let file = File::create(&file_path).expect("Failed to create file");
    lib.write(file).expect("Failed to write SchLib");
    let read_lib = SchLib::open(&file_path).expect("Failed to read SchLib");

    let read_sym = read_lib.get("QUAD_PIN").expect("Symbol not found");
    assert_eq!(read_sym.pins.len(), 4);

    let orientations: Vec<_> = read_sym.pins.iter().map(|p| p.orientation).collect();
    assert!(orientations.contains(&PinOrientation::Left));
    assert!(orientations.contains(&PinOrientation::Right));
    assert!(orientations.contains(&PinOrientation::Up));
    assert!(orientations.contains(&PinOrientation::Down));
}

// =============================================================================
// Component Ordering Tests
// =============================================================================

#[test]
fn pcblib_file_roundtrip_preserves_order() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_order.PcbLib");

    // Create library with components in specific order
    let mut lib = PcbLib::new();
    for name in ["ALPHA", "BETA", "GAMMA", "DELTA"] {
        let mut fp = Footprint::new(name);
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
        lib.add(fp);
    }

    assert_eq!(lib.names(), vec!["ALPHA", "BETA", "GAMMA", "DELTA"]);

    // Write and read back - order should be preserved
    lib.save(&file_path).expect("Failed to write PcbLib");
    let read_lib = PcbLib::open(&file_path).expect("Failed to read PcbLib");

    assert_eq!(
        read_lib.names(),
        vec!["ALPHA", "BETA", "GAMMA", "DELTA"],
        "Component order should be preserved after roundtrip"
    );
}

#[test]
fn pcblib_file_roundtrip_reorder_preserved() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_reorder.PcbLib");

    // Create library with components
    let mut lib = PcbLib::new();
    for name in ["A", "B", "C", "D"] {
        let mut fp = Footprint::new(name);
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
        lib.add(fp);
    }

    // Reorder components
    let new_order = lib.reorder(&["D", "B", "A", "C"]);
    assert_eq!(new_order, vec!["D", "B", "A", "C"]);

    // Write to file
    lib.save(&file_path).expect("Failed to write PcbLib");

    // Read back and verify order is preserved
    let read_lib = PcbLib::open(&file_path).expect("Failed to read PcbLib");
    assert_eq!(
        read_lib.names(),
        vec!["D", "B", "A", "C"],
        "Reordered component order should be preserved after roundtrip"
    );
}

#[test]
fn schlib_file_roundtrip_pin_symbols_and_colour() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("test_pin_symbols.SchLib");

    let mut lib = SchLib::new();
    let mut sym = Symbol::new("IC_WITH_SYMBOLS");
    sym.designator = "U?".to_string();

    // Pin with symbol decorations and custom colour
    let mut pin1 = Pin::new("CLK", "1", -40, 20, 10, PinOrientation::Left);
    pin1.symbol_inner_edge = PinSymbol::Dot;
    pin1.symbol_outer_edge = PinSymbol::Clock;
    pin1.colour = 0x00_00FF; // Red in BGR

    // Pin with different symbols
    let mut pin2 = Pin::new("OUT", "2", 40, 20, 10, PinOrientation::Right);
    pin2.symbol_inside = PinSymbol::ActiveLowOutput;
    pin2.symbol_outside = PinSymbol::OpenCollector;
    pin2.colour = 0xFF_0000; // Blue in BGR

    sym.add_pin(pin1);
    sym.add_pin(pin2);
    sym.add_rectangle(Rectangle::new(-30, 0, 30, 40));
    lib.add(sym);

    // Write and read back
    let file = File::create(&file_path).expect("Failed to create file");
    lib.write(file).expect("Failed to write SchLib");
    let read_lib = SchLib::open(&file_path).expect("Failed to read SchLib");

    let read_sym = read_lib.get("IC_WITH_SYMBOLS").expect("Symbol not found");
    assert_eq!(read_sym.pins.len(), 2);

    let p1 = read_sym.pins.iter().find(|p| p.designator == "1").unwrap();
    assert_eq!(p1.symbol_inner_edge, PinSymbol::Dot);
    assert_eq!(p1.symbol_outer_edge, PinSymbol::Clock);
    assert_eq!(p1.colour, 0x00_00FF);

    let p2 = read_sym.pins.iter().find(|p| p.designator == "2").unwrap();
    assert_eq!(p2.symbol_inside, PinSymbol::ActiveLowOutput);
    assert_eq!(p2.symbol_outside, PinSymbol::OpenCollector);
    assert_eq!(p2.colour, 0xFF_0000);
}
