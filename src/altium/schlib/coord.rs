//! Fractional schematic-coordinate encoding.
//!
//! Altium stores a coordinate as an integer property (e.g. `Location.X`) plus an
//! optional `<key>_Frac` companion holding the fractional part scaled by
//! [`FRAC_SCALE`] (100,000). The `_Frac` field is non-negative (Altium's maximum
//! is 99,999), so a value is reconstructed as `int + frac / FRAC_SCALE`.
//!
//! [`split`] therefore uses *floor* division (`div_euclid` / `rem_euclid`) rather
//! than truncation, which keeps the fraction in `[0, FRAC_SCALE)` for negative
//! coordinates as well (e.g. `-28.995` → int `-29`, frac `500`). It round-trips
//! through [`combine`] across the whole coordinate range — not just the
//! non-negative radii the elliptical-arc encoder originally special-cased.
//!
//! NOTE: the floor convention matches this crate's reader (`int + frac`) and the
//! documented non-negative `_Frac`. The negative-coordinate case has not yet been
//! confirmed against an Altium-authored file; verify it before relying on
//! fractional encoding for off-grid negative positions.

use std::collections::HashMap;

/// Scale factor for the fractional companion field (`<key>_Frac`).
pub const FRAC_SCALE: i64 = 100_000;

/// Splits a coordinate value into Altium's integer and (non-negative) fractional
/// parts, rounding to the nearest `1 / FRAC_SCALE` and carrying into the integer
/// part. Floor semantics keep `frac` in `[0, FRAC_SCALE)` for negative values.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn split(value: f64) -> (i64, u32) {
    #[allow(clippy::cast_precision_loss)]
    let scaled = (value * FRAC_SCALE as f64).round() as i64;
    (
        scaled.div_euclid(FRAC_SCALE),
        scaled.rem_euclid(FRAC_SCALE) as u32,
    )
}

/// Reconstructs a coordinate value from its integer and fractional parts.
#[allow(clippy::cast_precision_loss)]
pub fn combine(int: i64, frac: u32) -> f64 {
    int as f64 + f64::from(frac) / FRAC_SCALE as f64
}

/// Reads a coordinate that may carry a `<key>_frac` companion from parsed record
/// properties. `key` must be the lower-cased property name (the reader
/// lower-cases all keys). A missing integer or fractional part defaults to 0.
pub fn read(props: &HashMap<String, String>, key: &str) -> f64 {
    let int: i64 = props.get(key).and_then(|s| s.parse().ok()).unwrap_or(0);
    let frac: u32 = props
        .get(&format!("{key}_frac"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    combine(int, frac)
}

#[cfg(test)]
mod tests {
    use super::{combine, read, split};
    use std::collections::HashMap;

    #[test]
    fn integer_values_have_zero_fraction() {
        // Byte-identity invariant: integer coordinates must emit no `_Frac`.
        assert_eq!(split(0.0), (0, 0));
        assert_eq!(split(21.0), (21, 0));
        assert_eq!(split(-10.0), (-10, 0));
    }

    #[test]
    fn positive_fraction_round_trips() {
        let (int, frac) = split(7.5);
        assert_eq!((int, frac), (7, 50_000));
        assert!((combine(int, frac) - 7.5).abs() < 1e-9);
    }

    #[test]
    fn negative_fraction_uses_floor_and_round_trips() {
        // The case the elliptical-arc encoder never exercised: floor integer part
        // with a non-negative fraction.
        let (int, frac) = split(-28.995);
        assert_eq!((int, frac), (-29, 500));
        assert!((combine(int, frac) - (-28.995)).abs() < 1e-9);
    }

    #[test]
    fn near_boundary_value_carries_into_integer() {
        // 4.999995 must carry to 5 with no fraction (matches the elliptical-arc
        // carry behaviour) rather than clamping to 4 + 99999.
        assert_eq!(split(4.999_995), (5, 0));
    }

    #[test]
    fn read_combines_int_and_frac_keys() {
        let mut props = HashMap::new();
        props.insert("location.x".to_string(), "-29".to_string());
        props.insert("location.x_frac".to_string(), "500".to_string());
        assert!((read(&props, "location.x") - (-28.995)).abs() < 1e-9);
        // A key with no `_frac` companion reads as the bare integer.
        assert!((read(&props, "missing") - 0.0).abs() < 1e-9);
    }
}
