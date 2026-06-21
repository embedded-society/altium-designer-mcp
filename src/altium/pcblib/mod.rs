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
mod reader;
mod units;
mod writer;

use serde::{Deserialize, Serialize};

pub use primitives::{
    Arc, ComponentBody, EmbeddedModel, Fill, HoleShape, Layer, Model3D, Pad, PadShape,
    PadStackMode, PcbFlags, Region, StrokeFont, Text, TextJustification, TextKind, Track, Vertex,
    Via,
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

    /// Reads a `PcbLib` from any reader implementing `Read + Seek`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be parsed.
    pub fn read(reader: impl std::io::Read + std::io::Seek) -> AltiumResult<Self> {
        let mut cfb = crate::altium::open_ole(reader)?;

        let mut library = Self::new();

        // Read FileHeader for library metadata (validates file type)
        library.metadata = Self::read_file_header(&mut cfb)?;

        // Read Library/Data for component ordering (preferred over FileHeader)
        Self::read_library_data(&mut cfb, &mut library.metadata);

        // Read Storage stream for UniqueIdPrimitiveInformation (if present)
        // Note: This is currently a stub - the format is not fully documented
        Self::read_storage_stream(&mut cfb);

        // Read WideStrings stream if present (contains text content for Text primitives)
        let wide_strings = Self::read_wide_strings(&mut cfb);

        // Read embedded 3D models if present
        library.models = Self::read_models(&mut cfb);

        // List all entries to find footprint storages
        let entries: Vec<_> = cfb.walk().map(|e| e.path().to_path_buf()).collect();

        // Collect footprints with their OLE storage names for later reordering
        let mut footprints_by_ole_name: std::collections::HashMap<String, Footprint> =
            std::collections::HashMap::new();

        for entry_path in entries {
            // Skip non-storage entries and root
            let path_str = entry_path.to_string_lossy();
            if path_str == "/" || path_str.is_empty() {
                continue;
            }

            // Check if this is a component storage (has a Data stream)
            let data_path = entry_path.join("Data");
            if cfb.is_stream(&data_path) {
                let component_name = entry_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                // Filter out internal OLE storage entries (not actual footprints)
                let is_internal = INTERNAL_OLE_ENTRIES
                    .iter()
                    .any(|&entry| component_name == entry);

                if !component_name.is_empty() && !is_internal {
                    // Read the component data
                    match Self::read_footprint(
                        &mut cfb,
                        &entry_path,
                        &component_name,
                        &wide_strings,
                    ) {
                        Ok(footprint) => {
                            footprints_by_ole_name.insert(component_name.clone(), footprint);
                        }
                        Err(e) => {
                            tracing::warn!(
                                component = %component_name,
                                error = %e,
                                "Failed to read footprint, skipping"
                            );
                        }
                    }
                }
            }
        }

        // Reorder footprints according to FileHeader order (LIBREF{N} entries)
        // This ensures list_components returns components in the correct order
        // after reorder_components has been used.
        for ole_name in &library.metadata.component_names {
            if let Some(footprint) = footprints_by_ole_name.remove(ole_name) {
                library.footprints.push(footprint);
            }
        }

        // Append any orphaned footprints (found in OLE but not in FileHeader)
        // This handles edge cases like corrupted FileHeader or manually edited files
        for (ole_name, footprint) in footprints_by_ole_name {
            tracing::warn!(
                ole_name = %ole_name,
                footprint = %footprint.name,
                "Footprint not found in FileHeader, appending at end"
            );
            library.footprints.push(footprint);
        }

        // Populate model_3d from component_bodies for backward compatibility
        library.populate_model_3d_from_component_bodies();

        tracing::info!(count = library.footprints.len(), "Read PcbLib");

        Ok(library)
    }

    /// Populates `model_3d` field from `component_bodies` for backward compatibility.
    ///
    /// When reading a library, the 3D model data is stored in `component_bodies` as
    /// `ComponentBody` primitives. This method extracts the first `ComponentBody`
    /// and creates a `Model3D` reference for it, enabling backward compatibility
    /// with code that uses the simpler `model_3d` field.
    fn populate_model_3d_from_component_bodies(&mut self) {
        for footprint in &mut self.footprints {
            // Only populate if model_3d is None and there are component_bodies
            if footprint.model_3d.is_none() && !footprint.component_bodies.is_empty() {
                let body = &footprint.component_bodies[0];

                // Try to find the corresponding EmbeddedModel to get the actual filepath
                // If not found, use the model_name as the filepath
                // Note: GUID matching is case-insensitive due to inconsistent casing in Altium files
                let filepath = self
                    .models
                    .iter()
                    .find(|m| m.id.eq_ignore_ascii_case(&body.model_id))
                    .map_or_else(|| body.model_name.clone(), |m| m.name.clone());

                footprint.model_3d = Some(Model3D {
                    filepath,
                    x_offset: 0.0, // ComponentBody doesn't store X/Y offsets
                    y_offset: 0.0,
                    z_offset: body.z_offset,
                    rotation: body.rotation_z,
                });

                tracing::trace!(
                    footprint = %footprint.name,
                    model_id = %body.model_id,
                    "Populated model_3d from ComponentBody"
                );
            }
        }
    }

    /// Reads the `FileHeader` stream and parses library metadata.
    ///
    /// The `FileHeader` can be in two formats:
    ///
    /// 1. **Binary version string** (Altium/AltiumSharp format):
    ///    `[string_len:4 LE][string_len:1]["PCB 6.0 Binary Library File"]`
    ///
    /// 2. **Pipe-delimited key=value** (legacy format):
    ///    `|HEADER=Protel for Windows - PCB Library|COMPCOUNT=...|LIBREF0=...|`
    ///
    /// Component metadata is obtained from `/Library/Data` when available.
    ///
    /// # Errors
    ///
    /// Returns an error if the file is not a valid `PcbLib` (wrong file type).
    fn read_file_header<F: std::io::Read + std::io::Seek>(
        cfb: &mut cfb::CompoundFile<F>,
    ) -> AltiumResult<LibraryMetadata> {
        let mut metadata = LibraryMetadata::default();

        let Some(data) = crate::altium::read_stream_opt(cfb, "/FileHeader") else {
            return Ok(metadata);
        };

        // Try binary version string format first:
        // [string_len:4 LE u32][string_len:1 u8][string_data]
        if data.len() >= 5 {
            let block_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
            let str_len = data[4] as usize;

            if block_len == str_len && data.len() >= 5 + str_len {
                if let Ok(version) = std::str::from_utf8(&data[5..5 + str_len]) {
                    if version.contains("PCB") && version.contains("Binary Library File") {
                        metadata.header = version.to_string();
                        tracing::debug!(
                            header = %metadata.header,
                            "Parsed FileHeader (binary version string)"
                        );
                        return Ok(metadata);
                    }
                }
            }
        }

        // Fall back to pipe-delimited key=value format (legacy)
        let Ok(text) = String::from_utf8(data) else {
            return Ok(metadata);
        };

        for pair in text.split('|') {
            if let Some((key, value)) = pair.split_once('=') {
                let key_upper = key.to_uppercase();
                match key_upper.as_str() {
                    "HEADER" => {
                        metadata.header = value.to_string();
                    }
                    "COMPCOUNT" => {
                        metadata.component_count = value.parse().unwrap_or(0);
                    }
                    _ => {
                        if let Some(idx_str) = key_upper.strip_prefix("LIBREF") {
                            if let Ok(idx) = idx_str.parse::<usize>() {
                                while metadata.component_names.len() <= idx {
                                    metadata.component_names.push(String::new());
                                }
                                metadata.component_names[idx] = value.to_string();
                            }
                        } else if let Some(idx_str) = key_upper.strip_prefix("COMPDESCR") {
                            if let Ok(idx) = idx_str.parse::<usize>() {
                                while metadata.component_descriptions.len() <= idx {
                                    metadata.component_descriptions.push(String::new());
                                }
                                metadata.component_descriptions[idx] = value.to_string();
                            }
                        }
                    }
                }
            }
        }

        // Validate file type - must be a PCB library
        if !metadata.header.is_empty()
            && !metadata.header.contains("PCB Library")
            && !metadata.header.contains("PCB")
        {
            let actual_type = if metadata.header.contains("Schematic Library") {
                "SchLib (Schematic Library)"
            } else {
                &metadata.header
            };
            return Err(AltiumError::wrong_file_type("PcbLib", actual_type));
        }

        tracing::debug!(
            header = %metadata.header,
            count = metadata.component_count,
            names = metadata.component_names.len(),
            "Parsed FileHeader (pipe-delimited)"
        );

        Ok(metadata)
    }

    /// Reads the `/Library/Data` stream for component ordering metadata.
    ///
    /// # Format
    ///
    /// ```text
    /// [block_len:4]["|KEY=VAL|..." + \x00]   // parameter block
    /// [component_count:4 LE u32]
    /// [block_len:4][str_len:1][name]          // per component (WriteStringBlock)
    /// ```
    fn read_library_data<F: std::io::Read + std::io::Seek>(
        cfb: &mut cfb::CompoundFile<F>,
        metadata: &mut LibraryMetadata,
    ) {
        let Some(data) = crate::altium::read_stream_opt(cfb, "/Library/Data") else {
            return;
        };

        if data.len() < 8 {
            return;
        }

        // Skip parameter block: [block_len:4][content]
        let block_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let mut offset = 4 + block_len;

        if offset + 4 > data.len() {
            return;
        }

        // Read component count
        let comp_count = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        metadata.component_count = comp_count;
        metadata.component_names.clear();

        // Read component names: [block_len:4][str_len:1][name]
        for _ in 0..comp_count {
            if offset + 4 > data.len() {
                break;
            }

            let name_block_len = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            offset += 4;

            if name_block_len == 0 || offset + name_block_len > data.len() {
                break;
            }

            let str_len = data[offset] as usize;
            if str_len < name_block_len && offset + 1 + str_len <= data.len() {
                if let Ok(name) = std::str::from_utf8(&data[offset + 1..offset + 1 + str_len]) {
                    metadata.component_names.push(name.to_string());
                }
            }

            offset += name_block_len;
        }

        tracing::debug!(
            count = metadata.component_count,
            names = metadata.component_names.len(),
            "Parsed Library/Data"
        );
    }

    /// Reads the `/Storage` stream for `UniqueIdPrimitiveInformation` mappings.
    ///
    /// This stream contains mappings that link primitives to unique IDs.
    /// The exact format is not fully documented, so this is currently a stub
    /// that logs what we find for future analysis.
    ///
    /// # Format (partially documented)
    ///
    /// The Storage stream appears to contain pipe-delimited key=value pairs
    /// similar to other Altium streams. Known fields:
    /// - `UNIQUEIDPRIMITIVEINFORMATION{N}`: Primitive unique ID mappings
    fn read_storage_stream<F: std::io::Read + std::io::Seek>(cfb: &mut cfb::CompoundFile<F>) {
        let Some(data) = crate::altium::read_stream_opt(cfb, "/Storage") else {
            return;
        };

        // Storage stream is typically ASCII text with pipe-delimited key=value pairs
        if let Ok(text) = String::from_utf8(data) {
            // Count UniqueIdPrimitiveInformation entries for logging
            let uid_count = text.matches("UNIQUEIDPRIMITIVEINFORMATION").count();
            if uid_count > 0 {
                tracing::debug!(
                    count = uid_count,
                    "Found UniqueIdPrimitiveInformation entries in Storage stream"
                );
            }
        }
    }

    /// Reads the `WideStrings` stream if present.
    fn read_wide_strings<F: std::io::Read + std::io::Seek>(
        cfb: &mut cfb::CompoundFile<F>,
    ) -> reader::WideStrings {
        if let Some(data) = crate::altium::read_stream_opt(cfb, "/WideStrings") {
            return reader::parse_wide_strings(&data);
        }
        reader::WideStrings::new()
    }

    /// Reads embedded 3D models from `/Library/Models/` storage.
    ///
    /// Models are stored as:
    /// - `/Library/Models/Header` - Model count and metadata
    /// - `/Library/Models/Data` - GUID-to-index mapping
    /// - `/Library/Models/{N}` - zlib-compressed STEP files
    fn read_models<F: std::io::Read + std::io::Seek>(
        cfb: &mut cfb::CompoundFile<F>,
    ) -> Vec<EmbeddedModel> {
        // Check if Models storage exists
        let models_storage = std::path::Path::new("/Library/Models");
        if !cfb.is_storage(models_storage) {
            return Vec::new();
        }

        // Read Header to get model count
        let header_path = models_storage.join("Header");
        let model_count = crate::altium::read_stream_opt(cfb, &header_path)
            .map_or(0, |data| reader::parse_model_header_stream(&data));

        // Read Data stream to get GUID-to-index mapping
        let data_path = models_storage.join("Data");
        let model_index = crate::altium::read_stream_opt(cfb, &data_path)
            .map(|data| reader::parse_model_data_stream(&data))
            .unwrap_or_default();

        if model_index.is_empty() {
            tracing::debug!("No model index found in /Library/Models/Data");
            return Vec::new();
        }

        // Read compressed model streams
        let mut model_data: Vec<(usize, Vec<u8>)> = Vec::new();

        // Determine max index: use header count if available, otherwise use model_index size
        let max_index = if model_count > 0 {
            model_count
        } else {
            // Fall back to max index from model_index + 1
            model_index
                .values()
                .map(|(idx, _)| *idx + 1)
                .max()
                .unwrap_or(0)
        };

        // Model streams are numbered 0, 1, 2, ...
        for idx in 0..max_index {
            let stream_path = models_storage.join(idx.to_string());
            if let Some(data) = crate::altium::read_stream_opt(cfb, &stream_path) {
                tracing::trace!(
                    index = idx,
                    size = data.len(),
                    "Read compressed model stream"
                );
                model_data.push((idx, data));
            }
            // Don't break early - indices might not be sequential
        }

        let models = reader::parse_embedded_models(&model_index, &model_data);
        tracing::debug!(count = models.len(), "Parsed embedded 3D models");
        models
    }

    /// Reads a single footprint from the OLE document.
    fn read_footprint<F: std::io::Read + std::io::Seek>(
        cfb: &mut cfb::CompoundFile<F>,
        storage_path: &std::path::Path,
        name: &str,
        wide_strings: &reader::WideStrings,
    ) -> AltiumResult<Footprint> {
        let mut footprint = Footprint::new(name);

        // Read parameters if present
        let params_path = storage_path.join("Parameters");
        if let Some(params_data) = crate::altium::read_stream_opt(cfb, &params_path) {
            Self::parse_parameters(&mut footprint, &params_data);
        }

        // Read Data stream (contains primitives)
        let data_path = storage_path.join("Data");
        if cfb.is_stream(&data_path) {
            let mut stream = cfb.open_stream(&data_path).map_err(|e| {
                AltiumError::invalid_ole(format!("Failed to open Data stream: {e}"))
            })?;
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut stream, &mut data).map_err(|e| {
                AltiumError::invalid_ole(format!("Failed to read Data stream: {e}"))
            })?;

            Self::parse_primitives(&mut footprint, &data, wide_strings);
        }

        // Read UniqueIDPrimitiveInformation stream if present (contains unique IDs for primitives)
        let unique_id_path = storage_path.join("UniqueIDPrimitiveInformation/Data");
        if let Some(uid_data) = crate::altium::read_stream_opt(cfb, &unique_id_path) {
            let unique_ids = reader::parse_unique_id_stream(&uid_data);
            reader::apply_unique_ids(&mut footprint, &unique_ids);
        }

        Ok(footprint)
    }

    /// Parses parameters from the Parameters stream.
    ///
    /// The Parameters stream contains key=value pairs separated by `|`.
    /// Important fields:
    /// - `PATTERN`: The full footprint name (may be longer than 31-char OLE storage limit)
    /// - `DESCRIPTION`: Footprint description
    ///
    /// # Format
    ///
    /// The stream may have two formats:
    /// 1. With 4-byte length header: `[length:4 LE][text:length]`
    /// 2. Raw ASCII text: `|PATTERN=...|DESCRIPTION=...|`
    fn parse_parameters(footprint: &mut Footprint, data: &[u8]) {
        // Detect whether stream has a 4-byte length header or is raw text.
        // With header: first 4 bytes are u32 LE length, followed by pipe-delimited text.
        // Raw text: starts directly with '|' character.
        let text_data = if data.len() >= 4 {
            let potential_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
            // Valid header if: length is plausible AND text would start with '|'
            if potential_len > 0
                && potential_len <= data.len().saturating_sub(4)
                && data.get(4) == Some(&b'|')
            {
                &data[4..]
            } else {
                data
            }
        } else {
            data
        };

        if let Ok(text) = String::from_utf8(text_data.to_vec()) {
            for pair in text.split('|') {
                if let Some((key, value)) = pair.split_once('=') {
                    match key.to_uppercase().as_str() {
                        // Use PATTERN as the canonical name since OLE storage names
                        // are limited to 31 characters
                        "PATTERN" if !value.is_empty() => {
                            footprint.name = value.to_string();
                        }
                        "DESCRIPTION" => {
                            footprint.description = value.to_string();
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Parses primitives from the Data stream.
    ///
    /// The Data stream contains binary records for each primitive (pads, tracks, arcs, etc.).
    /// See the [`reader`] module for format details.
    fn parse_primitives(
        footprint: &mut Footprint,
        data: &[u8],
        wide_strings: &reader::WideStrings,
    ) {
        reader::parse_data_stream(footprint, data, Some(wide_strings));
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

    /// Writes the library to any writer implementing `Read + Write + Seek`.
    ///
    /// Takes `&mut self` because it materialises referenced 3D models
    /// (`prepare_3d_models_for_writing`) before serialising.
    ///
    /// # Errors
    ///
    /// Returns an error if the library cannot be serialised.
    pub fn write(
        &mut self,
        writer: impl std::io::Read + std::io::Write + std::io::Seek,
    ) -> AltiumResult<()> {
        // Convert model_3d references to ComponentBody + EmbeddedModel before writing
        self.prepare_3d_models_for_writing()?;

        let mut cfb = crate::altium::create_ole(writer)?;

        // Generate OLE-safe names for all footprints (handles long names and collisions)
        let ole_names =
            crate::altium::generate_ole_names(self.footprints.iter().map(|f| f.name.as_str()));

        // Write FileHeader (pipe-delimited format for reader compatibility)
        self.write_file_header(&mut cfb, &ole_names)?;

        // Write Library storage (Header + Data for Altium compatibility)
        self.write_library(&mut cfb, &ole_names)?;

        // Write embedded 3D models if present (under /Library/Models/)
        self.write_models(&mut cfb)?;

        // Write each footprint using its OLE-safe name
        for (footprint, ole_name) in self.footprints.iter().zip(ole_names.iter()) {
            self.write_footprint(&mut cfb, footprint, ole_name)?;
        }

        // Write the root FileVersionInfo metadata storage.
        Self::write_file_version_info(&mut cfb)?;

        tracing::info!(
            count = self.footprints.len(),
            models = self.models.len(),
            "Wrote PcbLib"
        );

        Ok(())
    }

    /// Converts `model_3d` references to `ComponentBody` + `EmbeddedModel` for writing.
    ///
    /// This method processes all footprints that have a `model_3d` field set:
    /// 1. Reads the STEP file from disk (using the filepath)
    /// 2. Creates an `EmbeddedModel` with a generated GUID
    /// 3. Creates a `ComponentBody` referencing the model
    /// 4. Adds the `ComponentBody` to the footprint's `component_bodies`
    /// 5. Adds the `EmbeddedModel` to the library's `models` collection
    ///
    /// # Errors
    ///
    /// Returns an error if a STEP file cannot be read.
    fn prepare_3d_models_for_writing(&mut self) -> AltiumResult<()> {
        use uuid::Uuid;

        for footprint in &mut self.footprints {
            if let Some(ref model_3d) = footprint.model_3d {
                let path = std::path::Path::new(&model_3d.filepath);

                // Only ever surface the bare file name in logs/errors, never the
                // caller's full path (sanitisation rule).
                let display_name = path.file_name().map_or_else(
                    || "<model>".to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );

                // Check if the filepath looks like an explicit path (has directory components)
                // vs just a model name (which gets set during read from ComponentBody)
                let is_explicit_path = path.parent().is_some_and(|p| !p.as_os_str().is_empty());

                // If footprint already has component_bodies AND filepath is just a name
                // (not an explicit path), skip to prevent Bug #2 (duplicate component bodies
                // from accidentally matching a file with the same name as the model)
                if !footprint.component_bodies.is_empty() && !is_explicit_path {
                    tracing::trace!(
                        footprint = %footprint.name,
                        component_bodies = footprint.component_bodies.len(),
                        model = %display_name,
                        "Skipping model_3d - footprint already has ComponentBody and filepath is not explicit"
                    );
                    continue;
                }

                // Check if file exists
                if !path.exists() || !path.is_file() {
                    // If footprint has existing component_bodies, the model is already embedded
                    // from a previous save - just skip (the filepath is stale)
                    if !footprint.component_bodies.is_empty() {
                        tracing::trace!(
                            footprint = %footprint.name,
                            model = %display_name,
                            "Skipping model_3d - file not found but ComponentBody exists"
                        );
                        continue;
                    }

                    // For NEW footprints, the file MUST exist - return error
                    return Err(AltiumError::InvalidParameter {
                        name: "step_model.filepath".to_string(),
                        message: format!(
                            "STEP file not found for footprint '{}': '{}'. \
                             Provide a valid path or use embed: false for external reference.",
                            footprint.name, display_name
                        ),
                    });
                }

                // If we have component_bodies but user explicitly set a path that exists,
                // they want to re-embed. Clear the old component_bodies first.
                if !footprint.component_bodies.is_empty() {
                    tracing::debug!(
                        footprint = %footprint.name,
                        old_bodies = footprint.component_bodies.len(),
                        "Clearing old ComponentBodies for re-embedding from new path"
                    );
                    footprint.component_bodies.clear();
                }

                // Read the STEP file
                let step_data = std::fs::read(path).map_err(|e| AltiumError::file_read(path, e))?;

                // Generate a GUID for the model
                let guid = format!("{{{}}}", Uuid::new_v4().to_string().to_uppercase());

                // Extract filename from path
                let filename = path.file_name().map_or_else(
                    || "model.step".to_string(),
                    |n| n.to_string_lossy().to_string(),
                );

                // Create EmbeddedModel
                let embedded_model = EmbeddedModel::new(&guid, &filename, step_data);
                self.models.push(embedded_model);

                // Create ComponentBody referencing the model
                let component_body = ComponentBody {
                    model_id: guid,
                    model_name: filename,
                    embedded: true,
                    rotation_x: 0.0,
                    rotation_y: 0.0,
                    rotation_z: model_3d.rotation,
                    z_offset: model_3d.z_offset,
                    overall_height: 0.0, // Could be calculated from STEP, but not implemented
                    standoff_height: 0.0,
                    layer: Layer::Top3DBody,
                    outline: Vec::new(), // Synthesised from the footprint extent on write
                    unique_id: None,
                };

                footprint.component_bodies.push(component_body);

                tracing::debug!(
                    footprint = %footprint.name,
                    filepath = %model_3d.filepath,
                    "Converted model_3d to ComponentBody"
                );
            }
        }

        Ok(())
    }

    /// Generates OLE-safe names for all footprints.
    ///
    /// OLE Compound File names are limited to 31 characters. This method:
    /// - Returns names as-is if they fit within the limit
    /// - Truncates longer names and adds unique suffixes to avoid collisions
    ///
    /// The full footprint name is still stored in the PATTERN field.
    /// Writes embedded 3D models to `/Library/Models/` storage.
    ///
    /// Creates:
    /// - `/Library/Models/Header` - Model count and metadata
    /// - `/Library/Models/Data` - GUID-to-index mapping
    /// - `/Library/Models/{N}` - zlib-compressed STEP files
    fn write_models<F: std::io::Read + std::io::Write + std::io::Seek>(
        &self,
        cfb: &mut cfb::CompoundFile<F>,
    ) -> AltiumResult<()> {
        if self.models.is_empty() {
            return Ok(());
        }

        // Create /Library storage if it doesn't exist
        if !cfb.exists("/Library") {
            crate::altium::create_storage(cfb, "/Library")?;
        }

        // Create /Library/Models storage
        crate::altium::create_storage(cfb, "/Library/Models")?;

        // Write Header stream
        let header_data = writer::encode_model_header_stream(self.models.len());
        crate::altium::write_stream(cfb, "/Library/Models/Header", &header_data)?;

        // Write Data stream (GUID-to-index mapping)
        let data_content = writer::encode_model_data_stream(&self.models);
        crate::altium::write_stream(cfb, "/Library/Models/Data", &data_content)?;

        // Write individual model streams (compressed)
        let compressed_models = writer::prepare_models_for_writing(&self.models)?;
        for (idx, compressed) in compressed_models {
            crate::altium::write_stream(cfb, &format!("/Library/Models/{idx}"), &compressed)?;
        }

        tracing::debug!(count = self.models.len(), "Wrote embedded 3D models");
        Ok(())
    }

    /// Writes the `/FileHeader` stream.
    ///
    /// The `FileHeader` contains a binary-encoded version string:
    /// ```text
    /// [string_length:4 LE u32][string_length:1 u8]["PCB 6.0 Binary Library File"]
    /// ```
    ///
    /// The 4-byte and 1-byte lengths are the same value (27).
    /// Component metadata is stored in `/Library/Data`, not here.
    #[allow(clippy::unused_self)]
    fn write_file_header<F: std::io::Read + std::io::Write + std::io::Seek>(
        &self,
        cfb: &mut cfb::CompoundFile<F>,
        _ole_names: &[String],
    ) -> AltiumResult<()> {
        // The canonical PcbLib FileHeader is 53 bytes with THREE fields (matching
        // AltiumSharp PcbLibWriter.WriteFileHeader). Altium Designer rejects the
        // file if the 5.01 version double and the UniqueId block are missing.
        let version_string = b"PCB 6.0 Binary Library File";
        #[allow(clippy::cast_possible_truncation)]
        let len = version_string.len() as u32;

        let unique_id = crate::util::generate_unique_id();
        let uid_bytes = unique_id.as_bytes();
        #[allow(clippy::cast_possible_truncation)]
        let uid_len = uid_bytes.len() as u32; // always 8

        let mut data =
            Vec::with_capacity(4 + 1 + version_string.len() + 8 + 4 + 1 + uid_bytes.len());
        // Field 1: version string block ([u32 len][u8 len][bytes]).
        data.extend_from_slice(&len.to_le_bytes());
        #[allow(clippy::cast_possible_truncation)]
        data.push(len as u8);
        data.extend_from_slice(version_string);
        // Field 2: version double 5.01 (8 raw little-endian bytes, NO length prefix).
        data.extend_from_slice(&5.01_f64.to_le_bytes());
        // Field 3: 8-char UniqueId string block ([u32 len][u8 len][bytes]).
        data.extend_from_slice(&uid_len.to_le_bytes());
        #[allow(clippy::cast_possible_truncation)]
        data.push(uid_len as u8);
        data.extend_from_slice(uid_bytes);

        crate::altium::write_stream(cfb, "/FileHeader", &data)?;

        Ok(())
    }

    /// Writes the `/Library` storage with Header and Data streams.
    ///
    /// # Streams Created
    ///
    /// - `/Library/Header` - 4-byte record count (always 1)
    /// - `/Library/Data` - Library parameters + component count + component names
    ///
    /// # Format
    ///
    /// Library/Data:
    /// ```text
    /// [block_len:4]["|KEY=VAL|..." + \x00]   // parameter block (null-terminated)
    /// [component_count:4 LE u32]
    /// [block_len:4][str_len:1][name]          // per component (WriteStringBlock)
    /// ```
    fn write_library<F: std::io::Read + std::io::Write + std::io::Seek>(
        &self,
        cfb: &mut cfb::CompoundFile<F>,
        ole_names: &[String],
    ) -> AltiumResult<()> {
        // Create /Library storage
        crate::altium::create_storage(cfb, "/Library")?;

        // Write Library/Header (record count = 1)
        crate::altium::write_stream(cfb, "/Library/Header", &1u32.to_le_bytes())?;

        // Build Library/Data content: a C-string parameter block, then the
        // component count + names.
        let params = Self::build_library_params(self.filepath.as_deref().unwrap_or(""));
        let mut data = Vec::new();
        crate::altium::framing::write_cstring_param_block(&mut data, params.as_bytes());

        // Component count
        #[allow(clippy::cast_possible_truncation)]
        data.extend_from_slice(&(self.footprints.len() as u32).to_le_bytes());

        // Component names as WriteStringBlock: [block_len:4][str_len:1][string]
        for ole_name in ole_names {
            let name_bytes = ole_name.as_bytes();
            #[allow(clippy::cast_possible_truncation)]
            {
                data.extend_from_slice(&((name_bytes.len() + 1) as u32).to_le_bytes());
                data.push(name_bytes.len() as u8);
            }
            data.extend_from_slice(name_bytes);
        }

        // Write Library/Data
        crate::altium::write_stream(cfb, "/Library/Data", &data)?;

        // Write the library metadata storages Altium emits for every library.
        self.write_library_metadata(cfb)?;

        Ok(())
    }

    /// Wraps a parameter string as an Altium C-string block:
    /// `[block_len:4][text + \x00]` where `block_len` includes the terminator.
    fn param_block(text: &str) -> Vec<u8> {
        let bytes = text.as_bytes();
        let mut v = Vec::with_capacity(4 + bytes.len() + 1);
        #[allow(clippy::cast_possible_truncation)]
        v.extend_from_slice(&((bytes.len() + 1) as u32).to_le_bytes());
        v.extend_from_slice(bytes);
        v.push(0x00);
        v
    }

    /// Creates a child storage containing a `Header` (record count) stream and a
    /// `Data` stream — the shape every Altium metadata storage uses.
    fn write_meta_storage<F: std::io::Read + std::io::Write + std::io::Seek>(
        cfb: &mut cfb::CompoundFile<F>,
        path: &str,
        header_count: u32,
        data: &[u8],
    ) -> AltiumResult<()> {
        crate::altium::create_storage(cfb, path)?;
        crate::altium::write_stream(cfb, &format!("{path}/Header"), &header_count.to_le_bytes())?;
        crate::altium::write_stream(cfb, &format!("{path}/Data"), data)?;
        Ok(())
    }

    /// Writes the `/Library` metadata storages that Altium emits for every
    /// library (`LayerKindMapping`, `PadViaLibrary`, `ComponentParamsTOC`, and
    /// the empty `Textures` / `ModelsNoEmbed`). Without these, Altium Designer
    /// considers the library incomplete.
    fn write_library_metadata<F: std::io::Read + std::io::Write + std::io::Seek>(
        &self,
        cfb: &mut cfb::CompoundFile<F>,
    ) -> AltiumResult<()> {
        use std::fmt::Write as _;
        use uuid::Uuid;

        // LayerKindMapping: [u32 textLen][UTF-16LE "1.0\0"][u32 signature=0][u32 count=0]
        let text16: Vec<u8> = "1.0\0".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut lkm = Vec::with_capacity(text16.len() + 12);
        #[allow(clippy::cast_possible_truncation)]
        lkm.extend_from_slice(&(text16.len() as u32).to_le_bytes());
        lkm.extend_from_slice(&text16);
        lkm.extend_from_slice(&0u32.to_le_bytes()); // signature
        lkm.extend_from_slice(&0u32.to_le_bytes()); // entry count
        Self::write_meta_storage(cfb, "/Library/LayerKindMapping", 1, &lkm)?;

        // PadViaLibrary: empty cache with a fresh library id.
        let guid = Uuid::new_v4().to_string().to_uppercase();
        let pvl = Self::param_block(&format!(
            "|PADVIALIBRARY.LIBRARYID={{{guid}}}|PADVIALIBRARY.LIBRARYNAME=<Local>|PADVIALIBRARY.DISPLAYUNITS=1"
        ));
        Self::write_meta_storage(cfb, "/Library/PadViaLibrary", 0, &pvl)?;

        // ComponentParamsTOC: one CRLF-terminated line per footprint.
        let mut toc = String::new();
        for fp in &self.footprints {
            let _ = write!(
                toc,
                "Name={}|Pad Count={}|Height=0|Description={}\r\n",
                fp.name,
                fp.pads.len(),
                fp.description
            );
        }
        Self::write_meta_storage(
            cfb,
            "/Library/ComponentParamsTOC",
            1,
            &Self::param_block(&toc),
        )?;

        // Always-empty library sub-storages.
        Self::write_meta_storage(cfb, "/Library/Textures", 0, &[])?;
        Self::write_meta_storage(cfb, "/Library/ModelsNoEmbed", 0, &[])?;

        // EmbeddedFonts is a plain stream holding a u32 font count (0).
        crate::altium::write_stream(cfb, "/Library/EmbeddedFonts", &0u32.to_le_bytes())?;

        // Empty Models storage when the library has no embedded models
        // (otherwise write_models creates it). Altium expects it to exist.
        if self.models.is_empty() {
            Self::write_meta_storage(cfb, "/Library/Models", 0, &[])?;
        }

        Ok(())
    }

    /// Writes the root `/FileVersionInfo` storage (Altium's version-history
    /// metadata). The payload is a fixed, library-agnostic blob.
    fn write_file_version_info<F: std::io::Read + std::io::Write + std::io::Seek>(
        cfb: &mut cfb::CompoundFile<F>,
    ) -> AltiumResult<()> {
        const FVI_DATA: &[u8] = include_bytes!("assets/file_version_info.bin");
        Self::write_meta_storage(cfb, "/FileVersionInfo", 1, FVI_DATA)
    }

    /// Builds the pipe-delimited parameter string for `/Library/Data`.
    ///
    /// Format: `|KEY=VAL|KEY=VAL|...` (leading pipe, NO trailing pipe).
    ///
    /// Altium Designer requires `VERSION=3.00` plus a minimal V9 layer stack
    /// definition to consider the file valid.
    fn build_library_params(filename: &str) -> String {
        use std::fmt::Write;

        let mut p = String::with_capacity(4096);

        // Core metadata (must be first, matching AltiumSharp order)
        let _ = write!(p, "|FILENAME={filename}");
        p.push_str("|KIND=Protel_Advanced_PCB_Library");
        p.push_str("|VERSION=3.00");
        let now = chrono::Local::now();
        let _ = write!(p, "|DATE={}", now.format("%d. %m. %Y"));
        let _ = write!(p, "|TIME={}", now.format("%H:%M:%S"));

        // V9 layer stack + full board configuration. A synthesised stack is
        // rejected by Altium ("Catastrophic failure whilst loading section
        // Library"), so we splice in a complete, known-good stack captured
        // verbatim from a real Altium-authored library (scripts/sample.PcbLib).
        p.push('|');
        p.push_str(include_str!("assets/library_data_stack.txt"));

        p
    }

    /// Writes a single footprint to the OLE document.
    ///
    /// # Arguments
    ///
    /// * `cfb` - The OLE compound file
    /// * `footprint` - The footprint to write
    /// * `ole_name` - The OLE-safe storage name (≤31 chars, unique)
    ///
    /// # Streams Created
    ///
    /// - `/{ole_name}/Header` - 4-byte primitive count
    /// - `/{ole_name}/Parameters` - Footprint metadata
    /// - `/{ole_name}/Data` - Binary primitive data
    /// - `/{ole_name}/WideStrings` - Encoded text content
    /// - `/{ole_name}/PrimitiveGuids/Header` - GUID record count
    /// - `/{ole_name}/PrimitiveGuids/Data` - GUIDs for each primitive
    /// - `/{ole_name}/UniqueIDPrimitiveInformation/Header` - UID record count (if applicable)
    /// - `/{ole_name}/UniqueIDPrimitiveInformation/Data` - UID data (if applicable)
    #[allow(clippy::unused_self)] // Method for consistency with other write methods
    fn write_footprint<F: std::io::Read + std::io::Write + std::io::Seek>(
        &self,
        cfb: &mut cfb::CompoundFile<F>,
        footprint: &Footprint,
        ole_name: &str,
    ) -> AltiumResult<()> {
        let storage_path = format!("/{ole_name}");

        // Create storage for the footprint
        crate::altium::create_storage(cfb, &storage_path)?;

        // Write Header stream (4-byte primitive count)
        let header_data = writer::encode_component_header(footprint);
        crate::altium::write_stream(cfb, &format!("{storage_path}/Header"), &header_data)?;

        // Write Parameters stream as a C-string parameter block.
        // Keys (no trailing pipe): PATTERN, HEIGHT, DESCRIPTION, ITEMGUID, REVISIONGUID.
        let params = format!(
            "|PATTERN={}|HEIGHT=0mil|DESCRIPTION={}|ITEMGUID=|REVISIONGUID=",
            footprint.name, footprint.description
        );
        let mut params_data = Vec::new();
        crate::altium::framing::write_cstring_param_block(&mut params_data, params.as_bytes());
        crate::altium::write_stream(cfb, &format!("{storage_path}/Parameters"), &params_data)?;

        // Write Data stream with primitives
        let data = Self::encode_primitives(footprint)?;
        crate::altium::write_stream(cfb, &format!("{storage_path}/Data"), &data)?;

        // Write WideStrings stream (per-component)
        let wide_strings_data = writer::encode_component_wide_strings(footprint);
        crate::altium::write_stream(
            cfb,
            &format!("{storage_path}/WideStrings"),
            &wide_strings_data,
        )?;

        // PrimitiveGuids is the editor's optional per-primitive GUID cache.
        // Altium (and AltiumSharp) omit it for from-scratch footprints, so we
        // do too — writing it with a guessed record layout only risked rejection.

        // Write UniqueIDPrimitiveInformation streams if any primitives have unique IDs
        if let Some(uid_data) = writer::encode_unique_id_stream(footprint) {
            // Create UniqueIDPrimitiveInformation storage
            let uid_storage_path = format!("{storage_path}/UniqueIDPrimitiveInformation");
            crate::altium::create_storage(cfb, &uid_storage_path)?;

            // Write Header + Data streams.
            let uid_header_data = writer::encode_unique_id_header(footprint);
            crate::altium::write_stream(
                cfb,
                &format!("{uid_storage_path}/Header"),
                &uid_header_data,
            )?;
            crate::altium::write_stream(cfb, &format!("{uid_storage_path}/Data"), &uid_data)?;

            tracing::trace!(
                footprint = %footprint.name,
                size = uid_data.len(),
                "Wrote UniqueIDPrimitiveInformation streams"
            );
        }

        Ok(())
    }

    /// Encodes footprint primitives to binary format.
    ///
    /// See the [`writer`] module for format details.
    ///
    /// # Errors
    ///
    /// Returns an error if any string (footprint name, pad designator, text) exceeds 255 bytes.
    fn encode_primitives(footprint: &Footprint) -> AltiumResult<Vec<u8>> {
        writer::encode_data_stream(footprint)
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
        // Build a position map for the desired order
        let order_map: std::collections::HashMap<&str, usize> = new_order
            .iter()
            .enumerate()
            .map(|(i, name)| (*name, i))
            .collect();

        // Sort footprints: those in order_map come first (by their position),
        // then those not in the map (preserving relative order via stable sort)
        let max_pos = new_order.len();
        self.footprints.sort_by(|a, b| {
            let pos_a = order_map.get(a.name.as_str()).copied().unwrap_or(max_pos);
            let pos_b = order_map.get(b.name.as_str()).copied().unwrap_or(max_pos);
            pos_a.cmp(&pos_b)
        });

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
            flags: PcbFlags::empty(),
            unique_id: None,
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
        let mut original = Footprint::new("ROUNDTRIP_PAD_ADVANCED");

        // Create a pad with hole shape and mask expansion
        let mut pad_with_square_hole = Pad::through_hole("1", -2.54, 0.0, 1.8, 1.8, 1.0);
        pad_with_square_hole.hole_shape = HoleShape::Square;
        pad_with_square_hole.solder_mask_expansion = Some(0.1);
        pad_with_square_hole.solder_mask_expansion_manual = true;
        original.add_pad(pad_with_square_hole);

        // Create a pad with slot hole
        let mut pad_with_slot = Pad::through_hole("2", 0.0, 0.0, 2.0, 1.5, 0.8);
        pad_with_slot.hole_shape = HoleShape::Slot;
        pad_with_slot.paste_mask_expansion = Some(-0.05);
        pad_with_slot.paste_mask_expansion_manual = true;
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
        assert!(decoded.pads[0].solder_mask_expansion_manual);

        // Pad 2: Slot hole + paste mask expansion
        assert_eq!(decoded.pads[1].designator, "2");
        assert_eq!(decoded.pads[1].hole_shape, HoleShape::Slot);
        assert!(decoded.pads[1].paste_mask_expansion.is_some());
        assert!(approx_eq(
            decoded.pads[1].paste_mask_expansion.unwrap(),
            -0.05,
            0.001
        ));
        assert!(decoded.pads[1].paste_mask_expansion_manual);

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

        // Create unique ID entries
        let entries = vec![
            reader::UniqueIdEntry {
                primitive_index: 1,
                primitive_type: "Pad".to_string(),
                unique_id: "PADUID01".to_string(),
            },
            reader::UniqueIdEntry {
                primitive_index: 2,
                primitive_type: "Pad".to_string(),
                unique_id: "PADUID02".to_string(),
            },
            reader::UniqueIdEntry {
                primitive_index: 1,
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
