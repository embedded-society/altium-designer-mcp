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
        // Storage names use the on-wire form — a name Windows-1252 cannot
        // hold becomes its UTF-8 bytes one char per byte — which is the form
        // the reader looks a header entry up by, so every symbol keeps its
        // place in the library on a read-modify-write (an ASCII-keyed rule
        // sent every Latin-1 name to the end of the list on the next read).
        let storage_names: Vec<String> = symbols
            .iter()
            .map(|s| crate::altium::to_wire_text(&s.name))
            .collect();
        // Each symbol keeps the storage it was read from; one built from
        // scratch, renamed or copied gets a name derived by Altium's rule.
        let ole_names = crate::altium::resolve_storage_names(
            &storage_names
                .iter()
                .zip(symbols.iter())
                .map(|(wire, s)| (wire.clone(), s.storage_name.clone()))
                .collect::<Vec<_>>(),
        );

        // FileHeader stream. The library keeps the UniqueID it was read
        // with; one built from scratch is given its first here.
        let unique_id = self
            .unique_id
            .clone()
            .unwrap_or_else(crate::util::generate_unique_id);
        crate::altium::write_stream(
            &mut cfb,
            "/FileHeader",
            &writer::encode_file_header(&symbols, &ole_names, &unique_id),
        )?;

        // Root SectionKeys stream: the LibRef -> storage-name map for every
        // symbol whose name reaches the storage cap — truncated or, as a
        // UI-authored `Generic Non-polarised Capacitor` (31 units exactly)
        // shows, merely filling it — or whose storage Altium's rule rewrites
        // (a forbidden character, or a `~NNN` suffix for a case-duplicate),
        // so the real name stays recoverable by Altium and by our own
        // reader's ordering pass. The decision is made on the key the entry
        // would record: a storage carried from a file authored on another
        // locale differs from the wire name yet records the wire bytes, so it
        // is not listed (the golden lists five). With no such name the stream
        // is not written, as in Altium.
        let truncated: Vec<(String, String)> = storage_names
            .iter()
            .zip(ole_names.iter())
            .filter(|(wire, ole)| {
                crate::altium::section_key_name(wire, ole) != wire.as_str()
                    || wire.encode_utf16().count() >= crate::altium::MAX_OLE_NAME_LEN
            })
            .map(|(wire, ole)| (wire.clone(), crate::altium::section_key_name(wire, ole)))
            .collect();
        if let Some(section_keys) = crate::altium::encode_schlib_section_keys(&truncated) {
            crate::altium::write_stream(&mut cfb, "/SectionKeys", &section_keys)?;
        }

        // One Data stream per symbol, under its own storage.
        for (symbol, ole_name) in symbols.iter().zip(ole_names.iter()) {
            crate::altium::create_storage(&mut cfb, &format!("/{ole_name}"))?;
            let data = writer::encode_data_stream(symbol)?;
            crate::altium::write_stream(&mut cfb, &format!("/{ole_name}/Data"), &data)?;

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

        // Root Storage stream (Altium's icon storage). Always present. EVERY
        // image with `embed_image` contributes exactly one compressed entry,
        // named with the image's `file_name` (real AD24 stores the full source
        // file path there; the reader matches by order, not name). An embedded
        // image without carried bytes emits an EMPTY entry rather than being
        // skipped: the reader assigns payloads to `EmbedImage=T` images purely
        // by ordinal, so skipping would shift every later payload onto the
        // wrong image (including across symbols). With no embedded images the
        // stream is just the header param block — byte-identical to the
        // pre-embedded-image output.
        let entries: Vec<(&str, &[u8])> = symbols
            .iter()
            .flat_map(|s| s.images.iter())
            .filter(|i| i.embed_image)
            .map(|i| {
                (
                    i.file_name.as_str(),
                    i.image_data.as_deref().unwrap_or_default(),
                )
            })
            .collect();
        let storage_stream = storage::encode_icon_storage(&entries)?;
        crate::altium::write_stream(&mut cfb, "/Storage", &storage_stream)?;

        cfb.flush()
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to flush OLE file: {e}")))?;

        Ok(())
    }
}
