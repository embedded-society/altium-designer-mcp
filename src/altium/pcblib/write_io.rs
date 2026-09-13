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
        // An empty name has no storage name to derive — the root storage
        // would be "created" twice and the save fail half-way — so refuse it
        // before touching the file.
        if let Some(i) = self.footprints.iter().position(|f| f.name.is_empty()) {
            return Err(AltiumError::InvalidParameter {
                name: "name".to_string(),
                message: format!("footprint {i} has an empty name"),
            });
        }
        for footprint in &self.footprints {
            footprint
                .check_record_text()
                .map_err(|message| AltiumError::InvalidParameter {
                    name: "text".to_string(),
                    message,
                })?;
        }

        // Convert model_3d references to ComponentBody + EmbeddedModel before writing
        self.prepare_3d_models_for_writing()?;

        let mut cfb = crate::altium::create_ole(writer)?;

        // Storage names use the on-wire form, so the CFB name and every place the
        // name is written stay consistent for a non-Windows-1252 footprint.
        let wire_names: Vec<String> = self
            .footprints
            .iter()
            .map(|f| crate::altium::to_wire_text(&f.name))
            .collect();
        // Each footprint keeps the storage it was read from; one built from
        // scratch, renamed or copied gets a name derived by Altium's rule.
        let ole_names = crate::altium::resolve_storage_names(
            &wire_names
                .iter()
                .zip(self.footprints.iter())
                .map(|(wire, fp)| (wire.clone(), fp.storage_name.clone()))
                .collect::<Vec<_>>(),
        );

        // Write FileHeader (pipe-delimited format for reader compatibility)
        self.write_file_header(&mut cfb)?;

        // Write Library storage (Header + Data for Altium compatibility). Its
        // component list carries every footprint's real (wire) name, as
        // Altium writes it: the storage name is derived from it on read, or
        // mapped through SectionKeys when the name reaches the cap.
        self.write_library(&mut cfb, &wire_names)?;

        // Root SectionKeys stream, in the binary layout Altium reads (#507):
        // the LibRef -> storage-name map for every footprint whose name
        // reaches the 31-unit storage cap — truncated, or merely filling it
        // (an AD21-authored library lists a 31-character name as an identity
        // pair). A shorter name is not listed even when its storage differs:
        // Altium re-derives that storage from the name itself. The real name
        // still travels in the footprint's own PATTERN parameter and in
        // Library/Data. Not written when no name reaches the cap — which
        // includes the whole golden.
        let truncated: Vec<(String, String)> = wire_names
            .iter()
            .zip(ole_names.iter())
            .filter(|(wire, _)| wire.encode_utf16().count() >= crate::altium::MAX_OLE_NAME_LEN)
            .map(|(wire, ole)| (wire.clone(), crate::altium::section_key_name(wire, ole)))
            .collect();
        if let Some(section_keys) = crate::altium::encode_pcblib_section_keys(&truncated)? {
            crate::altium::write_stream(&mut cfb, "/SectionKeys", &section_keys)?;
        }

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
                // (from a prior read) or it does not point at a file — keep the
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
                // orphans, which would bloat the library on every save.
                (true, true, true) => {
                    Self::drop_stale_bodies(
                        &footprint.name.clone(),
                        &mut footprint.component_bodies,
                        &mut self.models,
                    );
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
                identifier: String::new(),
                texture_center_x: None,
                texture_center_y: None,
                texture_size_x: None,
                texture_size_y: None,
                texture_rotation: None,
                raw_layer_id: None,
                v7_layer: None,
                model_name: filename,
                embedded: true,
                rotation_x: 0.0,
                rotation_y: 0.0,
                rotation_z: model_3d.rotation,
                z_offset: model_3d.z_offset,
                overall_height: 0.0, // Could be calculated from STEP, but not implemented
                standoff_height: 0.0,
                cavity_height: 0.0,
                layer: Layer::Top3DBody,
                outline: Vec::new(), // Synthesised from the footprint extent on write
                unique_id: None,
                guid: None,
                model_checksum: 0, // Fresh embed; Altium computes the real checksum on save.
                name: " ".to_string(),
                kind: 0,
                sub_poly_index: -1,
                union_index: 0,
                is_shape_based: false,
                body_projection: 0,
                body_color_3d: 8_421_504,
                body_opacity_3d: 1.0,
                model_2d_rotation: 0.0,
                model_2d_x: 0.0,
                model_2d_y: 0.0,
                net_index: 0xFFFF,
                polygon_index: 0xFFFF,
                component_index: -1,
                additional_parameters: Vec::new(),
                param_key_order: Vec::new(),
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
    /// Drops a footprint's embedded bodies and the models they referenced,
    /// ahead of a re-embed from a new explicit path — otherwise the old models
    /// linger in `self.models` as orphans and bloat the library on every save.
    fn drop_stale_bodies(
        name: &str,
        bodies: &mut Vec<ComponentBody>,
        models: &mut Vec<EmbeddedModel>,
    ) {
        tracing::debug!(
            footprint = %name,
            old_bodies = bodies.len(),
            "Re-embedding model_3d from new explicit path"
        );
        let stale: std::collections::HashSet<String> = bodies
            .iter()
            .filter(|cb| cb.embedded)
            .map(|cb| cb.model_id.to_lowercase())
            .collect();
        bodies.clear();
        models.retain(|m| !stale.contains(&m.id.to_lowercase()));
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
    /// The canonical `PcbLib` `FileHeader` is **53 bytes** with THREE fields
    /// (matching `AltiumSharp`'s `PcbLibWriter.WriteFileHeader`); Altium rejects
    /// the file if the version double or the `UniqueId` block are missing:
    /// ```text
    /// [len:4 LE u32][len:1 u8]["PCB 6.0 Binary Library File"]  // version string (4+1+27 = 32 bytes)
    /// [5.01 : 8 f64]                                            // format version double
    /// [len:4 LE u32][len:1 u8][8-char UniqueId]                // 4+1+8 = 13 bytes
    /// ```
    /// The version string's 4-byte and 1-byte lengths are the same value (27).
    /// Component metadata is stored in `/Library/Data`, not here.
    #[allow(clippy::unused_self)]
    fn write_file_header<F: std::io::Read + std::io::Write + std::io::Seek>(
        &self,
        cfb: &mut cfb::CompoundFile<F>,
    ) -> AltiumResult<()> {
        // The canonical PcbLib FileHeader is 53 bytes with THREE fields (matching
        // AltiumSharp PcbLibWriter.WriteFileHeader). Altium Designer rejects the
        // file if the 5.01 version double and the UniqueId block are missing.
        let version_string = b"PCB 6.0 Binary Library File";
        #[allow(clippy::cast_possible_truncation)]
        let len = version_string.len() as u32;

        // The library keeps the UniqueId it was read with; one built from
        // scratch is given its first here.
        let unique_id = self
            .metadata
            .unique_id
            .clone()
            .unwrap_or_else(crate::util::generate_unique_id);
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
        let params = self.library_params();
        let mut data = Vec::new();
        crate::altium::framing::write_cstring_param_block(&mut data, &params);

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

        // PadViaLibrary: empty cache under the library id it was read with
        // (a fresh one for a library built from scratch).
        let library_id = self
            .metadata
            .pad_via_library_id
            .clone()
            .unwrap_or_else(|| format!("{{{}}}", Uuid::new_v4().to_string().to_uppercase()));
        let pvl = Self::param_block(&format!(
            "|PADVIALIBRARY.LIBRARYID={library_id}|PADVIALIBRARY.LIBRARYNAME=<Local>|PADVIALIBRARY.DISPLAYUNITS=1"
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

    /// The encoded parameter block for `/Library/Data`.
    ///
    /// Format: `|KEY=VAL|KEY=VAL|...` (leading pipe, NO trailing pipe).
    ///
    /// A library read from a file replays its own block, so the stack a
    /// designer configured — custom mechanical layer names, which layers are
    /// enabled, the layer sets, the view state — comes back out unchanged.
    /// Only the three keys that describe *this* save are rewritten. A library
    /// built in memory has no stack to preserve and gets the template one.
    fn library_params(&self) -> Vec<u8> {
        // A library path is as free to leave Windows-1252 as any other string
        // Altium stores, so it goes on the wire the same way.
        let filename = crate::altium::to_wire_text(self.filepath.as_deref().unwrap_or(""));
        let now = chrono::Local::now();
        let (date, time) = (
            now.format("%d. %m. %Y").to_string(),
            now.format("%H:%M:%S").to_string(),
        );

        if let Some(stored) = self.metadata.library_params.as_deref() {
            let block = Self::override_param(stored, "FILENAME", &filename);
            let block = Self::override_param(&block, "DATE", &date);
            return Self::override_param(&block, "TIME", &time);
        }

        crate::altium::encode_windows1252(&Self::template_library_params(&filename, &date, &time))
    }

    /// Replaces one key's value inside a raw `|KEY=VALUE|` block, leaving every
    /// other byte exactly as it was.
    ///
    /// Works on bytes rather than text because the block is replayed verbatim:
    /// decoding and re-encoding it would round a handful of Windows-1252 byte
    /// values through `?`. A key Altium did not write is appended.
    fn override_param(block: &[u8], key: &str, value: &str) -> Vec<u8> {
        let value = crate::altium::encode_windows1252(value);
        let mut replaced = false;

        let mut segments: Vec<Vec<u8>> = block
            .split(|&b| b == b'|')
            .map(|seg| {
                let matches_key = seg
                    .iter()
                    .position(|&b| b == b'=')
                    .is_some_and(|eq| seg[..eq].eq_ignore_ascii_case(key.as_bytes()));
                if !matches_key {
                    return seg.to_vec();
                }
                replaced = true;
                let mut out = key.as_bytes().to_vec();
                out.push(b'=');
                out.extend_from_slice(&value);
                out
            })
            .collect();

        if !replaced {
            let mut appended = key.as_bytes().to_vec();
            appended.push(b'=');
            appended.extend_from_slice(&value);
            segments.push(appended);
        }

        segments.join(&b'|')
    }

    /// The parameter block a from-scratch library gets.
    ///
    /// Altium Designer requires `VERSION=3.00` plus a V9 layer stack definition
    /// to consider the file valid.
    fn template_library_params(filename: &str, date: &str, time: &str) -> String {
        use std::fmt::Write;

        let mut p = String::with_capacity(4096);

        // Core metadata (must be first, matching AltiumSharp order)
        let _ = write!(p, "|FILENAME={filename}");
        p.push_str("|KIND=Protel_Advanced_PCB_Library");
        p.push_str("|VERSION=3.00");
        let _ = write!(p, "|DATE={date}");
        let _ = write!(p, "|TIME={time}");

        // V9 layer stack + full board configuration. A synthesised stack is
        // rejected by Altium ("Catastrophic failure whilst loading section
        // Library"), so we splice in a complete, known-good stack captured
        // verbatim from a real Altium-authored library.
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
            crate::altium::to_wire_text(&footprint.name),
            footprint.description
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

        // PrimitiveGuids: Altium's stable per-primitive identity. Re-emitted only
        // when the footprint was read from a file that had one — a from-scratch
        // footprint has no identities to preserve, and inventing them would make
        // every save produce different bytes.
        if let Some(guid_data) = writer::encode_primitive_guids(footprint) {
            let guid_storage = format!("{storage_path}/PrimitiveGuids");
            crate::altium::create_storage(cfb, &guid_storage)?;
            // Header = record count; the data is fixed 24-byte records, so the
            // count is exactly what was emitted.
            let count = u32::try_from(guid_data.len() / 24).unwrap_or(u32::MAX);
            crate::altium::write_stream(
                cfb,
                &format!("{guid_storage}/Header"),
                &count.to_le_bytes(),
            )?;
            crate::altium::write_stream(cfb, &format!("{guid_storage}/Data"), &guid_data)?;
        }

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

#[cfg(test)]
mod tests {
    use crate::altium::pcblib::{Footprint, Pad, PcbLib};

    fn temp_dir() -> tempfile::TempDir {
        std::fs::create_dir_all(".tmp").expect("create .tmp");
        let root = std::path::Path::new(".tmp")
            .canonicalize()
            .expect("canonicalise .tmp");
        tempfile::tempdir_in(root).expect("create temp dir")
    }

    /// A footprint keeps the storage it was read from, however its name
    /// would derive today; a carried name that is already taken falls back to
    /// a derived one; a name with `*` derives Altium's `_` form (#507).
    #[test]
    fn a_carried_storage_name_is_kept_and_a_conflicting_one_is_derived() {
        let dir = temp_dir();
        let path = dir.path().join("Carried.PcbLib");
        let mut lib = PcbLib::new();
        for (name, carried) in [
            ("EC6*5.4", Some("LEGACY_STORAGE")),
            ("OTHER", Some("LEGACY_STORAGE")),
            ("EC10*10.5", None),
        ] {
            let mut fp = Footprint::new(name);
            fp.storage_name = carried.map(str::to_string);
            fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
            lib.add(fp);
        }
        lib.save(&path).expect("save");

        let cfb = cfb::open(&path).expect("open compound document");
        for storage in ["/LEGACY_STORAGE", "/OTHER", "/EC10_10.5"] {
            assert!(cfb.is_storage(storage), "{storage}");
        }
        assert!(
            !cfb.is_storage("/EC10*10.5"),
            "the star never reaches a storage name"
        );
        drop(cfb);

        let back = PcbLib::open(&path).expect("reopen");
        assert_eq!(back.names(), vec!["EC6*5.4", "OTHER", "EC10*10.5"]);
        assert_eq!(
            back.get("EC6*5.4").unwrap().storage_name.as_deref(),
            Some("LEGACY_STORAGE")
        );
        assert_eq!(
            back.get("EC10*10.5").unwrap().storage_name.as_deref(),
            Some("EC10_10.5")
        );
    }

    /// `SectionKeys` lists a name that reaches the 31-unit cap — as an
    /// identity pair when it merely fills it — and nothing shorter, even a
    /// sanitised name whose storage differs: Altium re-derives that one.
    #[test]
    fn only_names_at_the_storage_cap_are_listed_in_section_keys() {
        let dir = temp_dir();
        let path = dir.path().join("Listing.PcbLib");
        let exactly_31 = "SOP-8_L7.5-W5.9-P1.27-LS11.5-BL";
        assert_eq!(exactly_31.len(), 31);
        let mut lib = PcbLib::new();
        for name in [exactly_31, "EC6*5.4", "SHORT"] {
            let mut fp = Footprint::new(name);
            fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
            lib.add(fp);
        }
        lib.save(&path).expect("save");

        let mut cfb = cfb::open(&path).expect("open compound document");
        assert!(cfb.is_storage("/EC6_5.4"));
        let keys = crate::altium::read_stream_opt(&mut cfb, "/SectionKeys").expect("SectionKeys");
        assert_eq!(
            crate::altium::parse_pcblib_section_keys(&keys),
            vec![(exactly_31.to_string(), exactly_31.to_string())]
        );
    }

    /// A footprint past the 31-unit storage cap is stored truncated and
    /// listed in a root `SectionKeys` stream in Altium's binary layout, and
    /// the library reads back in its authored order with the real names
    /// (#507: the text layout of a `SchLib` here left Altium unable to
    /// resolve the mapped footprints).
    #[test]
    fn a_truncated_name_is_mapped_in_a_binary_section_keys_stream() {
        let dir = temp_dir();
        let path = dir.path().join("Truncated.PcbLib");
        let long = "GENERIC_MLCC_CAP_0402_IPC_MEDIUM_DENSITY";
        let mut lib = PcbLib::new();
        for name in [long, "SHORT", "GENERIC_MLCC_CAP_0603_IPC_MEDIUM_DENSITY"] {
            let mut fp = Footprint::new(name);
            fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
            lib.add(fp);
        }
        lib.save(&path).expect("save");

        let mut cfb = cfb::open(&path).expect("open compound document");
        assert!(cfb.is_storage("/GENERIC_MLCC_CAP_0402_IPC_MEDIU"));
        let keys = crate::altium::read_stream_opt(&mut cfb, "/SectionKeys").expect("SectionKeys");
        let mut expected = 2u32.to_le_bytes().to_vec();
        for (real, storage) in [
            (long, "GENERIC_MLCC_CAP_0402_IPC_MEDIU"),
            (
                "GENERIC_MLCC_CAP_0603_IPC_MEDIUM_DENSITY",
                "GENERIC_MLCC_CAP_0603_IPC_MEDIU",
            ),
        ] {
            crate::altium::framing::write_string_block(&mut expected, real.as_bytes());
            crate::altium::framing::write_string_block(&mut expected, storage.as_bytes());
        }
        assert_eq!(keys, expected);
        drop(cfb);

        let back = PcbLib::open(&path).expect("reopen");
        assert_eq!(
            back.names(),
            vec![
                long.to_string(),
                "SHORT".to_string(),
                "GENERIC_MLCC_CAP_0603_IPC_MEDIUM_DENSITY".to_string()
            ]
        );
    }

    #[test]
    fn overriding_a_parameter_leaves_every_other_byte_alone() {
        // The block is replayed verbatim, so a rewrite must touch only the one
        // key. Matching is case-insensitive because Altium's own casing varies.
        let block = b"|VERSION=3.00|UNITS=mm|DATE=2020-01-01|";
        let out = PcbLib::override_param(block, "DATE", "2026-08-16");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "|VERSION=3.00|UNITS=mm|DATE=2026-08-16|"
        );

        let out = PcbLib::override_param(block, "version", "4.00");
        assert!(
            String::from_utf8(out).unwrap().contains("version=4.00"),
            "a differently-cased key must still be replaced, not appended"
        );
    }

    #[test]
    fn a_key_altium_did_not_write_is_appended_rather_than_dropped() {
        // Silently dropping it would write a library missing the parameter the
        // caller asked for, with no error to show for it.
        let out = PcbLib::override_param(b"|VERSION=3.00|", "UNITS", "mm");
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("|VERSION=3.00|"), "{text}");
        assert!(text.ends_with("UNITS=mm"), "{text}");
    }
}
