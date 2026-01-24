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

pub mod primitives;
mod reader;
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
#[derive(Debug, Clone, Default)]
pub struct PcbLib {
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

    /// Reads a `PcbLib` from a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or is not a valid `PcbLib`.
    pub fn read(path: impl AsRef<std::path::Path>) -> AltiumResult<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).map_err(|e| AltiumError::file_read(path, e))?;

        Self::read_from(file, path)
    }

    /// Reads a `PcbLib` from a reader.
    fn read_from(
        reader: impl std::io::Read + std::io::Seek,
        path: &std::path::Path,
    ) -> AltiumResult<Self> {
        use cfb::CompoundFile;

        let mut cfb = CompoundFile::open(reader)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to open OLE file: {e}")))?;

        let mut library = Self::new();

        // Read FileHeader for library metadata (validates file type)
        library.metadata = Self::read_file_header(&mut cfb)?;

        // Read Storage stream for UniqueIdPrimitiveInformation (if present)
        // Note: This is currently a stub - the format is not fully documented
        Self::read_storage_stream(&mut cfb);

        // Read WideStrings stream if present (contains text content for Text primitives)
        let wide_strings = Self::read_wide_strings(&mut cfb);

        // Read embedded 3D models if present
        library.models = Self::read_models(&mut cfb);

        // List all entries to find footprint storages
        let entries: Vec<_> = cfb.walk().map(|e| e.path().to_path_buf()).collect();

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
                        Ok(footprint) => library.footprints.push(footprint),
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

        // Populate model_3d from component_bodies for backward compatibility
        library.populate_model_3d_from_component_bodies();

        tracing::info!(
            path = %path.display(),
            count = library.footprints.len(),
            "Read PcbLib"
        );

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
                let filepath = self
                    .models
                    .iter()
                    .find(|m| m.id == body.model_id)
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
    /// The `FileHeader` contains pipe-delimited key=value pairs:
    /// - `HEADER`: File type identifier
    /// - `CompCount`: Number of components
    /// - `LibRef{N}`: Component names (0-indexed)
    /// - `CompDescr{N}`: Component descriptions
    ///
    /// # Errors
    ///
    /// Returns an error if the file is not a valid `PcbLib` (wrong file type).
    fn read_file_header<F: std::io::Read + std::io::Seek>(
        cfb: &mut cfb::CompoundFile<F>,
    ) -> AltiumResult<LibraryMetadata> {
        let mut metadata = LibraryMetadata::default();

        let header_path = std::path::Path::new("/FileHeader");
        if !cfb.is_stream(header_path) {
            return Ok(metadata);
        }

        let Ok(mut stream) = cfb.open_stream(header_path) else {
            return Ok(metadata);
        };

        let mut data = Vec::new();
        if std::io::Read::read_to_end(&mut stream, &mut data).is_err() {
            return Ok(metadata);
        }

        // FileHeader is ASCII text with pipe-delimited key=value pairs
        let Ok(text) = String::from_utf8(data) else {
            return Ok(metadata);
        };

        // Parse key=value pairs
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
                        // Check for LibRef{N} and CompDescr{N} patterns
                        if let Some(idx_str) = key_upper.strip_prefix("LIBREF") {
                            if let Ok(idx) = idx_str.parse::<usize>() {
                                // Ensure vector is large enough
                                while metadata.component_names.len() <= idx {
                                    metadata.component_names.push(String::new());
                                }
                                metadata.component_names[idx] = value.to_string();
                            }
                        } else if let Some(idx_str) = key_upper.strip_prefix("COMPDESCR") {
                            if let Ok(idx) = idx_str.parse::<usize>() {
                                // Ensure vector is large enough
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
        if !metadata.header.is_empty() && !metadata.header.contains("PCB Library") {
            // Detect what type it actually is for a helpful error message
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
            "Parsed FileHeader"
        );

        Ok(metadata)
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
        let storage_path = std::path::Path::new("/Storage");
        if !cfb.is_stream(storage_path) {
            return;
        }

        let Ok(mut stream) = cfb.open_stream(storage_path) else {
            return;
        };

        let mut data = Vec::new();
        if std::io::Read::read_to_end(&mut stream, &mut data).is_err() {
            return;
        }

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
        let wide_strings_path = std::path::Path::new("/WideStrings");
        if cfb.is_stream(wide_strings_path) {
            if let Ok(mut stream) = cfb.open_stream(wide_strings_path) {
                let mut data = Vec::new();
                if std::io::Read::read_to_end(&mut stream, &mut data).is_ok() {
                    return reader::parse_wide_strings(&data);
                }
            }
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
        let _model_count = cfb
            .is_stream(&header_path)
            .then(|| {
                cfb.open_stream(&header_path).ok().and_then(|mut stream| {
                    let mut data = Vec::new();
                    std::io::Read::read_to_end(&mut stream, &mut data)
                        .ok()
                        .map(|_| reader::parse_model_header_stream(&data))
                })
            })
            .flatten()
            .unwrap_or(0);

        // Read Data stream to get GUID-to-index mapping
        let data_path = models_storage.join("Data");
        let model_index = cfb
            .is_stream(&data_path)
            .then(|| {
                cfb.open_stream(&data_path).ok().and_then(|mut stream| {
                    let mut data = Vec::new();
                    std::io::Read::read_to_end(&mut stream, &mut data)
                        .ok()
                        .map(|_| reader::parse_model_data_stream(&data))
                })
            })
            .flatten()
            .unwrap_or_default();

        if model_index.is_empty() {
            tracing::debug!("No model index found in /Library/Models/Data");
            return Vec::new();
        }

        // Read compressed model streams
        let mut model_data: Vec<(usize, Vec<u8>)> = Vec::new();

        // Model streams are numbered 0, 1, 2, ...
        for idx in 0..100 {
            // Reasonable upper limit
            let stream_path = models_storage.join(idx.to_string());
            if cfb.is_stream(&stream_path) {
                if let Ok(mut stream) = cfb.open_stream(&stream_path) {
                    let mut data = Vec::new();
                    if std::io::Read::read_to_end(&mut stream, &mut data).is_ok() {
                        tracing::trace!(
                            index = idx,
                            size = data.len(),
                            "Read compressed model stream"
                        );
                        model_data.push((idx, data));
                    }
                }
            } else {
                // No more model streams
                break;
            }
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
        if cfb.is_stream(&params_path) {
            if let Ok(mut stream) = cfb.open_stream(&params_path) {
                let mut params_data = Vec::new();
                if std::io::Read::read_to_end(&mut stream, &mut params_data).is_ok() {
                    Self::parse_parameters(&mut footprint, &params_data);
                }
            }
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
        if cfb.is_stream(&unique_id_path) {
            if let Ok(mut stream) = cfb.open_stream(&unique_id_path) {
                let mut uid_data = Vec::new();
                if std::io::Read::read_to_end(&mut stream, &mut uid_data).is_ok() {
                    let unique_ids = reader::parse_unique_id_stream(&uid_data);
                    reader::apply_unique_ids(&mut footprint, &unique_ids);
                }
            }
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
                        "PATTERN" => {
                            // Use PATTERN as the canonical name since OLE storage names
                            // are limited to 31 characters
                            if !value.is_empty() {
                                footprint.name = value.to_string();
                            }
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

    /// Writes the library to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn write(&mut self, path: impl AsRef<std::path::Path>) -> AltiumResult<()> {
        let path = path.as_ref();
        let file = std::fs::File::create(path).map_err(|e| AltiumError::file_write(path, e))?;

        self.write_to(file, path)
    }

    /// Writes the library to a writer.
    fn write_to(
        &mut self,
        writer: impl std::io::Read + std::io::Write + std::io::Seek,
        path: &std::path::Path,
    ) -> AltiumResult<()> {
        use cfb::CompoundFile;

        // Convert model_3d references to ComponentBody + EmbeddedModel before writing
        self.prepare_3d_models_for_writing()?;

        let mut cfb = CompoundFile::create(writer)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to create OLE file: {e}")))?;

        // Generate OLE-safe names for all footprints (handles long names and collisions)
        let ole_names = self.generate_ole_names();

        // Write FileHeader with OLE names
        self.write_file_header(&mut cfb, &ole_names)?;

        // Write WideStrings stream if there's text content
        self.write_wide_strings(&mut cfb)?;

        // Write embedded 3D models if present
        self.write_models(&mut cfb)?;

        // Write each footprint using its OLE-safe name
        for (footprint, ole_name) in self.footprints.iter().zip(ole_names.iter()) {
            self.write_footprint(&mut cfb, footprint, ole_name)?;
        }

        tracing::info!(
            path = %path.display(),
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
                // Read the STEP file
                let step_data = std::fs::read(&model_3d.filepath).map_err(|e| {
                    AltiumError::file_read(std::path::Path::new(&model_3d.filepath), e)
                })?;

                // Generate a GUID for the model
                let guid = format!("{{{}}}", Uuid::new_v4().to_string().to_uppercase());

                // Extract filename from path
                let filename = std::path::Path::new(&model_3d.filepath)
                    .file_name()
                    .map_or_else(|| "model.step".to_string(), |n| n.to_string_lossy().to_string());

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
    fn generate_ole_names(&self) -> Vec<String> {
        use std::collections::HashSet;

        let mut used_names = HashSet::new();
        let mut ole_names = Vec::with_capacity(self.footprints.len());

        for footprint in &self.footprints {
            let ole_name = super::generate_ole_name(&footprint.name, &used_names);
            used_names.insert(ole_name.clone());
            ole_names.push(ole_name);
        }

        ole_names
    }

    /// Writes the `WideStrings` stream if there's text content to store.
    fn write_wide_strings<F: std::io::Read + std::io::Write + std::io::Seek>(
        &self,
        cfb: &mut cfb::CompoundFile<F>,
    ) -> AltiumResult<()> {
        // Collect text content from all footprints
        let texts = writer::collect_wide_strings_content(&self.footprints);

        if texts.is_empty() {
            return Ok(());
        }

        // Convert to references for encoding
        let text_refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let data = writer::encode_wide_strings(&text_refs);

        let mut stream = cfb
            .create_stream("/WideStrings")
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to create WideStrings: {e}")))?;
        std::io::Write::write_all(&mut stream, &data)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to write WideStrings: {e}")))?;

        tracing::debug!(count = texts.len(), "Wrote WideStrings stream");
        Ok(())
    }

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
            cfb.create_storage("/Library").map_err(|e| {
                AltiumError::invalid_ole(format!("Failed to create Library storage: {e}"))
            })?;
        }

        // Create /Library/Models storage
        cfb.create_storage("/Library/Models").map_err(|e| {
            AltiumError::invalid_ole(format!("Failed to create Models storage: {e}"))
        })?;

        // Write Header stream
        let header_data = writer::encode_model_header_stream(self.models.len());
        let mut header_stream = cfb.create_stream("/Library/Models/Header").map_err(|e| {
            AltiumError::invalid_ole(format!("Failed to create Models/Header: {e}"))
        })?;
        std::io::Write::write_all(&mut header_stream, &header_data)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to write Models/Header: {e}")))?;

        // Write Data stream (GUID-to-index mapping)
        let data_content = writer::encode_model_data_stream(&self.models);
        let mut data_stream = cfb
            .create_stream("/Library/Models/Data")
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to create Models/Data: {e}")))?;
        std::io::Write::write_all(&mut data_stream, &data_content)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to write Models/Data: {e}")))?;

        // Write individual model streams (compressed)
        let compressed_models = writer::prepare_models_for_writing(&self.models)?;
        for (idx, compressed) in compressed_models {
            let stream_path = format!("/Library/Models/{idx}");
            let mut model_stream = cfb.create_stream(&stream_path).map_err(|e| {
                AltiumError::invalid_ole(format!("Failed to create model stream {idx}: {e}"))
            })?;
            std::io::Write::write_all(&mut model_stream, &compressed).map_err(|e| {
                AltiumError::invalid_ole(format!("Failed to write model stream {idx}: {e}"))
            })?;
        }

        tracing::debug!(count = self.models.len(), "Wrote embedded 3D models");
        Ok(())
    }

    /// Writes the `FileHeader` stream.
    ///
    /// The `FileHeader` contains library metadata as pipe-delimited key=value pairs:
    /// - `HEADER`: File type identifier
    /// - `WEIGHT`: Number of components (same as `CompCount`)
    /// - `CompCount`: Number of components
    /// - `LibRef{N}`: OLE storage names (used for lookup)
    /// - `CompDescr{N}`: Component descriptions
    fn write_file_header<F: std::io::Read + std::io::Write + std::io::Seek>(
        &self,
        cfb: &mut cfb::CompoundFile<F>,
        ole_names: &[String],
    ) -> AltiumResult<()> {
        use std::fmt::Write;

        let mut header = String::new();

        // File type identifier
        header.push_str("|HEADER=Protel for Windows - PCB Library");

        // Component count (WEIGHT is legacy name, CompCount is modern)
        let count = self.footprints.len();
        let _ = write!(header, "|WEIGHT={count}");
        let _ = write!(header, "|COMPCOUNT={count}");

        // Component names and descriptions
        for (idx, (footprint, ole_name)) in self.footprints.iter().zip(ole_names.iter()).enumerate()
        {
            // LibRef uses the OLE-safe name (for storage path lookup)
            let _ = write!(header, "|LIBREF{idx}={ole_name}");

            // CompDescr uses the footprint description
            if !footprint.description.is_empty() {
                let _ = write!(header, "|COMPDESCR{idx}={}", footprint.description);
            }
        }

        header.push('|');

        let mut stream = cfb
            .create_stream("/FileHeader")
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to create FileHeader: {e}")))?;
        std::io::Write::write_all(&mut stream, header.as_bytes())
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to write FileHeader: {e}")))?;

        Ok(())
    }

    /// Writes a single footprint to the OLE document.
    ///
    /// # Arguments
    ///
    /// * `cfb` - The OLE compound file
    /// * `footprint` - The footprint to write
    /// * `ole_name` - The OLE-safe storage name (≤31 chars, unique)
    #[allow(clippy::unused_self)] // Method for consistency with other write methods
    fn write_footprint<F: std::io::Read + std::io::Write + std::io::Seek>(
        &self,
        cfb: &mut cfb::CompoundFile<F>,
        footprint: &Footprint,
        ole_name: &str,
    ) -> AltiumResult<()> {
        let storage_path = format!("/{ole_name}");

        // Create storage for the footprint
        cfb.create_storage(&storage_path)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to create storage: {e}")))?;

        // Write Parameters stream
        let params = format!(
            "|PATTERN={}|DESCRIPTION={}|",
            footprint.name, footprint.description
        );
        let params_path = format!("{storage_path}/Parameters");
        let mut stream = cfb
            .create_stream(&params_path)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to create Parameters: {e}")))?;
        std::io::Write::write_all(&mut stream, params.as_bytes())
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to write Parameters: {e}")))?;

        // Write Data stream with primitives
        let data_path = format!("{storage_path}/Data");
        let mut stream = cfb
            .create_stream(&data_path)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to create Data: {e}")))?;

        let data = Self::encode_primitives(footprint)?;
        std::io::Write::write_all(&mut stream, &data)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to write Data: {e}")))?;

        // Write UniqueIDPrimitiveInformation stream if any primitives have unique IDs
        if let Some(uid_data) = writer::encode_unique_id_stream(footprint) {
            // Create UniqueIDPrimitiveInformation storage
            let uid_storage_path = format!("{storage_path}/UniqueIDPrimitiveInformation");
            cfb.create_storage(&uid_storage_path).map_err(|e| {
                AltiumError::invalid_ole(format!(
                    "Failed to create UniqueIDPrimitiveInformation storage: {e}"
                ))
            })?;

            // Write Data stream inside UniqueIDPrimitiveInformation
            let uid_data_path = format!("{uid_storage_path}/Data");
            let mut uid_stream = cfb.create_stream(&uid_data_path).map_err(|e| {
                AltiumError::invalid_ole(format!(
                    "Failed to create UniqueIDPrimitiveInformation/Data: {e}"
                ))
            })?;
            std::io::Write::write_all(&mut uid_stream, &uid_data).map_err(|e| {
                AltiumError::invalid_ole(format!(
                    "Failed to write UniqueIDPrimitiveInformation/Data: {e}"
                ))
            })?;

            tracing::trace!(
                footprint = %footprint.name,
                size = uid_data.len(),
                "Wrote UniqueIDPrimitiveInformation stream"
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
    pub fn footprints(&self) -> impl Iterator<Item = &Footprint> {
        self.footprints.iter()
    }

    /// Returns a mutable iterator over the footprints.
    pub fn footprints_mut(&mut self) -> impl Iterator<Item = &mut Footprint> {
        self.footprints.iter_mut()
    }

    /// Returns a list of footprint names.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.footprints.iter().map(|f| f.name.clone()).collect()
    }

    /// Gets a footprint by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Footprint> {
        self.footprints.iter().find(|f| f.name == name)
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
    #[must_use]
    pub fn get_model(&self, id: &str) -> Option<&EmbeddedModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Adds an embedded 3D model to the library.
    pub fn add_model(&mut self, model: EmbeddedModel) {
        self.models.push(model);
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

        // Add a ComponentBody with typical values
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

        // Pad 1: Square hole with solder mask expansion
        assert_eq!(decoded.pads[0].designator, "1");
        assert_eq!(decoded.pads[0].hole_shape, HoleShape::Square);
        assert!(decoded.pads[0].solder_mask_expansion.is_some());
        assert!(approx_eq(
            decoded.pads[0].solder_mask_expansion.unwrap(),
            0.1,
            0.001
        ));
        assert!(decoded.pads[0].solder_mask_expansion_manual);

        // Pad 2: Slot hole with paste mask expansion
        assert_eq!(decoded.pads[1].designator, "2");
        assert_eq!(decoded.pads[1].hole_shape, HoleShape::Slot);
        assert!(decoded.pads[1].paste_mask_expansion.is_some());
        assert!(approx_eq(
            decoded.pads[1].paste_mask_expansion.unwrap(),
            -0.05,
            0.001
        ));
        assert!(decoded.pads[1].paste_mask_expansion_manual);

        // Pad 3: Default round hole
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

        // RoundedRectangle pad without explicit corner radius gets default 50%
        let pad_default_radius = Pad::smd("2", 2.54, 0.0, 1.5, 0.8);
        original.add_pad(pad_default_radius);

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

        // RoundedRectangle without explicit radius gets default 50%
        assert_eq!(decoded.pads[1].corner_radius_percent, Some(50));
        assert_eq!(decoded.pads[1].stack_mode, PadStackMode::FullStack);

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
        offsets[0] = (0.1, 0.05); // Top layer offset from hole center
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
        assert_eq!(entries[0].primitive_index, 1);
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
        let result = PcbLib::read_from(buffer, std::path::Path::new("test.PcbLib"));

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
