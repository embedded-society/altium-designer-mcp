//! Altium *compressed-storage* framing and the library-level `/Storage` stream.
//!
//! Two `SchLib` stream families share one byte layout: the per-component pin
//! auxiliary streams (`PinFrac`, `PinSymbolLineWidth` — see [`super::pin_aux`])
//! and the root `/Storage` stream that carries the raw bytes of every embedded
//! image (`RECORD=30` with `EmbedImage=T`). The shared framing is:
//!
//! ```text
//! [u32 LE header_len][header_len header bytes]         # C-string param block
//! then, per entry:
//!   [u32 LE size]        # low 24 bits = block size, high byte = 0x01 flag
//!   0xD0                 # storage-entry tag
//!   [u8 name_len][name]  # Pascal string entry key (Windows-1252)
//!   [u32 LE comp_len][comp_len bytes]   # zlib-compressed payload
//! ```
//!
//! The header param block is `|HEADER=<name>` plus `|Weight=<count>` (Altium's
//! mixed-case key) when at least one entry follows. An **empty** `/Storage`
//! stream carries the bare `|HEADER=Icon storage` block with **no** `Weight`
//! key — the byte-identical from-scratch output the readability oracle guards.
//!
//! # `/Storage` entry naming and matching
//!
//! Real AD24 names each `/Storage` entry with the image's **full file path**
//! (the record's `FileName` value); `AltiumSharp`'s own writer uses the
//! zero-based index instead. We follow real AD24 on write. On read the entry
//! names are ignored entirely: payloads are matched to `EmbedImage=T` images
//! **in order across all symbols**, exactly like `AltiumSharp`'s
//! `ParseStorageImageData`.
//!
//! # Byte-identity note
//!
//! zlib's DEFLATE output is implementation-specific, so a non-empty stream we
//! emit is verified by self round-trip + decompressed-equality (we control both
//! compress and decompress), not compressed-byte-identity — the same documented
//! caveat as the pin auxiliary streams. Any genuinely Altium-authored stream
//! still *reads* correctly (zlib inflate is standardised).

use crate::altium::bytes::read_u32_le;
use crate::altium::error::{AltiumError, AltiumResult};
use crate::altium::framing::write_cstring_param_block;
use crate::altium::inflate::{inflate_capped, Budget};

/// The storage-entry tag byte Altium writes before each compressed entry.
const ENTRY_TAG: u8 = 0xD0;

/// Flag byte OR-ed into the high byte of each entry's 24-bit size word.
const ENTRY_SIZE_FLAG: u32 = 0x0100_0000;

/// Upper bound on a single decompressed embedded image, guarding against a
/// hostile or corrupt `/Storage` stream. Symbol pictures are icons/logos, so
/// 16 MiB is generous.
const MAX_IMAGE_DECOMPRESSED: usize = 16 * 1024 * 1024;

/// Compresses `payload` with a zlib (RFC 1950) wrapper, matching the reader's
/// inflate. Uses `flate2`'s default compression, exactly like the `PcbLib`
/// model-data compressor (`compress_model_data`).
pub(super) fn zlib_compress(payload: &[u8]) -> AltiumResult<Vec<u8>> {
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write as _;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(payload)
        .map_err(|e| AltiumError::compression_error("Failed to compress storage entry", Some(e)))?;
    encoder.finish().map_err(|e| {
        AltiumError::compression_error("Failed to finish storage entry compression", Some(e))
    })
}

/// Appends one compressed-storage entry (`0xD0` tag + Pascal-string key +
/// zlib-compressed payload) named `name` (encoded Windows-1252).
pub(super) fn write_entry(out: &mut Vec<u8>, name: &str, payload: &[u8]) -> AltiumResult<()> {
    write_entry_with(out, name, payload, None)
}

/// [`write_entry`], taking `compressed` — the entry's zlib bytes as read —
/// in place of this crate's own compression while it still inflates to
/// `payload`, so an unchanged entry keeps Altium's bytes.
pub(super) fn write_entry_with(
    out: &mut Vec<u8>,
    name: &str,
    payload: &[u8],
    compressed: Option<&[u8]>,
) -> AltiumResult<()> {
    let compressed = match compressed {
        Some(read) if inflate_capped(read, payload.len()).as_deref() == Some(payload) => {
            read.to_vec()
        }
        _ => zlib_compress(payload)?,
    };
    let name_bytes = crate::altium::encode_windows1252(name);
    if name_bytes.len() > 255 {
        return Err(AltiumError::InvalidParameter {
            name: "storage".to_string(),
            message: format!(
                "storage entry name {name:?} is too long ({} bytes; max 255)",
                name_bytes.len()
            ),
        });
    }

    // block_size = tag(1) + name_len(1) + name(N) + comp_len(4) + compressed(N)
    let block_size = 1 + 1 + name_bytes.len() + 4 + compressed.len();
    if block_size > 0x00FF_FFFF {
        return Err(AltiumError::InvalidParameter {
            name: "storage".to_string(),
            message: format!("storage entry {name:?} is too large ({block_size} bytes)"),
        });
    }

    #[allow(clippy::cast_possible_truncation)] // bounded above
    let size_word = (block_size as u32) | ENTRY_SIZE_FLAG;
    out.extend_from_slice(&size_word.to_le_bytes());
    out.push(ENTRY_TAG);
    #[allow(clippy::cast_possible_truncation)] // bounded by the 255-byte guard above
    out.push(name_bytes.len() as u8);
    out.extend_from_slice(&name_bytes);
    #[allow(clippy::cast_possible_truncation)] // bounded by block_size guard above
    out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    out.extend_from_slice(&compressed);
    Ok(())
}

/// Walks the compressed-storage entries after the header block, invoking
/// `on_entry(entry_name, decompressed_payload, compressed_bytes)` for each
/// well-formed entry.
///
/// Mirrors `AltiumSharp`'s parse loop: read the header length prefix and skip
/// it, then read `[u32 size][0xD0][pascal key][u32 comp_len][comp]` entries
/// until the stream is exhausted or a malformed entry is hit (which stops the
/// walk with a debug log, matching `AltiumSharp`'s `break`). Entries whose
/// payload fails to inflate or exceeds `max_decompressed` are skipped.
///
/// # Errors
///
/// When the entries inflate past what the read's `budget` allows.
pub(super) fn for_each_entry<F: FnMut(&str, &[u8], &[u8])>(
    raw: &[u8],
    max_decompressed: usize,
    budget: &mut Budget,
    mut on_entry: F,
) -> AltiumResult<()> {
    // Header block: [u32 LE len][len bytes]. Skip it; an offset past the end of
    // memory (saturating) is past the end of the stream, where the walk stops.
    let Some(header_len) = read_u32_le(raw, 0) else {
        return Ok(());
    };
    let mut offset = (header_len as usize).saturating_add(4);

    // Each block is `[u32 size word][block]`; the walk ends at the first
    // offset without a whole size word.
    while let Some(size_word) = read_u32_le(raw, offset) {
        let block_size = (size_word & 0x00FF_FFFF) as usize;
        if block_size == 0 {
            break;
        }
        let block_start = offset + 4;
        let block_end = block_start + block_size;
        if block_end > raw.len() {
            tracing::debug!(offset, block_size, "truncated storage entry; stopping");
            break;
        }
        let block = &raw[block_start..block_end];

        // 0xD0 tag
        if block.first().copied() != Some(ENTRY_TAG) {
            tracing::debug!(offset, "storage entry without 0xD0 tag; stopping");
            break;
        }
        // Pascal string: the entry key.
        let (key, after_key) = crate::altium::framing::read_pascal_string(block, 1);
        // Compressed data: [u32 LE comp_len][comp bytes].
        if let Some(comp_len) = read_u32_le(block, after_key) {
            let comp_start = after_key + 4;
            let comp_end = comp_start.saturating_add(comp_len as usize);
            if comp_end <= block.len() {
                let compressed = &block[comp_start..comp_end];
                if let Some(payload) = budget.inflate(compressed, max_decompressed)? {
                    on_entry(&key, &payload, compressed);
                } else {
                    tracing::debug!(entry = %key, "skipping storage entry that failed to inflate");
                }
            }
        }

        offset = block_end;
    }
    Ok(())
}

/// Encodes the shared header block (`|HEADER=<name>|Weight=<count>`), matching
/// Altium's mixed-case keys, then returns the buffer ready for entries.
pub(super) fn start_stream(header_name: &str, count: usize) -> Vec<u8> {
    let text = format!("|HEADER={header_name}|Weight={count}");
    let mut out = Vec::new();
    write_cstring_param_block(&mut out, &crate::altium::encode_windows1252(&text));
    out
}

/// One embedded image's `/Storage` entry: its name, its bytes, and the
/// compressed bytes it was read with, reused while they still hold it.
pub(super) type IconEntry<'a> = (&'a str, &'a [u8], Option<&'a [u8]>);

/// Encodes the root `/Storage` stream for `entries`, one compressed entry per
/// embedded image in global symbol order.
///
/// With no entries the stream is the bare `|HEADER=Icon storage` param block —
/// byte-identical to the pre-embedded-image writer output (no `Weight` key),
/// which is the oracle-guarded from-scratch shape.
///
/// # Errors
///
/// Returns an error if an entry name exceeds 255 Windows-1252 bytes or a
/// compressed entry exceeds the 24-bit block size.
pub(super) fn encode_icon_storage(entries: &[IconEntry<'_>]) -> AltiumResult<Vec<u8>> {
    if entries.is_empty() {
        let mut out = Vec::new();
        write_cstring_param_block(&mut out, b"|HEADER=Icon storage");
        return Ok(out);
    }

    let mut out = start_stream("Icon storage", entries.len());
    for (name, payload, compressed) in entries {
        write_entry_with(&mut out, name, payload, *compressed)?;
    }
    Ok(out)
}

/// Parses the root `/Storage` stream, returning each entry's decompressed
/// image bytes in stream order. Entry names are ignored — `AltiumSharp`'s
/// reader matches payloads to `EmbedImage=T` images purely by order — and
/// malformed entries stop the walk.
///
/// # Errors
///
/// When the images inflate past what the read's `budget` allows.
pub(super) fn parse_icon_storage(
    raw: &[u8],
    budget: &mut Budget,
) -> AltiumResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut payloads = Vec::new();
    for_each_entry(
        raw,
        MAX_IMAGE_DECOMPRESSED,
        budget,
        |_name, payload, compressed| {
            payloads.push((payload.to_vec(), compressed.to_vec()));
        },
    )?;
    Ok(payloads)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== entry walking under corruption =====================
    //
    // `/Storage` payloads are matched to embedded images by ordinal, so an
    // entry silently mis-read does not just lose its own image — it shifts
    // every image after it onto the wrong bytes. The walk therefore stops at
    // the first malformed entry rather than trying to resynchronise.

    /// Wraps entry bytes in the header block the walker skips.
    fn storage_stream(entries: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        write_cstring_param_block(&mut out, b"|HEADER=Icon storage");
        out.extend_from_slice(entries);
        out
    }

    /// Collects the entries a walk yields.
    fn walk(raw: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut seen = Vec::new();
        for_each_entry(
            raw,
            MAX_IMAGE_DECOMPRESSED,
            &mut Budget::default(),
            |name, payload, _| {
                seen.push((name.to_string(), payload.to_vec()));
            },
        )
        .expect("within budget");
        seen
    }

    #[test]
    fn a_well_formed_entry_round_trips_through_the_walker() {
        let mut entries = Vec::new();
        write_entry(&mut entries, "logo.bmp", b"BMPBYTES").expect("entry should write");
        let seen = walk(&storage_stream(&entries));
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "logo.bmp");
        assert_eq!(seen[0].1, b"BMPBYTES");
    }

    #[test]
    fn an_entry_name_longer_than_its_length_prefix_is_refused() {
        // The key is a Pascal short string: one byte of length. Over 255 the
        // prefix wraps and the reader takes the wrong byte count.
        let err = write_entry(&mut Vec::new(), &"x".repeat(256), b"payload")
            .expect_err("an oversize entry name must be refused");
        assert!(err.to_string().contains("too long"), "{err}");
    }

    #[test]
    fn the_walk_stops_at_the_first_malformed_entry() {
        let mut good = Vec::new();
        write_entry(&mut good, "first.bmp", b"ONE").expect("entry should write");

        // A zero-length block: no way to know how far to step.
        let mut zero = good.clone();
        zero.extend_from_slice(&ENTRY_SIZE_FLAG.to_le_bytes());
        assert_eq!(walk(&storage_stream(&zero)).len(), 1, "zero-size block");

        // A block claiming more bytes than the stream holds.
        let mut overrun = good.clone();
        overrun.extend_from_slice(&(0x0100_0FFF_u32).to_le_bytes());
        overrun.extend_from_slice(&[0xD0, 0x01, b'x']);
        assert_eq!(walk(&storage_stream(&overrun)).len(), 1, "truncated block");

        // A block whose tag is not 0xD0 — not an entry, so the layout after it
        // is unknown.
        let mut bad_tag = good.clone();
        bad_tag.extend_from_slice(&(0x0100_0003_u32).to_le_bytes());
        bad_tag.extend_from_slice(&[0x00, 0x01, b'x']);
        assert_eq!(walk(&storage_stream(&bad_tag)).len(), 1, "missing 0xD0 tag");
    }

    /// A payload length running past its own block is skipped as an entry
    /// and the walk carries on at the next block, whose size word is sound.
    #[test]
    fn an_entry_whose_payload_overruns_its_block_is_skipped_not_fatal() {
        let mut first = Vec::new();
        write_entry(&mut first, "a.bmp", b"AAA").expect("entry should write");
        // The compressed length sits after [size:4][tag:1][name_len:1][name].
        let comp_len_at = 4 + 1 + 1 + "a.bmp".len();
        first[comp_len_at..comp_len_at + 4].copy_from_slice(&0xFFFF_u32.to_le_bytes());
        let mut second = Vec::new();
        write_entry(&mut second, "b.bmp", b"BBB").expect("entry should write");

        let mut entries = first;
        entries.extend_from_slice(&second);
        let seen = walk(&storage_stream(&entries));
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert_eq!(seen[0].0, "b.bmp");
    }

    #[test]
    fn an_entry_that_will_not_inflate_is_skipped_without_stopping_the_walk() {
        // A corrupt payload costs its own image, but the entry framing is still
        // intact, so the walk keeps its place and later entries survive.
        let mut entries = Vec::new();
        write_entry(&mut entries, "first.bmp", b"ONE").expect("entry should write");

        let garbage = b"not zlib at all";
        let block_size = 1 + 1 + "bad.bmp".len() + 4 + garbage.len();
        let block_size = u32::try_from(block_size).expect("fixture fits");
        entries.extend_from_slice(&(block_size | ENTRY_SIZE_FLAG).to_le_bytes());
        entries.push(ENTRY_TAG);
        entries.push(7);
        entries.extend_from_slice(b"bad.bmp");
        let garbage_len = u32::try_from(garbage.len()).expect("fixture fits");
        entries.extend_from_slice(&garbage_len.to_le_bytes());
        entries.extend_from_slice(garbage);

        write_entry(&mut entries, "last.bmp", b"TWO").expect("entry should write");

        let seen = walk(&storage_stream(&entries));
        assert_eq!(
            seen.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            vec!["first.bmp", "last.bmp"],
            "a bad payload must not cost the entries after it"
        );
    }

    #[test]
    fn an_entry_past_its_cap_is_skipped() {
        // A small entry can inflate to an arbitrary size; the cap is what stops
        // one entry allocating without bound.
        let mut entries = Vec::new();
        write_entry(&mut entries, "big.bmp", &[0_u8; 4096]).expect("entry should write");
        let stream = storage_stream(&entries);
        let count = |cap| {
            let mut seen = 0;
            for_each_entry(&stream, cap, &mut Budget::default(), |_, _, _| seen += 1)
                .expect("within budget");
            seen
        };
        assert_eq!(count(4096), 1, "at the cap");
        assert_eq!(count(4095), 0, "one byte over the cap is skipped");
    }

    #[test]
    fn images_past_the_reads_budget_refuse_the_library() {
        // Each image within its own cap, but together past what their
        // compressed bytes allow: the walk fails rather than allocating on.
        let mut entries = Vec::new();
        for _ in 0..4 {
            write_entry(&mut entries, "bomb.bmp", &[0_u8; 4096]).expect("entry should write");
        }
        let err = parse_icon_storage(
            &storage_stream(&entries),
            &mut Budget::with_limits(10_000, 1),
        )
        .expect_err("past the budget");
        assert!(err.to_string().contains("refused"), "{err}");
    }

    #[test]
    fn a_stream_too_short_to_hold_its_header_yields_nothing() {
        assert!(walk(&[]).is_empty());
        assert!(walk(&[1, 2, 3]).is_empty());
    }

    #[test]
    fn empty_icon_storage_is_byte_identical_header_only() {
        // The oracle-guarded invariant: a library with no embedded images
        // emits EXACTLY the pre-change header-only stream — the bare
        // `|HEADER=Icon storage` C-string param block, with no Weight key.
        let bytes = encode_icon_storage(&[]).expect("encode empty storage");
        let mut expected = Vec::new();
        write_cstring_param_block(&mut expected, b"|HEADER=Icon storage");
        assert_eq!(bytes, expected, "empty /Storage must stay byte-identical");
        assert!(
            !String::from_utf8_lossy(&bytes).contains("Weight"),
            "empty /Storage carries no Weight key"
        );
    }

    #[test]
    fn icon_storage_self_round_trips_in_order() {
        let a = b"first payload".as_slice();
        let b = vec![0u8; 4096]; // compressible second payload
        let stream = encode_icon_storage(&[
            (r"C:\Users\Public\a.bmp", a, None),
            (r"C:\Users\Public\b.bmp", &b, None),
        ])
        .expect("encode storage");

        let text = String::from_utf8_lossy(&stream[..40]).into_owned();
        assert!(
            text.contains("|HEADER=Icon storage|Weight=2"),
            "non-empty header carries the mixed-case Weight count: {text}"
        );

        let payloads = parse_icon_storage(&stream, &mut Budget::default()).expect("within budget");
        assert_eq!(payloads.len(), 2, "both entries parse back");
        assert_eq!(payloads[0].0, a, "first payload survives in order");
        assert_eq!(payloads[1].0, b, "second payload survives in order");
    }

    #[test]
    fn icon_storage_entry_names_use_the_file_path() {
        // Real AD24 names each entry with the image's full file path (not the
        // AltiumSharp-writer index); verify the name is framed as authored.
        let stream = encode_icon_storage(&[(r"C:\img\logo.bmp", b"BM".as_slice(), None)])
            .expect("encode storage");
        let mut names = Vec::new();
        for_each_entry(
            &stream,
            MAX_IMAGE_DECOMPRESSED,
            &mut Budget::default(),
            |name, _, _| {
                names.push(name.to_string());
            },
        )
        .expect("within budget");
        assert_eq!(names, vec![r"C:\img\logo.bmp".to_string()]);
    }

    #[test]
    fn icon_storage_rejects_overlong_entry_name() {
        let name = "x".repeat(256);
        let err = encode_icon_storage(&[(name.as_str(), b"BM".as_slice(), None)])
            .expect_err("a >255-byte entry name must be rejected");
        assert!(
            err.to_string().contains("too long"),
            "error names the cause: {err}"
        );
    }

    #[test]
    fn corrupt_icon_storage_is_tolerated() {
        // Truncated / garbage streams must neither panic nor error.
        assert!(parse_icon_storage(&[], &mut Budget::default())
            .expect("within budget")
            .is_empty());
        assert!(parse_icon_storage(&[0x00, 0x00], &mut Budget::default())
            .expect("within budget")
            .is_empty());
        assert!(parse_icon_storage(&[0xFF; 16], &mut Budget::default())
            .expect("within budget")
            .is_empty());

        // A valid first entry followed by garbage keeps the first entry.
        let mut stream =
            encode_icon_storage(&[("a.bmp", b"payload".as_slice(), None)]).expect("encode storage");
        stream.extend_from_slice(&[0xAB; 7]);
        let payloads = parse_icon_storage(&stream, &mut Budget::default()).expect("within budget");
        assert_eq!(payloads.len(), 1, "walk stops at the malformed tail");
        assert_eq!(payloads[0].0, b"payload");
    }
}
