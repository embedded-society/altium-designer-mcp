//! Test for Region primitive parsing.

use altium_designer_mcp::altium::pcblib::PcbLib;

#[test]
#[ignore = "Requires sample.PcbLib with Region records"]
fn test_region_parsing() {
    let lib = PcbLib::read("scripts/sample.PcbLib").expect("Failed to read sample.PcbLib");

    println!("\n=== Testing Region Parsing ===\n");

    let mut total_regions = 0;
    for fp in lib.footprints() {
        if !fp.regions.is_empty() {
            println!("Footprint: {}", fp.name);
            println!("  Regions: {}", fp.regions.len());
            total_regions += fp.regions.len();

            for (i, region) in fp.regions.iter().enumerate() {
                println!("    Region[{i}]:");
                println!("      layer: {:?}", region.layer);
                println!("      vertices: {}", region.vertices.len());
                for (j, v) in region.vertices.iter().enumerate() {
                    println!("        [{j}] ({:.3}, {:.3})", v.x, v.y);
                }
            }
            println!();
        }
    }

    println!("Total regions found: {total_regions}");
    assert!(
        total_regions > 0,
        "Expected at least one region in sample.PcbLib"
    );
}
