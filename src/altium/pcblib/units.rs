//! `PcbLib` coordinate and unit conversions.
//!
//! Altium `PcbLib` internal units: `10000 = 1 mil = 0.0254 mm`. Centralising the
//! scale here gives the writer and reader a single source of truth instead of
//! each hard-coding `0.0254`/`10000`. These helpers are `PcbLib`-specific —
//! `SchLib` stores coordinates as ASCII decimals in raw grid units and must
//! NEVER be routed through them.

/// One mil in millimetres.
pub(super) const MM_PER_MIL: f64 = 0.0254;

/// Conversion factor from millimetres to Altium internal units (`10000 = 1 mil`).
pub(super) const MM_TO_INTERNAL_UNITS: f64 = 10000.0 / MM_PER_MIL;

/// Conversion factor from Altium internal units to millimetres.
pub(super) const INTERNAL_UNITS_TO_MM: f64 = MM_PER_MIL / 10000.0;

/// Multiplier for rounding millimetres to 6 decimal places (1 nm resolution),
/// used to strip floating-point noise from converted coordinates.
pub(super) const MM_ROUNDING_MULTIPLIER: f64 = 1_000_000.0;

/// Converts millimetres to Altium internal units (rounded to the nearest unit).
#[allow(clippy::cast_possible_truncation)] // Intentional: PCB coordinates fit in i32
pub(super) fn from_mm(mm: f64) -> i32 {
    (mm * MM_TO_INTERNAL_UNITS).round() as i32
}

/// Converts Altium internal units to millimetres.
///
/// Rounds to 6 decimal places (1 nm resolution) to avoid floating-point noise.
pub(super) fn to_mm(internal: i32) -> f64 {
    let raw = f64::from(internal) * INTERNAL_UNITS_TO_MM;
    (raw * MM_ROUNDING_MULTIPLIER).round() / MM_ROUNDING_MULTIPLIER
}

/// Converts millimetres to mils (for parameter strings).
pub(super) fn mm_to_mil(mm: f64) -> f64 {
    mm / MM_PER_MIL
}
