//! Shared low-level byte framing for Altium library streams.
//!
//! Both the `PcbLib` (binary) and `SchLib` (ASCII-record) writers frame data
//! with the same handful of length-prefixed primitives. They live here so the
//! two formats share one implementation of each frame instead of copy-pasting
//! the exact byte layout (which previously drifted and caused issue #68's
//! "Data does not end with 0x00").
//!
//! The per-format record *shape* (`PcbLib`'s binary sub-blocks vs `SchLib`'s
//! `|KEY=VALUE|` text records) stays in each module — only these byte frames
//! are shared.

/// Appends a length-prefixed binary block: `[u32 LE length][bytes]`.
///
/// This is the frame for every `PcbLib` primitive sub-block.
pub fn write_block(data: &mut Vec<u8>, block: &[u8]) {
    #[allow(clippy::cast_possible_truncation)]
    data.extend_from_slice(&(block.len() as u32).to_le_bytes());
    data.extend_from_slice(block);
}

/// Appends Altium's C-string parameter block: `[u32 LE length][bytes][0x00]`,
/// where the length **includes** the trailing null terminator.
///
/// This is the canonical `WriteCStringParameterBlock` frame used for parameter
/// and header blocks in both formats. Centralising it guarantees the
/// length-includes-null / trailing-`0x00` invariant that Altium requires
/// (omitting it was issue #68's "Data does not end with 0x00"). Callers that
/// hold text encode it first, e.g. `&crate::altium::encode_windows1252(s)`.
pub fn write_cstring_param_block(data: &mut Vec<u8>, text_bytes: &[u8]) {
    #[allow(clippy::cast_possible_truncation)]
    data.extend_from_slice(&((text_bytes.len() + 1) as u32).to_le_bytes());
    data.extend_from_slice(text_bytes);
    data.push(0x00);
}

/// Appends a Pascal short string: `[u8 length][bytes]`.
///
/// The caller is responsible for validating that `bytes.len() <= 255` (each
/// call site produces its own field-named error); in debug builds an overflow
/// is asserted.
pub fn write_pascal_string(record: &mut Vec<u8>, bytes: &[u8]) {
    debug_assert!(bytes.len() <= 255, "Pascal short string exceeds 255 bytes");
    #[allow(clippy::cast_possible_truncation)]
    record.push(bytes.len() as u8);
    record.extend_from_slice(bytes);
}

/// Reads a length-prefixed binary block written by [`write_block`]:
/// `[u32 LE length][bytes]`.
///
/// Returns the inner slice and the offset just past it, or `None` if the
/// length prefix or payload runs past the end of `data`. Imposes **no** size
/// cap — callers that need a sanity limit apply it themselves.
#[must_use]
pub fn read_block(data: &[u8], offset: usize) -> Option<(&[u8], usize)> {
    let len = crate::altium::bytes::read_u32_le(data, offset)? as usize;
    let end = offset + 4 + len;
    if end > data.len() {
        return None;
    }
    Some((&data[offset + 4..end], end))
}

/// Reads a Pascal short string written by [`write_pascal_string`]:
/// `[u8 length][win1252 bytes]` at `offset`.
///
/// Returns the decoded string and the offset just past the field
/// (`offset + 1 + length`). A length byte that is missing or whose bytes run
/// out of bounds yields an empty string; the offset still advances past the
/// declared length so callers stepping through fixed records stay aligned.
#[must_use]
pub fn read_pascal_string(data: &[u8], offset: usize) -> (String, usize) {
    let len = data.get(offset).copied().unwrap_or(0) as usize;
    let start = offset + 1;
    let end = start + len;
    let s = if len > 0 && end <= data.len() {
        crate::altium::decode_windows1252(&data[start..end])
    } else {
        String::new()
    };
    (s, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_is_length_prefixed() {
        let mut out = Vec::new();
        write_block(&mut out, &[0x01, 0x02, 0x03]);
        assert_eq!(out, vec![0x03, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn cstring_param_block_length_includes_null() {
        let mut out = Vec::new();
        write_cstring_param_block(&mut out, b"|K=V");
        // length = 4 bytes + 1 null = 5
        assert_eq!(
            out,
            vec![0x05, 0x00, 0x00, 0x00, b'|', b'K', b'=', b'V', 0x00]
        );
        assert_eq!(*out.last().unwrap(), 0x00);
    }

    #[test]
    fn pascal_string_has_u8_length() {
        let mut out = Vec::new();
        write_pascal_string(&mut out, b"R1");
        assert_eq!(out, vec![0x02, b'R', b'1']);
    }
}
