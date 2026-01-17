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

pub use primitives::{Arc, Layer, Model3D, Pad, PadShape, Region, Text, Track, Vertex};

use crate::altium::error::{AltiumError, AltiumResult};

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

    /// 3D model reference.
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
            tracks: Vec::new(),
            arcs: Vec::new(),
            regions: Vec::new(),
            text: Vec::new(),
            model_3d: None,
        }
    }

    /// Adds a pad to the footprint.
    pub fn add_pad(&mut self, pad: Pad) {
        self.pads.push(pad);
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
}

/// A `PcbLib` footprint library.
#[derive(Debug, Clone, Default)]
pub struct PcbLib {
    /// Footprints in the library.
    footprints: Vec<Footprint>,
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

                if !component_name.is_empty() && component_name != "FileHeader" {
                    // Read the component data
                    match Self::read_footprint(&mut cfb, &entry_path, &component_name) {
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

        tracing::info!(
            path = %path.display(),
            count = library.footprints.len(),
            "Read PcbLib"
        );

        Ok(library)
    }

    /// Reads a single footprint from the OLE document.
    fn read_footprint<F: std::io::Read + std::io::Seek>(
        cfb: &mut cfb::CompoundFile<F>,
        storage_path: &std::path::Path,
        name: &str,
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

            Self::parse_primitives(&mut footprint, &data);
        }

        Ok(footprint)
    }

    /// Parses parameters from the Parameters stream.
    fn parse_parameters(footprint: &mut Footprint, data: &[u8]) {
        // Parameters are typically ASCII key=value pairs separated by |
        if let Ok(text) = String::from_utf8(data.to_vec()) {
            for pair in text.split('|') {
                if let Some((key, value)) = pair.split_once('=') {
                    if key.to_uppercase() == "DESCRIPTION" {
                        footprint.description = value.to_string();
                    }
                }
            }
        }
    }

    /// Parses primitives from the Data stream.
    ///
    /// The Data stream contains binary records for each primitive (pads, tracks, arcs, etc.).
    /// See the [`reader`] module for format details.
    fn parse_primitives(footprint: &mut Footprint, data: &[u8]) {
        reader::parse_data_stream(footprint, data);
    }

    /// Writes the library to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn write(&self, path: impl AsRef<std::path::Path>) -> AltiumResult<()> {
        let path = path.as_ref();
        let file = std::fs::File::create(path).map_err(|e| AltiumError::file_write(path, e))?;

        self.write_to(file, path)
    }

    /// Writes the library to a writer.
    fn write_to(
        &self,
        writer: impl std::io::Read + std::io::Write + std::io::Seek,
        path: &std::path::Path,
    ) -> AltiumResult<()> {
        use cfb::CompoundFile;

        let mut cfb = CompoundFile::create(writer)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to create OLE file: {e}")))?;

        // Write FileHeader
        self.write_file_header(&mut cfb)?;

        // Write each footprint
        for footprint in &self.footprints {
            self.write_footprint(&mut cfb, footprint)?;
        }

        tracing::info!(
            path = %path.display(),
            count = self.footprints.len(),
            "Wrote PcbLib"
        );

        Ok(())
    }

    /// Writes the `FileHeader` stream.
    fn write_file_header<F: std::io::Read + std::io::Write + std::io::Seek>(
        &self,
        cfb: &mut cfb::CompoundFile<F>,
    ) -> AltiumResult<()> {
        let header = format!(
            "|HEADER=Protel for Windows - PCB Library|WEIGHT={}|",
            self.footprints.len()
        );

        let mut stream = cfb
            .create_stream("/FileHeader")
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to create FileHeader: {e}")))?;
        std::io::Write::write_all(&mut stream, header.as_bytes())
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to write FileHeader: {e}")))?;

        Ok(())
    }

    /// Writes a single footprint to the OLE document.
    #[allow(clippy::unused_self)] // Method for consistency with other write methods
    fn write_footprint<F: std::io::Read + std::io::Write + std::io::Seek>(
        &self,
        cfb: &mut cfb::CompoundFile<F>,
        footprint: &Footprint,
    ) -> AltiumResult<()> {
        let storage_path = format!("/{}", footprint.name);

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

        let data = Self::encode_primitives(footprint);
        std::io::Write::write_all(&mut stream, &data)
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to write Data: {e}")))?;

        Ok(())
    }

    /// Encodes footprint primitives to binary format.
    ///
    /// See the [`writer`] module for format details.
    fn encode_primitives(footprint: &Footprint) -> Vec<u8> {
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
        let data = writer::encode_data_stream(&original);

        // Decode from binary
        let mut decoded = Footprint::new("ROUNDTRIP_PAD");
        reader::parse_data_stream(&mut decoded, &data);

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

        let data = writer::encode_data_stream(&original);
        let mut decoded = Footprint::new("ROUNDTRIP_TRACK");
        reader::parse_data_stream(&mut decoded, &data);

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

        let data = writer::encode_data_stream(&original);
        let mut decoded = Footprint::new("ROUNDTRIP_ARC");
        reader::parse_data_stream(&mut decoded, &data);

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

        let data = writer::encode_data_stream(&original);
        let mut decoded = Footprint::new("ROUNDTRIP_MIXED");
        reader::parse_data_stream(&mut decoded, &data);

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

        let data = writer::encode_data_stream(&original);
        let mut decoded = Footprint::new("ROUNDTRIP_PRECISION");
        reader::parse_data_stream(&mut decoded, &data);

        // Altium internal units give ~2.54nm resolution
        assert!(approx_eq(decoded.pads[0].x, 0.125, 0.0001));
        assert!(approx_eq(decoded.pads[1].x, 1.27, 0.0001));
        assert!(approx_eq(decoded.pads[2].x, 2.54, 0.0001));
    }

    #[test]
    fn binary_roundtrip_through_hole_pad() {
        let mut original = Footprint::new("ROUNDTRIP_TH");
        original.add_pad(Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8));

        let data = writer::encode_data_stream(&original);
        let mut decoded = Footprint::new("ROUNDTRIP_TH");
        reader::parse_data_stream(&mut decoded, &data);

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

        let data = writer::encode_data_stream(&original);
        let mut decoded = Footprint::new("ROUNDTRIP_LAYERS");
        reader::parse_data_stream(&mut decoded, &data);

        assert_eq!(decoded.tracks.len(), 3);
        assert_eq!(decoded.tracks[0].layer, Layer::TopAssembly);
        assert_eq!(decoded.tracks[1].layer, Layer::TopCourtyard);
        assert_eq!(decoded.tracks[2].layer, Layer::Top3DBody);
    }
}
