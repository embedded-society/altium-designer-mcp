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
//! Both use Altium's *compressed-storage* framing (shared with the embedded
//! icon-image `/Storage` stream — see [`super::storage`] for the byte layout);
//! each entry is keyed by the pin ordinal as an ASCII-decimal Pascal string.
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
use super::storage;
use crate::altium::bytes::{read_i32_le, read_u32_le};

/// Upper bound on a single decompressed entry, guarding against a hostile or
/// corrupt stream. Both payload kinds are tiny (12 bytes / a short param block),
/// so 64 KiB is generous.
const MAX_ENTRY_DECOMPRESSED: usize = 64 * 1024;

/// Walks the compressed-storage entries after the header block, invoking
/// `on_entry(pin_index, decompressed_payload)` for each well-formed entry
/// whose Pascal-string key parses as a pin ordinal.
fn for_each_entry<F: FnMut(usize, &[u8])>(raw: &[u8], mut on_entry: F) {
    storage::for_each_entry(raw, MAX_ENTRY_DECOMPRESSED, |key, payload| {
        if let Ok(idx) = key.parse::<usize>() {
            on_entry(idx, payload);
        }
    });
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

    let mut out = storage::start_stream("PinFrac", entries.len());
    for (index, frac) in entries {
        let mut payload = Vec::with_capacity(12);
        payload.extend_from_slice(&frac.x.to_le_bytes());
        payload.extend_from_slice(&frac.y.to_le_bytes());
        payload.extend_from_slice(&frac.length.to_le_bytes());
        storage::write_entry(&mut out, &index.to_string(), &payload)?;
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

    let mut out = storage::start_stream("PinSymbolLineWidth", entries.len());
    for (index, width) in entries {
        let payload = encode_unicode_param_block(&format!("|SYMBOL_LINEWIDTH={width}"));
        storage::write_entry(&mut out, &index.to_string(), &payload)?;
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
        let stream = storage::start_stream("PinFrac", 2);
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
