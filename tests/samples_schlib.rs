//! Sample-library tests for `SchLib`.
//!
//! Unlike the round-trip tests in `file_io_roundtrip.rs` (which write a library
//! with our own writer and read it back), these tests open a *real*,
//! Altium-authored sample library from `scripts/samples/` with our reader and
//! assert the parsed values against the file's authored intent. This is the
//! reference pattern for the rest of the `samples_*` test files.

use altium_designer_mcp::altium::schlib::{
    Label, Parameter, Pin, PinElectricalType, PinOrientation, PinSymbol, SchLib, Symbol,
    TextJustification,
};
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

/// Compares two angles (degrees) within a tolerance. Arc angles are stored as
/// `f64`, so they are compared approximately rather than bit-for-bit.
fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Looks up a pin by designator within a symbol, panicking with context if
/// it is absent. Sample tests match primitives by stable fields, never index.
fn pin_by_designator<'a>(symbol: &'a Symbol, designator: &str) -> &'a Pin {
    symbol
        .pins
        .iter()
        .find(|p| p.designator == designator)
        .unwrap_or_else(|| panic!("{}: no pin with designator {designator:?}", symbol.name))
}

#[test]
fn samples_schlib_structure() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");

    // The library now contains nine Altium-authored symbols.
    assert_eq!(lib.len(), 9, "expected exactly nine symbols");

    let names = lib.names();
    for expected in [
        "PINS_ETYPE",
        "PINS_ORIENT",
        "PINS_VIS",
        "PINS_DECOR",
        "LINES",
        "ARCS",
        "LABELS",
        "PARAMS",
        "DUALPART",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing symbol {expected:?}; got {names:?}",
        );
    }
}

#[test]
fn samples_schlib_pins_etype() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");

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

#[test]
fn samples_schlib_pins_orient() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib
        .get("PINS_ORIENT")
        .expect("symbol PINS_ORIENT not found");
    assert_eq!(symbol.pins.len(), 4, "PINS_ORIENT has 4 pins");

    // One pin per orientation, matched by designator (not index).
    let expected: [(&str, &str, PinOrientation); 4] = [
        ("1", "R", PinOrientation::Right),
        ("2", "U", PinOrientation::Up),
        ("3", "L", PinOrientation::Left),
        ("4", "D", PinOrientation::Down),
    ];
    for (designator, name, orientation) in expected {
        let pin = pin_by_designator(symbol, designator);
        assert_eq!(pin.name, name, "pin {designator} name");
        assert_eq!(pin.orientation, orientation, "pin {designator} orientation");
        assert!(pin.show_name, "pin {designator} show_name");
        assert!(pin.show_designator, "pin {designator} show_designator");
        assert_eq!(pin.owner_part_id, 1, "pin {designator} owner_part_id");
    }
}

#[test]
fn samples_schlib_pins_vis() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("PINS_VIS").expect("symbol PINS_VIS not found");
    assert_eq!(symbol.pins.len(), 4, "PINS_VIS has 4 pins");

    // (designator, name, show_name, show_designator, hidden).
    let expected: [(&str, &str, bool, bool, bool); 4] = [
        ("1", "BOTH", true, true, false),
        ("2", "NONLY", true, false, false),
        ("3", "DONLY", false, true, false),
        ("4", "HIDE", true, true, true),
    ];
    for (designator, name, show_name, show_designator, hidden) in expected {
        let pin = pin_by_designator(symbol, designator);
        assert_eq!(pin.name, name, "pin {designator} name");
        assert_eq!(pin.show_name, show_name, "pin {designator} show_name");
        assert_eq!(
            pin.show_designator, show_designator,
            "pin {designator} show_designator",
        );
        assert_eq!(pin.hidden, hidden, "pin {designator} hidden");
    }
}

#[test]
fn samples_schlib_pins_decor() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("PINS_DECOR").expect("symbol PINS_DECOR not found");
    assert_eq!(symbol.pins.len(), 3, "PINS_DECOR has 3 pins");

    // The clock/inside decoration was deferred, so CLK reads back with no outer
    // edge; DOT and NCLK carry an inversion dot. Inner edge is None for all.
    let expected: [(&str, &str, PinSymbol); 3] = [
        ("1", "DOT", PinSymbol::Dot),
        ("2", "CLK", PinSymbol::None),
        ("3", "NCLK", PinSymbol::Dot),
    ];
    for (designator, name, outer_edge) in expected {
        let pin = pin_by_designator(symbol, designator);
        assert_eq!(pin.name, name, "pin {designator} name");
        assert_eq!(
            pin.symbol_outer_edge, outer_edge,
            "pin {designator} symbol_outer_edge",
        );
        assert_eq!(
            pin.symbol_inner_edge,
            PinSymbol::None,
            "pin {designator} symbol_inner_edge",
        );
    }
}

#[test]
fn samples_schlib_lines() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("LINES").expect("symbol LINES not found");
    assert_eq!(symbol.lines.len(), 3, "LINES has 3 lines");

    // Match each line by its (x1, y1, x2, y2) endpoints (reader units).
    for endpoints in [(0, 0, 10, 0), (0, 0, 0, 10), (0, 0, 10, 10)] {
        let (x1, y1, x2, y2) = endpoints;
        assert!(
            symbol
                .lines
                .iter()
                .any(|l| l.x1 == x1 && l.y1 == y1 && l.x2 == x2 && l.y2 == y2),
            "missing line {endpoints:?}",
        );
    }
}

#[test]
fn samples_schlib_arcs() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("ARCS").expect("symbol ARCS not found");
    assert_eq!(symbol.arcs.len(), 2, "ARCS has 2 arcs");

    // Full circle at the origin.
    let circle = symbol
        .arcs
        .iter()
        .find(|a| a.x == 0 && a.y == 0)
        .expect("full-circle arc at origin not found");
    assert_eq!(circle.radius, 5, "circle radius");
    assert!(approx_eq(circle.start_angle, 0.0), "circle start angle");
    assert!(approx_eq(circle.end_angle, 360.0), "circle end angle");

    // Quarter arc centred below the origin.
    let quarter = symbol
        .arcs
        .iter()
        .find(|a| a.x == 0 && a.y == -20)
        .expect("quarter arc at (0,-20) not found");
    assert_eq!(quarter.radius, 5, "quarter-arc radius");
    assert!(
        approx_eq(quarter.start_angle, 0.0),
        "quarter-arc start angle"
    );
    assert!(approx_eq(quarter.end_angle, 90.0), "quarter-arc end angle");
}

#[test]
fn samples_schlib_labels() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("LABELS").expect("symbol LABELS not found");
    assert_eq!(symbol.labels.len(), 3, "LABELS has 3 labels");

    let by_text = |text: &str| -> &Label {
        symbol
            .labels
            .iter()
            .find(|l| l.text == text)
            .unwrap_or_else(|| panic!("label {text:?} not found"))
    };

    // Match by text; assert the authored justification (rotation is not part of
    // the contract here and is left unchecked).
    assert_eq!(
        by_text("LBL_BL").justification,
        TextJustification::BottomLeft,
        "LBL_BL justification",
    );
    assert_eq!(
        by_text("LBL_TR").justification,
        TextJustification::TopRight,
        "LBL_TR justification",
    );
    assert_eq!(
        by_text("LBL_ROT90").justification,
        TextJustification::BottomLeft,
        "LBL_ROT90 justification",
    );
}

#[test]
fn samples_schlib_params() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("PARAMS").expect("symbol PARAMS not found");

    // Every symbol also carries an Altium-default `Comment` = "*", so we locate
    // the authored parameters by (name, value) rather than asserting a count.
    let find = |name: &str, value: &str| -> &Parameter {
        symbol
            .parameters
            .iter()
            .find(|p| p.name == name && p.value == value)
            .unwrap_or_else(|| panic!("parameter {name:?} = {value:?} not found"))
    };

    let value = find("Value", "10k");
    assert!(!value.hidden, "authored Value parameter is visible");

    let comment = find("Comment", "100nF");
    assert!(comment.hidden, "authored Comment parameter is hidden");
}

#[test]
fn samples_schlib_dualpart() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("DUALPART").expect("symbol DUALPART not found");

    assert_eq!(symbol.part_count, 2, "DUALPART is a two-part symbol");
    assert_eq!(symbol.pins.len(), 4, "DUALPART has 4 pins");

    // Pins split across the two parts by owner_part_id, matched by designator.
    let expected: [(&str, &str, i32); 4] = [
        ("1", "INA", 1),
        ("2", "OUTA", 1),
        ("3", "INB", 2),
        ("4", "OUTB", 2),
    ];
    for (designator, name, owner_part_id) in expected {
        let pin = pin_by_designator(symbol, designator);
        assert_eq!(pin.name, name, "pin {designator} name");
        assert_eq!(
            pin.owner_part_id, owner_part_id,
            "pin {designator} owner_part_id",
        );
    }
}
