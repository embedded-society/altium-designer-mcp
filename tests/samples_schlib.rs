//! Sample-library tests for `SchLib`.
//!
//! Unlike the round-trip tests in `file_io_roundtrip.rs` (which write a library
//! with our own writer and read it back), these tests open a *real*,
//! Altium-authored sample library from `scripts/samples/` with our reader and
//! assert the parsed values against the file's authored intent. This is the
//! reference pattern for the rest of the `samples_*` test files.

use altium_designer_mcp::altium::schlib::{
    Ellipse, Label, Parameter, Pin, PinElectricalType, PinOrientation, PinSymbol, Polygon,
    Rectangle, RoundRect, SchLib, Symbol, TextJustification,
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

    // The library now contains fifteen Altium-authored symbols (one per
    // primitive family plus a boundary-case `EDGE` symbol).
    assert_eq!(lib.len(), 15, "expected exactly fifteen symbols");

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
        "RECTS",
        "ELLIPSES",
        "POLYLINES",
        "ROUNDRECTS",
        "POLYGONS",
        "EDGE",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing symbol {expected:?}; got {names:?}",
        );
    }
}

#[test]
fn samples_schlib_edge() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");

    let symbol = lib.get("EDGE").expect("symbol EDGE not found");
    assert_eq!(symbol.name, "EDGE");
    assert_eq!(symbol.pins.len(), 3, "EDGE has 3 pins");

    // Boundary-case pins, matched by designator. Pins 1 and 2 push the
    // coordinate extremes (large and negative positions); pin 3 is the headline
    // case — a 35-character name that must survive the round-trip intact.
    let pin1 = pin_by_designator(symbol, "1");
    assert_eq!(pin1.name, "BIG", "pin 1 name");
    assert_eq!(pin1.x, 50, "pin 1 x");
    assert_eq!(pin1.y, 30, "pin 1 y");

    let pin2 = pin_by_designator(symbol, "2");
    assert_eq!(pin2.name, "NEG", "pin 2 name");
    assert_eq!(pin2.x, -50, "pin 2 x");
    assert_eq!(pin2.y, -30, "pin 2 y");

    let pin3 = pin_by_designator(symbol, "3");
    assert_eq!(
        pin3.name, "VERY_LONG_PIN_NAME_0123456789ABCDEF",
        "pin 3 long name survives intact",
    );
    assert_eq!(pin3.x, 0, "pin 3 x");
    assert_eq!(pin3.y, 20, "pin 3 y");
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
    assert_eq!(symbol.pins.len(), 4, "PINS_DECOR has 4 pins");

    // One pin per IEEE decoration slot: each sets exactly one slot, the other
    // three stay None. Confirms all four DelphiScript slot properties round-trip
    // (Symbol_InnerEdge / Symbol_OuterEdge / Symbol_Inner / Symbol_Outer).
    let expected: [(&str, &str, PinSymbol, PinSymbol, PinSymbol, PinSymbol); 4] = [
        (
            "1",
            "IECLK",
            PinSymbol::Clock,
            PinSymbol::None,
            PinSymbol::None,
            PinSymbol::None,
        ),
        (
            "2",
            "OEDOT",
            PinSymbol::None,
            PinSymbol::Dot,
            PinSymbol::None,
            PinSymbol::None,
        ),
        (
            "3",
            "INCLK",
            PinSymbol::None,
            PinSymbol::None,
            PinSymbol::Clock,
            PinSymbol::None,
        ),
        (
            "4",
            "OUTDOT",
            PinSymbol::None,
            PinSymbol::None,
            PinSymbol::None,
            PinSymbol::Dot,
        ),
    ];
    for (designator, name, inner_edge, outer_edge, inside, outside) in expected {
        let pin = pin_by_designator(symbol, designator);
        assert_eq!(pin.name, name, "pin {designator} name");
        assert_eq!(
            pin.symbol_inner_edge, inner_edge,
            "pin {designator} symbol_inner_edge"
        );
        assert_eq!(
            pin.symbol_outer_edge, outer_edge,
            "pin {designator} symbol_outer_edge"
        );
        assert_eq!(pin.symbol_inside, inside, "pin {designator} symbol_inside");
        assert_eq!(
            pin.symbol_outside, outside,
            "pin {designator} symbol_outside"
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

#[test]
fn samples_schlib_rects() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("RECTS").expect("symbol RECTS not found");
    assert_eq!(symbol.rectangles.len(), 2, "RECTS has 2 rectangles");

    // Match by left edge (x1); both share line_color 0 / fill_color 65535.
    let by_x1 = |x1: i32| -> &Rectangle {
        symbol
            .rectangles
            .iter()
            .find(|r| r.x1 == x1)
            .unwrap_or_else(|| panic!("rectangle with x1 = {x1} not found"))
    };

    let filled = by_x1(-10);
    assert_eq!(filled.y1, 0, "filled rect y1");
    assert_eq!(filled.x2, 10, "filled rect x2");
    assert_eq!(filled.y2, 10, "filled rect y2");
    assert!(filled.filled, "filled rect is filled");
    assert_eq!(filled.fill_color, 65535, "filled rect fill_color");
    assert_eq!(filled.line_color, 0, "filled rect line_color");

    let unfilled = by_x1(15);
    assert_eq!(unfilled.y1, 0, "unfilled rect y1");
    assert_eq!(unfilled.x2, 35, "unfilled rect x2");
    assert_eq!(unfilled.y2, 10, "unfilled rect y2");
    assert!(!unfilled.filled, "unfilled rect is not filled");
    assert_eq!(unfilled.fill_color, 65535, "unfilled rect fill_color");
    assert_eq!(unfilled.line_color, 0, "unfilled rect line_color");
}

#[test]
fn samples_schlib_ellipses() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("ELLIPSES").expect("symbol ELLIPSES not found");
    assert_eq!(symbol.ellipses.len(), 2, "ELLIPSES has 2 ellipses");

    // Match by horizontal radius (radius_x), which is unique here.
    let by_radius_x = |radius_x: i32| -> &Ellipse {
        symbol
            .ellipses
            .iter()
            .find(|e| e.radius_x == radius_x)
            .unwrap_or_else(|| panic!("ellipse with radius_x = {radius_x} not found"))
    };

    let circle = by_radius_x(5);
    assert_eq!(circle.x, 0, "circle x");
    assert_eq!(circle.y, 0, "circle y");
    assert_eq!(circle.radius_y, 5, "circle radius_y");
    assert!(circle.filled, "circle is filled");

    let ellipse = by_radius_x(8);
    assert_eq!(ellipse.x, 20, "ellipse x");
    assert_eq!(ellipse.y, 0, "ellipse y");
    assert_eq!(ellipse.radius_y, 4, "ellipse radius_y");
    assert!(!ellipse.filled, "ellipse is not filled");
}

#[test]
fn samples_schlib_polylines() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("POLYLINES").expect("symbol POLYLINES not found");
    assert_eq!(symbol.polylines.len(), 1, "POLYLINES has 1 polyline");

    let polyline = &symbol.polylines[0];
    assert_eq!(
        polyline.points,
        vec![(0, 0), (10, 5), (0, 10)],
        "polyline points",
    );
}

#[test]
fn samples_schlib_roundrects() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("ROUNDRECTS").expect("symbol ROUNDRECTS not found");
    assert_eq!(symbol.round_rects.len(), 1, "ROUNDRECTS has 1 rounded rect");

    let rr: &RoundRect = &symbol.round_rects[0];
    assert_eq!(rr.x1, -10, "round rect x1");
    assert_eq!(rr.y1, 0, "round rect y1");
    assert_eq!(rr.x2, 10, "round rect x2");
    assert_eq!(rr.y2, 10, "round rect y2");
    assert_eq!(rr.corner_x_radius, 2, "round rect corner_x_radius");
    assert_eq!(rr.corner_y_radius, 2, "round rect corner_y_radius");
    assert!(rr.filled, "round rect is filled");
}

#[test]
fn samples_schlib_polygons() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("POLYGONS").expect("symbol POLYGONS not found");
    assert_eq!(symbol.polygons.len(), 2, "POLYGONS has 2 polygons");

    // Both are 4-vertex boxes; match each by its first vertex x (unique here).
    let by_first_x = |x: i32| -> &Polygon {
        symbol
            .polygons
            .iter()
            .find(|p| p.points.first().is_some_and(|&(px, _)| px == x))
            .unwrap_or_else(|| panic!("polygon with first vertex x = {x} not found"))
    };

    let left = by_first_x(-10);
    assert_eq!(
        left.points,
        vec![(-10, 0), (10, 0), (10, 10), (-10, 10)],
        "left polygon points",
    );
    assert!(left.filled, "left polygon is filled");

    let right = by_first_x(15);
    assert_eq!(
        right.points,
        vec![(15, 0), (35, 0), (35, 10), (15, 10)],
        "right polygon points",
    );
    assert!(right.filled, "right polygon is filled");
}
