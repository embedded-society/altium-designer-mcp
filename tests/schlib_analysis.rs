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

    // Should have exactly 1 symbol
    assert_eq!(lib.len(), 1);

    // Get the symbol
    let symbol = lib.get("SMD Chip Resistor").expect("Symbol not found");

    // Should have 2 pins
    assert_eq!(symbol.pins.len(), 2, "Expected 2 pins");

    // Check pin 1
    let pin1 = symbol.pins.iter().find(|p| p.designator == "1").expect("Pin 1 not found");
    assert_eq!(pin1.name, "1");
    assert_eq!(pin1.x, -10);
    assert_eq!(pin1.y, 0);

    // Check pin 2
    let pin2 = symbol.pins.iter().find(|p| p.designator == "2").expect("Pin 2 not found");
    assert_eq!(pin2.name, "2");
    assert_eq!(pin2.x, 10);
    assert_eq!(pin2.y, 0);

    // Should have at least 1 rectangle (the body)
    assert!(!symbol.rectangles.is_empty(), "Expected at least one rectangle");

    // Should have parameters (Value, Part Number, Manufacturer)
    assert!(!symbol.parameters.is_empty(), "Expected parameters");

    // Should have footprint references
    assert!(!symbol.footprints.is_empty(), "Expected footprint references");

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
}
