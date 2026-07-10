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
        // OLE-safe storage names (handles long names + collisions).
        let ole_names = crate::altium::generate_ole_names(symbols.iter().map(|s| s.name.as_str()));

        // FileHeader stream.
        crate::altium::write_stream(
            &mut cfb,
            "/FileHeader",
            &writer::encode_file_header(&symbols, &ole_names),
        )?;

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
        }

        // Root Storage stream (Altium's icon storage). Always present. Each
        // image with `embed_image` AND carried bytes contributes one
        // compressed entry, named with the image's `file_name` (real AD24
        // stores the full source file path there; the reader matches by order,
        // not name). With no embedded images the stream is just the header
        // param block — byte-identical to the pre-embedded-image output.
        let entries: Vec<(&str, &[u8])> = symbols
            .iter()
            .flat_map(|s| s.images.iter())
            .filter(|i| i.embed_image)
            .filter_map(|i| i.image_data.as_deref().map(|d| (i.file_name.as_str(), d)))
            .collect();
        let storage_stream = storage::encode_icon_storage(&entries)?;
        crate::altium::write_stream(&mut cfb, "/Storage", &storage_stream)?;

        cfb.flush()
            .map_err(|e| AltiumError::invalid_ole(format!("Failed to flush OLE file: {e}")))?;

        Ok(())
    }
}
