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
