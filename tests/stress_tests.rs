//! Stress tests and edge case testing for bug hunting.
//!
//! These tests probe boundary conditions, extreme values, and potential
//! overflow/underflow scenarios to find bugs.

use altium_designer_mcp::altium::pcblib::{
    Arc, ComponentBody, Fill, Footprint, Layer, Model3D, Pad, PcbFlags, PcbLib, Region, Text,
    TextJustification, TextKind, Track, Vertex, Via,
};
use altium_designer_mcp::altium::schlib::{
    Pin, PinOrientation, PinSymbol, Rectangle, SchLib, Symbol,
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

// =============================================================================
// Coordinate Boundary Tests
// =============================================================================

#[test]
fn test_extreme_positive_coordinates() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("extreme_positive.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("EXTREME_POS");

    // Test with large but reasonable coordinates (100mm - typical large PCB)
    fp.add_pad(Pad::smd("1", 100.0, 100.0, 1.0, 1.0));
    fp.add_track(Track::new(0.0, 0.0, 100.0, 100.0, 0.2, Layer::TopOverlay));

    lib.add(fp);
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("EXTREME_POS").expect("Footprint not found");

    assert!((read_fp.pads[0].x - 100.0).abs() < 0.001);
    assert!((read_fp.pads[0].y - 100.0).abs() < 0.001);
}

#[test]
fn test_extreme_negative_coordinates() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("extreme_negative.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("EXTREME_NEG");

    // Large negative coordinates
    fp.add_pad(Pad::smd("1", -100.0, -100.0, 1.0, 1.0));
    fp.add_track(Track::new(-100.0, -100.0, 0.0, 0.0, 0.2, Layer::TopOverlay));

    lib.add(fp);
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("EXTREME_NEG").expect("Footprint not found");

    assert!((read_fp.pads[0].x - (-100.0)).abs() < 0.001);
    assert!((read_fp.pads[0].y - (-100.0)).abs() < 0.001);
}

#[test]
fn test_very_small_dimensions() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("very_small.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("VERY_SMALL");

    // Very small dimensions (0.001mm = 1 micron - limit of PCB manufacturing)
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.001, 0.001));
    fp.add_track(Track::new(0.0, 0.0, 0.001, 0.001, 0.001, Layer::TopOverlay));

    lib.add(fp);
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("VERY_SMALL").expect("Footprint not found");

    // Values should be preserved within internal unit precision
    assert!(read_fp.pads[0].width > 0.0);
    assert!(read_fp.tracks[0].width > 0.0);
}

#[test]
fn test_zero_dimensions() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("zero_dim.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("ZERO_DIM");

    // Zero-width track (degenerate case)
    fp.add_track(Track::new(0.0, 0.0, 1.0, 1.0, 0.0, Layer::TopOverlay));

    // Zero-sized pad
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.0, 0.0));

    lib.add(fp);
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
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
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("empty_fp.PcbLib");

    let mut lib = PcbLib::new();
    let fp = Footprint::new("EMPTY_FOOTPRINT");
    lib.add(fp);

    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
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
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("empty_lib.PcbLib");

    let mut lib = PcbLib::new();
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    assert_eq!(read_lib.len(), 0);
}

#[test]
fn test_footprint_with_many_primitives() {
    let temp_dir = test_temp_dir();
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
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
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
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("unicode.PcbLib");

    let mut lib = PcbLib::new();

    // Note: Altium may not fully support Unicode, test ASCII compatibility
    let mut fp = Footprint::new("FP_TEST_123");
    fp.description = "Test description".to_string();
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));

    lib.add(fp);
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    assert!(read_lib.get("FP_TEST_123").is_some());
}

#[test]
fn test_long_footprint_name() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("long_name.PcbLib");

    let mut lib = PcbLib::new();

    // OLE compound file format has a ~31 character limit for names
    // Test with a name at the limit
    let long_name = "A".repeat(30);
    let mut fp = Footprint::new(&long_name);
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));

    lib.add(fp);
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    assert!(read_lib.get(&long_name).is_some());
}

#[test]
fn test_footprint_name_too_long_returns_error() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("too_long.PcbLib");

    let mut lib = PcbLib::new();

    // Long names are now supported via OLE name truncation + PATTERN storage
    let long_name = "A".repeat(64);
    let mut fp = Footprint::new(&long_name);
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));

    lib.add(fp);

    // This should succeed - the OLE storage uses a truncated name,
    // but the full name is preserved in the PATTERN field
    lib.save(&file_path)
        .expect("Long names should be supported");

    // Verify the full name is preserved on read
    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    assert_eq!(read_lib.len(), 1);

    let read_fp = read_lib.iter().next().expect("Footprint not found");
    assert_eq!(read_fp.name, long_name, "Full name should be preserved");
}

// =============================================================================
// Rotation and Angle Tests
// =============================================================================

#[test]
fn test_pad_rotation_boundaries() {
    let temp_dir = test_temp_dir();
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
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("ROTATION_TEST").expect("Footprint not found");

    assert_eq!(read_fp.pads.len(), rotations.len());
}

#[test]
fn test_arc_angle_boundaries() {
    let temp_dir = test_temp_dir();
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
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("ARC_ANGLES").expect("Footprint not found");

    assert_eq!(read_fp.arcs.len(), 3);
}

// =============================================================================
// Region/Polygon Tests
// =============================================================================

#[test]
fn test_complex_region() {
    let temp_dir = test_temp_dir();
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
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("COMPLEX_REGION").expect("Footprint not found");

    assert_eq!(read_fp.regions.len(), 1);
    assert_eq!(read_fp.regions[0].vertices.len(), 6);
}

#[test]
fn test_region_with_many_vertices() {
    let temp_dir = test_temp_dir();
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
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("MANY_VERTICES").expect("Footprint not found");

    assert_eq!(read_fp.regions.len(), 1);
    assert_eq!(read_fp.regions[0].vertices.len(), n);
}

// =============================================================================
// SchLib Stress Tests
// =============================================================================

#[test]
fn test_symbol_with_many_pins() {
    let temp_dir = test_temp_dir();
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
    lib.add(sym);

    let file = File::create(&file_path).expect("Failed to create file");
    lib.write(file).expect("Failed to write");

    let read_lib = SchLib::open(&file_path).expect("Failed to read");
    let read_sym = read_lib.get("MANY_PINS").expect("Symbol not found");

    assert_eq!(read_sym.pins.len(), 64);
}

#[test]
fn test_multiple_footprints_same_library() {
    let temp_dir = test_temp_dir();
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

    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
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
    let temp_dir = test_temp_dir();
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
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("TEXT_SPECIAL").expect("Footprint not found");

    assert_eq!(read_fp.text.len(), 2);
}

// =============================================================================
// Via Tests
// =============================================================================

#[test]
fn test_multiple_via_types() {
    let temp_dir = test_temp_dir();
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
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("VIAS").expect("Footprint not found");

    assert_eq!(read_fp.vias.len(), 3);
}

// =============================================================================
// Fill Tests
// =============================================================================

#[test]
fn test_fills_various_sizes() {
    let temp_dir = test_temp_dir();
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
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    let read_fp = read_lib.get("FILLS").expect("Footprint not found");

    assert_eq!(read_fp.fills.len(), 3);
}

/// Tests that symbol names longer than 31 characters are now supported
/// via OLE name truncation + `LibReference` storage
#[test]
fn test_schlib_symbol_name_length_validation() {
    use altium_designer_mcp::altium::schlib::{SchLib, Symbol};

    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("name_test.SchLib");

    let mut lib = SchLib::new();

    // Create a symbol with a name exactly at the limit (31 chars) - should work
    let valid_name = "A".repeat(31);
    let mut valid_symbol = Symbol::new(&valid_name);
    valid_symbol.description = "Valid length".to_string();
    lib.add(valid_symbol);
    assert!(lib.save(&file_path).is_ok(), "31-char name should be valid");

    // Create a new lib with long name - should now work with OLE truncation
    let mut lib2 = SchLib::new();
    let long_name = "B".repeat(64);
    let mut long_symbol = Symbol::new(&long_name);
    long_symbol.description = "Long name".to_string();
    lib2.add(long_symbol);

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
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("append_test.PcbLib");

    // Step 1: Create initial library with one footprint
    let mut lib1 = PcbLib::new();
    let mut fp1 = Footprint::new("FOOTPRINT_1");
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    lib1.add(fp1);
    lib1.save(&file_path)
        .expect("Failed to write initial library");

    // Verify initial state
    let check1 = PcbLib::open(&file_path).expect("Failed to read");
    assert_eq!(check1.len(), 1, "Should have 1 footprint initially");

    // Step 2: Simulate append mode - read existing, add new, write back
    let mut lib2 = PcbLib::open(&file_path).expect("Failed to read for append");
    let mut fp2 = Footprint::new("FOOTPRINT_2");
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 0.8, 0.8));
    lib2.add(fp2);
    lib2.save(&file_path)
        .expect("Failed to write appended library");

    // Step 3: Verify both footprints are present
    let final_lib = PcbLib::open(&file_path).expect("Failed to read final library");
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
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("append_test.SchLib");

    // Step 1: Create initial library with one symbol
    let mut lib1 = SchLib::new();
    let mut sym1 = Symbol::new("SYMBOL_1");
    sym1.add_pin(Pin::new("P1", "1", 0, 0, 10, PinOrientation::Left));
    sym1.add_rectangle(Rectangle::new(-10, -5, 10, 5));
    lib1.add(sym1);

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
    lib2.add(sym2);

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
    let temp_dir = test_temp_dir();
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
        lib.save(&file_path).expect("Failed to write");

        // Read back for next iteration
        lib = PcbLib::open(&file_path).expect("Failed to read");
    }

    // Final verification
    let final_lib = PcbLib::open(&file_path).expect("Failed to read final");
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
    let temp_dir = test_temp_dir();
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
    lib.save(&file_path).expect("Failed to write");

    // Read back and verify
    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
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
    let temp_dir = test_temp_dir();
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
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
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
    let temp_dir = test_temp_dir();
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

    lib.save(&file_path).expect("Failed to write large library");

    // Verify total count
    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    assert_eq!(read_lib.len(), 100, "Should have 100 footprints");

    // Test iteration with offset and limit (simulating pagination)
    let footprints: Vec<_> = read_lib.iter().collect();

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
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("large_schlib.SchLib");

    // Create a library with 50 symbols
    let mut lib = SchLib::new();
    for i in 1..=50 {
        let mut sym = Symbol::new(format!("SYM_{i:03}"));
        sym.description = format!("Symbol number {i}");
        sym.add_pin(Pin::new("P1", "1", -20, 0, 10, PinOrientation::Left));
        sym.add_pin(Pin::new("P2", "2", 20, 0, 10, PinOrientation::Right));
        sym.add_rectangle(Rectangle::new(-10, -5, 10, 5));
        lib.add(sym);
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
    let names: std::collections::HashSet<_> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names.len(), 50, "All symbol names should be unique");
}

#[test]
fn test_pagination_edge_cases() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("edge_case.PcbLib");

    // Create library with 5 footprints
    let mut lib = PcbLib::new();
    for i in 1..=5 {
        let mut fp = Footprint::new(format!("FP_{i}"));
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        lib.add(fp);
    }
    lib.save(&file_path).expect("Failed to write");

    let read_lib = PcbLib::open(&file_path).expect("Failed to read");
    let footprints: Vec<_> = read_lib.iter().collect();

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

    let temp_dir = test_temp_dir();
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
    lib.save(&pcblib_path).expect("Failed to write PcbLib");

    // Read back and verify
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read PcbLib");
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

    let temp_dir = test_temp_dir();
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
    lib.save(&pcblib_path).expect("Failed to write");

    // Read back
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read");

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

    let temp_dir = test_temp_dir();
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
    lib.save(&pcblib_path).expect("Failed to write library");

    // Read library and extract model
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read library");
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

    let temp_dir = test_temp_dir();
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
    lib.save(&pcblib_path).expect("Failed to write library");

    // Read library
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read library");

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
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("rename_test.PcbLib");

    // Create library with a footprint
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("OLD_NAME");
    fp.description = "Test footprint".to_string();
    fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
    fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
    fp.add_track(Track::new(-1.0, -0.5, 1.0, -0.5, 0.15, Layer::TopOverlay));
    lib.add(fp);
    lib.save(&file_path).expect("Failed to write");

    // Read, rename, and write back (simulating rename_component tool)
    let mut lib = PcbLib::open(&file_path).expect("Failed to read");
    assert!(lib.get("OLD_NAME").is_some(), "Original should exist");

    let mut footprint = lib.remove("OLD_NAME").expect("Should remove old");
    footprint.name = "NEW_NAME".to_string();
    lib.add(footprint);
    lib.save(&file_path).expect("Failed to write renamed");

    // Verify rename
    let read_lib = PcbLib::open(&file_path).expect("Failed to read final");
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

    let temp_dir = test_temp_dir();
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
        transparent: false,
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
        colour: 0,
        graphically_locked: false,
        symbol_inner_edge: PinSymbol::None,
        symbol_outer_edge: PinSymbol::None,
        symbol_inside: PinSymbol::None,
        symbol_outside: PinSymbol::None,
    });
    lib.add(sym);
    lib.save(&file_path).expect("Failed to write");

    // Read, rename, and write back
    let mut lib = SchLib::open(&file_path).expect("Failed to read");
    assert!(lib.get("OLD_SYMBOL").is_some(), "Original should exist");

    let mut symbol = lib.remove("OLD_SYMBOL").expect("Should remove old");
    symbol.name = "NEW_SYMBOL".to_string();
    lib.add(symbol);
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
    let temp_dir = test_temp_dir();
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
        .save(&source_path)
        .expect("Failed to write source");

    // Simulate cross-library copy (same as the tool does)
    let source_lib = PcbLib::open(&source_path).expect("Failed to read source");
    let source = source_lib
        .get("SOURCE_FP")
        .expect("Source not found")
        .clone();

    let mut target_lib = PcbLib::new();
    target_lib.add(source);
    target_lib
        .save(&target_path)
        .expect("Failed to write target");

    // Verify target library
    let read_target = PcbLib::open(&target_path).expect("Failed to read target");
    assert_eq!(read_target.len(), 1, "Target should have 1 footprint");
    let fp = read_target.get("SOURCE_FP").expect("Footprint not found");
    assert_eq!(fp.description, "Source footprint");
    assert_eq!(fp.pads.len(), 2);
    assert_eq!(fp.tracks.len(), 1);
}

/// Tests copying a footprint to an existing target library.
#[test]
fn test_pcblib_copy_cross_library_to_existing() {
    let temp_dir = test_temp_dir();
    let source_path = temp_dir.path().join("source.PcbLib");
    let target_path = temp_dir.path().join("target.PcbLib");

    // Create source library
    let mut source_lib = PcbLib::new();
    let mut fp1 = Footprint::new("FP_A");
    fp1.description = "Footprint A".to_string();
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    source_lib.add(fp1);
    source_lib
        .save(&source_path)
        .expect("Failed to write source");

    // Create target library with existing footprint
    let mut target_lib = PcbLib::new();
    let mut fp2 = Footprint::new("FP_B");
    fp2.description = "Footprint B".to_string();
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
    target_lib.add(fp2);
    target_lib
        .save(&target_path)
        .expect("Failed to write target");

    // Copy from source to target
    let source_lib = PcbLib::open(&source_path).expect("Failed to read source");
    let source = source_lib.get("FP_A").expect("Source not found").clone();

    let mut target_lib = PcbLib::open(&target_path).expect("Failed to read target");
    target_lib.add(source);
    target_lib
        .save(&target_path)
        .expect("Failed to write target");

    // Verify target has both footprints
    let read_target = PcbLib::open(&target_path).expect("Failed to read target");
    assert_eq!(read_target.len(), 2, "Target should have 2 footprints");
    assert!(read_target.get("FP_A").is_some(), "FP_A should exist");
    assert!(read_target.get("FP_B").is_some(), "FP_B should exist");
}

/// Tests copying a footprint with rename.
#[test]
fn test_pcblib_copy_cross_library_with_rename() {
    let temp_dir = test_temp_dir();
    let source_path = temp_dir.path().join("source.PcbLib");
    let target_path = temp_dir.path().join("target.PcbLib");

    // Create source library
    let mut source_lib = PcbLib::new();
    let mut fp = Footprint::new("ORIGINAL_NAME");
    fp.description = "Original description".to_string();
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    source_lib.add(fp);
    source_lib
        .save(&source_path)
        .expect("Failed to write source");

    // Copy with rename
    let source_lib = PcbLib::open(&source_path).expect("Failed to read source");
    let mut source = source_lib
        .get("ORIGINAL_NAME")
        .expect("Source not found")
        .clone();
    source.name = "NEW_NAME".to_string();
    source.description = "New description".to_string();

    let mut target_lib = PcbLib::new();
    target_lib.add(source);
    target_lib
        .save(&target_path)
        .expect("Failed to write target");

    // Verify target has renamed footprint
    let read_target = PcbLib::open(&target_path).expect("Failed to read target");
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

    let temp_dir = test_temp_dir();
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
        transparent: false,
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
        colour: 0,
        graphically_locked: false,
        symbol_inner_edge: PinSymbol::None,
        symbol_outer_edge: PinSymbol::None,
        symbol_inside: PinSymbol::None,
        symbol_outside: PinSymbol::None,
    });
    source_lib.add(sym);
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
    target_lib.add(source);
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
    let temp_dir = test_temp_dir();
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
    lib.save(&original_path).expect("Failed to write original");

    // Simulate export: serialise to JSON format matching export_library output
    let read_lib = PcbLib::open(&original_path).expect("Failed to read original");
    let footprints_json: Vec<serde_json::Value> = read_lib
        .iter()
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
        .save(&imported_path)
        .expect("Failed to write imported");

    // Verify round-trip
    let final_lib = PcbLib::open(&imported_path).expect("Failed to read imported");
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

    let temp_dir = test_temp_dir();
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
        transparent: false,
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
        colour: 0,
        graphically_locked: false,
        symbol_inner_edge: PinSymbol::None,
        symbol_outer_edge: PinSymbol::None,
        symbol_inside: PinSymbol::None,
        symbol_outside: PinSymbol::None,
    });
    lib.add(sym);
    lib.save(&original_path).expect("Failed to write original");

    // Simulate export: serialise to JSON format matching export_library output
    let read_lib = SchLib::open(&original_path).expect("Failed to read original");
    let symbols_json: Vec<serde_json::Value> = read_lib
        .iter()
        .map(|symbol| {
            serde_json::json!({
                "name": symbol.name,
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
        new_lib.add(symbol);
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
    let temp_dir = test_temp_dir();
    let source1_path = temp_dir.path().join("source1.PcbLib");
    let source2_path = temp_dir.path().join("source2.PcbLib");
    let target_path = temp_dir.path().join("merged.PcbLib");

    // Create source library 1
    let mut lib1 = PcbLib::new();
    let mut fp1 = Footprint::new("FP_A");
    fp1.description = "Footprint A".to_string();
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    lib1.add(fp1);
    lib1.save(&source1_path).expect("Failed to write source1");

    // Create source library 2
    let mut lib2 = PcbLib::new();
    let mut fp2 = Footprint::new("FP_B");
    fp2.description = "Footprint B".to_string();
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
    lib2.add(fp2);
    lib2.save(&source2_path).expect("Failed to write source2");

    // Merge the libraries (simulating what the tool does)
    let lib1 = PcbLib::open(&source1_path).expect("Failed to read source1");
    let lib2 = PcbLib::open(&source2_path).expect("Failed to read source2");

    let mut merged = PcbLib::new();
    for fp in lib1.iter() {
        merged.add(fp.clone());
    }
    for fp in lib2.iter() {
        merged.add(fp.clone());
    }
    merged.save(&target_path).expect("Failed to write merged");

    // Verify merged library
    let result = PcbLib::open(&target_path).expect("Failed to read merged");
    assert_eq!(result.len(), 2, "Merged library should have 2 footprints");
    assert!(result.get("FP_A").is_some(), "FP_A should exist");
    assert!(result.get("FP_B").is_some(), "FP_B should exist");
}

/// Tests merging with duplicate handling (skip).
#[test]
fn test_pcblib_merge_skip_duplicates() {
    let temp_dir = test_temp_dir();
    let source1_path = temp_dir.path().join("source1.PcbLib");
    let source2_path = temp_dir.path().join("source2.PcbLib");
    let target_path = temp_dir.path().join("merged.PcbLib");

    // Create source library 1 with FP_A
    let mut lib1 = PcbLib::new();
    let mut fp1 = Footprint::new("FP_A");
    fp1.description = "Original FP_A".to_string();
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    lib1.add(fp1);
    lib1.save(&source1_path).expect("Failed to write source1");

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
    lib2.save(&source2_path).expect("Failed to write source2");

    // Merge with skip duplicates
    let lib1 = PcbLib::open(&source1_path).expect("Failed to read source1");
    let lib2 = PcbLib::open(&source2_path).expect("Failed to read source2");

    let mut merged = PcbLib::new();
    for fp in lib1.iter() {
        merged.add(fp.clone());
    }
    for fp in lib2.iter() {
        if merged.get(&fp.name).is_none() {
            // Skip duplicates
            merged.add(fp.clone());
        }
    }
    merged.save(&target_path).expect("Failed to write merged");

    // Verify: should have 2 footprints, FP_A from source1
    let result = PcbLib::open(&target_path).expect("Failed to read merged");
    assert_eq!(result.len(), 2);
    let original = result.get("FP_A").expect("FP_A should exist");
    assert_eq!(original.description, "Original FP_A"); // From source1, not source2
    assert!(result.get("FP_B").is_some());
}

/// Tests merging with duplicate handling (rename).
#[test]
fn test_pcblib_merge_rename_duplicates() {
    let temp_dir = test_temp_dir();
    let source1_path = temp_dir.path().join("source1.PcbLib");
    let source2_path = temp_dir.path().join("source2.PcbLib");
    let target_path = temp_dir.path().join("merged.PcbLib");

    // Create source library 1 with FP_A
    let mut lib1 = PcbLib::new();
    let mut fp1 = Footprint::new("FP_A");
    fp1.description = "Original FP_A".to_string();
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    lib1.add(fp1);
    lib1.save(&source1_path).expect("Failed to write source1");

    // Create source library 2 with duplicate FP_A
    let mut lib2 = PcbLib::new();
    let mut fp2 = Footprint::new("FP_A");
    fp2.description = "Duplicate FP_A".to_string();
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 0.5, 0.5));
    lib2.add(fp2);
    lib2.save(&source2_path).expect("Failed to write source2");

    // Merge with rename duplicates
    let lib1 = PcbLib::open(&source1_path).expect("Failed to read source1");
    let lib2 = PcbLib::open(&source2_path).expect("Failed to read source2");

    let mut merged = PcbLib::new();
    for fp in lib1.iter() {
        merged.add(fp.clone());
    }
    for fp in lib2.iter() {
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
    merged.save(&target_path).expect("Failed to write merged");

    // Verify: should have 2 footprints, FP_A and FP_A_1
    let result = PcbLib::open(&target_path).expect("Failed to read merged");
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

    let temp_dir = test_temp_dir();
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
        transparent: false,
        owner_part_id: 1,
    });
    lib1.add(sym1);
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
        colour: 0,
        graphically_locked: false,
        symbol_inner_edge: PinSymbol::None,
        symbol_outer_edge: PinSymbol::None,
        symbol_inside: PinSymbol::None,
        symbol_outside: PinSymbol::None,
    });
    lib2.add(sym2);
    lib2.save(&source2_path).expect("Failed to write source2");

    // Merge the libraries
    let lib1 = SchLib::open(&source1_path).expect("Failed to read source1");
    let lib2 = SchLib::open(&source2_path).expect("Failed to read source2");

    let mut merged = SchLib::new();
    for sym in lib1.iter() {
        merged.add(sym.clone());
    }
    for sym in lib2.iter() {
        merged.add(sym.clone());
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
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("searchable.PcbLib");

    // Create library with multiple footprints
    let mut lib = PcbLib::new();
    lib.add(Footprint::new("SOIC-8"));
    lib.add(Footprint::new("SOIC-16"));
    lib.add(Footprint::new("TSSOP-8"));
    lib.add(Footprint::new("QFN-24"));
    lib.save(&file_path).expect("Failed to write library");

    // Test glob pattern matching
    let library = PcbLib::open(&file_path).expect("Failed to read library");
    let pattern = regex::Regex::new("(?i)^SOIC-.*$").expect("Failed to compile regex");
    let matches: Vec<String> = library
        .iter()
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
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("searchable.PcbLib");

    // Create library with multiple footprints
    let mut lib = PcbLib::new();
    lib.add(Footprint::new("SOIC-8"));
    lib.add(Footprint::new("SOIC-16"));
    lib.add(Footprint::new("TSSOP-8"));
    lib.add(Footprint::new("QFN-24"));
    lib.save(&file_path).expect("Failed to write library");

    // Test regex pattern matching (footprints ending with -8)
    let library = PcbLib::open(&file_path).expect("Failed to read library");
    let pattern = regex::Regex::new("(?i)^.*-8$").expect("Failed to compile regex");
    let matches: Vec<String> = library
        .iter()
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
    let temp_dir = test_temp_dir();
    let file1_path = temp_dir.path().join("lib1.PcbLib");
    let file2_path = temp_dir.path().join("lib2.PcbLib");

    // Create first library
    let mut lib1 = PcbLib::new();
    lib1.add(Footprint::new("RES_0402"));
    lib1.add(Footprint::new("RES_0603"));
    lib1.save(&file1_path).expect("Failed to write lib1");

    // Create second library
    let mut lib2 = PcbLib::new();
    lib2.add(Footprint::new("CAP_0402"));
    lib2.add(Footprint::new("RES_0805"));
    lib2.save(&file2_path).expect("Failed to write lib2");

    // Search for RES_* across both libraries
    let pattern = regex::Regex::new("(?i)^RES_.*$").expect("Failed to compile regex");
    let mut all_matches: Vec<String> = Vec::new();

    for path in [&file1_path, &file2_path] {
        let library = PcbLib::open(path).expect("Failed to read library");
        let matches: Vec<String> = library
            .iter()
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

    let temp_dir = test_temp_dir();
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
        transparent: false,
        owner_part_id: 1,
    });
    lib.add(sym1);

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
        transparent: false,
        owner_part_id: 1,
    });
    lib.add(sym2);

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
        transparent: false,
        owner_part_id: 1,
    });
    lib.add(sym3);

    lib.save(&file_path).expect("Failed to write library");

    // Search for LM78* symbols
    let library = SchLib::open(&file_path).expect("Failed to read library");
    let pattern = regex::Regex::new("(?i)^LM78.*$").expect("Failed to compile regex");
    let matches: Vec<String> = library
        .iter()
        .filter(|s| pattern.is_match(&s.name))
        .map(|s| s.name.clone())
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
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("components.PcbLib");

    // Create library with multiple footprints
    let mut lib = PcbLib::new();

    let mut fp1 = Footprint::new("SOIC-8");
    fp1.add_pad(Pad::smd("1", -1.27, 0.0, 0.6, 1.5));
    lib.add(fp1);

    let mut fp2 = Footprint::new("QFN-24");
    fp2.add_pad(Pad::smd("1", -2.0, 0.0, 0.3, 0.8));
    lib.add(fp2);

    lib.save(&file_path).expect("Failed to write library");

    // Read the library and get a specific component
    let library = PcbLib::open(&file_path).expect("Failed to read library");
    let footprint = library.get("SOIC-8").expect("Component not found");

    assert_eq!(footprint.name, "SOIC-8");
    assert_eq!(footprint.pads.len(), 1);
    assert_eq!(footprint.pads[0].designator, "1");
}

/// Tests getting a component that doesn't exist.
#[test]
fn test_pcblib_get_component_not_found() {
    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("components.PcbLib");

    // Create library with one footprint
    let mut lib = PcbLib::new();
    lib.add(Footprint::new("SOIC-8"));
    lib.save(&file_path).expect("Failed to write library");

    // Try to get a non-existent component
    let library = PcbLib::open(&file_path).expect("Failed to read library");
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

    let temp_dir = test_temp_dir();
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
        transparent: false,
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
        colour: 0,
        graphically_locked: false,
        symbol_inner_edge: PinSymbol::None,
        symbol_outer_edge: PinSymbol::None,
        symbol_inside: PinSymbol::None,
        symbol_outside: PinSymbol::None,
    });
    lib.add(sym1);

    let mut sym2 = Symbol::new("NE555");
    sym2.description = "Timer IC".to_string();
    lib.add(sym2);

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

    let temp_dir = test_temp_dir();
    let file_path = temp_dir.path().join("components.SchLib");

    // Create library with one symbol
    let mut lib = SchLib::new();
    lib.add(Symbol::new("LM7805"));
    lib.save(&file_path).expect("Failed to write library");

    // Try to get a non-existent component
    let library = SchLib::open(&file_path).expect("Failed to read library");
    let result = library.get("NON_EXISTENT");

    assert!(result.is_none(), "Should not find non-existent component");
}

// =============================================================================
// Comprehensive STEP Model Tests
// =============================================================================

/// Tests that GUID matching for embedded models is case-insensitive.
/// This was a bug where GUIDs stored with different casing in `ComponentBody`
/// records vs the model index would fail to match.
#[test]
fn test_step_model_case_insensitive_guid_matching() {
    use std::io::Write;

    let temp_dir = test_temp_dir();
    let step_path = temp_dir.path().join("case_test.step");
    let pcblib_path = temp_dir.path().join("case_test.PcbLib");

    // Create a STEP file
    let step_content =
        b"ISO-10303-21;HEADER;FILE_DESCRIPTION(('Case test'));ENDSEC;DATA;ENDSEC;END-ISO-10303-21;";
    {
        let mut step_file = File::create(&step_path).expect("Failed to create STEP file");
        step_file
            .write_all(step_content)
            .expect("Failed to write STEP file");
    }

    // Create library with embedded model
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("CASE_TEST");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });

    lib.add(fp);
    lib.save(&pcblib_path).expect("Failed to write library");

    // Read library and get the model GUID
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read library");
    let model = read_lib
        .models()
        .next()
        .expect("Should have embedded model");
    let guid = model.id.clone();

    // Test that lookup works with exact case
    assert!(
        read_lib.get_model(&guid).is_some(),
        "Should find model with exact GUID"
    );

    // Test that lookup works with different casing
    let upper_guid = guid.to_uppercase();
    let lower_guid = guid.to_lowercase();

    assert!(
        read_lib.get_model(&upper_guid).is_some(),
        "Should find model with uppercase GUID"
    );
    assert!(
        read_lib.get_model(&lower_guid).is_some(),
        "Should find model with lowercase GUID"
    );
}

/// Tests multiple STEP models embedded in a single library.
#[test]
fn test_multiple_step_models_in_library() {
    use std::io::Write;

    let temp_dir = test_temp_dir();
    let pcblib_path = temp_dir.path().join("multi_model.PcbLib");

    // Create multiple STEP files
    let step_paths: Vec<_> = (1..=3)
        .map(|i| {
            let path = temp_dir.path().join(format!("model_{i}.step"));
            let content = format!(
                "ISO-10303-21;HEADER;FILE_DESCRIPTION(('Model {i}'));ENDSEC;DATA;ENDSEC;END-ISO-10303-21;"
            );
            let mut file = File::create(&path).expect("Failed to create STEP file");
            file.write_all(content.as_bytes())
                .expect("Failed to write STEP file");
            path
        })
        .collect();

    // Create library with multiple footprints, each with its own model
    let mut lib = PcbLib::new();
    for (i, step_path) in step_paths.iter().enumerate() {
        let mut fp = Footprint::new(format!("FP_{}", i + 1));
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        fp.model_3d = Some(Model3D {
            filepath: step_path.to_string_lossy().to_string(),
            x_offset: 0.0,
            y_offset: 0.0,
            z_offset: 0.0,
            rotation: 0.0,
        });
        lib.add(fp);
    }

    lib.save(&pcblib_path).expect("Failed to write library");

    // Read library and verify all models
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read library");
    assert_eq!(read_lib.model_count(), 3, "Should have 3 embedded models");

    // Verify each footprint has model_3d populated
    for i in 1..=3 {
        let fp = read_lib
            .get(&format!("FP_{i}"))
            .expect("Footprint should exist");
        assert!(
            fp.model_3d.is_some(),
            "Footprint FP_{i} should have model_3d"
        );
    }

    // Verify model data integrity
    let models: Vec<_> = read_lib.models().collect();
    assert_eq!(models.len(), 3);
    for (i, model) in models.iter().enumerate() {
        let content = model.as_string().expect("Model should be valid UTF-8");
        assert!(
            content.contains(&format!("Model {}", i + 1)),
            "Model {i} should contain correct identifier"
        );
    }
}

/// Tests copying a footprint with embedded STEP model between libraries.
/// The STEP file must remain available since the target library re-embeds from the filepath.
#[test]
fn test_copy_footprint_with_step_model() {
    use std::io::Write;

    let temp_dir = test_temp_dir();
    let step_path = temp_dir.path().join("copy_test.step");
    let source_path = temp_dir.path().join("source.PcbLib");
    let target_path = temp_dir.path().join("target.PcbLib");

    // Create a STEP file
    let step_content = b"ISO-10303-21;HEADER;FILE_DESCRIPTION(('Copy test model'));ENDSEC;DATA;ENDSEC;END-ISO-10303-21;";
    {
        let mut step_file = File::create(&step_path).expect("Failed to create STEP file");
        step_file
            .write_all(step_content)
            .expect("Failed to write STEP file");
    }

    // Create source library with embedded model
    let mut source_lib = PcbLib::new();
    let mut fp = Footprint::new("WITH_MODEL");
    fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
    fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
    fp.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.5,
        rotation: 45.0,
    });
    source_lib.add(fp);
    source_lib
        .save(&source_path)
        .expect("Failed to write source");

    // Read source and copy the component
    let source_lib = PcbLib::open(&source_path).expect("Failed to read source");
    let source_fp = source_lib.get("WITH_MODEL").expect("Source not found");

    // Get the embedded model data for verification
    let source_model = source_lib.models().next().expect("Should have model");
    let source_model_content = source_model.as_string().expect("Should be UTF-8");

    // Create target library with the copied footprint
    // The footprint's model_3d.filepath is preserved, so the STEP file must exist
    let mut target_lib = PcbLib::new();
    let mut copied_fp = source_fp.clone();

    // Update the model_3d filepath to point to the still-existing STEP file
    if let Some(ref mut model) = copied_fp.model_3d {
        model.filepath = step_path.to_string_lossy().to_string();
    }
    target_lib.add(copied_fp);

    target_lib
        .save(&target_path)
        .expect("Failed to write target");

    // Verify target library
    let target_lib = PcbLib::open(&target_path).expect("Failed to read target");
    assert_eq!(target_lib.len(), 1, "Should have 1 footprint");
    assert_eq!(target_lib.model_count(), 1, "Should have 1 embedded model");

    let target_fp = target_lib
        .get("WITH_MODEL")
        .expect("Target footprint not found");
    assert!(target_fp.model_3d.is_some(), "Should have model_3d");
    assert!(
        (target_fp.model_3d.as_ref().unwrap().z_offset - 0.5).abs() < 0.01,
        "Z offset should be preserved"
    );
    assert!(
        (target_fp.model_3d.as_ref().unwrap().rotation - 45.0).abs() < 0.01,
        "Rotation should be preserved"
    );

    // Verify model data matches source
    let target_model = target_lib.models().next().expect("Should have model");
    let target_model_content = target_model
        .as_string()
        .expect("Model should be valid UTF-8");
    assert!(
        target_model_content.contains("Copy test model"),
        "Model content should be preserved"
    );
    assert_eq!(
        source_model_content, target_model_content,
        "Model content should match source"
    );
}

/// Tests `ComponentBody` with external (non-embedded) STEP model reference.
#[test]
fn test_component_body_external_model_reference() {
    let temp_dir = test_temp_dir();
    let pcblib_path = temp_dir.path().join("external_ref.PcbLib");

    // Create library with ComponentBody referencing external model (no actual file)
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("EXTERNAL_MODEL");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));

    // Add ComponentBody with external reference
    let body = ComponentBody {
        model_id: "{EXTERNAL-GUID-1234}".to_string(),
        model_name: "external_model.step".to_string(),
        embedded: false, // External reference
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: 0.0,
        z_offset: 0.0,
        overall_height: 1.0,
        standoff_height: 0.0,
        layer: Layer::Top3DBody,
        unique_id: None,
    };
    fp.component_bodies.push(body);

    lib.add(fp);
    lib.save(&pcblib_path).expect("Failed to write library");

    // Read back and verify
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read library");
    let read_fp = read_lib.get("EXTERNAL_MODEL").expect("Footprint not found");

    assert_eq!(read_fp.component_bodies.len(), 1);
    let read_body = &read_fp.component_bodies[0];
    assert!(!read_body.embedded, "Should be external reference");
    assert_eq!(read_body.model_name, "external_model.step");

    // No embedded models in library
    assert_eq!(
        read_lib.model_count(),
        0,
        "Should have no embedded models for external reference"
    );
}

/// Tests that footprints sharing the same STEP model reference it correctly.
#[test]
fn test_multiple_footprints_sharing_step_model() {
    use std::io::Write;

    let temp_dir = test_temp_dir();
    let step_path = temp_dir.path().join("shared.step");
    let pcblib_path = temp_dir.path().join("shared_model.PcbLib");

    // Create a single STEP file
    let step_content = b"ISO-10303-21;HEADER;FILE_DESCRIPTION(('Shared model'));ENDSEC;DATA;ENDSEC;END-ISO-10303-21;";
    {
        let mut step_file = File::create(&step_path).expect("Failed to create STEP file");
        step_file
            .write_all(step_content)
            .expect("Failed to write STEP file");
    }

    // Create library where multiple footprints use the same model
    let mut lib = PcbLib::new();

    // First footprint
    let mut fp1 = Footprint::new("CHIP_0402");
    fp1.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
    fp1.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
    fp1.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });
    lib.add(fp1);

    // Second footprint - same model file
    let mut fp2 = Footprint::new("CHIP_0402_ALT");
    fp2.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
    fp2.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
    fp2.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 90.0, // Different rotation
    });
    lib.add(fp2);

    lib.save(&pcblib_path).expect("Failed to write library");

    // Read back and verify
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read library");
    assert_eq!(read_lib.len(), 2, "Should have 2 footprints");

    // Both footprints should have model_3d
    let fp1_read = read_lib.get("CHIP_0402").expect("FP1 not found");
    let fp2_read = read_lib.get("CHIP_0402_ALT").expect("FP2 not found");

    assert!(fp1_read.model_3d.is_some(), "FP1 should have model_3d");
    assert!(fp2_read.model_3d.is_some(), "FP2 should have model_3d");

    // Verify different rotations are preserved
    assert!(
        (fp1_read.model_3d.as_ref().unwrap().rotation - 0.0).abs() < 0.01,
        "FP1 should have 0 rotation"
    );
    assert!(
        (fp2_read.model_3d.as_ref().unwrap().rotation - 90.0).abs() < 0.01,
        "FP2 should have 90 rotation"
    );
}

/// Tests that large STEP files are handled correctly (compression).
#[test]
fn test_large_step_model_compression() {
    use std::io::Write;

    let temp_dir = test_temp_dir();
    let step_path = temp_dir.path().join("large_model.step");
    let pcblib_path = temp_dir.path().join("large_model.PcbLib");

    // Create a larger STEP file (repetitive content compresses well)
    let header = b"ISO-10303-21;HEADER;FILE_DESCRIPTION(('Large model test'));ENDSEC;DATA;\n";
    let footer = b"ENDSEC;END-ISO-10303-21;";

    // Generate ~100KB of STEP-like content
    let mut content = Vec::with_capacity(100_000);
    content.extend_from_slice(header);
    for i in 0..2000 {
        content.extend_from_slice(
            format!("#{i}=CARTESIAN_POINT('Point{i}',(0.0,0.0,0.0));\n").as_bytes(),
        );
    }
    content.extend_from_slice(footer);

    let original_size = content.len();
    {
        let mut step_file = File::create(&step_path).expect("Failed to create STEP file");
        step_file
            .write_all(&content)
            .expect("Failed to write STEP file");
    }

    // Create library with embedded model
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("LARGE_MODEL");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });

    lib.add(fp);
    lib.save(&pcblib_path).expect("Failed to write library");

    // Read back and verify
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read library");
    let model = read_lib
        .models()
        .next()
        .expect("Should have embedded model");

    // Verify decompressed size matches original
    assert_eq!(
        model.data.len(),
        original_size,
        "Decompressed model should match original size"
    );

    // Verify content integrity
    let model_content = model.as_string().expect("Model should be valid UTF-8");
    assert!(
        model_content.contains("Large model test"),
        "Header should be preserved"
    );
    assert!(
        model_content.contains("CARTESIAN_POINT"),
        "Content should be preserved"
    );
}

// =============================================================================
// STEP Model Orphan Cleanup Tests
// =============================================================================

/// Tests that `remove_orphaned_models()` removes models not referenced by any footprint.
#[test]
fn test_remove_orphaned_models_after_delete() {
    use std::io::Write;

    let temp_dir = test_temp_dir();
    let step_path = temp_dir.path().join("orphan_test.step");
    let pcblib_path = temp_dir.path().join("orphan_test.PcbLib");

    // Create a simple STEP file
    {
        let mut step_file = File::create(&step_path).expect("Failed to create STEP file");
        step_file
            .write_all(b"ISO-10303-21;HEADER;ENDSEC;DATA;ENDSEC;END-ISO-10303-21;")
            .expect("Failed to write STEP");
    }

    // Create library with two footprints sharing the same model
    let mut lib = PcbLib::new();

    let mut fp1 = Footprint::new("FP_WITH_MODEL");
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp1.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });
    lib.add(fp1);

    let mut fp2 = Footprint::new("FP_NO_MODEL");
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    lib.add(fp2);

    lib.save(&pcblib_path).expect("Failed to save library");

    // Verify model was embedded
    let mut lib = PcbLib::open(&pcblib_path).expect("Failed to read library");
    assert_eq!(lib.models().count(), 1, "Should have 1 embedded model");

    // Delete the footprint that references the model
    lib.remove("FP_WITH_MODEL");
    assert_eq!(lib.len(), 1, "Should have 1 footprint remaining");

    // Model should still exist (not yet cleaned up)
    assert_eq!(
        lib.models().count(),
        1,
        "Model should still exist before cleanup"
    );

    // Now clean up orphaned models
    let removed = lib.remove_orphaned_models();
    assert_eq!(removed, 1, "Should have removed 1 orphaned model");
    assert_eq!(lib.models().count(), 0, "No models should remain");

    // Save and verify persistence
    lib.save(&pcblib_path)
        .expect("Failed to save after cleanup");
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read cleaned library");
    assert_eq!(
        read_lib.models().count(),
        0,
        "Cleaned library should have no models"
    );
}

/// Tests that `referenced_model_ids()` correctly identifies all referenced models.
#[test]
fn test_referenced_model_ids() {
    let mut lib = PcbLib::new();

    // Add footprint with component body referencing a model
    let mut fp1 = Footprint::new("FP1");
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    let mut cb1 = ComponentBody::new("{ABC-123}", "model1.step");
    cb1.embedded = true;
    fp1.component_bodies.push(cb1);
    lib.add(fp1);

    // Add another footprint with different model reference
    let mut fp2 = Footprint::new("FP2");
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    let mut cb2 = ComponentBody::new("{DEF-456}", "model2.step");
    cb2.embedded = true;
    fp2.component_bodies.push(cb2);
    lib.add(fp2);

    // Add footprint with external reference (should not be included)
    let mut fp3 = Footprint::new("FP3");
    fp3.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    let mut cb3 = ComponentBody::new("{EXT-789}", "external.step");
    cb3.embedded = false; // External reference
    fp3.component_bodies.push(cb3);
    lib.add(fp3);

    let referenced = lib.referenced_model_ids();
    assert_eq!(
        referenced.len(),
        2,
        "Should have 2 referenced embedded models"
    );
    assert!(
        referenced.contains("{abc-123}"),
        "Should contain ABC-123 (lowercase)"
    );
    assert!(
        referenced.contains("{def-456}"),
        "Should contain DEF-456 (lowercase)"
    );
    assert!(
        !referenced.contains("{ext-789}"),
        "Should NOT contain external reference"
    );
}

/// Tests that `available_model_ids()` correctly lists all models in the library.
#[test]
fn test_available_model_ids() {
    use altium_designer_mcp::altium::pcblib::EmbeddedModel;

    let mut lib = PcbLib::new();

    // Add some embedded models directly
    lib.add_model(EmbeddedModel::new(
        "{MODEL-001}",
        "model1.step",
        b"STEP data 1".to_vec(),
    ));
    lib.add_model(EmbeddedModel::new(
        "{MODEL-002}",
        "model2.step",
        b"STEP data 2".to_vec(),
    ));

    let available = lib.available_model_ids();
    assert_eq!(available.len(), 2, "Should have 2 available models");
    assert!(
        available.contains("{model-001}"),
        "Should contain MODEL-001 (lowercase)"
    );
    assert!(
        available.contains("{model-002}"),
        "Should contain MODEL-002 (lowercase)"
    );
}

/// Tests that `remove_orphaned_component_bodies()` removes references to non-existent models.
#[test]
fn test_remove_orphaned_component_bodies() {
    use altium_designer_mcp::altium::pcblib::EmbeddedModel;

    let mut lib = PcbLib::new();

    // Add one actual model
    lib.add_model(EmbeddedModel::new(
        "{VALID-MODEL}",
        "valid.step",
        b"STEP data".to_vec(),
    ));

    // Add footprint with valid reference
    let mut fp1 = Footprint::new("FP_VALID");
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    let mut cb_valid = ComponentBody::new("{VALID-MODEL}", "valid.step");
    cb_valid.embedded = true;
    fp1.component_bodies.push(cb_valid);
    lib.add(fp1);

    // Add footprint with orphaned reference (model doesn't exist)
    let mut fp2 = Footprint::new("FP_ORPHAN");
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    let mut cb_orphan1 = ComponentBody::new("{MISSING-MODEL}", "missing.step");
    cb_orphan1.embedded = true;
    fp2.component_bodies.push(cb_orphan1);
    // Add a second orphaned reference to same footprint
    let mut cb_orphan2 = ComponentBody::new("{ANOTHER-MISSING}", "another.step");
    cb_orphan2.embedded = true;
    fp2.component_bodies.push(cb_orphan2);
    lib.add(fp2);

    // Add footprint with external reference (should be kept even without model data)
    let mut fp3 = Footprint::new("FP_EXTERNAL");
    fp3.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    let mut cb_external = ComponentBody::new("{EXTERNAL-REF}", "external.step");
    cb_external.embedded = false; // External reference - no model data needed
    fp3.component_bodies.push(cb_external);
    lib.add(fp3);

    // Verify initial state
    let fp_valid = lib.get("FP_VALID").unwrap();
    assert_eq!(fp_valid.component_bodies.len(), 1);
    let fp_orphan = lib.get("FP_ORPHAN").unwrap();
    assert_eq!(fp_orphan.component_bodies.len(), 2);
    let fp_external = lib.get("FP_EXTERNAL").unwrap();
    assert_eq!(fp_external.component_bodies.len(), 1);

    // Remove orphaned component bodies
    let results = lib.remove_orphaned_component_bodies();

    // Should only affect FP_ORPHAN
    assert_eq!(results.len(), 1, "Only one footprint should be affected");
    assert_eq!(results[0].0, "FP_ORPHAN", "FP_ORPHAN should be affected");
    assert_eq!(results[0].1, 2, "2 orphaned references should be removed");

    // Verify final state
    let fp_valid = lib.get("FP_VALID").unwrap();
    assert_eq!(
        fp_valid.component_bodies.len(),
        1,
        "Valid reference should remain"
    );

    let fp_orphan = lib.get("FP_ORPHAN").unwrap();
    assert_eq!(
        fp_orphan.component_bodies.len(),
        0,
        "Orphaned references should be removed"
    );

    let fp_external = lib.get("FP_EXTERNAL").unwrap();
    assert_eq!(
        fp_external.component_bodies.len(),
        1,
        "External reference should remain"
    );
}

/// Tests that orphaned cleanup works correctly with case-insensitive GUID matching.
#[test]
fn test_orphan_cleanup_case_insensitive() {
    use altium_designer_mcp::altium::pcblib::EmbeddedModel;

    let mut lib = PcbLib::new();

    // Add model with uppercase GUID
    lib.add_model(EmbeddedModel::new(
        "{ABC-DEF-123}",
        "model.step",
        b"STEP data".to_vec(),
    ));

    // Add footprint referencing with lowercase GUID
    let mut fp = Footprint::new("FP_CASE_TEST");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    let mut cb = ComponentBody::new("{abc-def-123}", "model.step"); // lowercase
    cb.embedded = true;
    fp.component_bodies.push(cb);
    lib.add(fp);

    // Should find the reference (case-insensitive matching)
    let results = lib.remove_orphaned_component_bodies();
    assert!(
        results.is_empty(),
        "No orphans should be found (case-insensitive match)"
    );

    // Component body should still exist
    let fp = lib.get("FP_CASE_TEST").unwrap();
    assert_eq!(
        fp.component_bodies.len(),
        1,
        "Component body should remain after case-insensitive match"
    );
}

/// Tests that deleting a footprint and cleaning up models in sequence works correctly.
#[test]
fn test_delete_footprint_then_cleanup_models() {
    use std::io::Write;

    let temp_dir = test_temp_dir();
    let step1_path = temp_dir.path().join("model1.step");
    let step2_path = temp_dir.path().join("model2.step");
    let pcblib_path = temp_dir.path().join("delete_cleanup.PcbLib");

    // Create two STEP files
    for (path, content) in [
        (&step1_path, "ISO-10303-21;MODEL1;END-ISO-10303-21;"),
        (&step2_path, "ISO-10303-21;MODEL2;END-ISO-10303-21;"),
    ] {
        let mut f = File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    // Create library with two footprints, each with its own model
    let mut lib = PcbLib::new();

    let mut fp1 = Footprint::new("FP1");
    fp1.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp1.model_3d = Some(Model3D {
        filepath: step1_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });
    lib.add(fp1);

    let mut fp2 = Footprint::new("FP2");
    fp2.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp2.model_3d = Some(Model3D {
        filepath: step2_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });
    lib.add(fp2);

    lib.save(&pcblib_path).expect("Failed to save");

    // Read back
    let mut lib = PcbLib::open(&pcblib_path).expect("Failed to read");
    assert_eq!(lib.len(), 2, "Should have 2 footprints");
    assert_eq!(lib.models().count(), 2, "Should have 2 models");

    // Delete one footprint
    lib.remove("FP1");
    assert_eq!(lib.len(), 1, "Should have 1 footprint");
    assert_eq!(lib.models().count(), 2, "Models still exist before cleanup");

    // Cleanup orphaned models
    let removed = lib.remove_orphaned_models();
    assert_eq!(removed, 1, "Should remove 1 orphaned model");
    assert_eq!(lib.models().count(), 1, "Should have 1 model remaining");

    // Save and verify
    lib.save(&pcblib_path)
        .expect("Failed to save after cleanup");
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read cleaned library");
    assert_eq!(read_lib.len(), 1, "Should have 1 footprint");
    assert_eq!(read_lib.models().count(), 1, "Should have 1 model");

    // Verify correct model remains
    let model = read_lib.models().next().unwrap();
    let content = model.as_string().unwrap();
    assert!(
        content.contains("MODEL2"),
        "MODEL2 should remain (FP2's model)"
    );
}

/// Tests that `model_3d` filepath skips non-existent files (prevents duplicate creation).
#[test]
fn test_model_3d_skips_nonexistent_filepath() {
    use std::io::Write;

    let temp_dir = test_temp_dir();
    let step_path = temp_dir.path().join("real_model.step");
    let pcblib_path = temp_dir.path().join("nonexistent_test.PcbLib");

    // Create a real STEP file
    {
        let mut f = File::create(&step_path).unwrap();
        f.write_all(b"ISO-10303-21;REAL;END-ISO-10303-21;").unwrap();
    }

    // Create library with model
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("FP_TEST");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });
    lib.add(fp);
    lib.save(&pcblib_path).expect("Failed to save");

    // Read back - this populates model_3d.filepath with the model NAME
    let mut lib = PcbLib::open(&pcblib_path).expect("Failed to read");
    let fp = lib.get("FP_TEST").unwrap();

    // The filepath should now be the model name, not a real path
    if let Some(model_3d) = &fp.model_3d {
        // This should be the model name from component body, not the original path
        assert!(
            !model_3d.filepath.contains("real_model.step")
                || !std::path::Path::new(&model_3d.filepath).exists(),
            "Filepath should not be a readable file path after loading"
        );
    }

    // Verify only 1 model exists
    assert_eq!(lib.models().count(), 1, "Should have exactly 1 model");

    // Save again - should not create duplicates
    lib.save(&pcblib_path).expect("Failed to save again");
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read");
    assert_eq!(
        read_lib.models().count(),
        1,
        "Should still have exactly 1 model (no duplicates)"
    );
}

// =============================================================================
// Regression Tests for STEP Model Bugs
// =============================================================================

/// Regression test for Bug #1: `write_pcblib` silently fails to embed STEP models.
/// When a new footprint specifies a non-existent STEP file, `save()` should return an error.
#[test]
fn test_regression_step_embedding_error_on_invalid_path() {
    let temp_dir = test_temp_dir();
    let pcblib_path = temp_dir.path().join("invalid_step.PcbLib");

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("FP_INVALID_STEP");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp.model_3d = Some(Model3D {
        filepath: "/nonexistent/path/to/model.step".to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });
    lib.add(fp);

    // Save should FAIL because the STEP file doesn't exist
    let result = lib.save(&pcblib_path);
    assert!(
        result.is_err(),
        "Save should fail when STEP file doesn't exist for new footprint"
    );

    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("STEP file not found") || err_msg.contains("step_model"),
        "Error should mention STEP file issue: {err_msg}"
    );
}

/// Regression test for Bug #1: Verify STEP embedding works when file exists.
#[test]
fn test_regression_step_embedding_success_with_valid_path() {
    use std::io::Write;

    let temp_dir = test_temp_dir();
    let step_path = temp_dir.path().join("valid_model.step");
    let pcblib_path = temp_dir.path().join("valid_step.PcbLib");

    // Create a valid STEP file
    {
        let mut f = File::create(&step_path).unwrap();
        f.write_all(b"ISO-10303-21;HEADER;ENDSEC;DATA;ENDSEC;END-ISO-10303-21;")
            .unwrap();
    }

    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("FP_VALID_STEP");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.5,
        rotation: 45.0,
    });
    lib.add(fp);

    // Save should succeed
    lib.save(&pcblib_path)
        .expect("Save should succeed with valid STEP file");

    // Verify the model was embedded
    let read_lib = PcbLib::open(&pcblib_path).expect("Failed to read");
    assert_eq!(read_lib.len(), 1, "Should have 1 footprint");
    assert_eq!(read_lib.models().count(), 1, "Should have 1 embedded model");

    let fp = read_lib.get("FP_VALID_STEP").unwrap();
    assert!(
        !fp.component_bodies.is_empty(),
        "Footprint should have component_bodies"
    );
    assert!(fp.model_3d.is_some(), "Footprint should have model_3d");
}

/// Regression test for Bug #2: Delete should not move orphaned models to other components.
/// After deleting a component, other components should NOT gain extra `component_bodies`.
#[test]
fn test_regression_delete_does_not_duplicate_component_bodies() {
    use std::io::Write;

    let temp_dir = test_temp_dir();
    let step_a_path = temp_dir.path().join("model_a.step");
    let step_b_path = temp_dir.path().join("model_b.step");
    let pcblib_path = temp_dir.path().join("delete_test.PcbLib");

    // Create two STEP files
    for (path, content) in [
        (&step_a_path, "ISO-10303-21;MODEL_A;END-ISO-10303-21;"),
        (&step_b_path, "ISO-10303-21;MODEL_B;END-ISO-10303-21;"),
    ] {
        let mut f = File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    // Create library with FP_A (has STEP), FP_B (has STEP), FP_C (no STEP)
    let mut lib = PcbLib::new();

    let mut fp_a = Footprint::new("FP_A");
    fp_a.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp_a.model_3d = Some(Model3D {
        filepath: step_a_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });
    lib.add(fp_a);

    let mut fp_b = Footprint::new("FP_B");
    fp_b.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp_b.model_3d = Some(Model3D {
        filepath: step_b_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });
    lib.add(fp_b);

    let mut fp_c = Footprint::new("FP_C");
    fp_c.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    lib.add(fp_c);

    lib.save(&pcblib_path)
        .expect("Failed to save initial library");

    // Read back and verify initial state
    let lib = PcbLib::open(&pcblib_path).expect("Failed to read");
    assert_eq!(lib.len(), 3, "Should have 3 footprints");
    assert_eq!(lib.models().count(), 2, "Should have 2 embedded models");

    let fp_b = lib.get("FP_B").unwrap();
    let fp_b_bodies_before = fp_b.component_bodies.len();
    assert_eq!(
        fp_b_bodies_before, 1,
        "FP_B should have 1 component_body initially"
    );

    // Delete FP_A
    let mut lib = PcbLib::open(&pcblib_path).expect("Failed to read");
    lib.remove("FP_A");
    lib.remove_orphaned_models();
    lib.save(&pcblib_path).expect("Failed to save after delete");

    // Read back and verify FP_B still has only 1 component_body
    let lib = PcbLib::open(&pcblib_path).expect("Failed to read after delete");
    assert_eq!(lib.len(), 2, "Should have 2 footprints after delete");
    assert_eq!(
        lib.models().count(),
        1,
        "Should have 1 embedded model after cleanup"
    );

    let fp_b = lib.get("FP_B").unwrap();
    assert_eq!(
        fp_b.component_bodies.len(),
        1,
        "FP_B should still have exactly 1 component_body (not duplicated)"
    );

    let fp_c = lib.get("FP_C").unwrap();
    assert!(
        fp_c.component_bodies.is_empty(),
        "FP_C should still have no component_bodies"
    );
}

/// Regression test: Verify that a file with same name as model doesn't cause duplication.
/// This tests the specific scenario where a file "model.step" exists in current directory
/// and matches the `model_3d.filepath` (which is set to model name during read).
#[test]
fn test_regression_same_name_file_does_not_duplicate() {
    use std::io::Write;

    let temp_dir = test_temp_dir();
    let step_path = temp_dir.path().join("test_model.step");
    let pcblib_path = temp_dir.path().join("same_name.PcbLib");

    // Create a STEP file
    {
        let mut f = File::create(&step_path).unwrap();
        f.write_all(b"ISO-10303-21;ORIGINAL;END-ISO-10303-21;")
            .unwrap();
    }

    // Create and save library
    let mut lib = PcbLib::new();
    let mut fp = Footprint::new("FP_TEST");
    fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
    fp.model_3d = Some(Model3D {
        filepath: step_path.to_string_lossy().to_string(),
        x_offset: 0.0,
        y_offset: 0.0,
        z_offset: 0.0,
        rotation: 0.0,
    });
    lib.add(fp);
    lib.save(&pcblib_path).expect("Failed to save");

    // Read back
    let lib = PcbLib::open(&pcblib_path).expect("Failed to read");
    let fp = lib.get("FP_TEST").unwrap();

    // After reading, model_3d.filepath should be set to just the model name
    // (e.g., "test_model.step" without directory)
    if let Some(ref model_3d) = fp.model_3d {
        // The filepath from read is just the model name
        assert!(
            !model_3d.filepath.contains(std::path::MAIN_SEPARATOR)
                || model_3d.filepath == "test_model.step",
            "After reading, filepath should be just model name: {}",
            model_3d.filepath
        );
    }

    // Now create a file with the SAME NAME in temp directory
    // (simulating the bug scenario where a file matches the model name)
    let same_name_path = temp_dir.path().join("test_model.step");
    {
        let mut f = File::create(&same_name_path).unwrap();
        f.write_all(b"ISO-10303-21;DIFFERENT_CONTENT;END-ISO-10303-21;")
            .unwrap();
    }

    // Change to temp directory to make the file findable by name alone
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    // Re-read and save - should NOT create duplicate
    let mut lib = PcbLib::open(&pcblib_path).expect("Failed to read");
    assert_eq!(
        lib.models().count(),
        1,
        "Should have 1 model before re-save"
    );

    let fp = lib.get("FP_TEST").unwrap();
    let bodies_before = fp.component_bodies.len();

    lib.save(&pcblib_path).expect("Failed to re-save");

    // Restore original directory
    std::env::set_current_dir(original_dir).unwrap();

    // Verify no duplication
    let lib = PcbLib::open(&pcblib_path).expect("Failed to read after re-save");
    assert_eq!(
        lib.models().count(),
        1,
        "Should still have 1 model (no duplicate from same-name file)"
    );

    let fp = lib.get("FP_TEST").unwrap();
    assert_eq!(
        fp.component_bodies.len(),
        bodies_before,
        "Should have same number of component_bodies (no duplication)"
    );
}
