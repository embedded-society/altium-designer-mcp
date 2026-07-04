//! `PcbLib` file format handling.
//!
//! This module handles reading and writing Altium `.PcbLib` footprint library files.
//!
//! # File Structure
//!
//! A `.PcbLib` file is an OLE Compound Document containing:
//!
//! ```text
//! Root/
//! ├── FileHeader           # Library metadata (ASCII key=value pairs)
//! ├── ComponentName1/      # Storage for first footprint
//! │   ├── Data             # Primitives in binary format
//! │   └── Parameters       # Component parameters (ASCII)
//! ├── ComponentName2/      # Storage for second footprint
//! │   ├── Data
//! │   └── Parameters
//! └── ...
//! ```
//!
//! # Data Stream Binary Format
//!
//! The Data stream contains primitives in binary format:
//!
//! ```text
//! [name_block_len:4][str_len:1][name:str_len]  // Component name
//! [record_type:1][blocks...]                   // First primitive
//! [record_type:1][blocks...]                   // Second primitive
//! ...
//! [0x00]                                       // End marker
//! ```
//!
//! Record types: Arc(1), Pad(2), Via(3), Track(4), Text(5), Fill(6), Region(11), ComponentBody(12)

mod flags;
pub mod primitives;
mod read_io;
mod reader;
mod units;
mod write_io;
mod writer;

use serde::{Deserialize, Serialize};

pub use primitives::{
    Arc, ComponentBody, EmbeddedModel, Fill, HoleShape, Layer, MaskExpansionMode, Model3D, Pad,
    PadShape, PadStackMode, PcbFlags, PowerPlaneConnectStyle, Region, RegionKind, StrokeFont, Text,
    TextJustification, TextKind, Track, Vertex, Via,
};

use crate::altium::error::{AltiumError, AltiumResult};

/// Internal OLE storage entries that should be filtered out when reading `PcbLib` files.
/// These are not actual footprints, but internal Altium data structures.
const INTERNAL_OLE_ENTRIES: &[&str] = &[
    "FileHeader",
    "Library",
    "Models",
    "Textures",
    "ModelsNoEmbed",
    "PadViaLibrary",
    "LayerKindMapping",
    "ComponentParamsTOC",
    "FileVersionInfo",
    "PrimitiveGuids",
    "UniqueIDPrimitiveInformation",
];

/// A complete PCB footprint.
///
/// # Example
///
/// ```
/// use altium_designer_mcp::altium::pcblib::{Footprint, Pad, Track, Layer};
///
/// let mut footprint = Footprint::new("RESC1608X55N");
/// footprint.description = "Chip Resistor 1608 (0603)".to_string();
///
/// // Add SMD pads
/// footprint.add_pad(Pad::smd("1", -0.75, 0.0, 0.85, 0.95));
/// footprint.add_pad(Pad::smd("2", 0.75, 0.0, 0.85, 0.95));
///
/// // Add silkscreen outline
/// footprint.add_track(Track::new(-0.35, 0.5, 0.35, 0.5, 0.15, Layer::TopOverlay));
/// footprint.add_track(Track::new(-0.35, -0.5, 0.35, -0.5, 0.15, Layer::TopOverlay));
///
/// assert_eq!(footprint.pads.len(), 2);
/// assert_eq!(footprint.tracks.len(), 2);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Footprint {
    /// Footprint name (e.g., "RESC1608X55N").
    pub name: String,

    /// Description of the footprint.
    #[serde(default)]
    pub description: String,

    /// Pads in the footprint.
    #[serde(default)]
    pub pads: Vec<Pad>,

    /// Vias in the footprint.
    #[serde(default)]
    pub vias: Vec<Via>,

    /// Tracks (lines) in the footprint.
    #[serde(default)]
    pub tracks: Vec<Track>,

    /// Arcs in the footprint.
    #[serde(default)]
    pub arcs: Vec<Arc>,

    /// Filled regions in the footprint.
    #[serde(default)]
    pub regions: Vec<Region>,

    /// Text items in the footprint.
    #[serde(default)]
    pub text: Vec<Text>,

    /// Filled rectangles in the footprint.
    #[serde(default)]
    pub fills: Vec<primitives::Fill>,

    /// 3D component bodies (embedded models).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub component_bodies: Vec<primitives::ComponentBody>,

    /// 3D model reference (legacy, use `component_bodies` for new code).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_3d: Option<Model3D>,
}

impl Footprint {
    /// Creates a new empty footprint with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            pads: Vec::new(),
            vias: Vec::new(),
            tracks: Vec::new(),
            arcs: Vec::new(),
            regions: Vec::new(),
            text: Vec::new(),
            fills: Vec::new(),
            component_bodies: Vec::new(),
            model_3d: None,
        }
    }

    /// Adds a pad to the footprint.
    pub fn add_pad(&mut self, pad: Pad) {
        self.pads.push(pad);
    }

    /// Adds a via to the footprint.
    pub fn add_via(&mut self, via: Via) {
        self.vias.push(via);
    }

    /// Adds a track to the footprint.
    pub fn add_track(&mut self, track: Track) {
        self.tracks.push(track);
    }

    /// Adds an arc to the footprint.
    pub fn add_arc(&mut self, arc: Arc) {
        self.arcs.push(arc);
    }

    /// Adds a region to the footprint.
    pub fn add_region(&mut self, region: Region) {
        self.regions.push(region);
    }

    /// Adds text to the footprint.
    pub fn add_text(&mut self, text: Text) {
        self.text.push(text);
    }

    /// Adds a fill to the footprint.
    pub fn add_fill(&mut self, fill: primitives::Fill) {
        self.fills.push(fill);
    }

    /// Adds a component body (3D model) to the footprint.
    pub fn add_component_body(&mut self, body: primitives::ComponentBody) {
        self.component_bodies.push(body);
    }
}

/// Library metadata parsed from the `FileHeader` stream.
///
/// The `FileHeader` contains metadata about the library as a whole,
/// including component names and descriptions indexed by position.
#[derive(Debug, Clone, Default)]
pub struct LibraryMetadata {
    /// File type identifier (e.g., "Protel for Windows - PCB Library").
    pub header: String,

    /// Component count from `CompCount` field.
    pub component_count: usize,

    /// Component names by index from `LibRef{N}` fields.
    ///
    /// Note: These may not match the footprint names stored in each
    /// component's Parameters stream (PATTERN field), which can be longer
    /// than the 31-character OLE storage name limit.
    pub component_names: Vec<String>,

    /// Component descriptions by index from `CompDescr{N}` fields.
    pub component_descriptions: Vec<String>,
}

/// A `PcbLib` footprint library.
///
/// # Example
///
/// ```no_run
/// use altium_designer_mcp::altium::pcblib::{PcbLib, Footprint, Pad};
///
/// // Create a new library and add footprints
/// let mut lib = PcbLib::new();
///
/// let mut footprint = Footprint::new("RESC1608X55N");
/// footprint.add_pad(Pad::smd("1", -0.75, 0.0, 0.85, 0.95));
/// footprint.add_pad(Pad::smd("2", 0.75, 0.0, 0.85, 0.95));
/// lib.add(footprint);
///
/// // Save to file
/// lib.save("MyLibrary.PcbLib").unwrap();
///
/// // Open an existing library
/// let lib = PcbLib::open("MyLibrary.PcbLib").unwrap();
/// for name in lib.names() {
///     println!("Footprint: {name}");
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct PcbLib {
    /// Library file path (if loaded from file).
    filepath: Option<String>,

    /// Footprints in the library.
    footprints: Vec<Footprint>,

    /// Embedded 3D models from `/Library/Models/` storage.
    ///
    /// These are zlib-compressed STEP files that are referenced by
    /// `ComponentBody` records via their GUID.
    models: Vec<EmbeddedModel>,

    /// Library metadata from the `FileHeader` stream.
    metadata: LibraryMetadata,
}

impl PcbLib {
    /// Creates a new empty library.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a `PcbLib` from a file path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or is not a valid `PcbLib`.
    pub fn open(path: impl AsRef<std::path::Path>) -> AltiumResult<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).map_err(|e| AltiumError::file_read(path, e))?;

        let mut lib = Self::read(file)?;
        lib.filepath = Some(path.display().to_string());
        Ok(lib)
    }

    /// Saves the library to a file.
    ///
    /// Uses atomic write: writes to a temporary file first, then renames on success.
    /// This prevents data loss if the write fails partway through.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&mut self, path: impl AsRef<std::path::Path>) -> AltiumResult<()> {
        crate::altium::save_atomic(path.as_ref(), "pcblib.tmp", |file| self.write(file))
    }

    /// Returns the number of footprints in the library.
    #[must_use]
    pub fn len(&self) -> usize {
        self.footprints.len()
    }

    /// Returns true if the library is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.footprints.is_empty()
    }

    /// Returns an iterator over the footprints.
    pub fn iter(&self) -> impl Iterator<Item = &Footprint> {
        self.footprints.iter()
    }

    /// Returns a mutable iterator over the footprints.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Footprint> {
        self.footprints.iter_mut()
    }

    /// Returns a list of footprint names.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.footprints.iter().map(|f| f.name.clone()).collect()
    }

    /// Returns the file path this library was loaded from, if any.
    #[must_use]
    pub fn filepath(&self) -> Option<&str> {
        self.filepath.as_deref()
    }

    /// Gets a footprint by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Footprint> {
        self.footprints.iter().find(|f| f.name == name)
    }

    /// Gets a mutable reference to a footprint by name.
    #[must_use]
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Footprint> {
        self.footprints.iter_mut().find(|f| f.name == name)
    }

    /// Adds a footprint to the library.
    pub fn add(&mut self, footprint: Footprint) {
        self.footprints.push(footprint);
    }

    /// Removes a footprint from the library by name.
    ///
    /// Returns the removed footprint if found, or `None` if no footprint with that name exists.
    pub fn remove(&mut self, name: &str) -> Option<Footprint> {
        if let Some(idx) = self.footprints.iter().position(|f| f.name == name) {
            Some(self.footprints.remove(idx))
        } else {
            None
        }
    }

    /// Updates a footprint in-place, preserving its position in the library.
    ///
    /// The footprint is matched by the `name` parameter. The replacement footprint's
    /// name does not need to match (allowing renames).
    ///
    /// Returns the old footprint if found, or `None` if no footprint with that name exists.
    pub fn update(&mut self, name: &str, replacement: Footprint) -> Option<Footprint> {
        if let Some(idx) = self.footprints.iter().position(|f| f.name == name) {
            Some(std::mem::replace(&mut self.footprints[idx], replacement))
        } else {
            None
        }
    }

    /// Reorders footprints according to the given name order.
    ///
    /// Footprints are reordered to match the order of names in `new_order`.
    /// Names not present in the library are ignored. Footprints not mentioned
    /// in `new_order` are placed at the end in their original relative order.
    ///
    /// Returns the new order of footprint names.
    pub fn reorder(&mut self, new_order: &[&str]) -> Vec<String> {
        // Stable-sort footprints into the desired order; footprints not listed
        // in `new_order` keep their relative order at the end.
        let rank = crate::altium::order_ranker(new_order);
        self.footprints.sort_by_key(|a| rank(a.name.as_str()));

        self.names()
    }

    /// Returns the number of embedded 3D models in the library.
    #[must_use]
    pub fn model_count(&self) -> usize {
        self.models.len()
    }

    /// Returns an iterator over the embedded 3D models.
    pub fn models(&self) -> impl Iterator<Item = &EmbeddedModel> {
        self.models.iter()
    }

    /// Gets an embedded model by GUID.
    ///
    /// GUID matching is case-insensitive since Altium files may store GUIDs
    /// with inconsistent casing between component body references and the model index.
    #[must_use]
    pub fn get_model(&self, id: &str) -> Option<&EmbeddedModel> {
        self.models.iter().find(|m| m.id.eq_ignore_ascii_case(id))
    }

    /// Adds an embedded 3D model to the library.
    pub fn add_model(&mut self, model: EmbeddedModel) {
        self.models.push(model);
    }

    /// Returns all model GUIDs referenced by footprints in this library.
    ///
    /// GUIDs are normalised to lowercase for consistent matching.
    #[must_use]
    pub fn referenced_model_ids(&self) -> std::collections::HashSet<String> {
        let mut ids = std::collections::HashSet::new();
        for fp in &self.footprints {
            for cb in &fp.component_bodies {
                if cb.embedded {
                    ids.insert(cb.model_id.to_lowercase());
                }
            }
        }
        ids
    }

    /// Removes models that are not referenced by any footprint.
    ///
    /// This should be called after deleting footprints to prevent library bloat
    /// from orphaned embedded models.
    ///
    /// Returns the number of models removed.
    pub fn remove_orphaned_models(&mut self) -> usize {
        let referenced = self.referenced_model_ids();
        let original_count = self.models.len();
        self.models
            .retain(|m| referenced.contains(&m.id.to_lowercase()));
        let removed = original_count - self.models.len();
        if removed > 0 {
            tracing::debug!(removed, "Removed orphaned embedded models");
        }
        removed
    }

    /// Returns all model GUIDs that exist in the library's model collection.
    ///
    /// GUIDs are normalised to lowercase for consistent matching.
    #[must_use]
    pub fn available_model_ids(&self) -> std::collections::HashSet<String> {
        self.models.iter().map(|m| m.id.to_lowercase()).collect()
    }

    /// Removes component body references that point to non-existent models.
    ///
    /// This repairs libraries where `component_bodies` have `embedded: true`
    /// but the actual model data is missing from `/Library/Models/`.
    ///
    /// Returns a vector of (`footprint_name`, `removed_count`) for each affected footprint.
    pub fn remove_orphaned_component_bodies(&mut self) -> Vec<(String, usize)> {
        let available = self.available_model_ids();
        let mut results = Vec::new();

        for footprint in &mut self.footprints {
            let original_count = footprint.component_bodies.len();
            footprint.component_bodies.retain(|cb| {
                // Keep external references (embedded: false) - they don't need model data
                if !cb.embedded {
                    return true;
                }
                // Keep if model_id is empty (shouldn't happen but be safe)
                if cb.model_id.is_empty() {
                    return true;
                }
                // Keep only if the model exists in the library
                available.contains(&cb.model_id.to_lowercase())
            });
            let removed = original_count - footprint.component_bodies.len();
            if removed > 0 {
                tracing::debug!(
                    footprint = %footprint.name,
                    removed,
                    "Removed orphaned component body references"
                );
                results.push((footprint.name.clone(), removed));
            }
        }

        results
    }

    /// Returns a reference to the library metadata.
    ///
    /// The metadata contains information parsed from the `FileHeader` stream,
    /// including component names and descriptions.
    #[must_use]
    pub const fn metadata(&self) -> &LibraryMetadata {
        &self.metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to compare floats with tolerance.
    fn approx_eq(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() < tolerance
    }

    #[test]
    fn footprint_creation() {
        let mut fp = Footprint::new("TEST");
        fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.8, 0.9));
        fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.8, 0.9));

        assert_eq!(fp.name, "TEST");
        assert_eq!(fp.pads.len(), 2);
    }

    #[test]
    fn file_version_info_frames_to_canonical_bytes() {
        // The embedded FileVersionInfo text must frame to exactly the bytes
        // Altium expects. Guards the asset against accidental mangling (a stray
        // newline, an editor re-encode, git EOL normalisation) that would
        // silently change the emitted /FileVersionInfo stream.
        let mut data = Vec::new();
        crate::altium::framing::write_cstring_param_block(&mut data, PcbLib::FVI_TEXT.as_bytes());
        assert_eq!(data.len(), 2573, "FileVersionInfo stream size changed");
        assert_eq!(&data[..4], &[0x09, 0x0a, 0x00, 0x00]); // LE length prefix = 2569
        assert_eq!(*data.last().unwrap(), 0x00);
        assert!(PcbLib::FVI_TEXT.starts_with("|COUNT="));
    }

    #[test]
    fn library_operations() {
        let mut lib = PcbLib::new();
        assert!(lib.is_empty());

        lib.add(Footprint::new("FP1"));
        lib.add(Footprint::new("FP2"));

        assert_eq!(lib.len(), 2);
        assert_eq!(lib.names(), vec!["FP1", "FP2"]);
        assert!(lib.get("FP1").is_some());
        assert!(lib.get("FP3").is_none());
    }

    #[test]
    fn library_reorder() {
        let mut lib = PcbLib::new();

        lib.add(Footprint::new("A"));
        lib.add(Footprint::new("B"));
        lib.add(Footprint::new("C"));
        lib.add(Footprint::new("D"));

        assert_eq!(lib.names(), vec!["A", "B", "C", "D"]);

        // Reorder: C, A first; B, D should follow in original relative order
        let new_order = lib.reorder(&["C", "A"]);
        assert_eq!(new_order, vec!["C", "A", "B", "D"]);

        // Reorder with non-existent names (should be ignored)
        let new_order = lib.reorder(&["D", "X", "B"]);
        assert_eq!(new_order, vec!["D", "B", "C", "A"]);

        // Reorder to completely reverse
        let new_order = lib.reorder(&["A", "C", "B", "D"]);
        assert_eq!(new_order, vec!["A", "C", "B", "D"]);
    }

    #[test]
    fn binary_roundtrip_pads() {
        // Create a footprint with pads
        let mut original = Footprint::new("ROUNDTRIP_PAD");
        original.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        original.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));

        // Encode to binary
        let data = writer::encode_data_stream(&original).expect("encoding should succeed");

        // Decode from binary
        let mut decoded = Footprint::new("ROUNDTRIP_PAD");
        reader::parse_data_stream(&mut decoded, &data, None);

        // Verify
        assert_eq!(decoded.pads.len(), 2);
        assert_eq!(decoded.pads[0].designator, "1");
        assert_eq!(decoded.pads[1].designator, "2");
        assert!(approx_eq(decoded.pads[0].x, -0.5, 0.001));
        assert!(approx_eq(decoded.pads[1].x, 0.5, 0.001));
        assert!(approx_eq(decoded.pads[0].width, 0.6, 0.001));
        assert!(approx_eq(decoded.pads[0].height, 0.5, 0.001));
    }

    #[test]
    fn binary_roundtrip_pad_thermal_relief() {
        // A pad with NON-default thermal-relief / power-plane settings must survive
        // encode -> decode with all six fields intact.
        let mut original = Footprint::new("ROUNDTRIP_PAD_RELIEF");
        let mut pad = Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8);
        pad.power_plane_connect_style = PowerPlaneConnectStyle::Direct;
        pad.relief_conductor_width = 0.3; // != 0.254 default
        pad.relief_entries = 2; // != 4 default
        pad.relief_air_gap = 0.2; // != 0.254 default
        pad.power_plane_relief_expansion = 0.6; // != 0.508 default
        pad.power_plane_clearance = 0.7; // != 0.508 default
        original.add_pad(pad);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_PAD_RELIEF");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 1);
        let p = &decoded.pads[0];
        assert_eq!(p.power_plane_connect_style, PowerPlaneConnectStyle::Direct);
        assert!(approx_eq(p.relief_conductor_width, 0.3, 0.0001));
        assert_eq!(p.relief_entries, 2);
        assert!(approx_eq(p.relief_air_gap, 0.2, 0.0001));
        assert!(approx_eq(p.power_plane_relief_expansion, 0.6, 0.0001));
        assert!(approx_eq(p.power_plane_clearance, 0.7, 0.0001));
    }

    #[test]
    fn pad_default_thermal_relief_byte_identical() {
        // A pad created with default thermal-relief must produce byte-for-byte
        // identical output regardless of whether the writer emits the struct
        // fields or the old fixed template constants. We prove this by checking
        // that the default field values map back to the canonical template raw
        // values (style 0; conductor width / air gap 100000; entries 4; relief
        // expansion / clearance 200000), so the oracle stays at 0 regressions.
        let pad = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        assert_eq!(
            pad.power_plane_connect_style,
            PowerPlaneConnectStyle::Relief
        );
        assert_eq!(pad.power_plane_connect_style.to_id(), 0);
        assert_eq!(units::from_mm(pad.relief_conductor_width), 100_000);
        assert_eq!(pad.relief_entries, 4);
        assert_eq!(units::from_mm(pad.relief_air_gap), 100_000);
        assert_eq!(units::from_mm(pad.power_plane_relief_expansion), 200_000);
        assert_eq!(units::from_mm(pad.power_plane_clearance), 200_000);
    }

    #[test]
    fn binary_roundtrip_pad_slot_hole_and_tolerances() {
        // PR-8: a pad with a slot hole (non-zero slot length + rotation) and
        // non-default drill tolerances must survive encode -> decode.
        let mut original = Footprint::new("ROUNDTRIP_PAD_SLOT");
        let mut pad = Pad::through_hole("1", 0.0, 0.0, 2.0, 1.2, 0.8);
        pad.hole_shape = HoleShape::Slot;
        pad.hole_slot_length = 1.5; // != 0 default
        pad.hole_rotation = 45.0; // != 0 default
        pad.hole_positive_tolerance = Some(0.05);
        pad.hole_negative_tolerance = Some(0.02);
        original.add_pad(pad);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_PAD_SLOT");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 1);
        let p = &decoded.pads[0];
        assert_eq!(p.hole_shape, HoleShape::Slot);
        assert!(approx_eq(p.hole_slot_length, 1.5, 0.0001));
        assert!(approx_eq(p.hole_rotation, 45.0, 0.0001));
        assert!(approx_eq(p.hole_positive_tolerance.unwrap(), 0.05, 0.0001));
        assert!(approx_eq(p.hole_negative_tolerance.unwrap(), 0.02, 0.0001));
    }

    #[test]
    fn pad_default_slot_hole_fields_byte_identical() {
        // A default (round-hole, unset-tolerance) pad must map its new fields back to
        // exactly the writer's previous hard-coded values so the oracle stays at 0
        // regressions: slot length 0, rotation 0, tolerances -> 0x7FFFFFFF sentinel.
        let pad = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        assert_eq!(units::from_mm(pad.hole_slot_length), 0); // writer hard-coded 0
        assert!(approx_eq(pad.hole_rotation, 0.0, 1e-9)); // writer hard-coded 0.0
        assert_eq!(pad.hole_positive_tolerance, None); // None -> sentinel
        assert_eq!(pad.hole_negative_tolerance, None);
    }

    #[test]
    fn binary_roundtrip_tracks() {
        let mut original = Footprint::new("ROUNDTRIP_TRACK");
        original.add_track(Track::new(-1.0, -0.5, 1.0, -0.5, 0.15, Layer::TopOverlay));
        original.add_track(Track::new(1.0, -0.5, 1.0, 0.5, 0.15, Layer::TopOverlay));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_TRACK");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.tracks.len(), 2);
        assert!(approx_eq(decoded.tracks[0].x1, -1.0, 0.001));
        assert!(approx_eq(decoded.tracks[0].x2, 1.0, 0.001));
        assert!(approx_eq(decoded.tracks[0].width, 0.15, 0.001));
        assert_eq!(decoded.tracks[0].layer, Layer::TopOverlay);
    }

    #[test]
    fn binary_roundtrip_arcs() {
        let mut original = Footprint::new("ROUNDTRIP_ARC");
        original.add_arc(Arc::circle(0.0, 0.0, 1.0, 0.15, Layer::TopOverlay));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_ARC");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.arcs.len(), 1);
        assert!(approx_eq(decoded.arcs[0].x, 0.0, 0.001));
        assert!(approx_eq(decoded.arcs[0].y, 0.0, 0.001));
        assert!(approx_eq(decoded.arcs[0].radius, 1.0, 0.001));
        assert!(approx_eq(decoded.arcs[0].start_angle, 0.0, 0.001));
        assert!(approx_eq(decoded.arcs[0].end_angle, 360.0, 0.001));
    }

    #[test]
    fn binary_roundtrip_mixed_primitives() {
        let mut original = Footprint::new("ROUNDTRIP_MIXED");

        // Add arcs first (record type 0x01)
        original.add_arc(Arc::circle(0.0, 0.0, 0.5, 0.1, Layer::TopOverlay));

        // Add pads (record type 0x02)
        original.add_pad(Pad::smd("1", -1.0, 0.0, 0.6, 0.5));
        original.add_pad(Pad::smd("2", 1.0, 0.0, 0.6, 0.5));

        // Add tracks (record type 0x04)
        original.add_track(Track::new(-1.5, -0.3, 1.5, -0.3, 0.12, Layer::TopOverlay));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_MIXED");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.arcs.len(), 1);
        assert_eq!(decoded.pads.len(), 2);
        assert_eq!(decoded.tracks.len(), 1);
    }

    #[test]
    fn binary_roundtrip_coordinate_precision() {
        let mut original = Footprint::new("ROUNDTRIP_PRECISION");

        // Test various coordinate values
        original.add_pad(Pad::smd("1", 0.125, 0.0, 0.3, 0.4));
        original.add_pad(Pad::smd("2", 1.27, 0.0, 0.5, 0.5));
        original.add_pad(Pad::smd("3", 2.54, 0.0, 1.0, 1.0));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_PRECISION");
        reader::parse_data_stream(&mut decoded, &data, None);

        // Altium internal units give ~2.54nm resolution
        assert!(approx_eq(decoded.pads[0].x, 0.125, 0.0001));
        assert!(approx_eq(decoded.pads[1].x, 1.27, 0.0001));
        assert!(approx_eq(decoded.pads[2].x, 2.54, 0.0001));
    }

    #[test]
    fn binary_roundtrip_through_hole_pad() {
        let mut original = Footprint::new("ROUNDTRIP_TH");
        original.add_pad(Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_TH");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 1);
        assert!(decoded.pads[0].hole_size.is_some());
        assert!(approx_eq(decoded.pads[0].hole_size.unwrap(), 0.8, 0.001));
    }

    #[test]
    fn binary_roundtrip_component_layers() {
        // Test that component layer pairs roundtrip correctly
        let mut original = Footprint::new("ROUNDTRIP_LAYERS");

        // Add tracks on each component layer pair
        original.add_track(Track::new(-1.0, 0.0, 1.0, 0.0, 0.1, Layer::TopAssembly));
        original.add_track(Track::new(-1.0, 0.1, 1.0, 0.1, 0.1, Layer::TopCourtyard));
        original.add_track(Track::new(-1.0, 0.2, 1.0, 0.2, 0.1, Layer::Top3DBody));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_LAYERS");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.tracks.len(), 3);
        assert_eq!(decoded.tracks[0].layer, Layer::TopAssembly);
        assert_eq!(decoded.tracks[1].layer, Layer::TopCourtyard);
        assert_eq!(decoded.tracks[2].layer, Layer::Top3DBody);
    }

    #[test]
    fn binary_roundtrip_text() {
        let mut original = Footprint::new("ROUNDTRIP_TEXT");

        // Add text with different rotations
        original.add_text(Text {
            x: 0.0,
            y: 1.0,
            text: ".Designator".to_string(),
            height: 0.8,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            justification: TextJustification::MiddleCenter,
            flags: PcbFlags::empty(),
            unique_id: None,
        });
        original.add_text(Text {
            x: 1.5,
            y: 0.5,
            text: "TEST".to_string(),
            height: 0.5,
            layer: Layer::TopOverlay,
            rotation: 90.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            justification: TextJustification::TopLeft,
            flags: PcbFlags::empty(),
            unique_id: None,
        });

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_TEXT");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.text.len(), 2);

        // First text
        assert_eq!(decoded.text[0].text, ".Designator");
        assert!(approx_eq(decoded.text[0].x, 0.0, 0.001));
        assert!(approx_eq(decoded.text[0].y, 1.0, 0.001));
        assert!(approx_eq(decoded.text[0].height, 0.8, 0.001));
        assert!(approx_eq(decoded.text[0].rotation, 0.0, 0.001));
        assert_eq!(decoded.text[0].layer, Layer::TopOverlay);

        // Second text (rotated)
        assert_eq!(decoded.text[1].text, "TEST");
        assert!(approx_eq(decoded.text[1].x, 1.5, 0.001));
        assert!(approx_eq(decoded.text[1].y, 0.5, 0.001));
        assert!(approx_eq(decoded.text[1].height, 0.5, 0.001));
        assert!(approx_eq(decoded.text[1].rotation, 90.0, 0.001));
    }

    #[test]
    fn text_stroke_width_round_trips() {
        // Previously every text inherited the 252-byte template's 4-mil stroke; an
        // explicit StrokeWidth (geometry offset 36) must now survive the round-trip.
        let mut original = Footprint::new("TEXT_STROKE");
        original.add_text(Text {
            x: 0.0,
            y: 0.0,
            text: "W".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: Some(0.2),
            italic: false,
            justification: TextJustification::MiddleCenter,
            flags: PcbFlags::empty(),
            unique_id: None,
        });

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("TEXT_STROKE");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.text.len(), 1);
        let w = decoded.text[0]
            .stroke_width
            .expect("explicit stroke width should round-trip");
        assert!(approx_eq(w, 0.2, 0.001), "expected 0.2 mm, got {w}");
    }

    #[test]
    fn text_truetype_italic_round_trips() {
        // A TrueType italic text must round-trip italic@45 and derive baseFontType@43=1.
        let mut original = Footprint::new("TT_ITALIC");
        original.add_text(Text {
            x: 0.0,
            y: 0.0,
            text: "String".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::TrueType,
            stroke_font: None,
            stroke_width: None,
            italic: true,
            justification: TextJustification::MiddleCenter,
            flags: PcbFlags::empty(),
            unique_id: None,
        });

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("TT_ITALIC");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.text.len(), 1);
        assert_eq!(decoded.text[0].kind, TextKind::TrueType);
        assert!(decoded.text[0].italic, "italic must survive the round-trip");
    }

    #[test]
    fn text_default_stroke_geometry_byte_identical() {
        // Guards the oracle: a from-scratch stroke text with default styling must emit
        // the unmodified template at the styling offsets (43, 45 == 0).
        let geom = writer::encode_text_geometry(&Text {
            x: 0.0,
            y: 0.0,
            text: "X".into(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            justification: TextJustification::MiddleCenter,
            flags: PcbFlags::empty(),
            unique_id: None,
        });
        assert_eq!(
            geom[43], 0x00,
            "stroke baseFontType must stay template default"
        );
        assert_eq!(geom[45], 0x00, "non-italic must stay template default");
    }

    #[test]
    fn binary_roundtrip_text_flags() {
        // parse_text previously discarded the flag word (read PcbFlags::empty());
        // a locked / tented text must now round-trip its flags.
        let mut original = Footprint::new("TEXT_FLAGS");
        original.add_text(Text {
            x: 0.0,
            y: 0.0,
            text: "LOCKED".to_string(),
            height: 0.5,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            justification: TextJustification::MiddleCenter,
            flags: PcbFlags::LOCKED | PcbFlags::TENTING_TOP,
            unique_id: None,
        });

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("TEXT_FLAGS");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.text.len(), 1);
        assert!(decoded.text[0].flags.contains(PcbFlags::LOCKED));
        assert!(decoded.text[0].flags.contains(PcbFlags::TENTING_TOP));
    }

    #[test]
    fn binary_roundtrip_region() {
        let mut original = Footprint::new("ROUNDTRIP_REGION");

        // Add a triangular region (similar to user's sample)
        original.add_region(Region {
            vertices: vec![
                Vertex {
                    x: -2.286,
                    y: 1.778,
                },
                Vertex {
                    x: -0.762,
                    y: 1.778,
                },
                Vertex {
                    x: -1.524,
                    y: 1.016,
                },
            ],
            layer: Layer::TopAssembly,
            ..Region::default()
        });

        // Add a rectangular region
        original.add_region(Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopCourtyard));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_REGION");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.regions.len(), 2);

        // Triangle
        assert_eq!(decoded.regions[0].vertices.len(), 3);
        assert!(approx_eq(decoded.regions[0].vertices[0].x, -2.286, 0.001));
        assert!(approx_eq(decoded.regions[0].vertices[0].y, 1.778, 0.001));
        assert!(approx_eq(decoded.regions[0].vertices[1].x, -0.762, 0.001));
        assert!(approx_eq(decoded.regions[0].vertices[2].x, -1.524, 0.001));
        assert_eq!(decoded.regions[0].layer, Layer::TopAssembly);

        // Rectangle
        assert_eq!(decoded.regions[1].vertices.len(), 4);
        assert_eq!(decoded.regions[1].layer, Layer::TopCourtyard);
    }

    #[test]
    fn binary_roundtrip_fill() {
        use super::primitives::Fill;

        let mut original = Footprint::new("ROUNDTRIP_FILL");

        // Add a simple fill rectangle
        original.add_fill(Fill::new(-2.0, -1.0, 2.0, 1.0, Layer::TopPaste));

        // Add a rotated fill
        original.add_fill(Fill {
            x1: -1.5,
            y1: -0.5,
            x2: 1.5,
            y2: 0.5,
            layer: Layer::BottomPaste,
            rotation: 45.0,
            flags: PcbFlags::empty(),
            solder_mask_expansion: None,
            keepout_restrictions: None,
            unique_id: None,
        });

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_FILL");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.fills.len(), 2);

        // First fill
        assert!(approx_eq(decoded.fills[0].x1, -2.0, 0.001));
        assert!(approx_eq(decoded.fills[0].y1, -1.0, 0.001));
        assert!(approx_eq(decoded.fills[0].x2, 2.0, 0.001));
        assert!(approx_eq(decoded.fills[0].y2, 1.0, 0.001));
        assert_eq!(decoded.fills[0].layer, Layer::TopPaste);
        assert!(approx_eq(decoded.fills[0].rotation, 0.0, 0.001));

        // Second fill (rotated)
        assert!(approx_eq(decoded.fills[1].x1, -1.5, 0.001));
        assert!(approx_eq(decoded.fills[1].y1, -0.5, 0.001));
        assert!(approx_eq(decoded.fills[1].x2, 1.5, 0.001));
        assert!(approx_eq(decoded.fills[1].y2, 0.5, 0.001));
        assert_eq!(decoded.fills[1].layer, Layer::BottomPaste);
        assert!(approx_eq(decoded.fills[1].rotation, 45.0, 0.001));
    }

    #[test]
    fn fill_extended_tail_round_trips() {
        use super::primitives::Fill;

        // Solder-mask expansion @37-40 and keepout @46 round-trip; a default fill
        // stays None (additive — byte-identical to the old zero-tail output).
        let mut fp = Footprint::new("FILL_TAIL");
        let mut fill = Fill::new(0.0, 0.0, 2.0, 1.0, Layer::TopLayer);
        fill.solder_mask_expansion = Some(0.1);
        fill.keepout_restrictions = Some(0x05);
        fp.add_fill(fill);
        fp.add_fill(Fill::new(5.0, 0.0, 6.0, 1.0, Layer::TopOverlay));

        let data = writer::encode_data_stream(&fp).expect("encode");
        let mut decoded = Footprint::new("FILL_TAIL");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.fills.len(), 2);
        assert!(approx_eq(
            decoded.fills[0].solder_mask_expansion.unwrap(),
            0.1,
            0.001
        ));
        assert_eq!(decoded.fills[0].keepout_restrictions, Some(0x05));
        // Additive: the default fill did not gain these fields.
        assert_eq!(decoded.fills[1].solder_mask_expansion, None);
        assert_eq!(decoded.fills[1].keepout_restrictions, None);
    }

    #[test]
    fn binary_roundtrip_component_body() {
        use super::primitives::ComponentBody;

        let mut original = Footprint::new("ROUNDTRIP_COMPONENT_BODY");

        // Add a ComponentBody with typical values and an explicit outline.
        let body = ComponentBody {
            model_id: "{TEST-GUID-1234-5678-ABCDEFGH}".to_string(),
            model_name: "TEST_MODEL.step".to_string(),
            embedded: true,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 45.0,
            z_offset: 0.5,        // mm
            overall_height: 1.0,  // mm
            standoff_height: 0.1, // mm
            layer: Layer::Top3DBody,
            outline: vec![(-2.0, 1.0), (-2.0, -1.0), (2.0, -1.0), (2.0, 1.0)],
            unique_id: None,
            model_checksum: 7_654_321,
            name: " ".to_string(),
            kind: 0,
            sub_poly_index: -1,
            union_index: 0,
            is_shape_based: false,
            body_projection: 0,
            body_color_3d: 8_421_504,
            body_opacity_3d: 1.0,
            model_2d_rotation: 0.0,
        };
        original.add_component_body(body);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_COMPONENT_BODY");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.component_bodies.len(), 1);

        let body = &decoded.component_bodies[0];
        assert_eq!(body.model_id, "{TEST-GUID-1234-5678-ABCDEFGH}");
        assert_eq!(body.model_name, "TEST_MODEL.step");
        assert!(body.embedded);
        assert!(approx_eq(body.rotation_x, 0.0, 0.001));
        assert!(approx_eq(body.rotation_y, 0.0, 0.001));
        assert!(approx_eq(body.rotation_z, 45.0, 0.001));
        // Heights are converted to/from mils with some precision loss
        assert!(approx_eq(body.z_offset, 0.5, 0.01));
        assert!(approx_eq(body.overall_height, 1.0, 0.01));
        assert!(approx_eq(body.standoff_height, 0.1, 0.01));
        assert_eq!(body.layer, Layer::Top3DBody);
        // MODEL.CHECKSUM round-trips verbatim (previously dropped + hard-coded to 0).
        assert_eq!(body.model_checksum, 7_654_321);

        // The explicit outline round-trips (4 vertices, in mm).
        assert_eq!(body.outline.len(), 4);
        assert!(approx_eq(body.outline[0].0, -2.0, 0.001));
        assert!(approx_eq(body.outline[0].1, 1.0, 0.001));
        assert!(approx_eq(body.outline[2].0, 2.0, 0.001));
        assert!(approx_eq(body.outline[2].1, -1.0, 0.001));
    }

    #[test]
    fn component_body_emits_single_block_with_outline() {
        // A footprint with two bodies must not emit stray empty blocks between
        // them (Altium reads exactly one block per body; trailing zero bytes are
        // mis-read as a bogus object-id-0 primitive — the #68 class of bug).
        let mut fp = Footprint::new("TWO_BODIES");
        fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.6, 0.5));
        fp.add_pad(Pad::smd("2", 1.0, 0.0, 0.6, 0.5));
        for i in 0..2 {
            fp.add_component_body(ComponentBody {
                model_id: format!("{{GUID-{i}}}"),
                model_name: format!("M{i}.step"),
                embedded: true,
                rotation_x: 0.0,
                rotation_y: 0.0,
                rotation_z: 0.0,
                z_offset: 0.0,
                overall_height: 1.0,
                standoff_height: 0.0,
                layer: Layer::Top3DBody,
                outline: Vec::new(), // exercise the synthesised-bbox fallback
                unique_id: None,
                model_checksum: 0,
                name: " ".to_string(),
                kind: 0,
                sub_poly_index: -1,
                union_index: 0,
                is_shape_based: false,
                body_projection: 0,
                body_color_3d: 8_421_504,
                body_opacity_3d: 1.0,
                model_2d_rotation: 0.0,
            });
        }

        let data = writer::encode_data_stream(&fp).expect("encoding should succeed");
        let mut decoded = Footprint::new("TWO_BODIES");
        reader::parse_data_stream(&mut decoded, &data, None);

        // Both bodies survive (no desync from stray blocks), and each gets a
        // non-degenerate synthesised outline (the pad bounding box).
        assert_eq!(decoded.component_bodies.len(), 2);
        for body in &decoded.component_bodies {
            assert_eq!(body.outline.len(), 4, "body must have a non-empty outline");
        }

        // Byte-level: a body must be EXACTLY one size-prefixed block (as Altium
        // writes). This catches the stray empty blocks regardless of whether our
        // own reader tolerates them. Build a one-body footprint and walk it.
        let mut single = Footprint::new("ONE_BODY");
        single.add_component_body(ComponentBody::new("{G}", "M.step"));
        let d = writer::encode_data_stream(&single).expect("encoding should succeed");
        let name_len = u32::from_le_bytes(d[0..4].try_into().unwrap()) as usize;
        let mut off = 4 + name_len;
        assert_eq!(d[off], 0x0C, "expected ComponentBody object id");
        off += 1;
        let block_len = u32::from_le_bytes(d[off..off + 4].try_into().unwrap()) as usize;
        off += 4 + block_len;
        assert_eq!(
            off,
            d.len(),
            "ComponentBody must be a single block with no trailing empty blocks"
        );

        // The body param block carries the full key set Altium emits.
        let s = String::from_utf8_lossy(&d);
        assert!(s.contains("IDENTIFIER="), "missing IDENTIFIER key");
        assert!(s.contains("TEXTURE="), "missing TEXTURE key");
        assert_eq!(
            s.matches("ARCRESOLUTION=").count(),
            2,
            "Altium emits ARCRESOLUTION twice"
        );
    }

    #[test]
    fn models_data_record_has_no_leading_pipe() {
        // AltiumSharp and every BODY_3D golden start the record at EMBED= with no
        // leading pipe; the u32 length prefix is followed directly by 'E'.
        let models = vec![EmbeddedModel::new("{GUID}", "part.step", Vec::new())];
        let stream = writer::encode_model_data_stream(&models);
        // [u32 len][record + NUL]; first record byte (offset 4) must be 'E', not '|'.
        assert_eq!(stream[4], b'E', "Models/Data record must start at EMBED=");
        assert_ne!(
            stream[4], b'|',
            "Models/Data record must not have a leading pipe"
        );
    }

    #[test]
    fn via_is_single_321_byte_block() {
        // #113: Altium writes a via as ONE block — the 13-byte common header plus
        // the 321-byte via SubRecord-1 — matching `PcbLibWriter.WriteVia`. We used
        // to emit six pad-style blocks, which Altium misreads. A self-consistent
        // round-trip can't catch that, so assert the on-disk block structure.
        let mut fp = Footprint::new("VIA_ONLY");
        fp.add_via(Via::new(1.0, 2.0, 0.6, 0.3));
        let data = writer::encode_data_stream(&fp).expect("encode");

        // The via record is `[0x03][block_len: u32 LE][block]`; 321 == 0x0000_0141.
        let sig = [0x03u8, 0x41, 0x01, 0x00, 0x00];
        let pos = data
            .windows(sig.len())
            .position(|w| w == sig)
            .expect("via must be a single 321-byte block");
        let block = &data[pos + 5..pos + 5 + 321];
        // Common-header layer byte is MultiLayer (74) for a via.
        assert_eq!(block[0], 74, "via common header should be on MultiLayer");
        // Exactly one block — no second via sub-block follows (the old 6-block bug).
        assert!(
            !data[pos + 5 + 321..].windows(sig.len()).any(|w| w == sig),
            "via should emit exactly one block, not several"
        );
    }

    #[test]
    fn track_arc_extended_tail_round_trips() {
        // #113: a track/arc's solder-mask expansion and keepout restrictions are
        // preserved on read->write (previously silently dropped). Additive: a
        // default primitive (None) must round-trip back to None.
        let mut fp = Footprint::new("FIDELITY");
        let mut track = Track::new(0.0, 0.0, 1.0, 0.0, 0.2, Layer::TopLayer);
        track.solder_mask_expansion = Some(0.1);
        track.keepout_restrictions = Some(0x05);
        fp.add_track(track);
        let mut arc = Arc::circle(2.0, 0.0, 0.5, 0.15, Layer::TopLayer);
        arc.solder_mask_expansion = Some(0.08);
        arc.keepout_restrictions = Some(0x03);
        fp.add_arc(arc);
        // A default track to prove additivity (None stays None).
        fp.add_track(Track::new(5.0, 0.0, 6.0, 0.0, 0.2, Layer::TopOverlay));

        let data = writer::encode_data_stream(&fp).expect("encode");
        let mut decoded = Footprint::new("FIDELITY");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.tracks.len(), 2);
        assert!(approx_eq(
            decoded.tracks[0].solder_mask_expansion.unwrap(),
            0.1,
            0.001
        ));
        assert_eq!(decoded.tracks[0].keepout_restrictions, Some(0x05));
        assert!(approx_eq(
            decoded.arcs[0].solder_mask_expansion.unwrap(),
            0.08,
            0.001
        ));
        assert_eq!(decoded.arcs[0].keepout_restrictions, Some(0x03));
        // Additive: the default track did not gain these fields.
        assert_eq!(decoded.tracks[1].solder_mask_expansion, None);
        assert_eq!(decoded.tracks[1].keepout_restrictions, None);
    }

    #[test]
    fn binary_roundtrip_via() {
        let mut original = Footprint::new("ROUNDTRIP_VIA");

        // Add a simple through via (top to bottom)
        original.add_via(Via::new(0.0, 0.0, 0.6, 0.3));

        // Add a via at different position
        original.add_via(Via::new(2.54, 1.27, 0.8, 0.4));

        // Add a blind via (top to mid layer) - though layers may map differently
        original.add_via(Via::blind(
            -1.0,
            -1.0,
            0.5,
            0.25,
            Layer::TopLayer,
            Layer::BottomLayer,
        ));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_VIA");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.vias.len(), 3);

        // First via
        assert!(approx_eq(decoded.vias[0].x, 0.0, 0.001));
        assert!(approx_eq(decoded.vias[0].y, 0.0, 0.001));
        assert!(approx_eq(decoded.vias[0].diameter, 0.6, 0.001));
        assert!(approx_eq(decoded.vias[0].hole_size, 0.3, 0.001));

        // Second via
        assert!(approx_eq(decoded.vias[1].x, 2.54, 0.001));
        assert!(approx_eq(decoded.vias[1].y, 1.27, 0.001));
        assert!(approx_eq(decoded.vias[1].diameter, 0.8, 0.001));
        assert!(approx_eq(decoded.vias[1].hole_size, 0.4, 0.001));

        // Third via (blind via)
        assert!(approx_eq(decoded.vias[2].x, -1.0, 0.001));
        assert!(approx_eq(decoded.vias[2].y, -1.0, 0.001));
        assert!(approx_eq(decoded.vias[2].diameter, 0.5, 0.001));
        assert!(approx_eq(decoded.vias[2].hole_size, 0.25, 0.001));
        assert_eq!(decoded.vias[2].from_layer, Layer::TopLayer);
        assert_eq!(decoded.vias[2].to_layer, Layer::BottomLayer);
    }

    #[test]
    fn via_solder_mask_mode_round_trips() {
        use super::primitives::MaskExpansionMode;

        // A fresh via defaults to FromRule (Altium's default, byte 66 = 1) — not the
        // old `manual=false` which wrote 0 (None). A Manual via must round-trip.
        let mut original = Footprint::new("VIA_MASK_MODE");
        assert_eq!(
            Via::new(0.0, 0.0, 0.6, 0.3).solder_mask_expansion_mode,
            MaskExpansionMode::FromRule
        );
        original.add_via(Via::new(0.0, 0.0, 0.6, 0.3));
        let mut manual = Via::new(1.0, 1.0, 0.6, 0.3);
        manual.solder_mask_expansion_mode = MaskExpansionMode::Manual;
        original.add_via(manual);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("VIA_MASK_MODE");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.vias.len(), 2);
        assert_eq!(
            decoded.vias[0].solder_mask_expansion_mode,
            MaskExpansionMode::FromRule
        );
        assert_eq!(
            decoded.vias[1].solder_mask_expansion_mode,
            MaskExpansionMode::Manual
        );
    }

    #[test]
    fn via_thermal_power_plane_fields_round_trip() {
        use super::primitives::PowerPlaneConnectStyle;

        // PR-7: the via flag word (tenting/keepout/locked), power-plane connection,
        // paste-mask expansion and net index all survive encode -> decode.
        let mut original = Footprint::new("VIA_PP");
        let mut via = Via::new(1.0, 2.0, 0.8, 0.4);
        via.flags =
            PcbFlags::TENTING_TOP | PcbFlags::TENTING_BOTTOM | PcbFlags::KEEPOUT | PcbFlags::LOCKED;
        via.power_plane_connect_style = PowerPlaneConnectStyle::Direct;
        via.power_plane_relief_expansion = 0.6;
        via.power_plane_clearance = 0.7;
        via.paste_mask_expansion = 0.05;
        via.net_index = 42;
        original.add_via(via);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("VIA_PP");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.vias.len(), 1);
        let d = &decoded.vias[0];
        assert!(d.flags.contains(PcbFlags::TENTING_TOP));
        assert!(d.flags.contains(PcbFlags::TENTING_BOTTOM));
        assert!(d.flags.contains(PcbFlags::KEEPOUT));
        assert!(d.flags.contains(PcbFlags::LOCKED));
        assert_eq!(d.power_plane_connect_style, PowerPlaneConnectStyle::Direct);
        assert!(approx_eq(d.power_plane_relief_expansion, 0.6, 0.001));
        assert!(approx_eq(d.power_plane_clearance, 0.7, 0.001));
        assert!(approx_eq(d.paste_mask_expansion, 0.05, 0.001));
        assert_eq!(d.net_index, 42);
    }

    #[test]
    fn default_via_defaults_match_template() {
        use super::primitives::PowerPlaneConnectStyle;

        // A from-scratch via must default to exactly the VIA_SR1_TEMPLATE constants
        // so it serialises byte-identically (the readability oracle exercises vias).
        let via = Via::new(0.0, 0.0, 0.6, 0.3);
        assert_eq!(via.flags, PcbFlags::empty()); // flag word 0x000C (saved|unlocked)
        assert_eq!(via.net_index, 0xFFFF); // @3-4 = 0xFFFF (no net)
        assert_eq!(
            via.power_plane_connect_style,
            PowerPlaneConnectStyle::Relief // @31 = 0
        );
        assert!(approx_eq(via.power_plane_relief_expansion, 0.508, 1e-9)); // @42 = 200000
        assert!(approx_eq(via.power_plane_clearance, 0.508, 1e-9)); // @46 = 200000
        assert!(approx_eq(via.paste_mask_expansion, 0.0, 1e-9)); // @50 = 0

        // Encode and confirm the SubRecord-1 bytes equal the template at each offset.
        let mut fp = Footprint::new("VIA_TEMPLATE");
        fp.add_via(via);
        let data = writer::encode_data_stream(&fp).expect("encode");
        let sig = [0x03u8, 0x41, 0x01, 0x00, 0x00];
        let pos = data
            .windows(sig.len())
            .position(|w| w == sig)
            .expect("via block");
        let block = &data[pos + 5..pos + 5 + 321];
        assert_eq!(&block[1..3], &[0x0C, 0x00]); // flags word
        assert_eq!(&block[3..5], &[0xFF, 0xFF]); // net index
        assert_eq!(block[31], 0x00); // power-plane connect style
        assert_eq!(&block[42..46], &200_000i32.to_le_bytes()); // relief expansion
        assert_eq!(&block[46..50], &200_000i32.to_le_bytes()); // plane clearance
        assert_eq!(&block[50..54], &0i32.to_le_bytes()); // paste-mask expansion
                                                         // PR-8: default drill tolerances stay the 0x7FFFFFFF "unset" sentinel @291/@295.
        assert_eq!(&block[291..295], &i32::MAX.to_le_bytes());
        assert_eq!(&block[295..299], &i32::MAX.to_le_bytes());
    }

    #[test]
    fn binary_roundtrip_via_tolerances() {
        // PR-8: a via with non-default drill tolerances must survive encode -> decode.
        // Vias carry no slot geometry.
        let mut original = Footprint::new("VIA_TOL");
        let mut via = Via::new(1.0, 2.0, 0.8, 0.4);
        via.hole_positive_tolerance = Some(0.05);
        via.hole_negative_tolerance = Some(0.02);
        original.add_via(via);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("VIA_TOL");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.vias.len(), 1);
        let d = &decoded.vias[0];
        assert!(approx_eq(d.hole_positive_tolerance.unwrap(), 0.05, 0.001));
        assert!(approx_eq(d.hole_negative_tolerance.unwrap(), 0.02, 0.001));
    }

    #[test]
    fn via_default_tolerances_unset() {
        // A from-scratch via leaves both drill tolerances unset (None -> sentinel), so
        // it serialises byte-identically to the template.
        let via = Via::new(0.0, 0.0, 0.6, 0.3);
        assert_eq!(via.hole_positive_tolerance, None);
        assert_eq!(via.hole_negative_tolerance, None);
    }

    #[test]
    fn pad_mask_expansion_mode_round_trips() {
        use super::primitives::MaskExpansionMode;

        // A fresh pad defaults to FromRule (Altium's default, bytes 101/102 = 1) — not
        // the old `manual=false` which wrote 0 (None). A Manual pad must round-trip.
        let fresh = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        assert_eq!(fresh.paste_mask_expansion_mode, MaskExpansionMode::FromRule);
        assert_eq!(
            fresh.solder_mask_expansion_mode,
            MaskExpansionMode::FromRule
        );

        let mut original = Footprint::new("PAD_MASK_MODE");
        original.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        let mut manual = Pad::smd("2", 1.0, 1.0, 1.0, 1.0);
        manual.paste_mask_expansion_mode = MaskExpansionMode::Manual;
        manual.solder_mask_expansion_mode = MaskExpansionMode::Manual;
        original.add_pad(manual);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("PAD_MASK_MODE");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 2);
        assert_eq!(
            decoded.pads[0].paste_mask_expansion_mode,
            MaskExpansionMode::FromRule
        );
        assert_eq!(
            decoded.pads[0].solder_mask_expansion_mode,
            MaskExpansionMode::FromRule
        );
        assert_eq!(
            decoded.pads[1].paste_mask_expansion_mode,
            MaskExpansionMode::Manual
        );
        assert_eq!(
            decoded.pads[1].solder_mask_expansion_mode,
            MaskExpansionMode::Manual
        );
    }

    #[test]
    fn via_solder_mask_back_round_trips() {
        // A default via leaves the back mask `None` (back@242 == front@54), and an
        // asymmetric via must round-trip a distinct back-face expansion. Tests the
        // deterministic encode_via -> parse_via path (a full library write embeds
        // fresh UUIDs/timestamps, so it is not byte-deterministic).
        let mut original = Footprint::new("VIA_SMASK");

        // Default via: back is None and must survive the round-trip as None.
        original.add_via(Via::new(0.0, 0.0, 0.6, 0.3));

        // Asymmetric via: distinct front/back mask expansion.
        let mut asym = Via::new(2.54, 0.0, 0.6, 0.3);
        asym.solder_mask_expansion = 0.1; // front 0.1 mm
        asym.solder_mask_expansion_back = Some(0.2); // back 0.2 mm
        original.add_via(asym);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("VIA_SMASK");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.vias.len(), 2);
        assert_eq!(decoded.vias[0].solder_mask_expansion_back, None);
        assert_eq!(decoded.vias[1].solder_mask_expansion_back, Some(0.2));
        // Front face unaffected.
        assert!((decoded.vias[1].solder_mask_expansion - 0.1).abs() < 1e-6);

        // Idempotent re-encode proves a byte-stable round-trip for vias.
        let data2 = writer::encode_data_stream(&decoded).expect("re-encode");
        assert_eq!(data, data2);
    }

    #[test]
    fn binary_roundtrip_mixed_with_vias() {
        let mut original = Footprint::new("ROUNDTRIP_MIXED_VIA");

        // Add various primitives including vias
        original.add_pad(Pad::smd("1", -1.0, 0.0, 0.6, 0.5));
        original.add_pad(Pad::smd("2", 1.0, 0.0, 0.6, 0.5));
        original.add_via(Via::new(0.0, 0.0, 0.5, 0.25));
        original.add_track(Track::new(-1.5, -0.3, 1.5, -0.3, 0.12, Layer::TopOverlay));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_MIXED_VIA");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 2);
        assert_eq!(decoded.vias.len(), 1);
        assert_eq!(decoded.tracks.len(), 1);

        // Verify via data
        assert!(approx_eq(decoded.vias[0].x, 0.0, 0.001));
        assert!(approx_eq(decoded.vias[0].diameter, 0.5, 0.001));
        assert!(approx_eq(decoded.vias[0].hole_size, 0.25, 0.001));
    }

    #[test]
    fn binary_roundtrip_pad_advanced_features() {
        use super::primitives::MaskExpansionMode;

        let mut original = Footprint::new("ROUNDTRIP_PAD_ADVANCED");

        // Create a pad with hole shape and mask expansion
        let mut pad_with_square_hole = Pad::through_hole("1", -2.54, 0.0, 1.8, 1.8, 1.0);
        pad_with_square_hole.hole_shape = HoleShape::Square;
        pad_with_square_hole.solder_mask_expansion = Some(0.1);
        pad_with_square_hole.solder_mask_expansion_mode = MaskExpansionMode::Manual;
        original.add_pad(pad_with_square_hole);

        // Create a pad with slot hole
        let mut pad_with_slot = Pad::through_hole("2", 0.0, 0.0, 2.0, 1.5, 0.8);
        pad_with_slot.hole_shape = HoleShape::Slot;
        pad_with_slot.paste_mask_expansion = Some(-0.05);
        pad_with_slot.paste_mask_expansion_mode = MaskExpansionMode::Manual;
        original.add_pad(pad_with_slot);

        // Create a simple pad with round hole (default)
        let pad_with_round_hole = Pad::through_hole("3", 2.54, 0.0, 1.5, 1.5, 0.8);
        original.add_pad(pad_with_round_hole);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_PAD_ADVANCED");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 3);

        // Non-round hole shapes now round-trip via the 596-byte size/shape block
        // (hole type at offset 262), alongside the main-block mask-expansion.

        // Pad 1: Square hole + solder mask expansion
        assert_eq!(decoded.pads[0].designator, "1");
        assert_eq!(decoded.pads[0].hole_shape, HoleShape::Square);
        assert!(decoded.pads[0].solder_mask_expansion.is_some());
        assert!(approx_eq(
            decoded.pads[0].solder_mask_expansion.unwrap(),
            0.1,
            0.001
        ));
        assert_eq!(
            decoded.pads[0].solder_mask_expansion_mode,
            MaskExpansionMode::Manual
        );

        // Pad 2: Slot hole + paste mask expansion
        assert_eq!(decoded.pads[1].designator, "2");
        assert_eq!(decoded.pads[1].hole_shape, HoleShape::Slot);
        assert!(decoded.pads[1].paste_mask_expansion.is_some());
        assert!(approx_eq(
            decoded.pads[1].paste_mask_expansion.unwrap(),
            -0.05,
            0.001
        ));
        assert_eq!(
            decoded.pads[1].paste_mask_expansion_mode,
            MaskExpansionMode::Manual
        );

        // Pad 3: Default round hole (empty Block 5)
        assert_eq!(decoded.pads[2].designator, "3");
        assert_eq!(decoded.pads[2].hole_shape, HoleShape::Round);
    }

    #[test]
    fn binary_roundtrip_pad_stack_modes() {
        let mut original = Footprint::new("ROUNDTRIP_STACK_MODES");

        // Pad with Simple stack mode (using Rectangle shape to avoid FullStack upgrade)
        // Note: RoundedRectangle pads automatically get FullStack to preserve corner radius
        let mut pad_simple = Pad::smd("1", -2.54, 0.0, 1.0, 0.5);
        pad_simple.shape = PadShape::Rectangle;
        assert_eq!(pad_simple.stack_mode, PadStackMode::Simple);
        original.add_pad(pad_simple);

        // Pad with TopMiddleBottom stack mode
        let mut pad_tmb = Pad::through_hole("2", 0.0, 0.0, 1.5, 1.5, 0.8);
        pad_tmb.stack_mode = PadStackMode::TopMiddleBottom;
        original.add_pad(pad_tmb);

        // Pad with FullStack stack mode
        let mut pad_full = Pad::through_hole("3", 2.54, 0.0, 1.8, 1.8, 1.0);
        pad_full.stack_mode = PadStackMode::FullStack;
        original.add_pad(pad_full);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_STACK_MODES");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 3);

        // Verify stack modes preserved
        assert_eq!(decoded.pads[0].stack_mode, PadStackMode::Simple);
        assert_eq!(decoded.pads[1].stack_mode, PadStackMode::TopMiddleBottom);
        assert_eq!(decoded.pads[2].stack_mode, PadStackMode::FullStack);
    }

    #[test]
    fn binary_roundtrip_pad_corner_radius() {
        let mut original = Footprint::new("ROUNDTRIP_CORNER_RADIUS");

        // SMD pad with explicit corner radius
        let mut pad_with_radius = Pad::smd("1", 0.0, 0.0, 2.0, 1.0);
        pad_with_radius.shape = PadShape::RoundedRectangle;
        pad_with_radius.corner_radius_percent = Some(25);
        // Setting corner radius requires FullStack mode
        pad_with_radius.stack_mode = PadStackMode::FullStack;
        original.add_pad(pad_with_radius);

        // Simple SMD pad with an EXPLICIT corner radius: now round-trips via the
        // 596-byte size/shape block (no FullStack needed).
        let mut pad_simple_radius = Pad::smd("2", 2.54, 0.0, 1.5, 0.8);
        pad_simple_radius.corner_radius_percent = Some(30);
        original.add_pad(pad_simple_radius);

        // Rectangle pad (no corner radius needed)
        let mut pad_no_radius = Pad::smd("3", 5.08, 0.0, 1.5, 0.8);
        pad_no_radius.shape = PadShape::Rectangle;
        original.add_pad(pad_no_radius);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_CORNER_RADIUS");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 3);

        // Verify explicit corner radius preserved
        assert_eq!(decoded.pads[0].corner_radius_percent, Some(25));
        assert_eq!(decoded.pads[0].stack_mode, PadStackMode::FullStack);

        // Simple pad's explicit corner radius round-trips without FullStack.
        assert_eq!(decoded.pads[1].corner_radius_percent, Some(30));
        assert_eq!(decoded.pads[1].stack_mode, PadStackMode::Simple);
        assert_eq!(decoded.pads[1].shape, PadShape::RoundedRectangle);

        // Rectangle pad has no corner radius
        assert_eq!(decoded.pads[2].corner_radius_percent, None);
        assert_eq!(decoded.pads[2].stack_mode, PadStackMode::Simple);
    }

    #[test]
    fn binary_roundtrip_per_layer_pad_data() {
        let mut original = Footprint::new("ROUNDTRIP_PER_LAYER");

        // Create a pad with per-layer data
        let mut pad = Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8);
        pad.stack_mode = PadStackMode::FullStack;

        // Set up per-layer sizes (32 layers)
        let mut sizes = vec![(1.6, 1.6); 32];
        sizes[0] = (1.8, 1.8); // Top layer larger
        sizes[1] = (1.4, 1.4); // Bottom layer smaller
        pad.per_layer_sizes = Some(sizes);

        // Set up per-layer shapes
        let mut shapes = vec![PadShape::Round; 32];
        shapes[0] = PadShape::RoundedRectangle; // Top layer rounded rect
        pad.per_layer_shapes = Some(shapes);

        // Set up per-layer corner radii
        let mut radii = vec![0_u8; 32];
        radii[0] = 50; // Top layer 50% corner radius
        pad.per_layer_corner_radii = Some(radii);

        // Set up per-layer offsets
        let mut offsets = vec![(0.0, 0.0); 32];
        offsets[0] = (0.1, 0.05); // Top layer offset from hole centre
        pad.per_layer_offsets = Some(offsets);

        original.add_pad(pad);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_PER_LAYER");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 1);
        let decoded_pad = &decoded.pads[0];

        // Verify stack mode
        assert_eq!(decoded_pad.stack_mode, PadStackMode::FullStack);

        // Verify per-layer sizes
        assert!(decoded_pad.per_layer_sizes.is_some());
        let decoded_sizes = decoded_pad.per_layer_sizes.as_ref().unwrap();
        assert_eq!(decoded_sizes.len(), 32);
        assert!(approx_eq(decoded_sizes[0].0, 1.8, 0.001)); // Top X
        assert!(approx_eq(decoded_sizes[0].1, 1.8, 0.001)); // Top Y
        assert!(approx_eq(decoded_sizes[1].0, 1.4, 0.001)); // Bottom X
        assert!(approx_eq(decoded_sizes[1].1, 1.4, 0.001)); // Bottom Y

        // Verify per-layer shapes
        assert!(decoded_pad.per_layer_shapes.is_some());
        let decoded_shapes = decoded_pad.per_layer_shapes.as_ref().unwrap();
        assert_eq!(decoded_shapes.len(), 32);
        assert_eq!(decoded_shapes[0], PadShape::RoundedRectangle);
        assert_eq!(decoded_shapes[1], PadShape::Round);

        // Verify per-layer corner radii
        assert!(decoded_pad.per_layer_corner_radii.is_some());
        let decoded_radii = decoded_pad.per_layer_corner_radii.as_ref().unwrap();
        assert_eq!(decoded_radii.len(), 32);
        assert_eq!(decoded_radii[0], 50);
        assert_eq!(decoded_radii[1], 0);

        // Verify per-layer offsets
        assert!(decoded_pad.per_layer_offsets.is_some());
        let decoded_offsets = decoded_pad.per_layer_offsets.as_ref().unwrap();
        assert_eq!(decoded_offsets.len(), 32);
        assert!(approx_eq(decoded_offsets[0].0, 0.1, 0.001));
        assert!(approx_eq(decoded_offsets[0].1, 0.05, 0.001));
        assert!(approx_eq(decoded_offsets[1].0, 0.0, 0.001));
        assert!(approx_eq(decoded_offsets[1].1, 0.0, 0.001));
    }

    // =========================================================================
    // UniqueID Roundtrip Tests
    // =========================================================================

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn unique_id_parse_stream() {
        // Test parsing the UniqueIDPrimitiveInformation stream format
        let mut test_data = Vec::new();

        // Record 1: |PRIMITIVEINDEX=1|PRIMITIVEOBJECTID=Pad|UNIQUEID=QHHMRSCB
        let record1 = b"|PRIMITIVEINDEX=1|PRIMITIVEOBJECTID=Pad|UNIQUEID=QHHMRSCB";
        test_data.extend_from_slice(&(record1.len() as u32).to_le_bytes());
        test_data.extend_from_slice(record1);

        // Record 2: |PRIMITIVEINDEX=2|PRIMITIVEOBJECTID=Pad|UNIQUEID=ABCD1234
        let record2 = b"|PRIMITIVEINDEX=2|PRIMITIVEOBJECTID=Pad|UNIQUEID=ABCD1234";
        test_data.extend_from_slice(&(record2.len() as u32).to_le_bytes());
        test_data.extend_from_slice(record2);

        // Record 3: |PRIMITIVEINDEX=1|PRIMITIVEOBJECTID=Track|UNIQUEID=WXYZ9876
        let record3 = b"|PRIMITIVEINDEX=1|PRIMITIVEOBJECTID=Track|UNIQUEID=WXYZ9876";
        test_data.extend_from_slice(&(record3.len() as u32).to_le_bytes());
        test_data.extend_from_slice(record3);

        let entries = reader::parse_unique_id_stream(&test_data);

        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].primitive_index, 1);
        assert_eq!(entries[0].primitive_type, "Pad");
        assert_eq!(entries[0].unique_id, "QHHMRSCB");

        assert_eq!(entries[1].primitive_index, 2);
        assert_eq!(entries[1].primitive_type, "Pad");
        assert_eq!(entries[1].unique_id, "ABCD1234");

        assert_eq!(entries[2].primitive_index, 1);
        assert_eq!(entries[2].primitive_type, "Track");
        assert_eq!(entries[2].unique_id, "WXYZ9876");
    }

    #[test]
    fn unique_id_encode_stream() {
        // Create a footprint with unique IDs
        let mut footprint = Footprint::new("TEST_UNIQUE_ID");

        let mut pad1 = Pad::smd("1", -0.5, 0.0, 0.6, 0.5);
        pad1.unique_id = Some("UID00001".to_string());
        footprint.add_pad(pad1);

        let mut pad2 = Pad::smd("2", 0.5, 0.0, 0.6, 0.5);
        pad2.unique_id = Some("UID00002".to_string());
        footprint.add_pad(pad2);

        let mut track = Track::new(-1.0, 0.0, 1.0, 0.0, 0.15, Layer::TopOverlay);
        track.unique_id = Some("TRACK001".to_string());
        footprint.add_track(track);

        // Encode the unique ID stream
        let uid_data = writer::encode_unique_id_stream(&footprint);
        assert!(uid_data.is_some());

        // Parse it back
        let entries = reader::parse_unique_id_stream(&uid_data.unwrap());

        assert_eq!(entries.len(), 3);

        // Find Pad entries
        let pad_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.primitive_type == "Pad")
            .collect();
        assert_eq!(pad_entries.len(), 2);
        assert_eq!(pad_entries[0].unique_id, "UID00001");
        assert_eq!(pad_entries[1].unique_id, "UID00002");

        // Find Track entry
        let track_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.primitive_type == "Track")
            .collect();
        assert_eq!(track_entries.len(), 1);
        assert_eq!(track_entries[0].unique_id, "TRACK001");
    }

    #[test]
    fn unique_id_apply_to_footprint() {
        // Create a footprint without unique IDs
        let mut footprint = Footprint::new("TEST_APPLY");
        footprint.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        footprint.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
        footprint.add_track(Track::new(-1.0, 0.0, 1.0, 0.0, 0.15, Layer::TopOverlay));

        // Global Data-stream ordinals: no arcs, so the two pads are 0 and 1, and the
        // track (after the pads/vias slot) is 2.
        let entries = vec![
            reader::UniqueIdEntry {
                primitive_index: 0,
                primitive_type: "Pad".to_string(),
                unique_id: "PADUID01".to_string(),
            },
            reader::UniqueIdEntry {
                primitive_index: 1,
                primitive_type: "Pad".to_string(),
                unique_id: "PADUID02".to_string(),
            },
            reader::UniqueIdEntry {
                primitive_index: 2,
                primitive_type: "Track".to_string(),
                unique_id: "TRKUID01".to_string(),
            },
        ];

        // Apply unique IDs
        reader::apply_unique_ids(&mut footprint, &entries);

        // Verify
        assert_eq!(footprint.pads[0].unique_id, Some("PADUID01".to_string()));
        assert_eq!(footprint.pads[1].unique_id, Some("PADUID02".to_string()));
        assert_eq!(footprint.tracks[0].unique_id, Some("TRKUID01".to_string()));
    }

    #[test]
    fn unique_id_global_ordinal_round_trip() {
        // A real footprint often has a silkscreen arc before the pads, so the first
        // pad is PRIMITIVEINDEX=1 (a single global, 0-based, Data-stream ordinal),
        // never 0 — the arc occupies ordinal 0. This locks the writer/reader contract.
        let mut fp = Footprint::new("RT_UID");
        let mut arc = Arc::circle(0.0, 0.0, 0.5, 0.1, Layer::TopOverlay);
        arc.unique_id = Some("ARCUID01".to_string());
        fp.add_arc(arc);
        let mut p1 = Pad::smd("1", -0.5, 0.0, 0.6, 0.5);
        p1.unique_id = Some("PADUID01".to_string());
        fp.add_pad(p1);
        let mut p2 = Pad::smd("2", 0.5, 0.0, 0.6, 0.5);
        p2.unique_id = Some("PADUID02".to_string());
        fp.add_pad(p2);

        let entries =
            reader::parse_unique_id_stream(&writer::encode_unique_id_stream(&fp).unwrap());

        // Arc=0, then the two pads at 1 and 2 (never 0).
        let arc_e = entries.iter().find(|e| e.primitive_type == "Arc").unwrap();
        assert_eq!(arc_e.primitive_index, 0);
        let mut pad_idx: Vec<usize> = entries
            .iter()
            .filter(|e| e.primitive_type == "Pad")
            .map(|e| e.primitive_index)
            .collect();
        pad_idx.sort_unstable();
        assert_eq!(pad_idx, vec![1, 2]);

        // Round-trip onto a fresh, id-less footprint of identical shape.
        let mut fresh = Footprint::new("RT_UID");
        fresh.add_arc(Arc::circle(0.0, 0.0, 0.5, 0.1, Layer::TopOverlay));
        fresh.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        fresh.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
        reader::apply_unique_ids(&mut fresh, &entries);
        assert_eq!(fresh.arcs[0].unique_id.as_deref(), Some("ARCUID01"));
        assert_eq!(fresh.pads[0].unique_id.as_deref(), Some("PADUID01"));
        assert_eq!(fresh.pads[1].unique_id.as_deref(), Some("PADUID02"));
    }

    #[test]
    fn unique_id_no_primitives_with_ids() {
        // Create a footprint without unique IDs
        let mut footprint = Footprint::new("TEST_NO_IDS");
        footprint.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        footprint.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));

        // Encode should return None since no primitives have unique IDs
        let uid_data = writer::encode_unique_id_stream(&footprint);
        assert!(uid_data.is_none());
    }

    #[test]
    fn unique_id_partial_primitives() {
        // Create a footprint where only some primitives have unique IDs
        let mut footprint = Footprint::new("TEST_PARTIAL");

        let mut pad1 = Pad::smd("1", -0.5, 0.0, 0.6, 0.5);
        pad1.unique_id = Some("ONLYTHIS".to_string());
        footprint.add_pad(pad1);

        // Pad 2 has no unique ID
        footprint.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));

        // Encode
        let uid_data = writer::encode_unique_id_stream(&footprint);
        assert!(uid_data.is_some());

        // Parse back
        let entries = reader::parse_unique_id_stream(&uid_data.unwrap());

        // Should only have 1 entry (the pad with the unique ID)
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].primitive_index, 0); // 0-based index
        assert_eq!(entries[0].unique_id, "ONLYTHIS");
    }

    #[test]
    fn wrong_file_type_schlib_as_pcblib() {
        use std::io::Cursor;

        // Create a SchLib file in memory
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut cfb = cfb::CompoundFile::create(&mut buffer).expect("create cfb");

            // Write a SchLib FileHeader (ASCII, just pipe-delimited - PcbLib expects ASCII format)
            let header = "|HEADER=Protel for Windows - Schematic Library Editor Binary File Version 5.0|COMPCOUNT=0|";
            let mut stream = cfb.create_stream("/FileHeader").expect("create stream");
            std::io::Write::write_all(&mut stream, header.as_bytes()).expect("write header");
        }

        // Try to read it as PcbLib - should fail with WrongFileType
        buffer.set_position(0);
        let result = PcbLib::read(buffer);

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("Wrong file type"),
            "Expected 'Wrong file type' error, got: {err_str}"
        );
        assert!(
            err_str.contains("expected PcbLib"),
            "Expected 'expected PcbLib' in error, got: {err_str}"
        );
    }
}
