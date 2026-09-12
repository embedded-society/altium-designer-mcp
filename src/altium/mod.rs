//! Altium Designer file format handling.
//!
//! This module provides read/write capabilities for Altium Designer library files:
//!
//! - `.PcbLib` — PCB footprint libraries
//! - `.SchLib` — Schematic symbol libraries
//!
//! # File Format
//!
//! Altium library files are OLE Compound Documents (CFB format) containing:
//!
//! - A `FileHeader` stream with library metadata
//! - One storage per component, each containing:
//!   - `Data` stream with primitives (pads, tracks, arcs, etc.)
//!   - `Parameters` stream with component properties
//!
//! See `docs/PCBLIB_FORMAT.md` and `docs/SCHLIB_FORMAT.md` for detailed format documentation.
//!
//! # Architecture
//!
//! This module provides low-level file I/O. The AI handles:
//! - IPC-7351B calculations
//! - Package layout decisions
//! - Style choices

pub(crate) mod base64_opt;
pub(crate) mod bytes;
/// Declares a primitive-kind enum together with everything that must list
/// every variant — the write order, the variant count and the JSON-boundary
/// name — from ONE list, so adding a kind cannot leave a list short. The
/// variants are declared in write order.
macro_rules! primitive_kinds {
    (
        $(#[$enum_doc:meta])*
        $enum_name:ident {
            $( $(#[$doc:meta])* $variant:ident => $name:literal ),+ $(,)?
        }
    ) => {
        $(#[$enum_doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $enum_name {
            $( $(#[$doc])* $variant, )+
        }

        impl $enum_name {
            /// How many kinds there are.
            pub const COUNT: usize = [$(stringify!($variant)),+].len();

            /// Every kind, in the order a component with no recorded order of
            /// its own is written in.
            pub const WRITE_ORDER: [Self; Self::COUNT] = [$(Self::$variant,)+];

            /// The kind's name as the JSON boundary spells it (the serde form),
            /// so a report key built from it matches the list the kind fills.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }
        }
    };
}

pub mod error;
pub(crate) mod framing;
pub mod libpkg;
pub mod pcblib;
pub mod schlib;
pub(crate) mod serde_round;
pub(crate) mod text;

pub use error::{AltiumError, AltiumResult};
pub use pcblib::{Footprint, PcbLib};
pub use schlib::{SchLib, Symbol};
pub use text::TextJustification;

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{BuildHasher, Hash, Hasher};

/// Maximum length for OLE Compound File storage/stream names.
///
/// OLE Compound Document format limits entry names to 31 UTF-16 code units.
/// We enforce that 31-code-unit limit (see `utf16_len` / `truncate_utf16`); for
/// ASCII names one code unit is one byte, so the effective limit is 31 chars.
pub const MAX_OLE_NAME_LEN: usize = 31;

/// Reserve 4 chars for "~XXX" suffix (allows 999 collisions).
const SUFFIX_LEN: usize = 4;

/// Encodes a string to Windows-1252 bytes — Altium's on-disk string encoding.
///
/// Altium stores all library strings as Windows-1252, not UTF-8. Each character
/// representable in Windows-1252 (all of Latin-1 plus the cp1252 punctuation
/// block — e.g. `µ`, `°`, `±`, `é`) maps to its single byte; any other character
/// is replaced with `?` so the byte length stays one-per-character and the file
/// never carries raw UTF-8 under a Windows-1252-decoded block.
#[must_use]
pub fn encode_windows1252(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut buf = [0u8; 4];
    for ch in s.chars() {
        let utf8 = ch.encode_utf8(&mut buf);
        let (bytes, _, had_errors) = encoding_rs::WINDOWS_1252.encode(utf8);
        if had_errors {
            out.push(b'?');
        } else {
            out.extend_from_slice(&bytes);
        }
    }
    out
}

/// Decodes Windows-1252 bytes to a string — Altium's on-disk string encoding.
///
/// Windows-1252 maps every byte to a character, so this never fails.
#[must_use]
pub fn decode_windows1252(bytes: &[u8]) -> String {
    encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned()
}

/// Decodes an Altium binary string, preferring UTF-8 when the bytes are UTF-8.
///
/// Altium writes a name that Windows-1252 cannot hold — CJK, Cyrillic, Thai,
/// any of them — as its raw UTF-8 bytes inside a record that is otherwise
/// Windows-1252. Decoding such a pin name as Windows-1252 yields mojibake:
/// `电阻` comes back as `ç”µé˜»`.
///
/// Multi-byte UTF-8 is a narrow subset of arbitrary byte pairs, so treating
/// valid non-ASCII UTF-8 as UTF-8 is safe in practice: a real Windows-1252
/// string like `Ohm é` is not valid UTF-8 and falls through unchanged. The
/// ambiguity is Altium's own — the same tradeoff [`decode_utf8_param_value`]
/// already makes for parameter values — and the `TEXT_WIN1252` golden pins the
/// Windows-1252 side of it.
#[must_use]
pub fn decode_altium_text(bytes: &[u8]) -> String {
    if !bytes.is_ascii() {
        if let Ok(text) = std::str::from_utf8(bytes) {
            return text.to_string();
        }
    }
    decode_windows1252(bytes)
}

/// Returns `true` when `value` cannot be represented losslessly in Windows-1252,
/// so it must be stored behind a `%UTF8%` key to avoid silent `?` corruption.
///
/// Altium stores a text value's plain `Text` key as Windows-1252; any character
/// outside that code page (Cyrillic, CJK, Greek `Ω`, …) would be replaced with
/// `?` on write. Altium (and `AltiumSharp`) detect this by re-encoding the value
/// through Windows-1252 and checking it survives; when it does not, the value is
/// emitted as `%UTF8%Text` instead. This mirrors that check exactly (the round
/// trip is `WINDOWS_1252.decode(WINDOWS_1252.encode(value)) != value`).
#[must_use]
pub fn requires_utf8(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    decode_windows1252(&encode_windows1252(value)) != value
}

/// Encodes a Unicode `value` into the "one Windows-1252 char per UTF-8 byte"
/// form Altium uses for a `%UTF8%`-prefixed value.
///
/// The surrounding parameter record is written as Windows-1252, so a value whose
/// UTF-8 bytes are mapped one-per-char here is emitted on disk as its raw UTF-8
/// byte sequence. This is the inverse of [`decode_utf8_param_value`]. The mapping
/// is a byte bijection (every 0x00–0xFF Windows-1252 char round-trips through
/// [`encode_windows1252`]), so no bytes are lost.
#[must_use]
pub fn encode_utf8_param_value(value: &str) -> String {
    decode_windows1252(value.as_bytes())
}

/// Decodes a `%UTF8%`-prefixed value that was read back from a Windows-1252
/// decoded record, recovering the original Unicode string.
///
/// The record was decoded as Windows-1252, so a UTF-8 value arrives as one char
/// per raw byte ("mojibake"). Re-encoding those chars to Windows-1252 bytes
/// recovers the original UTF-8 byte sequence, which is then decoded as UTF-8.
/// Inverse of [`encode_utf8_param_value`]; matches `AltiumSharp`'s
/// `DecodeUtf8ParameterValue`.
#[must_use]
pub fn decode_utf8_param_value(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let bytes = encode_windows1252(value);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Converts a value to the form Altium stores it in on the wire.
///
/// A Windows-1252 value is returned unchanged. Anything else becomes its UTF-8
/// bytes carried one char per byte, so that encoding the record as Windows-1252
/// emits exactly those bytes — which is how Altium stores a non-Latin name in a
/// component's name block, its `PATTERN`, the library component list and the CFB
/// storage name alike.
#[must_use]
pub fn to_wire_text(value: &str) -> String {
    if requires_utf8(value) {
        encode_utf8_param_value(value)
    } else {
        value.to_string()
    }
}

/// Recovers a value stored as raw UTF-8 bytes inside a Windows-1252 record.
///
/// Returns `None` when `raw` is plain ASCII (nothing to recover) or when its
/// bytes are not valid UTF-8, in which case it is a genuine Windows-1252 value
/// and must be taken verbatim. Inverse of [`to_wire_text`].
#[must_use]
pub fn from_wire_text(raw: &str) -> Option<String> {
    if raw.is_ascii() {
        return None;
    }
    // Every char came from a Windows-1252 decode, so re-encoding is exact.
    let bytes = encode_windows1252(raw);
    std::str::from_utf8(&bytes).ok().map(str::to_string)
}

/// Recovers real text from an ANSI-widened byte string, whatever single-byte
/// code page did the widening.
///
/// Altium widens a value's raw UTF-8 bytes one-per-char through the *authoring
/// machine's* ANSI code page (`PinWideText` values, CFB storage names), so the
/// same file reads differently by locale. Each plausible code page is tried:
/// the one that encodes `text` losslessly back to bytes forming valid
/// non-ASCII UTF-8 is the one that widened it, and those bytes decode to the
/// real value. Returns `None` when no code page fits — which is what happens
/// for text that is already real (its characters do not narrow to a UTF-8 byte
/// pattern), so a real value passed in is left for the caller to use verbatim.
#[must_use]
pub fn fold_ansi_widened(text: &str) -> Option<String> {
    if text.is_ascii() {
        return None;
    }
    for enc in [
        encoding_rs::WINDOWS_1252,
        encoding_rs::WINDOWS_1250,
        encoding_rs::WINDOWS_1251,
        encoding_rs::WINDOWS_1253,
        encoding_rs::WINDOWS_1254,
        encoding_rs::WINDOWS_1255,
        encoding_rs::WINDOWS_1256,
        encoding_rs::WINDOWS_1257,
        encoding_rs::WINDOWS_1258,
        encoding_rs::WINDOWS_874,
    ] {
        let (bytes, _, had_errors) = enc.encode(text);
        if had_errors {
            continue;
        }
        if let Ok(real) = std::str::from_utf8(&bytes) {
            if !real.is_ascii() {
                return Some(real.to_string());
            }
        }
    }
    None
}

/// Generates a safe OLE storage name for a component.
///
/// OLE Compound File names are limited to 31 UTF-16 code units. This function:
/// - Returns the name as-is if it fits within the limit
/// - Plain-truncates a longer name to the limit, as Altium does — the
///   `SectionKeys` stream carries the mapping back to the real name, so the
///   storage name has to match Altium's or the mapping misses
/// - Falls back to a `~NNN` suffix only when the truncation collides with a
///   name already taken
///
/// # Arguments
///
/// * `name` - The full component name (wire form)
/// * `used_names` - Set of OLE names already in use (to avoid collisions)
///
/// # Returns
///
/// A safe OLE name (≤31 units) that doesn't collide with existing names.
/// Characters an OLE/CFB storage name cannot contain. `generate_ole_name`
/// maps each to `_`; a library refuses to save a component whose name is
/// empty, since there is no storage name to derive from nothing.
pub const OLE_NAME_FORBIDDEN: &[char] = &['/', '\\', ':', '!'];

/// Whether two component names are the same name, regardless of case.
///
/// That is how the OLE directory compares the storage names they become
/// (`RES_0402` and `res_0402` cannot both be stored) and how Altium resolves
/// a component by name.
#[must_use]
pub fn same_name(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_uppercase)
        .eq(b.chars().flat_map(char::to_uppercase))
}

/// A name's case-folded form, for sets that must hold names the way the OLE
/// directory does (see [`same_name`]).
#[must_use]
pub fn folded_name(name: &str) -> String {
    name.chars().flat_map(char::to_uppercase).collect()
}

/// Whether `candidate` is already taken in `used_names`, ignoring case — a
/// storage name that differs only in case from one in use is the same
/// storage to the OLE directory, and creating it fails.
fn ole_name_taken<S: BuildHasher>(used_names: &HashSet<String, S>, candidate: &str) -> bool {
    used_names.contains(candidate) || used_names.iter().any(|used| same_name(used, candidate))
}

#[must_use]
pub fn generate_ole_name<S: BuildHasher>(name: &str, used_names: &HashSet<String, S>) -> String {
    // OLE/CFB storage names cannot contain `/`, `\`, `:` or `!`: the `cfb`
    // crate reads `/` and `\` as path separators (the storage-creation call
    // fails) and asserts on `:` (the whole save would panic). Altium sanitises
    // a slash to `_` before creating the component storage, so a component
    // whose name carries one still saves; the other three get the same
    // treatment. Apply it up front so both the short-name and truncated paths
    // use the sanitised form; `SectionKeys` still maps the storage name back
    // to the real one.
    let sanitized: String = name
        .chars()
        .map(|c| {
            if OLE_NAME_FORBIDDEN.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name = sanitized.as_str();

    // The OLE/CFB limit is 31 UTF-16 code units — not bytes or chars. Measure
    // it correctly so supplementary-plane characters (2 units each) cannot slip
    // a name past the limit and make the whole save fail.
    if utf16_len(name) <= MAX_OLE_NAME_LEN && !ole_name_taken(used_names, name) {
        return name.to_string();
    }

    // Altium's own rule: cut the name at the limit and record the mapping in
    // `SectionKeys`. The golden's Sinhala symbol is cut mid-codepoint on disk,
    // so the cut is on wire bytes with no regard for character boundaries; for
    // a wire name (one byte per char) truncating by UTF-16 unit is the same
    // cut, minus the ability to split a char in two.
    let plain = truncate_utf16(name, MAX_OLE_NAME_LEN);
    if !ole_name_taken(used_names, &plain) {
        return plain;
    }

    // Two names sharing their first 31 units: fall back to a "~NNN" suffix for
    // the later one. Altium's behaviour here is unobserved; uniqueness matters
    // more than matching it, and SectionKeys still maps the name back.
    let prefix = truncate_utf16(name, MAX_OLE_NAME_LEN - SUFFIX_LEN);
    for i in 1..1000 {
        let candidate = format!("{prefix}~{i:03}");
        if !ole_name_taken(used_names, &candidate) {
            return candidate;
        }
    }

    // Fallback: use hash-based suffix (extremely unlikely to reach here). Drop
    // one more *char* (never a byte) so we stay within the limit without
    // slicing on a non-char boundary.
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();
    let mut short = prefix;
    short.pop();
    format!("{short}~{:03X}", hash & 0xFFF)
}

/// Length of `s` in UTF-16 code units — the unit OLE/CFB storage names are
/// limited to. Supplementary-plane characters count as two.
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Truncates `s` to at most `max_units` UTF-16 code units, on a char boundary.
fn truncate_utf16(s: &str, max_units: usize) -> String {
    let mut out = String::new();
    let mut units = 0;
    for ch in s.chars() {
        let w = ch.len_utf16();
        if units + w > max_units {
            break;
        }
        out.push(ch);
        units += w;
    }
    out
}

/// Generates collision-free OLE storage names for an ordered list of component
/// names. Shared by both library writers so the truncation/uniquing rules are
/// identical; the returned names line up positionally with the input.
pub(crate) fn generate_ole_names<'a, I>(names: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut used = HashSet::new();
    let mut out = Vec::new();
    for name in names {
        let ole = generate_ole_name(name, &used);
        used.insert(ole.clone());
        out.push(ole);
    }
    out
}

/// Encodes a `PcbLib`'s root `/SectionKeys` stream: the map from a footprint's
/// real `LibRef` back to its truncated storage name, in the binary layout of
/// Altium-authored libraries (four in the reference corpus, every entry a name
/// past the 31-unit cap; `AltiumSharp` reads the same layout):
///
/// ```text
/// [u32 count]
/// [u32 len][u8 str_len][LibRef]  [u32 len][u8 str_len][SectionKey]   (count times)
/// ```
///
/// Each string is a `WriteStringBlock` of wire bytes (Windows-1252; a
/// non-1252 name as its raw UTF-8 bytes), so `str_len` caps a name at 255
/// bytes, the cap the footprint's own records impose anyway. A `SchLib` uses a
/// text record here instead ([`encode_schlib_section_keys`]); writing that
/// layout into a `PcbLib` left Altium Designer unable to resolve the mapped
/// footprints (#507).
///
/// Returns `Ok(None)` when no name was truncated, so no stream is written, as
/// in Altium.
///
/// # Errors
///
/// A name or storage name longer than 255 bytes cannot be framed.
pub(crate) fn encode_pcblib_section_keys(
    pairs: &[(String, String)],
) -> AltiumResult<Option<Vec<u8>>> {
    if pairs.is_empty() {
        return Ok(None);
    }
    let count = u32::try_from(pairs.len()).map_err(|_| AltiumError::InvalidParameter {
        name: "name".to_string(),
        message: format!(
            "{} truncated names exceed the SectionKeys count",
            pairs.len()
        ),
    })?;
    let mut data = count.to_le_bytes().to_vec();
    for (lib_ref, section_key) in pairs {
        for (what, value) in [("name", lib_ref), ("storage name", section_key)] {
            let bytes = encode_windows1252(value);
            if bytes.len() > 255 {
                return Err(AltiumError::InvalidParameter {
                    name: "name".to_string(),
                    message: format!(
                        "footprint {what} '{value}' is {} bytes; SectionKeys holds at most 255",
                        bytes.len()
                    ),
                });
            }
            framing::write_string_block(&mut data, &bytes);
        }
    }
    Ok(Some(data))
}

/// Parses a `PcbLib`'s `/SectionKeys` stream into `(LibRef, SectionKey)`
/// pairs, both in wire form. Inverse of [`encode_pcblib_section_keys`]; a
/// stream that ends mid-entry yields the pairs before the cut.
///
/// A stream in the `SchLib` text layout is accepted too: this crate wrote that
/// layout into every `PcbLib` with a truncated name before #507, and those
/// libraries still have to order their footprints correctly.
pub(crate) fn parse_pcblib_section_keys(data: &[u8]) -> Vec<(String, String)> {
    let Some(count) = bytes::read_u32_le(data, 0) else {
        return Vec::new();
    };
    let mut pairs = Vec::new();
    let mut offset = 4;
    for _ in 0..count {
        let Some((lib_ref, next)) = read_wire_string_block(data, offset) else {
            break;
        };
        let Some((section_key, next)) = read_wire_string_block(data, next) else {
            break;
        };
        pairs.push((lib_ref, section_key));
        offset = next;
    }
    if pairs.is_empty() {
        return parse_schlib_section_keys(data);
    }
    pairs
}

/// Reads one `WriteStringBlock` at `offset` as wire text (the Pascal string's
/// bytes decoded as Windows-1252), with the offset just past the block.
fn read_wire_string_block(data: &[u8], offset: usize) -> Option<(String, usize)> {
    let (block, next) = framing::read_block(data, offset)?;
    let len = usize::from(*block.first()?);
    let text = block.get(1..1 + len)?;
    Some((decode_windows1252(text), next))
}

/// Encodes a `SchLib`'s root `/SectionKeys` stream: the map from a symbol's
/// real `LibRef` back to its truncated storage name.
///
/// Altium writes one entry per symbol whose name does not survive the
/// 31-unit storage cap. Layout, pinned by the golden `SchLib` (`KeyCount=5`,
/// one entry per over-cap name); a `PcbLib` carries a binary stream instead
/// ([`encode_pcblib_section_keys`]):
///
/// ```text
/// [u32 len]["|KeyCount=N|%UTF8%LibRef0=…|||LibRef0=…|%UTF8%SectionKey0=…|||SectionKey0=…" + 0x00]
/// ```
///
/// Values are wire strings (a non-Windows-1252 name is its raw UTF-8 bytes).
/// A non-ASCII value gets a `%UTF8%` twin, written **before** the plain key and
/// followed by two empty segments — the `|||` is Altium's own separator, kept
/// so the stream matches theirs byte-for-byte given the same values. The twin
/// carries the same bytes as the plain key: Altium builds its twin by decoding
/// the UTF-8 bytes through the authoring machine's ANSI code page, which makes
/// the golden's twin content a locale artefact (Windows-1250 there), not a
/// format rule — identical bytes are correct on every machine and every reader
/// recovers the same name from either key.
///
/// Returns `None` when no name was truncated, so no stream is written — the
/// common case, and byte-identical to Altium's output for such a library.
pub(crate) fn encode_schlib_section_keys(pairs: &[(String, String)]) -> Option<Vec<u8>> {
    use std::fmt::Write as _;

    if pairs.is_empty() {
        return None;
    }

    let mut text = format!("|KeyCount={}", pairs.len());
    let field = |key: &str, value: &str, out: &mut String| {
        if value.is_ascii() {
            let _ = write!(out, "|{key}={value}");
        } else {
            let _ = write!(out, "|%UTF8%{key}={value}|||{key}={value}");
        }
    };
    for (i, (lib_ref, section_key)) in pairs.iter().enumerate() {
        field(&format!("LibRef{i}"), lib_ref, &mut text);
        field(&format!("SectionKey{i}"), section_key, &mut text);
    }

    let mut data = Vec::new();
    framing::write_cstring_param_block(&mut data, &encode_windows1252(&text));
    Some(data)
}

/// Parses a `SchLib`'s `/SectionKeys` stream into `(LibRef, SectionKey)` pairs,
/// both in wire form. Inverse of [`encode_schlib_section_keys`]; the plain keys are read and
/// the `%UTF8%` twins ignored, since the plain key already holds the raw UTF-8
/// bytes and the twin's encoding depends on the locale that authored the file.
pub(crate) fn parse_schlib_section_keys(data: &[u8]) -> Vec<(String, String)> {
    let Some((block, _)) = framing::read_block(data, 0) else {
        return Vec::new();
    };
    let text = decode_windows1252(block.strip_suffix(&[0x00]).unwrap_or(block));
    let params = parse_pipe_params_raw(&text);

    let count = params
        .get("KeyCount")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    (0..count)
        .filter_map(|i| {
            let lib_ref = params.get(&format!("LibRef{i}"))?;
            let section_key = params.get(&format!("SectionKey{i}"))?;
            Some((lib_ref.clone(), section_key.clone()))
        })
        .collect()
}

/// Creates an Altium-mandated OLE v3 (512-byte sector) compound file.
///
/// Altium Designer requires v3; both writers must go through here so they stay
/// on the same version.
pub(crate) fn create_ole<W: std::io::Read + std::io::Write + std::io::Seek>(
    writer: W,
) -> AltiumResult<cfb::CompoundFile<W>> {
    cfb::CompoundFile::create_with_version(cfb::Version::V3, writer)
        .map_err(|e| AltiumError::invalid_ole(format!("Failed to create OLE file: {e}")))
}

/// Opens an existing OLE compound file.
pub(crate) fn open_ole<R: std::io::Read + std::io::Seek>(
    reader: R,
) -> AltiumResult<cfb::CompoundFile<R>> {
    cfb::CompoundFile::open(reader)
        .map_err(|e| AltiumError::invalid_ole(format!("Failed to open OLE file: {e}")))
}

/// Creates a stream at `path` and writes `data` to it. The emitted stream
/// content is exactly `data`, so output is byte-identical to a hand-written
/// `create_stream` + `write_all`.
pub(crate) fn write_stream<F: std::io::Read + std::io::Write + std::io::Seek>(
    cfb: &mut cfb::CompoundFile<F>,
    path: &str,
    data: &[u8],
) -> AltiumResult<()> {
    let mut stream = cfb
        .create_stream(path)
        .map_err(|e| AltiumError::invalid_ole(format!("Failed to create stream {path}: {e}")))?;
    std::io::Write::write_all(&mut stream, data)
        .map_err(|e| AltiumError::invalid_ole(format!("Failed to write stream {path}: {e}")))?;
    Ok(())
}

/// Opens the OLE stream at `path` and reads it fully into a `Vec`.
///
/// Returns `None` if the stream is absent or cannot be opened/read — the
/// read-side counterpart of [`write_stream`]. `path` is an internal OLE path,
/// not a filesystem path.
pub(crate) fn read_stream_opt<F, P>(cfb: &mut cfb::CompoundFile<F>, path: P) -> Option<Vec<u8>>
where
    F: std::io::Read + std::io::Seek,
    P: AsRef<std::path::Path>,
{
    let path = path.as_ref();
    if !cfb.is_stream(path) {
        return None;
    }
    let mut stream = cfb.open_stream(path).ok()?;
    let mut data = Vec::new();
    std::io::Read::read_to_end(&mut stream, &mut data).ok()?;
    Some(data)
}

/// Creates an OLE storage at `path`, wrapping failures as `invalid_ole`.
///
/// The storage-creation mirror of [`write_stream`]. `path` is an internal OLE
/// path. Callers that must guard against an already-existing storage check
/// `cfb.exists(path)` themselves.
pub(crate) fn create_storage<F: std::io::Read + std::io::Write + std::io::Seek>(
    cfb: &mut cfb::CompoundFile<F>,
    path: &str,
) -> AltiumResult<()> {
    cfb.create_storage(path)
        .map_err(|e| AltiumError::invalid_ole(format!("Failed to create storage {path}: {e}")))?;
    Ok(())
}

/// Writes a library to `path` atomically.
///
/// `write` serialises into memory; the bytes then go to a sibling temp file
/// (named with `tmp_ext`) in one write, which is renamed over the
/// destination, so a failed or partial write never clobbers an existing
/// file and nothing is left behind on failure. Serialising in memory rather
/// than straight into the file matters: a compound-file writer seeks and
/// rewrites its sector and directory tables constantly, and doing that
/// against an unbuffered file costs a disk round trip each time — some 40×
/// the time of building the image and writing it once. Shared by both
/// library writers and by `restore_backup`.
pub(crate) fn save_atomic(
    path: &std::path::Path,
    tmp_ext: &str,
    write: impl FnOnce(&mut std::io::Cursor<Vec<u8>>) -> AltiumResult<()>,
) -> AltiumResult<()> {
    let mut image = std::io::Cursor::new(Vec::new());
    write(&mut image)?;

    // Temp file in the same directory ensures the rename stays on one filesystem.
    let temp_path = path.with_extension(tmp_ext);
    if let Err(e) = std::fs::write(&temp_path, image.get_ref()) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(AltiumError::file_write(&temp_path, e));
    }

    // Atomically rename the temp file over the target (overwrites existing).
    std::fs::rename(&temp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        AltiumError::file_write(path, e)
    })?;

    Ok(())
}

/// Parses a pipe-delimited `KEY=VALUE` parameter string into a map, lowercasing
/// keys (values kept verbatim). Segments that are empty or lack `=` are skipped;
/// duplicate keys keep the last value. Used by `SchLib`'s text/property records.
pub(crate) fn parse_pipe_params(text: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for part in text.split('|') {
        if let Some((key, value)) = part.split_once('=') {
            map.insert(key.to_lowercase(), value.to_string());
        }
    }
    map
}

/// Like [`parse_pipe_params`] but preserves key case verbatim and trims trailing
/// NUL padding (then surrounding whitespace) from values. `PcbLib` records match
/// keys in their native UPPERCASE form and pad values with `\0`, neither of which
/// the lowercasing `parse_pipe_params` handles. Callers look keys up in UPPERCASE.
pub(crate) fn parse_pipe_params_raw(text: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for part in text.split('|') {
        if let Some((key, value)) = part.split_once('=') {
            // Only the NUL terminator is stripped. Surrounding spaces are
            // significant: Altium writes `NAME= ` (a single space) on a default
            // region and component body, and trimming turned that into an empty
            // name which the writer then emitted, changing the record on every
            // read-modify-write. Numeric readers trim at their own call sites.
            map.insert(key.to_string(), value.trim_end_matches('\0').to_string());
        }
    }
    map
}

/// Like [`parse_pipe_params_raw`] but preserves the original key order and every
/// occurrence (no de-duplication), returning `(KEY, VALUE)` pairs in read order.
///
/// Parsing stops at the first NUL byte: `PcbLib` parameter blocks are
/// NUL-terminated and (for a `ComponentBody`) followed by binary outline bytes,
/// which must not be mistaken for further `KEY=VALUE` segments. Keys are kept in
/// their native UPPERCASE form and values are trimmed of trailing NUL padding then
/// surrounding whitespace, matching [`parse_pipe_params_raw`].
///
/// Used to capture the unmodelled Region / `ComponentBody` parameters into an
/// order-preserving `additional_parameters` catch-all so a read-modify-write does
/// not silently drop keys the typed model does not recognise.
pub(crate) fn parse_pipe_params_ordered(text: &str) -> Vec<(String, String)> {
    // Truncate at the NUL terminator so trailing binary (outline) bytes are ignored.
    let text = text.split('\0').next().unwrap_or(text);
    text.split('|')
        .filter_map(|part| {
            part.split_once('=')
                .map(|(key, value)| (key.to_string(), value.trim().to_string()))
        })
        .collect()
}

/// Builds a stable-reorder ranking function from a desired name order.
///
/// The returned closure maps a name to its sort rank: its index in `new_order`,
/// or `new_order.len()` for names not listed — so unlisted items sort after
/// listed ones, keeping their original relative order under a stable sort.
/// Shared by both libraries' `reorder` methods, which differ only in their
/// backing collection (`IndexMap` vs `Vec`).
pub(crate) fn order_ranker<'a>(new_order: &[&'a str]) -> impl Fn(&str) -> usize + 'a {
    // Names compare the way the library resolves them (see `same_name`).
    let order_map: std::collections::HashMap<String, usize> = new_order
        .iter()
        .enumerate()
        .map(|(i, name)| (folded_name(name), i))
        .collect();
    let max_pos = new_order.len();
    move |name: &str| {
        order_map
            .get(&folded_name(name))
            .copied()
            .unwrap_or(max_pos)
    }
}

/// The path of the first string in `record`'s JSON shape that contains
/// `|`, the separator of Altium's pipe-delimited records — or `None`.
///
/// The format has no way to escape one, so any such string would come back
/// cut at it. Walking the serialised record covers every text field without
/// a list; `exempt` names what never reaches such a record: a bare key
/// matches that key at any depth, a path (`pads[].designator`, `pins[]`)
/// matches it and everything beneath it. Array indices read as `[]`.
#[must_use]
pub fn record_separator_path<T: serde::Serialize>(record: &T, exempt: &[&str]) -> Option<String> {
    fn exempted(path: &str, key: &str, exempt: &[&str]) -> bool {
        exempt.iter().any(|e| {
            *e == key
                || *e == path
                || path.starts_with(&format!("{e}."))
                || path.starts_with(&format!("{e}["))
        })
    }
    fn walk(value: &serde_json::Value, path: &str, key: &str, exempt: &[&str]) -> Option<String> {
        if exempted(path, key, exempt) {
            return None;
        }
        match value {
            serde_json::Value::String(text) if text.contains('|') => Some(path.to_string()),
            serde_json::Value::Object(fields) => fields.iter().find_map(|(k, v)| {
                let child = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                walk(v, &child, k, exempt)
            }),
            serde_json::Value::Array(items) => items
                .iter()
                .find_map(|v| walk(v, &format!("{path}[]"), key, exempt)),
            _ => None,
        }
    }
    let value = serde_json::to_value(record).ok()?;
    walk(&value, "", "", exempt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_differing_only_in_case_are_one_storage_name() {
        // The OLE directory compares storage names without regard to case,
        // so the second of two such names gets a distinct storage name (the
        // real name still travels in SectionKeys) instead of failing the
        // whole save inside the directory.
        assert!(same_name("RES_0402", "res_0402"));
        assert!(same_name("Ω_MODULE", "ω_module"));
        assert!(!same_name("RES_0402", "RES_0403"));
        assert_eq!(folded_name("res_0402"), "RES_0402");

        let names = generate_ole_names(["RES_0402", "res_0402", "Res_0402"]);
        assert_eq!(names, ["RES_0402", "res_0402~001", "Res_0402~002"]);

        // A ranker ranks a case variant with the name it stands for.
        let rank = order_ranker(&["b", "A"]);
        assert_eq!(rank("B"), 0);
        assert_eq!(rank("a"), 1);
        assert_eq!(rank("c"), 2);
    }

    #[test]
    fn short_name_unchanged() {
        let used = HashSet::new();
        assert_eq!(generate_ole_name("RESISTOR", &used), "RESISTOR");
    }

    #[test]
    fn ole_name_sanitises_slash() {
        let used = HashSet::new();
        assert_eq!(generate_ole_name("A/B", &used), "A_B");
    }

    #[test]
    fn windows1252_ascii_is_identical_to_utf8() {
        assert_eq!(encode_windows1252("RESC0402"), b"RESC0402");
    }

    #[test]
    fn parse_pipe_params_ordered_preserves_order_and_duplicates() {
        // Order is preserved and repeated keys are kept (unlike the HashMap variant),
        // so the region/body catch-all re-emits every occurrence in read order.
        let pairs = parse_pipe_params_ordered("A=1|B=2|A=3");
        assert_eq!(
            pairs,
            vec![
                ("A".to_string(), "1".to_string()),
                ("B".to_string(), "2".to_string()),
                ("A".to_string(), "3".to_string()),
            ]
        );
    }

    #[test]
    fn parse_pipe_params_ordered_stops_at_nul() {
        // A ComponentBody param block is NUL-terminated and followed by binary
        // outline bytes; segments after the NUL must be ignored.
        let pairs = parse_pipe_params_ordered("A=1|B=2\0\u{7}garbage|C=3");
        assert_eq!(
            pairs,
            vec![
                ("A".to_string(), "1".to_string()),
                ("B".to_string(), "2".to_string()),
            ]
        );
    }

    #[test]
    fn windows1252_encodes_latin1_as_single_bytes() {
        // "10µF": µ is U+00B5 -> a single 0xB5 byte in cp1252 (two bytes in UTF-8).
        assert_eq!(
            encode_windows1252("10\u{00B5}F"),
            vec![b'1', b'0', 0xB5, b'F']
        );
        // °, ±, é are all representable in cp1252.
        assert_eq!(
            encode_windows1252("\u{00B0}\u{00B1}\u{00E9}"),
            vec![0xB0, 0xB1, 0xE9]
        );
    }

    #[test]
    fn windows1252_substitutes_unmappable_chars() {
        // Ω (U+03A9) is not in Windows-1252 -> replaced with '?', one byte per char.
        assert_eq!(encode_windows1252("1k\u{03A9}"), vec![b'1', b'k', b'?']);
    }

    #[test]
    fn windows1252_round_trips() {
        let s = "10\u{00B5}F \u{00B1}1% \u{00B0}C";
        assert_eq!(decode_windows1252(&encode_windows1252(s)), s);
    }

    #[test]
    fn exactly_31_chars_unchanged() {
        let used = HashSet::new();
        let name = "A".repeat(31);
        assert_eq!(generate_ole_name(&name, &used), name);
    }

    #[test]
    fn long_name_truncated() {
        // Altium's rule: a plain cut at the limit, with SectionKeys carrying
        // the mapping back; the ~NNN suffix appears only on a collision.
        let used = HashSet::new();
        let name = "VERY_LONG_COMPONENT_NAME_THAT_EXCEEDS_LIMIT";
        let result = generate_ole_name(name, &used);
        assert_eq!(result, "VERY_LONG_COMPONENT_NAME_THAT_E");
        assert_eq!(result.len(), MAX_OLE_NAME_LEN);
    }

    #[test]
    fn ole_name_respects_utf16_limit_for_non_bmp() {
        // Supplementary-plane chars are 2 UTF-16 units each. cfb rejects names
        // over 31 UTF-16 code units, so a 20-emoji name must still fit.
        let used = HashSet::new();
        let name = "\u{1F600}".repeat(20); // 20 chars = 40 UTF-16 units
        let result = generate_ole_name(&name, &used);
        assert!(
            result.encode_utf16().count() <= MAX_OLE_NAME_LEN,
            "got {} UTF-16 units",
            result.encode_utf16().count()
        );
    }

    #[test]
    fn ole_name_hash_fallback_handles_multibyte_prefix_without_panicking() {
        // A long all-multibyte name forces truncation; exhausting the 999
        // numeric suffixes drives the hash fallback, which must not panic
        // by byte-slicing inside a multi-byte char.
        let name = "\u{00B5}".repeat(32); // 'µ': 1 UTF-16 unit each, 32 > 31
        let prefix = "\u{00B5}".repeat(MAX_OLE_NAME_LEN - SUFFIX_LEN);
        let mut used: HashSet<String> = (1..1000).map(|i| format!("{prefix}~{i:03}")).collect();
        // Also occupy the plain 31-unit truncation so the suffix path runs.
        used.insert("\u{00B5}".repeat(MAX_OLE_NAME_LEN));
        let result = generate_ole_name(&name, &used);
        assert!(result.encode_utf16().count() <= MAX_OLE_NAME_LEN);
        assert!(result.contains('~'));
    }

    #[test]
    fn collision_avoided() {
        let mut used = HashSet::new();
        let name = "VERY_LONG_COMPONENT_NAME_THAT_EXCEEDS_LIMIT";

        let first = generate_ole_name(name, &used);
        used.insert(first.clone());

        let second = generate_ole_name(name, &used);
        assert_ne!(first, second);
        assert!(second.len() <= MAX_OLE_NAME_LEN);
    }

    #[test]
    fn short_name_collision_handled() {
        let mut used = HashSet::new();
        used.insert("RESISTOR".to_string());

        let result = generate_ole_name("RESISTOR", &used);
        assert_ne!(result, "RESISTOR");
        assert!(result.len() <= MAX_OLE_NAME_LEN);
    }

    #[test]
    fn an_empty_parameter_value_decodes_without_a_round_trip() {
        // The decode narrows through Windows-1252 and back; an empty value has
        // nothing to narrow and must not become a lone replacement character.
        assert_eq!(decode_utf8_param_value(""), "");
        // A value that really is UTF-8 bytes widened by the ANSI read comes
        // back as the text it was.
        assert_eq!(decode_utf8_param_value("abc"), "abc");
    }

    #[test]
    fn ansi_folding_declines_text_that_was_never_widened() {
        // ASCII cannot be the widened form of anything, so there is nothing to
        // fold and the caller keeps what it had.
        assert_eq!(fold_ansi_widened("plain ascii"), None);
        assert_eq!(fold_ansi_widened(""), None);

        // Real Unicode that no single-byte page can encode is likewise left
        // alone rather than mangled into a guess.
        assert_eq!(fold_ansi_widened("\u{7535}\u{963B}"), None);

        // The golden's shape: the name's UTF-8 bytes read back through an ANSI
        // code page. Folding them through that same page recovers the real
        // text. Built with the real decoder rather than by hand — Windows-1252
        // and Latin-1 disagree over 0x80-0x9F, and these bytes land there.
        let real = "\u{7535}\u{963B}";
        let widened = decode_windows1252(real.as_bytes());
        assert_eq!(fold_ansi_widened(&widened).as_deref(), Some(real));
    }

    #[test]
    fn section_keys_from_a_stream_with_no_block_are_empty() {
        // A stream too short to frame a block yields no keys rather than
        // reading past its end.
        assert!(parse_schlib_section_keys(&[]).is_empty());
        assert!(parse_schlib_section_keys(&[1, 2, 3]).is_empty());
    }

    /// The `PcbLib` stream is Altium's binary layout — a count, then a
    /// `WriteStringBlock` for the real name and one for the storage name per
    /// entry — checked byte for byte against a 40-character name truncated to
    /// the 31-unit cap, the shape of every entry in the reference corpus.
    #[test]
    fn pcblib_section_keys_use_altium_s_binary_layout() {
        let real = "GENERIC_MLCC_CAP_0402_IPC_MEDIUM_DENSITY";
        let storage = "GENERIC_MLCC_CAP_0402_IPC_MEDIU";
        assert_eq!((real.len(), storage.len()), (40, 31));
        let pairs = vec![(real.to_string(), storage.to_string())];

        let data = encode_pcblib_section_keys(&pairs)
            .expect("two short names frame")
            .expect("one truncated name is one entry");
        let mut expected = 1u32.to_le_bytes().to_vec();
        expected.extend_from_slice(&41u32.to_le_bytes());
        expected.push(40);
        expected.extend_from_slice(real.as_bytes());
        expected.extend_from_slice(&32u32.to_le_bytes());
        expected.push(31);
        expected.extend_from_slice(storage.as_bytes());
        assert_eq!(data, expected);
        assert_ne!(data.get(4), Some(&b'|'), "not the SchLib text record");

        assert_eq!(parse_pcblib_section_keys(&data), pairs);
        assert_eq!(encode_pcblib_section_keys(&[]).unwrap(), None);
    }

    /// The stream Altium Designer wrote for a four-footprint library whose
    /// names all run past the cap (`generic_smd_chip_capacitors.PcbLib` in the
    /// reference corpus): the encoder reproduces it byte for byte from the
    /// pairs, and the parser reads the pairs back.
    #[test]
    fn pcblib_section_keys_reproduce_an_altium_authored_stream() {
        const ALTIUM: &[u8] = &[
            0x04, 0x00, 0x00, 0x00, 0x29, 0x00, 0x00, 0x00, 0x28, 0x47, 0x45, 0x4e, 0x45, 0x52,
            0x49, 0x43, 0x5f, 0x4d, 0x4c, 0x43, 0x43, 0x5f, 0x43, 0x41, 0x50, 0x5f, 0x30, 0x34,
            0x30, 0x32, 0x5f, 0x49, 0x50, 0x43, 0x5f, 0x4d, 0x45, 0x44, 0x49, 0x55, 0x4d, 0x5f,
            0x44, 0x45, 0x4e, 0x53, 0x49, 0x54, 0x59, 0x20, 0x00, 0x00, 0x00, 0x1f, 0x47, 0x45,
            0x4e, 0x45, 0x52, 0x49, 0x43, 0x5f, 0x4d, 0x4c, 0x43, 0x43, 0x5f, 0x43, 0x41, 0x50,
            0x5f, 0x30, 0x34, 0x30, 0x32, 0x5f, 0x49, 0x50, 0x43, 0x5f, 0x4d, 0x45, 0x44, 0x49,
            0x55, 0x29, 0x00, 0x00, 0x00, 0x28, 0x47, 0x45, 0x4e, 0x45, 0x52, 0x49, 0x43, 0x5f,
            0x4d, 0x4c, 0x43, 0x43, 0x5f, 0x43, 0x41, 0x50, 0x5f, 0x30, 0x36, 0x30, 0x33, 0x5f,
            0x49, 0x50, 0x43, 0x5f, 0x4d, 0x45, 0x44, 0x49, 0x55, 0x4d, 0x5f, 0x44, 0x45, 0x4e,
            0x53, 0x49, 0x54, 0x59, 0x20, 0x00, 0x00, 0x00, 0x1f, 0x47, 0x45, 0x4e, 0x45, 0x52,
            0x49, 0x43, 0x5f, 0x4d, 0x4c, 0x43, 0x43, 0x5f, 0x43, 0x41, 0x50, 0x5f, 0x30, 0x36,
            0x30, 0x33, 0x5f, 0x49, 0x50, 0x43, 0x5f, 0x4d, 0x45, 0x44, 0x49, 0x55, 0x29, 0x00,
            0x00, 0x00, 0x28, 0x47, 0x45, 0x4e, 0x45, 0x52, 0x49, 0x43, 0x5f, 0x4d, 0x4c, 0x43,
            0x43, 0x5f, 0x43, 0x41, 0x50, 0x5f, 0x30, 0x38, 0x30, 0x35, 0x5f, 0x49, 0x50, 0x43,
            0x5f, 0x4d, 0x45, 0x44, 0x49, 0x55, 0x4d, 0x5f, 0x44, 0x45, 0x4e, 0x53, 0x49, 0x54,
            0x59, 0x20, 0x00, 0x00, 0x00, 0x1f, 0x47, 0x45, 0x4e, 0x45, 0x52, 0x49, 0x43, 0x5f,
            0x4d, 0x4c, 0x43, 0x43, 0x5f, 0x43, 0x41, 0x50, 0x5f, 0x30, 0x38, 0x30, 0x35, 0x5f,
            0x49, 0x50, 0x43, 0x5f, 0x4d, 0x45, 0x44, 0x49, 0x55, 0x29, 0x00, 0x00, 0x00, 0x28,
            0x47, 0x45, 0x4e, 0x45, 0x52, 0x49, 0x43, 0x5f, 0x4d, 0x4c, 0x43, 0x43, 0x5f, 0x43,
            0x41, 0x50, 0x5f, 0x31, 0x32, 0x30, 0x36, 0x5f, 0x49, 0x50, 0x43, 0x5f, 0x4d, 0x45,
            0x44, 0x49, 0x55, 0x4d, 0x5f, 0x44, 0x45, 0x4e, 0x53, 0x49, 0x54, 0x59, 0x20, 0x00,
            0x00, 0x00, 0x1f, 0x47, 0x45, 0x4e, 0x45, 0x52, 0x49, 0x43, 0x5f, 0x4d, 0x4c, 0x43,
            0x43, 0x5f, 0x43, 0x41, 0x50, 0x5f, 0x31, 0x32, 0x30, 0x36, 0x5f, 0x49, 0x50, 0x43,
            0x5f, 0x4d, 0x45, 0x44, 0x49, 0x55,
        ];
        let pairs: Vec<(String, String)> = [
            (
                "GENERIC_MLCC_CAP_0402_IPC_MEDIUM_DENSITY",
                "GENERIC_MLCC_CAP_0402_IPC_MEDIU",
            ),
            (
                "GENERIC_MLCC_CAP_0603_IPC_MEDIUM_DENSITY",
                "GENERIC_MLCC_CAP_0603_IPC_MEDIU",
            ),
            (
                "GENERIC_MLCC_CAP_0805_IPC_MEDIUM_DENSITY",
                "GENERIC_MLCC_CAP_0805_IPC_MEDIU",
            ),
            (
                "GENERIC_MLCC_CAP_1206_IPC_MEDIUM_DENSITY",
                "GENERIC_MLCC_CAP_1206_IPC_MEDIU",
            ),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        assert_eq!(ALTIUM.len(), 328);
        assert_eq!(parse_pcblib_section_keys(ALTIUM), pairs);
        assert_eq!(
            encode_pcblib_section_keys(&pairs).unwrap().as_deref(),
            Some(ALTIUM)
        );
    }

    /// A non-Windows-1252 name travels as its wire bytes, exactly as the
    /// storage name and PATTERN carry it, so a reader recovers the same name
    /// from all three.
    #[test]
    fn pcblib_section_keys_carry_wire_bytes_and_survive_a_round_trip() {
        let pairs = vec![
            (
                to_wire_text("\u{7535}\u{963B}_A_MUCH_LONGER_FOOTPRINT_NAME"),
                to_wire_text("\u{7535}\u{963B}_A_MUCH_LONGER_FOO"),
            ),
            ("PLAIN/NAME".to_string(), "PLAIN_NAME".to_string()),
        ];
        let data = encode_pcblib_section_keys(&pairs).unwrap().unwrap();
        assert_eq!(parse_pcblib_section_keys(&data), pairs);
    }

    /// A stream cut mid-entry keeps the entries before the cut; a name past
    /// the 255-byte Pascal cap is refused at encode time rather than framed
    /// with a wrapped length.
    #[test]
    fn pcblib_section_keys_tolerate_a_cut_stream_and_refuse_an_overlong_name() {
        let pairs = vec![
            (
                "FIRST_NAME_PAST_THE_STORAGE_CAP_XX".to_string(),
                "FIRST_NAME_PAST_THE_STORAGE_CAP".to_string(),
            ),
            (
                "SECOND_NAME_PAST_THE_STORAGE_CAP_X".to_string(),
                "SECOND_NAME_PAST_THE_STORAGE_CA".to_string(),
            ),
        ];
        let data = encode_pcblib_section_keys(&pairs).unwrap().unwrap();
        let cut = &data[..data.len() - 10];
        assert_eq!(parse_pcblib_section_keys(cut), pairs[..1].to_vec());
        assert!(parse_pcblib_section_keys(&[]).is_empty());
        assert!(parse_pcblib_section_keys(&[1, 0, 0, 0, 9]).is_empty());

        let long = "X".repeat(256);
        let err = encode_pcblib_section_keys(&[(long, "X".repeat(31))])
            .expect_err("256 bytes do not fit a Pascal length byte");
        assert!(err.to_string().contains("255"), "{err}");
    }

    /// The text layout this crate wrote into a `PcbLib` before #507 still
    /// parses, so a library saved by an earlier release orders its truncated
    /// footprints correctly instead of appending them as orphans.
    #[test]
    fn pcblib_section_keys_still_read_the_pre_507_text_layout() {
        let pairs = vec![(
            "A_MUCH_LONGER_FOOTPRINT_NAME_THAN_OLE_ALLOWS".to_string(),
            "A_MUCH_LONGER_FOOTPRINT_NAME_TH".to_string(),
        )];
        let text = encode_schlib_section_keys(&pairs).unwrap();
        assert_eq!(parse_pcblib_section_keys(&text), pairs);
    }

    /// The walk names the first offending string by path, reads array
    /// indices as `[]`, and honours an exemption by bare key or by path.
    #[test]
    fn record_separator_path_names_the_offender_and_honours_exemptions() {
        use serde_json::json;

        let record = json!({
            "name": "clean",
            "items": [{ "label": "fine", "flags": "A | B" }, { "label": "bad|one" }],
            "pins": [{ "name": "p|q" }],
            "nested": { "deep": [["k", "v|w"]] },
        });
        assert_eq!(
            record_separator_path(&record, &["flags", "pins[]"]).as_deref(),
            Some("items[].label")
        );
        assert_eq!(
            record_separator_path(&record, &["flags", "pins[]", "items[].label"]).as_deref(),
            Some("nested.deep[][]")
        );
        assert_eq!(
            record_separator_path(&record, &["flags", "pins[]", "items[].label", "nested"]),
            None
        );
        // Without the exemptions the flag names and the pin are offenders too.
        assert_eq!(
            record_separator_path(&record, &[]).as_deref(),
            Some("items[].flags")
        );
    }
}
