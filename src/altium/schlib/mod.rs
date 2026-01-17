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

use cfb::CompoundFile;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Seek};
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
}
