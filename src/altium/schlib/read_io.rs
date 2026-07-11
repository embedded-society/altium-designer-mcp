//! `SchLib` read/parse path: the `impl SchLib` methods (incl. the public
//! `read` entry) that decode an OLE compound document into a library. Split
//! out of `mod.rs` for navigability; same `impl SchLib`.

use cfb::CompoundFile;
use std::collections::HashMap;
use std::io::{Read, Seek};
use tracing::warn;

use super::{pin_aux, reader, storage, AltiumError, AltiumResult, SchLib, Symbol};

impl SchLib {
    /// Reads a `SchLib` from any reader implementing `Read + Seek`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be parsed.
    pub fn read<R: Read + Seek>(reader: R) -> AltiumResult<Self> {
        let mut cfb = crate::altium::open_ole(reader)?;

        let mut lib = Self::new();

        // Read FileHeader to get component list
        let header = read_file_header(&mut cfb)?;

        // Read each component
        for comp_name in header.component_names {
            let stream_path = format!("{comp_name}/Data");

            let mut stream = match cfb.open_stream(&stream_path) {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        component = %comp_name,
                        error = %e,
                        "Failed to open component stream, skipping"
                    );
                    continue;
                }
            };

            let mut data = Vec::new();
            if let Err(e) = stream.read_to_end(&mut data) {
                warn!(
                    component = %comp_name,
                    error = %e,
                    "Failed to read component data, skipping"
                );
                continue;
            }

            let mut symbol = Symbol::new(&comp_name);
            symbol.description = header
                .component_descriptions
                .get(&comp_name)
                .cloned()
                .unwrap_or_default();

            reader::parse_data_stream(&mut symbol, &data);

            // Apply the optional per-component pin auxiliary streams. They sit
            // alongside `Data` in the same storage and are keyed by pin ordinal,
            // so they must be applied AFTER the pins are parsed. Absent streams
            // (the common case, incl. the whole golden) leave the pins untouched.
            if let Some(frac) =
                crate::altium::read_stream_opt(&mut cfb, format!("{comp_name}/PinFrac"))
            {
                pin_aux::apply_pin_frac(&mut symbol.pins, &frac);
            }
            if let Some(widths) =
                crate::altium::read_stream_opt(&mut cfb, format!("{comp_name}/PinSymbolLineWidth"))
            {
                pin_aux::apply_pin_symbol_line_widths(&mut symbol.pins, &widths);
            }

            // Use the symbol's actual name (from LibReference) as the key
            // This handles long names that were truncated in the OLE storage path
            let key = symbol.name.clone();
            lib.symbols.insert(key, symbol);
        }

        // Attach embedded image bytes from the library-level `/Storage`
        // stream. Entry names are ignored: each decompressed payload is
        // matched to the next `EmbedImage=T` image in global symbol order,
        // exactly like `AltiumSharp`'s `ParseStorageImageData`. An absent or
        // header-only stream (the common case) leaves every image untouched.
        // An EMPTY payload (the writer's placeholder for a bytes-less embedded
        // image) still consumes its ordinal slot but maps back to `None`, so a
        // bytes-less image round-trips without stealing the next payload.
        if let Some(raw) = crate::altium::read_stream_opt(&mut cfb, "/Storage") {
            let mut payloads = storage::parse_icon_storage(&raw).into_iter();
            'attach: for symbol in lib.symbols.values_mut() {
                for image in symbol.images.iter_mut().filter(|i| i.embed_image) {
                    let Some(data) = payloads.next() else {
                        break 'attach;
                    };
                    image.image_data = if data.is_empty() { None } else { Some(data) };
                }
            }
        }

        Ok(lib)
    }
}

/// Parsed file header information.
struct FileHeader {
    component_names: Vec<String>,
    component_descriptions: HashMap<String, String>,
}

/// Reads the `FileHeader` stream.
///
/// # Errors
///
/// Returns an error if the file is not a valid `SchLib` (wrong file type).
fn read_file_header<R: Read + Seek>(cfb: &mut CompoundFile<R>) -> AltiumResult<FileHeader> {
    // A `SchLib` without a readable FileHeader is invalid, so map the shared
    // optional read onto a hard error.
    let data = crate::altium::read_stream_opt(&mut *cfb, "/FileHeader")
        .ok_or_else(|| AltiumError::missing_stream("FileHeader"))?;

    // Parse header: [length:4 LE][pipe-delimited key=value pairs]
    if data.len() < 4 {
        return Err(AltiumError::parse_error(0, "FileHeader too short"));
    }

    let length = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if data.len() < 4 + length {
        return Err(AltiumError::parse_error(4, "FileHeader truncated"));
    }

    // The block is a C-string; drop the trailing null terminator (and any
    // padding) before splitting so values don't carry a stray '\0'.
    let text = String::from_utf8_lossy(&data[4..4 + length]);
    let text = text.trim_end_matches('\u{0}');
    let props = crate::altium::parse_pipe_params(text);

    // Validate file type - must be a Schematic Library
    if let Some(header) = props.get("header") {
        if !header.contains("Schematic Library") {
            // Detect what type it actually is for a helpful error message
            let actual_type = if header.contains("PCB Library") {
                "PcbLib (PCB Footprint Library)"
            } else {
                header
            };
            return Err(AltiumError::wrong_file_type("SchLib", actual_type));
        }
    }

    // Get component count
    let comp_count: usize = props
        .get("compcount")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let mut component_names = Vec::with_capacity(comp_count);
    let mut component_descriptions = HashMap::new();

    for i in 0..comp_count {
        if let Some(name) = props.get(&format!("libref{i}")) {
            component_names.push(name.clone());
            if let Some(desc) = props.get(&format!("compdescr{i}")) {
                component_descriptions.insert(name.clone(), desc.clone());
            }
        }
    }

    Ok(FileHeader {
        component_names,
        component_descriptions,
    })
}
