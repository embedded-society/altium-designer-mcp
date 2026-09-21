//! Per-component pin auxiliary OLE streams (`PinFrac`, `PinSymbolLineWidth`,
//! `PinWideText`).
//!
//! Alongside a symbol's `Data` stream, Altium may store optional sibling
//! streams that carry data the binary pin record cannot hold:
//!
//! - **`PinFrac`** — the fractional part of each off-grid pin's `X` / `Y` /
//!   `length`. The binary pin record stores only the integer DXP part (`i16`),
//!   so a pin sitting between grid points keeps its sub-unit remainder here.
//! - **`PinSymbolLineWidth`** — a `SYMBOL_LINEWIDTH=N` parameter per pin whose
//!   symbol line width is non-default.
//! - **`PinWideText`** — a `|NAME=<text>` Unicode parameter block per pin
//!   whose name leaves ASCII: the name's authoritative wide form, since the
//!   binary record narrows it through the writing machine's ANSI code page.
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

/// Encodes the `PinWideText` stream: one entry per pin whose name leaves
/// ASCII, keyed by pin ordinal, payload `[u32 len][UTF-16LE "|NAME=<name>"]`.
///
/// This stream is the pin name's authoritative wide form. The binary pin
/// record narrows the name through the writing machine's ANSI code page, so a
/// name typed as real Unicode in the AD UI survives only here — which is the
/// stream's whole purpose. We hold real Unicode in memory and write it as real
/// UTF-16; the golden's own entries instead carry the ANSI-widened form of the
/// name's UTF-8 bytes, because its script-authored pins were mangled to
/// exactly that before Altium stored them (see `apply_pin_wide_text` for how
/// both shapes read back).
///
/// Only `NAME` is emitted: it is the only key the golden's 52 streams carry,
/// and inventing sibling keys (a designator, say) without evidence would write
/// fiction into every i18n library.
pub(super) fn encode_pin_wide_text(
    pins: &[Pin],
) -> crate::altium::error::AltiumResult<Option<Vec<u8>>> {
    let entries: Vec<(usize, &str)> = pins
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.name.is_ascii())
        .map(|(i, p)| (i, p.name.as_str()))
        .collect();
    if entries.is_empty() {
        return Ok(None);
    }

    let mut out = storage::start_stream("PinWideText", entries.len());
    for (index, name) in entries {
        let payload = encode_unicode_param_block(&format!("|NAME={name}"));
        storage::write_entry(&mut out, &index.to_string(), &payload)?;
    }
    Ok(Some(out))
}

/// Applies a parsed `PinWideText` stream onto `pins`, keyed by pin ordinal.
///
/// The stream's value wins only when it genuinely knows more than the binary
/// record did:
///
/// - A record that carried the name's raw UTF-8 bytes already decoded to the
///   real name (the golden's case), so a non-ASCII in-memory name is kept.
/// - A record the ANSI narrowing reduced to `?`s leaves an ASCII husk, and the
///   wide value replaces it — the UI-authored case the stream exists for.
/// - A wide value that is itself the ANSI-widened form of UTF-8 bytes (the
///   golden again) is folded back to the real name first, so applying it can
///   only ever improve the husk, never install mojibake.
pub(super) fn apply_pin_wide_text(pins: &mut [Pin], raw: &[u8]) {
    for_each_entry(raw, |idx, payload| {
        let Some(text) = decode_unicode_param_block(payload) else {
            return;
        };
        let params = crate::altium::parse_pipe_params(&text);
        let Some(wide) = params.get("name") else {
            return;
        };
        let Some(pin) = pins.get_mut(idx) else {
            return;
        };
        if !pin.name.is_ascii() {
            // The record already yielded a real (or at least non-degenerate)
            // name; the wide copy adds nothing.
            return;
        }
        // Fold a widened-bytes value back to the real name where some ANSI
        // code page provably widened it (the authoring locale's — Windows-1250
        // for the golden). A real Unicode value folds through none of them and
        // is applied verbatim.
        let resolved = crate::altium::fold_ansi_widened(wide).unwrap_or_else(|| wide.clone());
        if !resolved.is_empty() && resolved != pin.name {
            pin.name = resolved;
        }
    });
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
    if end > payload.len() || !inner_len.is_multiple_of(2) {
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

    // ==================== aux-stream skip paths ==============================
    //
    // Every aux stream is keyed by pin ordinal, so a malformed entry must be
    // skipped on its own rather than shifting the entries after it onto the
    // wrong pins — or, worse, panicking on a library that Altium opens fine.

    /// Builds a real aux stream — compressed-storage entries keyed by the pin
    /// ordinal — so a test can hand one malformed payload to a real applier
    /// through the same path a library takes.
    fn aux_stream(entries: &[(usize, Vec<u8>)]) -> Vec<u8> {
        let mut out = storage::start_stream("Test", entries.len());
        for (idx, payload) in entries {
            storage::write_entry(&mut out, &idx.to_string(), payload)
                .expect("test entry should write");
        }
        out
    }

    #[test]
    fn a_frac_entry_too_short_to_hold_three_coordinates_is_skipped() {
        // A partial entry would otherwise read whichever bytes followed it as
        // a coordinate, nudging the pin off-grid by an arbitrary amount.
        let mut pins = vec![pin()];
        apply_pin_frac(&mut pins, &aux_stream(&[(0, vec![0_u8; 11])]));
        assert!(pins[0].frac.is_none(), "a short entry must not apply");

        // An entry naming a pin the symbol does not have.
        let mut pins = vec![pin()];
        let mut payload = 5_i32.to_le_bytes().to_vec();
        payload.extend_from_slice(&5_i32.to_le_bytes());
        payload.extend_from_slice(&5_i32.to_le_bytes());
        apply_pin_frac(&mut pins, &aux_stream(&[(9, payload.clone())]));
        assert!(
            pins[0].frac.is_none(),
            "an out-of-range ordinal must not apply"
        );

        // A well-formed entry does apply, and an all-zero one reads as "no
        // fractional part" rather than as a zero offset.
        let mut pins = vec![pin()];
        apply_pin_frac(&mut pins, &aux_stream(&[(0, payload)]));
        assert!(pins[0].frac.is_some());

        let mut pins = vec![pin()];
        apply_pin_frac(&mut pins, &aux_stream(&[(0, vec![0_u8; 12])]));
        assert!(pins[0].frac.is_none(), "an all-zero frac is no frac");
    }

    #[test]
    fn a_malformed_unicode_block_is_skipped_by_both_readers() {
        // Both the wide-text and line-width streams carry a
        // `[u32 len][utf16le]` block; a length that overruns, or an odd byte
        // count that cannot be UTF-16, must drop the entry rather than panic.
        let bad_blocks: [Vec<u8>; 3] = [
            999_u32.to_le_bytes().to_vec(), // length past the payload
            {
                let mut v = 3_u32.to_le_bytes().to_vec(); // odd inner length
                v.extend_from_slice(&[0, 0, 0]);
                v
            },
            vec![1, 2], // too short for the prefix
        ];

        for block in bad_blocks {
            let mut pins = vec![pin()];
            apply_pin_wide_text(&mut pins, &aux_stream(&[(0, block.clone())]));
            assert_eq!(pins[0].name, "A", "wide text applied a malformed block");

            let mut pins = vec![pin()];
            let before = pins[0].symbol_line_width;
            apply_pin_symbol_line_widths(&mut pins, &aux_stream(&[(0, block)]));
            assert_eq!(pins[0].symbol_line_width, before);
        }
    }

    #[test]
    fn a_well_formed_block_missing_its_key_is_skipped() {
        // The block decodes, but carries no `name` / `symbol_linewidth` key —
        // there is nothing to apply, and the pin keeps what the record gave it.
        let named = |text: &str| encode_unicode_param_block(text);

        let mut pins = vec![pin()];
        apply_pin_wide_text(&mut pins, &aux_stream(&[(0, named("|nothing=useful"))]));
        assert_eq!(pins[0].name, "A");

        let mut pins = vec![pin()];
        let before = pins[0].symbol_line_width;
        apply_pin_symbol_line_widths(&mut pins, &aux_stream(&[(0, named("|nothing=useful"))]));
        assert_eq!(pins[0].symbol_line_width, before);

        // A non-numeric width is not a width.
        let mut pins = vec![pin()];
        apply_pin_symbol_line_widths(
            &mut pins,
            &aux_stream(&[(0, named("|SYMBOL_LINEWIDTH=wide"))]),
        );
        assert_eq!(pins[0].symbol_line_width, before);

        // And an ordinal past the end of the pin list.
        let mut pins = vec![pin()];
        apply_pin_symbol_line_widths(&mut pins, &aux_stream(&[(9, named("|SYMBOL_LINEWIDTH=2"))]));
        assert_eq!(pins[0].symbol_line_width, before);
    }

    #[test]
    fn ascii_pin_names_emit_no_wide_text() {
        let pins = vec![pin(), pin()];
        assert!(encode_pin_wide_text(&pins).unwrap().is_none());
    }

    #[test]
    fn wide_text_round_trips_a_real_unicode_name() {
        // The UI-authored case the stream exists for: the binary record can
        // only hold the ANSI narrowing (a `?` husk), so the wide stream is the
        // sole carrier of the real name.
        let mut authored = pin();
        authored.name = "\u{7535}\u{963B}".to_string(); // 电阻
        let raw = encode_pin_wide_text(&[pin(), authored])
            .unwrap()
            .expect("non-ASCII name emits the stream");

        let mut read_back = vec![pin(), pin()];
        read_back[1].name = "??".to_string(); // the record's ANSI husk
        apply_pin_wide_text(&mut read_back, &raw);
        assert_eq!(read_back[1].name, "\u{7535}\u{963B}");
        assert_eq!(read_back[0].name, "A", "pin without an entry is untouched");
    }

    #[test]
    fn wide_text_never_overwrites_a_recovered_name() {
        // The golden's case: the record carried raw UTF-8 bytes and already
        // decoded to the real name; the wide copy (whatever its locale shape)
        // must not replace it.
        let mut authored = pin();
        authored.name = "\u{7535}\u{963B}".to_string();
        let raw = encode_pin_wide_text(std::slice::from_ref(&authored))
            .unwrap()
            .unwrap();

        let mut pins = vec![authored.clone()];
        apply_pin_wide_text(&mut pins, &raw);
        assert_eq!(pins[0].name, authored.name);
    }

    #[test]
    fn widened_wide_text_is_folded_back_to_the_real_name() {
        // An Altium-authored stream can carry the ANSI-widened form of the
        // name's UTF-8 bytes (the golden's 52 streams all do). Applied onto a
        // husk, it must resolve to the real name, not install the mojibake.
        let widened = crate::altium::encode_utf8_param_value("\u{7535}\u{963B}");
        let mut out = storage::start_stream("PinWideText", 1);
        let payload = encode_unicode_param_block(&format!("|NAME={widened}"));
        storage::write_entry(&mut out, "0", &payload).unwrap();

        let mut pins = vec![pin()];
        pins[0].name = "??".to_string();
        apply_pin_wide_text(&mut pins, &out);
        assert_eq!(pins[0].name, "\u{7535}\u{963B}");
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
    /// A wide-text entry whose index names no pin is ignored: a damaged
    /// aux stream must not touch the pins it cannot address.
    #[test]
    fn a_wide_text_entry_with_an_out_of_range_index_is_ignored() {
        let mut pins = vec![pin()];
        let raw = aux_stream(&[(7, encode_unicode_param_block("|NAME=GHOST"))]);
        apply_pin_wide_text(&mut pins, &raw);
        assert_eq!(pins[0].name, "A", "the one real pin stays untouched");
    }
}
