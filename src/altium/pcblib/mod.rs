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

pub mod primitives;

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
            let mut stream = cfb
                .open_stream(&data_path)
                .map_err(|e| AltiumError::invalid_ole(format!("Failed to open Data stream: {e}")))?;
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut stream, &mut data)
                .map_err(|e| AltiumError::invalid_ole(format!("Failed to read Data stream: {e}")))?;

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
    /// This is a simplified parser - full implementation would need to handle
    /// all the binary record formats.
    fn parse_primitives(_footprint: &mut Footprint, data: &[u8]) {
        // The Data stream contains binary records
        // Each record starts with a record type byte followed by length and data
        //
        // For now, we'll implement a basic parser that can read the most common
        // primitive types. A full implementation would need extensive reverse
        // engineering of the binary format.

        let mut offset = 0;

        while offset < data.len() {
            // Need at least 1 byte for record type
            if offset >= data.len() {
                break;
            }

            let record_type = data[offset];
            offset += 1;

            // Read record length (varies by record type)
            // This is a simplified approach - actual format is more complex
            match record_type {
                0x00 => {
                    // End of records or padding
                    break;
                }
                0x02 => {
                    // Pad record (simplified)
                    // Skip for now - would need full binary format parsing
                    if offset + 4 <= data.len() {
                        let record_len =
                            u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
                                as usize;
                        offset += 4 + record_len;
                    } else {
                        break;
                    }
                }
                0x04 => {
                    // Track record (simplified)
                    if offset + 4 <= data.len() {
                        let record_len =
                            u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
                                as usize;
                        offset += 4 + record_len;
                    } else {
                        break;
                    }
                }
                _ => {
                    // Unknown record type - try to skip based on length prefix
                    if offset + 4 <= data.len() {
                        let record_len =
                            u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
                                as usize;
                        if record_len > 0 && offset + 4 + record_len <= data.len() {
                            offset += 4 + record_len;
                        } else {
                            // Invalid length, stop parsing
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        // Note: This is a stub implementation. Full primitive parsing would require
        // detailed knowledge of the binary format. For now, the footprint structure
        // is populated with basic info only.
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
    #[allow(clippy::unused_self)] // Will need self when encoding is fully implemented
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
    /// This is a simplified encoder - full implementation would need to match
    /// Altium's exact binary format for all primitive types.
    fn encode_primitives(_footprint: &Footprint) -> Vec<u8> {
        let mut data = Vec::new();

        // Note: This is a stub implementation. Encoding primitives in the exact
        // Altium binary format requires detailed reverse engineering.
        //
        // For a working implementation, we would need to:
        // 1. Encode each pad with exact binary layout (position, size, shape, layer)
        // 2. Encode each track, arc, region, text
        // 3. Include proper record headers and checksums
        //
        // For now, we'll create a minimal valid Data stream that Altium can read
        // but won't contain the actual primitive data. This allows the library
        // structure to be created, but primitives would need to be added in Altium.

        // Write a minimal header
        data.extend_from_slice(&[0u8; 4]); // Placeholder

        // TODO: Implement proper binary encoding for:
        // - Pads (record type 0x02)
        // - Tracks (record type 0x04)
        // - Arcs (record type 0x01)
        // - Regions (record type 0x0B)
        // - Text (record type 0x05)

        data
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
}
