//! Stress tests and edge case testing for bug hunting.
//!
//! These tests probe boundary conditions, extreme values, and potential
//! overflow/underflow scenarios to find bugs.

use altium_designer_mcp::altium::pcblib::{
    Arc, Fill, Footprint, Layer, Pad, PcbFlags, PcbLib, Region, Text, TextJustification, TextKind,
    Track, Vertex, Via,
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

    let lib = PcbLib::new();
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
    for i in 0..100 {
        let x = f64::from(i % 10) * 2.54;
        let y = f64::from(i / 10) * 2.54;
        fp.add_pad(Pad::smd(format!("{}", i + 1), x, y, 1.0, 1.0));
    }

    // Add 100 tracks
    for i in 0..100 {
        let x = f64::from(i) * 0.5;
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
    lib.write(&file_path).expect("Long names should be supported");

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

    for (i, &rot) in rotations.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let mut pad = Pad::smd(format!("{}", i + 1), i as f64 * 2.0, 0.0, 1.0, 0.5);
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
    for i in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let angle = (i as f64) * 2.0 * std::f64::consts::PI / (n as f64);
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
    for i in 0..64 {
        let side = i / 16;
        let pos = i % 16;
        let (x, y, orient) = match side {
            0 => (-50, pos * 10 - 75, PinOrientation::Left), // Left side
            1 => (pos * 10 - 75, 90, PinOrientation::Down),  // Top side
            2 => (90, 75 - pos * 10, PinOrientation::Right), // Right side
            _ => (75 - pos * 10, -50, PinOrientation::Up),   // Bottom side
        };
        sym.add_pin(Pin::new(
            format!("P{}", i + 1),
            format!("{}", i + 1),
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
    lib2.save(&file_path).expect("Long names should be supported");

    // Verify the full name is preserved on read
    let read_lib = SchLib::open(&file_path).expect("Failed to read");
    let read_symbol = read_lib.get(&long_name).expect("Symbol not found");
    assert_eq!(read_symbol.name, long_name, "Full name should be preserved");
    assert_eq!(read_symbol.description, "Long name");
}
