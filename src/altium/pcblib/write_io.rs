//! `PcbLib` write/serialisation path: the `impl PcbLib` methods (incl. the
//! public `write` entry) that serialise a library to an OLE compound document.
//! Split out of `mod.rs` for navigability; same `impl PcbLib`.

use super::{
    writer, AltiumError, AltiumResult, ComponentBody, EmbeddedModel, Footprint, Layer, PcbLib,
};

impl PcbLib {
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
            let Some(ref model_3d) = footprint.model_3d else {
                continue;
            };
            let path = std::path::Path::new(&model_3d.filepath);

            // Only ever surface the bare file name in logs/errors, never the
            // caller's full path (sanitisation rule).
            let display_name = path.file_name().map_or_else(
                || "<model>".to_string(),
                |n| n.to_string_lossy().into_owned(),
            );

            // The three conditions that decide what to do with this model_3d.
            let has_bodies = !footprint.component_bodies.is_empty();
            // An explicit path has directory components; a bare model name (set
            // when reading a ComponentBody back) does not.
            let is_explicit_path = path.parent().is_some_and(|p| !p.as_os_str().is_empty());
            let file_present = path.exists() && path.is_file();

            match (has_bodies, is_explicit_path, file_present) {
                // New footprint whose STEP file is missing — the path is required.
                (false, _, false) => {
                    return Err(AltiumError::InvalidParameter {
                        name: "step_model.filepath".to_string(),
                        message: format!(
                            "STEP file not found for footprint '{}': '{}'. \
                             Provide a valid path or use embed: false for external reference.",
                            footprint.name, display_name
                        ),
                    });
                }
                // Already embedded, and either the filepath is a bare model name
                // (from a prior read) or it no longer points at a file — keep the
                // existing ComponentBody as-is.
                (true, false, _) | (true, _, false) => {
                    tracing::trace!(
                        footprint = %footprint.name,
                        model = %display_name,
                        "Skipping model_3d - already embedded or filepath not actionable"
                    );
                    continue;
                }
                // Already embedded but the user pointed at a new explicit path that
                // exists — re-embed. Drop the old ComponentBodies AND the models
                // they referenced, so the latter don't linger in self.models as
                // orphans (which previously bloated the library on every save).
                (true, true, true) => {
                    tracing::debug!(
                        footprint = %footprint.name,
                        old_bodies = footprint.component_bodies.len(),
                        "Re-embedding model_3d from new explicit path"
                    );
                    let stale: std::collections::HashSet<String> = footprint
                        .component_bodies
                        .iter()
                        .filter(|cb| cb.embedded)
                        .map(|cb| cb.model_id.to_lowercase())
                        .collect();
                    footprint.component_bodies.clear();
                    self.models
                        .retain(|m| !stale.contains(&m.id.to_lowercase()));
                }
                // Fresh embed: new footprint, file present.
                (false, _, true) => {}
            }

            // Embed: read the STEP file, then create the EmbeddedModel +
            // ComponentBody (shared by the fresh-embed and re-embed cases above).
            let step_data = std::fs::read(path).map_err(|e| AltiumError::file_read(path, e))?;
            let guid = format!("{{{}}}", Uuid::new_v4().to_string().to_uppercase());
            let filename = path.file_name().map_or_else(
                || "model.step".to_string(),
                |n| n.to_string_lossy().to_string(),
            );

            self.models
                .push(EmbeddedModel::new(&guid, &filename, step_data));
            footprint.component_bodies.push(ComponentBody {
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
            });

            tracing::debug!(
                footprint = %footprint.name,
                filepath = %model_3d.filepath,
                "Converted model_3d to ComponentBody"
            );
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
        crate::altium::framing::write_cstring_param_block(
            &mut data,
            &crate::altium::encode_windows1252(&params),
        );

        // Component count
        #[allow(clippy::cast_possible_truncation)]
        data.extend_from_slice(&(self.footprints.len() as u32).to_le_bytes());

        // Component names as WriteStringBlock: [block_len:4][str_len:1][string].
        // The read-side mirror is `read_library_data`.
        for ole_name in ole_names {
            crate::altium::framing::write_string_block(
                &mut data,
                &crate::altium::encode_windows1252(ole_name),
            );
        }

        // Write Library/Data
        crate::altium::write_stream(cfb, "/Library/Data", &data)?;

        // Write the library metadata storages Altium emits for every library.
        self.write_library_metadata(cfb)?;

        Ok(())
    }

    /// Wraps a parameter string as an Altium C-string block:
    /// `[block_len:4][text + \x00]` where `block_len` includes the terminator.
    ///
    /// Thin owned-`Vec` wrapper around the shared
    /// [`crate::altium::framing::write_cstring_param_block`] frame.
    fn param_block(text: &str) -> Vec<u8> {
        let mut v = Vec::new();
        crate::altium::framing::write_cstring_param_block(
            &mut v,
            &crate::altium::encode_windows1252(text),
        );
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

    /// The `/FileVersionInfo` payload — Altium's version-history metadata, stored
    /// as reviewable `|KEY=VALUE|` text rather than an opaque binary blob, so any
    /// change shows up in a diff (it cannot be silently swapped). The C-string
    /// block framing (`[u32 len][text][0x00]`) is re-added on write, reproducing
    /// byte-for-byte the stream Altium emits.
    pub(crate) const FVI_TEXT: &str = include_str!("assets/file_version_info.txt");

    /// Writes the root `/FileVersionInfo` storage. The payload is a fixed,
    /// library-agnostic version-history blob (see [`Self::FVI_TEXT`]).
    fn write_file_version_info<F: std::io::Read + std::io::Write + std::io::Seek>(
        cfb: &mut cfb::CompoundFile<F>,
    ) -> AltiumResult<()> {
        let mut data = Vec::new();
        crate::altium::framing::write_cstring_param_block(&mut data, Self::FVI_TEXT.as_bytes());
        Self::write_meta_storage(cfb, "/FileVersionInfo", 1, &data)
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
        crate::altium::framing::write_cstring_param_block(
            &mut params_data,
            &crate::altium::encode_windows1252(&params),
        );
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
}
