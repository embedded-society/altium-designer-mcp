//! Stress tests and edge case testing for bug hunting.
//!
//! These tests probe boundary conditions, extreme values, and potential
//! overflow/underflow scenarios to find bugs.

use altium_designer_mcp::altium::pcblib::{
    Arc, ComponentBody, Fill, Footprint, Layer, Model3D, Pad, PcbFlags, PcbLib, Region, Text,
    TextJustification, TextKind, Track, Vertex, Via,
};
use altium_designer_mcp::altium::schlib::{Pin, PinOrientation, Rectangle, SchLib, Symbol};
use std::fs::File;
use tempfile::tempdir;

// =============================================================================
// Coordinate Boundary Tests
// =============================================================================

#[test]
fn test_extreme_positive_coordinates() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("extreme_positive.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("EXTREME_POS");

    // Test with large but reasonable coordinates (100mm - typical large PCB)
    fp.add_pad(Pad::smd("1", 100.0, 100.0, 1.0, 1.0));
    fp.add_track(Track::new(0.0, 0.0, 100.0, 100.0, 0.2, Layer::TopOverlay));

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("EXTREME_POS").expect("Footprint not found");

    assert!((read_fp.pads[0].x - 100.0).abs() < 0.001);
    assert!((read_fp.pads[0].y - 100.0).abs() < 0.001);
}

#[test]
fn test_extreme_negative_coordinates() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("extreme_negative.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("EXTREME_NEG");

    // Large negative coordinates
    fp.add_pad(Pad::smd("1", -100.0, -100.0, 1.0, 1.0));
    fp.add_track(Track::new(-100.0, -100.0, 0.0, 0.0, 0.2, Layer::TopOverlay));

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("EXTREME_NEG").expect("Footprint not found");

    assert!((read_fp.pads[0].x - (-100.0)).abs() < 0.001);
    assert!((read_fp.pads[0].y - (-100.0)).abs() < 0.001);
}

#[test]
fn test_very_small_dimensions() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("very_small.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("VERY_SMALL");

    // Very small dimensions (0.001mm = 1 micron - limit of PCB manufacturing)
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.001, 0.001));
    fp.add_track(Track::new(0.0, 0.0, 0.001, 0.001, 0.001, Layer::TopOverlay));

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("VERY_SMALL").expect("Footprint not found");

    // Values should be preserved within internal unit precision
    assert!(read_fp.pads[0].width > 0.0);
    assert!(read_fp.tracks[0].width > 0.0);
}

#[test]
fn test_zero_dimensions() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("zero_dim.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("ZERO_DIM");

    // Zero-width track (degenerate case)
    fp.add_track(Track::new(0.0, 0.0, 1.0, 1.0, 0.0, Layer::TopOverlay));

    // Zero-sized pad
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.0, 0.0));

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("ZERO_DIM").expect("Footprint not found");

    // Should handle zero dimensions gracefully
    assert_eq!(read_fp.tracks.len(), 1);
    assert_eq!(read_fp.pads.len(), 1);
}

// =============================================================================
// Empty/Minimal Content Tests
// =============================================================================

#[test]
fn test_empty_footprint() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("empty_fp.PcbLib");

    let mut lib = PcbLib::new();
    let fp = Footprint::new("EMPTY_FOOTPRINT");
    lib.add(fp);

    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib
        .get("EMPTY_FOOTPRINT")
        .expect("Footprint not found");

    assert!(read_fp.pads.is_empty());
    assert!(read_fp.tracks.is_empty());
    assert!(read_fp.arcs.is_empty());
    assert!(read_fp.regions.is_empty());
}

#[test]
fn test_empty_library() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("empty_lib.PcbLib");

    let mut lib = PcbLib::new();
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    assert_eq!(read_lib.len(), 0);
}

#[test]
fn test_footprint_with_many_primitives() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("many_primitives.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("MANY_PRIMITIVES");

    // Add 100 pads
    for pad_idx in 0..100 {
        let x = f64::from(pad_idx % 10) * 2.54;
        let y = f64::from(pad_idx / 10) * 2.54;
        fp.add_pad(Pad::smd(format!("{}", pad_idx + 1), x, y, 1.0, 1.0));
    }

    // Add 100 tracks
    for track_idx in 0..100 {
        let x = f64::from(track_idx) * 0.5;
        fp.add_track(Track::new(x, 0.0, x, 10.0, 0.2, Layer::TopOverlay));
    }

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib
        .get("MANY_PRIMITIVES")
        .expect("Footprint not found");

    assert_eq!(read_fp.pads.len(), 100);
    assert_eq!(read_fp.tracks.len(), 100);
}

// =============================================================================
// Special Character Tests
// =============================================================================

#[test]
fn test_unicode_in_footprint_name() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("unicode.PcbLib");

    let mut lib = PcbLib::new();

    // Note: Altium may not fully support Unicode, test ASCII compatibility
    let mut fp = Footprint::new("FP_TEST_123");
    fp.description = "Test description".to_string();
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    assert!(read_lib.get("FP_TEST_123").is_some());
}

#[test]
fn test_long_footprint_name() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("long_name.PcbLib");

    let mut lib = PcbLib::new();

    // OLE compound file format has a ~31 character limit for names
    // Test with a name at the limit
    let long_name = "A".repeat(30);
    let mut fp = Footprint::new(&long_name);
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    assert!(read_lib.get(&long_name).is_some());
}

#[test]
fn test_footprint_name_too_long_returns_error() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("too_long.PcbLib");

    let mut lib = PcbLib::new();

    // Long names are now supported via OLE name truncation + PATTERN storage
    let long_name = "A".repeat(64);
    let mut fp = Footprint::new(&long_name);
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));

    lib.add(fp);

    // This should succeed - the OLE storage uses a truncated name,
    // but the full name is preserved in the PATTERN field
    lib.write(&file_path)
        .expect("Long names should be supported");

    // Verify the full name is preserved on read
    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    assert_eq!(read_lib.len(), 1);

    let read_fp = read_lib.footprints().next().expect("Footprint not found");
    assert_eq!(read_fp.name, long_name, "Full name should be preserved");
}

// =============================================================================
// Rotation and Angle Tests
// =============================================================================

#[test]
fn test_pad_rotation_boundaries() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("rotation.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("ROTATION_TEST");

    // Test various rotation values
    let rotations = [0.0, 45.0, 90.0, 180.0, 270.0, 359.9, -45.0, -90.0];

    for (pad_idx, &rot) in rotations.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let mut pad = Pad::smd(
            format!("{}", pad_idx + 1),
            pad_idx as f64 * 2.0,
            0.0,
            1.0,
            0.5,
        );
        pad.rotation = rot;
        fp.add_pad(pad);
    }

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("ROTATION_TEST").expect("Footprint not found");

    assert_eq!(read_fp.pads.len(), rotations.len());
}

#[test]
fn test_arc_angle_boundaries() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("arc_angles.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("ARC_ANGLES");

    // Full circle
    fp.add_arc(Arc::circle(0.0, 0.0, 1.0, 0.1, Layer::TopOverlay));

    // Quarter arc
    let mut quarter = Arc::circle(5.0, 0.0, 1.0, 0.1, Layer::TopOverlay);
    quarter.start_angle = 0.0;
    quarter.end_angle = 90.0;
    fp.add_arc(quarter);

    // Negative angles
    let mut neg_arc = Arc::circle(10.0, 0.0, 1.0, 0.1, Layer::TopOverlay);
    neg_arc.start_angle = -90.0;
    neg_arc.end_angle = 90.0;
    fp.add_arc(neg_arc);

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("ARC_ANGLES").expect("Footprint not found");

    assert_eq!(read_fp.arcs.len(), 3);
}

// =============================================================================
// Region/Polygon Tests
// =============================================================================

#[test]
fn test_complex_region() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("complex_region.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("COMPLEX_REGION");

    // L-shaped region
    let region = Region {
        vertices: vec![
            Vertex { x: 0.0, y: 0.0 },
            Vertex { x: 2.0, y: 0.0 },
            Vertex { x: 2.0, y: 1.0 },
            Vertex { x: 1.0, y: 1.0 },
            Vertex { x: 1.0, y: 2.0 },
            Vertex { x: 0.0, y: 2.0 },
        ],
        layer: Layer::TopCourtyard,
        flags: PcbFlags::default(),
        unique_id: None,
    };
    fp.add_region(region);

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("COMPLEX_REGION").expect("Footprint not found");

    assert_eq!(read_fp.regions.len(), 1);
    assert_eq!(read_fp.regions[0].vertices.len(), 6);
}

#[test]
fn test_region_with_many_vertices() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("many_vertices.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("MANY_VERTICES");

    // Circle approximation with many vertices
    let n = 36;
    let mut vertices = Vec::new();
    for vertex_idx in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let angle = (vertex_idx as f64) * 2.0 * std::f64::consts::PI / (n as f64);
        vertices.push(Vertex {
            x: angle.cos(),
            y: angle.sin(),
        });
    }

    let region = Region {
        vertices,
        layer: Layer::TopCourtyard,
        flags: PcbFlags::default(),
        unique_id: None,
    };
    fp.add_region(region);

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("MANY_VERTICES").expect("Footprint not found");

    assert_eq!(read_fp.regions.len(), 1);
    assert_eq!(read_fp.regions[0].vertices.len(), n);
}

// =============================================================================
// SchLib Stress Tests
// =============================================================================

#[test]
fn test_symbol_with_many_pins() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("many_pins.SchLib");

    let mut lib = SchLib::new();
    let mut sym = Symbol::new("MANY_PINS");
    sym.designator = "U?".to_string();

    // 64-pin IC
    for pin_idx in 0..64 {
        let side = pin_idx / 16;
        let pos = pin_idx % 16;
        let (x, y, orient) = match side {
            0 => (-50, pos * 10 - 75, PinOrientation::Left), // Left side
            1 => (pos * 10 - 75, 90, PinOrientation::Down),  // Top side
            2 => (90, 75 - pos * 10, PinOrientation::Right), // Right side
            _ => (75 - pos * 10, -50, PinOrientation::Up),   // Bottom side
        };
        sym.add_pin(Pin::new(
            format!("P{}", pin_idx + 1),
            format!("{}", pin_idx + 1),
            x,
            y,
            10,
            orient,
        ));
    }

    sym.add_rectangle(Rectangle::new(-40, -40, 80, 80));
    lib.add_symbol(sym);

    let file = File::create(&file_path).expect("Failed to create file");
    lib.write(file).expect("Failed to write");

    let read_lib = SchLib::open(&file_path).expect("Failed to read");
    let read_sym = read_lib.get("MANY_PINS").expect("Symbol not found");

    assert_eq!(read_sym.pins.len(), 64);
}

#[test]
fn test_multiple_footprints_same_library() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("multi_fp.PcbLib");

    let mut lib = PcbLib::new();

    // Add 20 different footprints
    for i in 0..20 {
        let mut fp = Footprint::new(format!("FP_{i:02}"));
        fp.description = format!("Footprint number {i}");
        fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
        fp.add_track(Track::new(-1.0, -0.5, 1.0, -0.5, 0.15, Layer::TopOverlay));
        lib.add(fp);
    }

    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    assert_eq!(read_lib.len(), 20);

    // Verify each footprint
    for i in 0..20 {
        let name = format!("FP_{i:02}");
        let fp = read_lib
            .get(&name)
            .unwrap_or_else(|| panic!("Footprint {name} not found"));
        assert_eq!(fp.pads.len(), 2);
        assert_eq!(fp.tracks.len(), 1);
    }
}

// =============================================================================
// Text Handling Tests
// =============================================================================

#[test]
fn test_text_special_strings() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("text_special.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("TEXT_SPECIAL");

    // Standard designator reference
    let text1 = Text {
        x: 0.0,
        y: 0.0,
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
    fp.add_text(text1);

    // Comment reference
    let text2 = Text {
        x: 0.0,
        y: -2.0,
        text: ".Comment".to_string(),
        height: 1.0,
        layer: Layer::TopOverlay,
        kind: TextKind::Stroke,
        rotation: 0.0,
        stroke_font: None,
        justification: TextJustification::default(),
        flags: PcbFlags::default(),
        unique_id: None,
    };
    fp.add_text(text2);

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("TEXT_SPECIAL").expect("Footprint not found");

    assert_eq!(read_fp.text.len(), 2);
}

// =============================================================================
// Via Tests
// =============================================================================

#[test]
fn test_multiple_via_types() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("vias.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("VIAS");

    // Standard through-via
    fp.add_via(Via::new(0.0, 0.0, 0.6, 0.3));

    // Larger via
    fp.add_via(Via::new(2.0, 0.0, 1.0, 0.5));

    // Small via
    fp.add_via(Via::new(4.0, 0.0, 0.4, 0.2));

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("VIAS").expect("Footprint not found");

    assert_eq!(read_fp.vias.len(), 3);
}

// =============================================================================
// Fill Tests
// =============================================================================

#[test]
fn test_fills_various_sizes() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("fills.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("FILLS");

    // Small fill
    fp.add_fill(Fill::new(-0.5, -0.5, 0.5, 0.5, Layer::TopOverlay));

    // Large fill
    fp.add_fill(Fill::new(2.0, 2.0, 10.0, 10.0, Layer::TopOverlay));

    // Thin fill (line-like)
    fp.add_fill(Fill::new(0.0, 15.0, 20.0, 15.2, Layer::TopOverlay));

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("FILLS").expect("Footprint not found");

    assert_eq!(read_fp.fills.len(), 3);
}

/// Tests that symbol names longer than 31 characters are now supported
/// via OLE name truncation + `LibReference` storage
#[test]
fn test_schlib_symbol_name_length_validation() {
    use altium_designer_mcp::altium::schlib::{SchLib, Symbol};

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("name_test.SchLib");

    let mut lib = SchLib::new();

    // Create a symbol with a name exactly at the limit (31 chars) - should work
    let valid_name = "A".repeat(31);
    let mut valid_symbol = Symbol::new(&valid_name);
    valid_symbol.description = "Valid length".to_string();
    lib.add_symbol(valid_symbol);
    assert!(lib.save(&file_path).is_ok(), "31-char name should be valid");

    // Create a new lib with long name - should now work with OLE truncation
    let mut lib2 = SchLib::new();
    let long_name = "B".repeat(64);
    let mut long_symbol = Symbol::new(&long_name);
    long_symbol.description = "Long name".to_string();
    lib2.add_symbol(long_symbol);

    // Long names are now supported - the OLE storage uses a truncated name,
    // but the full name is preserved in the LibReference field
    lib2.save(&file_path)
        .expect("Long names should be supported");

    // Verify the full name is preserved on read
    let read_lib = SchLib::open(&file_path).expect("Failed to read");
    let read_symbol = read_lib.get(&long_name).expect("Symbol not found");
    assert_eq!(read_symbol.name, long_name, "Full name should be preserved");
    assert_eq!(read_symbol.description, "Long name");
}

// =============================================================================
// Append Mode Tests (simulating write_pcblib/write_schlib append: true)
// =============================================================================

#[test]
fn test_pcblib_append_mode() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("append_test.PcbLib");

    // Step 1: Create initial library with one footprint
    let mut lib1 = PcbLib::new();
    let mut fp1 = Footprint::new("FOOTPRINT_1");
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    lib1.add(fp1);
    lib1.write(&file_path)
        .expect("Failed to write initial library");

    // Verify initial state
    let check1 = PcbLib::read(&file_path).expect("Failed to read");
    assert_eq!(check1.len(), 1, "Should have 1 footprint initially");

    // Step 2: Simulate append mode - read existing, add new, write back
    let mut lib2 = PcbLib::read(&file_path).expect("Failed to read for append");
    let mut fp2 = Footprint::new("FOOTPRINT_2");
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 0.8, 0.8));
    lib2.add(fp2);
    lib2.write(&file_path)
        .expect("Failed to write appended library");

    // Step 3: Verify both footprints are present
    let final_lib = PcbLib::read(&file_path).expect("Failed to read final library");
    assert_eq!(final_lib.len(), 2, "Should have 2 footprints after append");
    assert!(
        final_lib.get("FOOTPRINT_1").is_some(),
        "Original footprint should be preserved"
    );
    assert!(
        final_lib.get("FOOTPRINT_2").is_some(),
        "Appended footprint should be present"
    );
}

#[test]
fn test_schlib_append_mode() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("append_test.SchLib");

    // Step 1: Create initial library with one symbol
    let mut lib1 = SchLib::new();
    let mut sym1 = Symbol::new("SYMBOL_1");
    sym1.add_pin(Pin::new("P1", "1", 0, 0, 10, PinOrientation::Left));
    sym1.add_rectangle(Rectangle::new(-10, -5, 10, 5));
    lib1.add_symbol(sym1);

    let file = File::create(&file_path).expect("Failed to create file");
    lib1.write(file).expect("Failed to write initial library");

    // Verify initial state
    let check1 = SchLib::open(&file_path).expect("Failed to read");
    assert_eq!(check1.len(), 1, "Should have 1 symbol initially");

    // Step 2: Simulate append mode - read existing, add new, write back
    let mut lib2 = SchLib::open(&file_path).expect("Failed to read for append");
    let mut sym2 = Symbol::new("SYMBOL_2");
    sym2.add_pin(Pin::new("P1", "1", 0, 0, 10, PinOrientation::Left));
    sym2.add_rectangle(Rectangle::new(-10, -5, 10, 5));
    lib2.add_symbol(sym2);

    let file = File::create(&file_path).expect("Failed to create file for write");
    lib2.write(file).expect("Failed to write appended library");

    // Step 3: Verify both symbols are present
    let final_lib = SchLib::open(&file_path).expect("Failed to read final library");
    assert_eq!(final_lib.len(), 2, "Should have 2 symbols after append");
    assert!(
        final_lib.get("SYMBOL_1").is_some(),
        "Original symbol should be preserved"
    );
    assert!(
        final_lib.get("SYMBOL_2").is_some(),
        "Appended symbol should be present"
    );
}

#[test]
fn test_pcblib_append_multiple_footprints() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("append_multi.PcbLib");

    // Start with empty library
    let mut lib = PcbLib::new();

    // Simulate multiple append operations
    for i in 1..=5 {
        let mut fp = Footprint::new(format!("FP_{i}"));
        fp.description = format!("Footprint number {i}");
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        lib.add(fp);

        // Write after each addition
        lib.write(&file_path).expect("Failed to write");

        // Read back for next iteration
        lib = PcbLib::read(&file_path).expect("Failed to read");
    }

    // Final verification
    let final_lib = PcbLib::read(&file_path).expect("Failed to read final");
    assert_eq!(final_lib.len(), 5, "Should have 5 footprints after appends");

    for i in 1..=5 {
        assert!(
            final_lib.get(&format!("FP_{i}")).is_some(),
            "Footprint FP_{i} should exist"
        );
    }
}

// =============================================================================
// 3D Model / ComponentBody Tests
// =============================================================================

#[test]
fn test_component_body_roundtrip() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("component_body.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("WITH_3D_MODEL");
    fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
    fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));

    // Add a ComponentBody with typical 3D model properties
    let body = ComponentBody {
        model_id: "TEST-MODEL-GUID".to_string(),
        model_name: "RESC0603.step".to_string(),
        embedded: false, // External reference (not embedded in library)
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: 0.0,
        z_offset: 0.0,
        overall_height: 0.35,
        standoff_height: 0.0,
        layer: Layer::TopLayer,
        unique_id: None,
    };
    fp.component_bodies.push(body);

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    // Read back and verify
    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("WITH_3D_MODEL").expect("Footprint not found");

    assert_eq!(read_fp.pads.len(), 2, "Should have 2 pads");
    assert_eq!(
        read_fp.component_bodies.len(),
        1,
        "Should have 1 component body"
    );

    let read_body = &read_fp.component_bodies[0];
    assert_eq!(read_body.model_name, "RESC0603.step");
    assert!((read_body.overall_height - 0.35).abs() < 0.001);
}

#[test]
fn test_component_body_with_rotation() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("rotated_body.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("ROTATED_MODEL");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));

    // 3D model with rotation
    let body = ComponentBody {
        model_id: "ROTATED-GUID".to_string(),
        model_name: "SOIC8.step".to_string(),
        embedded: false,
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: 90.0, // Rotated 90 degrees
        z_offset: 0.1,    // 0.1mm standoff
        overall_height: 1.75,
        standoff_height: 0.0,
        layer: Layer::TopLayer,
        unique_id: None,
    };
    fp.component_bodies.push(body);

    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("ROTATED_MODEL").expect("Footprint not found");

    assert_eq!(read_fp.component_bodies.len(), 1);
    let read_body = &read_fp.component_bodies[0];
    assert!((read_body.rotation_z - 90.0).abs() < 0.001);
    assert!((read_body.z_offset - 0.1).abs() < 0.001);
}

// =============================================================================
// Large Library / Pagination Stress Tests
// =============================================================================

#[test]
fn test_large_pcblib_pagination() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("large_library.PcbLib");

    // Create a library with 100 footprints
    let mut lib = PcbLib::new();
    for i in 1..=100 {
        let mut fp = Footprint::new(format!("FP_{i:03}"));
        fp.description = format!("Footprint number {i}");
        fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
        lib.add(fp);
    }

    lib.write(&file_path)
        .expect("Failed to write large library");

    // Verify total count
    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    assert_eq!(read_lib.len(), 100, "Should have 100 footprints");

    // Test iteration with offset and limit (simulating pagination)
    let footprints: Vec<_> = read_lib.footprints().collect();

    // Page 1: first 10
    assert_eq!(footprints.iter().take(10).count(), 10);

    // Page 2: skip 10, take 10
    assert_eq!(footprints.iter().skip(10).take(10).count(), 10);

    // Last page: skip 90, take remaining
    assert_eq!(footprints.iter().skip(90).count(), 10);

    // Verify no duplicates across all footprints
    let names: std::collections::HashSet<_> = footprints.iter().map(|fp| &fp.name).collect();
    assert_eq!(names.len(), 100, "All footprint names should be unique");
}

#[test]
fn test_large_schlib_pagination() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("large_schlib.SchLib");

    // Create a library with 50 symbols
    let mut lib = SchLib::new();
    for i in 1..=50 {
        let mut sym = Symbol::new(format!("SYM_{i:03}"));
        sym.description = format!("Symbol number {i}");
        sym.add_pin(Pin::new("P1", "1", -20, 0, 10, PinOrientation::Left));
        sym.add_pin(Pin::new("P2", "2", 20, 0, 10, PinOrientation::Right));
        sym.add_rectangle(Rectangle::new(-10, -5, 10, 5));
        lib.add_symbol(sym);
    }

    let file = File::create(&file_path).expect("Failed to create file");
    lib.write(file).expect("Failed to write large library");

    // Verify total count
    let read_lib = SchLib::open(&file_path).expect("Failed to read");
    assert_eq!(read_lib.len(), 50, "Should have 50 symbols");

    // Test iteration with pagination
    let symbols: Vec<_> = read_lib.iter().collect();

    // Page 1: first 10
    assert_eq!(symbols.iter().take(10).count(), 10);

    // Page 3: skip 20, take 10
    assert_eq!(symbols.iter().skip(20).take(10).count(), 10);

    // Verify all symbols are present
    let names: std::collections::HashSet<_> =
        symbols.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names.len(), 50, "All symbol names should be unique");
}

#[test]
fn test_pagination_edge_cases() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("edge_case.PcbLib");

    // Create library with 5 footprints
    let mut lib = PcbLib::new();
    for i in 1..=5 {
        let mut fp = Footprint::new(format!("FP_{i}"));
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        lib.add(fp);
    }
    lib.write(&file_path).expect("Failed to write");

    let read_lib = PcbLib::read(&file_path).expect("Failed to read");
    let footprints: Vec<_> = read_lib.footprints().collect();

    // Edge case: offset beyond available items
    assert!(
        footprints.get(100).is_none(),
        "Skipping past all items should return empty"
    );

    // Edge case: limit larger than available
    assert_eq!(
        footprints.iter().take(1000).count(),
        5,
        "Should return all available items"
    );

    // Edge case: zero limit
    assert_eq!(
        footprints.iter().take(0).count(),
        0,
        "Zero limit should return empty"
    );
}

// =============================================================================
// 3D Model Persistence Tests (model_3d -> ComponentBody -> model_3d roundtrip)
// =============================================================================

#[test]
fn test_model_3d_persistence() {
    use std::io::Write;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let step_path = temp_dir.path().join("test_model.step");
    let pcblib_path = temp_dir.path().join("model_3d_test.PcbLib");

    // Create a minimal STEP file (valid header but empty content)
    let step_content = br"ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('Test STEP file'), '2;1');
FILE_NAME('test_model.step', '2024-01-01T00:00:00', (''), (''), '', '', '');
FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));
ENDSEC;
DATA;
ENDSEC;
END-ISO-10303-21;
";

    {
        let mut step_file = File::create(&step_path).expect("Failed to create STEP file");
        step_file
            .write_all(step_content)
            .expect("Failed to write STEP file");
    }

    // Create a footprint with model_3d set
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("MODEL_3D_TEST");
    fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
    fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));

    // Set model_3d (the new persistence feature)
    fp.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.5,  // 0.5mm standoff
        rotation: 45.0, // 45 degree rotation
    });

    lib.add(fp);
    lib.write(&pcblib_path).expect("Failed to write PcbLib");

    // Read back and verify
    let read_lib = PcbLib::read(&pcblib_path).expect("Failed to read PcbLib");
    let read_fp = read_lib.get("MODEL_3D_TEST").expect("Footprint not found");

    // Verify model_3d is populated (from ComponentBody)
    assert!(
        read_fp.model_3d.is_some(),
        "model_3d should be populated after read"
    );

    let model = read_fp.model_3d.as_ref().unwrap();

    // Filepath should be the model filename (not the original full path)
    assert!(
        model.filepath.contains("test_model.step"),
        "Filepath should contain the model filename, got: {}",
        model.filepath
    );

    // Z offset should be preserved
    assert!(
        (model.z_offset - 0.5).abs() < 0.01,
        "Z offset should be ~0.5mm, got: {}",
        model.z_offset
    );

    // Rotation should be preserved
    assert!(
        (model.rotation - 45.0).abs() < 0.01,
        "Rotation should be ~45 degrees, got: {}",
        model.rotation
    );

    // Verify component_bodies is also populated
    assert_eq!(
        read_fp.component_bodies.len(),
        1,
        "Should have 1 ComponentBody"
    );
    let body = &read_fp.component_bodies[0];
    assert_eq!(body.model_name, "test_model.step");
    assert!(body.embedded);
}

#[test]
fn test_model_3d_embedded_data_persistence() {
    use std::io::Write;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let step_path = temp_dir.path().join("embedded_model.step");
    let pcblib_path = temp_dir.path().join("embedded_test.PcbLib");

    // Create a STEP file with recognizable content
    let step_content = b"ISO-10303-21;HEADER;FILE_DESCRIPTION(('Embedded test'));ENDSEC;DATA;ENDSEC;END-ISO-10303-21;";

    {
        let mut step_file = File::create(&step_path).expect("Failed to create STEP file");
        step_file
            .write_all(step_content)
            .expect("Failed to write STEP file");
    }

    // Create footprint with model_3d
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("EMBEDDED_TEST");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });

    lib.add(fp);
    lib.write(&pcblib_path).expect("Failed to write");

    // Read back
    let read_lib = PcbLib::read(&pcblib_path).expect("Failed to read");

    // Verify embedded model data
    assert_eq!(read_lib.model_count(), 1, "Should have 1 embedded model");

    let embedded = read_lib.models().next().expect("Should have model");
    assert_eq!(embedded.name, "embedded_model.step");
    assert!(embedded.size() > 0, "Embedded model should have data");

    // Verify the model data is the decompressed STEP file
    let model_text = embedded.as_string().expect("Model should be valid UTF-8");
    assert!(
        model_text.contains("Embedded test"),
        "Model data should contain original content"
    );
}

/// Tests STEP model extraction to a file (workflow used by `extract_step_model` tool).
#[test]
fn test_step_model_extraction_to_file() {
    use std::io::Write;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let step_path = temp_dir.path().join("original.step");
    let pcblib_path = temp_dir.path().join("extraction_test.PcbLib");
    let extracted_path = temp_dir.path().join("extracted.step");

    // Create a STEP file with recognisable content
    let step_content = b"ISO-10303-21;HEADER;FILE_DESCRIPTION(('Extraction test model'));ENDSEC;DATA;ENDSEC;END-ISO-10303-21;";

    {
        let mut step_file = File::create(&step_path).expect("Failed to create STEP file");
        step_file
            .write_all(step_content)
            .expect("Failed to write STEP file");
    }

    // Create library with embedded model
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("EXTRACT_TEST");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });

    lib.add(fp);
    lib.write(&pcblib_path).expect("Failed to write library");

    // Read library and extract model
    let read_lib = PcbLib::read(&pcblib_path).expect("Failed to read library");
    assert_eq!(read_lib.model_count(), 1, "Should have 1 embedded model");

    let model = read_lib.models().next().expect("Should have model");

    // Extract to file (same as extract_step_model tool does)
    std::fs::write(&extracted_path, &model.data).expect("Failed to extract model");

    // Verify extracted file
    let extracted_content =
        std::fs::read_to_string(&extracted_path).expect("Failed to read extracted file");
    assert!(
        extracted_content.contains("Extraction test model"),
        "Extracted file should contain original content"
    );
    assert!(
        extracted_content.starts_with("ISO-10303"),
        "Extracted file should be valid STEP format"
    );
}

/// Tests STEP model lookup by name and GUID.
#[test]
fn test_step_model_lookup_by_name_and_id() {
    use std::io::Write;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let step_path = temp_dir.path().join("lookup_test.step");
    let pcblib_path = temp_dir.path().join("lookup_test.PcbLib");

    // Create STEP file
    let step_content = b"ISO-10303-21;DATA;ENDSEC;END-ISO-10303-21;";
    {
        let mut step_file = File::create(&step_path).expect("Failed to create STEP file");
        step_file
            .write_all(step_content)
            .expect("Failed to write STEP file");
    }

    // Create library with embedded model
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("LOOKUP_TEST");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });

    lib.add(fp);
    lib.write(&pcblib_path).expect("Failed to write library");

    // Read library
    let read_lib = PcbLib::read(&pcblib_path).expect("Failed to read library");

    // Get model and its ID
    let model = read_lib.models().next().expect("Should have model");
    let model_id = model.id.clone();
    let model_name = model.name.clone();

    // Lookup by GUID should work
    assert!(
        read_lib.get_model(&model_id).is_some(),
        "Should find model by GUID"
    );

    // Verify the model name is preserved
    assert_eq!(
        model_name, "lookup_test.step",
        "Model name should be preserved"
    );
}

// =============================================================================
// Component Rename Tests
// =============================================================================

/// Tests renaming a footprint in a `PcbLib` file.
#[test]
fn test_pcblib_rename_component() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("rename_test.PcbLib");

    // Create library with a footprint
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("OLD_NAME");
    fp.description = "Test footprint".to_string();
    fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
    fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
    fp.add_track(Track::new(-1.0, -0.5, 1.0, -0.5, 0.15, Layer::TopOverlay));
    lib.add(fp);
    lib.write(&file_path).expect("Failed to write");

    // Read, rename, and write back (simulating rename_component tool)
    let mut lib = PcbLib::read(&file_path).expect("Failed to read");
    assert!(lib.get("OLD_NAME").is_some(), "Original should exist");

    let mut footprint = lib.remove("OLD_NAME").expect("Should remove old");
    footprint.name = "NEW_NAME".to_string();
    lib.add(footprint);
    lib.write(&file_path).expect("Failed to write renamed");

    // Verify rename
    let read_lib = PcbLib::read(&file_path).expect("Failed to read final");
    assert_eq!(read_lib.len(), 1, "Should still have 1 component");
    assert!(
        read_lib.get("OLD_NAME").is_none(),
        "Old name should not exist"
    );
    assert!(read_lib.get("NEW_NAME").is_some(), "New name should exist");

    // Verify primitives preserved
    let renamed = read_lib.get("NEW_NAME").unwrap();
    assert_eq!(renamed.description, "Test footprint");
    assert_eq!(renamed.pads.len(), 2);
    assert_eq!(renamed.tracks.len(), 1);
}

/// Tests renaming a symbol in a `SchLib` file.
#[test]
fn test_schlib_rename_component() {
    use altium_designer_mcp::altium::schlib::{
        Pin, PinElectricalType, PinOrientation, Rectangle, Symbol,
    };
    use altium_designer_mcp::altium::SchLib;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("rename_test.SchLib");

    // Create library with a symbol
    let mut lib = SchLib::new();
    let mut sym = Symbol::new("OLD_SYMBOL");
    sym.description = "Test symbol".to_string();
    sym.designator = "U".to_string();
    sym.rectangles.push(Rectangle {
        x1: -40,
        y1: -40,
        x2: 40,
        y2: 40,
        line_width: 1,
        line_color: 0x0000_0000,
        fill_color: 0x0000_FFFF,
        filled: true,
        owner_part_id: 1,
    });
    sym.pins.push(Pin {
        name: "VCC".to_string(),
        designator: "1".to_string(),
        x: -40,
        y: 0,
        length: 20,
        orientation: PinOrientation::Right,
        electrical_type: PinElectricalType::Passive,
        hidden: false,
        show_name: true,
        show_designator: true,
        description: String::new(),
        owner_part_id: 1,
    });
    lib.add_symbol(sym);
    lib.save(&file_path).expect("Failed to write");

    // Read, rename, and write back
    let mut lib = SchLib::open(&file_path).expect("Failed to read");
    assert!(lib.get("OLD_SYMBOL").is_some(), "Original should exist");

    let mut symbol = lib.remove("OLD_SYMBOL").expect("Should remove old");
    symbol.name = "NEW_SYMBOL".to_string();
    lib.add_symbol(symbol);
    lib.save(&file_path).expect("Failed to write renamed");

    // Verify rename
    let read_lib = SchLib::open(&file_path).expect("Failed to read final");
    assert_eq!(read_lib.len(), 1, "Should still have 1 component");
    assert!(
        read_lib.get("OLD_SYMBOL").is_none(),
        "Old name should not exist"
    );
    assert!(
        read_lib.get("NEW_SYMBOL").is_some(),
        "New name should exist"
    );

    // Verify primitives preserved
    let renamed = read_lib.get("NEW_SYMBOL").unwrap();
    assert_eq!(renamed.description, "Test symbol");
    assert_eq!(renamed.designator, "U");
    assert_eq!(renamed.rectangles.len(), 1);
    assert_eq!(renamed.pins.len(), 1);
}

// =============================================================================
// Cross-Library Component Copy Tests
// =============================================================================

/// Tests copying a footprint from one `PcbLib` to another.
#[test]
fn test_pcblib_copy_cross_library() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let source_path = temp_dir.path().join("source.PcbLib");
    let target_path = temp_dir.path().join("target.PcbLib");

    // Create source library with a footprint
    let mut source_lib = PcbLib::new();
    let mut fp = Footprint::new("SOURCE_FP");
    fp.description = "Source footprint".to_string();
    fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
    fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
    fp.add_track(Track::new(-1.0, -0.5, 1.0, -0.5, 0.15, Layer::TopOverlay));
    source_lib.add(fp);
    source_lib
        .write(&source_path)
        .expect("Failed to write source");

    // Simulate cross-library copy (same as the tool does)
    let source_lib = PcbLib::read(&source_path).expect("Failed to read source");
    let source = source_lib
        .get("SOURCE_FP")
        .expect("Source not found")
        .clone();

    let mut target_lib = PcbLib::new();
    target_lib.add(source);
    target_lib
        .write(&target_path)
        .expect("Failed to write target");

    // Verify target library
    let read_target = PcbLib::read(&target_path).expect("Failed to read target");
    assert_eq!(read_target.len(), 1, "Target should have 1 footprint");
    let fp = read_target.get("SOURCE_FP").expect("Footprint not found");
    assert_eq!(fp.description, "Source footprint");
    assert_eq!(fp.pads.len(), 2);
    assert_eq!(fp.tracks.len(), 1);
}

/// Tests copying a footprint to an existing target library.
#[test]
fn test_pcblib_copy_cross_library_to_existing() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let source_path = temp_dir.path().join("source.PcbLib");
    let target_path = temp_dir.path().join("target.PcbLib");

    // Create source library
    let mut source_lib = PcbLib::new();
    let mut fp1 = Footprint::new("FP_A");
    fp1.description = "Footprint A".to_string();
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    source_lib.add(fp1);
    source_lib
        .write(&source_path)
        .expect("Failed to write source");

    // Create target library with existing footprint
    let mut target_lib = PcbLib::new();
    let mut fp2 = Footprint::new("FP_B");
    fp2.description = "Footprint B".to_string();
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
    target_lib.add(fp2);
    target_lib
        .write(&target_path)
        .expect("Failed to write target");

    // Copy from source to target
    let source_lib = PcbLib::read(&source_path).expect("Failed to read source");
    let source = source_lib.get("FP_A").expect("Source not found").clone();

    let mut target_lib = PcbLib::read(&target_path).expect("Failed to read target");
    target_lib.add(source);
    target_lib
        .write(&target_path)
        .expect("Failed to write target");

    // Verify target has both footprints
    let read_target = PcbLib::read(&target_path).expect("Failed to read target");
    assert_eq!(read_target.len(), 2, "Target should have 2 footprints");
    assert!(read_target.get("FP_A").is_some(), "FP_A should exist");
    assert!(read_target.get("FP_B").is_some(), "FP_B should exist");
}

/// Tests copying a footprint with rename.
#[test]
fn test_pcblib_copy_cross_library_with_rename() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let source_path = temp_dir.path().join("source.PcbLib");
    let target_path = temp_dir.path().join("target.PcbLib");

    // Create source library
    let mut source_lib = PcbLib::new();
    let mut fp = Footprint::new("ORIGINAL_NAME");
    fp.description = "Original description".to_string();
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    source_lib.add(fp);
    source_lib
        .write(&source_path)
        .expect("Failed to write source");

    // Copy with rename
    let source_lib = PcbLib::read(&source_path).expect("Failed to read source");
    let mut source = source_lib
        .get("ORIGINAL_NAME")
        .expect("Source not found")
        .clone();
    source.name = "NEW_NAME".to_string();
    source.description = "New description".to_string();

    let mut target_lib = PcbLib::new();
    target_lib.add(source);
    target_lib
        .write(&target_path)
        .expect("Failed to write target");

    // Verify target has renamed footprint
    let read_target = PcbLib::read(&target_path).expect("Failed to read target");
    assert_eq!(read_target.len(), 1);
    assert!(
        read_target.get("ORIGINAL_NAME").is_none(),
        "Old name should not exist"
    );
    assert!(
        read_target.get("NEW_NAME").is_some(),
        "New name should exist"
    );
    assert_eq!(
        read_target.get("NEW_NAME").unwrap().description,
        "New description"
    );
}

/// Tests copying a symbol from one `SchLib` to another.
#[test]
fn test_schlib_copy_cross_library() {
    use altium_designer_mcp::altium::schlib::{
        Pin, PinElectricalType, PinOrientation, Rectangle, Symbol,
    };
    use altium_designer_mcp::altium::SchLib;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let source_path = temp_dir.path().join("source.SchLib");
    let target_path = temp_dir.path().join("target.SchLib");

    // Create source library with a symbol
    let mut source_lib = SchLib::new();
    let mut sym = Symbol::new("SOURCE_SYM");
    sym.description = "Source symbol".to_string();
    sym.designator = "U".to_string();
    sym.rectangles.push(Rectangle {
        x1: -40,
        y1: -40,
        x2: 40,
        y2: 40,
        line_width: 1,
        line_color: 0x0000_0000,
        fill_color: 0x0000_FFFF,
        filled: true,
        owner_part_id: 1,
    });
    sym.pins.push(Pin {
        name: "VCC".to_string(),
        designator: "1".to_string(),
        x: -40,
        y: 0,
        length: 20,
        orientation: PinOrientation::Right,
        electrical_type: PinElectricalType::Passive,
        hidden: false,
        show_name: true,
        show_designator: true,
        description: String::new(),
        owner_part_id: 1,
    });
    source_lib.add_symbol(sym);
    source_lib
        .save(&source_path)
        .expect("Failed to write source");

    // Copy to target
    let source_lib = SchLib::open(&source_path).expect("Failed to read source");
    let source = source_lib
        .get("SOURCE_SYM")
        .expect("Source not found")
        .clone();

    let mut target_lib = SchLib::new();
    target_lib.add_symbol(source);
    target_lib
        .save(&target_path)
        .expect("Failed to write target");

    // Verify target library
    let read_target = SchLib::open(&target_path).expect("Failed to read target");
    assert_eq!(read_target.len(), 1, "Target should have 1 symbol");
    let sym = read_target.get("SOURCE_SYM").expect("Symbol not found");
    assert_eq!(sym.description, "Source symbol");
    assert_eq!(sym.designator, "U");
    assert_eq!(sym.rectangles.len(), 1);
    assert_eq!(sym.pins.len(), 1);
}

// =============================================================================
// Library Import/Export Round-trip Tests
// =============================================================================

/// Tests `PcbLib` round-trip through JSON export/import format.
#[test]
fn test_pcblib_json_roundtrip() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let original_path = temp_dir.path().join("original.PcbLib");
    let imported_path = temp_dir.path().join("imported.PcbLib");

    // Create original library
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("TEST_FP");
    fp.description = "Test footprint for round-trip".to_string();
    fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
    fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
    fp.add_track(Track::new(-1.0, -0.5, 1.0, -0.5, 0.15, Layer::TopOverlay));
    lib.add(fp);
    lib.write(&original_path).expect("Failed to write original");

    // Simulate export: serialise to JSON format matching export_library output
    let read_lib = PcbLib::read(&original_path).expect("Failed to read original");
    let footprints_json: Vec<serde_json::Value> = read_lib
        .footprints()
        .map(|fp| {
            serde_json::json!({
                "name": fp.name,
                "description": fp.description,
                "pads": fp.pads,
                "tracks": fp.tracks,
                "arcs": fp.arcs,
                "regions": fp.regions,
                "text": fp.text,
            })
        })
        .collect();

    let export_json = serde_json::json!({
        "file_type": "PcbLib",
        "footprints": footprints_json,
    });

    // Simulate import: deserialise from JSON and write new library
    let mut new_lib = PcbLib::new();
    let footprints = export_json["footprints"].as_array().unwrap();
    for fp_json in footprints {
        let footprint: Footprint =
            serde_json::from_value(fp_json.clone()).expect("Failed to parse");
        new_lib.add(footprint);
    }
    new_lib
        .write(&imported_path)
        .expect("Failed to write imported");

    // Verify round-trip
    let final_lib = PcbLib::read(&imported_path).expect("Failed to read imported");
    assert_eq!(final_lib.len(), 1);
    let fp = final_lib.get("TEST_FP").expect("Footprint not found");
    assert_eq!(fp.description, "Test footprint for round-trip");
    assert_eq!(fp.pads.len(), 2);
    assert_eq!(fp.tracks.len(), 1);
}

/// Tests `SchLib` round-trip through JSON export/import format.
#[test]
fn test_schlib_json_roundtrip() {
    use altium_designer_mcp::altium::schlib::{
        Pin, PinElectricalType, PinOrientation, Rectangle, Symbol,
    };
    use altium_designer_mcp::altium::SchLib;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let original_path = temp_dir.path().join("original.SchLib");
    let imported_path = temp_dir.path().join("imported.SchLib");

    // Create original library
    let mut lib = SchLib::new();
    let mut sym = Symbol::new("TEST_SYM");
    sym.description = "Test symbol for round-trip".to_string();
    sym.designator = "U".to_string();
    sym.rectangles.push(Rectangle {
        x1: -40,
        y1: -40,
        x2: 40,
        y2: 40,
        line_width: 1,
        line_color: 0x0000_0000,
        fill_color: 0x0000_FFFF,
        filled: true,
        owner_part_id: 1,
    });
    sym.pins.push(Pin {
        name: "VCC".to_string(),
        designator: "1".to_string(),
        x: -40,
        y: 0,
        length: 20,
        orientation: PinOrientation::Right,
        electrical_type: PinElectricalType::Passive,
        hidden: false,
        show_name: true,
        show_designator: true,
        description: String::new(),
        owner_part_id: 1,
    });
    lib.add_symbol(sym);
    lib.save(&original_path).expect("Failed to write original");

    // Simulate export: serialise to JSON format matching export_library output
    let read_lib = SchLib::open(&original_path).expect("Failed to read original");
    let symbols_json: Vec<serde_json::Value> = read_lib
        .iter()
        .map(|(name, symbol)| {
            serde_json::json!({
                "name": name,
                "description": symbol.description,
                "designator": symbol.designator,
                "pins": symbol.pins,
                "rectangles": symbol.rectangles,
                "lines": symbol.lines,
                "polylines": symbol.polylines,
                "arcs": symbol.arcs,
                "ellipses": symbol.ellipses,
                "labels": symbol.labels,
                "parameters": symbol.parameters,
                "footprints": symbol.footprints,
            })
        })
        .collect();

    let export_json = serde_json::json!({
        "file_type": "SchLib",
        "symbols": symbols_json,
    });

    // Simulate import: deserialise from JSON and write new library
    let mut new_lib = SchLib::new();
    let symbols = export_json["symbols"].as_array().unwrap();
    for sym_json in symbols {
        let symbol: Symbol = serde_json::from_value(sym_json.clone()).expect("Failed to parse");
        new_lib.add_symbol(symbol);
    }
    new_lib
        .save(&imported_path)
        .expect("Failed to write imported");

    // Verify round-trip
    let final_lib = SchLib::open(&imported_path).expect("Failed to read imported");
    assert_eq!(final_lib.len(), 1);
    let sym = final_lib.get("TEST_SYM").expect("Symbol not found");
    assert_eq!(sym.description, "Test symbol for round-trip");
    assert_eq!(sym.designator, "U");
    assert_eq!(sym.rectangles.len(), 1);
    assert_eq!(sym.pins.len(), 1);
}

// =============================================================================
// Library Merge Tests
// =============================================================================

/// Tests merging multiple `PcbLib` files into one.
#[test]
fn test_pcblib_merge_libraries() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let source1_path = temp_dir.path().join("source1.PcbLib");
    let source2_path = temp_dir.path().join("source2.PcbLib");
    let target_path = temp_dir.path().join("merged.PcbLib");

    // Create source library 1
    let mut lib1 = PcbLib::new();
    let mut fp1 = Footprint::new("FP_A");
    fp1.description = "Footprint A".to_string();
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    lib1.add(fp1);
    lib1.write(&source1_path).expect("Failed to write source1");

    // Create source library 2
    let mut lib2 = PcbLib::new();
    let mut fp2 = Footprint::new("FP_B");
    fp2.description = "Footprint B".to_string();
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
    lib2.add(fp2);
    lib2.write(&source2_path).expect("Failed to write source2");

    // Merge the libraries (simulating what the tool does)
    let lib1 = PcbLib::read(&source1_path).expect("Failed to read source1");
    let lib2 = PcbLib::read(&source2_path).expect("Failed to read source2");

    let mut merged = PcbLib::new();
    for fp in lib1.footprints() {
        merged.add(fp.clone());
    }
    for fp in lib2.footprints() {
        merged.add(fp.clone());
    }
    merged.write(&target_path).expect("Failed to write merged");

    // Verify merged library
    let result = PcbLib::read(&target_path).expect("Failed to read merged");
    assert_eq!(result.len(), 2, "Merged library should have 2 footprints");
    assert!(result.get("FP_A").is_some(), "FP_A should exist");
    assert!(result.get("FP_B").is_some(), "FP_B should exist");
}

/// Tests merging with duplicate handling (skip).
#[test]
fn test_pcblib_merge_skip_duplicates() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let source1_path = temp_dir.path().join("source1.PcbLib");
    let source2_path = temp_dir.path().join("source2.PcbLib");
    let target_path = temp_dir.path().join("merged.PcbLib");

    // Create source library 1 with FP_A
    let mut lib1 = PcbLib::new();
    let mut fp1 = Footprint::new("FP_A");
    fp1.description = "Original FP_A".to_string();
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    lib1.add(fp1);
    lib1.write(&source1_path).expect("Failed to write source1");

    // Create source library 2 with duplicate FP_A and unique FP_B
    let mut lib2 = PcbLib::new();
    let mut dup_footprint = Footprint::new("FP_A");
    dup_footprint.description = "Duplicate FP_A".to_string();
    dup_footprint.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
    lib2.add(dup_footprint);
    let mut unique_footprint = Footprint::new("FP_B");
    unique_footprint.description = "Footprint B".to_string();
    unique_footprint.add_pad(Pad::smd("1", 0.0, 0.0, 0.7, 0.7));
    lib2.add(unique_footprint);
    lib2.write(&source2_path).expect("Failed to write source2");

    // Merge with skip duplicates
    let lib1 = PcbLib::read(&source1_path).expect("Failed to read source1");
    let lib2 = PcbLib::read(&source2_path).expect("Failed to read source2");

    let mut merged = PcbLib::new();
    for fp in lib1.footprints() {
        merged.add(fp.clone());
    }
    for fp in lib2.footprints() {
        if merged.get(&fp.name).is_none() {
            // Skip duplicates
            merged.add(fp.clone());
        }
    }
    merged.write(&target_path).expect("Failed to write merged");

    // Verify: should have 2 footprints, FP_A from source1
    let result = PcbLib::read(&target_path).expect("Failed to read merged");
    assert_eq!(result.len(), 2);
    let original = result.get("FP_A").expect("FP_A should exist");
    assert_eq!(original.description, "Original FP_A"); // From source1, not source2
    assert!(result.get("FP_B").is_some());
}

/// Tests merging with duplicate handling (rename).
#[test]
fn test_pcblib_merge_rename_duplicates() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let source1_path = temp_dir.path().join("source1.PcbLib");
    let source2_path = temp_dir.path().join("source2.PcbLib");
    let target_path = temp_dir.path().join("merged.PcbLib");

    // Create source library 1 with FP_A
    let mut lib1 = PcbLib::new();
    let mut fp1 = Footprint::new("FP_A");
    fp1.description = "Original FP_A".to_string();
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    lib1.add(fp1);
    lib1.write(&source1_path).expect("Failed to write source1");

    // Create source library 2 with duplicate FP_A
    let mut lib2 = PcbLib::new();
    let mut fp2 = Footprint::new("FP_A");
    fp2.description = "Duplicate FP_A".to_string();
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
    lib2.add(fp2);
    lib2.write(&source2_path).expect("Failed to write source2");

    // Merge with rename duplicates
    let lib1 = PcbLib::read(&source1_path).expect("Failed to read source1");
    let lib2 = PcbLib::read(&source2_path).expect("Failed to read source2");

    let mut merged = PcbLib::new();
    for fp in lib1.footprints() {
        merged.add(fp.clone());
    }
    for fp in lib2.footprints() {
        let mut fp_to_add = fp.clone();
        if merged.get(&fp.name).is_some() {
            // Rename duplicate
            let mut counter = 1;
            let mut new_name = format!("{}_{}", fp.name, counter);
            while merged.get(&new_name).is_some() {
                counter += 1;
                new_name = format!("{}_{}", fp.name, counter);
            }
            fp_to_add.name = new_name;
        }
        merged.add(fp_to_add);
    }
    merged.write(&target_path).expect("Failed to write merged");

    // Verify: should have 2 footprints, FP_A and FP_A_1
    let result = PcbLib::read(&target_path).expect("Failed to read merged");
    assert_eq!(result.len(), 2);
    assert!(result.get("FP_A").is_some());
    assert!(result.get("FP_A_1").is_some());
    assert_eq!(result.get("FP_A").unwrap().description, "Original FP_A");
    assert_eq!(result.get("FP_A_1").unwrap().description, "Duplicate FP_A");
}

/// Tests merging multiple `SchLib` files into one.
#[test]
fn test_schlib_merge_libraries() {
    use altium_designer_mcp::altium::schlib::{
        Pin, PinElectricalType, PinOrientation, Rectangle, Symbol,
    };
    use altium_designer_mcp::altium::SchLib;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let source1_path = temp_dir.path().join("source1.SchLib");
    let source2_path = temp_dir.path().join("source2.SchLib");
    let target_path = temp_dir.path().join("merged.SchLib");

    // Create source library 1
    let mut lib1 = SchLib::new();
    let mut sym1 = Symbol::new("SYM_A");
    sym1.description = "Symbol A".to_string();
    sym1.designator = "U".to_string();
    sym1.rectangles.push(Rectangle {
        x1: -40,
        y1: -40,
        x2: 40,
        y2: 40,
        line_width: 1,
        line_color: 0x0000_0000,
        fill_color: 0x0000_FFFF,
        filled: true,
        owner_part_id: 1,
    });
    lib1.add_symbol(sym1);
    lib1.save(&source1_path).expect("Failed to write source1");

    // Create source library 2
    let mut lib2 = SchLib::new();
    let mut sym2 = Symbol::new("SYM_B");
    sym2.description = "Symbol B".to_string();
    sym2.designator = "R".to_string();
    sym2.pins.push(Pin {
        name: "1".to_string(),
        designator: "1".to_string(),
        x: -40,
        y: 0,
        length: 20,
        orientation: PinOrientation::Right,
        electrical_type: PinElectricalType::Passive,
        hidden: false,
        show_name: true,
        show_designator: true,
        description: String::new(),
        owner_part_id: 1,
    });
    lib2.add_symbol(sym2);
    lib2.save(&source2_path).expect("Failed to write source2");

    // Merge the libraries
    let lib1 = SchLib::open(&source1_path).expect("Failed to read source1");
    let lib2 = SchLib::open(&source2_path).expect("Failed to read source2");

    let mut merged = SchLib::new();
    for (_, sym) in lib1.iter() {
        merged.add_symbol(sym.clone());
    }
    for (_, sym) in lib2.iter() {
        merged.add_symbol(sym.clone());
    }
    merged.save(&target_path).expect("Failed to write merged");

    // Verify merged library
    let result = SchLib::open(&target_path).expect("Failed to read merged");
    assert_eq!(result.len(), 2, "Merged library should have 2 symbols");
    assert!(result.get("SYM_A").is_some(), "SYM_A should exist");
    assert!(result.get("SYM_B").is_some(), "SYM_B should exist");
}

// =============================================================================
// Search Components Tests
// =============================================================================

/// Tests glob pattern search in `PcbLib`.
#[test]
fn test_pcblib_search_glob() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("searchable.PcbLib");

    // Create library with multiple footprints
    let mut lib = PcbLib::new();
    lib.add(Footprint::new("SOIC-8"));
    lib.add(Footprint::new("SOIC-16"));
    lib.add(Footprint::new("TSSOP-8"));
    lib.add(Footprint::new("QFN-24"));
    lib.write(&file_path).expect("Failed to write library");

    // Test glob pattern matching
    let library = PcbLib::read(&file_path).expect("Failed to read library");
    let pattern = regex::Regex::new("(?i)^SOIC-.*$").expect("Failed to compile regex");
    let matches: Vec<String> = library
        .footprints()
        .filter(|fp| pattern.is_match(&fp.name))
        .map(|fp| fp.name.clone())
        .collect();

    assert_eq!(matches.len(), 2, "Should find 2 SOIC footprints");
    assert!(matches.contains(&"SOIC-8".to_string()));
    assert!(matches.contains(&"SOIC-16".to_string()));
}

/// Tests regex pattern search in `PcbLib`.
#[test]
fn test_pcblib_search_regex() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("searchable.PcbLib");

    // Create library with multiple footprints
    let mut lib = PcbLib::new();
    lib.add(Footprint::new("SOIC-8"));
    lib.add(Footprint::new("SOIC-16"));
    lib.add(Footprint::new("TSSOP-8"));
    lib.add(Footprint::new("QFN-24"));
    lib.write(&file_path).expect("Failed to write library");

    // Test regex pattern matching (footprints ending with -8)
    let library = PcbLib::read(&file_path).expect("Failed to read library");
    let pattern = regex::Regex::new("(?i)^.*-8$").expect("Failed to compile regex");
    let matches: Vec<String> = library
        .footprints()
        .filter(|fp| pattern.is_match(&fp.name))
        .map(|fp| fp.name.clone())
        .collect();

    assert_eq!(matches.len(), 2, "Should find 2 footprints ending with -8");
    assert!(matches.contains(&"SOIC-8".to_string()));
    assert!(matches.contains(&"TSSOP-8".to_string()));
}

/// Tests search across multiple `PcbLib` files.
#[test]
fn test_pcblib_search_multiple_libraries() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file1_path = temp_dir.path().join("lib1.PcbLib");
    let file2_path = temp_dir.path().join("lib2.PcbLib");

    // Create first library
    let mut lib1 = PcbLib::new();
    lib1.add(Footprint::new("RES_0402"));
    lib1.add(Footprint::new("RES_0603"));
    lib1.write(&file1_path).expect("Failed to write lib1");

    // Create second library
    let mut lib2 = PcbLib::new();
    lib2.add(Footprint::new("CAP_0402"));
    lib2.add(Footprint::new("RES_0805"));
    lib2.write(&file2_path).expect("Failed to write lib2");

    // Search for RES_* across both libraries
    let pattern = regex::Regex::new("(?i)^RES_.*$").expect("Failed to compile regex");
    let mut all_matches: Vec<String> = Vec::new();

    for path in [&file1_path, &file2_path] {
        let library = PcbLib::read(path).expect("Failed to read library");
        let matches: Vec<String> = library
            .footprints()
            .filter(|fp| pattern.is_match(&fp.name))
            .map(|fp| fp.name.clone())
            .collect();
        all_matches.extend(matches);
    }

    assert_eq!(
        all_matches.len(),
        3,
        "Should find 3 RES_ footprints across libraries"
    );
    assert!(all_matches.contains(&"RES_0402".to_string()));
    assert!(all_matches.contains(&"RES_0603".to_string()));
    assert!(all_matches.contains(&"RES_0805".to_string()));
}

/// Tests search in `SchLib`.
#[test]
fn test_schlib_search() {
    use altium_designer_mcp::altium::schlib::{Rectangle, Symbol};
    use altium_designer_mcp::altium::SchLib;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("searchable.SchLib");

    // Create library with multiple symbols
    let mut lib = SchLib::new();
    let mut sym1 = Symbol::new("LM7805");
    sym1.rectangles.push(Rectangle {
        x1: -40,
        y1: -20,
        x2: 40,
        y2: 20,
        line_width: 1,
        line_color: 0,
        fill_color: 0,
        filled: false,
        owner_part_id: 1,
    });
    lib.add_symbol(sym1);

    let mut sym2 = Symbol::new("LM7812");
    sym2.rectangles.push(Rectangle {
        x1: -40,
        y1: -20,
        x2: 40,
        y2: 20,
        line_width: 1,
        line_color: 0,
        fill_color: 0,
        filled: false,
        owner_part_id: 1,
    });
    lib.add_symbol(sym2);

    let mut sym3 = Symbol::new("NE555");
    sym3.rectangles.push(Rectangle {
        x1: -40,
        y1: -40,
        x2: 40,
        y2: 40,
        line_width: 1,
        line_color: 0,
        fill_color: 0,
        filled: false,
        owner_part_id: 1,
    });
    lib.add_symbol(sym3);

    lib.save(&file_path).expect("Failed to write library");

    // Search for LM78* symbols
    let library = SchLib::open(&file_path).expect("Failed to read library");
    let pattern = regex::Regex::new("(?i)^LM78.*$").expect("Failed to compile regex");
    let matches: Vec<String> = library
        .iter()
        .filter(|(name, _)| pattern.is_match(name))
        .map(|(name, _)| name.clone())
        .collect();

    assert_eq!(matches.len(), 2, "Should find 2 LM78 symbols");
    assert!(matches.contains(&"LM7805".to_string()));
    assert!(matches.contains(&"LM7812".to_string()));
}

// =============================================================================
// Get Component Tests
// =============================================================================

/// Tests getting a single footprint from a `PcbLib`.
#[test]
fn test_pcblib_get_component() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("components.PcbLib");

    // Create library with multiple footprints
    let mut lib = PcbLib::new();

    let mut fp1 = Footprint::new("SOIC-8");
    fp1.add_pad(Pad::smd("1", -1.27, 0.0, 0.6, 1.5));
    lib.add(fp1);

    let mut fp2 = Footprint::new("QFN-24");
    fp2.add_pad(Pad::smd("1", -2.0, 0.0, 0.3, 0.8));
    lib.add(fp2);

    lib.write(&file_path).expect("Failed to write library");

    // Read the library and get a specific component
    let library = PcbLib::read(&file_path).expect("Failed to read library");
    let footprint = library.get("SOIC-8").expect("Component not found");

    assert_eq!(footprint.name, "SOIC-8");
    assert_eq!(footprint.pads.len(), 1);
    assert_eq!(footprint.pads[0].designator, "1");
}

/// Tests getting a component that doesn't exist.
#[test]
fn test_pcblib_get_component_not_found() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("components.PcbLib");

    // Create library with one footprint
    let mut lib = PcbLib::new();
    lib.add(Footprint::new("SOIC-8"));
    lib.write(&file_path).expect("Failed to write library");

    // Try to get a non-existent component
    let library = PcbLib::read(&file_path).expect("Failed to read library");
    let result = library.get("NON_EXISTENT");

    assert!(result.is_none(), "Should not find non-existent component");
}

/// Tests getting a single symbol from a `SchLib`.
#[test]
fn test_schlib_get_component() {
    use altium_designer_mcp::altium::schlib::{
        Pin, PinElectricalType, PinOrientation, Rectangle, Symbol,
    };
    use altium_designer_mcp::altium::SchLib;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("components.SchLib");

    // Create library with multiple symbols
    let mut lib = SchLib::new();

    let mut sym1 = Symbol::new("LM7805");
    sym1.description = "5V Regulator".to_string();
    sym1.rectangles.push(Rectangle {
        x1: -40,
        y1: -30,
        x2: 40,
        y2: 30,
        line_width: 1,
        line_color: 0,
        fill_color: 0,
        filled: false,
        owner_part_id: 1,
    });
    sym1.pins.push(Pin {
        name: "IN".to_string(),
        designator: "1".to_string(),
        x: -40,
        y: 0,
        length: 20,
        orientation: PinOrientation::Right,
        electrical_type: PinElectricalType::Input,
        hidden: false,
        show_name: true,
        show_designator: true,
        description: String::new(),
        owner_part_id: 1,
    });
    lib.add_symbol(sym1);

    let mut sym2 = Symbol::new("NE555");
    sym2.description = "Timer IC".to_string();
    lib.add_symbol(sym2);

    lib.save(&file_path).expect("Failed to write library");

    // Read the library and get a specific component
    let library = SchLib::open(&file_path).expect("Failed to read library");
    let symbol = library.get("LM7805").expect("Component not found");

    assert_eq!(symbol.name, "LM7805");
    assert_eq!(symbol.description, "5V Regulator");
    assert_eq!(symbol.pins.len(), 1);
    assert_eq!(symbol.pins[0].name, "IN");
}

/// Tests getting a symbol that doesn't exist.
#[test]
fn test_schlib_get_component_not_found() {
    use altium_designer_mcp::altium::schlib::Symbol;
    use altium_designer_mcp::altium::SchLib;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("components.SchLib");

    // Create library with one symbol
    let mut lib = SchLib::new();
    lib.add_symbol(Symbol::new("LM7805"));
    lib.save(&file_path).expect("Failed to write library");

    // Try to get a non-existent component
    let library = SchLib::open(&file_path).expect("Failed to read library");
    let result = library.get("NON_EXISTENT");

    assert!(result.is_none(), "Should not find non-existent component");
}
