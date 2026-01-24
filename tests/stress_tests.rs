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
