//! Binary writer for `SchLib` Data streams.
//!
//! This module handles encoding symbol primitives to the binary format
//! used in Altium `SchLib` Data streams.
//!
//! # Data Stream Format
//!
//! ```text
//! [RecordLength:2 LE][RecordType:2 BE][data:RecordLength]
//! ...
//! [0x00 0x00]  // End marker (length = 0)
//! ```
//!
//! Record types:
//! - `0x0000`: Text record (pipe-delimited key=value pairs)
//! - `0x0001`: Binary pin record

use super::primitives::{
    Arc, Ellipse, FootprintModel, Label, Line, Parameter, Pin, Polyline, Rectangle,
    TextJustification,
};
use super::Symbol;

/// Writes a text record (type 0) to the output.
fn write_text_record(data: &mut Vec<u8>, content: &str) {
    let content_bytes = content.as_bytes();
    let mut record = Vec::with_capacity(content_bytes.len() + 1);
    record.extend_from_slice(content_bytes);
    record.push(0x00); // Null terminator

    // Header: [length:2 LE][type:2 BE]
    #[allow(clippy::cast_possible_truncation)]
    let length = record.len() as u16;
    data.extend_from_slice(&length.to_le_bytes());
    data.extend_from_slice(&0u16.to_be_bytes()); // Type 0 = text
    data.extend_from_slice(&record);
}

/// Writes a binary pin record (type 1) to the output.
fn write_binary_pin(data: &mut Vec<u8>, pin: &Pin) {
    let mut record = Vec::with_capacity(64);

    // Record type (4 bytes) - always 2 for pin
    record.extend_from_slice(&2i32.to_le_bytes());

    // Unknown byte
    record.push(0x00);

    // Owner part ID (2 bytes)
    #[allow(clippy::cast_possible_truncation)]
    let owner_part = pin.owner_part_id as i16;
    record.extend_from_slice(&owner_part.to_le_bytes());

    // Owner part display mode (1 byte)
    record.push(0x00);

    // Symbol flags (4 bytes: inner_edge, outer_edge, inside, outside)
    record.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

    // Description: [length:1][unknown:1][string]
    let desc_bytes = pin.description.as_bytes();
    #[allow(clippy::cast_possible_truncation)]
    record.push(desc_bytes.len() as u8);
    record.push(0x00); // Unknown byte
    record.extend_from_slice(desc_bytes);

    // Electrical type (1 byte)
    record.push(pin.electrical_type.to_id());

    // Flags (1 byte)
    let (rotated, flipped) = pin.orientation.to_flags();
    let mut flags: u8 = 0;
    if rotated {
        flags |= 0x01;
    }
    if flipped {
        flags |= 0x02;
    }
    if pin.hidden {
        flags |= 0x04;
    }
    if pin.show_name {
        flags |= 0x08;
    }
    if pin.show_designator {
        flags |= 0x10;
    }
    record.push(flags);

    // Length (2 bytes)
    #[allow(clippy::cast_possible_truncation)]
    let length = pin.length as i16;
    record.extend_from_slice(&length.to_le_bytes());

    // Location X, Y (2 bytes each, signed)
    #[allow(clippy::cast_possible_truncation)]
    let x = pin.x as i16;
    #[allow(clippy::cast_possible_truncation)]
    let y = pin.y as i16;
    record.extend_from_slice(&x.to_le_bytes());
    record.extend_from_slice(&y.to_le_bytes());

    // Colour (4 bytes) - default black
    record.extend_from_slice(&0u32.to_le_bytes());

    // Name: [length:1][string]
    let name_bytes = pin.name.as_bytes();
    #[allow(clippy::cast_possible_truncation)]
    record.push(name_bytes.len() as u8);
    record.extend_from_slice(name_bytes);

    // Designator: [length:1][string]
    let desig_bytes = pin.designator.as_bytes();
    #[allow(clippy::cast_possible_truncation)]
    record.push(desig_bytes.len() as u8);
    record.extend_from_slice(desig_bytes);

    // Header: [length:2 LE][type:2 BE]
    #[allow(clippy::cast_possible_truncation)]
    let record_length = record.len() as u16;
    data.extend_from_slice(&record_length.to_le_bytes());
    data.extend_from_slice(&1u16.to_be_bytes()); // Type 1 = binary pin
    data.extend_from_slice(&record);
}

/// Encodes a component header record.
fn encode_component_header(symbol: &Symbol) -> String {
    let parts = vec![
        "RECORD=1".to_string(),
        format!("LibReference={}", symbol.name),
        format!("ComponentDescription={}", symbol.description),
        format!("PartCount={}", symbol.part_count + 1), // Altium uses part_count + 1
        "DisplayModeCount=1".to_string(),
        "IndexInSheet=-1".to_string(),
        "OwnerPartId=-1".to_string(),
        "CurrentPartId=1".to_string(),
        "SourceLibraryName=*".to_string(),
        "TargetFileName=*".to_string(),
        format!("AllPinCount={}", symbol.pins.len()),
        "AreaColor=11599871".to_string(), // Light yellow fill
        "Color=128".to_string(),          // Dark red outline
        "PartIDLocked=F".to_string(),
    ];

    format!("|{}|", parts.join("|"))
}

/// Encodes a rectangle record.
fn encode_rectangle(rect: &Rectangle, index: usize) -> String {
    format!(
        "|RECORD=14|IndexInSheet={}|OwnerPartId={}|IsNotAccesible=T\
         |Location.X={}|Location.Y={}|Corner.X={}|Corner.Y={}\
         |LineWidth={}|Color={}|AreaColor={}|IsSolid=T|UniqueID={}|",
        index,
        rect.owner_part_id,
        rect.x1,
        rect.y1,
        rect.x2,
        rect.y2,
        rect.line_width,
        rect.line_color,
        rect.fill_color,
        generate_unique_id()
    )
}

/// Encodes a line record.
fn encode_line(line: &Line, index: usize) -> String {
    format!(
        "|RECORD=13|IndexInSheet={}|OwnerPartId={}|Location.X={}|Location.Y={}|Corner.X={}|Corner.Y={}|LineWidth={}|Color={}|UniqueID={}|",
        index,
        line.owner_part_id,
        line.x1,
        line.y1,
        line.x2,
        line.y2,
        line.line_width,
        line.color,
        generate_unique_id()
    )
}

/// Encodes a parameter record.
fn encode_parameter(param: &Parameter, index: usize) -> String {
    let hidden = if param.hidden { "T" } else { "F" };
    format!(
        "|RECORD=41|IndexInSheet={}|OwnerPartId={}|Location.X={}|Location.Y={}|Color={}|FontID={}|IsHidden={}|Text={}|Name={}|UniqueID={}|",
        index,
        param.owner_part_id,
        param.x,
        param.y,
        param.color,
        param.font_id,
        hidden,
        param.value,
        param.name,
        generate_unique_id()
    )
}

/// Encodes a designator record.
fn encode_designator(designator: &str) -> String {
    format!(
        "|RECORD=34|IndexInSheet=-1|OwnerPartId=-1|Location.Y=-6|Color=8388608|FontID=1|Text={}|Name=Designator|ReadOnlyState=1|UniqueID={}|",
        designator,
        generate_unique_id()
    )
}

/// Encodes a polyline record.
fn encode_polyline(polyline: &Polyline, index: usize) -> String {
    let mut parts = vec![
        "RECORD=6".to_string(),
        format!("IndexInSheet={index}"),
        format!("OwnerPartId={}", polyline.owner_part_id),
        format!("LineWidth={}", polyline.line_width),
        format!("Color={}", polyline.color),
        format!("LocationCount={}", polyline.points.len()),
    ];

    for (i, (x, y)) in polyline.points.iter().enumerate() {
        parts.push(format!("X{}={}", i + 1, x));
        parts.push(format!("Y{}={}", i + 1, y));
    }

    parts.push(format!("UniqueID={}", generate_unique_id()));

    format!("|{}|", parts.join("|"))
}

/// Encodes an arc record.
fn encode_arc(arc: &Arc, index: usize) -> String {
    format!(
        "|RECORD=12|IndexInSheet={}|OwnerPartId={}|Location.X={}|Location.Y={}|Radius={}|StartAngle={}|EndAngle={}|LineWidth={}|Color={}|UniqueID={}|",
        index,
        arc.owner_part_id,
        arc.x,
        arc.y,
        arc.radius,
        arc.start_angle,
        arc.end_angle,
        arc.line_width,
        arc.color,
        generate_unique_id()
    )
}

/// Encodes an ellipse record.
fn encode_ellipse(ellipse: &Ellipse, index: usize) -> String {
    let is_solid = if ellipse.filled { "T" } else { "F" };
    format!(
        "|RECORD=8|IndexInSheet={}|OwnerPartId={}|Location.X={}|Location.Y={}|Radius={}|SecondaryRadius={}|LineWidth={}|Color={}|AreaColor={}|IsSolid={}|UniqueID={}|",
        index,
        ellipse.owner_part_id,
        ellipse.x,
        ellipse.y,
        ellipse.radius_x,
        ellipse.radius_y,
        ellipse.line_width,
        ellipse.line_color,
        ellipse.fill_color,
        is_solid,
        generate_unique_id()
    )
}

/// Encodes a label record.
fn encode_label(label: &Label, index: usize) -> String {
    #[allow(clippy::cast_possible_truncation)]
    let orientation = (label.rotation / 90.0).round() as i32 % 4;
    let justification = justification_to_id(label.justification);
    format!(
        "|RECORD=4|IndexInSheet={}|OwnerPartId={}|Location.X={}|Location.Y={}|Color={}|FontID={}|Orientation={}|Justification={}|Text={}|UniqueID={}|",
        index,
        label.owner_part_id,
        label.x,
        label.y,
        label.color,
        label.font_id,
        orientation,
        justification,
        label.text,
        generate_unique_id()
    )
}

/// Converts `TextJustification` to Altium ID.
const fn justification_to_id(justification: TextJustification) -> u8 {
    match justification {
        TextJustification::BottomLeft => 0,
        TextJustification::BottomCenter => 1,
        TextJustification::BottomRight => 2,
        TextJustification::MiddleLeft => 3,
        TextJustification::MiddleCenter => 4,
        TextJustification::MiddleRight => 5,
        TextJustification::TopLeft => 6,
        TextJustification::TopCenter => 7,
        TextJustification::TopRight => 8,
    }
}

/// Encodes an implementation list record (start of model list).
fn encode_implementation_list() -> String {
    "|RECORD=44|OwnerIndex=0|".to_string()
}

/// Encodes a footprint model record.
fn encode_footprint_model(model: &FootprintModel, owner_index: usize) -> String {
    format!(
        "|RECORD=45|OwnerIndex={}|Description={}|ModelName={}|ModelType=PCBLIB|DatafileCount=0|UniqueID={}|",
        owner_index,
        model.description,
        model.name,
        generate_unique_id()
    )
}

/// Generates a random 8-character unique ID (similar to Altium's format).
fn generate_unique_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    // Simple pseudo-random based on time
    let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars().collect();
    let mut id = String::with_capacity(8);
    let mut n = seed;
    for _ in 0..8 {
        #[allow(clippy::cast_possible_truncation)]
        let idx = (n % 26) as usize;
        id.push(chars[idx]);
        n = n.wrapping_mul(1_103_515_245).wrapping_add(12345);
    }
    id
}

/// Encodes symbol primitives to binary format for the Data stream.
#[must_use]
pub fn encode_data_stream(symbol: &Symbol) -> Vec<u8> {
    let mut data = Vec::new();
    let mut index_counter = 0usize;

    // 1. Component header
    let header = encode_component_header(symbol);
    write_text_record(&mut data, &header);

    // 2. Parameters (Value, Part Number, etc.)
    for param in &symbol.parameters {
        let record = encode_parameter(param, index_counter);
        write_text_record(&mut data, &record);
        index_counter += 1;
    }

    // 3. Pins (binary format)
    for pin in &symbol.pins {
        write_binary_pin(&mut data, pin);
    }

    // 4. Rectangles
    for rect in &symbol.rectangles {
        let record = encode_rectangle(rect, index_counter);
        write_text_record(&mut data, &record);
        index_counter += 1;
    }

    // 5. Lines
    for line in &symbol.lines {
        let record = encode_line(line, index_counter);
        write_text_record(&mut data, &record);
        index_counter += 1;
    }

    // 6. Polylines
    for polyline in &symbol.polylines {
        let record = encode_polyline(polyline, index_counter);
        write_text_record(&mut data, &record);
        index_counter += 1;
    }

    // 7. Arcs
    for arc in &symbol.arcs {
        let record = encode_arc(arc, index_counter);
        write_text_record(&mut data, &record);
        index_counter += 1;
    }

    // 8. Ellipses
    for ellipse in &symbol.ellipses {
        let record = encode_ellipse(ellipse, index_counter);
        write_text_record(&mut data, &record);
        index_counter += 1;
    }

    // 9. Labels
    for label in &symbol.labels {
        let record = encode_label(label, index_counter);
        write_text_record(&mut data, &record);
        index_counter += 1;
    }

    // 10. Designator
    if !symbol.designator.is_empty() {
        let record = encode_designator(&symbol.designator);
        write_text_record(&mut data, &record);
    }

    // 10. Implementation list (if we have footprints)
    if !symbol.footprints.is_empty() {
        let impl_list = encode_implementation_list();
        write_text_record(&mut data, &impl_list);

        // 11. Footprint models
        for (i, model) in symbol.footprints.iter().enumerate() {
            let record = encode_footprint_model(model, i);
            write_text_record(&mut data, &record);
        }
    }

    // End marker: length = 0
    data.extend_from_slice(&0u16.to_le_bytes());

    data
}

/// Encodes the `FileHeader` stream content.
#[must_use]
pub fn encode_file_header(symbols: &[&Symbol]) -> Vec<u8> {
    let mut parts = vec![
        "HEADER=Protel for Windows - Schematic Library Editor Binary File Version 5.0".to_string(),
        "Weight=47".to_string(),
        "MinorVersion=9".to_string(),
        format!("UniqueID={}", generate_unique_id()),
        "FontIdCount=1".to_string(),
        "Size1=10".to_string(),
        "FontName1=Times New Roman".to_string(),
        "UseMBCS=T".to_string(),
        "IsBOC=T".to_string(),
        "SheetStyle=9".to_string(),
        "BorderOn=T".to_string(),
        "SheetNumberSpaceSize=12".to_string(),
        "AreaColor=16317695".to_string(),
        "SnapGridOn=T".to_string(),
        "SnapGridSize=10".to_string(),
        "VisibleGridOn=T".to_string(),
        "VisibleGridSize=10".to_string(),
        "CustomX=18000".to_string(),
        "CustomY=18000".to_string(),
        "UseCustomSheet=T".to_string(),
        "ReferenceZonesOn=T".to_string(),
        "Display_Unit=0".to_string(),
        format!("CompCount={}", symbols.len()),
    ];

    // Add component references
    for (i, symbol) in symbols.iter().enumerate() {
        parts.push(format!("LibRef{}={}", i, symbol.name));
        parts.push(format!("CompDescr{}={}", i, symbol.description));
        parts.push(format!("PartCount{}={}", i, symbol.part_count + 1));
    }

    let text = format!("|{}|", parts.join("|"));
    let text_bytes = text.as_bytes();

    // Format: [length:4 LE][text]
    let mut data = Vec::with_capacity(4 + text_bytes.len());
    #[allow(clippy::cast_possible_truncation)]
    let length = text_bytes.len() as u32;
    data.extend_from_slice(&length.to_le_bytes());
    data.extend_from_slice(text_bytes);

    data
}

#[cfg(test)]
mod tests {
    use super::super::primitives::PinOrientation;
    use super::*;

    #[test]
    fn test_write_text_record() {
        let mut data = Vec::new();
        write_text_record(&mut data, "|RECORD=1|Name=Test|");

        // Check header
        let length = u16::from_le_bytes([data[0], data[1]]);
        let record_type = u16::from_be_bytes([data[2], data[3]]);

        assert_eq!(length, 21); // "|RECORD=1|Name=Test|" + null
        assert_eq!(record_type, 0); // Text record
    }

    #[test]
    fn test_encode_simple_symbol() {
        let mut symbol = Symbol::new("TEST");
        symbol.description = "Test symbol".to_string();
        symbol.designator = "U?".to_string();
        symbol.add_pin(Pin::new("IN", "1", -10, 0, 10, PinOrientation::Right));
        symbol.add_rectangle(Rectangle::new(-5, -5, 5, 5));

        let data = encode_data_stream(&symbol);

        // Should have content
        assert!(!data.is_empty());

        // Should end with 0x00 0x00
        let len = data.len();
        assert_eq!(data[len - 2], 0x00);
        assert_eq!(data[len - 1], 0x00);
    }

    #[test]
    fn test_encode_file_header() {
        let symbol = Symbol::new("TEST_SYMBOL");
        let symbols = vec![&symbol];

        let data = encode_file_header(&symbols);

        // Should start with length
        assert!(data.len() > 4);
        let length = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        assert_eq!(data.len(), 4 + length);

        // Should contain component info
        let text = String::from_utf8_lossy(&data[4..]);
        assert!(text.contains("HEADER="));
        assert!(text.contains("CompCount=1"));
        assert!(text.contains("LibRef0=TEST_SYMBOL"));
    }
}
