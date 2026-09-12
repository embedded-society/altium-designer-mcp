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
        lib.unique_id.clone_from(&header.unique_id);

        // Components are discovered by walking the storages that actually hold a
        // `Data` stream, with the FileHeader's LibRef list used only for ordering
        // — the same approach the PcbLib reader takes.
        //
        // The header cannot be trusted for lookup: it is a Windows-1252 parameter
        // block, while a CFB storage name is UTF-16, so for a name outside that
        // code page the two carry different bytes and no header entry matches any
        // storage. Altium writes the name's raw UTF-8 bytes into the header and
        // widens those same bytes through the machine's ANSI code page for the
        // storage name, so the mapping also depends on the locale that authored
        // the file. Enumerating storages sidesteps all of it.
        let storages: Vec<String> = cfb
            .walk()
            .filter(cfb::Entry::is_storage)
            .filter_map(|e| {
                let path = e.path().to_path_buf();
                let name = path.file_name()?.to_string_lossy().to_string();
                (!name.is_empty()).then_some(name)
            })
            .filter(|name| cfb.is_stream(format!("/{name}/Data")))
            .collect();

        // Header order first (so `list_components` keeps the library's own
        // ordering), then any storage the header does not mention. A header
        // name is matched to its storage three ways, in decreasing directness:
        // as-is (ASCII names), through its wire form (a non-Windows-1252 name
        // is stored under its UTF-8 bytes one char per byte), and through the
        // root SectionKeys stream (a name past the 31-unit storage cap is
        // stored truncated, and SectionKeys is the authoritative map back).
        // An Altium file authored on a non-1252 locale can still widen its
        // storage names through a code page we cannot reconstruct; such
        // storages simply fall through to the extras pass below.
        let section_keys: std::collections::HashMap<String, String> =
            crate::altium::read_stream_opt(&mut cfb, "/SectionKeys")
                .map(|data| {
                    crate::altium::parse_schlib_section_keys(&data)
                        .into_iter()
                        .collect()
                })
                .unwrap_or_default();
        let mut ordered: Vec<String> = header
            .component_names
            .iter()
            .filter_map(|n| {
                if storages.contains(n) {
                    return Some(n.clone());
                }
                let wire = crate::altium::to_wire_text(n);
                if storages.contains(&wire) {
                    return Some(wire);
                }
                section_keys
                    .get(&wire)
                    .filter(|sk| storages.contains(*sk))
                    .cloned()
            })
            .collect();
        let extras: Vec<String> = storages
            .iter()
            .filter(|n| !ordered.contains(n))
            .cloned()
            .collect();
        ordered.extend(extras);

        for comp_name in ordered {
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

            apply_pin_aux_streams(&mut cfb, &comp_name, &mut symbol);
            carry_extra_streams(&mut cfb, &comp_name, &mut symbol);

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

/// Applies the optional per-component pin auxiliary streams. They sit
/// alongside `Data` in the same storage and are keyed by pin ordinal, so they
/// must be applied AFTER the pins are parsed. Absent streams (the common case)
/// leave the pins untouched.
fn apply_pin_aux_streams<R: Read + Seek>(
    cfb: &mut CompoundFile<R>,
    comp_name: &str,
    symbol: &mut Symbol,
) {
    if let Some(frac) = crate::altium::read_stream_opt(&mut *cfb, format!("{comp_name}/PinFrac")) {
        pin_aux::apply_pin_frac(&mut symbol.pins, &frac);
    }
    if let Some(widths) =
        crate::altium::read_stream_opt(&mut *cfb, format!("{comp_name}/PinSymbolLineWidth"))
    {
        pin_aux::apply_pin_symbol_line_widths(&mut symbol.pins, &widths);
    }
    if let Some(wide) =
        crate::altium::read_stream_opt(&mut *cfb, format!("{comp_name}/PinWideText"))
    {
        pin_aux::apply_pin_wide_text(&mut symbol.pins, &wide);
    }
}

/// The streams of the component's storage this crate does not read — a
/// `PinFunctionData` from a newer Altium — are the names beside `Data` and
/// the pin auxiliaries, kept verbatim as [`Symbol::extra_streams`].
const READ_STREAMS: &[&str] = &["Data", "PinFrac", "PinSymbolLineWidth", "PinWideText"];

/// Carries every stream of the component's storage that nothing above reads.
fn carry_extra_streams<R: Read + Seek>(
    cfb: &mut CompoundFile<R>,
    comp_name: &str,
    symbol: &mut Symbol,
) {
    let extra: Vec<String> = cfb
        .read_storage(format!("/{comp_name}"))
        .map(|entries| {
            entries
                .filter(cfb::Entry::is_stream)
                .map(|entry| entry.name().to_string())
                .filter(|name| !READ_STREAMS.contains(&name.as_str()))
                .collect()
        })
        .unwrap_or_default();
    for name in extra {
        if let Some(bytes) =
            crate::altium::read_stream_opt(&mut *cfb, format!("{comp_name}/{name}"))
        {
            symbol.extra_streams.push((name, bytes));
        }
    }
}

/// Parsed file header information.
struct FileHeader {
    component_names: Vec<String>,
    component_descriptions: HashMap<String, String>,
    unique_id: Option<String>,
}

/// Reads the `FileHeader` stream.
///
/// # Errors
///
/// Returns an error if the file is not a valid `SchLib` (wrong file type).
/// Recognises a `PcbLib`'s `/FileHeader` so it can be rejected by name rather
/// than parsed as an empty `SchLib`.
///
/// The layout is `[u32 len][u8 len]["PCB <v> Binary Library File"]`, so the
/// version string starts at offset 5. Both markers are required: `PCB ` alone
/// is short enough to appear by chance in a corrupt block, whereas the pair
/// only occurs in a genuine footprint-library header.
fn looks_like_pcblib_header(data: &[u8]) -> bool {
    const SCAN: usize = 64;
    let window = &data[..data.len().min(SCAN)];
    let contains = |needle: &[u8]| window.windows(needle.len()).any(|w| w == needle);
    contains(b"PCB ") && contains(b"Binary Library File")
}

fn read_file_header<R: Read + Seek>(cfb: &mut CompoundFile<R>) -> AltiumResult<FileHeader> {
    // A `SchLib` without a readable FileHeader is invalid, so map the shared
    // optional read onto a hard error.
    let data = crate::altium::read_stream_opt(&mut *cfb, "/FileHeader")
        .ok_or_else(|| AltiumError::missing_stream("FileHeader"))?;

    // Parse header: [length:4 LE][pipe-delimited key=value pairs]
    if data.len() < 4 {
        return Err(AltiumError::parse_error(0, "FileHeader too short"));
    }

    // A PcbLib's FileHeader is a binary version-string block, not a
    // length-prefixed pipe list, so it yields no properties at all. Detect it
    // positively rather than falling through to "zero symbols": reading a
    // footprint library as a symbol library must fail, not look empty, because
    // any append-style caller would then save an empty library over the file.
    if looks_like_pcblib_header(&data) {
        return Err(AltiumError::wrong_file_type(
            "SchLib",
            "PcbLib (PCB Footprint Library)",
        ));
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

    // Validate file type - must be a Schematic Library. The HEADER property is
    // required rather than optional: Altium always writes it and so do we (see
    // `schlib::writer`), so its absence means this is not a SchLib, and treating
    // that as an empty-but-valid library is the same silent-data-loss trap as
    // the PcbLib case above.
    let Some(header) = props.get("header") else {
        return Err(AltiumError::wrong_file_type(
            "SchLib",
            "unrecognised file (FileHeader has no HEADER property)",
        ));
    };
    if !header.contains("Schematic Library") {
        // Detect what type it actually is for a helpful error message
        let actual_type = if header.contains("PCB Library") {
            "PcbLib (PCB Footprint Library)"
        } else {
            header
        };
        return Err(AltiumError::wrong_file_type("SchLib", actual_type));
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
        unique_id: props.get("uniqueid").cloned(),
    })
}

#[cfg(test)]
mod tests {
    use super::SchLib;
    use std::io::Write as _;

    /// Builds a compound document with the given `/FileHeader` bytes and,
    /// optionally, one component storage. Enough to drive every header path
    /// and the component walk without an Altium-authored file.
    fn library_with(path: &std::path::Path, header: &[u8], components: &[&str]) {
        let mut compound = cfb::create(path).expect("create compound document");
        {
            let mut stream = compound
                .create_stream("/FileHeader")
                .expect("create FileHeader");
            stream.write_all(header).expect("write FileHeader");
        }
        for name in components {
            compound
                .create_storage(format!("/{name}"))
                .expect("create component storage");
            let mut stream = compound
                .create_stream(format!("/{name}/Data"))
                .expect("create component Data");
            // A name block naming the symbol, then no records.
            let mut data = u32::try_from(name.len() + 1)
                .unwrap()
                .to_le_bytes()
                .to_vec();
            data.push(u8::try_from(name.len()).unwrap());
            data.extend_from_slice(name.as_bytes());
            stream.write_all(&data).expect("write component Data");
        }
        compound.flush().expect("flush compound document");
    }

    /// The length-prefixed, null-terminated parameter block a `SchLib` header
    /// is.
    fn header_block(params: &str) -> Vec<u8> {
        let mut body = params.as_bytes().to_vec();
        body.push(0);
        let mut out = u32::try_from(body.len()).unwrap().to_le_bytes().to_vec();
        out.extend_from_slice(&body);
        out
    }

    fn temp_dir() -> tempfile::TempDir {
        std::fs::create_dir_all(".tmp").expect("create .tmp");
        let root = std::path::Path::new(".tmp")
            .canonicalize()
            .expect("canonicalise .tmp");
        tempfile::tempdir_in(root).expect("create temp dir")
    }

    #[test]
    fn a_header_that_does_not_identify_a_symbol_library_is_refused() {
        // Reading a footprint library — or anything unrecognised — as a symbol
        // library must fail rather than look empty. An append-style caller
        // that saw "zero symbols" would write an empty library over the file.
        let dir = temp_dir();

        for (name, header, needle) in [
            ("Short.SchLib", vec![1, 2, 3], "too short"),
            (
                "NoHeaderKey.SchLib",
                header_block("|COMPCOUNT=0"),
                "unrecognised file",
            ),
            (
                "Pcb.SchLib",
                header_block("|HEADER=Protel for Windows - PCB Library"),
                "PcbLib",
            ),
            (
                "Foreign.SchLib",
                header_block("|HEADER=Some Other Tool"),
                "Some Other Tool",
            ),
        ] {
            let path = dir.path().join(name);
            library_with(&path, &header, &[]);
            let err = SchLib::open(&path).expect_err("a bad header must be refused");
            assert!(
                err.to_string().contains(needle),
                "{name}: expected {needle:?}, got: {err}"
            );
        }
    }

    #[test]
    fn a_truncated_header_block_is_refused() {
        // The block is length-prefixed; a length past the end of the stream
        // would otherwise slice out of bounds.
        let dir = temp_dir();
        let path = dir.path().join("Truncated.SchLib");
        let mut header = header_block("|HEADER=Schematic Library");
        header[0] = 0xFF; // claim far more than follows
        library_with(&path, &header, &[]);

        let err = SchLib::open(&path).expect_err("a truncated header must be refused");
        assert!(err.to_string().contains("truncated"), "{err}");
    }

    #[test]
    fn a_component_the_header_names_but_the_file_lacks_is_skipped() {
        // The header's LIBREF list and the actual storages can disagree in a
        // hand-edited or partially-written file. A missing storage costs that
        // one symbol; the rest of the library still opens.
        let dir = temp_dir();
        let path = dir.path().join("Partial.SchLib");
        library_with(
            &path,
            &header_block("|HEADER=Schematic Library|COMPCOUNT=2|LIBREF0=PRESENT|LIBREF1=MISSING"),
            &["PRESENT"],
        );

        let lib = SchLib::open(&path).expect("the library should still open");
        assert_eq!(lib.len(), 1, "only the present symbol should load");
        assert!(lib.get("PRESENT").is_some());
        assert!(lib.get("MISSING").is_none());
    }

    #[test]
    fn a_component_storage_with_no_data_stream_is_skipped() {
        // The storage walk finds the component by its storage, but the Data
        // stream is what carries the symbol. A storage without one — a
        // half-written file — must cost that symbol alone, not the open.
        let dir = temp_dir();
        let path = dir.path().join("Ghost.SchLib");
        {
            let mut compound = cfb::create(&path).expect("create compound document");
            {
                let mut stream = compound
                    .create_stream("/FileHeader")
                    .expect("create FileHeader");
                stream
                    .write_all(&header_block(
                        "|HEADER=Schematic Library|COMPCOUNT=2|LIBREF0=REAL|LIBREF1=GHOST",
                    ))
                    .expect("write FileHeader");
            }
            for name in ["REAL", "GHOST"] {
                compound
                    .create_storage(format!("/{name}"))
                    .expect("create component storage");
            }
            // Only REAL gets a Data stream; GHOST is an empty storage.
            let mut stream = compound
                .create_stream("/REAL/Data")
                .expect("create component Data");
            stream
                .write_all(&[5, 0, 0, 0, 4, b'R', b'E', b'A', b'L'])
                .expect("write component Data");
            compound.flush().expect("flush compound document");
        }

        let lib = SchLib::open(&path).expect("the library should still open");
        assert_eq!(lib.len(), 1, "only the symbol with a Data stream loads");
        assert!(lib.get("REAL").is_some());
        assert!(lib.get("GHOST").is_none());
    }

    #[test]
    fn a_library_with_no_file_header_at_all_is_refused() {
        let dir = temp_dir();
        let path = dir.path().join("Headerless.SchLib");
        {
            let mut compound = cfb::create(&path).expect("create compound document");
            compound.flush().expect("flush");
        }
        let err = SchLib::open(&path).expect_err("a headerless library must be refused");
        assert!(err.to_string().contains("FileHeader"), "{err}");
    }
    /// A component whose Data stream is missing is skipped with a warning;
    /// the rest of the library still reads.
    #[test]
    fn a_component_without_a_data_stream_is_skipped_and_the_rest_kept() {
        let dir = temp_dir();
        let path = dir.path().join("Damaged.SchLib");
        library_with(
            &path,
            &header_block(
                "|HEADER=Protel for Windows - Schematic Library Editor Binary File Version 5.0",
            ),
            &["GOOD", "BROKEN"],
        );
        {
            let mut compound = cfb::open_rw(&path).expect("reopen compound");
            compound
                .remove_stream("/BROKEN/Data")
                .expect("remove the component's Data stream");
        }
        let lib = SchLib::open(&path).expect("the library still opens");
        assert!(lib.get("GOOD").is_some(), "the intact symbol is kept");
        assert!(lib.get("BROKEN").is_none(), "the damaged one is skipped");
    }

    /// When /Storage holds fewer payloads than the symbols have embedded
    /// images, attachment stops at the shortage instead of failing the read:
    /// the images past it simply carry no bytes.
    #[test]
    fn embedded_images_past_the_storage_payloads_carry_no_bytes() {
        use crate::altium::schlib::primitives::Image;
        use crate::altium::schlib::{SchLib as Lib, Symbol};
        use std::io::{Cursor, Read as _};

        let write_lib = |image_count: usize| -> Vec<u8> {
            let mut symbol = Symbol::new("IMGS");
            for i in 0..image_count {
                let mut image = Image::new(
                    0,
                    i32::try_from(i).unwrap() * 20,
                    10,
                    10,
                    format!("i{i}.bmp"),
                );
                image.embed_image = true;
                image.image_data = Some(vec![0x42; 8]);
                symbol.add_image(image);
            }
            let mut lib = Lib::new();
            lib.add(symbol);
            let mut buffer = Cursor::new(Vec::new());
            lib.write(&mut buffer).expect("write");
            buffer.into_inner()
        };

        let two_images = write_lib(2);
        let one_image = write_lib(1);

        // Swap the two-image library's /Storage for the one-image library's,
        // so the payload count falls short of the image count.
        let short_storage = {
            let mut donor = cfb::CompoundFile::open(Cursor::new(one_image)).expect("open donor");
            let mut bytes = Vec::new();
            donor
                .open_stream("/Storage")
                .expect("donor /Storage")
                .read_to_end(&mut bytes)
                .expect("read donor /Storage");
            bytes
        };
        let doctored = {
            let mut compound =
                cfb::CompoundFile::open(Cursor::new(two_images)).expect("open target");
            compound.remove_stream("/Storage").expect("drop /Storage");
            {
                let mut stream = compound
                    .create_stream("/Storage")
                    .expect("recreate /Storage");
                std::io::Write::write_all(&mut stream, &short_storage)
                    .expect("write short /Storage");
            }
            compound.flush().expect("flush");
            compound.into_inner().into_inner()
        };

        let lib = SchLib::read(Cursor::new(doctored)).expect("the library still reads");
        let symbol = lib.get("IMGS").expect("symbol");
        assert_eq!(symbol.images.len(), 2, "both image records survive");
        assert!(
            symbol.images[0].image_data.is_some(),
            "the available payload attaches"
        );
        assert!(
            symbol.images[1].image_data.is_none(),
            "the image past the shortage carries no bytes"
        );
    }
}
