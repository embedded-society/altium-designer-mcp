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
//! - `Storage` stream: Embedded image bytes (icon storage; see the `storage` module)
//!
//! # Data Stream Format
//!
//! ```text
//! [length:3 LE u24][flags:1 u8][data:length]
//! ...
//! ```
//!
//! The 4-byte header is Altium's single 32-bit little-endian size word: the low
//! 24 bits are the payload length, the high byte is the record-type flag.
//! There is NO end-of-stream marker — records run until the stream is exhausted.
//! (A trailing `0x0000` would be mis-read as a zero-length record; see issue #68.)
//!
//! Record-type flag (the header's high byte):
//! - `0x00`: Text record (pipe-delimited key=value pairs)
//! - `0x01`: Binary pin record
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
//! | 9 | Pie | Filled circular sector |
//! | 10 | RoundRect | Rounded rectangle |
//! | 11 | EllipticalArc | Elliptical arc segment |
//! | 12 | Arc | Arc segment |
//! | 13 | Line | Single line segment |
//! | 14 | Rectangle | Rectangle shape |
//! | 28 | TextFrame | Bordered multi-line text box |
//! | 30 | Image | Embedded/linked picture |
//! | 34 | Designator | Component designator (R?, U?, etc.) |
//! | 41 | Parameter | Component parameter (Value, etc.) |
//! | 44 | Implementation List | Start of model list |
//! | 45 | Model | Footprint model reference |

pub(crate) mod coord;
pub(crate) mod pin_aux;
pub mod primitives;
mod read_io;
pub mod reader;
pub(crate) mod storage;
mod write_io;
pub mod writer;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::{AltiumError, AltiumResult};
pub use primitives::*;

/// A schematic symbol library.
///
/// # Example
///
/// ```no_run
/// use altium_designer_mcp::altium::schlib::{SchLib, Symbol, Pin, PinOrientation};
///
/// // Create a new library and add symbols
/// let mut lib = SchLib::new();
///
/// let mut symbol = Symbol::new("RESISTOR");
/// symbol.description = "Generic Resistor".to_string();
/// // Pin::new(name, designator, x, y, length, orientation)
/// symbol.add_pin(Pin::new("1", "1", -200, 0, 100, PinOrientation::Right));
/// symbol.add_pin(Pin::new("2", "2", 200, 0, 100, PinOrientation::Left));
/// lib.add(symbol);
///
/// // Save to file
/// lib.save("MyLibrary.SchLib").unwrap();
///
/// // Open an existing library
/// let lib = SchLib::open("MyLibrary.SchLib").unwrap();
/// for name in lib.names() {
///     println!("Symbol: {name}");
/// }
/// ```
#[derive(Debug, Default)]
pub struct SchLib {
    /// Library file path (if loaded from file).
    filepath: Option<String>,
    /// Symbols in the library, keyed by name (insertion order preserved).
    symbols: IndexMap<String, Symbol>,
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

    /// Returns the file path this library was loaded from, if any.
    #[must_use]
    pub fn filepath(&self) -> Option<&str> {
        self.filepath.as_deref()
    }

    /// Gets a symbol by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    /// Gets a mutable reference to a symbol by name.
    #[must_use]
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Symbol> {
        self.symbols.get_mut(name)
    }

    /// Returns an iterator over all symbols.
    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.values()
    }

    /// Returns a mutable iterator over all symbols.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Symbol> {
        self.symbols.values_mut()
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
    pub fn add(&mut self, symbol: Symbol) {
        self.symbols.insert(symbol.name.clone(), symbol);
    }

    /// Removes a symbol from the library by name.
    ///
    /// Returns the removed symbol if found, or `None` if no symbol with that name exists.
    pub fn remove(&mut self, name: &str) -> Option<Symbol> {
        self.symbols.shift_remove(name)
    }

    /// Updates a symbol in-place, preserving its position in the library.
    ///
    /// The symbol is matched by the `name` parameter. The replacement symbol
    /// will be stored under the same key, preserving position. If you need to
    /// rename the symbol, use `rename` after updating.
    ///
    /// Returns the old symbol if found, or `None` if no symbol with that name exists.
    pub fn update(&mut self, name: &str, replacement: Symbol) -> Option<Symbol> {
        self.symbols
            .get_mut(name)
            .map(|old| std::mem::replace(old, replacement))
    }

    /// Returns a list of symbol names in order.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.symbols.keys().cloned().collect()
    }

    /// Reorders symbols according to the given name order.
    ///
    /// Symbols are reordered to match the order of names in `new_order`.
    /// Names not present in the library are ignored. Symbols not mentioned
    /// in `new_order` are placed at the end in their original relative order.
    ///
    /// Returns the new order of symbol names.
    pub fn reorder(&mut self, new_order: &[&str]) -> Vec<String> {
        // Stable-sort symbols into the desired order; symbols not listed in
        // `new_order` keep their relative order at the end.
        let rank = crate::altium::order_ranker(new_order);
        self.symbols
            .sort_by(|a_key, _, b_key, _| rank(a_key.as_str()).cmp(&rank(b_key.as_str())));

        self.names()
    }

    /// Saves the library to a file.
    ///
    /// Uses atomic write: writes to a temporary file first, then renames on success.
    /// This prevents data loss if the write fails partway through.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, path: impl AsRef<Path>) -> AltiumResult<()> {
        crate::altium::save_atomic(path.as_ref(), "schlib.tmp", |file| self.write(file))
    }
}

/// A schematic symbol.
///
/// # Example
///
/// ```
/// use altium_designer_mcp::altium::schlib::{Symbol, Pin, Rectangle, PinOrientation};
///
/// let mut symbol = Symbol::new("RESISTOR");
/// symbol.description = "Chip Resistor".to_string();
/// symbol.designator = "R?".to_string();
///
/// // Add body rectangle
/// symbol.add_rectangle(Rectangle::new(-100, -40, 100, 40));
///
/// // Add pins (using SchLib units: 1 unit = 10 mils)
/// // Pin::new(name, designator, x, y, length, orientation)
/// symbol.add_pin(Pin::new("1", "1", -200, 0, 100, PinOrientation::Right));
/// symbol.add_pin(Pin::new("2", "2", 200, 0, 100, PinOrientation::Left));
///
/// assert_eq!(symbol.pins.len(), 2);
/// assert_eq!(symbol.rectangles.len(), 1);
/// ```
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
    /// X position of the designator text (`RECORD=34` `Location.X`). The AD24
    /// golden authors `Location.X=-5` on every from-scratch symbol, so this
    /// defaults to `-5`.
    #[serde(
        default = "default_designator_x",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub designator_x: f64,
    /// Y position of the designator text (`RECORD=34` `Location.Y`). The AD24
    /// golden authors `Location.Y=5`, so this defaults to `5`.
    #[serde(
        default = "default_designator_y",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub designator_y: f64,
    /// Unique ID of the designator record (`RECORD=34` `UniqueID`). Preserved on
    /// read so a read-modify-write re-emits the same id (deterministic RMW); a
    /// from-scratch symbol generates a fresh one on write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub designator_unique_id: Option<String>,
    /// Number of parts (for multi-part symbols).
    #[serde(default = "default_part_count")]
    pub part_count: u32,
    /// Number of display modes.
    #[serde(default = "default_part_count")]
    pub display_mode_count: u32,
    /// Currently displayed part ID.
    #[serde(default = "default_part_count")]
    pub current_part_id: u32,
    /// Whether the part ID is locked.
    #[serde(default)]
    pub part_id_locked: bool,
    /// Source library name.
    #[serde(default = "default_source_library")]
    pub source_library_name: String,
    /// Target file name.
    #[serde(default = "default_target_file")]
    pub target_file_name: String,
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
    /// Pies (filled circular sectors, `RECORD=9`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pies: Vec<Pie>,
    /// Images (embedded/linked pictures, `RECORD=30`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<Image>,
    /// Text frames (bordered multi-line text boxes, `RECORD=28`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_frames: Vec<TextFrame>,
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
    /// Text annotations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text: Vec<Text>,
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

/// Golden-verified from-scratch designator X position (`Location.X=-5`).
const fn default_designator_x() -> f64 {
    -5.0
}

/// Golden-verified from-scratch designator Y position (`Location.Y=5`).
const fn default_designator_y() -> f64 {
    5.0
}

fn default_source_library() -> String {
    "*".to_string()
}

fn default_target_file() -> String {
    "*".to_string()
}

impl Symbol {
    /// Creates a new symbol with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            designator_x: default_designator_x(),
            designator_y: default_designator_y(),
            part_count: 1,
            display_mode_count: 1,
            current_part_id: 1,
            part_id_locked: false,
            source_library_name: "*".to_string(),
            target_file_name: "*".to_string(),
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

    /// Adds a pie (filled sector) to the symbol.
    pub fn add_pie(&mut self, pie: Pie) {
        self.pies.push(pie);
    }

    /// Adds an image to the symbol.
    pub fn add_image(&mut self, image: Image) {
        self.images.push(image);
    }

    /// Adds a text frame to the symbol.
    pub fn add_text_frame(&mut self, text_frame: TextFrame) {
        self.text_frames.push(text_frame);
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

    /// Adds a text annotation to the symbol.
    pub fn add_text(&mut self, text: Text) {
        self.text.push(text);
    }
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
        lib.add(symbol);

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
    fn roundtrip_footprint_iscurrent_flag() {
        // The writer emits IsCurrent positionally (first model = current); the reader
        // now preserves that flag instead of dropping it.
        let mut symbol = Symbol::new("R1");
        symbol.add_footprint(FootprintModel::new("0603"));
        symbol.add_footprint(FootprintModel::new("0805"));

        let data = writer::encode_data_stream(&symbol).expect("encode");
        let mut decoded = Symbol::new("R1");
        reader::parse_data_stream(&mut decoded, &data);

        assert_eq!(
            decoded.footprints.len(),
            2,
            "both models survive the round-trip"
        );
        assert!(
            decoded.footprints[0].is_current,
            "first model is current (IsCurrent=T)"
        );
        assert!(
            !decoded.footprints[1].is_current,
            "second model is not current"
        );
    }

    #[test]
    fn roundtrip_text_annotation_stays_text_not_label() {
        // Regression: encode_text emitted RECORD=4 (the Label id), so a Text
        // annotation round-tripped back as a Label (reader: 3 => Text, 4 => Label).
        let mut symbol = Symbol::new("TXT");
        symbol.add_text(Text {
            x: 5.0,
            y: 7.0,
            text: "NOTE".to_string(),
            font_id: 1,
            color: 0,
            justification: TextJustification::BottomLeft,
            rotation: 0.0,
            is_mirrored: false,
            is_hidden: false,
            owner_part_id: 1,
            unique_id: None,
        });

        let data = writer::encode_data_stream(&symbol).expect("encode");
        let mut decoded = Symbol::new("TXT");
        reader::parse_data_stream(&mut decoded, &data);

        assert_eq!(decoded.text.len(), 1, "the Text survives as a Text");
        assert_eq!(decoded.text[0].text, "NOTE");
        assert!(
            decoded.labels.is_empty(),
            "the Text must NOT be mis-read as a Label"
        );
    }

    #[test]
    fn roundtrip_pie() {
        // Pie (RECORD=9) is a filled circular sector; verify every field survives
        // encode -> parse, and that a Pie is not mistaken for an Arc.
        let mut symbol = Symbol::new("PIE");
        let mut pie = Pie::new(10, 20, 30, 45.0, 135.0);
        pie.line_width = 2;
        pie.line_color = 0x00_00_FF;
        pie.fill_color = 0xFF_00_00;
        pie.filled = true;
        pie.transparent = true;
        pie.is_not_accessible = false;
        pie.display_flags.graphically_locked = true;
        symbol.add_pie(pie);

        let data = writer::encode_data_stream(&symbol).expect("encode");
        let mut decoded = Symbol::new("PIE");
        reader::parse_data_stream(&mut decoded, &data);

        assert_eq!(decoded.pies.len(), 1, "the Pie survives as a Pie");
        assert!(decoded.arcs.is_empty(), "a Pie is not read as an Arc");
        let p = &decoded.pies[0];
        assert!((p.x - 10.0).abs() < 1e-9 && (p.y - 20.0).abs() < 1e-9);
        assert!((p.radius - 30.0).abs() < 1e-9);
        assert!((p.start_angle - 45.0).abs() < 1e-6);
        assert!((p.end_angle - 135.0).abs() < 1e-6);
        assert_eq!(p.line_width, 2);
        assert_eq!(p.line_color, 0x00_00_FF);
        assert_eq!(p.fill_color, 0xFF_00_00);
        assert!(p.filled, "IsSolid round-trips");
        assert!(p.transparent, "Transparent round-trips");
        assert!(!p.is_not_accessible, "false IsNotAccesible round-trips");
        assert!(
            p.display_flags.graphically_locked,
            "GraphicallyLocked round-trips"
        );
    }

    #[test]
    fn roundtrip_image() {
        // Image (RECORD=30) is a bounding-box picture record; verify the metadata
        // round-trips (the embedded bytes in /Storage are a separate concern).
        let mut symbol = Symbol::new("IMG");
        let mut image = Image::new(10, 20, 60, 50, "logo.png");
        image.line_width = 2;
        image.line_color = 0x00_00_FF;
        image.line_style = 1;
        image.fill_color = 0xAB_CD_EF;
        image.filled = true;
        image.transparent = true;
        image.show_border = true;
        image.keep_aspect = true;
        image.embed_image = true;
        image.is_not_accessible = false;
        image.display_flags.dimmed = true;
        symbol.add_image(image);

        let data = writer::encode_data_stream(&symbol).expect("encode");
        let mut decoded = Symbol::new("IMG");
        reader::parse_data_stream(&mut decoded, &data);

        assert_eq!(decoded.images.len(), 1, "the Image survives");
        let im = &decoded.images[0];
        assert!((im.x1 - 10.0).abs() < 1e-9 && (im.y1 - 20.0).abs() < 1e-9);
        assert!((im.x2 - 60.0).abs() < 1e-9 && (im.y2 - 50.0).abs() < 1e-9);
        assert_eq!(im.line_width, 2);
        assert_eq!(im.line_color, 0x00_00_FF);
        assert_eq!(im.line_style, 1);
        assert_eq!(im.fill_color, 0xAB_CD_EF);
        assert!(im.filled && im.transparent && im.show_border && im.keep_aspect);
        assert!(im.embed_image, "EmbedImage round-trips");
        assert_eq!(im.file_name, "logo.png");
        assert!(!im.is_not_accessible, "false IsNotAccesible round-trips");
        assert!(im.display_flags.dimmed, "Dimmed round-trips");
    }

    #[test]
    fn roundtrip_embedded_image_bytes() {
        // Embedded image BYTES round-trip through the library-level /Storage
        // stream: two embedded images with distinct payloads, interleaved with
        // a non-embedded one (which must be skipped by the in-order matching
        // and stay byte-less). Full in-RAM write -> read cycle.
        let mut symbol = Symbol::new("EMBED_IMGS");

        let mut first = Image::new(0, 0, 10, 6, r"C:\img\first.bmp");
        first.embed_image = true;
        first.image_data = Some(vec![0x42, 0x4D, 0x01, 0x02, 0x03]);
        symbol.add_image(first);

        // Non-embedded (linked) image between the two embedded ones.
        symbol.add_image(Image::new(0, 10, 5, 13, "linked.png"));

        let mut second = Image::new(-10, -6, 0, 0, r"C:\img\second.bmp");
        second.embed_image = true;
        second.image_data = Some(vec![0xAB; 300]);
        symbol.add_image(second);

        let mut lib = SchLib::new();
        lib.add(symbol);
        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");
        let read_symbol = read_lib.get("EMBED_IMGS").expect("Symbol not found");

        assert_eq!(read_symbol.images.len(), 3, "all three images survive");
        let [a, b, c] = &read_symbol.images[..] else {
            panic!("expected exactly three images");
        };
        assert!(a.embed_image, "first image stays embedded");
        assert_eq!(
            a.image_data.as_deref(),
            Some([0x42, 0x4D, 0x01, 0x02, 0x03].as_slice()),
            "first payload matches in order"
        );
        assert!(!b.embed_image, "linked image stays non-embedded");
        assert_eq!(b.image_data, None, "linked image carries no bytes");
        assert!(c.embed_image, "second image stays embedded");
        assert_eq!(
            c.image_data.as_deref(),
            Some(vec![0xAB; 300].as_slice()),
            "second payload matches in order (not the linked slot)"
        );
    }

    #[test]
    fn bytesless_embedded_image_does_not_steal_next_payload_same_symbol() {
        // Regression: an `embed_image` image WITHOUT carried bytes used to be
        // skipped by the writer while the reader still consumed a payload for
        // it, so it stole the next embedded image's bytes. The writer now
        // emits an empty placeholder entry, keeping the ordinals aligned.
        let mut symbol = Symbol::new("BYTELESS_FIRST");

        let mut byteless = Image::new(0, 0, 10, 6, r"C:\img\byteless.bmp");
        byteless.embed_image = true; // no image_data
        symbol.add_image(byteless);

        let mut real = Image::new(0, 10, 10, 16, r"C:\img\real.bmp");
        real.embed_image = true;
        real.image_data = Some(vec![0x42, 0x4D, 0x99]);
        symbol.add_image(real);

        let mut lib = SchLib::new();
        lib.add(symbol);
        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");
        let sym = read_lib.get("BYTELESS_FIRST").expect("Symbol not found");
        let [a, b] = &sym.images[..] else {
            panic!("expected exactly two images");
        };
        assert!(a.embed_image && b.embed_image, "both stay embedded");
        assert_eq!(
            a.image_data, None,
            "the bytes-less image must NOT steal the next payload"
        );
        assert_eq!(
            b.image_data.as_deref(),
            Some([0x42, 0x4D, 0x99].as_slice()),
            "the real image keeps its own payload"
        );
    }

    #[test]
    fn bytesless_embedded_image_does_not_steal_payload_across_symbols() {
        // Same regression across symbol boundaries: the payload<->image match
        // is in GLOBAL symbol order, so a bytes-less embedded image in an
        // earlier symbol used to capture a later symbol's bytes.
        let mut first = Symbol::new("A_BYTELESS");
        let mut byteless = Image::new(0, 0, 10, 6, r"C:\img\byteless.bmp");
        byteless.embed_image = true; // no image_data
        first.add_image(byteless);

        let mut second = Symbol::new("B_REAL");
        let mut real = Image::new(0, 0, 10, 6, r"C:\img\real.bmp");
        real.embed_image = true;
        real.image_data = Some(vec![0xCA, 0xFE, 0xBA, 0xBE]);
        second.add_image(real);

        let mut lib = SchLib::new();
        lib.add(first);
        lib.add(second);
        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");
        let a = &read_lib.get("A_BYTELESS").expect("first symbol").images[0];
        let b = &read_lib.get("B_REAL").expect("second symbol").images[0];
        assert_eq!(
            a.image_data, None,
            "bytes-less image in the earlier symbol carries no bytes"
        );
        assert_eq!(
            b.image_data.as_deref(),
            Some([0xCA, 0xFE, 0xBA, 0xBE].as_slice()),
            "the later symbol's image keeps its own payload"
        );
    }

    #[test]
    fn roundtrip_text_frame() {
        // TextFrame (RECORD=28) round-trips through a full in-RAM library
        // write/read, with every field at a non-default value.
        let mut symbol = Symbol::new("FRAME_TEST");
        symbol.designator = "U?".to_string();
        let mut frame = TextFrame::new(-12.5, -6, 12.5, 6, "Line one");
        frame.color = 0x00_00_FF;
        frame.area_color = 0xAB_CD_EF;
        frame.text_color = 0x12_34_56;
        frame.text_margin = 1.25;
        frame.line_width = 2;
        frame.line_style = 1;
        frame.transparent = true;
        frame.font_id = 2;
        frame.orientation = 1;
        frame.alignment = 2;
        frame.is_solid = true;
        frame.show_border = false;
        frame.word_wrap = false;
        frame.clip_to_rect = false;
        frame.is_not_accessible = false;
        frame.owner_part_id = 1;
        frame.display_flags.dimmed = true;
        symbol.add_text_frame(frame);

        let mut lib = SchLib::new();
        lib.add(symbol);
        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");
        let read_symbol = read_lib.get("FRAME_TEST").expect("Symbol not found");

        assert_eq!(read_symbol.text_frames.len(), 1, "the TextFrame survives");
        let f = &read_symbol.text_frames[0];
        assert!((f.x1 - -12.5).abs() < 1e-9 && (f.y1 - -6.0).abs() < 1e-9);
        assert!((f.x2 - 12.5).abs() < 1e-9 && (f.y2 - 6.0).abs() < 1e-9);
        assert_eq!(f.text, "Line one");
        assert_eq!(f.color, 0x00_00_FF);
        assert_eq!(f.area_color, 0xAB_CD_EF);
        assert_eq!(f.text_color, 0x12_34_56);
        assert!(
            (f.text_margin - 1.25).abs() < 1e-9,
            "fractional TextMargin round-trips"
        );
        assert_eq!(f.line_width, 2);
        assert_eq!(f.line_style, 1);
        assert!(f.transparent, "Transparent round-trips");
        assert_eq!(f.font_id, 2);
        assert_eq!(f.orientation, 1);
        assert_eq!(f.alignment, 2);
        assert!(f.is_solid, "IsSolid round-trips");
        assert!(!f.show_border, "false ShowBorder round-trips");
        assert!(!f.word_wrap, "false WordWrap round-trips");
        assert!(!f.clip_to_rect, "false ClipToRect round-trips");
        assert!(!f.is_not_accessible, "false IsNotAccesible round-trips");
        assert!(f.display_flags.dimmed, "Dimmed round-trips");
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
        lib.add(symbol);

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
        lib.add(symbol);

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        // Read back and verify
        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");

        let read_symbol = read_lib.get("BEZIER_TEST").expect("Symbol not found");
        assert_eq!(read_symbol.beziers.len(), 2, "Expected 2 Bezier curves");

        // Verify first Bezier
        let b1 = &read_symbol.beziers[0];
        assert_eq!(
            (b1.x1, b1.y1, b1.x2, b1.y2, b1.x3, b1.y3, b1.x4, b1.y4),
            (-50.0, 20.0, -60.0, 30.0, -50.0, 30.0, -40.0, 30.0)
        );

        // Verify second Bezier
        let b2 = &read_symbol.beziers[1];
        assert_eq!((b2.x1, b2.y1, b2.x4, b2.y4), (0.0, 0.0, 30.0, 0.0));
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
            points: vec![(-30.0, 40.0), (-20.0, 30.0), (-10.0, 40.0)],
            line_width: 2,
            line_color: 0x00_00_FF, // Red border
            fill_color: 0xFF_00_00, // Blue fill
            line_style: 2,          // Dotted border (non-default)
            filled: true,
            transparent: true,        // Transparent fill (non-default)
            is_not_accessible: false, // Non-default (Altium omits the key)
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        };
        symbol.add_polygon(polygon.clone());

        // Add an unfilled rectangle polygon
        polygon = Polygon {
            points: vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)],
            line_width: 1,
            line_color: 0x00_80_00, // Green border
            fill_color: 0,
            line_style: 0,
            filled: false,
            transparent: false,
            is_not_accessible: true,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        };
        symbol.add_polygon(polygon);

        // Create library and write
        let mut lib = SchLib::new();
        lib.add(symbol);

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
        assert_eq!(p1.points[0], (-30.0, 40.0));
        assert_eq!(p1.points[1], (-20.0, 30.0));
        assert_eq!(p1.points[2], (-10.0, 40.0));
        assert_eq!(p1.line_width, 2);
        assert_eq!(p1.line_color, 0x00_00_FF);
        assert_eq!(p1.fill_color, 0xFF_00_00);
        assert!(p1.filled);
        assert_eq!(p1.line_style, 2, "dotted border round-trips");
        assert!(p1.transparent, "transparent fill round-trips");
        assert!(
            !p1.is_not_accessible,
            "false IsNotAccesible round-trips as false (Altium omits the key)"
        );

        // Verify second polygon (rectangle)
        let p2 = &read_symbol.polygons[1];
        assert_eq!(p2.points.len(), 4);
        assert!(!p2.filled);
        // The rectangle polygon left the new fields at their defaults.
        assert_eq!(p2.line_style, 0, "default line_style");
        assert!(!p2.transparent, "default opaque");
        assert!(p2.is_not_accessible, "default IsNotAccesible=T round-trips");
    }

    #[test]
    fn polygon_default_is_byte_identical() {
        // Byte-identity guard: a default polygon (is_not_accessible=true,
        // line_style=0, transparent=false) must emit exactly the golden record
        // shape — IsNotAccesible=T between RECORD and OwnerPartId (the
        // SHAPESTYLE golden's `|RECORD=7|IsNotAccesible=T|IndexInSheet=4|
        // OwnerPartId=1|` order; the token itself is omitted here at slot 0),
        // and NO LineStyle / Transparent tokens.
        let mut sym = Symbol::new("POLY_DEFAULT");
        sym.add_polygon(Polygon {
            points: vec![(0.0, 0.0), (5.0, 0.0), (2.5, 5.0)],
            line_width: 1,
            line_color: 0,
            fill_color: 0,
            line_style: 0,
            filled: true,
            transparent: false,
            is_not_accessible: true,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: Some("ABCD1234".to_string()),
        });
        let data = writer::encode_data_stream(&sym).expect("encode");
        let text = String::from_utf8_lossy(&data);
        assert!(
            text.contains("|RECORD=7|IsNotAccesible=T|OwnerPartId=1|LineWidth=1"),
            "default polygon keeps IsNotAccesible=T in the golden position: {text}"
        );
        assert!(
            !text.contains("LineStyle"),
            "default line_style emits no LineStyle token: {text}"
        );
        assert!(
            !text.contains("Transparent"),
            "default opaque polygon emits no Transparent token: {text}"
        );
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
        lib.add(symbol);

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        // Read back and verify
        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");

        let read_symbol = read_lib.get("ROUNDRECT_TEST").expect("Symbol not found");
        assert_eq!(read_symbol.round_rects.len(), 2, "Expected 2 RoundRects");

        // Verify first rounded rectangle
        let rr1 = &read_symbol.round_rects[0];
        assert_eq!(
            (
                rr1.x1,
                rr1.y1,
                rr1.x2,
                rr1.y2,
                rr1.corner_x_radius,
                rr1.corner_y_radius
            ),
            (40.0, 20.0, 90.0, 50.0, 20.0, 20.0)
        );
        assert!(rr1.filled);

        // Verify second rounded rectangle
        let rr2 = &read_symbol.round_rects[1];
        assert_eq!(
            (
                rr2.x1,
                rr2.y1,
                rr2.x2,
                rr2.y2,
                rr2.corner_x_radius,
                rr2.corner_y_radius
            ),
            (0.0, 0.0, 30.0, 20.0, 5.0, 10.0)
        );
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
        lib.add(symbol);

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
        assert_eq!((ea1.x, ea1.y), (-60.0, 0.0));
        // Check radii are close (allowing for fractional representation)
        assert!((ea1.radius - 9.96689).abs() < 0.001);
        assert!((ea1.secondary_radius - 9.99668).abs() < 0.001);
        assert!((ea1.start_angle - 90.0).abs() < 0.001);
        assert!((ea1.end_angle - 270.0).abs() < 0.001);

        // Verify second elliptical arc
        let ea2 = &read_symbol.elliptical_arcs[1];
        assert_eq!((ea2.x, ea2.y), (20.0, 30.0));
        assert!((ea2.radius - 15.5).abs() < 0.001);
        assert!((ea2.secondary_radius - 10.25).abs() < 0.001);
        assert_eq!(ea2.line_width, 2);
        assert_eq!(ea2.color, 0x00_FF_00);
    }

    #[test]
    fn roundtrip_per_record_optional_fields() {
        // Exercises the per-record optional fields added for round-trip fidelity:
        // AreaColor (Arc/EllipticalArc), LineStyle (Line/RoundRect), LineStyleExt
        // (Rectangle), Transparent (Ellipse/RoundRect), and the IsNotAccesible
        // default-true booleans on Line/Bezier.
        let mut symbol = Symbol::new("OPTFIELDS_TEST");

        // AreaColor on Arc (Arc has no ::new — build a struct literal).
        let arc = Arc {
            x: 0.0,
            y: 0.0,
            radius: 10.0,
            is_not_accessible: true,
            start_angle: 0.0,
            end_angle: 360.0,
            line_width: 1,
            color: 0,
            fill_color: 0x11_22_33,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        };
        symbol.add_arc(arc);

        // AreaColor on EllipticalArc.
        let mut earc = EllipticalArc::new(-60, 0, 9.966_89, 9.996_68, 90.0, 270.0);
        earc.fill_color = 0x44_55_66;
        symbol.add_elliptical_arc(earc);

        // LineStyle on Line.
        let mut line = Line::new(0, 0, 10, 0);
        line.line_style = 2;
        symbol.add_line(line);

        // LineStyle + Transparent on RoundRect.
        let mut round_rect = RoundRect::new(0, 0, 30, 20, 5, 5);
        round_rect.line_style = 1;
        round_rect.transparent = true;
        symbol.add_round_rect(round_rect);

        // LineStyleExt on Rectangle.
        let mut rect = Rectangle::new(0, 0, 40, 40);
        rect.line_style = 1;
        symbol.add_rectangle(rect);

        // Transparent on Ellipse.
        let mut ell = Ellipse::new(5, 5, 8, 8);
        ell.transparent = true;
        symbol.add_ellipse(ell);

        // IsNotAccesible = false on Line (rare non-default case).
        let mut line2 = Line::new(0, 0, 5, 5);
        line2.is_not_accessible = false;
        symbol.add_line(line2);

        // IsNotAccesible = false on Bezier (rare non-default case).
        let mut bez = Bezier::new(0, 0, 1, 1, 2, 2, 3, 3);
        bez.is_not_accessible = false;
        symbol.add_bezier(bez);

        let mut lib = SchLib::new();
        lib.add(symbol);

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("Failed to write SchLib");

        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("Failed to read SchLib");
        let s = read_lib.get("OPTFIELDS_TEST").expect("Symbol not found");

        assert_eq!(s.arcs[0].fill_color, 0x11_22_33, "Arc AreaColor preserved");
        assert_eq!(
            s.elliptical_arcs[0].fill_color, 0x44_55_66,
            "EllipticalArc AreaColor preserved"
        );
        assert_eq!(s.lines[0].line_style, 2, "Line LineStyle preserved");
        assert!(
            s.lines[0].is_not_accessible,
            "default Line IsNotAccesible stays true"
        );
        assert_eq!(
            s.round_rects[0].line_style, 1,
            "RoundRect LineStyle preserved"
        );
        assert!(
            s.round_rects[0].transparent,
            "RoundRect Transparent preserved"
        );
        assert_eq!(
            s.rectangles[0].line_style, 1,
            "Rectangle LineStyleExt preserved"
        );
        assert!(s.ellipses[0].transparent, "Ellipse Transparent preserved");

        // With the reader matching parse_arc (Altium omits the key when false, so
        // absent => false), a `false` IsNotAccesible now round-trips: it is omitted
        // on write and read back as false.
        assert!(
            !s.lines[1].is_not_accessible,
            "false Line IsNotAccesible round-trips as false"
        );
        assert!(
            !s.beziers[0].is_not_accessible,
            "false Bezier IsNotAccesible round-trips as false"
        );

        // Byte-identity: a `false` shape omits the token entirely, while a default
        // (true) shape still emits `=T`, so from-scratch output is unchanged.
        let mut false_sym = Symbol::new("INA_FALSE");
        let mut fline = Line::new(0, 0, 5, 5);
        fline.is_not_accessible = false;
        false_sym.add_line(fline);
        let mut fbez = Bezier::new(0, 0, 1, 1, 2, 2, 3, 3);
        fbez.is_not_accessible = false;
        false_sym.add_bezier(fbez);
        let false_data = writer::encode_data_stream(&false_sym).expect("encode");
        let false_text = String::from_utf8_lossy(&false_data);
        assert!(
            !false_text.contains("IsNotAccesible"),
            "false Line/Bezier must omit the IsNotAccesible token: {false_text}"
        );

        let mut true_sym = Symbol::new("INA_TRUE");
        true_sym.add_line(Line::new(0, 0, 5, 5));
        true_sym.add_bezier(Bezier::new(0, 0, 1, 1, 2, 2, 3, 3));
        let true_data = writer::encode_data_stream(&true_sym).expect("encode");
        let true_text = String::from_utf8_lossy(&true_data);
        assert_eq!(
            true_text.matches("IsNotAccesible=T").count(),
            2,
            "default Line + Bezier still emit IsNotAccesible=T: {true_text}"
        );
    }

    #[test]
    fn elliptical_arc_radius_frac_carry_and_roundtrip() {
        // Grid-aligned radii must emit NO _Frac token — the byte-identical / oracle-safe
        // path for from-scratch symbols.
        let mut grid = Symbol::new("EARC_GRID");
        grid.add_elliptical_arc(EllipticalArc::new(0, 0, 5.0, 3.0, 0.0, 360.0));
        let g = String::from_utf8_lossy(&writer::encode_data_stream(&grid).expect("encode"))
            .into_owned();
        assert!(!g.contains("_Frac"), "grid-aligned radii omit _Frac: {g}");
        assert!(
            g.contains("|Radius=5|"),
            "integer radius emitted plainly: {g}"
        );

        // A near-boundary radius must CARRY into the integer part, not clamp to 99999.
        let mut sym = Symbol::new("EARC_CARRY");
        sym.add_elliptical_arc(EllipticalArc::new(0, 0, 4.999_995, 3.5, 0.0, 360.0));
        let enc = String::from_utf8_lossy(&writer::encode_data_stream(&sym).expect("encode"))
            .into_owned();
        assert!(
            enc.contains("|Radius=5|"),
            "boundary radius carries to int: {enc}"
        );
        assert!(
            !enc.contains("|Radius_Frac"),
            "primary radius carried, so no Radius_Frac: {enc}"
        );
        assert!(
            enc.contains("|SecondaryRadius_Frac=50000"),
            "secondary 3.5 keeps its frac: {enc}"
        );

        // Round-trip: 4.999995 -> 5.0; 3.5 -> SecondaryRadius_Frac=50000 -> 3.5.
        let mut lib = SchLib::new();
        lib.add(sym);
        let mut buf = Cursor::new(Vec::new());
        lib.write(&mut buf).expect("write");
        buf.set_position(0);
        let read = SchLib::read(buf).expect("read");
        let ea = &read.get("EARC_CARRY").expect("symbol").elliptical_arcs[0];
        assert!(
            (ea.radius - 5.0).abs() < 1e-9,
            "carried radius round-trips: {}",
            ea.radius
        );
        assert!(
            (ea.secondary_radius - 3.5).abs() < 1e-9,
            "frac round-trips: {}",
            ea.secondary_radius
        );
    }

    #[test]
    fn wrong_file_type_pcblib_as_schlib() {
        // Create a PcbLib file in memory (using SchLib format with length prefix)
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut cfb = cfb::CompoundFile::create(&mut buffer).expect("create cfb");

            // Write a FileHeader with PcbLib header string (but SchLib format with length prefix)
            let header_text = "|HEADER=Protel for Windows - PCB Library|COMPCOUNT=0|";
            let header_bytes = header_text.as_bytes();

            // SchLib format: [length:4 LE][text]
            #[allow(clippy::cast_possible_truncation)]
            let length = header_bytes.len() as u32;
            let mut header_data = Vec::with_capacity(4 + header_bytes.len());
            header_data.extend_from_slice(&length.to_le_bytes());
            header_data.extend_from_slice(header_bytes);

            let mut stream = cfb.create_stream("/FileHeader").expect("create stream");
            std::io::Write::write_all(&mut stream, &header_data).expect("write header");
        }

        // Try to read it as SchLib - should fail with WrongFileType
        buffer.set_position(0);
        let result = SchLib::read(&mut buffer);

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("Wrong file type"),
            "Expected 'Wrong file type' error, got: {err_str}"
        );
        assert!(
            err_str.contains("expected SchLib"),
            "Expected 'expected SchLib' in error, got: {err_str}"
        );
    }

    #[test]
    fn roundtrip_line_fractional_and_negative_coords() {
        // Off-grid endpoints — including a negative fractional coordinate, the
        // case the elliptical-arc encoder never exercised — must survive a
        // write -> read round-trip through the `_Frac` companion fields.
        let mut symbol = Symbol::new("FRAC_LINE");
        symbol.add_line(Line::new(-28.995, 7.5, 10.25, -0.5));

        let mut lib = SchLib::new();
        lib.add(symbol);
        let mut buf = Cursor::new(Vec::new());
        lib.write(&mut buf).expect("Failed to write SchLib");
        buf.set_position(0);
        let read = SchLib::read(buf).expect("Failed to read SchLib");

        let l = &read.get("FRAC_LINE").expect("symbol present").lines[0];
        assert!((l.x1 - (-28.995)).abs() < 1e-9, "x1 round-trips: {}", l.x1);
        assert!((l.y1 - 7.5).abs() < 1e-9, "y1 round-trips: {}", l.y1);
        assert!((l.x2 - 10.25).abs() < 1e-9, "x2 round-trips: {}", l.x2);
        assert!((l.y2 - (-0.5)).abs() < 1e-9, "y2 round-trips: {}", l.y2);
    }

    #[test]
    fn roundtrip_line_integer_coords_emit_no_frac() {
        // Integer-grid lines must serialise without any `_Frac` token (byte
        // identity with pre-migration output) and still round-trip exactly.
        let mut symbol = Symbol::new("INT_LINE");
        symbol.add_line(Line::new(-30, 0, 30, 0));
        let data = writer::encode_data_stream(&symbol).expect("encode");
        let text = String::from_utf8_lossy(&data);
        assert!(
            !text.contains("_Frac"),
            "integer line must emit no _Frac: {text}"
        );

        let mut decoded = Symbol::new("INT_LINE");
        reader::parse_data_stream(&mut decoded, &data);
        let l = &decoded.lines[0];
        assert!((l.x1 - (-30.0)).abs() < 1e-9 && (l.x2 - 30.0).abs() < 1e-9);
    }

    #[test]
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)] // exercises every fractional-capable primitive
    fn roundtrip_all_primitives_fractional_and_negative_coords() {
        // Every graphic primitive carries off-grid (including negative) coordinates
        // through a write -> read round-trip via the `_Frac` companion fields.
        let approx = |a: f64, b: f64| (a - b).abs() < 1e-9;

        let mut sym = Symbol::new("FRAC_ALL");
        sym.add_rectangle(Rectangle::new(-10.25, -0.5, 10.75, 20.125));
        sym.add_round_rect(RoundRect::new(-5.5, -5.5, 5.5, 5.5, 1.25, 2.75));
        sym.add_ellipse(Ellipse::new(-1.5, 2.5, 7.5, 3.25));
        let arc = Arc {
            x: -3.5,
            y: 4.25,
            radius: 6.75,
            is_not_accessible: true,
            start_angle: 0.0,
            end_angle: 180.0,
            line_width: 1,
            color: 0,
            fill_color: 0,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        };
        sym.add_arc(arc);
        sym.add_bezier(Bezier::new(-0.5, 0.5, 1.5, 2.5, 3.5, 4.5, 5.5, -6.5));
        sym.add_polyline(Polyline {
            points: vec![(-1.25, 0.0), (2.5, -3.75), (10.0, 0.5)],
            line_width: 1,
            color: 0,
            line_style: 0,
            start_line_shape: 0,
            end_line_shape: 0,
            line_shape_size: 0,
            transparent: false,
            is_not_accessible: true,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        });
        sym.add_polygon(Polygon {
            points: vec![(-2.5, -2.5), (2.5, -2.5), (0.0, 3.125)],
            line_width: 1,
            line_color: 0,
            fill_color: 0,
            line_style: 0,
            filled: true,
            transparent: false,
            is_not_accessible: true,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        });
        let label = Label {
            x: -7.5,
            y: 0.25,
            text: "L".to_string(),
            font_id: 1,
            color: 0,
            justification: TextJustification::BottomLeft,
            rotation: 0.0,
            is_mirrored: false,
            is_hidden: false,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        };
        sym.add_label(label);
        let mut param = Parameter::new("Value", "1k");
        param.x = -20.5;
        param.y = 30.25;
        sym.add_parameter(param);

        let mut lib = SchLib::new();
        lib.add(sym);
        let mut buf = std::io::Cursor::new(Vec::new());
        lib.write(&mut buf).expect("write");
        buf.set_position(0);
        let s = SchLib::read(buf).expect("read");
        let s = s.get("FRAC_ALL").expect("symbol present");

        let r = &s.rectangles[0];
        assert!(
            approx(r.x1, -10.25)
                && approx(r.y1, -0.5)
                && approx(r.x2, 10.75)
                && approx(r.y2, 20.125)
        );
        let rr = &s.round_rects[0];
        assert!(
            approx(rr.x1, -5.5)
                && approx(rr.corner_x_radius, 1.25)
                && approx(rr.corner_y_radius, 2.75)
        );
        let e = &s.ellipses[0];
        assert!(
            approx(e.x, -1.5)
                && approx(e.y, 2.5)
                && approx(e.radius_x, 7.5)
                && approx(e.radius_y, 3.25)
        );
        let a = &s.arcs[0];
        assert!(approx(a.x, -3.5) && approx(a.y, 4.25) && approx(a.radius, 6.75));
        let b = &s.beziers[0];
        assert!(approx(b.x1, -0.5) && approx(b.y4, -6.5));
        let pl = &s.polylines[0];
        assert!(approx(pl.points[1].0, 2.5) && approx(pl.points[1].1, -3.75));
        let pg = &s.polygons[0];
        assert!(approx(pg.points[2].1, 3.125));
        let lab = &s.labels[0];
        assert!(approx(lab.x, -7.5) && approx(lab.y, 0.25));
        let p = &s.parameters[0];
        assert!(approx(p.x, -20.5) && approx(p.y, 30.25));
    }
}
