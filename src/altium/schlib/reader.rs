//! Binary reader for `SchLib` Data streams.
//!
//! This module handles parsing the binary format of Altium `SchLib` Data streams,
//! which contain the primitives (pins, rectangles, lines, etc.) that make up symbols.
//!
//! # Data Stream Format
//!
//! ```text
//! [RecordLength:2 LE][RecordType:2 BE][data:RecordLength]
//! ...
//! [0x00 0x00]  // End marker (length = 0)
//! ```
//!
//! # Record Types
//!
//! - `0x0000`: Text record (pipe-delimited key=value)
//! - `0x0001`: Binary pin record

use super::primitives::{
    Arc, Ellipse, FootprintModel, Line, Parameter, Pin, PinElectricalType, PinOrientation,
    Polyline, Rectangle,
};
use super::Symbol;
use std::collections::HashMap;

/// Parses primitives from a `SchLib` Data stream.
pub fn parse_data_stream(symbol: &mut Symbol, data: &[u8]) {
    if data.len() < 4 {
        tracing::warn!("Data stream too short");
        return;
    }

    let mut offset = 0;

    // Parse records until end marker or end of data
    while offset + 4 <= data.len() {
        // Read header: [length:2 LE][type:2 BE]
        let record_length = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        let record_type = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);

        if record_length == 0 {
            // End marker
            break;
        }

        if offset + 4 + record_length > data.len() {
            tracing::warn!("Record extends beyond data at offset {offset:#x}");
            break;
        }

        let record_data = &data[offset + 4..offset + 4 + record_length];

        match record_type {
            0 => {
                // Text record (pipe-delimited key=value)
                parse_text_record(symbol, record_data);
            }
            1 => {
                // Binary pin record
                if let Some(pin) = parse_binary_pin(record_data) {
                    symbol.add_pin(pin);
                }
            }
            _ => {
                tracing::debug!("Unknown record type {record_type:#x} at offset {offset:#x}");
            }
        }

        offset += 4 + record_length;
    }
}

/// Parses a text record (pipe-delimited key=value pairs).
fn parse_text_record(symbol: &mut Symbol, data: &[u8]) {
    let text = if let Ok(s) = std::str::from_utf8(data) {
        s.trim_end_matches('\0')
    } else {
        // Try Windows-1252 encoding
        let decoded: String = data
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect();
        return parse_text_record_from_string(symbol, &decoded);
    };

    parse_text_record_from_string(symbol, text);
}

/// Parses a text record from a decoded string.
fn parse_text_record_from_string(symbol: &mut Symbol, text: &str) {
    let props = parse_properties(text);

    let record_id: u32 = props
        .get("record")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    match record_id {
        1 => {
            // Component header
            if let Some(desc) = props.get("componentdescription") {
                symbol.description.clone_from(desc);
            }
            if let Some(part_count) = props.get("partcount") {
                // Altium stores part_count + 1, so we subtract 1 when reading
                let raw_count: u32 = part_count.trim().parse().unwrap_or(2);
                symbol.part_count = raw_count.saturating_sub(1).max(1);
            }
        }
        14 => {
            // Rectangle
            if let Some(rect) = parse_rectangle(&props) {
                symbol.add_rectangle(rect);
            }
        }
        13 => {
            // Line
            if let Some(line) = parse_line(&props) {
                symbol.add_line(line);
            }
        }
        34 => {
            // Designator
            if let Some(text) = props.get("text") {
                symbol.designator.clone_from(text);
            }
        }
        41 => {
            // Parameter
            if let Some(param) = parse_parameter(&props) {
                symbol.add_parameter(param);
            }
        }
        45 => {
            // Model (footprint reference)
            if let Some(name) = props.get("modelname") {
                let mut fp = FootprintModel::new(name);
                if let Some(desc) = props.get("description") {
                    fp.description.clone_from(desc);
                }
                symbol.add_footprint(fp);
            }
        }
        6 => {
            // Polyline
            if let Some(polyline) = parse_polyline(&props) {
                symbol.add_polyline(polyline);
            }
        }
        8 => {
            // Ellipse
            if let Some(ellipse) = parse_ellipse(&props) {
                symbol.add_ellipse(ellipse);
            }
        }
        12 => {
            // Arc
            if let Some(arc) = parse_arc(&props) {
                symbol.add_arc(arc);
            }
        }
        2 | 4 | 5 | 7 | 10 | 11 | 44 | 46 | 47 | 48 => {
            // Known but not yet implemented:
            // 2=Pin(text), 4=Label, 5=Bezier, 7=Polygon,
            // 10=RoundRect, 11=EllipticalArc,
            // 44=ImplementationList, 46/47/48=Model data
            tracing::trace!("Skipping record type {record_id}");
        }
        _ => {
            tracing::debug!("Unknown text record type {record_id}");
        }
    }
}

/// Parses pipe-delimited key=value properties.
fn parse_properties(text: &str) -> HashMap<String, String> {
    let mut props = HashMap::new();

    for part in text.split('|') {
        if let Some((key, value)) = part.split_once('=') {
            props.insert(key.to_lowercase(), value.to_string());
        }
    }

    props
}

/// Parses a binary pin record.
fn parse_binary_pin(data: &[u8]) -> Option<Pin> {
    if data.len() < 20 {
        return None;
    }

    let mut offset = 0;

    // record type (4 bytes) - should be 2 for pin
    let _record_type = read_i32(data, offset)?;
    offset += 4;

    // unknown byte
    offset += 1;

    // owner_part_id (2 bytes)
    let owner_part_id = read_i16(data, offset)?;
    offset += 2;

    // owner_part_display_mode (1 byte)
    offset += 1;

    // symbol flags (4 bytes: inner_edge, outer_edge, inside, outside)
    offset += 4;

    // description: [length:1][unknown:1][string]
    let desc_len = data.get(offset).copied().unwrap_or(0) as usize;
    offset += 2; // length + unknown byte
    let description = if desc_len > 0 && offset + desc_len <= data.len() {
        String::from_utf8_lossy(&data[offset..offset + desc_len]).to_string()
    } else {
        String::new()
    };
    offset += desc_len;

    // electrical_type (1 byte)
    let electrical_type = data.get(offset).copied().unwrap_or(4);
    offset += 1;

    // flags (1 byte)
    let flags = data.get(offset).copied().unwrap_or(0);
    offset += 1;

    let rotated = (flags & 0x01) != 0;
    let flipped = (flags & 0x02) != 0;
    let hidden = (flags & 0x04) != 0;
    let show_name = (flags & 0x08) != 0;
    let show_designator = (flags & 0x10) != 0;

    // length (2 bytes)
    let length = i32::from(read_i16(data, offset).unwrap_or(10));
    offset += 2;

    // location X, Y (2 bytes each, signed)
    let x = i32::from(read_i16_signed(data, offset).unwrap_or(0));
    offset += 2;
    let y = i32::from(read_i16_signed(data, offset).unwrap_or(0));
    offset += 2;

    // color (4 bytes)
    offset += 4;

    // name: [length:1][string]
    let name_len = data.get(offset).copied().unwrap_or(0) as usize;
    offset += 1;
    let name = if name_len > 0 && offset + name_len <= data.len() {
        String::from_utf8_lossy(&data[offset..offset + name_len]).to_string()
    } else {
        String::new()
    };
    offset += name_len;

    // designator: [length:1][string]
    let desig_len = data.get(offset).copied().unwrap_or(0) as usize;
    offset += 1;
    let designator = if desig_len > 0 && offset + desig_len <= data.len() {
        String::from_utf8_lossy(&data[offset..offset + desig_len]).to_string()
    } else {
        String::new()
    };

    Some(Pin {
        name,
        designator,
        x,
        y,
        length,
        orientation: PinOrientation::from_flags(rotated, flipped),
        electrical_type: PinElectricalType::from_id(electrical_type),
        hidden,
        show_name,
        show_designator,
        description,
        owner_part_id: owner_part_id.into(),
    })
}

/// Parses a rectangle from properties.
fn parse_rectangle(props: &HashMap<String, String>) -> Option<Rectangle> {
    let x1 = props.get("location.x")?.parse().ok()?;
    let y1 = props.get("location.y")?.parse().ok()?;
    let x2 = props.get("corner.x")?.parse().ok()?;
    let y2 = props.get("corner.y")?.parse().ok()?;

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let line_color = props
        .get("color")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x00_00_80);
    let fill_color = props
        .get("areacolor")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0xFF_FF_B0);
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    Some(Rectangle {
        x1,
        y1,
        x2,
        y2,
        line_width,
        line_color,
        fill_color,
        filled: true,
        owner_part_id,
    })
}

/// Parses a line from properties.
fn parse_line(props: &HashMap<String, String>) -> Option<Line> {
    let x1 = props.get("location.x")?.parse().ok()?;
    let y1 = props.get("location.y")?.parse().ok()?;
    let x2 = props.get("corner.x")?.parse().ok()?;
    let y2 = props.get("corner.y")?.parse().ok()?;

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props
        .get("color")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x00_00_80);
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    Some(Line {
        x1,
        y1,
        x2,
        y2,
        line_width,
        color,
        owner_part_id,
    })
}

/// Parses a parameter from properties.
fn parse_parameter(props: &HashMap<String, String>) -> Option<Parameter> {
    let name = props.get("name")?.clone();
    let value = props.get("text").cloned().unwrap_or_default();

    let x = props
        .get("location.x")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let y = props
        .get("location.y")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let font_id = props
        .get("fontid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props
        .get("color")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x80_00_00);
    let hidden = props.get("ishidden").is_some_and(|s| s == "T");
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    Some(Parameter {
        name,
        value,
        x,
        y,
        font_id,
        color,
        hidden,
        owner_part_id,
    })
}

/// Parses a polyline from properties.
fn parse_polyline(props: &HashMap<String, String>) -> Option<Polyline> {
    // Polylines have LocationCount and Location{n}.X/Location{n}.Y properties
    let location_count: usize = props
        .get("locationcount")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if location_count < 2 {
        return None;
    }

    let mut points = Vec::with_capacity(location_count);
    for i in 1..=location_count {
        let x_key = format!("x{i}");
        let y_key = format!("y{i}");
        let x = props.get(&x_key).and_then(|s| s.parse().ok())?;
        let y = props.get(&y_key).and_then(|s| s.parse().ok())?;
        points.push((x, y));
    }

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props
        .get("color")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x00_00_80);
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    Some(Polyline {
        points,
        line_width,
        color,
        owner_part_id,
    })
}

/// Parses an ellipse from properties.
fn parse_ellipse(props: &HashMap<String, String>) -> Option<Ellipse> {
    let x = props.get("location.x")?.parse().ok()?;
    let y = props.get("location.y")?.parse().ok()?;
    let radius_x = props.get("radius")?.parse().ok()?;
    // Secondary radius, defaults to radius for circles
    let radius_y = props
        .get("secondaryradius")
        .and_then(|s| s.parse().ok())
        .unwrap_or(radius_x);

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let line_color = props
        .get("color")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x00_00_80);
    let fill_color = props
        .get("areacolor")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0xFF_FF_B0);
    let filled = !props.get("issolid").is_some_and(|s| s == "F");
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    Some(Ellipse {
        x,
        y,
        radius_x,
        radius_y,
        line_width,
        line_color,
        fill_color,
        filled,
        owner_part_id,
    })
}

/// Parses an arc from properties.
fn parse_arc(props: &HashMap<String, String>) -> Option<Arc> {
    let x = props.get("location.x")?.parse().ok()?;
    let y = props.get("location.y")?.parse().ok()?;
    let radius = props.get("radius")?.parse().ok()?;

    let start_angle = props
        .get("startangle")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let end_angle = props
        .get("endangle")
        .and_then(|s| s.parse().ok())
        .unwrap_or(360.0);

    let line_width = props
        .get("linewidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let color = props
        .get("color")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x00_00_80);
    let owner_part_id = props
        .get("ownerpartid")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    Some(Arc {
        x,
        y,
        radius,
        start_angle,
        end_angle,
        line_width,
        color,
        owner_part_id,
    })
}

/// Reads a 4-byte little-endian signed integer.
fn read_i32(data: &[u8], offset: usize) -> Option<i32> {
    if offset + 4 > data.len() {
        return None;
    }
    Some(i32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

/// Reads a 2-byte little-endian unsigned integer.
fn read_i16(data: &[u8], offset: usize) -> Option<i16> {
    if offset + 2 > data.len() {
        return None;
    }
    Some(i16::from_le_bytes([data[offset], data[offset + 1]]))
}

/// Reads a 2-byte little-endian signed integer.
fn read_i16_signed(data: &[u8], offset: usize) -> Option<i16> {
    read_i16(data, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_properties() {
        let props = parse_properties("|RECORD=14|Location.X=-10|Location.Y=-4|");
        assert_eq!(props.get("record"), Some(&"14".to_string()));
        assert_eq!(props.get("location.x"), Some(&"-10".to_string()));
    }

    #[test]
    fn test_parse_rectangle() {
        let props = parse_properties(
            "|RECORD=14|Location.X=-10|Location.Y=-4|Corner.X=10|Corner.Y=4|LineWidth=1|",
        );
        let rect = parse_rectangle(&props).unwrap();
        assert_eq!(rect.x1, -10);
        assert_eq!(rect.y1, -4);
        assert_eq!(rect.x2, 10);
        assert_eq!(rect.y2, 4);
    }
}
