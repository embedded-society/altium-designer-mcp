//! `PcbLib` read/parse path: the `impl PcbLib` methods (incl. the public
//! `read` entry) that decode an OLE compound document into a library. Split
//! out of `mod.rs` for navigability; same `impl PcbLib`, calls via `self`.

use super::{
    reader, AltiumError, AltiumResult, EmbeddedModel, Footprint, LibraryMetadata, Model3D, PcbLib,
    INTERNAL_OLE_ENTRIES,
};

impl PcbLib {
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
        use crate::altium::{
            bytes::read_u32_le,
            framing::{read_block, read_pascal_string},
        };

        let Some(data) = crate::altium::read_stream_opt(cfb, "/Library/Data") else {
            return;
        };

        // Skip the leading parameter block, then read the component count.
        let Some((_, mut offset)) = read_block(&data, 0) else {
            return;
        };
        let Some(comp_count) = read_u32_le(&data, offset) else {
            return;
        };
        offset += 4;
        let comp_count = comp_count as usize;

        metadata.component_count = comp_count;
        metadata.component_names.clear();

        // Read component names, each a WriteStringBlock wrapping a Pascal string:
        // [block_len:4][str_len:1][name]. Stop gracefully at the first
        // malformed/truncated entry, keeping whatever was parsed.
        for _ in 0..comp_count {
            let Some((name_block, next)) = read_block(&data, offset) else {
                break;
            };
            let (name, _) = read_pascal_string(name_block, 0);
            if !name.is_empty() {
                metadata.component_names.push(name);
            }
            offset = next;
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

        // Bound the scan by the indices actually present in the parsed Data
        // index. The Header count (`model_count`) comes straight from the
        // untrusted /Library/Models/Header stream and is uncapped, so it must
        // never drive the loop — a crafted count would otherwise force an
        // unbounded stream scan (DoS). Treat it as advisory only.
        let max_index = model_index
            .values()
            .map(|(idx, _)| idx.saturating_add(1))
            .max()
            .unwrap_or(0);
        if model_count != max_index {
            tracing::debug!(
                header_count = model_count,
                indexed = max_index,
                "Model Header count disagrees with parsed index; using the index"
            );
        }

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
            let params = crate::altium::parse_pipe_params(&text);
            // Use PATTERN as the canonical name since OLE storage names are
            // limited to 31 characters; DESCRIPTION is free text.
            if let Some(pattern) = params.get("pattern") {
                if !pattern.is_empty() {
                    footprint.name.clone_from(pattern);
                }
            }
            if let Some(description) = params.get("description") {
                footprint.description.clone_from(description);
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
}
