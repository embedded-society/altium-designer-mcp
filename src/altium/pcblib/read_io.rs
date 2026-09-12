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
                    match Self::read_footprint(&mut cfb, &entry_path, &component_name) {
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

        // Order the footprints as Library/Data (or a legacy FileHeader) lists
        // them, so list_components follows the authored order and the one
        // reorder_components wrote. The list names a footprint by its real
        // name; its storage is that name as-is, its wire form (a non-1252 name
        // is stored under its UTF-8 bytes), the truncated name the root
        // SectionKeys stream maps it to, or — with no such stream — the name
        // cut at the storage cap by Altium's own rule.
        let section_keys: std::collections::HashMap<String, String> =
            crate::altium::read_stream_opt(&mut cfb, "/SectionKeys")
                .map(|data| {
                    crate::altium::parse_pcblib_section_keys(&data)
                        .into_iter()
                        .collect()
                })
                .unwrap_or_default();
        let no_names = std::collections::HashSet::new();
        for name in &library.metadata.component_names {
            let wire = crate::altium::to_wire_text(name);
            let storage = [name.clone(), wire.clone()]
                .into_iter()
                .chain(section_keys.get(&wire).cloned())
                .chain(std::iter::once(crate::altium::generate_ole_name(
                    &wire, &no_names,
                )))
                .find(|candidate| footprints_by_ole_name.contains_key(candidate));
            if let Some(footprint) = storage.and_then(|s| footprints_by_ole_name.remove(&s)) {
                library.footprints.push(footprint);
            }
        }

        // Append any orphaned footprints (found in OLE but not listed) in a
        // stable order, so a corrupted list or a hand-edited file still reads
        // the same way twice.
        let mut orphans: Vec<_> = footprints_by_ole_name.into_iter().collect();
        orphans.sort_by(|a, b| a.0.cmp(&b.0));
        for (ole_name, footprint) in orphans {
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
                        // After the version string and the 8-byte format
                        // double: `[len:4][len:1][8-char UniqueId]`.
                        let uid = 5 + str_len + 8;
                        if let Some(block) = data.get(uid..) {
                            if block.len() >= 5 {
                                let n = usize::from(block[4]);
                                if let Some(id) = block.get(5..5 + n) {
                                    if !id.is_empty() {
                                        metadata.unique_id =
                                            Some(crate::altium::decode_windows1252(id));
                                    }
                                }
                            }
                        }
                        metadata.pad_via_library_id =
                            crate::altium::read_stream_opt(cfb, "/Library/PadViaLibrary/Data")
                                .and_then(|pvl| {
                                    let text = crate::altium::decode_windows1252(&pvl);
                                    crate::altium::parse_pipe_params(&text)
                                        .get("padvialibrary.libraryid")
                                        .cloned()
                                });
                        tracing::debug!(
                            header = %metadata.header,
                            "Parsed FileHeader (binary version string)"
                        );
                        return Ok(metadata);
                    }
                }
            }
        }

        // Fall back to pipe-delimited key=value format (legacy).
        // Altium stores these as Windows-1252, not UTF-8 (#68).
        let text = crate::altium::decode_windows1252(&data);

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

        // Keep the leading parameter block verbatim — it is the library's own
        // layer stack and board configuration, which the writer replays rather
        // than rebuilding (see `LibraryMetadata::library_params`) — then read
        // the component count that follows it.
        let Some((params, mut offset)) = read_block(&data, 0) else {
            return;
        };
        metadata.library_params = Some(params.strip_suffix(&[0x00]).unwrap_or(params).to_vec());
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

    /// Reads a component's `WideStrings` stream if present.
    ///
    /// The stream is **per component** (`/{component}/WideStrings`), matching
    /// Altium and our own writer; no `PcbLib` carries a library-wide one.
    fn read_wide_strings<F: std::io::Read + std::io::Seek>(
        cfb: &mut cfb::CompoundFile<F>,
        path: &std::path::Path,
    ) -> reader::WideStrings {
        crate::altium::read_stream_opt(cfb, path)
            .map(|data| {
                // `[block_len:4][text + \x00]` — the parser takes the payload,
                // so the binary prefix is stripped here.
                reader::parse_wide_strings(data.get(4..).unwrap_or_default())
            })
            .unwrap_or_default()
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
    ) -> AltiumResult<Footprint> {
        // The storage name carries a non-Latin name as UTF-8 bytes; recover it so
        // the footprint is keyed by its true name. The Data stream's own name block
        // overwrites this when present.
        let mut footprint =
            Footprint::new(crate::altium::from_wire_text(name).unwrap_or_else(|| name.to_string()));

        // This component's out-of-line text, read here rather than library-wide
        // because that is where Altium puts it.
        let wide_strings = Self::read_wide_strings(cfb, &storage_path.join("WideStrings"));

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

            Self::parse_primitives(&mut footprint, &data, &wide_strings);
        }

        // PrimitiveGuids holds Altium's stable per-primitive identity, keyed
        // by the primitive's ordinal among ALL the footprint's primitives in
        // Data-stream order — which right after parsing is exactly the
        // sequence `write_sequence()` yields. Each identity is attached to the
        // primitive it names (with the record's kind cross-checked, so a
        // foreign file with a shifted ordinal base mis-attaches nothing);
        // kind 85 is the footprint record's own identity.
        let guid_path = storage_path.join("PrimitiveGuids/Data");
        if let Some(guid_data) = crate::altium::read_stream_opt(cfb, &guid_path) {
            reader::apply_primitive_guids(
                &mut footprint,
                &reader::parse_primitive_guids(&guid_data),
            );
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

        // Altium stores parameter strings as Windows-1252, not UTF-8 (#68).
        let text = crate::altium::decode_windows1252(text_data);
        let params = crate::altium::parse_pipe_params(&text);
        // Use PATTERN as the canonical name since OLE storage names are
        // limited to 31 characters; DESCRIPTION is free text.
        if let Some(pattern) = params.get("pattern") {
            if !pattern.is_empty() {
                // PATTERN carries a non-Latin name as raw UTF-8 bytes.
                footprint.name =
                    crate::altium::from_wire_text(pattern).unwrap_or_else(|| pattern.clone());
            }
        }
        if let Some(description) = params.get("description") {
            footprint.description =
                crate::altium::from_wire_text(description).unwrap_or_else(|| description.clone());
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

#[cfg(test)]
mod tests {
    use super::PcbLib;
    use std::io::Write;

    /// Builds a minimal compound document carrying just a `/FileHeader`
    /// stream, so the header paths can be driven without an Altium file.
    fn library_with_header(path: &std::path::Path, header: &[u8]) {
        let mut compound = cfb::create(path).expect("create compound document");
        {
            let mut stream = compound
                .create_stream("/FileHeader")
                .expect("create FileHeader");
            stream.write_all(header).expect("write FileHeader");
        }
        compound.flush().expect("flush compound document");
    }

    fn temp_dir() -> tempfile::TempDir {
        std::fs::create_dir_all(".tmp").expect("create .tmp");
        let root = std::path::Path::new(".tmp")
            .canonicalize()
            .expect("canonicalise .tmp");
        tempfile::tempdir_in(root).expect("create temp dir")
    }

    /// Runs `body` with a TRACE-level subscriber installed on this thread.
    /// The reader reports what it found — and what it gave up on — through
    /// `tracing`, and those fields are only evaluated when a subscriber wants
    /// them, so a damaged file has to be read under one to prove the
    /// diagnostics themselves are well-formed.
    fn with_tracing<T>(body: impl FnOnce() -> T) -> T {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_test_writer()
            .finish();
        tracing::subscriber::with_default(subscriber, body)
    }

    #[test]
    fn the_legacy_pipe_delimited_header_still_parses() {
        // Older libraries carry a key=value header instead of the binary
        // version string, and the component order and descriptions live in it.
        // Losing that ordering is what `reorder_components` writes, so the
        // fallback has to keep working.
        let dir = temp_dir();
        let path = dir.path().join("Legacy.PcbLib");
        library_with_header(
            &path,
            b"|HEADER=Protel for Windows - PCB Library|COMPCOUNT=2\
              |LIBREF0=FIRST|LIBREF1=SECOND|COMPDESCR0=first part|COMPDESCR1=second part|",
        );

        let library = with_tracing(|| PcbLib::open(&path).expect("a legacy header should read"));
        assert_eq!(library.metadata.component_count, 2);
        assert_eq!(library.metadata.component_names, vec!["FIRST", "SECOND"]);
        assert_eq!(
            library.metadata.component_descriptions,
            vec!["first part", "second part"]
        );
    }

    #[test]
    fn out_of_order_indices_land_in_their_own_slots() {
        // The indices are positional, not sequential, so a header listing
        // LIBREF2 before LIBREF0 must still place each name at its own index
        // rather than in arrival order.
        let dir = temp_dir();
        let path = dir.path().join("Sparse.PcbLib");
        library_with_header(
            &path,
            b"|HEADER=Protel for Windows - PCB Library|LIBREF2=THIRD|LIBREF0=FIRST|",
        );

        let library = PcbLib::open(&path).expect("a sparse header should read");
        assert_eq!(library.metadata.component_names.len(), 3);
        assert_eq!(library.metadata.component_names[0], "FIRST");
        // The gap is filled rather than shifting the later entry down.
        assert_eq!(library.metadata.component_names[1], "");
        assert_eq!(library.metadata.component_names[2], "THIRD");
    }

    #[test]
    fn a_schematic_library_is_refused_by_name() {
        // Opening a SchLib as a PcbLib has to say which it actually is —
        // silently reading it as an empty footprint library would look like a
        // library that lost all its parts.
        let dir = temp_dir();
        let path = dir.path().join("Actually.PcbLib");
        library_with_header(&path, b"|HEADER=Protel for Windows - Schematic Library|");

        let err = PcbLib::open(&path).expect_err("a schematic library must be refused");
        let message = err.to_string();
        assert!(message.contains("SchLib"), "{message}");
    }

    #[test]
    fn an_unrecognised_header_names_itself_in_the_rejection() {
        let dir = temp_dir();
        let path = dir.path().join("Foreign.PcbLib");
        library_with_header(&path, b"|HEADER=Some Other Tool Library|");

        let err = PcbLib::open(&path).expect_err("a foreign header must be refused");
        assert!(err.to_string().contains("Some Other Tool"), "{err}");
    }

    /// Builds a library carrying one footprint storage: a `Parameters` block
    /// (the canonical name and description live there, not in the storage
    /// name), a `Data` stream, and a library-level `/Storage` stream.
    fn library_with_footprint(path: &std::path::Path, ole_name: &str, parameters: &[u8]) {
        let mut compound = cfb::create(path).expect("create compound document");
        {
            let mut stream = compound
                .create_stream("/FileHeader")
                .expect("create FileHeader");
            stream
                .write_all(
                    format!(
                        "|HEADER=Protel for Windows - PCB Library|COMPCOUNT=1|LIBREF0={ole_name}"
                    )
                    .as_bytes(),
                )
                .expect("write FileHeader");
        }
        compound
            .create_storage(format!("/{ole_name}"))
            .expect("create footprint storage");
        {
            let mut stream = compound
                .create_stream(format!("/{ole_name}/Parameters"))
                .expect("create Parameters");
            stream.write_all(parameters).expect("write Parameters");
        }
        {
            let mut stream = compound
                .create_stream(format!("/{ole_name}/Data"))
                .expect("create Data");
            let mut data = u32::try_from(ole_name.len() + 1)
                .unwrap()
                .to_le_bytes()
                .to_vec();
            data.push(u8::try_from(ole_name.len()).unwrap());
            data.extend_from_slice(ole_name.as_bytes());
            stream.write_all(&data).expect("write Data");
        }
        {
            let mut stream = compound.create_stream("/Storage").expect("create Storage");
            stream
                .write_all(b"|UNIQUEIDPRIMITIVEINFORMATION=1|")
                .expect("write Storage");
        }
        compound.flush().expect("flush compound document");
    }

    /// Library/Data lists footprints by their real names while a name past the
    /// 31-unit cap is stored truncated; the root `SectionKeys` stream — Altium's
    /// binary layout, or the text layout this crate wrote before #507 — maps
    /// one to the other, and with no stream at all the cap rule does. In every
    /// case the authored order holds and no footprint is appended as an orphan.
    #[test]
    fn truncated_storages_are_ordered_through_section_keys() {
        let long = "GENERIC_MLCC_CAP_0402_IPC_MEDIUM_DENSITY";
        let storage = "GENERIC_MLCC_CAP_0402_IPC_MEDIU";
        let pairs = vec![(long.to_string(), storage.to_string())];
        let binary = crate::altium::encode_pcblib_section_keys(&pairs)
            .unwrap()
            .unwrap();
        let text = crate::altium::encode_schlib_section_keys(&pairs).unwrap();
        let dir = temp_dir();
        for (label, keys) in [
            ("Binary", Some(binary.as_slice())),
            ("Text", Some(text.as_slice())),
            ("None", None),
        ] {
            let path = dir.path().join(format!("{label}.PcbLib"));
            library_with_ordered_footprints(&path, &[(long, storage), ("SHORT", "SHORT")], keys);
            let library = PcbLib::open(&path).expect("the library should open");
            assert_eq!(
                library.names(),
                vec![long.to_string(), "SHORT".to_string()],
                "{label}"
            );
        }
    }

    /// Builds a library whose `Library/Data` lists `(real name, storage name)`
    /// footprints in order, each storage holding a `Parameters` block naming
    /// the real name in PATTERN, plus an optional root `SectionKeys` stream.
    fn library_with_ordered_footprints(
        path: &std::path::Path,
        footprints: &[(&str, &str)],
        section_keys: Option<&[u8]>,
    ) {
        use std::io::Write as _;
        let mut compound = cfb::create(path).expect("create compound document");
        {
            let mut stream = compound
                .create_stream("/FileHeader")
                .expect("create FileHeader");
            stream
                .write_all(b"|HEADER=Protel for Windows - PCB Library|")
                .expect("write FileHeader");
        }
        compound.create_storage("/Library").expect("create Library");
        {
            let mut stream = compound
                .create_stream("/Library/Data")
                .expect("create Library/Data");
            let mut data = Vec::new();
            crate::altium::framing::write_cstring_param_block(
                &mut data,
                b"|KIND=Protel_Advanced_PCB_Library|",
            );
            data.extend_from_slice(&u32::try_from(footprints.len()).unwrap().to_le_bytes());
            for (real, _) in footprints {
                crate::altium::framing::write_string_block(&mut data, real.as_bytes());
            }
            stream.write_all(&data).expect("write Library/Data");
        }
        if let Some(keys) = section_keys {
            let mut stream = compound
                .create_stream("/SectionKeys")
                .expect("create SectionKeys");
            stream.write_all(keys).expect("write SectionKeys");
        }
        for (real, storage) in footprints {
            compound
                .create_storage(format!("/{storage}"))
                .expect("create footprint storage");
            {
                let mut stream = compound
                    .create_stream(format!("/{storage}/Parameters"))
                    .expect("create Parameters");
                let mut block = Vec::new();
                crate::altium::framing::write_cstring_param_block(
                    &mut block,
                    format!("|PATTERN={real}|DESCRIPTION=ordered").as_bytes(),
                );
                stream.write_all(&block).expect("write Parameters");
            }
            {
                let mut stream = compound
                    .create_stream(format!("/{storage}/Data"))
                    .expect("create Data");
                let mut data = Vec::new();
                crate::altium::framing::write_string_block(&mut data, storage.as_bytes());
                stream.write_all(&data).expect("write Data");
            }
        }
        compound.flush().expect("flush compound document");
    }

    #[test]
    fn the_canonical_name_comes_from_parameters_not_the_storage_name() {
        // OLE storage names are capped at 31 characters, so a longer footprint
        // name only survives in PATTERN. Taking the storage name instead would
        // silently truncate every long name in the library.
        let dir = temp_dir();
        let path = dir.path().join("Named.PcbLib");
        library_with_footprint(
            &path,
            "TRUNCATED_OLE_NAME",
            b"|PATTERN=A_MUCH_LONGER_FOOTPRINT_NAME_THAN_OLE_ALLOWS|DESCRIPTION=a described part",
        );

        let library = PcbLib::open(&path).expect("the library should open");
        assert_eq!(library.len(), 1);
        let fp = library.iter().next().expect("one footprint");
        assert_eq!(fp.name, "A_MUCH_LONGER_FOOTPRINT_NAME_THAN_OLE_ALLOWS");
        assert_eq!(fp.description, "a described part");
    }

    #[test]
    fn a_parameters_block_is_read_with_or_without_its_length_prefix() {
        // Altium writes the block length-prefixed; hand-built and older files
        // start straight at the pipe. Both have to yield the same name, or the
        // footprint loads under its truncated storage name instead.
        let dir = temp_dir();
        let bare = b"|PATTERN=BARE_FORM|DESCRIPTION=no prefix".to_vec();

        let mut prefixed = u32::try_from(bare.len()).unwrap().to_le_bytes().to_vec();
        prefixed.extend_from_slice(&bare);

        for (name, parameters, expected) in [
            ("Bare.PcbLib", bare, "BARE_FORM"),
            ("Prefixed.PcbLib", prefixed, "BARE_FORM"),
        ] {
            let path = dir.path().join(name);
            library_with_footprint(&path, "FP", &parameters);
            let library = PcbLib::open(&path).expect("the library should open");
            assert_eq!(
                library.iter().next().expect("one footprint").name,
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn a_footprint_with_no_pattern_keeps_its_storage_name() {
        // An empty or absent PATTERN is not a name; falling back to the
        // storage name is what keeps such a footprint addressable at all.
        let dir = temp_dir();
        let path = dir.path().join("NoPattern.PcbLib");
        library_with_footprint(&path, "FALLBACK", b"|PATTERN=|DESCRIPTION=empty pattern");

        let library = PcbLib::open(&path).expect("the library should open");
        assert_eq!(
            library.iter().next().expect("one footprint").name,
            "FALLBACK"
        );
    }

    #[test]
    fn a_header_stream_too_short_to_hold_a_version_string_is_not_fatal() {
        // The binary form is length-prefixed; a truncated one must fall
        // through to the key=value reader rather than slicing out of bounds.
        let dir = temp_dir();
        let path = dir.path().join("Short.PcbLib");
        library_with_header(&path, &[1, 0, 0]);

        let library = PcbLib::open(&path).expect("a short header should read as empty");
        assert!(library.metadata.header.is_empty());
        assert_eq!(library.len(), 0);
    }

    /// Wraps `payload` in the length-prefixed binary `FileHeader` framing:
    /// `[block_len:4][str_len:1][payload]`.
    fn binary_header(payload: &[u8]) -> Vec<u8> {
        let len = u8::try_from(payload.len()).unwrap();
        let mut out = u32::try_from(payload.len() + 1)
            .unwrap()
            .to_le_bytes()
            .to_vec();
        out.push(len);
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn a_well_framed_header_that_is_not_a_pcb_version_string_falls_through() {
        // The binary branch only claims a header that actually names itself a
        // PCB binary library. Anything else — including bytes that are not
        // UTF-8 at all — has to reach the key=value reader instead of being
        // taken as the header or panicking on the slice.
        let dir = temp_dir();

        let path = dir.path().join("NotPcb.PcbLib");
        library_with_header(&path, &binary_header(b"Some Other Binary Library File"));
        let library = PcbLib::open(&path).expect("a non-PCB version string should read");
        assert!(
            library.metadata.header.is_empty(),
            "{:?}",
            library.metadata.header
        );

        let path = dir.path().join("NotUtf8.PcbLib");
        library_with_header(&path, &binary_header(&[0xFF, 0xFE, 0xFD]));
        let library = PcbLib::open(&path).expect("a non-UTF-8 header should read");
        assert!(library.metadata.header.is_empty());
    }

    #[test]
    fn header_keys_with_unparsable_indices_are_skipped_not_fatal() {
        // LIBREF/COMPDESCR are positional; a key whose suffix is not a number
        // has no slot to land in. It must be dropped rather than derailing the
        // rest of the header, and an unrelated key must simply be ignored.
        let dir = temp_dir();
        let path = dir.path().join("BadIndex.PcbLib");
        library_with_header(
            &path,
            b"|HEADER=Protel for Windows - PCB Library|LIBREFX=nope|COMPDESCRY=nope\
              |SOMETHINGELSE=ignored|LIBREF0=REAL|",
        );

        let library = PcbLib::open(&path).expect("unparsable indices should not be fatal");
        assert_eq!(library.metadata.component_names, vec!["REAL"]);
        assert!(library.metadata.component_descriptions.is_empty());
    }

    /// Builds a library with a `/Library/Data` stream carrying `library_data`
    /// verbatim, so its framing can be truncated at each stage.
    fn library_with_library_data(path: &std::path::Path, library_data: &[u8]) {
        let mut compound = cfb::create(path).expect("create compound document");
        {
            let mut stream = compound
                .create_stream("/FileHeader")
                .expect("create FileHeader");
            stream
                .write_all(b"|HEADER=Protel for Windows - PCB Library|")
                .expect("write FileHeader");
        }
        compound
            .create_storage("/Library")
            .expect("create Library storage");
        {
            let mut stream = compound
                .create_stream("/Library/Data")
                .expect("create Library/Data");
            stream.write_all(library_data).expect("write Library/Data");
        }
        compound.flush().expect("flush compound document");
    }

    #[test]
    fn a_truncated_library_data_stream_stops_where_it_runs_out() {
        // Each stage of the framing — the parameter block, the component
        // count, then each name block — can be the point at which a damaged
        // file ends. Every one has to stop cleanly and keep what was parsed,
        // because the alternative is refusing to open the library at all.
        let dir = temp_dir();

        // Too short to hold even the leading parameter block's length word.
        let path = dir.path().join("NoBlock.PcbLib");
        library_with_library_data(&path, &[0, 0]);
        assert_eq!(PcbLib::open(&path).expect("should open").len(), 0);

        // Parameter block present, but the component count is cut off.
        let mut data = 4u32.to_le_bytes().to_vec();
        data.extend_from_slice(b"|X=1");
        let path = dir.path().join("NoCount.PcbLib");
        library_with_library_data(&path, &data);
        let library = PcbLib::open(&path).expect("should open");
        assert!(library.metadata.library_params.is_some());

        // Count claims two names but only one name block follows.
        let mut data = 4u32.to_le_bytes().to_vec();
        data.extend_from_slice(b"|X=1");
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&3u32.to_le_bytes());
        data.push(2);
        data.extend_from_slice(b"FP");
        let path = dir.path().join("ShortNames.PcbLib");
        library_with_library_data(&path, &data);
        let library = with_tracing(|| PcbLib::open(&path).expect("should open"));
        assert_eq!(library.metadata.component_count, 2);
        assert_eq!(library.metadata.component_names, vec!["FP"]);
    }

    /// Builds a one-footprint library carrying `parameters`, a minimal `Data`
    /// stream (the enumerator only treats a storage as a component when one is
    /// present), and a library-level `/Storage` only when `storage` is given.
    fn library_with_optional_storage(
        path: &std::path::Path,
        parameters: &[u8],
        storage: Option<&[u8]>,
    ) {
        let mut compound = cfb::create(path).expect("create compound document");
        {
            let mut stream = compound
                .create_stream("/FileHeader")
                .expect("create FileHeader");
            stream
                .write_all(b"|HEADER=Protel for Windows - PCB Library|COMPCOUNT=1|LIBREF0=FP")
                .expect("write FileHeader");
        }
        compound.create_storage("/FP").expect("create FP storage");
        {
            let mut stream = compound
                .create_stream("/FP/Parameters")
                .expect("create Parameters");
            stream.write_all(parameters).expect("write Parameters");
        }
        {
            let mut stream = compound.create_stream("/FP/Data").expect("create Data");
            stream
                .write_all(&[3, 0, 0, 0, 2, b'F', b'P'])
                .expect("write Data");
        }
        if let Some(storage) = storage {
            let mut stream = compound.create_stream("/Storage").expect("create Storage");
            stream.write_all(storage).expect("write Storage");
        }
        compound.flush().expect("flush compound document");
    }

    #[test]
    fn parameters_missing_their_keys_leave_the_footprint_defaults_alone() {
        // A block with neither PATTERN nor DESCRIPTION, and one too short to
        // even carry a length prefix, both have to leave the storage name
        // standing rather than blanking the footprint out.
        let dir = temp_dir();

        let path = dir.path().join("NoKeys.PcbLib");
        library_with_optional_storage(&path, b"|SOMETHING=else|", None);
        let library = PcbLib::open(&path).expect("the library should open");
        let fp = library.iter().next().expect("one footprint");
        assert_eq!(fp.name, "FP");
        assert!(fp.description.is_empty(), "{:?}", fp.description);

        // Shorter than the 4-byte length prefix the block may carry.
        let path = dir.path().join("Tiny.PcbLib");
        library_with_optional_storage(&path, b"|X=", None);
        let library = PcbLib::open(&path).expect("the library should open");
        assert_eq!(library.iter().next().expect("one footprint").name, "FP");
    }

    #[test]
    fn a_storage_stream_without_unique_id_entries_is_read_without_complaint() {
        // The stream is advisory: entries absent, or bytes that are not UTF-8
        // at all, must both leave the library readable.
        let dir = temp_dir();

        let path = dir.path().join("PlainStorage.PcbLib");
        library_with_optional_storage(&path, b"|PATTERN=FP|", Some(b"|NOTHING=here|"));
        assert_eq!(PcbLib::open(&path).expect("should open").len(), 1);

        let path = dir.path().join("BinaryStorage.PcbLib");
        library_with_optional_storage(&path, b"|PATTERN=FP|", Some(&[0xFF, 0xFE, 0xFD]));
        assert_eq!(PcbLib::open(&path).expect("should open").len(), 1);
    }

    #[test]
    fn a_models_header_that_disagrees_with_the_index_defers_to_the_index() {
        // The Header count comes straight from the file and is uncapped, so a
        // crafted one must not drive the stream scan — the parsed index does.
        // A stream the index names but the file lacks costs that model alone.
        let dir = temp_dir();
        let path = dir.path().join("Models.PcbLib");
        {
            let mut compound = cfb::create(&path).expect("create compound document");
            {
                let mut stream = compound
                    .create_stream("/FileHeader")
                    .expect("create FileHeader");
                stream
                    .write_all(b"|HEADER=Protel for Windows - PCB Library|")
                    .expect("write FileHeader");
            }
            compound.create_storage("/Library").expect("create Library");
            compound
                .create_storage("/Library/Models")
                .expect("create Models");
            {
                // Claims far more models than the index describes.
                let mut stream = compound
                    .create_stream("/Library/Models/Header")
                    .expect("create Models/Header");
                stream
                    .write_all(&9999u32.to_le_bytes())
                    .expect("write Models/Header");
            }
            {
                // One record, so the index bounds the scan at a single stream.
                let record = b"|ID={AAAA}|NAME=part.step|";
                let mut data = u32::try_from(record.len()).unwrap().to_le_bytes().to_vec();
                data.extend_from_slice(record);
                let mut stream = compound
                    .create_stream("/Library/Models/Data")
                    .expect("create Models/Data");
                stream.write_all(&data).expect("write Models/Data");
            }
            // Deliberately no /Library/Models/0 stream.
            compound.flush().expect("flush compound document");
        }

        let library = with_tracing(|| PcbLib::open(&path).expect("the library should open"));
        assert!(
            library.models.is_empty(),
            "a model whose stream is absent must not be invented"
        );
    }
    /// A footprint whose Data stream is garbage still reads, keyed by its
    /// storage name and carrying no primitives: the reader is deliberately
    /// lenient with a damaged component so the rest of the library survives.
    #[test]
    fn a_footprint_with_garbage_data_is_kept_by_its_storage_name() {
        use crate::altium::pcblib::{Footprint, PcbLib as Lib};
        use std::io::{Cursor, Write as _};

        let mut lib = Lib::new();
        lib.add(Footprint::new("GOOD"));
        lib.add(Footprint::new("BROKEN"));
        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("write");

        let doctored = {
            let mut compound =
                cfb::CompoundFile::open(Cursor::new(buffer.into_inner())).expect("open");
            compound.remove_stream("/BROKEN/Data").expect("drop Data");
            {
                let mut stream = compound
                    .create_stream("/BROKEN/Data")
                    .expect("recreate Data");
                // A name block whose length runs past the stream's end.
                stream.write_all(&[0xFF, 0xFF, 0xFF, 0x00]).expect("write");
            }
            compound.flush().expect("flush");
            compound.into_inner().into_inner()
        };

        let lib =
            with_tracing(|| PcbLib::read(Cursor::new(doctored)).expect("the library still reads"));
        assert!(lib.get("GOOD").is_some(), "the intact footprint is kept");
        let broken = lib.get("BROKEN").expect("kept under its storage name");
        assert!(
            broken.pads.is_empty(),
            "garbage yields no primitives, never phantom ones"
        );
    }

    /// A component storage without a Data stream is not a footprint at all:
    /// the library walk recognises components by that stream, so the husk is
    /// passed over and the read still succeeds.
    #[test]
    fn a_storage_without_a_data_stream_is_not_read_as_a_footprint() {
        use crate::altium::pcblib::{Footprint, PcbLib as Lib};
        use std::io::Cursor;

        let mut lib = Lib::new();
        let mut footprint = Footprint::new("NODATA");
        footprint.add_pad(crate::altium::pcblib::Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        lib.add(footprint);
        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("write");

        let doctored = {
            let mut compound =
                cfb::CompoundFile::open(Cursor::new(buffer.into_inner())).expect("open");
            compound.remove_stream("/NODATA/Data").expect("drop Data");
            compound.flush().expect("flush");
            compound.into_inner().into_inner()
        };

        let lib = PcbLib::read(Cursor::new(doctored)).expect("the library still reads");
        assert!(
            lib.get("NODATA").is_none(),
            "a storage with no Data stream is not a component"
        );
    }

    /// A `FileHeader` whose `UniqueId` length byte says zero yields no library
    /// unique id — an empty id is absence, not an empty string.
    #[test]
    fn a_file_header_with_a_zero_length_unique_id_reads_as_none() {
        use crate::altium::pcblib::{Footprint, PcbLib as Lib};
        use std::io::{Cursor, Read as _, Seek as _, Write as _};

        let mut lib = Lib::new();
        lib.add(Footprint::new("F"));
        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("write");

        let doctored = {
            let mut compound =
                cfb::CompoundFile::open(Cursor::new(buffer.into_inner())).expect("open");
            let mut header = Vec::new();
            {
                let mut stream = compound.open_stream("/FileHeader").expect("open header");
                stream.read_to_end(&mut header).expect("read header");
            }
            // Layout: [block_len:4][str_len:1][version][f64:8][uid_len4:4?]
            // — the unique-id block sits at 5 + str_len + 8 and starts with a
            // 4-byte length, then its own 1-byte string length. Zero that.
            let str_len = usize::from(header[4]);
            let uid_len_at = 5 + str_len + 8 + 4;
            assert_ne!(header[uid_len_at], 0, "the writer stamped a unique id");
            header[uid_len_at] = 0;
            {
                let mut stream = compound.open_stream("/FileHeader").expect("reopen header");
                stream.rewind().expect("rewind");
                stream.write_all(&header).expect("write doctored header");
            }
            compound.flush().expect("flush");
            compound.into_inner().into_inner()
        };

        let lib = PcbLib::read(Cursor::new(doctored)).expect("the library still reads");
        assert!(
            lib.metadata.unique_id.is_none(),
            "a zero-length unique id reads as None"
        );
    }
}
