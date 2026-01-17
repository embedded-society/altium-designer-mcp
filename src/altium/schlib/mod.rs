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
//! | 6 | Polyline | Multiple connected lines |
//! | 7 | Polygon | Filled polygon |
//! | 8 | Ellipse | Ellipse or circle |
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
        let file =
            std::fs::File::create(path).map_err(|e| AltiumError::file_write(path, e))?;
        self.write(file)
    }

    /// Writes the library to any writer implementing `Read + Write + Seek`.
    ///
    /// # Errors
    ///
    /// Returns an error if the library cannot be written.
    pub fn write<W: Read + Write + Seek>(&self, writer: W) -> AltiumResult<()> {
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
            let stream_path = format!("/{}/Data", symbol.name);

            // Create the component directory first
            let dir_path = format!("/{}", symbol.name);
            cfb.create_storage(&dir_path)
                .map_err(|e| AltiumError::invalid_ole(format!("Failed to create storage {dir_path}: {e}")))?;

            // Create and write the Data stream
            let data = writer::encode_data_stream(symbol);
            let mut stream = cfb
                .create_stream(&stream_path)
                .map_err(|e| AltiumError::invalid_ole(format!("Failed to create stream {stream_path}: {e}")))?;
            stream
                .write_all(&data)
                .map_err(|e| AltiumError::invalid_ole(format!("Failed to write stream {stream_path}: {e}")))?;
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
    /// Arcs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arcs: Vec<Arc>,
    /// Ellipses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ellipses: Vec<Ellipse>,
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

    /// Adds an arc to the symbol.
    pub fn add_arc(&mut self, arc: Arc) {
        self.arcs.push(arc);
    }

    /// Adds an ellipse to the symbol.
    pub fn add_ellipse(&mut self, ellipse: Ellipse) {
        self.ellipses.push(ellipse);
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
}
