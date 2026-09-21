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

/// Test-only re-export of [`pin_aux::apply_pin_wide_text`], so the golden
/// fidelity test can resolve a `PinWideText` stream through the same fold the
/// reader uses rather than reimplementing it.
#[doc(hidden)]
pub fn apply_pin_wide_text_for_test(pins: &mut [Pin], raw: &[u8]) {
    pin_aux::apply_pin_wide_text(pins, raw);
}

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
    /// The library's own `UniqueID` from its `FileHeader`, kept for the
    /// library's lifetime as Altium keeps it; a library built from scratch
    /// is given one on its first save.
    unique_id: Option<String>,
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
        // The whole file is read into memory first: a compound-file reader
        // seeks through its sector chains constantly, and each seek against
        // an unbuffered file is a system call — several times the cost of
        // parsing the same bytes from memory.
        let bytes = std::fs::read(path).map_err(|e| AltiumError::file_read(path, e))?;

        let mut lib = Self::read(std::io::Cursor::new(bytes))?;
        lib.filepath = Some(path.display().to_string());
        Ok(lib)
    }

    /// Returns the file path this library was loaded from, if any.
    #[must_use]
    pub fn filepath(&self) -> Option<&str> {
        self.filepath.as_deref()
    }

    /// The index of the symbol `name` resolves to: the exact name, else the
    /// symbol whose name is the same regardless of case — the way the file's
    /// own directory resolves it (see [`crate::altium::same_name`]).
    fn index_of(&self, name: &str) -> Option<usize> {
        self.symbols.get_index_of(name).or_else(|| {
            self.symbols
                .keys()
                .position(|key| crate::altium::same_name(key, name))
        })
    }

    /// Gets a symbol by name — the exact name, else the one the name
    /// resolves to regardless of case.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.index_of(name)
            .and_then(|i| self.symbols.get_index(i))
            .map(|(_, symbol)| symbol)
    }

    /// Gets a mutable reference to a symbol by name (resolved as
    /// [`Self::get`] resolves it).
    #[must_use]
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Symbol> {
        self.index_of(name)
            .and_then(move |i| self.symbols.get_index_mut(i))
            .map(|(_, symbol)| symbol)
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
        self.index_of(name)
            .and_then(|i| self.symbols.shift_remove_index(i))
            .map(|(_, symbol)| symbol)
    }

    /// Updates a symbol in-place, preserving its position in the library.
    ///
    /// The symbol is matched by the `name` parameter; a replacement that
    /// carries another name renames it, and the library resolves the new
    /// name from then on.
    ///
    /// Returns the old symbol if found, or `None` if no symbol with that name exists.
    pub fn update(&mut self, name: &str, replacement: Symbol) -> Option<Symbol> {
        let renamed = self
            .get(name)
            .is_some_and(|old| old.name != replacement.name);
        let old = self
            .get_mut(name)
            .map(|old| std::mem::replace(old, replacement));
        if renamed {
            self.rekey();
        }
        old
    }

    /// Renames a symbol in place, so it keeps its position in the library
    /// and in the file. Returns whether `old_name` resolved to one.
    pub fn rename(&mut self, old_name: &str, new_name: &str) -> bool {
        self.rename_all(&[(old_name.to_string(), new_name.to_string())])
            .is_empty()
    }

    /// Renames several symbols at once, each in place. Every `(old, new)`
    /// pair resolves against the names as they were before the call, so a
    /// chain such as `A -> B, B -> C` renames both rather than renaming the
    /// new `B` twice. Returns the old names that resolved to nothing.
    pub fn rename_all(&mut self, renames: &[(String, String)]) -> Vec<String> {
        let mut missing = Vec::new();
        let mut resolved: Vec<(usize, &str)> = Vec::with_capacity(renames.len());
        for (old, new) in renames {
            match self.index_of(old) {
                Some(i) => resolved.push((i, new.as_str())),
                None => missing.push(old.clone()),
            }
        }
        for (i, new) in resolved {
            if let Some((_, symbol)) = self.symbols.get_index_mut(i) {
                symbol.name = new.to_string();
                // A renamed symbol gets the storage its new name derives.
                symbol.storage_name = None;
            }
        }
        self.rekey();
        missing
    }

    /// Re-derives every key from its symbol's name, in order, after names
    /// were changed in place.
    fn rekey(&mut self) {
        self.symbols = self
            .symbols
            .drain(..)
            .map(|(_, symbol)| (symbol.name.clone(), symbol))
            .collect();
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
        crate::altium::save_atomic(path.as_ref(), "schlib.tmp", |image| self.write(image))
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
    pub ieee_symbols: Vec<IeeeSymbol>,
    /// Parameters (Value, Part Number, etc.).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    /// Footprint model references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footprints: Vec<FootprintModel>,

    /// The header record (`RECORD=1`) exactly as read: every segment as a
    /// `(key, value)` pair in stored order, an empty segment as `("", "")`.
    /// The writer replays each verbatim unless the field behind it was
    /// edited, so a header comes back byte for byte whichever way Altium
    /// wrote it — a UI-authored one omits `LibraryPath` and
    /// `SheetPartFileName`, carries `COMPONENTKINDVERSION2`, and stores a
    /// Latin-1 description as `%UTF8%Key=<UTF-8>|||Key=<Windows-1252>`; a
    /// scripted one puts UTF-8 bytes in both keys. Empty for a symbol built
    /// from scratch, which emits the canonical header.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub header_params: Vec<(String, String)>,

    /// Streams of the symbol's storage this crate does not read — a
    /// `PinFunctionData` from a newer Altium, say — carried verbatim and
    /// written back beside the ones it does, so nothing Altium stored is
    /// dropped for being unknown.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        with = "crate::altium::base64_opt::named"
    )]
    pub extra_streams: Vec<(String, Vec<u8>)>,

    /// The CFB storage the symbol was read from, kept so a rewrite stores it
    /// under the same name: Altium resolves a symbol by re-deriving its
    /// storage name from the real name or, for a long one, through
    /// `SectionKeys`, so a storage that moved is a symbol it cannot load
    /// (#507, for footprints; the same rule governs symbols). `None` for a
    /// symbol built from scratch, renamed or copied: derived on save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_name: Option<String>,

    /// `AllPinCount` as stored. Altium keeps a stale value here — a 32-pin
    /// UI-drawn MCU stores 1, a one-pin header 2 — so it is carried rather
    /// than recomputed; `None` (a symbol built from scratch) writes the pin
    /// count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_pin_count: Option<u32>,

    /// The order the content records are stored in, one entry per record.
    ///
    /// Altium interleaves the record kinds in authoring order — the golden's
    /// `LOCKFLAGS2` runs line, arc, ellipse, round-rect, polyline, polygon,
    /// pie, bezier, label — and numbers them with one shared `IndexInSheet`
    /// counter in exactly that sequence. Emitting the kinds in blocks
    /// renumbers every record.
    ///
    /// An entry names one of the lists above; its n-th occurrence refers to
    /// that list's n-th element, so the sequence alone reconstructs the
    /// interleaving. It is maintained by the `add_*` methods, which is how
    /// reading a symbol records the file's order. Empty when the lists were
    /// populated directly, in which case the writer falls back to
    /// [`SchPrimitiveKind::WRITE_ORDER`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primitive_order: Vec<SchPrimitiveKind>,
}

primitive_kinds! {
    /// One of a [`Symbol`]'s content-record lists, as named by `primitive_order`.
    ///
    /// Footprint models are absent on purpose: they are written in the
    /// implementation section after the content records and take no
    /// `IndexInSheet` slot. The write order leads with rectangles so a
    /// solid-filled body sits behind the pins; emitting pins first would let
    /// the body paint over the pin names inside it.
    SchPrimitiveKind {
        /// [`Symbol::rectangles`].
        Rectangle => "rectangle",
        /// [`Symbol::pins`].
        Pin => "pin",
        /// [`Symbol::lines`].
        Line => "line",
        /// [`Symbol::polylines`].
        Polyline => "polyline",
        /// [`Symbol::polygons`].
        Polygon => "polygon",
        /// [`Symbol::arcs`].
        Arc => "arc",
        /// [`Symbol::pies`].
        Pie => "pie",
        /// [`Symbol::images`].
        Image => "image",
        /// [`Symbol::text_frames`].
        TextFrame => "text_frame",
        /// [`Symbol::beziers`].
        Bezier => "bezier",
        /// [`Symbol::ellipses`].
        Ellipse => "ellipse",
        /// [`Symbol::round_rects`].
        RoundRect => "round_rect",
        /// [`Symbol::elliptical_arcs`].
        EllipticalArc => "elliptical_arc",
        /// [`Symbol::labels`].
        Label => "label",
        /// [`Symbol::ieee_symbols`].
        IeeeSymbol => "ieee_symbol",
        /// [`Symbol::parameters`].
        Parameter => "parameter",
    }
}

/// What one symbol record draws with, as [`Symbol::styles_of`] reports it.
///
/// The stroke width and colour, the fill colour and the text colour, each
/// `None` for a kind that has no such property. Colours are BGR.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordStyle {
    /// Stroke width of an outline (`LineWidth`).
    pub line_width: Option<u8>,
    /// Stroke colour of an outline, or a pin's colour.
    pub line_color: Option<u32>,
    /// Fill colour of a fillable shape (`AreaColor`), whether or not it is
    /// currently filled.
    pub fill_color: Option<u32>,
    /// Colour of the text a label, text frame or parameter shows.
    pub text_color: Option<u32>,
}

impl RecordStyle {
    /// An outline: a stroke width and colour, nothing filled, no text.
    #[must_use]
    pub const fn stroke(line_width: u8, line_color: u32) -> Self {
        Self {
            line_width: Some(line_width),
            line_color: Some(line_color),
            fill_color: None,
            text_color: None,
        }
    }

    /// A fillable shape: an outline plus its fill colour.
    #[must_use]
    pub const fn shape(line_width: u8, line_color: u32, fill_color: u32) -> Self {
        Self {
            fill_color: Some(fill_color),
            ..Self::stroke(line_width, line_color)
        }
    }

    /// Text alone, as a label or parameter draws.
    #[must_use]
    pub const fn text(text_color: u32) -> Self {
        Self {
            line_width: None,
            line_color: None,
            fill_color: None,
            text_color: Some(text_color),
        }
    }
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
    /// Gives the symbol the identity of a brand-new component: the designator
    /// record's unique id and every record's unique id are cleared, so the
    /// writer mints fresh ones exactly as for a symbol built from scratch. For
    /// a clone that will live beside its source.
    pub fn reset_identities(&mut self) {
        self.designator_unique_id = None;
        macro_rules! clear {
            ($($list:ident),* $(,)?) => { $( for item in &mut self.$list { item.unique_id = None; } )* };
        }
        clear!(
            rectangles,
            lines,
            polylines,
            polygons,
            arcs,
            pies,
            images,
            text_frames,
            beziers,
            ellipses,
            round_rects,
            elliptical_arcs,
            labels,
            parameters,
            footprints,
        );
    }

    pub fn add_pin(&mut self, pin: Pin) {
        self.pins.push(pin);
        self.primitive_order.push(SchPrimitiveKind::Pin);
    }

    /// Adds a rectangle to the symbol.
    pub fn add_rectangle(&mut self, rect: Rectangle) {
        self.rectangles.push(rect);
        self.primitive_order.push(SchPrimitiveKind::Rectangle);
    }

    /// Adds a line to the symbol.
    pub fn add_line(&mut self, line: Line) {
        self.lines.push(line);
        self.primitive_order.push(SchPrimitiveKind::Line);
    }

    /// Adds a parameter to the symbol.
    pub fn add_parameter(&mut self, param: Parameter) {
        self.parameters.push(param);
        self.primitive_order.push(SchPrimitiveKind::Parameter);
    }

    /// Removes the parameter at `index` and its slot in
    /// [`Self::primitive_order`], so every other record keeps its place in
    /// the file instead of the later parameters each moving up one slot.
    ///
    /// # Panics
    ///
    /// Panics when `index` is past the end of [`Self::parameters`].
    pub fn remove_parameter(&mut self, index: usize) -> Parameter {
        let param = self.parameters.remove(index);
        let slot = self
            .primitive_order
            .iter()
            .enumerate()
            .filter(|(_, kind)| **kind == SchPrimitiveKind::Parameter)
            .nth(index)
            .map(|(position, _)| position);
        if let Some(position) = slot {
            self.primitive_order.remove(position);
        }
        param
    }

    /// Adds a footprint model reference.
    pub fn add_footprint(&mut self, footprint: FootprintModel) {
        self.footprints.push(footprint);
    }

    /// Adds a polyline to the symbol.
    pub fn add_polyline(&mut self, polyline: Polyline) {
        self.polylines.push(polyline);
        self.primitive_order.push(SchPrimitiveKind::Polyline);
    }

    /// Adds a polygon to the symbol.
    pub fn add_polygon(&mut self, polygon: Polygon) {
        self.polygons.push(polygon);
        self.primitive_order.push(SchPrimitiveKind::Polygon);
    }

    /// Adds an arc to the symbol.
    pub fn add_arc(&mut self, arc: Arc) {
        self.arcs.push(arc);
        self.primitive_order.push(SchPrimitiveKind::Arc);
    }

    /// Adds a pie (filled sector) to the symbol.
    pub fn add_pie(&mut self, pie: Pie) {
        self.pies.push(pie);
        self.primitive_order.push(SchPrimitiveKind::Pie);
    }

    /// Adds an image to the symbol.
    pub fn add_image(&mut self, image: Image) {
        self.images.push(image);
        self.primitive_order.push(SchPrimitiveKind::Image);
    }

    /// Adds a text frame to the symbol.
    pub fn add_text_frame(&mut self, text_frame: TextFrame) {
        self.text_frames.push(text_frame);
        self.primitive_order.push(SchPrimitiveKind::TextFrame);
    }

    /// Adds a Bezier curve to the symbol.
    pub fn add_bezier(&mut self, bezier: Bezier) {
        self.beziers.push(bezier);
        self.primitive_order.push(SchPrimitiveKind::Bezier);
    }

    /// Adds an ellipse to the symbol.
    pub fn add_ellipse(&mut self, ellipse: Ellipse) {
        self.ellipses.push(ellipse);
        self.primitive_order.push(SchPrimitiveKind::Ellipse);
    }

    /// Adds a rounded rectangle to the symbol.
    pub fn add_round_rect(&mut self, round_rect: RoundRect) {
        self.round_rects.push(round_rect);
        self.primitive_order.push(SchPrimitiveKind::RoundRect);
    }

    /// Adds an elliptical arc to the symbol.
    pub fn add_elliptical_arc(&mut self, elliptical_arc: EllipticalArc) {
        self.elliptical_arcs.push(elliptical_arc);
        self.primitive_order.push(SchPrimitiveKind::EllipticalArc);
    }

    /// Adds a label to the symbol.
    pub fn add_label(&mut self, label: Label) {
        self.labels.push(label);
        self.primitive_order.push(SchPrimitiveKind::Label);
    }

    /// Adds a text annotation to the symbol.
    pub fn add_ieee_symbol(&mut self, symbol: IeeeSymbol) {
        self.ieee_symbols.push(symbol);
        self.primitive_order.push(SchPrimitiveKind::IeeeSymbol);
    }

    /// The strings the writer never places in a pipe-delimited record: a
    /// pin is a binary record with length-prefixed strings, and an extra
    /// stream's name is an OLE storage name.
    pub const RECORD_TEXT_EXEMPT: &'static [&'static str] = &["pins[]", "extra_streams"];

    /// Refuses text the record format cannot hold: a `|` in any string the
    /// writer would place between the separators of a text record (see
    /// [`Self::RECORD_TEXT_EXEMPT`] for the strings it never does). Altium's
    /// own schematic editor stores such a `|` as `¦` (U+00A6), so the text is
    /// refused with that on offer rather than written to be cut.
    ///
    /// # Errors
    ///
    /// A message naming this symbol and the offending field's path.
    pub fn check_record_text(&self) -> Result<(), String> {
        crate::altium::record_separator_path(self, Self::RECORD_TEXT_EXEMPT).map_or(
            Ok(()),
            |path| {
                Err(format!(
                    "Symbol '{}' {path} contains '|', the separator of Altium's record format, \
                 which cannot hold it (Altium's own schematic editor stores it as '¦', U+00A6 \
                 — send that character if it is what you mean)",
                    self.name
                ))
            },
        )
    }

    /// The symbol's content records in the order they are written, as
    /// `(kind, index into that kind's list)` pairs.
    ///
    /// [`Self::primitive_order`] is advisory: the lists are public, so a caller
    /// can push to or truncate one without it. Entries pointing past the end of
    /// their list are therefore dropped, and any record the sequence never
    /// reaches is appended in [`SchPrimitiveKind::WRITE_ORDER`] — so a symbol
    /// with no recorded order, or one edited behind its back, still writes
    /// every record exactly once.
    #[must_use]
    pub fn write_sequence(&self) -> Vec<(SchPrimitiveKind, usize)> {
        let mut taken: std::collections::HashMap<SchPrimitiveKind, usize> =
            std::collections::HashMap::new();
        let mut sequence = Vec::new();

        for &kind in &self.primitive_order {
            let next = taken.entry(kind).or_insert(0);
            if *next < self.count_of(kind) {
                sequence.push((kind, *next));
                *next += 1;
            }
        }
        for kind in SchPrimitiveKind::WRITE_ORDER {
            let next = taken.entry(kind).or_insert(0);
            while *next < self.count_of(kind) {
                sequence.push((kind, *next));
                *next += 1;
            }
        }
        sequence
    }

    /// How many body graphics the symbol draws: every shape kind — rectangle,
    /// rounded rectangle, line, polyline, polygon, arc, elliptical arc, pie,
    /// ellipse, bezier, image, text frame — but not pins, labels, IEEE
    /// symbols or parameters, which decorate a body rather than form one.
    #[must_use]
    pub fn body_graphic_count(&self) -> usize {
        [
            SchPrimitiveKind::Rectangle,
            SchPrimitiveKind::RoundRect,
            SchPrimitiveKind::Line,
            SchPrimitiveKind::Polyline,
            SchPrimitiveKind::Polygon,
            SchPrimitiveKind::Arc,
            SchPrimitiveKind::EllipticalArc,
            SchPrimitiveKind::Pie,
            SchPrimitiveKind::Ellipse,
            SchPrimitiveKind::Bezier,
            SchPrimitiveKind::Image,
            SchPrimitiveKind::TextFrame,
        ]
        .into_iter()
        .map(|kind| self.count_of(kind))
        .sum()
    }

    /// The stroke, fill and text colours every record of `kind` draws with,
    /// one entry per record. A property the kind lacks is `None`: a pin has
    /// a colour but no stroke width, a label only a text colour, an arc no
    /// fill (Altium stores an `AreaColor` on arcs it never paints).
    #[must_use]
    pub fn styles_of(&self, kind: SchPrimitiveKind) -> Vec<RecordStyle> {
        use RecordStyle as S;
        match kind {
            SchPrimitiveKind::Rectangle => self
                .rectangles
                .iter()
                .map(|r| S::shape(r.line_width, r.line_color, r.fill_color))
                .collect(),
            SchPrimitiveKind::Pin => self
                .pins
                .iter()
                .map(|pin| S {
                    line_color: Some(pin.colour),
                    ..S::default()
                })
                .collect(),
            SchPrimitiveKind::Line => self
                .lines
                .iter()
                .map(|line| S::stroke(line.line_width, line.color))
                .collect(),
            SchPrimitiveKind::Polyline => self
                .polylines
                .iter()
                .map(|p| S::stroke(p.line_width, p.color))
                .collect(),
            SchPrimitiveKind::Polygon => self
                .polygons
                .iter()
                .map(|p| S::shape(p.line_width, p.line_color, p.fill_color))
                .collect(),
            SchPrimitiveKind::Arc => self
                .arcs
                .iter()
                .map(|arc| S::stroke(arc.line_width, arc.color))
                .collect(),
            SchPrimitiveKind::Pie => self
                .pies
                .iter()
                .map(|pie| S::shape(pie.line_width, pie.line_color, pie.fill_color))
                .collect(),
            SchPrimitiveKind::Image => self
                .images
                .iter()
                .map(|i| S::shape(i.line_width, i.line_color, i.fill_color))
                .collect(),
            SchPrimitiveKind::TextFrame => self
                .text_frames
                .iter()
                .map(|f| S {
                    text_color: Some(f.text_color),
                    ..S::shape(f.line_width, f.color, f.area_color)
                })
                .collect(),
            SchPrimitiveKind::Bezier => self
                .beziers
                .iter()
                .map(|b| S::stroke(b.line_width, b.color))
                .collect(),
            SchPrimitiveKind::Ellipse => self
                .ellipses
                .iter()
                .map(|e| S::shape(e.line_width, e.line_color, e.fill_color))
                .collect(),
            SchPrimitiveKind::RoundRect => self
                .round_rects
                .iter()
                .map(|r| S::shape(r.line_width, r.line_color, r.fill_color))
                .collect(),
            SchPrimitiveKind::EllipticalArc => self
                .elliptical_arcs
                .iter()
                .map(|arc| S::stroke(arc.line_width, arc.color))
                .collect(),
            SchPrimitiveKind::Label => self.labels.iter().map(|l| S::text(l.color)).collect(),
            SchPrimitiveKind::IeeeSymbol => self
                .ieee_symbols
                .iter()
                .map(|i| S::stroke(i.line_width, i.color))
                .collect(),
            SchPrimitiveKind::Parameter => {
                self.parameters.iter().map(|p| S::text(p.color)).collect()
            }
        }
    }

    /// How many content records of one kind the symbol holds.
    #[must_use]
    pub const fn count_of(&self, kind: SchPrimitiveKind) -> usize {
        match kind {
            SchPrimitiveKind::Rectangle => self.rectangles.len(),
            SchPrimitiveKind::Pin => self.pins.len(),
            SchPrimitiveKind::Line => self.lines.len(),
            SchPrimitiveKind::Polyline => self.polylines.len(),
            SchPrimitiveKind::Polygon => self.polygons.len(),
            SchPrimitiveKind::Arc => self.arcs.len(),
            SchPrimitiveKind::Pie => self.pies.len(),
            SchPrimitiveKind::Image => self.images.len(),
            SchPrimitiveKind::TextFrame => self.text_frames.len(),
            SchPrimitiveKind::Bezier => self.beziers.len(),
            SchPrimitiveKind::Ellipse => self.ellipses.len(),
            SchPrimitiveKind::RoundRect => self.round_rects.len(),
            SchPrimitiveKind::EllipticalArc => self.elliptical_arcs.len(),
            SchPrimitiveKind::Label => self.labels.len(),
            SchPrimitiveKind::IeeeSymbol => self.ieee_symbols.len(),
            SchPrimitiveKind::Parameter => self.parameters.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind reports one style per record, and a record's colours land
    /// in the right slot: strokes for outlines and pins, fills for fillable
    /// shapes and frames, text colours for labels, frames and parameters.
    #[test]
    fn styles_of_reports_one_style_per_record_of_every_kind() {
        use SchPrimitiveKind as K;

        let mut sym = Symbol::new("KINDS");
        sym.add_pin(Pin::new("1", "A", -10, 0, 10, PinOrientation::Right));
        sym.add_rectangle(Rectangle::new(0, 0, 10, 10));
        sym.add_line(Line::new(0, 0, 10, 10));
        sym.add_polyline(Polyline::new(vec![(0.0, 0.0), (5.0, 5.0)]));
        sym.add_polygon(Polygon::new(vec![(0.0, 0.0), (10.0, 0.0), (5.0, 10.0)]));
        sym.add_arc(Arc::new(0, 0, 5, 0.0, 90.0));
        sym.add_pie(Pie::new(0, 0, 5, 0.0, 90.0));
        sym.add_image(Image::new(0, 0, 10, 10, "logo.bmp"));
        sym.add_text_frame(TextFrame::new(0, 0, 10, 10, "note"));
        sym.add_bezier(Bezier::new(0, 0, 3, 5, 7, 5, 10, 0));
        sym.add_ellipse(Ellipse::new(0, 0, 5, 3));
        sym.add_round_rect(RoundRect::new(0, 0, 10, 10, 2, 2));
        sym.add_elliptical_arc(EllipticalArc::new(0, 0, 5, 3, 0.0, 180.0));
        sym.add_label(Label::new(0, 0, "L"));
        sym.add_ieee_symbol(IeeeSymbol::new(1, 0.0, 0.0));
        sym.add_parameter(Parameter::new("Value", "1k"));

        for kind in SchPrimitiveKind::WRITE_ORDER {
            let styles = sym.styles_of(kind);
            assert_eq!(styles.len(), sym.count_of(kind), "{kind:?}");
            let style = styles[0];
            let strokes = !matches!(kind, K::Pin | K::Label | K::Parameter);
            assert_eq!(style.line_width.is_some(), strokes, "{kind:?} width");
            assert_eq!(
                style.line_color.is_some(),
                strokes || kind == K::Pin,
                "{kind:?} stroke colour"
            );
            let fills = matches!(
                kind,
                K::Rectangle
                    | K::Polygon
                    | K::Pie
                    | K::Image
                    | K::TextFrame
                    | K::Ellipse
                    | K::RoundRect
            );
            assert_eq!(style.fill_color.is_some(), fills, "{kind:?} fill");
            let text = matches!(kind, K::Label | K::TextFrame | K::Parameter);
            assert_eq!(style.text_color.is_some(), text, "{kind:?} text colour");
        }
    }

    /// A name that exactly fills the 31-unit storage cap is listed in
    /// `SectionKeys` although nothing was truncated — a UI-authored
    /// `Generic Non-polarised Capacitor` is — while a shorter one is not.
    #[test]
    fn a_name_that_fills_the_storage_cap_is_listed_in_section_keys() {
        std::fs::create_dir_all(".tmp").unwrap();
        let dir =
            tempfile::tempdir_in(std::path::Path::new(".tmp").canonicalize().unwrap()).unwrap();
        for (name, listed) in [
            ("Generic Non-polarised Capacitor", true),
            ("Generic Resistor", false),
        ] {
            assert!(name.len() <= 31);
            let mut lib = SchLib::new();
            lib.add(Symbol::new(name));
            let path = dir.path().join(format!("{}.SchLib", name.len()));
            lib.save(&path).unwrap();
            let mut cfb = cfb::open(&path).unwrap();
            assert_eq!(cfb.is_stream("/SectionKeys"), listed, "{name}");
            if listed {
                let keys = crate::altium::read_stream_opt(&mut cfb, "/SectionKeys").unwrap();
                let text = String::from_utf8_lossy(&keys);
                assert!(
                    text.contains("|KeyCount=1|LibRef0=Generic Non-polarised Capacitor|SectionKey0=Generic Non-polarised Capacitor"),
                    "{text}"
                );
            }
        }
    }

    /// `reset_identities` clears the designator record's id and every
    /// record's unique id across all sixteen lists.
    #[test]
    fn reset_identities_clears_every_record_unique_id() {
        let uid = || Some("ABCDEFGH".to_string());
        let mut s = Symbol::new("S");
        s.designator_unique_id = uid();
        let mut rect = Rectangle::new(0.0, 0.0, 1.0, 1.0);
        rect.unique_id = uid();
        s.add_rectangle(rect);
        let mut line = Line::new(0.0, 0.0, 1.0, 1.0);
        line.unique_id = uid();
        s.add_line(line);
        let mut param = Parameter::new("Value", "10k");
        param.unique_id = uid();
        s.add_parameter(param);
        let mut fpm = FootprintModel::new("RESC1608X55N");
        fpm.unique_id = uid();
        s.add_footprint(fpm);

        s.reset_identities();

        assert!(s.designator_unique_id.is_none());
        assert!(s.rectangles[0].unique_id.is_none());
        assert!(s.lines[0].unique_id.is_none());
        assert!(s.parameters[0].unique_id.is_none());
        assert!(s.footprints[0].unique_id.is_none());
        assert_eq!(s.parameters[0].value, "10k", "content untouched");
    }
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
    fn writing_systems_survive_a_write_read_cycle() {
        // Our own writer and reader, deliberately without Altium in the loop.
        //
        // The Altium-authored golden cannot cover these: a DelphiScript string
        // literal is mangled before Altium ever sees it for byte sequences the
        // scripting host's code page cannot represent, so four of the fixture's
        // symbols carry mojibake that Altium itself stored faithfully. That is a
        // limit of how the fixture is authored, not of the format layer — and
        // this test is what tells the two apart.
        //
        // Every entry is a name that cannot be written in Windows-1252, and the
        // last is long enough that its UTF-8 encoding exceeds the 31-character
        // cap on a compound-file storage name.
        let words = [
            "ᏣᎳᎩ",          // Cherokee
            "রোধক",         // Bengali
            "ᐃᓄᒃᑎᑐᑦ",       // Inuktitut syllabics
            "𠮷野",         // Han beyond the BMP: surrogate pairs
            "𞤀𞤣𞤤𞤢𞤥",        // Adlam beyond the BMP, right to left
            "ការធ្វើតេស្តយូនីកូដ", // Khmer, 19 chars and 57 UTF-8 bytes
        ];

        let mut lib = SchLib::new();
        for word in words {
            let mut symbol = Symbol::new(word);
            symbol.description = format!("desc {word}");
            symbol.add_parameter(Parameter::new("Value", word));
            lib.add(symbol);
        }

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("write");
        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("read");

        for word in words {
            let symbol = read_lib
                .get(word)
                .unwrap_or_else(|| panic!("symbol {word:?} not found; got {:?}", read_lib.names()));
            assert_eq!(
                symbol.description,
                format!("desc {word}"),
                "{word}: description"
            );
            let param = symbol
                .parameters
                .iter()
                .find(|p| p.name == "Value")
                .unwrap_or_else(|| panic!("{word}: no Value parameter"));
            assert_eq!(param.value, word, "{word}: parameter value");
        }
    }

    #[test]
    fn parameter_display_properties_round_trip() {
        // AUTOPOSITION / ISRULE / ISSYSTEMPARAMETER / TEXTHORZANCHOR / TEXTVERTANCHOR.
        // Written in memory and read back, because no golden carries them: AD24 does
        // not expose these on ISch_Parameter, so they cannot be authored by script.
        let mut symbol = Symbol::new("PARAMPROPS");
        let mut p = Parameter::new("Rule", "Width");
        p.auto_position = false;
        p.is_rule = true;
        p.is_system_parameter = true;
        p.text_horz_anchor = 2;
        p.text_vert_anchor = 1;
        p.is_mirrored = true;
        symbol.add_parameter(p);
        symbol.add_parameter(Parameter::new("Value", "10k"));

        let mut lib = SchLib::new();
        lib.add(symbol);
        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("write");
        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("read");

        let sym = read_lib.get("PARAMPROPS").expect("symbol");
        let rule = sym
            .parameters
            .iter()
            .find(|q| q.name == "Rule")
            .expect("Rule parameter");
        assert!(
            !rule.auto_position,
            "authored with auto-positioning turned off"
        );
        assert!(rule.is_rule);
        assert!(rule.is_system_parameter);
        assert_eq!(rule.text_horz_anchor, 2);
        assert_eq!(rule.text_vert_anchor, 1);
        assert!(rule.is_mirrored);

        // The second parameter is the control: a reader that set these
        // unconditionally, or leaked them between records, fails here.
        let value = sym
            .parameters
            .iter()
            .find(|q| q.name == "Value")
            .expect("Value parameter");
        assert!(value.auto_position, "an untouched parameter auto-positions");
        assert!(!value.is_rule);
        assert!(!value.is_system_parameter);
        assert_eq!(value.text_horz_anchor, 0);
        assert_eq!(value.text_vert_anchor, 0);
        assert!(!value.is_mirrored);
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
    fn roundtrip_ieee_symbol() {
        // RECORD=3 is Altium's IEEE symbol; it reads back as one, glyph,
        // scale, rotation and mirror intact, and never as a label.
        let mut symbol = Symbol::new("IEEE");
        let mut clock = IeeeSymbol::new(3, 5.0, 7.0);
        clock.rotation = 90.0;
        clock.is_mirrored = true;
        clock.scale_factor = 20.0;
        clock.color = 0xFF_00_00;
        symbol.add_ieee_symbol(clock);

        let data = writer::encode_data_stream(&symbol).expect("encode");
        let mut decoded = Symbol::new("IEEE");
        reader::parse_data_stream(&mut decoded, &data);
        assert!(decoded.labels.is_empty(), "not a label");
        assert_eq!(decoded.ieee_symbols.len(), 1);
        let back = &decoded.ieee_symbols[0];
        assert_eq!(back.symbol, 3);
        assert!((back.x - 5.0).abs() < 1e-9 && (back.y - 7.0).abs() < 1e-9);
        assert!((back.scale_factor - 20.0).abs() < 1e-9);
        assert!((back.rotation - 90.0).abs() < 1e-9);
        assert!(back.is_mirrored);
        assert_eq!(back.color, 0xFF_00_00);
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
        // An `embed_image` image WITHOUT carried bytes must still emit an
        // empty placeholder entry: the reader consumes one payload per such
        // image, so skipping it on write would steal the next image's bytes.
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
        bezier2.display_flags.graphically_locked = true;
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
        assert!(
            b2.display_flags.graphically_locked,
            "the lock flag survives a write/read cycle"
        );
        assert!(
            !b1.display_flags.graphically_locked,
            "the untouched curve is the control"
        );
    }

    #[test]
    fn roundtrip_polygon() {
        // Create a symbol with a polygon
        let mut symbol = Symbol::new("POLYGON_TEST");
        symbol.description = "Test with Polygon".to_string();

        // Add a filled triangle polygon
        let mut polygon = Polygon {
            raw_params: Vec::new(),
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
            raw_params: Vec::new(),
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
            raw_params: Vec::new(),
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
            raw_params: Vec::new(),
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
    fn wrong_file_type_real_pcblib_as_schlib() {
        // Regression for #310. The test above hand-builds a hybrid — a
        // SchLib-format length-prefixed FileHeader whose *text* names a PCB
        // library — and that shape was already detected. A real PcbLib's
        // FileHeader is a binary version-string block, so it yielded no
        // properties at all and the reader returned Ok with zero symbols: the
        // wrong-type guard never fired on the only file shape that occurs in
        // practice. An append-style caller would then have saved an empty
        // library over a real one.
        use crate::altium::pcblib::{Footprint, Pad, PcbLib};

        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("R0402");
        fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        lib.add(fp);

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("write a genuine PcbLib");

        buffer.set_position(0);
        let err = SchLib::read(&mut buffer).expect_err("a PcbLib must not read as a SchLib");
        let err_str = err.to_string();
        assert!(
            err_str.contains("Wrong file type") && err_str.contains("expected SchLib"),
            "got: {err_str}"
        );
        assert!(
            err_str.contains("PcbLib"),
            "the error should name what the file actually is, got: {err_str}"
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
            raw_params: Vec::new(),
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
            raw_params: Vec::new(),
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
            raw_params: Vec::new(),
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
            raw_params: Vec::new(),
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

    #[test]
    fn library_roundtrip_non_windows_1252_text_fields() {
        // Records are stored as Windows-1252, so any field holding text outside
        // that code page must be emitted under a `%UTF8%<Key>` key and read back
        // through it. A field that skips the promotion comes back as `?` per
        // character, which is silent data loss on a value the user typed.
        //
        // `Text`-keyed fields (a parameter's value, a symbol's designator) are
        // covered alongside the ones keyed by their own name, so the two paths
        // are asserted together.
        let cyrillic = "Опис";
        let mut sym = Symbol::new("UNICODE_FIELDS");
        sym.description = format!("{cyrillic}-описание");
        sym.designator = "U?".to_string();
        sym.source_library_name = format!("{cyrillic}.SchLib");

        let mut param = Parameter::new(cyrillic, "значение");
        param.description = format!("{cyrillic}-подпись");
        sym.add_parameter(param);

        let mut lib = SchLib::new();
        lib.add(sym);
        let mut buf = Cursor::new(Vec::new());
        lib.write(&mut buf).expect("write");
        buf.set_position(0);
        let read_back = SchLib::read(buf).expect("read");
        let s = read_back.get("UNICODE_FIELDS").expect("symbol present");

        assert_eq!(s.description, format!("{cyrillic}-описание"));
        assert_eq!(s.source_library_name, format!("{cyrillic}.SchLib"));

        let p = &s.parameters[0];
        assert_eq!(p.name, cyrillic, "parameter name");
        assert_eq!(p.value, "значение", "parameter value");
        assert_eq!(p.description, format!("{cyrillic}-подпись"));
    }

    #[test]
    fn library_roundtrip_symbol_names_in_any_script() {
        // A symbol name outside Windows-1252 must survive a write -> read cycle.
        // Both halves matter: the name is promoted so it is not `?`-mangled, and
        // components are located by walking storages, because the FileHeader's
        // LibRef list is a Windows-1252 block while a CFB storage name is UTF-16
        // — for such a name the two cannot agree, and trusting the list drops the
        // symbol entirely.
        for name in ["Резистор", "電阻", "Ωmega", "Ελλάδα", "מעגל"] {
            let mut sym = Symbol::new(name);
            sym.description = format!("{name} description");

            let mut lib = SchLib::new();
            lib.add(sym);
            let mut buf = Cursor::new(Vec::new());
            lib.write(&mut buf).expect("write");
            buf.set_position(0);
            let read_back = SchLib::read(buf).expect("read");

            assert_eq!(read_back.len(), 1, "{name}: symbol must not be dropped");
            let s = read_back
                .get(name)
                .unwrap_or_else(|| panic!("{name}: not found, got {:?}", read_back.names()));
            assert_eq!(s.name, name);
            assert_eq!(s.description, format!("{name} description"));
        }
    }

    #[test]
    fn a_library_reports_where_it_came_from_and_whether_it_holds_anything() {
        // `filepath` is how a caller re-saves in place; `is_empty` is what the
        // validator checks before reporting a library with no symbols.
        let mut lib = SchLib::new();
        assert!(
            lib.filepath().is_none(),
            "a from-scratch library has no path"
        );
        assert!(lib.is_empty());
        assert_eq!(lib.len(), 0);

        lib.add(Symbol::new("R1"));
        assert!(!lib.is_empty());
        assert_eq!(lib.len(), 1);
        assert!(lib.get_mut("R1").is_some());
        assert_eq!(lib.iter().count(), 1);
        assert_eq!(lib.iter_mut().count(), 1);
    }

    #[test]
    fn streams_this_crate_does_not_read_go_back_as_they_were() {
        // A newer Altium adds streams beside Data (a `PinFunctionData`); a
        // rewrite carries them verbatim rather than dropping what it does not
        // understand, and a reopen offers them back the same way.
        let mut symbol = Symbol::new("U1");
        symbol.add_pin(Pin::new("1", "A", -10, 0, 10, PinOrientation::Right));
        symbol.extra_streams = vec![
            ("PinFunctionData".to_string(), vec![0x01, 0x00, 0xFF, 0x7E]),
            ("Future".to_string(), b"|FUTURE=1".to_vec()),
        ];
        let mut lib = SchLib::new();
        lib.add(symbol);

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("write");
        buffer.set_position(0);
        let mut cfb = cfb::CompoundFile::open(buffer).expect("cfb");
        let mut stream = cfb
            .open_stream("/U1/PinFunctionData")
            .expect("the stream is written");
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut stream, &mut bytes).expect("read");
        assert_eq!(bytes, vec![0x01, 0x00, 0xFF, 0x7E]);

        let mut buffer = cfb.into_inner();
        buffer.set_position(0);
        let read_lib = SchLib::read(buffer).expect("read");
        let read_symbol = read_lib.get("U1").expect("symbol");
        let mut carried = read_symbol.extra_streams.clone();
        carried.sort();
        assert_eq!(
            carried,
            vec![
                ("Future".to_string(), b"|FUTURE=1".to_vec()),
                ("PinFunctionData".to_string(), vec![0x01, 0x00, 0xFF, 0x7E]),
            ]
        );
        // The streams this crate reads are not doubled up as extras.
        assert!(read_symbol
            .extra_streams
            .iter()
            .all(|(name, _)| name != "Data"));

        // The JSON form is a base64 string per stream, and reads back.
        let json = serde_json::to_value(read_symbol).expect("json");
        assert_eq!(
            json["extra_streams"].as_array().map(Vec::len),
            Some(2),
            "{json}"
        );
        let back: Symbol = serde_json::from_value(json).expect("from json");
        assert_eq!(back.extra_streams.len(), 2);
    }

    #[test]
    fn removing_a_parameter_leaves_the_other_records_where_they_were() {
        // Interleaved as a file might store it: A, pin 1, B, pin 2. Removing
        // A must not move B in front of pin 1, which is what a bare
        // `parameters.remove` does once the order has one slot too many.
        let mut symbol = Symbol::new("U1");
        symbol.add_parameter(Parameter::new("A", "1"));
        symbol.add_pin(Pin::new("1", "A", -10, 0, 10, PinOrientation::Right));
        symbol.add_parameter(Parameter::new("B", "2"));
        symbol.add_pin(Pin::new("2", "B", -10, -10, 10, PinOrientation::Right));

        let removed = symbol.remove_parameter(0);
        assert_eq!(removed.name, "A");
        assert_eq!(
            symbol.write_sequence(),
            vec![
                (SchPrimitiveKind::Pin, 0),
                (SchPrimitiveKind::Parameter, 0),
                (SchPrimitiveKind::Pin, 1),
            ]
        );

        // A symbol whose order was never recorded is unaffected.
        symbol.primitive_order.clear();
        symbol.remove_parameter(0);
        assert!(symbol.parameters.is_empty());
        assert!(symbol.primitive_order.is_empty());
    }
    /// A storage entry's 3-byte length field caps an embedded image at
    /// 16 MB compressed; a bigger one is refused with the entry named, not
    /// written wrapped.
    #[test]
    fn an_embedded_image_too_large_for_a_storage_entry_is_refused() {
        // Incompressible pseudo-random bytes, so the compressed block stays
        // over the cap.
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let payload: Vec<u8> = (0..17 * 1024 * 1024)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                #[allow(clippy::cast_possible_truncation)] // deliberate byte take
                {
                    (state >> 33) as u8
                }
            })
            .collect();

        let mut symbol = Symbol::new("HUGE_IMAGE");
        let mut image = Image::new(0, 0, 10, 10, "huge.bmp");
        image.embed_image = true;
        image.image_data = Some(payload);
        symbol.add_image(image);
        let mut lib = SchLib::new();
        lib.add(symbol);

        let err = lib
            .write(&mut Cursor::new(Vec::new()))
            .expect_err("a 17 MB incompressible entry must be refused");
        let text = err.to_string();
        assert!(text.contains("too large"), "{text}");
        assert!(text.contains("huge.bmp"), "{text}");
    }
}
