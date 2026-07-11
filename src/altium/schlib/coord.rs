//! Fractional schematic-coordinate encoding.
//!
//! Altium stores a coordinate as an integer property (e.g. `Location.X`) plus an
//! optional `<key>_Frac` companion holding the fractional part scaled by
//! [`FRAC_SCALE`] (100,000). A value is reconstructed as `int + frac / FRAC_SCALE`.
//!
//! Real AD24 output uses **truncation toward zero with a signed fraction**: the
//! FRACSHAPES golden rectangle stores `-5.45` as `Location.X=-5` with
//! `Location.X_Frac=-45000` (`-5 + -45000/100000 = -5.45`). [`split`] therefore
//! truncates toward zero and lets the fraction carry the coordinate's sign,
//! keeping `frac` in `(-FRAC_SCALE, FRAC_SCALE)`.
//!
//! Historically this crate wrote the *floor* form instead (`div_euclid` /
//! `rem_euclid`, non-negative fraction: `-5.45` → int `-6`, frac `55000`).
//! [`read`] parses the fraction as a **signed** integer and adds it to the
//! integer part, so both encodings decode to the same value
//! (`-6 + 55000/100000 = -5 + -45000/100000 = -5.45`); files written by older
//! versions of this crate remain readable. Before this fix the reader parsed the
//! fraction as `u32`, so Altium's negative `_Frac` values failed to parse and
//! were silently truncated to zero — every real off-grid negative coordinate
//! lost its fractional part.

use std::collections::HashMap;

/// Scale factor for the fractional companion field (`<key>_Frac`).
pub const FRAC_SCALE: i64 = 100_000;

/// Splits a coordinate value into Altium's integer and fractional parts,
/// rounding to the nearest `1 / FRAC_SCALE` and carrying into the integer part.
/// Matches AD24's convention: truncation toward zero with the fraction carrying
/// the sign (e.g. `-5.45` → int `-5`, frac `-45000`; `5.55` → int `5`, frac
/// `55000`), so `frac` lies in `(-FRAC_SCALE, FRAC_SCALE)` and `int` and `frac`
/// never have opposite signs.
#[allow(clippy::cast_possible_truncation)]
pub fn split(value: f64) -> (i64, i64) {
    #[allow(clippy::cast_precision_loss)]
    let scaled = (value * FRAC_SCALE as f64).round() as i64;
    // Rust's `/` and `%` truncate toward zero, which is exactly Altium's
    // convention: the remainder carries the value's sign.
    (scaled / FRAC_SCALE, scaled % FRAC_SCALE)
}

/// Reconstructs a coordinate value from its integer and (signed) fractional
/// parts. Decodes both AD24's toward-zero/signed form and this crate's
/// historical floor/non-negative form, since `int + frac / FRAC_SCALE` is exact
/// for either.
#[allow(clippy::cast_precision_loss)]
pub fn combine(int: i64, frac: i64) -> f64 {
    int as f64 + frac as f64 / FRAC_SCALE as f64
}

/// Reads a coordinate that may carry a `<key>_frac` companion from parsed record
/// properties. `key` must be the lower-cased property name (the reader
/// lower-cases all keys). A missing integer or fractional part defaults to 0.
/// The fraction is parsed as a **signed** integer: AD24 emits negative `_Frac`
/// values for negative off-grid coordinates (truncation toward zero).
pub fn read(props: &HashMap<String, String>, key: &str) -> f64 {
    let int: i64 = props.get(key).and_then(|s| s.parse().ok()).unwrap_or(0);
    let frac: i64 = props
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
    fn negative_fraction_truncates_toward_zero_and_round_trips() {
        // AD24's convention (FRACSHAPES golden): the integer part truncates
        // toward zero and the fraction carries the sign.
        let (int, frac) = split(-5.45);
        assert_eq!((int, frac), (-5, -45_000));
        assert!((combine(int, frac) - (-5.45)).abs() < 1e-9);

        let (int, frac) = split(-28.995);
        assert_eq!((int, frac), (-28, -99_500));
        assert!((combine(int, frac) - (-28.995)).abs() < 1e-9);
    }

    #[test]
    fn near_boundary_value_carries_into_integer() {
        // 4.999995 must carry to 5 with no fraction (matches the elliptical-arc
        // carry behaviour) rather than clamping to 4 + 99999. Ditto mirrored.
        assert_eq!(split(4.999_995), (5, 0));
        assert_eq!(split(-4.999_995), (-5, 0));
    }

    #[test]
    fn read_combines_int_and_signed_frac_keys() {
        // AD24's toward-zero/signed form (the FRACSHAPES golden bytes).
        let mut props = HashMap::new();
        props.insert("location.x".to_string(), "-5".to_string());
        props.insert("location.x_frac".to_string(), "-45000".to_string());
        assert!((read(&props, "location.x") - (-5.45)).abs() < 1e-9);
        // A key with no `_frac` companion reads as the bare integer.
        assert!((read(&props, "missing") - 0.0).abs() < 1e-9);
    }

    #[test]
    fn read_still_decodes_historical_floor_form() {
        // Files written by older versions of this crate used floor semantics
        // (non-negative fraction); the signed reader decodes them identically.
        let mut props = HashMap::new();
        props.insert("location.x".to_string(), "-6".to_string());
        props.insert("location.x_frac".to_string(), "55000".to_string());
        assert!((read(&props, "location.x") - (-5.45)).abs() < 1e-9);

        let mut props = HashMap::new();
        props.insert("location.x".to_string(), "-29".to_string());
        props.insert("location.x_frac".to_string(), "500".to_string());
        assert!((read(&props, "location.x") - (-28.995)).abs() < 1e-9);
    }

    #[test]
    fn read_int_omitted_with_frac_present() {
        // AD24 omits a zero integer key when the fraction is non-zero (the
        // FRACSHAPES golden arc: `Location.X_Frac=5000` with no `Location.X`).
        let mut props = HashMap::new();
        props.insert("location.x_frac".to_string(), "5000".to_string());
        assert!((read(&props, "location.x") - 0.05).abs() < 1e-9);
    }
}
