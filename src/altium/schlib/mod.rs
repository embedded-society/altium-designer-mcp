//! Schematic library (`SchLib`) file handling.
//!
//! This module provides read/write capabilities for Altium Designer schematic
//! symbol libraries (`.SchLib` files).
//!
//! # File Format
//!
//! `SchLib` files are OLE Compound Documents containing:
//!
//! - `FileHeader` stream: Library metadata (component list, fonts)
//! - `{ComponentName}/Data` stream: Symbol primitives
//! - `Storage` stream: Additional metadata
//!
//! # Data Stream Format
//!
//! ```text
//! [RecordLength:2 LE][RecordType:2 BE][data:RecordLength]
//! ...
//! [0x00 0x00]  // End marker
//! ```
//!
//! Record types:
//! - `0x0000`: Text record (pipe-delimited key=value pairs)
//! - `0x0001`: Binary pin record
//!
//! # Record IDs (RECORD= field in text records)
//!
//! | ID | Type | Description |
//! |----|------|-------------|
//! | 1 | Component | Symbol header |
//! | 2 | Pin | Pin (binary format uses type 0x0001) |
//! | 4 | Label | Text label |
//! | 5 | Bezier | Cubic Bezier curve |
//! | 6 | Polyline | Multiple connected lines |
//! | 7 | Polygon | Filled polygon |
//! | 8 | Ellipse | Ellipse or circle |
//! | 10 | RoundRect | Rounded rectangle |
//! | 11 | EllipticalArc | Elliptical arc segment |
//! | 12 | Arc | Arc segment |
//! | 13 | Line | Single line segment |
//! | 14 | Rectangle | Rectangle shape |
//! | 34 | Designator | Component designator (R?, U?, etc.) |
//! | 41 | Parameter | Component parameter (Value, etc.) |
//! | 44 | Implementation List | Start of model list |
//! | 45 | Model | Footprint model reference |

pub mod primitives;
pub mod reader;
pub mod writer;

use cfb::CompoundFile;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Seek, Write};
use std::path::Path;

use super::{AltiumError, AltiumResult};
pub use primitives::*;

/// A schematic symbol library.
#[derive(Debug, Default)]
pub struct SchLib {
    /// Library file path (if loaded from file).
    pub filepath: Option<String>,
    /// Symbols in the library, keyed by name.
    pub symbols: HashMap<String, Symbol>,
}

impl SchLib {
    /// Creates a new empty `SchLib`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a `SchLib` file from the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or parsed.
    pub fn open(path: impl AsRef<Path>) -> AltiumResult<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).map_err(|e| AltiumError::file_read(path, e))?;

        let mut lib = Self::read(file)?;
        lib.filepath = Some(path.display().to_string());
        Ok(lib)
    }

    /// Reads a `SchLib` from any reader implementing `Read + Seek`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be parsed.
    pub fn read<R: Read + Seek>(reader: R) -> AltiumResult<Self> {
        let mut cfb = CompoundFile::open(reader)
            .map_err(|e| AltiumError::invalid_ole(format!("Invalid OLE file: {e}")))?;

        let mut lib = Self::new();

        // Read FileHeader to get component list
        let header = read_file_header(&mut cfb)?;

        // Read each component
        for comp_name in header.component_names {
            let stream_path = format!("{comp_name}/Data");

            if let Ok(mut stream) = cfb.open_stream(&stream_path) {
                let mut data = Vec::new();
                // Skip components we can't read
                if stream.read_to_end(&mut data).is_err() {
                    continue;
                }

                let mut symbol = Symbol::new(&comp_name);
                symbol.description = header
                    .component_descriptions
                    .get(&comp_name)
                    .cloned()
                    .unwrap_or_default();

                reader::parse_data_stream(&mut symbol, &data);
                lib.symbols.insert(comp_name, symbol);
            }
        }

        Ok(lib)
    }

    /// Gets a symbol by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    /// Returns an iterator over all symbols.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Symbol)> {
        self.symbols.iter()
    }

    /// Returns the number of symbols in the library.
    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Returns true if the library contains no symbols.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Adds a symbol to the library.
    pub fn add_symbol(&mut self, symbol: Symbol) {
        self.symbols.insert(symbol.name.clone(), symbol);
    }

    /// Saves the library to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, path: impl AsRef<Path>) -> AltiumResult<()> {
        let path = path.as_ref();
        let file = std::fs::File::create(path).map_err(|e| AltiumError::file_write(path, e))?;
        self.write(file)
    }

    /// Writes the library to any writer implementing `Read + Write + Seek`.
    ///
    /// # Errors
    ///
    /// Returns an error if the library cannot be written.
    pub fn write<W: Read + Write + Seek>(&self, writer: W) -> AltiumResult<()> {
        // OLE Compound File names are limited to 31 characters
        const MAX_NAME_LEN: usize = 31;

        let mut cfb = CompoundFile::create(writer)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to create OLE file: {e}")))?;

        // Collect symbols for header
        let symbols: Vec<&Symbol> = self.symbols.values().collect();

        // Write FileHeader stream
        let header_data = writer::encode_file_header(&symbols);
        let mut header_stream = cfb
            .create_stream("/FileHeader")
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to create FileHeader: {e}")))?;
        header_stream
            .write_all(&header_data)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to write FileHeader: {e}")))?;
        drop(header_stream);

        // Write each symbol's Data stream
        for symbol in &symbols {
            if symbol.name.len() > MAX_NAME_LEN {
                return Err(AltiumError::invalid_ole(format!(
                    "Symbol name '{}' exceeds {} character limit (length: {})",
                    symbol.name,
                    MAX_NAME_LEN,
                    symbol.name.len()
                )));
            }
            let stream_path = format!("/{}/Data", symbol.name);

            // Create the component directory first
            let dir_path = format!("/{}", symbol.name);
            cfb.create_storage(&dir_path).map_err(|e| {
                AltiumError::invalid_ole(format!("Failed to create storage {dir_path}: {e}"))
            })?;

            // Create and write the Data stream
            let data = writer::encode_data_stream(symbol);
            let mut stream = cfb.create_stream(&stream_path).map_err(|e| {
                AltiumError::invalid_ole(format!("Failed to create stream {stream_path}: {e}"))
            })?;
            stream.write_all(&data).map_err(|e| {
                AltiumError::invalid_ole(format!("Failed to write stream {stream_path}: {e}"))
            })?;
        }

        cfb.flush()
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to flush OLE file: {e}")))?;

        Ok(())
    }
}

/// A schematic symbol.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Symbol {
    /// Symbol name (Design Item ID).
    pub name: String,
    /// Symbol description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Default designator (e.g., "R?", "U?").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub designator: String,
    /// Number of parts (for multi-part symbols).
    #[serde(default = "default_part_count")]
    pub part_count: u32,
    /// Pins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pins: Vec<Pin>,
    /// Rectangles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rectangles: Vec<Rectangle>,
    /// Lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<Line>,
    /// Polylines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub polylines: Vec<Polyline>,
    /// Polygons.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub polygons: Vec<Polygon>,
    /// Arcs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arcs: Vec<Arc>,
    /// Bezier curves.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub beziers: Vec<Bezier>,
    /// Ellipses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ellipses: Vec<Ellipse>,
    /// Rounded rectangles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub round_rects: Vec<RoundRect>,
    /// Elliptical arcs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elliptical_arcs: Vec<EllipticalArc>,
    /// Labels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<Label>,
    /// Parameters (Value, Part Number, etc.).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    /// Footprint model references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footprints: Vec<FootprintModel>,
}

const fn default_part_count() -> u32 {
    1
}

impl Symbol {
    /// Creates a new symbol with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            part_count: 1,
            ..Default::default()
        }
    }

    /// Adds a pin to the symbol.
    pub fn add_pin(&mut self, pin: Pin) {
        self.pins.push(pin);
    }

    /// Adds a rectangle to the symbol.
    pub fn add_rectangle(&mut self, rect: Rectangle) {
        self.rectangles.push(rect);
    }

    /// Adds a line to the symbol.
    pub fn add_line(&mut self, line: Line) {
        self.lines.push(line);
    }

    /// Adds a parameter to the symbol.
    pub fn add_parameter(&mut self, param: Parameter) {
        self.parameters.push(param);
    }

    /// Adds a footprint model reference.
    pub fn add_footprint(&mut self, footprint: FootprintModel) {
        self.footprints.push(footprint);
    }

    /// Adds a polyline to the symbol.
    pub fn add_polyline(&mut self, polyline: Polyline) {
        self.polylines.push(polyline);
    }

    /// Adds a polygon to the symbol.
    pub fn add_polygon(&mut self, polygon: Polygon) {
        self.polygons.push(polygon);
    }

    /// Adds an arc to the symbol.
    pub fn add_arc(&mut self, arc: Arc) {
        self.arcs.push(arc);
    }

    /// Adds a Bezier curve to the symbol.
    pub fn add_bezier(&mut self, bezier: Bezier) {
        self.beziers.push(bezier);
    }

    /// Adds an ellipse to the symbol.
    pub fn add_ellipse(&mut self, ellipse: Ellipse) {
        self.ellipses.push(ellipse);
    }

    /// Adds a rounded rectangle to the symbol.
    pub fn add_round_rect(&mut self, round_rect: RoundRect) {
        self.round_rects.push(round_rect);
    }

    /// Adds an elliptical arc to the symbol.
    pub fn add_elliptical_arc(&mut self, elliptical_arc: EllipticalArc) {
        self.elliptical_arcs.push(elliptical_arc);
    }

    /// Adds a label to the symbol.
    pub fn add_label(&mut self, label: Label) {
        self.labels.push(label);
    }
}

/// Parsed file header information.
struct FileHeader {
    component_names: Vec<String>,
    component_descriptions: HashMap<String, String>,
}

/// Reads the `FileHeader` stream.
fn read_file_header<R: Read + Seek>(cfb: &mut CompoundFile<R>) -> AltiumResult<FileHeader> {
    let mut stream = cfb
        .open_stream("/FileHeader")
        .map_err(|_| AltiumError::missing_stream("FileHeader"))?;

    let mut data = Vec::new();
    stream
        .read_to_end(&mut data)
        .map_err(|_| AltiumError::parse_error(0, "Failed to read FileHeader"))?;

    // Parse header: [length:4 LE][pipe-delimited key=value pairs]
    if data.len() < 4 {
        return Err(AltiumError::parse_error(0, "FileHeader too short"));
    }

    let length = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if data.len() < 4 + length {
        return Err(AltiumError::parse_error(4, "FileHeader truncated"));
    }

    let text = String::from_utf8_lossy(&data[4..4 + length]);
    let mut props: HashMap<String, String> = HashMap::new();

    for part in text.split('|') {
        if let Some((key, value)) = part.split_once('=') {
            props.insert(key.to_lowercase(), value.to_string());
        }
    }

    // Get component count
    let comp_count: usize = props
        .get("compcount")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let mut component_names = Vec::with_capacity(comp_count);
    let mut component_descriptions = HashMap::new();

    for i in 0..comp_count {
        if let Some(name) = props.get(&format!("libref{i}")) {
            component_names.push(name.clone());
            if let Some(desc) = props.get(&format!("compdescr{i}")) {
                component_descriptions.insert(name.clone(), desc.clone());
            }
        }
    }

    Ok(FileHeader {
        component_names,
        component_descriptions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn symbol_creation() {
        let mut symbol = Symbol::new("TEST_SYMBOL");
        symbol.add_pin(Pin::new("IN", "1", -10, 0, 10, PinOrientation::Right));
        symbol.add_rectangle(Rectangle::new(-5, -5, 5, 5));

        assert_eq!(symbol.name, "TEST_SYMBOL");
        assert_eq!(symbol.pins.len(), 1);
        assert_eq!(symbol.rectangles.len(), 1);
    }

    #[test]
    #[ignore = "Requires sample file"]
    fn read_sample_schlib() {
        let lib = SchLib::open("scripts/sample.SchLib").unwrap();
        assert!(!lib.is_empty());

        let symbol = lib.get("SMD Chip Resistor").expect("Symbol not found");
        assert_eq!(symbol.pins.len(), 2);
        assert!(!symbol.rectangles.is_empty());
    }

    #[test]
    fn roundtrip_simple_symbol() {
        // Create a simple symbol
        let mut symbol = Symbol::new("RESISTOR");
        symbol.description = "Test resistor".to_string();
        symbol.designator = "R?".to_string();

        // Add two pins
        symbol.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Right));
        symbol.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Left));

        // Add rectangle body
        symbol.add_rectangle(Rectangle::new(-10, -5, 10, 5));

        // Add a parameter
        symbol.add_parameter(Parameter::new("Value", "*"));

        // Add a footprint reference
        symbol.add_footprint(FootprintModel::new("0603"));

        // Create library and add symbol
        let mut lib = SchLib::new();
        lib.add_symbol(symbol);

        // Write to memory
        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        // Read back
        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");

        // Verify
        assert_eq!(read_lib.len(), 1);
        let read_symbol = read_lib.get("RESISTOR").expect("Symbol not found");
        assert_eq!(read_symbol.name, "RESISTOR");
        assert_eq!(
            read_symbol.designator, "R?",
            "Designator should be preserved"
        );
        assert_eq!(read_symbol.part_count, 1, "part_count should be 1");
        assert_eq!(read_symbol.pins.len(), 2);
        assert_eq!(read_symbol.rectangles.len(), 1);
        assert_eq!(read_symbol.parameters.len(), 1);
        assert_eq!(read_symbol.footprints.len(), 1);

        // Verify pin details
        let pin1 = &read_symbol.pins[0];
        assert_eq!(pin1.designator, "1");
        assert_eq!(pin1.x, -20);
        assert_eq!(pin1.y, 0);
        assert_eq!(pin1.length, 10);
    }

    #[test]
    fn roundtrip_multi_part_symbol() {
        // Create a multi-part symbol (like a dual op-amp)
        let mut symbol = Symbol::new("OPAMP_DUAL");
        symbol.description = "Dual operational amplifier".to_string();
        symbol.designator = "U?".to_string();
        symbol.part_count = 2;

        // Part 1 pins
        let mut pin1 = Pin::new("IN+", "3", -30, 10, 15, PinOrientation::Right);
        pin1.owner_part_id = 1;
        pin1.electrical_type = PinElectricalType::Input;
        symbol.add_pin(pin1);

        let mut pin2 = Pin::new("IN-", "2", -30, -10, 15, PinOrientation::Right);
        pin2.owner_part_id = 1;
        pin2.electrical_type = PinElectricalType::Input;
        symbol.add_pin(pin2);

        let mut pin3 = Pin::new("OUT", "1", 30, 0, 15, PinOrientation::Left);
        pin3.owner_part_id = 1;
        pin3.electrical_type = PinElectricalType::Output;
        symbol.add_pin(pin3);

        // Part 2 pins
        let mut pin4 = Pin::new("IN+", "5", -30, 10, 15, PinOrientation::Right);
        pin4.owner_part_id = 2;
        pin4.electrical_type = PinElectricalType::Input;
        symbol.add_pin(pin4);

        let mut pin5 = Pin::new("IN-", "6", -30, -10, 15, PinOrientation::Right);
        pin5.owner_part_id = 2;
        pin5.electrical_type = PinElectricalType::Input;
        symbol.add_pin(pin5);

        let mut pin6 = Pin::new("OUT", "7", 30, 0, 15, PinOrientation::Left);
        pin6.owner_part_id = 2;
        pin6.electrical_type = PinElectricalType::Output;
        symbol.add_pin(pin6);

        // Rectangle bodies for both parts
        let mut rect1 = Rectangle::new(-15, -20, 15, 20);
        rect1.owner_part_id = 1;
        symbol.add_rectangle(rect1);

        let mut rect2 = Rectangle::new(-15, -20, 15, 20);
        rect2.owner_part_id = 2;
        symbol.add_rectangle(rect2);

        // Create library and write
        let mut lib = SchLib::new();
        lib.add_symbol(symbol);

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        // Read back and verify
        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");

        let read_symbol = read_lib.get("OPAMP_DUAL").expect("Symbol not found");
        assert_eq!(
            read_symbol.designator, "U?",
            "Designator should be preserved"
        );
        assert_eq!(read_symbol.part_count, 2, "part_count should be 2");
        assert_eq!(read_symbol.pins.len(), 6);
        assert_eq!(read_symbol.rectangles.len(), 2);

        // Verify electrical types preserved
        let input_pin_count = read_symbol
            .pins
            .iter()
            .filter(|p| p.electrical_type == PinElectricalType::Input)
            .count();
        assert_eq!(input_pin_count, 4);

        let output_pin_count = read_symbol
            .pins
            .iter()
            .filter(|p| p.electrical_type == PinElectricalType::Output)
            .count();
        assert_eq!(output_pin_count, 2);
    }

    #[test]
    fn roundtrip_bezier_curve() {
        // Create a symbol with a Bezier curve
        let mut symbol = Symbol::new("BEZIER_TEST");
        symbol.description = "Test with Bezier".to_string();
        symbol.designator = "U?".to_string();

        // Add a Bezier curve
        symbol.add_bezier(Bezier::new(-50, 20, -60, 30, -50, 30, -40, 30));

        // Add a second Bezier with different properties
        let mut bezier2 = Bezier::new(0, 0, 10, 20, 20, 20, 30, 0);
        bezier2.line_width = 2;
        bezier2.color = 0x00_00_FF; // Red
        symbol.add_bezier(bezier2);

        // Create library and write
        let mut lib = SchLib::new();
        lib.add_symbol(symbol);

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        // Read back and verify
        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");

        let read_symbol = read_lib.get("BEZIER_TEST").expect("Symbol not found");
        assert_eq!(read_symbol.beziers.len(), 2, "Expected 2 Bezier curves");

        // Verify first Bezier
        let b1 = &read_symbol.beziers[0];
        assert_eq!(b1.x1, -50);
        assert_eq!(b1.y1, 20);
        assert_eq!(b1.x2, -60);
        assert_eq!(b1.y2, 30);
        assert_eq!(b1.x3, -50);
        assert_eq!(b1.y3, 30);
        assert_eq!(b1.x4, -40);
        assert_eq!(b1.y4, 30);

        // Verify second Bezier
        let b2 = &read_symbol.beziers[1];
        assert_eq!(b2.x1, 0);
        assert_eq!(b2.y1, 0);
        assert_eq!(b2.x4, 30);
        assert_eq!(b2.y4, 0);
        assert_eq!(b2.line_width, 2);
        assert_eq!(b2.color, 0x00_00_FF);
    }

    #[test]
    fn roundtrip_polygon() {
        // Create a symbol with a polygon
        let mut symbol = Symbol::new("POLYGON_TEST");
        symbol.description = "Test with Polygon".to_string();

        // Add a filled triangle polygon
        let mut polygon = Polygon {
            points: vec![(-30, 40), (-20, 30), (-10, 40)],
            line_width: 2,
            line_color: 0x00_00_FF, // Red border
            fill_color: 0xFF_00_00, // Blue fill
            filled: true,
            owner_part_id: 1,
        };
        symbol.add_polygon(polygon.clone());

        // Add an unfilled rectangle polygon
        polygon = Polygon {
            points: vec![(0, 0), (20, 0), (20, 20), (0, 20)],
            line_width: 1,
            line_color: 0x00_80_00, // Green border
            fill_color: 0,
            filled: false,
            owner_part_id: 1,
        };
        symbol.add_polygon(polygon);

        // Create library and write
        let mut lib = SchLib::new();
        lib.add_symbol(symbol);

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        // Read back and verify
        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");

        let read_symbol = read_lib.get("POLYGON_TEST").expect("Symbol not found");
        assert_eq!(read_symbol.polygons.len(), 2, "Expected 2 Polygons");

        // Verify first polygon (triangle)
        let p1 = &read_symbol.polygons[0];
        assert_eq!(p1.points.len(), 3);
        assert_eq!(p1.points[0], (-30, 40));
        assert_eq!(p1.points[1], (-20, 30));
        assert_eq!(p1.points[2], (-10, 40));
        assert_eq!(p1.line_width, 2);
        assert_eq!(p1.line_color, 0x00_00_FF);
        assert_eq!(p1.fill_color, 0xFF_00_00);
        assert!(p1.filled);

        // Verify second polygon (rectangle)
        let p2 = &read_symbol.polygons[1];
        assert_eq!(p2.points.len(), 4);
        assert!(!p2.filled);
    }

    #[test]
    fn roundtrip_round_rect() {
        // Create a symbol with rounded rectangles
        let mut symbol = Symbol::new("ROUNDRECT_TEST");
        symbol.description = "Test with RoundRect".to_string();

        // Add a filled rounded rectangle
        let round_rect1 = RoundRect::new(40, 20, 90, 50, 20, 20);
        symbol.add_round_rect(round_rect1);

        // Add a second rounded rectangle with different properties
        let mut round_rect2 = RoundRect::new(0, 0, 30, 20, 5, 10);
        round_rect2.line_width = 2;
        round_rect2.line_color = 0x00_00_FF; // Red
        round_rect2.fill_color = 0xFF_00_00; // Blue
        round_rect2.filled = false;
        symbol.add_round_rect(round_rect2);

        // Create library and write
        let mut lib = SchLib::new();
        lib.add_symbol(symbol);

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        // Read back and verify
        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");

        let read_symbol = read_lib.get("ROUNDRECT_TEST").expect("Symbol not found");
        assert_eq!(read_symbol.round_rects.len(), 2, "Expected 2 RoundRects");

        // Verify first rounded rectangle
        let rr1 = &read_symbol.round_rects[0];
        assert_eq!(rr1.x1, 40);
        assert_eq!(rr1.y1, 20);
        assert_eq!(rr1.x2, 90);
        assert_eq!(rr1.y2, 50);
        assert_eq!(rr1.corner_x_radius, 20);
        assert_eq!(rr1.corner_y_radius, 20);
        assert!(rr1.filled);

        // Verify second rounded rectangle
        let rr2 = &read_symbol.round_rects[1];
        assert_eq!(rr2.x1, 0);
        assert_eq!(rr2.y1, 0);
        assert_eq!(rr2.x2, 30);
        assert_eq!(rr2.y2, 20);
        assert_eq!(rr2.corner_x_radius, 5);
        assert_eq!(rr2.corner_y_radius, 10);
        assert_eq!(rr2.line_width, 2);
        assert!(!rr2.filled);
    }

    #[test]
    fn roundtrip_elliptical_arc() {
        // Create a symbol with elliptical arcs
        let mut symbol = Symbol::new("ELLIPTICAL_ARC_TEST");
        symbol.description = "Test with EllipticalArc".to_string();

        // Add an elliptical arc with fractional radii
        let arc1 = EllipticalArc::new(-60, 0, 9.96689, 9.99668, 90.0, 270.0);
        symbol.add_elliptical_arc(arc1);

        // Add a second elliptical arc (full ellipse)
        let mut arc2 = EllipticalArc::full_ellipse(20, 30, 15.5, 10.25);
        arc2.line_width = 2;
        arc2.color = 0x00_FF_00; // Green
        symbol.add_elliptical_arc(arc2);

        // Create library and write
        let mut lib = SchLib::new();
        lib.add_symbol(symbol);

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        // Read back and verify
        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");

        let read_symbol = read_lib
            .get("ELLIPTICAL_ARC_TEST")
            .expect("Symbol not found");
        assert_eq!(
            read_symbol.elliptical_arcs.len(),
            2,
            "Expected 2 EllipticalArcs"
        );

        // Verify first elliptical arc
        let ea1 = &read_symbol.elliptical_arcs[0];
        assert_eq!(ea1.x, -60);
        assert_eq!(ea1.y, 0);
        // Check radii are close (allowing for fractional representation)
        assert!((ea1.radius - 9.96689).abs() < 0.001);
        assert!((ea1.secondary_radius - 9.99668).abs() < 0.001);
        assert!((ea1.start_angle - 90.0).abs() < 0.001);
        assert!((ea1.end_angle - 270.0).abs() < 0.001);

        // Verify second elliptical arc
        let ea2 = &read_symbol.elliptical_arcs[1];
        assert_eq!(ea2.x, 20);
        assert_eq!(ea2.y, 30);
        assert!((ea2.radius - 15.5).abs() < 0.001);
        assert!((ea2.secondary_radius - 10.25).abs() < 0.001);
        assert_eq!(ea2.line_width, 2);
        assert_eq!(ea2.color, 0x00_FF_00);
    }
}
