//! Test for `ComponentBody` primitive parsing.

use altium_designer_mcp::altium::pcblib::PcbLib;

#[test]
#[ignore = "Requires sample.PcbLib with ComponentBody records"]
fn test_component_body_parsing() {
    let lib = PcbLib::read("scripts/sample.PcbLib").expect("Failed to read sample.PcbLib");

    println!("\n=== Testing ComponentBody Parsing ===\n");

    let mut total_bodies = 0;
    for fp in lib.footprints() {
        if !fp.component_bodies.is_empty() {
            println!("Footprint: {}", fp.name);
            println!("  ComponentBodies: {}", fp.component_bodies.len());
            total_bodies += fp.component_bodies.len();

            for (body_index, body) in fp.component_bodies.iter().enumerate() {
                println!("    Body[{body_index}]:");
                println!("      model_id: {}", body.model_id);
                println!("      model_name: {}", body.model_name);
                println!("      embedded: {}", body.embedded);
                println!(
                    "      rotation: ({:.3}, {:.3}, {:.3})",
                    body.rotation_x, body.rotation_y, body.rotation_z
                );
                println!("      z_offset: {:.4} mm", body.z_offset);
                println!("      overall_height: {:.4} mm", body.overall_height);
                println!("      standoff_height: {:.4} mm", body.standoff_height);
                println!("      layer: {:?}", body.layer);
            }
            println!();
        }
    }

    println!("Total component bodies found: {total_bodies}");
    assert!(
        total_bodies > 0,
        "Expected at least one component body in sample.PcbLib"
    );
}
