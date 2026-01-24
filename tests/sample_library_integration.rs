//! Integration tests with real Altium library files.
//!
//! These tests use the sample library files in `scripts/` directory.
//! They are marked with `#[ignore]` to avoid CI failures when sample files are missing.
//!
//! Run manually with:
//! ```bash
//! cargo test --test sample_library_integration -- --ignored --nocapture
//! ```

use altium_designer_mcp::altium::{PcbLib, SchLib};
use std::path::Path;

/// Path to sample `PcbLib` file.
const SAMPLE_PCBLIB: &str = "scripts/sample.PcbLib";

/// Path to sample `SchLib` file.
const SAMPLE_SCHLIB: &str = "scripts/sample.SchLib";

// =============================================================================
// PcbLib Integration Tests
// =============================================================================

#[test]
#[ignore = "Requires scripts/sample.PcbLib"]
fn test_read_sample_pcblib() {
    let path = Path::new(SAMPLE_PCBLIB);
    assert!(path.exists(), "Sample file not found: {}", path.display());

    let library = PcbLib::read(path).expect("Failed to read PcbLib");

    println!("PcbLib contains {} footprints:", library.len());
    for fp in library.footprints() {
        println!(
            "  - {} (pads: {}, tracks: {}, arcs: {})",
            fp.name,
            fp.pads.len(),
            fp.tracks.len(),
            fp.arcs.len()
        );
    }

    // Basic sanity checks
    assert!(!library.is_empty(), "Library should not be empty");
}

#[test]
#[ignore = "Requires scripts/sample.PcbLib"]
fn test_pcblib_footprint_data_integrity() {
    let path = Path::new(SAMPLE_PCBLIB);
    if !path.exists() {
        return;
    }

    let library = PcbLib::read(path).expect("Failed to read PcbLib");

    for fp in library.footprints() {
        // Each footprint should have a non-empty name
        assert!(!fp.name.is_empty(), "Footprint name should not be empty");

        // Pad coordinates should be reasonable (within +/- 100mm)
        for pad in &fp.pads {
            assert!(
                pad.x.abs() < 100.0 && pad.y.abs() < 100.0,
                "Pad {} in {} has unreasonable coordinates: ({}, {})",
                pad.designator,
                fp.name,
                pad.x,
                pad.y
            );

            assert!(
                pad.width > 0.0 && pad.height > 0.0,
                "Pad {} in {} has invalid dimensions: {}x{}",
                pad.designator,
                fp.name,
                pad.width,
                pad.height
            );
        }

        // Track coordinates should be reasonable
        for (i, track) in fp.tracks.iter().enumerate() {
            assert!(
                track.x1.abs() < 100.0 && track.y1.abs() < 100.0,
                "Track {i} in {} has unreasonable start coordinates",
                fp.name
            );
            assert!(
                track.x2.abs() < 100.0 && track.y2.abs() < 100.0,
                "Track {i} in {} has unreasonable end coordinates",
                fp.name
            );
            assert!(
                track.width > 0.0,
                "Track {i} in {} has invalid width: {}",
                fp.name,
                track.width
            );
        }
    }
}

#[test]
#[ignore = "Requires scripts/sample.PcbLib"]
fn test_pcblib_roundtrip() {
    use tempfile::tempdir;

    let path = Path::new(SAMPLE_PCBLIB);
    if !path.exists() {
        return;
    }

    let original = PcbLib::read(path).expect("Failed to read original PcbLib");
    let original_count = original.len();

    // Write to temp file
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().join("roundtrip.PcbLib");

    original.write(&temp_path).expect("Failed to write PcbLib");

    // Read back
    let reread = PcbLib::read(&temp_path).expect("Failed to read roundtrip PcbLib");

    // Verify component count matches
    assert_eq!(
        reread.len(),
        original_count,
        "Component count mismatch after roundtrip"
    );

    // Verify all footprint names are preserved
    for fp in original.footprints() {
        assert!(
            reread.get(&fp.name).is_some(),
            "Footprint '{}' missing after roundtrip",
            fp.name
        );
    }
}

// =============================================================================
// SchLib Integration Tests
// =============================================================================

#[test]
#[ignore = "Requires scripts/sample.SchLib"]
fn test_read_sample_schlib() {
    let path = Path::new(SAMPLE_SCHLIB);
    assert!(path.exists(), "Sample file not found: {}", path.display());

    let library = SchLib::open(path).expect("Failed to read SchLib");

    println!("SchLib contains {} symbols:", library.len());
    for (name, sym) in library.iter() {
        println!(
            "  - {} (pins: {}, rectangles: {}, lines: {})",
            name,
            sym.pins.len(),
            sym.rectangles.len(),
            sym.lines.len()
        );
    }

    // Basic sanity checks
    assert!(!library.is_empty(), "Library should not be empty");
}

#[test]
#[ignore = "Requires scripts/sample.SchLib"]
fn test_schlib_symbol_data_integrity() {
    let path = Path::new(SAMPLE_SCHLIB);
    if !path.exists() {
        return;
    }

    let library = SchLib::open(path).expect("Failed to read SchLib");

    for (name, sym) in library.iter() {
        // Each symbol should have a non-empty name
        assert!(!name.is_empty(), "Symbol name should not be empty");

        // Pin coordinates should be reasonable (within +/- 1000 schematic units)
        for pin in &sym.pins {
            assert!(
                pin.x.abs() < 1000 && pin.y.abs() < 1000,
                "Pin {} in {name} has unreasonable coordinates: ({}, {})",
                pin.designator,
                pin.x,
                pin.y
            );

            assert!(
                pin.length >= 0,
                "Pin {} in {name} has negative length: {}",
                pin.designator,
                pin.length
            );
        }

        // Rectangle coordinates should be reasonable
        for (i, rect) in sym.rectangles.iter().enumerate() {
            assert!(
                rect.x1.abs() < 1000 && rect.y1.abs() < 1000,
                "Rectangle {i} in {name} has unreasonable coordinates"
            );
        }
    }
}

#[test]
#[ignore = "Requires scripts/sample.SchLib"]
fn test_schlib_roundtrip() {
    use std::fs::File;
    use tempfile::tempdir;

    let path = Path::new(SAMPLE_SCHLIB);
    if !path.exists() {
        return;
    }

    let original = SchLib::open(path).expect("Failed to read original SchLib");
    let original_count = original.len();

    // Write to temp file
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().join("roundtrip.SchLib");

    let file = File::create(&temp_path).expect("Failed to create temp file");
    original.write(file).expect("Failed to write SchLib");

    // Read back
    let reread = SchLib::open(&temp_path).expect("Failed to read roundtrip SchLib");

    // Verify component count matches
    assert_eq!(
        reread.len(),
        original_count,
        "Component count mismatch after roundtrip"
    );

    // Verify all symbol names are preserved
    for (name, _) in original.iter() {
        assert!(
            reread.get(name).is_some(),
            "Symbol '{name}' missing after roundtrip"
        );
    }
}

// =============================================================================
// Cross-Library Tests
// =============================================================================

#[test]
#[ignore = "Requires scripts/sample.PcbLib and scripts/sample.SchLib"]
fn test_validate_sample_libraries() {
    // Test both libraries exist and are valid
    let pcblib_path = Path::new(SAMPLE_PCBLIB);
    let schlib_path = Path::new(SAMPLE_SCHLIB);

    if pcblib_path.exists() {
        let lib = PcbLib::read(pcblib_path).expect("PcbLib should be valid");
        println!("PcbLib validation passed: {} footprints", lib.len());
    }

    if schlib_path.exists() {
        let lib = SchLib::open(schlib_path).expect("SchLib should be valid");
        println!("SchLib validation passed: {} symbols", lib.len());
    }
}

#[test]
#[ignore = "Requires scripts/sample.PcbLib"]
#[allow(clippy::float_cmp)]
fn test_render_sample_footprint_ascii() {
    let path = Path::new(SAMPLE_PCBLIB);
    if !path.exists() {
        return;
    }

    let library = PcbLib::read(path).expect("Failed to read PcbLib");

    // Get first footprint
    let footprints: Vec<_> = library.footprints().collect();
    if let Some(fp) = footprints.first() {
        println!("\nASCII render of '{}':", fp.name);
        println!("Pads: {}, Tracks: {}", fp.pads.len(), fp.tracks.len());

        // Basic bounding box calculation
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        let mut min_y = f64::MAX;
        let mut max_y = f64::MIN;

        for pad in &fp.pads {
            min_x = min_x.min(pad.x - pad.width / 2.0);
            max_x = max_x.max(pad.x + pad.width / 2.0);
            min_y = min_y.min(pad.y - pad.height / 2.0);
            max_y = max_y.max(pad.y + pad.height / 2.0);
        }

        if min_x != f64::MAX {
            println!(
                "Bounding box: {:.2} x {:.2} mm",
                max_x - min_x,
                max_y - min_y
            );
        }
    }
}
