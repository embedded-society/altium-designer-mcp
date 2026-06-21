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

pub(crate) mod bytes;
pub mod error;
pub(crate) mod framing;
pub mod pcblib;
pub mod schlib;
pub(crate) mod serde_round;

pub use error::{AltiumError, AltiumResult};
pub use pcblib::{Footprint, PcbLib};
pub use schlib::{SchLib, Symbol};

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{BuildHasher, Hash, Hasher};

/// Maximum length for OLE Compound File storage/stream names.
///
/// OLE Compound Document format limits entry names to 31 UTF-16 code units.
/// We enforce 31 bytes for ASCII compatibility.
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

/// Generates a safe OLE storage name for a component.
///
/// OLE Compound File names are limited to 31 characters. This function:
/// - Returns the name as-is if it fits within the limit
/// - Truncates longer names and adds a unique suffix to avoid collisions
///
/// # Arguments
///
/// * `name` - The full component name
/// * `used_names` - Set of OLE names already in use (to avoid collisions)
///
/// # Returns
///
/// A safe OLE name (≤31 chars) that doesn't collide with existing names.
#[must_use]
pub fn generate_ole_name<S: BuildHasher>(name: &str, used_names: &HashSet<String, S>) -> String {
    // OLE/CFB storage names cannot contain the path separator '/'; a component
    // named e.g. "A/B" would otherwise make the storage-creation call fail.
    // Altium sanitises it to '_' before creating the component storage, so a
    // component whose name carries a slash still saves. Apply it up front so
    // both the short-name and truncated paths use the sanitised form.
    let sanitized = name.replace('/', "_");
    let name = sanitized.as_str();

    if name.len() <= MAX_OLE_NAME_LEN && !used_names.contains(name) {
        return name.to_string();
    }

    // Need to truncate - use format: "{prefix}~{suffix}"
    let max_prefix_len = MAX_OLE_NAME_LEN - SUFFIX_LEN;

    // Truncate to max prefix length, respecting char boundaries
    let prefix: String = name.chars().take(max_prefix_len).collect();
    let prefix = if prefix.len() > max_prefix_len {
        // If multi-byte chars, truncate further
        prefix.chars().take(max_prefix_len - 1).collect()
    } else {
        prefix
    };

    // Find a unique suffix
    for i in 1..1000 {
        let candidate = format!("{prefix}~{i:03}");
        if !used_names.contains(&candidate) {
            return candidate;
        }
    }

    // Fallback: use hash-based suffix (extremely unlikely to reach here)
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();
    format!(
        "{}~{:03X}",
        &prefix[..prefix.len().saturating_sub(1)],
        hash & 0xFFF
    )
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let used = HashSet::new();
        let name = "VERY_LONG_COMPONENT_NAME_THAT_EXCEEDS_LIMIT";
        let result = generate_ole_name(name, &used);
        assert!(result.len() <= MAX_OLE_NAME_LEN);
        assert!(result.starts_with("VERY_LONG_COMPONENT_NAME_TH"));
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
}
