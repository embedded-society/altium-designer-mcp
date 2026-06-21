//! Shared bounds-checked little-endian scalar readers.
//!
//! Both the `PcbLib` and `SchLib` readers walk byte buffers at explicit
//! offsets; these helpers replace the byte-identical `read_*` functions each
//! had defined locally. Every reader returns `None` past the end of the slice
//! rather than panicking, so callers can use `?`.

/// Reads a little-endian `u16` at `offset`, or `None` if out of bounds.
pub fn read_u16_le(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2)?
        .try_into()
        .ok()
        .map(u16::from_le_bytes)
}

/// Reads a little-endian `u32` at `offset`, or `None` if out of bounds.
pub fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)?
        .try_into()
        .ok()
        .map(u32::from_le_bytes)
}

/// Reads a little-endian `i16` at `offset`, or `None` if out of bounds.
pub fn read_i16_le(data: &[u8], offset: usize) -> Option<i16> {
    data.get(offset..offset + 2)?
        .try_into()
        .ok()
        .map(i16::from_le_bytes)
}

/// Reads a little-endian `i32` at `offset`, or `None` if out of bounds.
pub fn read_i32_le(data: &[u8], offset: usize) -> Option<i32> {
    data.get(offset..offset + 4)?
        .try_into()
        .ok()
        .map(i32::from_le_bytes)
}

/// Reads a little-endian IEEE-754 `f64` at `offset`, or `None` if out of bounds.
pub fn read_f64_le(data: &[u8], offset: usize) -> Option<f64> {
    data.get(offset..offset + 8)?
        .try_into()
        .ok()
        .map(f64::from_le_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_endian_values() {
        let d = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        assert_eq!(read_u16_le(&d, 0), Some(0x0201));
        assert_eq!(read_u32_le(&d, 0), Some(0x0403_0201));
        assert_eq!(read_i16_le(&d, 0), Some(0x0201));
        assert_eq!(read_i32_le(&d, 0), Some(0x0403_0201));
        assert_eq!(read_f64_le(&d, 0), Some(f64::from_le_bytes(d)));
    }

    #[test]
    fn out_of_bounds_is_none() {
        let d = [0x01, 0x02];
        assert_eq!(read_u32_le(&d, 0), None);
        assert_eq!(read_u16_le(&d, 1), None);
        assert_eq!(read_f64_le(&d, 0), None);
    }
}
