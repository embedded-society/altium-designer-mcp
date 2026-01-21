//! `SchLib` analysis test - run manually to inspect sample file contents.

use altium_designer_mcp::altium::SchLib;

#[test]
#[ignore = "Run manually with: cargo test --test schlib_analysis -- --ignored --nocapture"]
fn analyze_sample_schlib() {
    let lib = SchLib::open("scripts/sample.SchLib").expect("Failed to open sample.SchLib");

    println!("\n=== SchLib Analysis ===");
    println!("Symbols: {}", lib.len());

    for (name, symbol) in lib.iter() {
        println!("\n--- Symbol: {name} ---");
        println!("  Description: {}", symbol.description);
        println!("  Designator: {}", symbol.designator);
        println!("  Part count: {}", symbol.part_count);

        println!("\n  Pins ({}):", symbol.pins.len());
        for pin in &symbol.pins {
            println!(
                "    {} ({}): pos=({}, {}), len={}, orient={:?}, elec={:?}",
                pin.name,
                pin.designator,
                pin.x,
                pin.y,
                pin.length,
                pin.orientation,
                pin.electrical_type
            );
        }

        println!("\n  Rectangles ({}):", symbol.rectangles.len());
        for rect in &symbol.rectangles {
            println!(
                "    ({}, {}) to ({}, {}), line_color={:#08x}, fill_color={:#08x}",
                rect.x1, rect.y1, rect.x2, rect.y2, rect.line_color, rect.fill_color
            );
        }

        println!("\n  Parameters ({}):", symbol.parameters.len());
        for param in &symbol.parameters {
            println!(
                "    {} = '{}' (hidden={})",
                param.name, param.value, param.hidden
            );
        }

        println!("\n  Footprints ({}):", symbol.footprints.len());
        for fp in &symbol.footprints {
            println!("    {} - {}", fp.name, fp.description);
        }
    }
}

#[test]
fn schlib_basic_parsing() {
    let lib = SchLib::open("scripts/sample.SchLib").expect("Failed to open sample.SchLib");

    // Should have 2 symbols (SMD Chip Resistor and NCV8163ASN330T1G)
    assert_eq!(lib.len(), 2);

    // Get the SMD Chip Resistor symbol
    let symbol = lib.get("SMD Chip Resistor").expect("Symbol not found");

    // Should have 2 pins
    assert_eq!(symbol.pins.len(), 2, "Expected 2 pins");

    // Check pin 1
    let pin1 = symbol
        .pins
        .iter()
        .find(|p| p.designator == "1")
        .expect("Pin 1 not found");
    assert_eq!(pin1.name, "1");
    assert_eq!(pin1.x, -10);
    assert_eq!(pin1.y, 0);

    // Check pin 2
    let pin2 = symbol
        .pins
        .iter()
        .find(|p| p.designator == "2")
        .expect("Pin 2 not found");
    assert_eq!(pin2.name, "2");
    assert_eq!(pin2.x, 10);
    assert_eq!(pin2.y, 0);

    // Should have at least 1 rectangle (the body)
    assert!(
        !symbol.rectangles.is_empty(),
        "Expected at least one rectangle"
    );

    // Should have parameters (Value, Part Number, Manufacturer)
    assert!(!symbol.parameters.is_empty(), "Expected parameters");

    // Should have footprint references
    assert!(
        !symbol.footprints.is_empty(),
        "Expected footprint references"
    );

    // Check that we have multiple footprint options
    let footprint_names: Vec<_> = symbol.footprints.iter().map(|f| &f.name).collect();
    assert!(
        footprint_names.iter().any(|n| n.contains("0402")),
        "Expected 0402 footprint"
    );
    assert!(
        footprint_names.iter().any(|n| n.contains("0805")),
        "Expected 0805 footprint"
    );
    assert!(
        footprint_names.iter().any(|n| n.contains("1206")),
        "Expected 1206 footprint"
    );

    // Verify NCV8163ASN330T1G symbol exists and has a Bezier curve
    let ncv_symbol = lib
        .get("NCV8163ASN330T1G")
        .expect("NCV8163ASN330T1G symbol not found");
    assert_eq!(ncv_symbol.pins.len(), 5, "Expected 5 pins");
    assert_eq!(ncv_symbol.beziers.len(), 1, "Expected 1 Bezier curve");

    // Check Bezier curve properties
    let bezier = &ncv_symbol.beziers[0];
    assert_eq!(bezier.x1, -50);
    assert_eq!(bezier.y1, 20);
    assert_eq!(bezier.x4, -40);
    assert_eq!(bezier.y4, 30);

    // Check Polygon
    assert_eq!(ncv_symbol.polygons.len(), 1, "Expected 1 Polygon");
    let polygon = &ncv_symbol.polygons[0];
    assert_eq!(polygon.points.len(), 3, "Expected 3 vertices");
    assert_eq!(polygon.points[0], (-30, 40));
    assert_eq!(polygon.points[1], (-20, 30));
    assert_eq!(polygon.points[2], (-10, 40));

    // Check RoundRect
    assert_eq!(ncv_symbol.round_rects.len(), 1, "Expected 1 RoundRect");
    let round_rect = &ncv_symbol.round_rects[0];
    assert_eq!(round_rect.x1, 40);
    assert_eq!(round_rect.y1, 20);
    assert_eq!(round_rect.x2, 90);
    assert_eq!(round_rect.y2, 50);
    assert_eq!(round_rect.corner_x_radius, 20);
    assert_eq!(round_rect.corner_y_radius, 20);

    // Check EllipticalArc
    assert_eq!(
        ncv_symbol.elliptical_arcs.len(),
        1,
        "Expected 1 EllipticalArc"
    );
    let elliptical_arc = &ncv_symbol.elliptical_arcs[0];
    assert_eq!(elliptical_arc.x, -60);
    assert_eq!(elliptical_arc.y, 0);
    assert!((elliptical_arc.radius - 9.96689).abs() < 0.01);
    assert!((elliptical_arc.secondary_radius - 9.99668).abs() < 0.01);
}
