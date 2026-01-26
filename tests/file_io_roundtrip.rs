//! File I/O roundtrip tests for `PcbLib` and `SchLib`.
//!
//! These tests verify that libraries can be written to files and read back
//! with all data preserved through the full OLE compound document format.

use altium_designer_mcp::altium::pcblib::{
    Arc, Fill, Footprint, Layer, Pad, PadShape, PadStackMode, PcbFlags, PcbLib, Region, Text,
    TextJustification, TextKind, Track, Via,
};
use altium_designer_mcp::altium::schlib::{
    Pin, PinElectricalType, PinOrientation, Rectangle, SchLib, Symbol,
};
use std::fs::File;
use tempfile::TempDir;

/// Creates a temporary directory inside `.tmp/` for test isolation.
/// The directory is automatically cleaned up when the returned `TempDir` is dropped.
///
/// Converts to an absolute path to avoid issues with parallel test execution.
fn test_temp_dir() -> TempDir {
    let tmp_root = std::path::Path::new(".tmp");
    std::fs::create_dir_all(tmp_root).expect("Failed to create .tmp directory");
    // Canonicalize to get absolute path, avoiding cwd-related issues in parallel tests
    let tmp_root = tmp_root
        .canonicalize()
        .expect("Failed to canonicalize .tmp path");
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
        justification: TextJustification::default(),
        flags: PcbFlags::default(),
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

    let mut pad3 = Pad::smd("3", 1.0, 0.0, 1.0, 1.0);
    pad3.shape = PadShape::Oval;
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
    assert_eq!(read_fp.pads[2].shape, PadShape::Oval);
    assert_eq!(read_fp.pads[3].shape, PadShape::RoundedRectangle);
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
