//! Per-component pin auxiliary OLE streams (`PinFrac`, `PinSymbolLineWidth`).
//!
//! Alongside a symbol's `Data` stream, Altium may store two optional sibling
//! streams that carry data the binary pin record cannot hold:
//!
//! - **`PinFrac`** — the fractional part of each off-grid pin's `X` / `Y` /
//!   `length`. The binary pin record stores only the integer DXP part (`i16`),
//!   so a pin sitting between grid points keeps its sub-unit remainder here.
//! - **`PinSymbolLineWidth`** — a `SYMBOL_LINEWIDTH=N` parameter per pin whose
//!   symbol line width is non-default.
//!
//! Both share Altium's *compressed-storage* framing (the same layout used for
//! embedded icon images):
//!
//! ```text
//! [u32 LE header_len][header_len header bytes]         # C-string param block
//! then, per non-default pin:
//!   [u32 LE size]        # low 24 bits = block size, high byte = 0x01 flag
//!   0xD0                 # storage-entry tag
//!   [u8 name_len][name]  # Pascal string: the pin ordinal as ASCII decimal
//!   [u32 LE comp_len][comp_len bytes]   # zlib-compressed payload
//! ```
//!
//! The compressed payload differs per stream:
//! - `PinFrac`: 12 bytes = three little-endian `i32` (`frac_x`, `frac_y`,
//!   `frac_length`).
//! - `PinSymbolLineWidth`: a Unicode parameter block
//!   (`[u32 LE inner_len][UTF-16LE "|SYMBOL_LINEWIDTH=N"]`).
//!
//! # Byte-identity note
//!
//! A symbol whose pins are all on-grid with default line width emits **neither**
//! stream (the entry maps are empty), so its storage is byte-identical to
//! Altium's — this is the load-bearing invariant the golden library exercises.
//! For a NON-default pin there is no golden fixture, so the compressed bytes we
//! emit are only verified by a self round-trip (we control both compress and
//! decompress); zlib's DEFLATE output is implementation-specific, so the exact
//! bytes may differ from Altium's writer even though the framing matches. Any
//! genuinely Altium-authored stream still *reads* correctly (zlib inflate is
//! standardised), so round-tripping a real off-grid pin is lossless.

use super::primitives::{Pin, PinFrac};
use crate::altium::bytes::{read_i32_le, read_u32_le};
use crate::altium::framing::write_cstring_param_block;

/// The storage-entry tag byte Altium writes before each compressed entry.
const ENTRY_TAG: u8 = 0xD0;

/// Flag byte OR-ed into the high byte of each entry's 24-bit size word.
const ENTRY_SIZE_FLAG: u32 = 0x0100_0000;

/// Upper bound on a single decompressed entry, guarding against a hostile or
/// corrupt stream. Both payload kinds are tiny (12 bytes / a short param block),
/// so 64 KiB is generous.
const MAX_ENTRY_DECOMPRESSED: usize = 64 * 1024;

/// Compresses `payload` with a zlib (RFC 1950) wrapper, matching the reader's
/// inflate. Uses `flate2`'s default compression, exactly like the `PcbLib`
/// model-data compressor (`compress_model_data`).
fn zlib_compress(payload: &[u8]) -> crate::altium::error::AltiumResult<Vec<u8>> {
    use crate::altium::error::AltiumError;
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write as _;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(payload)
        .map_err(|e| AltiumError::compression_error("Failed to compress pin aux entry", Some(e)))?;
    encoder.finish().map_err(|e| {
        AltiumError::compression_error("Failed to finish pin aux compression", Some(e))
    })
}

/// Decompresses a zlib entry, rejecting output larger than
/// [`MAX_ENTRY_DECOMPRESSED`]. Returns `None` on any error (a corrupt entry is
/// skipped rather than failing the whole read).
fn zlib_decompress(data: &[u8]) -> Option<Vec<u8>> {
    use flate2::read::ZlibDecoder;
    use std::io::Read as _;

    let limit = MAX_ENTRY_DECOMPRESSED.saturating_add(1) as u64;
    let mut decoder = ZlibDecoder::new(data).take(limit);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).ok()?;
    if out.len() > MAX_ENTRY_DECOMPRESSED {
        return None;
    }
    Some(out)
}

/// Appends one compressed-storage entry (`0xD0` tag + Pascal-string key +
/// zlib-compressed payload) for pin ordinal `index`.
fn write_entry(
    out: &mut Vec<u8>,
    index: usize,
    payload: &[u8],
) -> crate::altium::error::AltiumResult<()> {
    use crate::altium::error::AltiumError;

    let compressed = zlib_compress(payload)?;
    let name = index.to_string();
    let name_bytes = name.as_bytes();

    // block_size = tag(1) + name_len(1) + name(N) + comp_len(4) + compressed(N)
    let block_size = 1 + 1 + name_bytes.len() + 4 + compressed.len();
    if block_size > 0x00FF_FFFF {
        return Err(AltiumError::InvalidParameter {
            name: "pin_aux".to_string(),
            message: format!("pin aux entry {index} is too large ({block_size} bytes)"),
        });
    }

    #[allow(clippy::cast_possible_truncation)] // bounded above
    let size_word = (block_size as u32) | ENTRY_SIZE_FLAG;
    out.extend_from_slice(&size_word.to_le_bytes());
    out.push(ENTRY_TAG);
    #[allow(clippy::cast_possible_truncation)] // a pin ordinal never needs >255 ASCII digits
    out.push(name_bytes.len() as u8);
    out.extend_from_slice(name_bytes);
    #[allow(clippy::cast_possible_truncation)] // bounded by block_size guard above
    out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    out.extend_from_slice(&compressed);
    Ok(())
}

/// Walks the compressed-storage entries after the header block, invoking
/// `on_entry(pin_index, decompressed_payload)` for each well-formed entry.
///
/// Mirrors `AltiumSharp`'s parse loop: read the header length prefix and skip it,
/// then read `[u32 size][0xD0][pascal key][u32 comp_len][comp]` entries until
/// the stream is exhausted or a malformed entry is hit (which stops the walk,
/// matching `AltiumSharp`'s `break`).
fn for_each_entry<F: FnMut(usize, &[u8])>(raw: &[u8], mut on_entry: F) {
    // Header block: [u32 LE len][len bytes]. Skip it.
    let Some(header_len) = read_u32_le(raw, 0) else {
        return;
    };
    let mut offset = 4 + header_len as usize;

    while offset + 4 <= raw.len() {
        let Some(size_word) = read_u32_le(raw, offset) else {
            break;
        };
        let block_size = (size_word & 0x00FF_FFFF) as usize;
        if block_size == 0 {
            break;
        }
        let block_start = offset + 4;
        let block_end = block_start + block_size;
        if block_end > raw.len() {
            break;
        }
        let block = &raw[block_start..block_end];

        // 0xD0 tag
        if block.first().copied() != Some(ENTRY_TAG) {
            break;
        }
        // Pascal string: pin ordinal as ASCII decimal.
        let (key, after_key) = crate::altium::framing::read_pascal_string(block, 1);
        // Compressed data: [u32 LE comp_len][comp bytes].
        if let (Some(comp_len), Ok(idx)) = (read_u32_le(block, after_key), key.parse::<usize>()) {
            let comp_start = after_key + 4;
            let comp_end = comp_start + comp_len as usize;
            if comp_end <= block.len() {
                if let Some(payload) = zlib_decompress(&block[comp_start..comp_end]) {
                    on_entry(idx, &payload);
                }
            }
        }

        offset = block_end;
    }
}

/// Writes the shared header block (`|HEADER=<name>|Weight=<count>`), matching
/// Altium's mixed-case keys, then returns the buffer ready for entries.
fn start_stream(header_name: &str, count: usize) -> Vec<u8> {
    let text = format!("|HEADER={header_name}|Weight={count}");
    let mut out = Vec::new();
    write_cstring_param_block(&mut out, &crate::altium::encode_windows1252(&text));
    out
}

/// Encodes the `PinFrac` stream for `pins`, or `None` when every pin is on-grid
/// (no fractional parts) — in which case Altium writes no stream and the
/// storage stays byte-identical to the golden.
///
/// # Errors
///
/// Returns an error if an entry's compressed payload exceeds the 24-bit block
/// size (never in practice — each payload is 12 bytes).
pub(super) fn encode_pin_frac(pins: &[Pin]) -> crate::altium::error::AltiumResult<Option<Vec<u8>>> {
    let entries: Vec<(usize, PinFrac)> = pins
        .iter()
        .enumerate()
        .filter_map(|(i, p)| match p.frac {
            Some(f) if !f.is_zero() => Some((i, f)),
            _ => None,
        })
        .collect();
    if entries.is_empty() {
        return Ok(None);
    }

    let mut out = start_stream("PinFrac", entries.len());
    for (index, frac) in entries {
        let mut payload = Vec::with_capacity(12);
        payload.extend_from_slice(&frac.x.to_le_bytes());
        payload.extend_from_slice(&frac.y.to_le_bytes());
        payload.extend_from_slice(&frac.length.to_le_bytes());
        write_entry(&mut out, index, &payload)?;
    }
    Ok(Some(out))
}

/// Encodes the `PinSymbolLineWidth` stream for `pins`, or `None` when every pin
/// has the default (zero) width — matching Altium's omit-when-default.
///
/// # Errors
///
/// Returns an error if an entry's compressed payload exceeds the 24-bit block
/// size (never in practice — each payload is a short param block).
pub(super) fn encode_pin_symbol_line_widths(
    pins: &[Pin],
) -> crate::altium::error::AltiumResult<Option<Vec<u8>>> {
    let entries: Vec<(usize, i32)> = pins
        .iter()
        .enumerate()
        .filter(|(_, p)| p.symbol_line_width != 0)
        .map(|(i, p)| (i, p.symbol_line_width))
        .collect();
    if entries.is_empty() {
        return Ok(None);
    }

    let mut out = start_stream("PinSymbolLineWidth", entries.len());
    for (index, width) in entries {
        let payload = encode_unicode_param_block(&format!("|SYMBOL_LINEWIDTH={width}"));
        write_entry(&mut out, index, &payload)?;
    }
    Ok(Some(out))
}

/// Encodes a Unicode (UTF-16LE) parameter block: `[u32 LE byte_len][utf16le]`.
/// The length counts the UTF-16 byte count (not including its own 4 bytes),
/// matching `AltiumSharp`'s `WriteUnicodeParameterBlock` / `ReadUnicodeParameterBlock`.
fn encode_unicode_param_block(text: &str) -> Vec<u8> {
    let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = Vec::with_capacity(4 + utf16.len());
    #[allow(clippy::cast_possible_truncation)] // a SYMBOL_LINEWIDTH param block is tiny
    out.extend_from_slice(&(utf16.len() as u32).to_le_bytes());
    out.extend_from_slice(&utf16);
    out
}

/// Applies a parsed `PinFrac` stream onto `pins`, keyed by pin ordinal.
pub(super) fn apply_pin_frac(pins: &mut [Pin], raw: &[u8]) {
    for_each_entry(raw, |idx, payload| {
        if payload.len() < 12 {
            return;
        }
        let (Some(x), Some(y), Some(length)) = (
            read_i32_le(payload, 0),
            read_i32_le(payload, 4),
            read_i32_le(payload, 8),
        ) else {
            return;
        };
        if let Some(pin) = pins.get_mut(idx) {
            let frac = PinFrac { x, y, length };
            pin.frac = if frac.is_zero() { None } else { Some(frac) };
        }
    });
}

/// Applies a parsed `PinSymbolLineWidth` stream onto `pins`, keyed by pin ordinal.
pub(super) fn apply_pin_symbol_line_widths(pins: &mut [Pin], raw: &[u8]) {
    for_each_entry(raw, |idx, payload| {
        let Some(text) = decode_unicode_param_block(payload) else {
            return;
        };
        let params = crate::altium::parse_pipe_params(&text);
        if let Some(width) = params
            .get("symbol_linewidth")
            .and_then(|s| s.trim().parse::<i32>().ok())
        {
            if let Some(pin) = pins.get_mut(idx) {
                pin.symbol_line_width = width;
            }
        }
    });
}

/// Decodes a Unicode parameter block written by [`encode_unicode_param_block`].
/// Returns `None` if the length prefix or the UTF-16 payload is malformed.
fn decode_unicode_param_block(payload: &[u8]) -> Option<String> {
    let inner_len = read_u32_le(payload, 0)? as usize;
    let start = 4usize;
    let end = start.checked_add(inner_len)?;
    if end > payload.len() || inner_len % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = payload[start..end]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::altium::schlib::primitives::PinOrientation;

    fn pin() -> Pin {
        Pin::new("A", "1", 0, 0, 10, PinOrientation::Right)
    }

    #[test]
    fn all_default_pins_emit_no_streams() {
        // The load-bearing byte-identity invariant: on-grid, default-width pins
        // produce no aux streams at all (matching the golden library).
        let pins = vec![pin(), pin()];
        assert!(encode_pin_frac(&pins).unwrap().is_none());
        assert!(encode_pin_symbol_line_widths(&pins).unwrap().is_none());
    }

    #[test]
    fn pin_frac_self_round_trips() {
        let mut pins = vec![pin(), pin(), pin()];
        pins[1].frac = Some(PinFrac {
            x: 50_000,
            y: -25_000,
            length: 12_345,
        });
        let stream = encode_pin_frac(&pins)
            .unwrap()
            .expect("a fractional pin must emit a PinFrac stream");

        let mut read_back = vec![pin(), pin(), pin()];
        apply_pin_frac(&mut read_back, &stream);
        assert_eq!(read_back[0].frac, None, "on-grid pin 0 stays None");
        assert_eq!(
            read_back[1].frac,
            Some(PinFrac {
                x: 50_000,
                y: -25_000,
                length: 12_345,
            }),
            "fractional pin 1 survives the round-trip keyed by ordinal"
        );
        assert_eq!(read_back[2].frac, None, "on-grid pin 2 stays None");
    }

    #[test]
    fn pin_symbol_line_width_self_round_trips() {
        let mut pins = vec![pin(), pin()];
        pins[0].symbol_line_width = 3;
        let stream = encode_pin_symbol_line_widths(&pins)
            .unwrap()
            .expect("a non-default width must emit a PinSymbolLineWidth stream");

        let mut read_back = vec![pin(), pin()];
        apply_pin_symbol_line_widths(&mut read_back, &stream);
        assert_eq!(
            read_back[0].symbol_line_width, 3,
            "width survives round-trip"
        );
        assert_eq!(read_back[1].symbol_line_width, 0, "default pin stays 0");
    }

    #[test]
    fn header_uses_altium_mixed_case_keys() {
        let stream = start_stream("PinFrac", 2);
        let text = String::from_utf8_lossy(&stream);
        assert!(
            text.contains("|HEADER=PinFrac"),
            "HEADER key present: {text}"
        );
        assert!(text.contains("|Weight=2"), "mixed-case Weight key: {text}");
    }

    #[test]
    fn unicode_param_block_round_trips() {
        let block = encode_unicode_param_block("|SYMBOL_LINEWIDTH=5");
        assert_eq!(
            decode_unicode_param_block(&block).as_deref(),
            Some("|SYMBOL_LINEWIDTH=5")
        );
    }

    #[test]
    fn corrupt_stream_is_ignored_not_panicked() {
        // A truncated / garbage stream must not panic; unknown entries are skipped.
        let mut pins = vec![pin()];
        apply_pin_frac(&mut pins, &[0x00, 0x00]); // too short for even a header
        apply_pin_symbol_line_widths(&mut pins, &[0xFF; 8]);
        assert_eq!(pins[0].frac, None);
        assert_eq!(pins[0].symbol_line_width, 0);
    }
}
