//! Sample-library tests for `SchLib`.
//!
//! Unlike the round-trip tests in `file_io_roundtrip.rs` (which write a library
//! with our own writer and read it back), these tests open a *real*,
//! Altium-authored sample library from `scripts/samples/` with our reader and
//! assert the parsed values against the file's authored intent. This is the
//! reference pattern for the rest of the `samples_*` test files.

use altium_designer_mcp::altium::schlib::{
    Ellipse, Label, Parameter, Pin, PinElectricalType, PinOrientation, PinSymbol, Polygon,
    Rectangle, RoundRect, SchLib, SchPrimitiveKind, ShapeDisplayFlags, Symbol, TextJustification,
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

    // Fifteen per-primitive-family symbols, the coverage-enrichment symbols, and
    // fifty-three i18n symbols — one per writing system (see AddI18nSymbol in
    // GenerateSamples.pas).
    // Fifteen per-primitive-family symbols plus the coverage-enrichment symbols
    // (SHAPESTYLE, SHAPESTYLE2, SHAPECOLOR, LOCKFLAGS, JUSTIFY, FRACPINS,
    // BEZIERSYM, PIESYM, IMAGESYM, TEXTFRAMESYM, EMBIMGSYM, SWAPPIN, FRACSHAPES,
    // DISPMODE, batch 5's ELLARC, IMPLCHAIN, FRACSHAPES2 and batch 6's IEEESYM)
    // added to GenerateSamples.pas and regenerated on-site.
    assert_eq!(lib.len(), 88, "expected exactly eighty-eight symbols");

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
        "SHAPESTYLE",
        "LOCKFLAGS",
        "JUSTIFY",
        "FRACPINS",
        "BEZIERSYM",
        "PIESYM",
        "IMAGESYM",
        "TEXTFRAMESYM",
        "EMBIMGSYM",
        "SWAPPIN",
        "FRACSHAPES",
        "DISPMODE",
        "SHAPECOLOR",
        "SHAPESTYLE2",
        "ELLARC",
        "IMPLCHAIN",
        "FRACSHAPES2",
        "IEEESYM",
        "LOCKFLAGS2",
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
        // PR-R3 pin auxiliary data. The Altium-authored golden pins are on-grid
        // with a default symbol line width and display mode, so all three read
        // back at their defaults — the byte-identity anchor for the aux streams
        // (a from-scratch default pin therefore writes no PinFrac /
        // PinSymbolLineWidth stream, keeping the storage identical).
        assert_eq!(
            pin.owner_part_display_mode, 0,
            "pin[{i}] ({designator}) OwnerPartDisplayMode reads the golden byte (0)"
        );
        assert_eq!(
            pin.symbol_line_width, 0,
            "pin[{i}] ({designator}) has default symbol line width"
        );
        assert_eq!(
            pin.frac, None,
            "pin[{i}] ({designator}) is on-grid (no PinFrac remainder)"
        );
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

    // Match each line by its (x1, y1, x2, y2) endpoints (reader units). Coords are
    // f64; the integer-grid sample reads back as exact whole values.
    for endpoints in [
        (0.0, 0.0, 10.0, 0.0),
        (0.0, 0.0, 0.0, 10.0),
        (0.0, 0.0, 10.0, 10.0),
    ] {
        let (x1, y1, x2, y2) = endpoints;
        assert!(
            symbol.lines.iter().any(|l| {
                (l.x1 - x1).abs() < 1e-9
                    && (l.y1 - y1).abs() < 1e-9
                    && (l.x2 - x2).abs() < 1e-9
                    && (l.y2 - y2).abs() < 1e-9
            }),
            "missing line {endpoints:?}",
        );
    }

    // The golden authors the designator record at Location.X=-5|Location.Y=5
    // with a stable UniqueID; position and identity must read back rather than
    // being re-hardcoded or regenerated on write.
    assert!(
        approx_eq(symbol.designator_x, -5.0) && approx_eq(symbol.designator_y, 5.0),
        "golden designator position must read back as (-5, 5), got ({}, {})",
        symbol.designator_x,
        symbol.designator_y
    );
    let uid = symbol
        .designator_unique_id
        .as_deref()
        .expect("golden designator UniqueID must be preserved on read");
    assert_eq!(uid.len(), 8, "designator UniqueID is an 8-char Altium id");
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
        .find(|a| approx_eq(a.x, 0.0) && approx_eq(a.y, 0.0))
        .expect("full-circle arc at origin not found");
    assert!(approx_eq(circle.radius, 5.0), "circle radius");
    assert!(approx_eq(circle.start_angle, 0.0), "circle start angle");
    assert!(approx_eq(circle.end_angle, 360.0), "circle end angle");

    // Quarter arc centred below the origin.
    let quarter = symbol
        .arcs
        .iter()
        .find(|a| approx_eq(a.x, 0.0) && approx_eq(a.y, -20.0))
        .expect("quarter arc at (0,-20) not found");
    assert!(approx_eq(quarter.radius, 5.0), "quarter-arc radius");
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
    // The golden's parameters carry neither SHOWNAME nor HIDENAME, so both
    // name-visibility toggles read back as their omit-when-default `false`.
    // Visibility is driven solely by IsHidden.
    assert!(!value.show_name, "golden Value parameter has no ShowName");
    assert!(!value.hide_name, "golden Value parameter has no HideName");
    assert_eq!(value.orientation, 0, "golden Value parameter Orientation=0");
    assert!(
        value.description.is_empty(),
        "golden Value parameter has no Description"
    );
    assert!(
        !value.is_configurable,
        "golden Value parameter is not configurable"
    );

    let comment = find("Comment", "100nF");
    assert!(comment.hidden, "authored Comment parameter is hidden");
    assert!(
        !comment.show_name,
        "golden Comment parameter has no ShowName"
    );
    assert!(
        !comment.hide_name,
        "golden Comment parameter has no HideName"
    );
}

#[test]
fn samples_schlib_no_utf8_key_for_win1252_golden() {
    // Promotion must be reserved for values that need it: a Windows-1252
    // symbol must NOT gain a `%UTF8%` key, so the common case stays
    // byte-identical and the readability oracle sees no change. The Cyrillic
    // symbol is excluded — it is the one that legitimately requires promotion.
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    for symbol in lib.iter().filter(|s| s.name.is_ascii()) {
        let data = altium_designer_mcp::altium::schlib::writer::encode_data_stream(symbol)
            .expect("encode");
        assert!(
            !data.windows(6).any(|w| w == b"%UTF8%"),
            "symbol {:?}: golden (all Windows-1252) must not gain a %UTF8% key",
            symbol.name
        );
    }

    // And the specific text-bearing symbols round-trip their values unchanged.
    let labels = lib.get("LABELS").expect("LABELS symbol");
    assert!(
        labels.labels.iter().all(|l| l.text.is_ascii()),
        "golden labels are ASCII, so their values must read back verbatim"
    );
    let params = lib.get("PARAMS").expect("PARAMS symbol");
    assert!(
        params.parameters.iter().any(|p| p.value == "10k"),
        "golden Value parameter reads back as the plain Windows-1252 value"
    );
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
    let by_x1 = |x1: f64| -> &Rectangle {
        symbol
            .rectangles
            .iter()
            .find(|r| approx_eq(r.x1, x1))
            .unwrap_or_else(|| panic!("rectangle with x1 = {x1} not found"))
    };

    let filled = by_x1(-10.0);
    assert_eq!(
        (filled.y1, filled.x2, filled.y2),
        (0.0, 10.0, 10.0),
        "filled rect geometry"
    );
    assert!(filled.filled, "filled rect is filled");
    assert_eq!(filled.fill_color, 65535, "filled rect fill_color");
    assert_eq!(filled.line_color, 0, "filled rect line_color");

    let unfilled = by_x1(15.0);
    assert_eq!(
        (unfilled.y1, unfilled.x2, unfilled.y2),
        (0.0, 35.0, 10.0),
        "unfilled rect geometry"
    );
    assert!(!unfilled.filled, "unfilled rect is not filled");
    assert_eq!(unfilled.fill_color, 65535, "unfilled rect fill_color");
    assert_eq!(unfilled.line_color, 0, "unfilled rect line_color");
}

#[test]
fn samples_schlib_display_flags_default_on_golden_shapes() {
    // The Altium-authored golden library carries no GraphicallyLocked / Disabled
    // / Dimmed / OwnerPartDisplayMode on its graphic shapes, so the reader must
    // decode each as its default (false / 0) — the read half of the
    // omit-when-default contract. Exercises one shape per graphic family.
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let def = ShapeDisplayFlags::default();

    for r in &lib.get("RECTS").unwrap().rectangles {
        assert_eq!(r.display_flags, def, "golden rectangle flags default");
    }
    for r in &lib.get("ROUNDRECTS").unwrap().round_rects {
        assert_eq!(r.display_flags, def, "golden round_rect flags default");
    }
    for e in &lib.get("ELLIPSES").unwrap().ellipses {
        assert_eq!(e.display_flags, def, "golden ellipse flags default");
    }
    for l in &lib.get("LINES").unwrap().lines {
        assert_eq!(l.display_flags, def, "golden line flags default");
    }
    for p in &lib.get("POLYLINES").unwrap().polylines {
        assert_eq!(p.display_flags, def, "golden polyline flags default");
    }
    for p in &lib.get("POLYGONS").unwrap().polygons {
        assert_eq!(p.display_flags, def, "golden polygon flags default");
    }
    for a in &lib.get("ARCS").unwrap().arcs {
        assert_eq!(a.display_flags, def, "golden arc flags default");
    }
    for l in &lib.get("LABELS").unwrap().labels {
        assert_eq!(l.display_flags, def, "golden label flags default");
    }
    for p in &lib.get("PARAMS").unwrap().parameters {
        assert_eq!(p.display_flags, def, "golden parameter flags default");
    }
}

#[test]
fn samples_schlib_ellipses() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("ELLIPSES").expect("symbol ELLIPSES not found");
    assert_eq!(symbol.ellipses.len(), 2, "ELLIPSES has 2 ellipses");

    // Match by horizontal radius (radius_x), which is unique here.
    let by_radius_x = |radius_x: f64| -> &Ellipse {
        symbol
            .ellipses
            .iter()
            .find(|e| approx_eq(e.radius_x, radius_x))
            .unwrap_or_else(|| panic!("ellipse with radius_x = {radius_x} not found"))
    };

    let circle = by_radius_x(5.0);
    assert_eq!(
        (circle.x, circle.y, circle.radius_y),
        (0.0, 0.0, 5.0),
        "circle geometry"
    );
    assert!(circle.filled, "circle is filled");

    let ellipse = by_radius_x(8.0);
    assert_eq!(
        (ellipse.x, ellipse.y, ellipse.radius_y),
        (20.0, 0.0, 4.0),
        "ellipse geometry"
    );
    assert!(!ellipse.filled, "ellipse is not filled");

    // The golden tags every ellipse IsNotAccesible=T; the field must read back.
    for e in &symbol.ellipses {
        assert!(
            e.is_not_accessible,
            "golden ellipse must read IsNotAccesible=T"
        );
    }
}

#[test]
fn samples_schlib_polylines() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("POLYLINES").expect("symbol POLYLINES not found");
    assert_eq!(symbol.polylines.len(), 1, "POLYLINES has 1 polyline");

    let polyline = &symbol.polylines[0];
    assert_eq!(
        polyline.points,
        vec![(0.0, 0.0), (10.0, 5.0), (0.0, 10.0)],
        "polyline points",
    );

    // The golden tags every polyline IsNotAccesible=T; the field must read back.
    assert!(
        polyline.is_not_accessible,
        "golden polyline must read IsNotAccesible=T"
    );
}

#[test]
fn samples_schlib_roundrects() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("ROUNDRECTS").expect("symbol ROUNDRECTS not found");
    assert_eq!(symbol.round_rects.len(), 1, "ROUNDRECTS has 1 rounded rect");

    let rr: &RoundRect = &symbol.round_rects[0];
    assert_eq!(
        (rr.x1, rr.y1, rr.x2, rr.y2),
        (-10.0, 0.0, 10.0, 10.0),
        "round rect geometry"
    );
    assert_eq!(
        (rr.corner_x_radius, rr.corner_y_radius),
        (2.0, 2.0),
        "round rect corner radii"
    );
    assert!(rr.filled, "round rect is filled");
}

#[test]
fn samples_schlib_polygons() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let symbol = lib.get("POLYGONS").expect("symbol POLYGONS not found");
    assert_eq!(symbol.polygons.len(), 2, "POLYGONS has 2 polygons");

    // Both are 4-vertex boxes; match each by its first vertex x (unique here).
    let by_first_x = |x: f64| -> &Polygon {
        symbol
            .polygons
            .iter()
            .find(|p| p.points.first().is_some_and(|&(px, _)| approx_eq(px, x)))
            .unwrap_or_else(|| panic!("polygon with first vertex x = {x} not found"))
    };

    let left = by_first_x(-10.0);
    assert_eq!(
        left.points,
        vec![(-10.0, 0.0), (10.0, 0.0), (10.0, 10.0), (-10.0, 10.0)],
        "left polygon points",
    );
    assert!(left.filled, "left polygon is filled");

    let right = by_first_x(15.0);
    assert_eq!(
        right.points,
        vec![(15.0, 0.0), (35.0, 0.0), (35.0, 10.0), (15.0, 10.0)],
        "right polygon points",
    );
    assert!(right.filled, "right polygon is filled");
}

// ---------------------------------------------------------------------------
// Coverage-enrichment tests (scripts/samples/COVERAGE.md).
//
// These assert the NON-default property values authored by the enrichment block
// in GenerateSamples.pas, read from the real Altium-regenerated fixture. This is
// the whole point of the enrichment: values a self-round-trip cannot vouch
// for (line style, transparency, non-default justification,
// off-grid PinFrac coordinates) verified against a genuine Altium file.
// ---------------------------------------------------------------------------

#[test]
fn samples_schlib_shapestyle() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("SHAPESTYLE").expect("SHAPESTYLE symbol not found");

    // Two lines, one dashed (line_style 1) and one dotted (line_style 2).
    assert_eq!(sym.lines.len(), 2, "SHAPESTYLE has two lines");
    let styles: Vec<u8> = sym.lines.iter().map(|l| l.line_style).collect();
    assert!(
        styles.contains(&1) && styles.contains(&2),
        "SHAPESTYLE must carry a dashed (1) and a dotted (2) line, got {styles:?}"
    );

    // Three rectangles: one solid-opaque, one transparent, one dashed.
    assert_eq!(sym.rectangles.len(), 3, "SHAPESTYLE has three rectangles");
    assert_eq!(
        sym.rectangles.iter().filter(|r| r.transparent).count(),
        1,
        "exactly one SHAPESTYLE rectangle is transparent"
    );

    // The dashed rectangle: a rectangle's line style persists as `LineStyleExt`
    // (between Corner.Y and LineWidth), unlike the round-rect, which accepts
    // the property and drops it. This is the first real-Altium golden for a
    // styled rectangle — the key placement is byte-pinned by the fidelity
    // replay suite.
    let dashed = sym
        .rectangles
        .iter()
        .find(|r| r.line_style != 0)
        .expect("SHAPESTYLE has a styled rectangle");
    assert_eq!(dashed.line_style, 1, "the styled rectangle is dashed");
    assert!(
        dashed.raw_params.iter().any(|(k, _)| k == "LineStyleExt"),
        "the dashed rectangle's record carries LineStyleExt, not LineStyle"
    );
    assert!(
        !dashed.raw_params.iter().any(|(k, _)| k == "LineStyle"),
        "Altium omits the LineStyle key on a rectangle"
    );
    assert!(
        approx_eq(dashed.x1, 15.0)
            && approx_eq(dashed.y1, -10.0)
            && approx_eq(dashed.x2, 25.0)
            && approx_eq(dashed.y2, -5.0),
        "dashed rectangle corners, got ({}, {})..({}, {})",
        dashed.x1,
        dashed.y1,
        dashed.x2,
        dashed.y2
    );

    // One transparent polygon (ISch_Polygon.Transparent round-trips from Altium).
    assert_eq!(sym.polygons.len(), 1, "SHAPESTYLE has one polygon");
    assert!(sym.polygons[0].transparent, "the polygon is transparent");

    // One transparent ellipse (ISch_Ellipse.Transparent round-trips). Note:
    // RoundRectangle.Transparent is deliberately NOT authored — Altium does not
    // persist it on a library round-rect (reads back false), so it is not testable.
    assert_eq!(sym.ellipses.len(), 1, "SHAPESTYLE has one ellipse");
    assert!(sym.ellipses[0].transparent, "the ellipse is transparent");
}

#[test]
fn samples_schlib_shape_colours() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("SHAPECOLOR").expect("SHAPECOLOR symbol not found");

    // Every other symbol authors Color := $000000, which is Altium's default and
    // is therefore omitted from the record — so without this symbol no shape
    // parser's colour arm is ever exercised against a real file. SHAPECOLOR
    // carries one of each shape in a DISTINCT colour, so a mismatched read
    // cannot pass by picking up a neighbour's value.
    assert_eq!(sym.lines.len(), 1, "SHAPECOLOR has one line");
    assert_eq!(sym.lines[0].color, 255, "line is red ($0000FF)");

    assert_eq!(sym.rectangles.len(), 1, "SHAPECOLOR has one rectangle");
    assert_eq!(
        sym.rectangles[0].line_color, 65280,
        "rectangle border is green"
    );
    assert_eq!(sym.rectangles[0].fill_color, 16_776_960, "rectangle fill");
    assert!(sym.rectangles[0].filled, "rectangle is solid");

    assert_eq!(sym.round_rects.len(), 1, "SHAPECOLOR has one round rect");
    assert_eq!(
        sym.round_rects[0].line_color, 16_711_680,
        "round rect border is blue"
    );

    assert_eq!(sym.arcs.len(), 1, "SHAPECOLOR has one arc");
    assert_eq!(sym.arcs[0].color, 65535, "arc is yellow");
    // A non-zero StartAngle: every other arc in the library starts at 0, which
    // Altium omits, leaving the start-angle read path uncovered.
    assert!(approx_eq(sym.arcs[0].start_angle, 45.0), "arc start angle");
    assert!(approx_eq(sym.arcs[0].end_angle, 315.0), "arc end angle");

    assert_eq!(sym.ellipses.len(), 1, "SHAPECOLOR has one ellipse");
    assert_eq!(
        sym.ellipses[0].line_color, 16_711_935,
        "ellipse border is magenta"
    );

    assert_eq!(sym.polylines.len(), 1, "SHAPECOLOR has one polyline");
    assert_eq!(sym.polylines[0].color, 8_421_376, "polyline is teal");

    assert_eq!(sym.polygons.len(), 1, "SHAPECOLOR has one polygon");
    assert_eq!(
        sym.polygons[0].line_color, 128,
        "polygon border is dark red"
    );

    assert_eq!(sym.pies.len(), 1, "SHAPECOLOR has one pie");
    assert_eq!(sym.pies[0].line_color, 32896, "pie border is olive");
    assert_eq!(sym.pies[0].fill_color, 42495, "pie fill");

    assert_eq!(sym.beziers.len(), 1, "SHAPECOLOR has one bezier");
    assert_eq!(sym.beziers[0].color, 8_404_992, "bezier colour");
    assert_eq!(sym.beziers[0].line_width, 2, "bezier is eMedium (2)");

    let label = sym
        .labels
        .iter()
        .find(|l| l.text == "COLOURED")
        .expect("SHAPECOLOR label not found");
    assert_eq!(label.color, 4_227_327, "label is orange");
}

#[test]
fn samples_schlib_polyline_styling() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib
        .get("SHAPESTYLE2")
        .expect("SHAPESTYLE2 symbol not found");

    // A polyline carries four styling properties no other primitive family has,
    // and AD24 persists all four. Authored as dashed with an open arrow at the
    // start, a solid arrow at the end, and the large end-shape size.
    // Two polylines: this styled one, and an unstyled control that documents the
    // fill properties AD24 refuses to persist.
    assert_eq!(sym.polylines.len(), 2, "SHAPESTYLE2 has two polylines");
    let pl = sym
        .polylines
        .iter()
        .find(|p| p.start_line_shape != 0)
        .expect("the styled polyline");
    assert_eq!(pl.line_style, 1, "dashed");
    assert_eq!(pl.start_line_shape, 1, "start is an arrow");
    assert_eq!(pl.end_line_shape, 2, "end is a solid arrow");
    assert_eq!(pl.line_shape_size, 3, "end shapes are eLarge");
}

#[test]
fn samples_schlib_label_and_parameter_display_props() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib
        .get("SHAPESTYLE2")
        .expect("SHAPESTYLE2 symbol not found");

    // A mirrored label. AD24 writes `IsMirrored=T` before `UniqueID` here, and
    // *after* it on the parameter below — the orders genuinely differ.
    let label = sym
        .labels
        .iter()
        .find(|l| l.text == "MIRRORED")
        .expect("SHAPESTYLE2 label not found");
    assert!(label.is_mirrored, "the label is mirrored");

    // The parameter display properties, none of which any other symbol sets.
    let param = sym
        .parameters
        .iter()
        .find(|p| p.name == "Rating")
        .expect("SHAPESTYLE2 Rating parameter not found");
    assert_eq!(param.value, "10V", "parameter value");
    assert!(param.show_name, "the name is shown beside the value");
    assert_eq!(param.read_only_state, 1, "parameter is read-only");
    assert!(param.is_mirrored, "the parameter text is mirrored");

    // Authored `LineStyle := eLineStyleDotted` on the round rect, which AD24
    // accepts and then does not persist — the saved record carries no
    // LineStyle key at all, so it must read back as the 0 default.
    assert_eq!(sym.round_rects.len(), 1, "SHAPESTYLE2 has one round rect");
    assert_eq!(
        sym.round_rects[0].line_style, 0,
        "AD24 does not persist LineStyle on a library round rect"
    );
}

#[test]
fn samples_schlib_polyline_and_frame_fill_are_not_persisted() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib
        .get("SHAPESTYLE2")
        .expect("SHAPESTYLE2 symbol not found");

    // Both were authored with a fill and transparency and AD24 accepted both
    // without complaint, then wrote neither. Asserting the defaults keeps the
    // negative honest: if a later AD version starts writing them, this fails
    // rather than quietly passing.
    let plain = sym
        .polylines
        .iter()
        .find(|p| p.line_style == 0)
        .expect("the unstyled polyline");
    assert!(
        !plain.transparent,
        "AD24 does not persist polyline Transparent"
    );

    assert_eq!(sym.text_frames.len(), 1, "SHAPESTYLE2 has one text frame");
    let frame = &sym.text_frames[0];
    assert!(
        !frame.transparent,
        "AD24 does not persist text-frame Transparent"
    );
    // A whole-mil margin: every other frame's is sub-mil, so the record carries
    // only TextMargin_Frac and the integer key goes unread.
    assert!(
        approx_eq(frame.text_margin, 3.0),
        "whole-mil text margin, got {}",
        frame.text_margin
    );
}

#[test]
fn samples_schlib_parameter_type() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib
        .get("SHAPESTYLE2")
        .expect("SHAPESTYLE2 symbol not found");

    // eParameterType_Integer. Every other parameter in the library leaves the
    // type at its default, which Altium omits.
    let param = sym
        .parameters
        .iter()
        .find(|p| p.name == "Rating")
        .expect("Rating parameter not found");
    assert_eq!(param.param_type, 2, "eParameterType_Integer");
}

#[test]
fn samples_schlib_writing_systems_decode() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");

    // Altium writes text outside Windows-1252 as its raw UTF-8 bytes inside a
    // record that is otherwise Windows-1252. Decoding those as Windows-1252
    // yields mojibake: the pin name of the Han symbol read back as `ç”µé˜»`
    // rather than `电阻` until the binary string reader learned to prefer UTF-8.
    //
    // Expectations are spelled out here rather than derived from the file. A
    // check that compares the file against itself passes even when every field
    // is mojibake, which is exactly the mistake this table avoids.
    let cases: &[(&str, &str)] = &[
        ("Resistor_LA", "Resistor"),     // ASCII control
        ("Résistance_L1", "Résistance"), // Latin-1 supplement
        ("Điện trở_VI", "Điện trở"),     // Latin, precomposed diacritics
        ("Резистор_RU", "Резистор"),     // Cyrillic
        ("Αντίσταση_EL", "Αντίσταση"),   // Greek
        ("电阻_ZH", "电阻"),             // Han
        ("저항기_KO", "저항기"),         // Hangul
        ("مقاومة_AR", "مقاومة"),         // Arabic, right to left
        ("נגד_HE", "נגד"),               // Hebrew, right to left
        ("प्रतिरोधक_HI", "प्रतिरोधक"),     // Devanagari, combining marks
        ("ตัวต้านทาน_TH", "ตัวต้านทาน"),     // Thai, no word spacing
        ("រេស៊ីស្ទ័រ_KM", "រេស៊ីស្ទ័រ"),           // Khmer, stacked consonants
        ("𞤀𞤣𞤤𞤢𞤥_AD", "𞤀𞤣𞤤𞤢𞤥"),           // Adlam, beyond the BMP
    ];

    for (symbol, word) in cases {
        let sym = lib
            .get(symbol)
            .unwrap_or_else(|| panic!("symbol {symbol:?} not found"));

        // The pin name is a length-prefixed string in the binary pin record —
        // a different decode path from the parameter blocks below.
        let pin = sym.pins.first().expect("one pin");
        assert_eq!(&pin.name.as_str(), word, "{symbol}: pin name");

        let label = sym.labels.first().expect("one label");
        assert_eq!(&label.text.as_str(), word, "{symbol}: label text");

        let param = sym
            .parameters
            .iter()
            .find(|p| p.name == "Value")
            .unwrap_or_else(|| panic!("{symbol}: no Value parameter"));
        assert_eq!(&param.value.as_str(), word, "{symbol}: parameter value");
    }
}

#[test]
fn samples_schlib_lockflags() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("LOCKFLAGS").expect("LOCKFLAGS symbol not found");

    // The rectangle was authored with GraphicallyLocked := True (a verified
    // ISch_GraphicalObject flag); it round-trips from the real Altium file.
    assert_eq!(sym.rectangles.len(), 1, "LOCKFLAGS has one rectangle");
    assert!(
        sym.rectangles[0].display_flags.graphically_locked,
        "the LOCKFLAGS rectangle must be graphically locked"
    );

    // A pin stores the same flag as bit 0x40 of its binary record — golden
    // evidence for the pin-record byte, which was self-round-trip only until
    // this fixture.
    assert_eq!(sym.pins.len(), 1, "LOCKFLAGS has one pin");
    assert_eq!(sym.pins[0].designator, "1");
    assert_eq!(sym.pins[0].name, "LK");
    assert!(
        sym.pins[0].graphically_locked,
        "the LOCKFLAGS pin must be graphically locked"
    );
}

#[test]
fn samples_schlib_locked_shapes() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("LOCKFLAGS2").expect("LOCKFLAGS2 symbol not found");

    // GraphicallyLocked lives on ISch_GraphicalObject, but whether AD24 WRITES an
    // inherited flag varies by record — Disabled and Dimmed are dropped from a
    // rectangle, a polyline's fill is dropped, a text frame's transparency is
    // dropped. So each shape type is authored and asserted rather than assumed
    // from the rectangle that LOCKFLAGS already covers.
    assert!(sym.lines[0].display_flags.graphically_locked, "line");
    assert!(sym.arcs[0].display_flags.graphically_locked, "arc");
    assert!(sym.ellipses[0].display_flags.graphically_locked, "ellipse");
    assert!(
        sym.round_rects[0].display_flags.graphically_locked,
        "round rect"
    );
    assert!(
        sym.polylines[0].display_flags.graphically_locked,
        "polyline"
    );
    assert!(sym.polygons[0].display_flags.graphically_locked, "polygon");
    assert!(sym.pies[0].display_flags.graphically_locked, "pie");
    assert!(sym.beziers[0].display_flags.graphically_locked, "bezier");
    let label = sym
        .labels
        .iter()
        .find(|l| l.text == "LOCKED")
        .expect("LOCKFLAGS2 label not found");
    assert!(label.display_flags.graphically_locked, "label");
}

#[test]
fn samples_schlib_justify() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("JUSTIFY").expect("JUSTIFY symbol not found");

    // Labels prove three distinct justifications round-trip from a real Altium
    // file: BottomLeft (default), MiddleCenter (authored eJustify_Center), and
    // TopRight.
    let has = |j: TextJustification| sym.labels.iter().any(|l| l.justification == j);
    assert!(
        has(TextJustification::TopRight),
        "JUSTIFY must carry a TopRight label, got {:?}",
        sym.labels
            .iter()
            .map(|l| l.justification)
            .collect::<Vec<_>>()
    );
    assert!(
        has(TextJustification::MiddleCenter),
        "JUSTIFY must carry a MiddleCenter label (authored eJustify_Center)"
    );
    assert!(
        has(TextJustification::BottomLeft),
        "JUSTIFY must carry a BottomLeft label"
    );

    // Parameter justification (the golden carries `Justification=8` on Value
    // and `Justification=4` on the hidden Tol), which must survive the read.
    let param = |name: &str| -> &Parameter {
        sym.parameters
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("JUSTIFY parameter {name:?} not found"))
    };
    assert_eq!(
        param("Value").justification,
        8,
        "Value parameter is top-right justified (Justification=8)"
    );
    let tol = param("Tol");
    assert_eq!(
        tol.justification, 4,
        "Tol parameter is centre justified (Justification=4)"
    );
    assert_eq!(tol.orientation, 1, "Tol parameter is rotated 90 degrees");
    assert!(tol.hidden, "Tol parameter is hidden");
}

#[test]
fn samples_schlib_fracpins() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("FRACPINS").expect("FRACPINS symbol not found");

    // Three pins: two off-grid (PinFrac stream) and one with a non-default symbol
    // line width (PinSymbolLineWidth stream). This is the FIRST real-Altium ground
    // truth for BOTH pin auxiliary streams, beyond a self-round-trip.
    assert_eq!(sym.pins.len(), 3, "FRACPINS has three pins");
    let pin = |d: &str| {
        sym.pins
            .iter()
            .find(|p| p.designator == d)
            .unwrap_or_else(|| panic!("pin {d} not found"))
    };
    // Pin 1 authored at (5, 3) mil + (0.5, 0.3) mil => frac (55000, 33000).
    assert_eq!(
        pin("1").frac.map(|f| (f.x, f.y)),
        Some((55_000, 33_000)),
        "pin 1 PinFrac"
    );
    // Pin 2 authored at (0, 97) mil + (0.5, 0.3) mil => frac (5000, 73000).
    assert_eq!(
        pin("2").frac.map(|f| (f.x, f.y)),
        Some((5_000, 73_000)),
        "pin 2 PinFrac"
    );
    // Pin 3 authored with Symbol_LineWidth := eLarge (index 3) — the PinSymbolLineWidth
    // aux stream; it is on-grid so carries no PinFrac.
    assert_eq!(
        pin("3").symbol_line_width,
        3,
        "pin 3 symbol_line_width (eLarge)"
    );
    assert_eq!(pin("3").frac, None, "pin 3 is on-grid (no PinFrac)");
}

#[test]
fn samples_schlib_bezier() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("BEZIERSYM").expect("BEZIERSYM symbol not found");

    // One cubic Bezier (four control points) authored via the verified
    // eBezier factory + SetState_Vertex path: AddBezier4(-100, 0, -50, 80,
    // 50, 80, 100, 0) in mils = (-10, 0) (-5, 8) (5, 8) (10, 0) in units.
    assert_eq!(sym.beziers.len(), 1, "BEZIERSYM has one Bezier curve");
    let bez = &sym.beziers[0];
    assert!(
        approx_eq(bez.x1, -10.0)
            && approx_eq(bez.y1, 0.0)
            && approx_eq(bez.x2, -5.0)
            && approx_eq(bez.y2, 8.0)
            && approx_eq(bez.x3, 5.0)
            && approx_eq(bez.y3, 8.0)
            && approx_eq(bez.x4, 10.0)
            && approx_eq(bez.y4, 0.0),
        "Bezier control points must match the authored values, got \
         ({}, {}) ({}, {}) ({}, {}) ({}, {})",
        bez.x1,
        bez.y1,
        bez.x2,
        bez.y2,
        bez.x3,
        bez.y3,
        bez.x4,
        bez.y4
    );
}

#[test]
fn samples_schlib_pie() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("PIESYM").expect("PIESYM symbol not found");

    // One filled pie sector (RECORD=9), authored 30..210 deg, radius 50 mil (=5
    // reader units), yellow fill. This is real-Altium ground truth for a primitive
    // the reader did not parse at all before this change — read as a Pie, NOT an Arc.
    assert!(
        sym.arcs.is_empty(),
        "PIESYM has no arcs (the sector is a Pie)"
    );
    assert_eq!(sym.pies.len(), 1, "PIESYM has one pie");
    let p = &sym.pies[0];
    assert!(
        (p.x - 0.0).abs() < 1e-6 && (p.y - 0.0).abs() < 1e-6,
        "pie centre"
    );
    assert!(
        (p.radius - 5.0).abs() < 1e-6,
        "pie radius (50 mil = 5 units)"
    );
    assert!((p.start_angle - 30.0).abs() < 1e-3, "pie start angle");
    assert!((p.end_angle - 210.0).abs() < 1e-3, "pie end angle");
    assert!(p.filled, "pie is filled (IsSolid)");
    assert_eq!(p.fill_color, 0x00_FF_FF, "pie fill colour (yellow)");
}

#[test]
fn samples_schlib_image() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("IMAGESYM").expect("IMAGESYM symbol not found");

    // One linked image (RECORD=30) — a 100x60 mil box (-5,-3)-(5,3 reader units)
    // referencing "logo.bmp", not embedded, aspect kept. Real-Altium ground truth
    // for a primitive the reader did not parse at all before this change.
    assert_eq!(sym.images.len(), 1, "IMAGESYM has one image");
    let im = &sym.images[0];
    assert!(
        (im.x1 - -5.0).abs() < 1e-6 && (im.y1 - -3.0).abs() < 1e-6,
        "image corner 1"
    );
    assert!(
        (im.x2 - 5.0).abs() < 1e-6 && (im.y2 - 3.0).abs() < 1e-6,
        "image corner 2"
    );
    assert_eq!(im.file_name, "logo.bmp", "image file name round-trips");
    assert!(!im.embed_image, "image is linked, not embedded");
    assert!(im.keep_aspect, "KeepAspect round-trips");
    assert_eq!(
        im.image_data, None,
        "a linked image carries no /Storage bytes"
    );
}

#[test]
fn samples_schlib_embedded_image() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("EMBIMGSYM").expect("EMBIMGSYM symbol not found");

    // One EMBEDDED image (RECORD=30, EmbedImage=T) whose raw bytes AD24 stored
    // in the library-level /Storage stream (one 0xD0 compressed entry, named
    // with the image's full source file path). The committed embed.bmp is
    // byte-identical to the bytes AD24 embedded, so this is real-Altium ground
    // truth for the /Storage read path — exact, all 70 bytes.
    assert_eq!(sym.images.len(), 1, "EMBIMGSYM has one image");
    let im = &sym.images[0];
    assert!(im.embed_image, "the image is embedded (EmbedImage=T)");
    assert_eq!(
        im.file_name, r"C:\Users\Public\altium_designer_mcp\samples\embed.bmp",
        "AD24 stores the full source file path as the FileName"
    );
    let expected: &[u8] = include_bytes!("../scripts/samples/embed.bmp");
    assert_eq!(expected.len(), 70, "the committed embed.bmp is 70 bytes");
    assert_eq!(
        im.image_data.as_deref(),
        Some(expected),
        "the /Storage bytes match the committed embed.bmp exactly"
    );
}

#[test]
fn samples_schlib_text_frame() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib
        .get("TEXTFRAMESYM")
        .expect("TEXTFRAMESYM symbol not found");

    // One text frame (RECORD=28) — a (-10,-5)-(10,5) box holding "Frame text",
    // light-yellow fill, dark-blue text, centred, word-wrapped and clipped, with
    // a 0.2-unit text margin (TextMargin_Frac=20000). Real-Altium ground truth
    // for a primitive the reader did not parse at all before this change.
    assert_eq!(sym.text_frames.len(), 1, "TEXTFRAMESYM has one text frame");
    let f = &sym.text_frames[0];
    assert!(
        (f.x1 - -10.0).abs() < 1e-6 && (f.y1 - -5.0).abs() < 1e-6,
        "frame corner 1"
    );
    assert!(
        (f.x2 - 10.0).abs() < 1e-6 && (f.y2 - 5.0).abs() < 1e-6,
        "frame corner 2"
    );
    assert_eq!(f.text, "Frame text", "frame text round-trips");
    assert_eq!(f.area_color, 11_599_871, "fill colour (light yellow)");
    assert_eq!(f.text_color, 8_388_608, "text colour (dark blue)");
    assert_eq!(f.line_width, 1, "border width");
    assert_eq!(f.font_id, 1, "font id");
    assert!(f.is_solid, "IsSolid round-trips");
    assert!(f.show_border, "ShowBorder round-trips");
    assert!(f.word_wrap, "WordWrap round-trips");
    assert!(f.clip_to_rect, "ClipToRect round-trips");
    assert_eq!(f.alignment, 1, "centred alignment");
    assert!(
        (f.text_margin - 0.2).abs() < 1e-6,
        "text margin (TextMargin_Frac=20000 = 0.2 units), got {}",
        f.text_margin
    );
    assert!(f.is_not_accessible, "IsNotAccesible round-trips");
}

#[test]
fn samples_schlib_fracshapes() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("FRACSHAPES").expect("FRACSHAPES symbol not found");

    // Off-grid shapes authored at MilsToCoord(n) + 5000 internal units (+0.5 mil
    // = +0.05 units per coordinate). AD24 stores negative off-grid coordinates
    // as truncation-toward-zero with a SIGNED fraction — the golden rectangle is
    // literally `Location.X=-5|Location.X_Frac=-45000` (= -5.45). Before the
    // signed-frac fix the reader parsed `_Frac` as u32, so -45000 failed to
    // parse and silently truncated the coordinate to -5.0.
    assert_eq!(sym.rectangles.len(), 1, "FRACSHAPES has one rectangle");
    let r = &sym.rectangles[0];
    assert!(
        approx_eq(r.x1, -5.45) && approx_eq(r.y1, -2.45),
        "rectangle corner 1 (authored -55-25 mil + 0.5 mil), got ({}, {})",
        r.x1,
        r.y1
    );
    assert!(
        approx_eq(r.x2, 5.55) && approx_eq(r.y2, 2.55),
        "rectangle corner 2 (authored 55/25 mil + 0.5 mil), got ({}, {})",
        r.x2,
        r.y2
    );

    // The arc centre sits at (+0.05, +0.05): AD24 omits the zero integer keys
    // entirely and stores only `Location.X_Frac=5000|Location.Y_Frac=5000`,
    // plus `Radius=4|Radius_Frac=5000` (= 4.05).
    assert_eq!(sym.arcs.len(), 1, "FRACSHAPES has one arc");
    let a = &sym.arcs[0];
    assert!(
        approx_eq(a.x, 0.05) && approx_eq(a.y, 0.05),
        "arc centre (integer keys omitted, frac-only), got ({}, {})",
        a.x,
        a.y
    );
    assert!(approx_eq(a.radius, 4.05), "arc radius, got {}", a.radius);
    assert!(approx_eq(a.end_angle, 270.0), "arc end angle");
}

#[test]
fn samples_schlib_swappin() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("SWAPPIN").expect("SWAPPIN symbol not found");

    // One pin authored with the swap-id tail: SwapId_Pin='A', SwapId_Part='1',
    // DefaultValue='3V3'. The binary pin record stores these as three trailing
    // Pascal short strings, mapped by the reader in order:
    //   SwapId_Pin  -> swap_id_group      ("A")
    //   SwapId_Part -> part_and_sequence  ("1" — replacing the "|&|" default)
    //   DefaultValue -> default_value     ("3V3")
    // Real-Altium ground truth for the tail, beyond a
    // self-round-trip.
    assert_eq!(sym.pins.len(), 1, "SWAPPIN has one pin");
    let pin = pin_by_designator(sym, "1");
    assert_eq!(pin.name, "SWP", "pin name");
    assert_eq!(pin.swap_id_group, "A", "SwapId_Pin lands in swap_id_group");
    assert_eq!(
        pin.part_and_sequence, "1",
        "SwapId_Part lands in part_and_sequence"
    );
    assert_eq!(
        pin.default_value, "3V3",
        "DefaultValue lands in default_value"
    );
}

#[test]
fn samples_schlib_dispmode() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("DISPMODE").expect("DISPMODE symbol not found");

    // A DisplayModeCount=2 symbol with one rectangle per display mode — the
    // first real-Altium golden for a non-default `OwnerPartDisplayMode` (it was
    // self-round-trip only until this fixture). The RECORD=1 header carries
    // `DisplayModeCount=2` verbatim.
    assert_eq!(sym.display_mode_count, 2, "DisplayModeCount round-trips");
    assert_eq!(sym.rectangles.len(), 2, "DISPMODE has two rectangles");

    // Mode-0 (normal view) rectangle: (-5, -2.5)..(5, 2.5) units. The ±2.5 y
    // coordinates are more signed-frac ground truth — the golden stores
    // `Location.Y=-2|Location.Y_Frac=-50000` (truncation toward zero with a
    // signed fraction) and `Corner.Y=2|Corner.Y_Frac=50000`.
    let mode0 = sym
        .rectangles
        .iter()
        .find(|r| r.display_flags.owner_part_display_mode == 0)
        .expect("DISPMODE has a mode-0 rectangle");
    assert!(
        approx_eq(mode0.x1, -5.0) && approx_eq(mode0.y1, -2.5),
        "mode-0 corner 1 (signed-frac y), got ({}, {})",
        mode0.x1,
        mode0.y1
    );
    assert!(
        approx_eq(mode0.x2, 5.0) && approx_eq(mode0.y2, 2.5),
        "mode-0 corner 2, got ({}, {})",
        mode0.x2,
        mode0.y2
    );

    // Mode-1 (alternate view) rectangle: (-6, -3)..(6, 3) units, carrying
    // `OwnerPartDisplayMode=1` in its RECORD=14 line.
    let mode1 = sym
        .rectangles
        .iter()
        .find(|r| r.display_flags.owner_part_display_mode == 1)
        .expect("DISPMODE has a mode-1 rectangle");
    assert!(
        approx_eq(mode1.x1, -6.0) && approx_eq(mode1.y1, -3.0),
        "mode-1 corner 1, got ({}, {})",
        mode1.x1,
        mode1.y1
    );
    assert!(
        approx_eq(mode1.x2, 6.0) && approx_eq(mode1.y2, 3.0),
        "mode-1 corner 2, got ({}, {})",
        mode1.x2,
        mode1.y2
    );

    // Both rectangles belong to part 1 — display modes are orthogonal to parts.
    assert_eq!(mode0.owner_part_id, 1, "mode-0 owner part");
    assert_eq!(mode1.owner_part_id, 1, "mode-1 owner part");

    // A pin stores its display mode as a byte of the binary pin record —
    // golden evidence for the non-default value, which was self-round-trip
    // only until this fixture.
    assert_eq!(sym.pins.len(), 2, "DISPMODE has a pin per display mode");
    let pin_mode0 = sym
        .pins
        .iter()
        .find(|p| p.designator == "1")
        .expect("DISPMODE has pin 1");
    assert_eq!(pin_mode0.name, "M0");
    assert_eq!(pin_mode0.owner_part_display_mode, 0, "pin 1 is mode 0");
    let pin_mode1 = sym
        .pins
        .iter()
        .find(|p| p.designator == "2")
        .expect("DISPMODE has pin 2");
    assert_eq!(pin_mode1.name, "M1");
    assert_eq!(pin_mode1.owner_part_display_mode, 1, "pin 2 is mode 1");
}

// ---------------------------------------------------------------------------
// Read-modify-write byte fidelity against the golden.
//
// These read a symbol from the Altium-authored golden and re-encode it with
// our writer, then compare the emitted records against the golden's exact
// record text (dumped byte-for-byte from scripts/samples/symbols.SchLib).
// Only the RECORD=1 component header is excluded (its UniqueID / AllPinCount
// fidelity is tracked separately in TODO §B); every content, designator and
// system-parameter record must match the golden token-for-token.
// ---------------------------------------------------------------------------

/// Replaces each `UniqueID` value with the `<UID>` placeholder.
///
/// Altium mints fresh random ids every time the samples are authored, so the
/// literal values cannot be asserted without the expectations breaking on every
/// regeneration. The shape still is: the key must be present, in the same
/// position, with exactly eight uppercase letters.
fn normalise_unique_ids(record: &str) -> String {
    let mut out = String::with_capacity(record.len());
    let mut rest = record;
    while let Some(at) = rest.find("UniqueID=") {
        let (before, tail) = rest.split_at(at + "UniqueID=".len());
        out.push_str(before);
        let id: String = tail.chars().take_while(char::is_ascii_uppercase).collect();
        assert_eq!(
            id.len(),
            8,
            "UniqueID must be 8 uppercase letters, got {id:?} in {record:?}"
        );
        out.push_str("<UID>");
        rest = &tail[id.len()..];
    }
    out.push_str(rest);
    out
}

/// Re-encodes `name` from the golden and returns its records as text (the
/// trailing NUL trimmed; binary pin records surface as `"<PIN>"`), excluding
/// the RECORD=1 component header, with `UniqueID` values normalised.
fn reencoded_records(lib: &SchLib, name: &str) -> Vec<String> {
    let symbol = lib.get(name).unwrap_or_else(|| panic!("{name} not found"));
    let data = altium_designer_mcp::altium::schlib::writer::encode_data_stream(symbol)
        .expect("re-encode golden symbol");
    let mut records = Vec::new();
    let mut off = 0;
    while off + 4 <= data.len() {
        let len =
            data[off] as usize | ((data[off + 1] as usize) << 8) | ((data[off + 2] as usize) << 16);
        let flags = data[off + 3];
        if flags == 1 {
            records.push("<PIN>".to_string());
        } else {
            records.push(
                String::from_utf8_lossy(&data[off + 4..off + 4 + len])
                    .trim_end_matches('\0')
                    .to_string(),
            );
        }
        off += 4 + len;
    }
    records.remove(0); // RECORD=1 header (see doc comment)
    records.iter().map(|r| normalise_unique_ids(r)).collect()
}

#[test]
fn samples_schlib_rmw_dispmode_matches_golden_records() {
    // The F1 headline proof: the system Comment keeps its IndexInSheet=-1
    // sentinel (no counter slot), the first rectangle stays at slot 0 (token
    // omitted) and the second at =1 — the full stream re-encodes exactly.
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    assert_eq!(
        reencoded_records(&lib, "DISPMODE"),
        [
            "|RECORD=14|IsNotAccesible=T|OwnerPartId=1|Location.X=-5|Location.Y=-2|Location.Y_Frac=-50000|Corner.X=5|Corner.Y=2|Corner.Y_Frac=50000|LineWidth=1|AreaColor=11599871|IsSolid=T|UniqueID=<UID>",
            "|RECORD=14|IsNotAccesible=T|IndexInSheet=1|OwnerPartId=1|OwnerPartDisplayMode=1|Location.X=-6|Location.Y=-3|Corner.X=6|Corner.Y=3|LineWidth=1|AreaColor=11599871|IsSolid=T|UniqueID=<UID>",
            "<PIN>",
            "<PIN>",
            "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=U?|Name=Designator|ReadOnlyState=1|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15|Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=<UID>",
            "|RECORD=44",
        ]
    );
}

#[test]
fn samples_schlib_rmw_lines_matches_golden_records() {
    // F2 proof: every zero coordinate key stays omitted on re-encode (the
    // golden line (0,0)->(10,0) carries only Corner.X=10).
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    assert_eq!(
        reencoded_records(&lib, "LINES"),
        [
            "|RECORD=13|IsNotAccesible=T|OwnerPartId=1|Corner.X=10|LineWidth=1|UniqueID=<UID>",
            "|RECORD=13|IsNotAccesible=T|IndexInSheet=1|OwnerPartId=1|Corner.Y=10|LineWidth=1|UniqueID=<UID>",
            "|RECORD=13|IsNotAccesible=T|IndexInSheet=2|OwnerPartId=1|Corner.X=10|Corner.Y=10|LineWidth=1|UniqueID=<UID>",
            "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=U?|Name=Designator|ReadOnlyState=1|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15|Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=<UID>",
            "|RECORD=44",
        ]
    );
}

#[test]
fn samples_schlib_rmw_arcs_and_fracshapes_match_golden_records() {
    // F3.6 proof: LineWidth precedes the 3-decimal angles and a zero
    // StartAngle is omitted (EndAngle=360.000 / 90.000 / 270.000), plus the
    // FRACSHAPES signed-frac coordinates re-encode exactly.
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    assert_eq!(
        reencoded_records(&lib, "ARCS"),
        [
            "|RECORD=12|IsNotAccesible=T|OwnerPartId=1|Radius=5|LineWidth=1|EndAngle=360.000|UniqueID=<UID>",
            "|RECORD=12|IsNotAccesible=T|IndexInSheet=1|OwnerPartId=1|Location.Y=-20|Radius=5|LineWidth=1|EndAngle=90.000|UniqueID=<UID>",
            "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=U?|Name=Designator|ReadOnlyState=1|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15|Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=<UID>",
            "|RECORD=44",
        ]
    );
    assert_eq!(
        reencoded_records(&lib, "FRACSHAPES"),
        [
            "|RECORD=14|IsNotAccesible=T|OwnerPartId=1|Location.X=-5|Location.X_Frac=-45000|Location.Y=-2|Location.Y_Frac=-45000|Corner.X=5|Corner.X_Frac=55000|Corner.Y=2|Corner.Y_Frac=55000|LineWidth=1|AreaColor=11599871|IsSolid=T|UniqueID=<UID>",
            "|RECORD=12|IsNotAccesible=T|IndexInSheet=1|OwnerPartId=1|Location.X_Frac=5000|Location.Y_Frac=5000|Radius=4|Radius_Frac=5000|LineWidth=1|EndAngle=270.000|UniqueID=<UID>",
            "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=U?|Name=Designator|ReadOnlyState=1|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15|Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=<UID>",
            "|RECORD=44",
        ]
    );
}

#[test]
fn samples_schlib_rmw_justify_and_params_match_golden_records() {
    // F3.2/F3.9 proof: user parameters keep the golden token order (Location,
    // Orientation, Justification, FontID, IsHidden, Text, Name), the zero
    // Color stays omitted, and they follow the labels on the shared content
    // counter (slots 4 and 5, exactly as the golden stores).
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    assert_eq!(
        reencoded_records(&lib, "JUSTIFY"),
        [
            "|RECORD=4|IsNotAccesible=T|OwnerPartId=1|Location.X=-10|Location.Y=10|FontID=1|Text=BL|UniqueID=<UID>",
            "|RECORD=4|IsNotAccesible=T|IndexInSheet=1|OwnerPartId=1|Location.X=-10|Location.Y=5|Justification=4|FontID=1|Text=CC|UniqueID=<UID>",
            "|RECORD=4|IsNotAccesible=T|IndexInSheet=2|OwnerPartId=1|Location.X=-10|Justification=8|FontID=1|Text=TR|UniqueID=<UID>",
            "|RECORD=4|IsNotAccesible=T|IndexInSheet=3|OwnerPartId=1|Location.X=-10|Location.Y=-5|Orientation=1|FontID=1|Text=ROT90|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=4|OwnerPartId=1|Location.X=10|Location.Y=10|Justification=8|FontID=1|Text=1k|Name=Value|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=5|OwnerPartId=1|Location.X=10|Location.Y=5|Orientation=1|Justification=4|FontID=1|IsHidden=T|Text=5%|Name=Tol|UniqueID=<UID>",
            "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=U?|Name=Designator|ReadOnlyState=1|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15|Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=<UID>",
            "|RECORD=44",
        ]
    );
    assert_eq!(
        reencoded_records(&lib, "PARAMS"),
        [
            "|RECORD=41|OwnerPartId=1|Location.X=5|Location.Y=40|FontID=1|Text=10k|Name=Value|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=1|OwnerPartId=1|Location.X=5|Location.Y=45|FontID=1|IsHidden=T|Text=100nF|Name=Comment|UniqueID=<UID>",
            "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=U?|Name=Designator|ReadOnlyState=1|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15|Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=<UID>",
            "|RECORD=44",
        ]
    );
}

#[test]
fn samples_schlib_rmw_polyline_ellipse_bezier_textframe_match_golden_records() {
    // F3.1 (IsNotAccesible on ellipse/polyline), F3.4 (zero polyline style
    // keys omitted), F2 (zero vertices omitted) and F4 (text frame token
    // order with unconditional AreaColor/FontID) all re-encode exactly.
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    assert_eq!(
        reencoded_records(&lib, "POLYLINES"),
        [
            "|RECORD=6|IsNotAccesible=T|OwnerPartId=1|LineWidth=1|LocationCount=3|X2=10|Y2=5|Y3=10|UniqueID=<UID>",
            "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=U?|Name=Designator|ReadOnlyState=1|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15|Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=<UID>",
            "|RECORD=44",
        ]
    );
    assert_eq!(
        reencoded_records(&lib, "ELLIPSES"),
        [
            "|RECORD=8|IsNotAccesible=T|OwnerPartId=1|Radius=5|SecondaryRadius=5|LineWidth=1|AreaColor=65535|IsSolid=T|UniqueID=<UID>",
            "|RECORD=8|IsNotAccesible=T|IndexInSheet=1|OwnerPartId=1|Location.X=20|Radius=8|SecondaryRadius=4|LineWidth=1|AreaColor=65535|UniqueID=<UID>",
            "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=U?|Name=Designator|ReadOnlyState=1|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15|Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=<UID>",
            "|RECORD=44",
        ]
    );
    assert_eq!(
        reencoded_records(&lib, "BEZIERSYM"),
        [
            "|RECORD=5|IsNotAccesible=T|OwnerPartId=1|LineWidth=1|LocationCount=4|X1=-10|X2=-5|Y2=8|X3=5|Y3=8|X4=10|UniqueID=<UID>",
            "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=U?|Name=Designator|ReadOnlyState=1|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15|Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=<UID>",
            "|RECORD=44",
        ]
    );
    assert_eq!(
        reencoded_records(&lib, "TEXTFRAMESYM"),
        [
            "|RECORD=28|IsNotAccesible=T|OwnerPartId=1|Location.X=-10|Location.Y=-5|Corner.X=10|Corner.Y=5|LineWidth=1|AreaColor=11599871|TextColor=8388608|FontID=1|IsSolid=T|ShowBorder=T|Alignment=1|WordWrap=T|ClipToRect=T|Text=Frame text|TextMargin_Frac=20000|UniqueID=<UID>",
            "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=U?|Name=Designator|ReadOnlyState=1|UniqueID=<UID>",
            "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15|Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=<UID>",
            "|RECORD=44",
        ]
    );
}

#[test]
fn samples_schlib_rmw_shapestyle_records_match_golden_ignoring_stream_order() {
    // SHAPESTYLE mixes shape families, and our writer groups families in a
    // fixed order while the golden stream interleaves them — so the shared
    // IndexInSheet slots differ by position. Every record must still match a
    // golden record exactly once with the positional IndexInSheet token
    // stripped (proving the F3.5 LineStyle+LineStyleExt dual-key emission and
    // the F3.3/polygon Transparent placement byte-exactly), and the emitted
    // slots must still be the golden's {omitted, 1..5}.
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let strip = |s: &str| -> String {
        match (s.find("|IndexInSheet="), s.find("|OwnerPartId=")) {
            (Some(a), Some(b)) if a < b => format!("{}{}", &s[..a], &s[b..]),
            _ => s.to_string(),
        }
    };
    let mut ours: Vec<String> = reencoded_records(&lib, "SHAPESTYLE")
        .iter()
        .map(|s| strip(s))
        .collect();
    let mut golden: Vec<String> = [
        "|RECORD=13|IsNotAccesible=T|OwnerPartId=1|Location.X=-20|LineWidth=1|LineStyle=1|LineStyleExt=1|UniqueID=<UID>",
        "|RECORD=13|IsNotAccesible=T|IndexInSheet=1|OwnerPartId=1|Corner.X=20|LineWidth=1|LineStyle=2|LineStyleExt=2|UniqueID=<UID>",
        "|RECORD=14|IsNotAccesible=T|IndexInSheet=2|OwnerPartId=1|Location.X=-10|Location.Y=-10|Corner.X=10|Corner.Y=-5|LineWidth=1|AreaColor=65535|IsSolid=T|UniqueID=<UID>",
        "|RECORD=14|IsNotAccesible=T|IndexInSheet=3|OwnerPartId=1|Location.X=-10|Location.Y=5|Corner.X=10|Corner.Y=10|LineWidth=1|AreaColor=11599871|IsSolid=T|Transparent=T|UniqueID=<UID>",
        "|RECORD=7|IsNotAccesible=T|IndexInSheet=4|OwnerPartId=1|LineWidth=1|AreaColor=65280|IsSolid=T|Transparent=T|LocationCount=3|X1=-5|Y1=12|X2=5|Y2=12|X3=5|Y3=17|UniqueID=<UID>",
        "|RECORD=8|IsNotAccesible=T|IndexInSheet=5|OwnerPartId=1|Location.X=15|Location.Y=10|Radius=3|SecondaryRadius=2|LineWidth=1|AreaColor=11599871|IsSolid=T|Transparent=T|UniqueID=<UID>",
        "|RECORD=14|IsNotAccesible=T|IndexInSheet=6|OwnerPartId=1|Location.X=15|Location.Y=-10|Corner.X=25|Corner.Y=-5|LineStyleExt=1|LineWidth=1|AreaColor=11599871|UniqueID=<UID>",
        "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=5|Color=8388608|FontID=1|Text=U?|Name=Designator|ReadOnlyState=1|UniqueID=<UID>",
        "|RECORD=41|IndexInSheet=-1|OwnerPartId=-1|Location.X=-5|Location.Y=-15|Color=8388608|FontID=1|Text=*|Name=Comment|UniqueID=<UID>",
        "|RECORD=44",
    ]
    .iter()
    .map(|s| strip(s))
    .collect();
    ours.sort();
    golden.sort();
    assert_eq!(ours, golden, "SHAPESTYLE records (IndexInSheet-stripped)");

    // The shared counter itself must still produce the golden slot set.
    let slots: Vec<Option<i32>> = reencoded_records(&lib, "SHAPESTYLE")
        .iter()
        .filter(|s| !s.contains("OwnerPartId=-1") && !s.ends_with("RECORD=44"))
        .map(|s| {
            s.find("|IndexInSheet=")
                .map(|a| s[a + 14..].split('|').next().unwrap().parse().unwrap())
        })
        .collect();
    let mut numbered: Vec<i32> = slots.iter().filter_map(|s| *s).collect();
    numbered.sort_unstable();
    assert_eq!(
        slots.iter().filter(|s| s.is_none()).count(),
        1,
        "exactly one content record sits at slot 0 (token omitted)"
    );
    assert_eq!(numbered, vec![1, 2, 3, 4, 5, 6], "content slots 1..6");
}

#[test]
fn samples_schlib_unicode_symbol_name_and_description() {
    // A symbol whose name is outside Windows-1252, authored by Altium itself.
    //
    // Altium writes such a value as its raw UTF-8 bytes under the plain key and
    // widens the same bytes through the authoring machine's ANSI code page for
    // the CFB storage name, so the header's LibRef entry and the storage name
    // carry different bytes. Components are therefore located by walking the
    // storages rather than trusting that list, and the name is recovered from
    // the plain key's UTF-8 bytes — the `%UTF8%` companion Altium writes
    // alongside is locale-dependent and decodes to mojibake off that machine.
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");

    let sym = lib
        .get("Резистор")
        .expect("the Cyrillic-named symbol must be found, not silently skipped");
    assert_eq!(sym.name, "Резистор");
    assert_eq!(sym.description, "описание Ω", "Greek omega survives too");
    assert_eq!(sym.rectangles.len(), 1, "its body shape is read normally");
}

#[test]
fn samples_schlib_manual_parameter_properties() {
    // Hand-authored fixture (scripts/samples/manual/parameters.SchLib) — AD24 exposes
    // neither of these on ISch_Parameter, so no DelphiScript can author them and the
    // generated golden cannot carry them. See that folder's README to rebuild it.
    //
    // NotAutoPosition is stored inverted and omit-when-default: Altium writes the key
    // only when the user turns auto-positioning OFF. This fixture is what proved that,
    // and that the key is not the `AUTOPOSITION` the docs had led us to expect.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("samples")
        .join("manual")
        .join("parameters.SchLib");
    let lib = SchLib::open(&path).expect("failed to open manual/parameters.SchLib");
    let sym = lib.get("PARAMPROPS").expect("symbol PARAMPROPS not found");

    let param = |n: &str| {
        sym.parameters
            .iter()
            .find(|p| p.name == n)
            .unwrap_or_else(|| panic!("parameter {n} not found"))
    };
    let test_param = param("TestParam");
    assert!(
        !test_param.auto_position,
        "TestParam was authored with auto-positioning turned off"
    );
    assert_eq!(test_param.justification, 7, "top-centre");

    // The control: an untouched parameter omits the key, which must read as ON rather
    // than defaulting to off — the inversion is easy to get backwards.
    assert!(
        param("Comment").auto_position,
        "an untouched parameter auto-positions"
    );

    // A rule parameter is identified by its name and payload, not by a flag: AD24
    // writes no IsRule key into a library.
    let rule = param("Rule");
    assert!(
        rule.value.contains("RULEKIND=Width"),
        "value: {}",
        rule.value
    );
    assert!(rule.hidden, "the rule parameter is hidden");
}

/// The `SectionKeys` stream — the real-name -> truncated-storage-name map for
/// components past the 31-unit cap — must survive a read-modify-write with the
/// same pairs, and the truncated storage names must match Altium's plain cut.
#[test]
fn samples_schlib_section_keys_survive_a_read_modify_write() {
    use std::io::{Cursor, Read as _};

    /// `(LibRef, SectionKey)` pairs from a library's `/SectionKeys` stream, as
    /// a set — entry numbering is Altium's iteration order, which we do not
    /// promise to reproduce.
    fn pairs(bytes: &[u8]) -> std::collections::BTreeSet<(String, String)> {
        let mut cfb = cfb::CompoundFile::open(Cursor::new(bytes)).expect("parse OLE");
        let mut data = Vec::new();
        cfb.open_stream("/SectionKeys")
            .expect("SectionKeys stream present")
            .read_to_end(&mut data)
            .expect("read SectionKeys");
        // [u32 len][text + NUL]: split the pipe list, keep LibRefN/SectionKeyN.
        let text: String = data[4..].iter().map(|&b| b as char).collect();
        let text = text.trim_end_matches('\0');
        let mut lib_refs = std::collections::BTreeMap::new();
        let mut section_keys = std::collections::BTreeMap::new();
        for part in text.split('|') {
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            if let Some(n) = key.strip_prefix("LibRef") {
                if let Ok(n) = n.parse::<usize>() {
                    lib_refs.insert(n, value.to_string());
                }
            } else if let Some(n) = key.strip_prefix("SectionKey") {
                if let Ok(n) = n.parse::<usize>() {
                    section_keys.insert(n, value.to_string());
                }
            }
        }
        lib_refs
            .into_iter()
            .filter_map(|(n, lr)| section_keys.get(&n).map(|sk| (lr, sk.clone())))
            .collect()
    }

    let golden = std::fs::read(sample("symbols.SchLib")).expect("read golden");
    let golden_pairs = pairs(&golden);
    assert_eq!(
        golden_pairs.len(),
        5,
        "the golden truncates five names (Khmer, Malayalam, Thai, Sinhala, Lao)"
    );
    for (lib_ref, section_key) in &golden_pairs {
        assert_eq!(
            section_key.as_bytes(),
            &lib_ref.as_bytes()[..section_key.len()],
            "a SectionKey is its LibRef plain-cut at the storage cap"
        );
        // The pair strings hold one char per wire byte, so the cap is counted
        // in chars, not in this string's own UTF-8 length.
        assert!(section_key.chars().count() <= 31, "storage cap is 31 units");
    }

    let lib = SchLib::read(Cursor::new(golden.as_slice())).expect("read golden");
    let mut rewritten = Cursor::new(Vec::new());
    lib.write(&mut rewritten).expect("write the golden back");

    // The Inuktitut fixture is one of the five DelphiScript-mangled symbols:
    // its RECORD name (what any reader gets) is a 45-byte mangled string that
    // needs truncating, while Altium's in-memory name was the real 9-unit word
    // and needed no entry. Golden and rewrite legitimately disagree there, so
    // the damaged fixtures are excluded and everything else must match.
    let damaged = ["_JV", "_BN", "_CR", "_IU", "_SB"];
    let clean = |set: std::collections::BTreeSet<(String, String)>| {
        set.into_iter()
            .filter(|(lr, _)| !damaged.iter().any(|d| lr.ends_with(d)))
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        clean(pairs(rewritten.get_ref())),
        clean(golden_pairs),
        "every LibRef -> SectionKey pair must come back; a changed pair breaks \
         Altium's name resolution for that component"
    );
}

/// The golden's 52 `PinWideText` streams — the authoritative wide form of each
/// non-ASCII pin name — must come back from a read-modify-write for the same
/// components, resolving to the same names.
#[test]
fn samples_schlib_pin_wide_text_survives_a_read_modify_write() {
    use std::io::{Cursor, Read as _};

    /// Canonical component -> resolved pin names from every `PinWideText` stream.
    /// Payload values are folded the way the reader folds them, so a golden
    /// stream (ANSI-widened bytes) and ours (real UTF-16) compare equal.
    fn wide_names(bytes: &[u8]) -> std::collections::BTreeMap<String, Vec<String>> {
        let mut cfb = cfb::CompoundFile::open(Cursor::new(bytes)).expect("parse OLE");
        let paths: Vec<std::path::PathBuf> = cfb
            .walk()
            .filter(cfb::Entry::is_stream)
            .filter(|e| e.name() == "PinWideText")
            .map(|e| e.path().to_path_buf())
            .collect();
        let mut map = std::collections::BTreeMap::new();
        for path in paths {
            let mut raw = Vec::new();
            cfb.open_stream(&path)
                .expect("open PinWideText")
                .read_to_end(&mut raw)
                .expect("read PinWideText");

            // Resolve through the reader itself: husk pins + apply = the names.
            let mut pins: Vec<Pin> = (0..64)
                .map(|i| Pin::new("?", i.to_string(), 0, 0, 10, PinOrientation::Right))
                .collect();
            altium_designer_mcp::altium::schlib::apply_pin_wide_text_for_test(&mut pins, &raw);
            let names: Vec<String> = pins
                .into_iter()
                .map(|p| p.name)
                .filter(|n| n != "?")
                .collect();

            // Fold the component storage name to a locale-free key: its bytes
            // are the name's UTF-8 widened through the authoring code page.
            let seg = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            map.insert(canonical(&seg), names);
        }
        map
    }

    /// Locale-free fold of a storage name (1250 for the golden, 1252 for us).
    /// A truncated storage name can end mid-codepoint, so an incomplete tail
    /// is accepted and folded lossily — identically on both sides.
    fn canonical(seg: &str) -> String {
        for enc in [encoding_rs::WINDOWS_1252, encoding_rs::WINDOWS_1250] {
            let (bytes, _, had_errors) = enc.encode(seg);
            if had_errors {
                continue;
            }
            let sound = match std::str::from_utf8(&bytes) {
                Ok(s) => !s.is_ascii(),
                Err(e) => e.error_len().is_none() && e.valid_up_to() > 0,
            };
            if sound {
                return String::from_utf8_lossy(&bytes).to_lowercase();
            }
        }
        seg.to_lowercase()
    }

    let golden = std::fs::read(sample("symbols.SchLib")).expect("read golden");
    let golden_names = wide_names(&golden);
    assert_eq!(
        golden_names.len(),
        52,
        "one PinWideText stream per i18n symbol"
    );

    let lib = SchLib::read(Cursor::new(golden.as_slice())).expect("read golden");
    let mut rewritten = Cursor::new(Vec::new());
    lib.write(&mut rewritten).expect("write the golden back");
    let ours = wide_names(rewritten.get_ref());

    assert_eq!(ours.len(), 52, "the rewrite emits a stream per i18n symbol");

    // The five DelphiScript-damaged fixtures resolve differently on purpose
    // (their storage names and record names disagree in the golden itself), so
    // golden-side subset equality is asserted over the other 47: every clean
    // golden component must come back with the same resolved pin names.
    let damaged = ["_jv", "_bn", "_cr", "_iu", "_sb"];
    let mut checked = 0;
    for (component, names) in &golden_names {
        if damaged.iter().any(|d| component.ends_with(d)) {
            continue;
        }
        assert_eq!(
            ours.get(component),
            Some(names),
            "{component}: wide pin names must survive the rewrite"
        );
        checked += 1;
    }
    assert_eq!(checked, 47, "all clean components were compared");
}

/// The hand-authored `manual/i18n5.SchLib`: the five scripts whose generated
/// fixtures are internally inconsistent (see `FIXTURE_INCONSISTENT` in
/// `golden_fidelity.rs`), authored once in the AD24 UI on 2026-08-16 — the
/// only route that bypasses AD's broken decode of these sequences.
///
/// The file doubles as ground truth for the UI-authoring convention: the
/// record's plain keys are ANSI `?` husks, the real names live in `%UTF8%`
/// twins as raw UTF-8 bytes (exactly what our writer emits), the CFB storage
/// names are real UTF-16 (surrogate pair included), and the pin names arrive
/// only via `PinWideText` applied onto the husks.
#[test]
fn samples_manual_i18n5_scripts_read_exactly() {
    let lib =
        SchLib::open(sample("manual/i18n5.SchLib")).expect("failed to open manual/i18n5.SchLib");
    assert_eq!(lib.len(), 5, "five symbols, one per damaged script");

    let cases: [(&str, &str, &str); 5] = [
        (
            "\u{A997}\u{A9AE}_JV",
            "\u{A997}\u{A9AE}",
            "Script: Javanese",
        ),
        (
            "\u{09B0}\u{09CB}\u{09A7}\u{0995}_BN",
            "\u{09B0}\u{09CB}\u{09A7}\u{0995}",
            "Script: Bengali",
        ),
        (
            "\u{13E3}\u{13B3}\u{13A9}_CR",
            "\u{13E3}\u{13B3}\u{13A9}",
            "Script: Cherokee",
        ),
        (
            "\u{1403}\u{14C4}\u{1483}\u{144E}\u{1450}\u{1466}_IU",
            "\u{1403}\u{14C4}\u{1483}\u{144E}\u{1450}\u{1466}",
            "Script: Canadian Aboriginal Syllabics, Inuktitut",
        ),
        (
            "\u{20BB7}\u{91CE}_SB",
            "\u{20BB7}\u{91CE}",
            "Script: Han beyond the BMP: surrogate pair",
        ),
    ];
    for (name, word, desc) in cases {
        let s = lib
            .get(name)
            .unwrap_or_else(|| panic!("symbol {name:?} not found"));
        assert_eq!(s.description, desc, "{name}: description");
        assert_eq!(s.pins.len(), 1, "{name}: one pin");
        assert_eq!(s.pins[0].name, word, "{name}: pin name (via PinWideText)");
        assert_eq!(s.pins[0].designator, "1", "{name}: pin designator");
        assert_eq!(s.labels.len(), 1, "{name}: one label");
        assert_eq!(s.labels[0].text, word, "{name}: label text");
        let value = s
            .parameters
            .iter()
            .find(|p| p.name == "Value")
            .unwrap_or_else(|| panic!("{name}: Value parameter"));
        assert_eq!(value.value, word, "{name}: parameter value");
    }
}

/// The manual i18n fixture survives our own write -> read, every field.
#[test]
fn samples_manual_i18n5_survives_a_write_read_cycle() {
    use std::io::Cursor;

    let lib = SchLib::open(sample("manual/i18n5.SchLib")).expect("open manual/i18n5.SchLib");
    let mut buf = Cursor::new(Vec::new());
    lib.write(&mut buf).expect("write");
    buf.set_position(0);
    let re = SchLib::read(&mut buf).expect("read back");

    assert_eq!(re.len(), lib.len());
    for s in lib.iter() {
        let r = re
            .get(&s.name)
            .unwrap_or_else(|| panic!("{} missing after rewrite", s.name));
        assert_eq!(r.description, s.description, "{}: description", s.name);
        assert_eq!(
            r.pins.iter().map(|p| &p.name).collect::<Vec<_>>(),
            s.pins.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "{}: pin names",
            s.name
        );
        assert_eq!(
            r.labels.iter().map(|l| &l.text).collect::<Vec<_>>(),
            s.labels.iter().map(|l| &l.text).collect::<Vec<_>>(),
            "{}: label texts",
            s.name
        );
        assert_eq!(
            r.parameters.iter().map(|p| &p.value).collect::<Vec<_>>(),
            s.parameters.iter().map(|p| &p.value).collect::<Vec<_>>(),
            "{}: parameter values",
            s.name
        );
    }
}

/// `ELLARC` (batch 5): the first golden elliptical arcs (RECORD=11). The
/// second is off-grid — every coordinate and both radii carry a `_Frac` key —
/// and graphically locked: `GraphicallyLocked=T` sits right after
/// `OwnerPartId`, as on every other graphic, which is what settled that the
/// record carries the universal display flags.
#[test]
fn samples_schlib_ellarc() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("ELLARC").expect("ELLARC symbol not found");
    assert_eq!(
        sym.elliptical_arcs.len(),
        2,
        "ELLARC has two elliptical arcs"
    );

    let plain = &sym.elliptical_arcs[0];
    assert!(
        approx_eq(plain.x, -6.0) && approx_eq(plain.y, 0.0),
        "centre"
    );
    assert!(approx_eq(plain.radius, 5.0) && approx_eq(plain.secondary_radius, 3.0));
    assert!(approx_eq(plain.start_angle, 0.0) && approx_eq(plain.end_angle, 270.0));
    assert_eq!(plain.line_width, 1);
    assert!(!plain.display_flags.graphically_locked);

    let locked = &sym.elliptical_arcs[1];
    assert!(
        approx_eq(locked.x, 6.05) && approx_eq(locked.y, 0.05),
        "off-grid centre, got ({}, {})",
        locked.x,
        locked.y
    );
    assert!(approx_eq(locked.radius, 5.05), "Radius + Radius_Frac");
    assert!(
        approx_eq(locked.secondary_radius, 3.05),
        "SecondaryRadius + SecondaryRadius_Frac"
    );
    assert!(approx_eq(locked.start_angle, 45.0) && approx_eq(locked.end_angle, 315.0));
    assert!(
        locked.display_flags.graphically_locked,
        "GraphicallyLocked=T"
    );
    assert!(!locked.display_flags.disabled && !locked.display_flags.dimmed);
}

/// `IMPLCHAIN` (batch 5): footprint model links as Altium writes the chain —
/// `RECORD=44`, then per link `RECORD=45` + `46` + `48`. The current link has
/// a datafile (`DatafileCount=1` with the path, entity and kind); a name-only
/// link has NO datafile keys at all, and an empty description is omitted.
/// The keys of a footprint link as stored, in order.
fn link_keys(f: &altium_designer_mcp::altium::schlib::FootprintModel) -> Vec<&str> {
    f.raw_params.iter().map(|(k, _)| k.as_str()).collect()
}

#[test]
fn samples_schlib_implchain() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("IMPLCHAIN").expect("IMPLCHAIN symbol not found");
    assert_eq!(sym.footprints.len(), 3, "three footprint links");

    let current = &sym.footprints[0];
    assert_eq!(current.name, "SOIC-8");
    assert_eq!(current.description, "Narrow body");
    assert_eq!(current.library_path.as_deref(), Some("Footprints.PcbLib"));
    assert!(current.is_current, "IsCurrent=T");
    assert!(link_keys(current).contains(&"DatafileCount"));
    assert!(link_keys(current).contains(&"ModelDatafileEntity0"));

    let plain = &sym.footprints[1];
    assert_eq!(plain.name, "SOIC-8-WIDE");
    assert_eq!(plain.description, "");
    assert_eq!(plain.library_path, None);
    assert!(!plain.is_current, "IsCurrent omitted, never F");
    assert_eq!(
        link_keys(plain),
        [
            "RECORD",
            "OwnerIndex",
            "IndexInSheet",
            "ModelName",
            "ModelType",
            "UniqueID"
        ],
        "a name-only link: no datafile group, no Description"
    );

    let described = &sym.footprints[2];
    assert_eq!(described.name, "DIP-8");
    assert_eq!(described.description, "Through-hole");
    assert!(!described.is_current);
    assert!(!link_keys(described).contains(&"DatafileCount"));
    for link in &sym.footprints {
        assert!(link.unique_id.as_ref().is_some_and(|id| id.len() == 8));
    }
}

/// `FRACSHAPES2` (batch 5): the off-grid shapes `FRACSHAPES` did not cover,
/// every coordinate authored at `MilsToCoord(n) + 5000` (+0.05 units). The
/// negatives show AD24's convention again — truncation toward zero with a
/// signed fraction (`Location.X=-14|Location.X_Frac=-95000` = −14.95).
#[test]
#[allow(clippy::too_many_lines)] // one assertion block per off-grid shape kind
fn samples_schlib_fracshapes2() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib
        .get("FRACSHAPES2")
        .expect("FRACSHAPES2 symbol not found");

    assert_eq!(sym.ellipses.len(), 1);
    let ellipse = &sym.ellipses[0];
    assert!(
        approx_eq(ellipse.x, -14.95) && approx_eq(ellipse.y, 10.05),
        "ellipse centre"
    );
    assert!(
        approx_eq(ellipse.radius_x, 3.05) && approx_eq(ellipse.radius_y, 2.05),
        "ellipse radii"
    );

    assert_eq!(sym.pies.len(), 1);
    let pie = &sym.pies[0];
    assert!(
        approx_eq(pie.x, -4.95) && approx_eq(pie.y, 10.05),
        "pie centre"
    );
    assert!(approx_eq(pie.radius, 3.05), "pie radius");
    assert!(approx_eq(pie.start_angle, 30.0) && approx_eq(pie.end_angle, 210.0));

    assert_eq!(sym.round_rects.len(), 1);
    let rr = &sym.round_rects[0];
    assert!(
        approx_eq(rr.x1, 5.05) && approx_eq(rr.y1, 8.05),
        "round-rect corner 1"
    );
    assert!(
        approx_eq(rr.x2, 15.05) && approx_eq(rr.y2, 13.05),
        "round-rect corner 2"
    );
    assert!(
        approx_eq(rr.corner_x_radius, 1.05) && approx_eq(rr.corner_y_radius, 1.05),
        "CornerXRadius_Frac / CornerYRadius_Frac"
    );

    assert_eq!(sym.lines.len(), 1);
    let line = &sym.lines[0];
    assert!(
        approx_eq(line.x1, -14.95) && approx_eq(line.y1, 0.05),
        "line start"
    );
    assert!(
        approx_eq(line.x2, -4.95) && approx_eq(line.y2, 3.05),
        "line end"
    );

    assert_eq!(sym.polylines.len(), 1);
    let pts = &sym.polylines[0].points;
    assert_eq!(pts.len(), 3);
    assert!(
        approx_eq(pts[0].0, -2.95) && approx_eq(pts[0].1, 0.05),
        "X1_Frac/Y1_Frac"
    );
    assert!(
        approx_eq(pts[1].0, 0.05) && approx_eq(pts[1].1, 3.05),
        "frac-only X2"
    );
    assert!(approx_eq(pts[2].0, 3.05) && approx_eq(pts[2].1, 0.05));

    assert_eq!(sym.polygons.len(), 1);
    let pts = &sym.polygons[0].points;
    assert_eq!(pts.len(), 3);
    assert!(approx_eq(pts[0].0, 5.05) && approx_eq(pts[0].1, 0.05));
    assert!(approx_eq(pts[1].0, 15.05) && approx_eq(pts[1].1, 0.05));
    assert!(approx_eq(pts[2].0, 10.05) && approx_eq(pts[2].1, 4.05));

    assert_eq!(sym.beziers.len(), 1);
    let bezier = &sym.beziers[0];
    assert!(
        approx_eq(bezier.x1, -14.95) && approx_eq(bezier.y1, -7.95),
        "bezier p1"
    );
    assert!(
        approx_eq(bezier.x2, -11.95) && approx_eq(bezier.y2, -3.95),
        "bezier p2"
    );
    assert!(
        approx_eq(bezier.x3, -7.95) && approx_eq(bezier.y3, -11.95),
        "bezier p3"
    );
    assert!(
        approx_eq(bezier.x4, -4.95) && approx_eq(bezier.y4, -7.95),
        "bezier p4"
    );

    assert_eq!(sym.labels.len(), 1);
    let label = &sym.labels[0];
    assert!(
        approx_eq(label.x, 5.05) && approx_eq(label.y, -7.95),
        "label anchor"
    );
    assert_eq!(label.text, "FRAC");

    assert_eq!(
        sym.primitive_order,
        [
            SchPrimitiveKind::Ellipse,
            SchPrimitiveKind::Pie,
            SchPrimitiveKind::RoundRect,
            SchPrimitiveKind::Line,
            SchPrimitiveKind::Polyline,
            SchPrimitiveKind::Polygon,
            SchPrimitiveKind::Bezier,
            SchPrimitiveKind::Label,
            SchPrimitiveKind::Parameter,
        ],
        "authoring order, the Comment parameter last"
    );
}

/// `IEEESYM` (batch 6): the first golden `RECORD=3` records, which settled
/// that the record is Altium's IEEE symbol — `Symbol`, `ScaleFactor`,
/// `Orientation`, `Mirror=T`, `Color`, the display flags and no `UniqueID`
/// — and not the text annotation earlier versions of this crate read it as.
#[test]
fn samples_schlib_ieeesym() {
    let lib = SchLib::open(sample("symbols.SchLib")).expect("failed to open symbols.SchLib");
    let sym = lib.get("IEEESYM").expect("IEEESYM symbol not found");
    assert_eq!(sym.ieee_symbols.len(), 3, "three IEEE symbols");
    assert!(sym.labels.is_empty(), "none of them is a text string");

    let dot = &sym.ieee_symbols[0];
    assert_eq!(dot.symbol, 1, "eDot");
    assert!(approx_eq(dot.x, -10.0) && approx_eq(dot.y, 0.0));
    assert!(
        approx_eq(dot.scale_factor, 10.0),
        "MilsToCoord(100) = 10 units"
    );
    assert!(approx_eq(dot.rotation, 0.0) && !dot.is_mirrored);
    assert_eq!(dot.line_width, 1);
    assert_eq!(dot.color, 0);

    let clock = &sym.ieee_symbols[1];
    assert_eq!(clock.symbol, 3, "eClock");
    assert!(approx_eq(clock.rotation, 90.0), "Orientation=1");
    assert!(clock.is_mirrored, "Mirror=T");

    let active_low = &sym.ieee_symbols[2];
    assert_eq!(active_low.symbol, 4, "eActiveLowInput");
    assert!(approx_eq(active_low.x, 10.0));
    assert!(approx_eq(active_low.scale_factor, 20.0));
    assert_eq!(active_low.color, 16_711_680, "$FF0000 is blue in BGR");
    assert!(active_low.display_flags.graphically_locked);

    // Altium gives these records no UniqueID; the stored keys are exactly these.
    let keys: Vec<&str> = active_low
        .raw_params
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "RECORD",
            "IsNotAccesible",
            "IndexInSheet",
            "OwnerPartId",
            "GraphicallyLocked",
            "Symbol",
            "Location.X",
            "ScaleFactor",
            "LineWidth",
            "Color",
        ]
    );
}

/// `manual/pipe.SchLib` (AD24, scripted 2026-08-30): a symbol whose
/// description, parameter name and value, and label were given a `|`
/// through Altium's own API. Altium stores every one as `¦` (U+00A6) — the
/// record format has no escape for its separator — which is why the writer
/// refuses a `|` and points at that character.
#[test]
fn manual_pipe_fixture_shows_altium_stores_a_pipe_as_a_broken_bar() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("samples")
        .join("manual")
        .join("pipe.SchLib");
    let lib = SchLib::open(&path).expect("failed to open manual/pipe.SchLib");
    let sym = lib.get("PIPESYM").expect("symbol PIPESYM not found");
    assert_eq!(sym.description, "A\u{a6}B=C");
    let param = sym
        .parameters
        .iter()
        .find(|p| p.name == "Val\u{a6}ue")
        .expect("parameter Val¦ue not found");
    assert_eq!(param.value, "1\u{a6}2");
    assert_eq!(sym.labels[0].text, "x\u{a6}y");
}

/// `manual/footprint_link.SchLib` (AD24 UI, 2026-09-21): one symbol whose
/// footprint link `R0402` was added through Properties → Footprint → Add, with
/// the PCB library left on "Any". Altium writes the link without
/// `IntegratedModel` or `DatabaseModel` — the flags one corpus link carries do
/// not come from this route — and stores the dialog's status line,
/// `Footprint not found`, as the description.
#[test]
fn samples_schlib_manual_footprint_link_from_the_ui() {
    use std::io::Cursor;

    let lib = SchLib::open(sample("manual/footprint_link.SchLib"))
        .expect("open manual/footprint_link.SchLib");
    let sym = lib.get("Component_1").expect("symbol Component_1");
    assert_eq!(sym.footprints.len(), 1, "one footprint link");
    let link = &sym.footprints[0];
    assert_eq!(link.name, "R0402");
    assert_eq!(link.description, "Footprint not found");
    assert!(link.is_current, "the only link is the current one");
    for flag in ["IntegratedModel", "DatabaseModel"] {
        assert!(
            !link
                .raw_params
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case(flag)),
            "a UI link with the library on Any carries no {flag}"
        );
    }

    // The link survives our own write -> read unchanged.
    let mut buffer = Cursor::new(Vec::new());
    lib.write(&mut buffer).expect("write");
    buffer.set_position(0);
    let back = SchLib::read(&mut buffer).expect("read back");
    let again = &back.get("Component_1").expect("symbol").footprints[0];
    assert_eq!(again.name, link.name);
    assert_eq!(again.description, link.description);
    assert_eq!(again.is_current, link.is_current);
    assert_eq!(again.raw_params, link.raw_params);
}

/// `manual/intlib_link.SchLib`: a symbol copied from the maintainer's own
/// Altium library (github.com/MatejGomboc/altium-library), renamed
/// `INTLIB_LINK` with generic names and descriptions. Its header names an
/// integrated library as its source (`SourceLibraryName`, here
/// `Library.IntLib`), and its footprint link is the only Altium-written one known to carry
/// `IntegratedModel=T|DatabaseModel=T` — flags a link added in the library
/// editor does not get (`manual/footprint_link.SchLib`). The reader keeps
/// them verbatim and the writer puts them back.
#[test]
fn samples_schlib_manual_intlib_sourced_footprint_link() {
    use std::io::Cursor;

    let lib =
        SchLib::open(sample("manual/intlib_link.SchLib")).expect("open manual/intlib_link.SchLib");
    let flags = |lib: &SchLib| -> Vec<(String, String)> {
        let sym = lib.get("INTLIB_LINK").expect("symbol INTLIB_LINK");
        assert_eq!(sym.footprints.len(), 1, "one footprint link");
        let link = &sym.footprints[0];
        assert_eq!(link.name, "PLANE_DIRECT");
        link.raw_params
            .iter()
            .filter(|(k, _)| k == "IntegratedModel" || k == "DatabaseModel")
            .cloned()
            .collect()
    };
    let expected = vec![
        ("IntegratedModel".to_string(), "T".to_string()),
        ("DatabaseModel".to_string(), "T".to_string()),
    ];
    assert_eq!(flags(&lib), expected);

    let mut buffer = Cursor::new(Vec::new());
    lib.write(&mut buffer).expect("write");
    buffer.set_position(0);
    let back = SchLib::read(&mut buffer).expect("read back");
    assert_eq!(flags(&back), expected);
}
