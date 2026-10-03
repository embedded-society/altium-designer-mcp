//! `SchLib` write/serialisation path: the `impl SchLib` methods (incl. the
//! public `write` entry) that serialise a library to an OLE compound document.
//! Split out of `mod.rs` for navigability; same `impl SchLib`.

use std::io::{Read, Seek, Write};

use super::{pin_aux, storage, writer, AltiumError, AltiumResult, SchLib, Symbol};

impl SchLib {
    /// Writes the library to any writer implementing `Read + Write + Seek`.
    ///
    /// # Errors
    ///
    /// Returns an error if the library cannot be written.
    pub fn write<W: Read + Write + Seek>(&self, writer: W) -> AltiumResult<()> {
        let mut cfb = crate::altium::create_ole(writer)?;

        let symbols: Vec<&Symbol> = self.symbols.values().collect();

        // A non-ASCII name becomes the storage name in Altium's own form: its
        // UTF-8 bytes carried one char per byte. That keeps the storage name
        // and the FileHeader's `LibRef{i}` consistent, because the header is
        // a Windows-1252 block and encoding this form back yields exactly those
        // UTF-8 bytes — which is what Altium's library browser reads. The gate
        // is ASCII to match `text_field`: the golden stores `Résistance` this
        // way despite `é` having a single-byte form, so the storage name and
        // the record's `LibReference` stay the same bytes.
        for symbol in &symbols {
            symbol
                .check_record_text()
                .map_err(|message| AltiumError::InvalidParameter {
                    name: "text".to_string(),
                    message,
                })?;
        }
        if let Some(i) = symbols.iter().position(|s| s.name.is_empty()) {
            return Err(AltiumError::InvalidParameter {
                name: "name".to_string(),
                message: format!("symbol {i} has an empty name"),
            });
        }
        let ole_names = Self::storage_plan(&symbols);

        // FileHeader stream. The library keeps the UniqueID it was read
        // with; one built from scratch is given its first here.
        let unique_id = self
            .unique_id
            .clone()
            .unwrap_or_else(crate::util::generate_unique_id);
        // Every symbol's Data stream, encoded first: the header's Weight is
        // their record count plus one.
        let datas = symbols
            .iter()
            .map(|symbol| writer::encode_data_stream(symbol))
            .collect::<AltiumResult<Vec<_>>>()?;
        let weight = datas
            .iter()
            .map(|d| writer::count_records(d))
            .sum::<usize>()
            + 1;
        crate::altium::write_stream(
            &mut cfb,
            "/FileHeader",
            &writer::encode_file_header(
                &symbols,
                &unique_id,
                weight,
                &self.file_header,
                &self.file_header_list_basis,
            ),
        )?;

        // Root SectionKeys stream: the LibRef -> storage-name map for every
        // symbol whose name reaches the storage cap — truncated or, as a
        // UI-authored `Generic Non-polarised Capacitor` (31 units exactly)
        // shows, merely filling it — or whose storage is not its name (a
        // forbidden character, a `~NNN` suffix for a case-duplicate, a
        // storage carried from a script's library), so the real name stays
        // recoverable by Altium and by our own reader's ordering pass. With
        // no such name the stream is not written, as in Altium.
        //
        // Altium's own stream goes back as read while the symbols still build
        // the stream they were read with (see `SchLib::section_keys_read`).
        let built = Self::section_keys_stream(&symbols, &ole_names);
        let section_keys = if built == self.section_keys_basis {
            self.section_keys_read.clone()
        } else {
            built
        };
        if let Some(section_keys) = section_keys {
            crate::altium::write_stream(&mut cfb, "/SectionKeys", &section_keys)?;
        }

        // One Data stream per symbol, under its own storage.
        for ((symbol, ole_name), data) in symbols.iter().zip(ole_names.iter()).zip(&datas) {
            crate::altium::create_storage(&mut cfb, &format!("/{ole_name}"))?;
            crate::altium::write_stream(&mut cfb, &format!("/{ole_name}/Data"), data)?;

            // Optional per-component pin auxiliary streams, written into the same
            // storage. Each is emitted ONLY when at least one pin carries a
            // non-default value; an all-default symbol (the common case, incl.
            // the golden) writes neither, keeping its storage byte-identical.
            if let Some(frac) = pin_aux::encode_pin_frac(&symbol.pins)? {
                crate::altium::write_stream(&mut cfb, &format!("/{ole_name}/PinFrac"), &frac)?;
            }
            if let Some(widths) = pin_aux::encode_pin_symbol_line_widths(&symbol.pins)? {
                crate::altium::write_stream(
                    &mut cfb,
                    &format!("/{ole_name}/PinSymbolLineWidth"),
                    &widths,
                )?;
            }
            if let Some(wide) = pin_aux::encode_pin_wide_text(&symbol.pins)? {
                crate::altium::write_stream(&mut cfb, &format!("/{ole_name}/PinWideText"), &wide)?;
            }
            // Streams read but not understood go back as they were.
            for (name, bytes) in &symbol.extra_streams {
                crate::altium::write_stream(&mut cfb, &format!("/{ole_name}/{name}"), bytes)?;
            }
        }

        // Root Storage stream (Altium's icon storage).
        crate::altium::write_stream(&mut cfb, "/Storage", &Self::storage_stream(&symbols)?)?;

        cfb.flush()
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to flush OLE file: {e}")))?;

        Ok(())
    }

    /// The storage each symbol is written under.
    ///
    /// Each symbol keeps the storage it was read from. One built from scratch,
    /// renamed or copied gets the storage Altium derives from its name: the
    /// name itself, real Unicode included (`manual/i18n5.SchLib`), within the
    /// 31-unit cap — Altium finds a symbol's `PinWideText` there, under that
    /// name — and past the cap its ANSI form cut at 31 with every `?` as `_`,
    /// mapped by `SectionKeys`: AD24 looks for a long Cyrillic name's storage
    /// as 31 underscores, where a `PcbLib` keeps the `?`s.
    pub(super) fn storage_plan(symbols: &[&Symbol]) -> Vec<String> {
        let encoding = crate::altium::current_ansi_encoding();
        crate::altium::resolve_storage_names(
            &symbols
                .iter()
                .map(|s| {
                    let seed =
                        if crate::altium::utf16_len(&s.name) <= crate::altium::MAX_OLE_NAME_LEN {
                            s.name.clone()
                        } else {
                            crate::altium::ansi_cut_storage_name(
                                &crate::altium::to_ansi_wire_text(&s.name, encoding),
                                encoding,
                            )
                            .replace('?', "_")
                        };
                    (seed, s.storage_name.clone())
                })
                .collect::<Vec<_>>(),
        )
    }

    /// The root `/SectionKeys` stream this crate builds, or `None` when no
    /// symbol needs an entry (see `write` for the rule).
    pub(super) fn section_keys_stream(
        symbols: &[&Symbol],
        ole_names: &[String],
    ) -> Option<Vec<u8>> {
        let entries: Vec<(String, String)> = symbols
            .iter()
            .zip(ole_names)
            .filter(|(s, ole)| {
                crate::altium::utf16_len(&s.name) >= crate::altium::MAX_OLE_NAME_LEN
                    || **ole != s.name
            })
            .map(|(s, ole)| (s.name.clone(), ole.clone()))
            .collect();
        crate::altium::encode_schlib_section_keys(&entries)
    }

    /// The root `/Storage` stream this crate builds (Altium's icon storage).
    ///
    /// EVERY image with `embed_image` contributes exactly one compressed
    /// entry, named with the image's `file_name` (real AD24 stores the full
    /// source file path there; the reader matches by order, not name). An
    /// embedded image without carried bytes emits an EMPTY entry rather than
    /// being skipped: the reader assigns payloads to `EmbedImage=T` images
    /// purely by ordinal, so skipping would shift every later payload onto the
    /// wrong image (including across symbols). An image's compressed bytes as
    /// read go back while they still hold it (`Image::image_compressed`).
    /// With no embedded images the stream is just the header param block.
    fn storage_stream(symbols: &[&Symbol]) -> AltiumResult<Vec<u8>> {
        let entries: Vec<storage::IconEntry<'_>> = symbols
            .iter()
            .flat_map(|s| s.images.iter())
            .filter(|i| i.embed_image)
            .map(|i| {
                (
                    i.file_name.as_str(),
                    i.image_data.as_deref().unwrap_or_default(),
                    i.image_compressed.as_deref(),
                )
            })
            .collect();
        storage::encode_icon_storage(&entries)
    }
}
