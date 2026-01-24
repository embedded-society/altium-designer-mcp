//! Test for embedded 3D model parsing.

use altium_designer_mcp::altium::pcblib::PcbLib;

#[test]
#[ignore = "Requires sample.PcbLib with embedded 3D models"]
fn test_model_parsing() {
    let lib_path = "scripts/sample.PcbLib";
    let lib = PcbLib::read(lib_path).expect("Failed to read sample.PcbLib");

    println!("\n=== Testing 3D Model Parsing ===\n");

    let model_count = lib.model_count();
    println!("Embedded 3D models: {model_count}");

    for model in lib.models() {
        println!("  Model ID: {}", model.id);
        println!("    Name: {}", model.name);
        println!("    Data size: {} bytes", model.size());
        println!("    Compressed size: {} bytes", model.compressed_size);

        // Check if it looks like a STEP file
        if let Some(text) = model.as_string() {
            if text.starts_with("ISO-10303") {
                println!("    Format: STEP (ISO 10303)");
            }
        }
        println!();
    }

    // Check ComponentBody references
    println!("\n=== ComponentBody to Model Mapping ===\n");
    for fp in lib.footprints() {
        if !fp.component_bodies.is_empty() {
            println!("Footprint: {}", fp.name);
            for body in &fp.component_bodies {
                println!("  ComponentBody model_id: {}", body.model_id);
                println!("    model_name: {}", body.model_name);
                println!("    embedded: {}", body.embedded);

                // Check if model is available
                if lib.get_model(&body.model_id).is_some() {
                    println!("    => Model found in library!");
                } else {
                    println!("    => Model NOT found in library");
                }
            }
        }
    }
}

#[test]
fn test_model_parsing_missing_file() {
    // This test ensures that the PcbLib::read API behaves sensibly when the
    // requested file is not present. It does not rely on any external files
    // and therefore can run in CI/CD environments.
    let result = PcbLib::read("nonexistent_sample.PcbLib");
    assert!(
        result.is_err(),
        "PcbLib::read should fail when the input file does not exist"
    );
}
