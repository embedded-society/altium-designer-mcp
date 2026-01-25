//! Analysis of real `PcbLib` binary format.
//!
//! This test reads actual Altium `PcbLib` files and dumps their structure
//! to help reverse-engineer the binary format.

use cfb::CompoundFile;
use std::io::Read;
use std::path::Path;

/// Analyzes a `PcbLib` file and prints its structure.
fn analyze_pcblib(path: &Path) {
    println!("\n=== Analyzing: {} ===\n", path.display());

    let file = std::fs::File::open(path).expect("Failed to open file");
    let mut cfb = CompoundFile::open(file).expect("Failed to open OLE file");

    // List all entries
    println!("OLE Structure:");
    let entries: Vec<_> = cfb
        .walk()
        .map(|e| (e.path().to_path_buf(), e.is_stream(), e.len()))
        .collect();

    for (entry_path, is_stream, size) in &entries {
        let type_str = if *is_stream { "STREAM" } else { "STORAGE" };
        println!("  [{type_str}] {} ({} bytes)", entry_path.display(), size);
    }

    // Read FileHeader
    if cfb.is_stream("/FileHeader") {
        let mut stream = cfb.open_stream("/FileHeader").unwrap();
        let mut data = Vec::new();
        stream.read_to_end(&mut data).unwrap();
        println!("\nFileHeader content:");
        if let Ok(text) = String::from_utf8(data.clone()) {
            println!("  {text}");
        } else {
            println!("  (binary, {} bytes)", data.len());
        }
    }

    // Find actual footprint storages (not Library or FileVersionInfo)
    for (entry_path, _is_stream, _) in &entries {
        let path_str = entry_path.to_string_lossy();

        // Skip non-footprint entries
        if path_str == "/"
            || path_str.contains("FileHeader")
            || path_str.contains("Library")
            || path_str.contains("FileVersionInfo")
            || path_str.contains("SectionKeys")
        {
            continue;
        }

        // Check if this is a footprint storage (has Data and Parameters)
        let data_path = entry_path.join("Data");
        let params_path = entry_path.join("Parameters");

        if cfb.is_stream(&data_path) && cfb.is_stream(&params_path) {
            let component_name = entry_path.file_name().unwrap().to_string_lossy();
            println!("\n--- Footprint: {component_name} ---");

            // Read Parameters
            {
                let mut stream = cfb.open_stream(&params_path).unwrap();
                let mut data = Vec::new();
                stream.read_to_end(&mut data).unwrap();
                if let Ok(text) = String::from_utf8(data) {
                    println!("Parameters: {}", text.trim());
                }
            }

            // Read and analyse Data stream
            let mut stream = cfb.open_stream(&data_path).unwrap();
            let mut data = Vec::new();
            stream.read_to_end(&mut data).unwrap();

            println!("Data stream: {} bytes", data.len());
            analyze_data_stream(&data);

            // Only analyse first footprint for brevity
            break;
        }
    }
}

/// Analyses the binary Data stream to understand the record format.
fn analyze_data_stream(data: &[u8]) {
    println!("\nFirst 256 bytes (hex dump):");
    for (i, chunk) in data
        .iter()
        .take(256)
        .collect::<Vec<_>>()
        .chunks(16)
        .enumerate()
    {
        print!("  {:04x}: ", i * 16);
        for byte in chunk {
            print!("{byte:02x} ");
        }
        // ASCII representation
        print!(" |");
        for byte in chunk {
            let c = **byte as char;
            if c.is_ascii_graphic() || c == ' ' {
                print!("{c}");
            } else {
                print!(".");
            }
        }
        println!("|");
    }

    // Try to find record structure
    println!("\nAttempting to parse records:");
    parse_records(data);
}

/// Attempts to parse records from the Data stream.
fn parse_records(data: &[u8]) {
    // Based on pyAltiumLib documentation:
    // - Data stream starts with a header containing component name
    // - Format: [total_len:4][name_len:1][name:name_len][records...]
    // - Record types: 1=Arc, 2=Pad, 4=Track, 11=Region
    // - Each record has: [record_type:1][block_len:4][block_data:block_len]

    if data.len() < 5 {
        println!("  (data stream too short)");
        return;
    }

    println!("  First 16 bytes: {:02x?}", &data[..data.len().min(16)]);

    // Parse header
    let header_block_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    println!("  Header block length: {header_block_len} bytes");

    if header_block_len + 4 > data.len() {
        println!("  Header length exceeds data size!");
        find_patterns(data);
        return;
    }

    // After header length, we have: [name_len:1][name:name_len]
    let name_len = data[4] as usize;
    if name_len + 5 <= header_block_len + 4 {
        if let Ok(name) = std::str::from_utf8(&data[5..5 + name_len]) {
            println!("  Component name: {name} ({name_len} chars)");
        }
    }

    // Records start after the header block
    let mut offset = 4 + header_block_len;
    let mut record_count = 0;

    println!("\nParsing primitive records:");

    while offset + 5 <= data.len() && record_count < 50 {
        let record_type = data[offset];

        // Record format: [type:1][len:4][data:len]
        let block_len = u32::from_le_bytes([
            data.get(offset + 1).copied().unwrap_or(0),
            data.get(offset + 2).copied().unwrap_or(0),
            data.get(offset + 3).copied().unwrap_or(0),
            data.get(offset + 4).copied().unwrap_or(0),
        ]) as usize;

        // Validate
        if block_len == 0 || block_len > 100_000 || offset + 5 + block_len > data.len() {
            println!(
                "  Invalid block at 0x{offset:04x}: type=0x{record_type:02x}, len={block_len}"
            );
            break;
        }

        let record_type_name = match record_type {
            0x01 => "Arc",
            0x02 => "Pad",
            0x03 => "Via",
            0x04 => "Track",
            0x05 => "Text",
            0x06 => "Fill",
            0x0B => "Region",
            0x0C => "ComponentBody",
            _ => "Unknown",
        };

        println!(
            "  [{record_count}] Record at 0x{offset:04x}: type={record_type_name} \
             (0x{record_type:02x}), block_len={block_len} bytes"
        );

        // For pads, try to extract some info
        if record_type == 0x02 && block_len > 20 {
            parse_pad_block(&data[offset + 5..offset + 5 + block_len]);
        }

        // For tracks, try to extract coordinates
        if record_type == 0x04 && block_len > 30 {
            parse_track_block(&data[offset + 5..offset + 5 + block_len]);
        }

        offset += 5 + block_len;
        record_count += 1;
    }

    println!("\n  Total records found: {record_count}");
    println!("  Remaining bytes: {}", data.len().saturating_sub(offset));
}

/// Parse a pad block to extract designator and position.
fn parse_pad_block(data: &[u8]) {
    // Pad format: [designator_len:4][designator_string][more blocks...]
    if data.len() < 5 {
        return;
    }

    let des_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if des_len < 100 && des_len + 4 < data.len() {
        // Designator is stored with length byte prefix
        let actual_len = data[4] as usize;
        if actual_len + 5 <= data.len() {
            if let Ok(des) = std::str::from_utf8(&data[5..5 + actual_len]) {
                println!("      Designator: '{des}'");
            }
        }
    }

    // Try to find pipe-separated properties
    if let Some(pipe_pos) = data.iter().position(|&b| b == b'|') {
        let end_pos = data[pipe_pos..]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(data.len() - pipe_pos);
        if let Ok(props) = std::str::from_utf8(&data[pipe_pos..pipe_pos + end_pos]) {
            if !props.is_empty() && props.len() < 200 {
                println!("      Properties: {}", props.trim());
            }
        }
    }
}

/// Parse a track block to extract coordinates.
fn parse_track_block(data: &[u8]) {
    // Track format: 13-byte header, then two 8-byte coordinate pairs, 4-byte width
    // But first there's a length-prefixed block

    if data.len() < 45 {
        return;
    }

    // Skip the initial block structure and look for coordinate-like values
    // Altium uses internal units (10000 units = 1 inch = 25.4mm)

    // Try to find doubles after the 13-byte header
    let header_offset = 4; // Skip block length
    if header_offset + 13 + 32 <= data.len() {
        let coord_offset = header_offset + 13;

        // Try reading as little-endian i32 (internal units)
        // X1, Y1 at coord_offset
        // X2, Y2 at coord_offset + 8
        // Width at coord_offset + 16

        // Actually coordinates might be stored as doubles (8 bytes each)
        // Let's try both

        println!("      Track data (raw bytes at coord region):");
        println!(
            "      {:02x?}",
            &data[coord_offset..data.len().min(coord_offset + 40)]
        );
    }
}

/// Look for patterns in the binary data.
#[allow(clippy::cast_precision_loss)]
fn find_patterns(data: &[u8]) {
    // Look for repeated sequences or known markers
    println!("\n  Pattern analysis:");

    // Count byte frequency
    let mut freq = [0usize; 256];
    for &b in data {
        freq[b as usize] += 1;
    }

    // Most common bytes
    let mut sorted: Vec<_> = freq.iter().enumerate().filter(|(_, &c)| c > 0).collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    println!("  Most common bytes:");
    for (byte, count) in sorted.iter().take(10) {
        let pct = **count as f64 / data.len() as f64 * 100.0;
        println!("    0x{byte:02x}: {count} times ({pct:.1}%)");
    }

    // Look for 4-byte aligned structures (common in binary formats)
    println!("\n  Looking for floating-point values (pad positions/sizes):");
    for i in (0..data.len().saturating_sub(8)).step_by(4).take(20) {
        let raw = i32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        let val = f64::from(raw);
        // Altium uses mils (1/1000 inch) internally
        let mm = val / 10000.0 * 25.4; // Convert from internal units to mm
        if mm.abs() < 100.0 && mm.abs() > 0.001 {
            println!("    Offset 0x{i:04x}: raw={raw}, mm={mm:.4}");
        }
    }
}

#[test]
#[ignore = "Run manually with: cargo test --test pcblib_analysis -- --ignored --nocapture"]
fn analyze_sample_pcblib() {
    // Use sample file in scripts folder (relative to project root)
    let sample_path = Path::new("scripts/sample.PcbLib");

    if sample_path.exists() {
        analyze_pcblib(sample_path);
    } else {
        println!("Sample file not found: {}", sample_path.display());
        println!("Place a .PcbLib file at scripts/sample.PcbLib to run this analysis.");
    }
}
