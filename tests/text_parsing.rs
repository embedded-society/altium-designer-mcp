//! Test for Text primitive parsing.

use altium_designer_mcp::altium::pcblib::PcbLib;

#[test]
#[ignore = "Requires sample.PcbLib with Text records"]
fn test_text_parsing() {
    let lib = PcbLib::read("scripts/sample.PcbLib").expect("Failed to read sample.PcbLib");

    println!("\n=== Testing Text Parsing ===\n");

    for fp in lib.footprints() {
        println!("Footprint: {}", fp.name);
        println!("  Pads: {}", fp.pads.len());
        println!("  Tracks: {}", fp.tracks.len());
        println!("  Arcs: {}", fp.arcs.len());
        println!("  Text: {}", fp.text.len());

        for (i, text) in fp.text.iter().enumerate() {
            println!("    Text[{i}]:");
            println!("      content: '{}'", text.text);
            println!("      position: ({}, {})", text.x, text.y);
            println!("      height: {}", text.height);
            println!("      rotation: {}", text.rotation);
            println!("      layer: {:?}", text.layer);
        }
        println!();
    }
}
