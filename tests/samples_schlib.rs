//! Sample-library tests for `SchLib`.
//!
//! Unlike the round-trip tests in `file_io_roundtrip.rs` (which write a library
//! with our own writer and read it back), these tests open a *real*,
//! Altium-authored sample library from `scripts/samples/` with our reader and
//! assert the parsed values against the file's authored intent. This is the
//! reference pattern for the rest of the `samples_*` test files.

use altium_designer_mcp::altium::schlib::{PinElectricalType, PinOrientation, SchLib};
use std::path::PathBuf;

/// Resolves a sample fixture by name under `scripts/samples/`.
fn sample(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("samples")
        .join(name)
}

#[test]
fn samples_exist() {
    let path = sample("symbols.SchLib");
    assert!(
        path.exists(),
        "missing sample fixture: {} — the samples_schlib tests read a real \
         Altium-authored library that must be present on disk",
        path.display()
    );
}

#[test]
fn samples_schlib_pins_etype() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");

    // The library contains exactly one symbol, PINS_ETYPE.
    assert_eq!(lib.len(), 1, "expected exactly one symbol");
    assert_eq!(lib.names(), vec!["PINS_ETYPE"], "unexpected symbol name(s)");

    let symbol = lib.get("PINS_ETYPE").expect("symbol PINS_ETYPE not found");
    assert_eq!(symbol.name, "PINS_ETYPE");
    assert_eq!(symbol.part_count, 1, "PINS_ETYPE is a single-part symbol");
    assert_eq!(symbol.pins.len(), 8, "PINS_ETYPE has 8 pins");

    // Authored pins: each is oriented Left with length 20 (reader units), at
    // x = 0 and y stepping down by 10 (0, -10, -20, … -70). We assert the
    // load-bearing fields per pin; volatile identity (unique_id) is not checked.
    let expected: [(&str, &str, PinElectricalType, i32); 8] = [
        ("1", "IN", PinElectricalType::Input, 0),
        ("2", "IO", PinElectricalType::Bidirectional, -10),
        ("3", "OUT", PinElectricalType::Output, -20),
        ("4", "OC", PinElectricalType::OpenCollector, -30),
        ("5", "PAS", PinElectricalType::Passive, -40),
        ("6", "HIZ", PinElectricalType::HiZ, -50),
        ("7", "OE", PinElectricalType::OpenEmitter, -60),
        ("8", "PWR", PinElectricalType::Power, -70),
    ];

    for (i, &(designator, name, electrical_type, y)) in expected.iter().enumerate() {
        let pin = &symbol.pins[i];
        assert_eq!(pin.designator, designator, "pin[{i}] designator");
        assert_eq!(pin.name, name, "pin[{i}] name");
        assert_eq!(
            pin.electrical_type, electrical_type,
            "pin[{i}] ({designator}) electrical type",
        );
        assert_eq!(
            pin.orientation,
            PinOrientation::Left,
            "pin[{i}] ({designator}) orientation",
        );
        assert_eq!(pin.length, 20, "pin[{i}] ({designator}) length");
        assert_eq!(pin.x, 0, "pin[{i}] ({designator}) x");
        assert_eq!(pin.y, y, "pin[{i}] ({designator}) y");
    }

    // One Altium-default parameter (a `Comment` = "*").
    assert_eq!(symbol.parameters.len(), 1, "expected one parameter");
}
