//! Test for Fill primitive parsing.

use altium_designer_mcp::altium::pcblib::PcbLib;

#[test]
#[ignore = "Requires sample.PcbLib with Fill records"]
fn test_fill_parsing() {
    let lib = PcbLib::open("scripts/sample.PcbLib").expect("Failed to read sample.PcbLib");

    println!("\n=== Testing Fill Parsing ===\n");

    let mut total_fills = 0;
    for fp in lib.iter() {
        if !fp.fills.is_empty() {
            println!("Footprint: {}", fp.name);
            println!("  Fills: {}", fp.fills.len());
            total_fills += fp.fills.len();

            for (i, fill) in fp.fills.iter().enumerate() {
                println!("    Fill[{i}]:");
                println!("      layer: {:?}", fill.layer);
                println!(
                    "      corners: ({:.3}, {:.3}) to ({:.3}, {:.3})",
                    fill.x1, fill.y1, fill.x2, fill.y2
                );
                println!("      rotation: {:.1}°", fill.rotation);
            }
            println!();
        }
    }

    println!("Total fills found: {total_fills}");
    assert!(
        total_fills > 0,
        "Expected at least one fill in sample.PcbLib"
    );
}
