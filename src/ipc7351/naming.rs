//! IPC-7351B naming convention generator.
//!
//! Generates standardised footprint names per IPC-7351B naming conventions.
//!
//! # Name Format
//!
//! The general format is: `{TYPE}{PITCH}P{LENGTH}X{WIDTH}X{HEIGHT}-{PINS}{DENSITY}`
//!
//! Examples:
//! - `RESC1608X55N` - 0603 chip resistor, nominal density
//! - `CAPC2012X70M` - 0805 chip capacitor, most density
//! - `SOIC127P600X175-8N` - SOIC-8, 1.27mm pitch, nominal density

use crate::ipc7351::density::DensityLevel;

/// Generates IPC-7351B name for a chip component (resistor, capacitor, etc.).
///
/// Format: `{PREFIX}{LENGTH}{WIDTH}X{HEIGHT}{DENSITY}`
///
/// Where dimensions are in 0.01mm units.
///
/// # Arguments
///
/// * `body_length` - Body length in mm
/// * `body_width` - Body width in mm
/// * `height` - Component height in mm
/// * `density` - Density level
///
/// # Examples
///
/// ```
/// use altium_designer_mcp::ipc7351::naming::chip_name;
/// use altium_designer_mcp::ipc7351::density::DensityLevel;
///
/// let name = chip_name(1.6, 0.8, 0.45, DensityLevel::Nominal);
/// assert_eq!(name, "RESC1608X45N");
/// ```
#[must_use]
pub fn chip_name(body_length: f64, body_width: f64, height: f64, density: DensityLevel) -> String {
    // Body dimensions in 0.1mm units, height in 0.01mm units
    let length_units = mm_to_tenths(body_length);
    let width_units = mm_to_tenths(body_width);
    let height_units = mm_to_hundredths(height);

    format!(
        "RESC{length_units:02}{width_units:02}X{height_units}{density}",
        density = density.suffix()
    )
}

/// Generates IPC-7351B name for a chip capacitor.
///
/// Same format as chip resistor but with CAPC prefix.
#[must_use]
pub fn chip_capacitor_name(
    body_length: f64,
    body_width: f64,
    height: f64,
    density: DensityLevel,
) -> String {
    // Body dimensions in 0.1mm units, height in 0.01mm units
    let length_units = mm_to_tenths(body_length);
    let width_units = mm_to_tenths(body_width);
    let height_units = mm_to_hundredths(height);

    format!(
        "CAPC{length_units:02}{width_units:02}X{height_units}{density}",
        density = density.suffix()
    )
}

/// Generates IPC-7351B name for a chip inductor.
///
/// Same format as chip resistor but with INDC prefix.
#[must_use]
pub fn chip_inductor_name(
    body_length: f64,
    body_width: f64,
    height: f64,
    density: DensityLevel,
) -> String {
    // Body dimensions in 0.1mm units, height in 0.01mm units
    let length_units = mm_to_tenths(body_length);
    let width_units = mm_to_tenths(body_width);
    let height_units = mm_to_hundredths(height);

    format!(
        "INDC{length_units:02}{width_units:02}X{height_units}{density}",
        density = density.suffix()
    )
}

/// Generates IPC-7351B name for a MELF component.
///
/// Format: `{PREFIX}{LENGTH}X{DIAMETER}X{HEIGHT}{DENSITY}`
#[must_use]
pub fn melf_name(length: f64, diameter: f64, density: DensityLevel) -> String {
    let length_units = mm_to_hundredths(length);
    let diameter_units = mm_to_hundredths(diameter);

    format!(
        "MELF{length_units}X{diameter_units}{density}",
        density = density.suffix()
    )
}

/// Generates IPC-7351B name for a SOIC/SSOP/TSSOP component.
///
/// Format: `{PREFIX}{PITCH}P{LENGTH}X{WIDTH}X{HEIGHT}-{PINS}{DENSITY}`
///
/// # Arguments
///
/// * `prefix` - Package prefix (SOIC, SSOP, TSSOP, MSOP)
/// * `pitch` - Lead pitch in mm
/// * `body_length` - Body length in mm
/// * `body_width` - Body width in mm
/// * `height` - Component height in mm
/// * `pin_count` - Number of pins
/// * `density` - Density level
#[must_use]
pub fn soic_name(
    prefix: &str,
    pitch: f64,
    body_length: f64,
    body_width: f64,
    height: f64,
    pin_count: u32,
    density: DensityLevel,
) -> String {
    let pitch_units = mm_to_hundredths(pitch);
    let length_units = mm_to_hundredths(body_length);
    let width_units = mm_to_hundredths(body_width);
    let height_units = mm_to_hundredths(height);

    format!(
        "{prefix}{pitch_units}P{length_units}X{width_units}X{height_units}-{pin_count}{density}",
        density = density.suffix()
    )
}

/// Generates IPC-7351B name for a QFP/LQFP/TQFP component.
///
/// Format: `{PREFIX}{PITCH}P{LENGTH}X{WIDTH}X{HEIGHT}-{PINS}{DENSITY}`
#[must_use]
pub fn qfp_name(
    prefix: &str,
    pitch: f64,
    body_length: f64,
    body_width: f64,
    height: f64,
    pin_count: u32,
    density: DensityLevel,
) -> String {
    let pitch_units = mm_to_hundredths(pitch);
    let length_units = mm_to_hundredths(body_length);
    let width_units = mm_to_hundredths(body_width);
    let height_units = mm_to_hundredths(height);

    format!(
        "{prefix}{pitch_units}P{length_units}X{width_units}X{height_units}-{pin_count}{density}",
        density = density.suffix()
    )
}

/// Generates IPC-7351B name for a QFN/DFN/SON component.
///
/// Format: `{PREFIX}{PITCH}P{LENGTH}X{WIDTH}X{HEIGHT}-{PINS}{VARIANT}{DENSITY}`
///
/// Variant indicates thermal pad presence: N = no thermal pad, T = thermal pad
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn qfn_name(
    prefix: &str,
    pitch: f64,
    body_length: f64,
    body_width: f64,
    height: f64,
    pin_count: u32,
    has_thermal_pad: bool,
    density: DensityLevel,
) -> String {
    let pitch_units = mm_to_hundredths(pitch);
    let length_units = mm_to_hundredths(body_length);
    let width_units = mm_to_hundredths(body_width);
    let height_units = mm_to_hundredths(height);
    let variant = if has_thermal_pad { "T" } else { "N" };

    format!(
        "{prefix}{pitch_units}P{length_units}X{width_units}X{height_units}-{pin_count}{variant}{density}",
        density = density.suffix()
    )
}

/// Generates IPC-7351B name for a BGA component.
///
/// Format: `BGA{PITCH}P{COLUMNS}X{ROWS}-{BALLS}{DENSITY}`
#[must_use]
pub fn bga_name(
    pitch: f64,
    body_length: f64,
    body_width: f64,
    ball_count: u32,
    density: DensityLevel,
) -> String {
    let pitch_units = mm_to_hundredths(pitch);
    let length_units = mm_to_hundredths(body_length);
    let width_units = mm_to_hundredths(body_width);

    format!(
        "BGA{pitch_units}P{length_units}X{width_units}-{ball_count}{density}",
        density = density.suffix()
    )
}

/// Generates IPC-7351B name for a SOT component.
///
/// Format: `SOT{VARIANT}P{LENGTH}X{WIDTH}X{HEIGHT}-{PINS}{DENSITY}`
#[must_use]
pub fn sot_name(
    variant: &str,
    pitch: f64,
    body_length: f64,
    body_width: f64,
    height: f64,
    pin_count: u32,
    density: DensityLevel,
) -> String {
    let pitch_units = mm_to_hundredths(pitch);
    let length_units = mm_to_hundredths(body_length);
    let width_units = mm_to_hundredths(body_width);
    let height_units = mm_to_hundredths(height);

    format!(
        "SOT{variant}{pitch_units}P{length_units}X{width_units}X{height_units}-{pin_count}{density}",
        density = density.suffix()
    )
}

/// Converts millimetres to IPC naming units (0.01mm = 1 unit).
///
/// Used for height dimensions in chip component names.
/// Values are rounded to nearest integer.
fn mm_to_hundredths(mm: f64) -> u32 {
    // IPC names use height in 0.01mm units
    // All PCB component dimensions are positive and small, so cast is safe
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let units = (mm * 100.0).round() as u32;
    units
}

/// Converts millimetres to IPC naming units (0.1mm = 1 unit).
///
/// Used for body length/width dimensions in chip component names.
/// Values are rounded to nearest integer.
fn mm_to_tenths(mm: f64) -> u32 {
    // IPC chip names use body dimensions in 0.1mm units
    // All PCB component dimensions are positive and small, so cast is safe
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let units = (mm * 10.0).round() as u32;
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_name_0603() {
        // 0603 = 1.6mm x 0.8mm, typical height 0.45mm
        let name = chip_name(1.6, 0.8, 0.45, DensityLevel::Nominal);
        assert_eq!(name, "RESC1608X45N");
    }

    #[test]
    fn chip_name_0805() {
        // 0805 = 2.0mm x 1.2mm (IPC nominal), typical height 0.55mm
        let name = chip_name(2.0, 1.2, 0.55, DensityLevel::Nominal);
        assert_eq!(name, "RESC2012X55N");
    }

    #[test]
    fn chip_name_densities() {
        let most = chip_name(1.6, 0.8, 0.45, DensityLevel::Most);
        let nominal = chip_name(1.6, 0.8, 0.45, DensityLevel::Nominal);
        let least = chip_name(1.6, 0.8, 0.45, DensityLevel::Least);

        assert!(most.ends_with('M'));
        assert!(nominal.ends_with('N'));
        assert!(least.ends_with('L'));
    }

    #[test]
    fn capacitor_name() {
        // 0805 capacitor = 2.0mm x 1.2mm (IPC nominal), typical height 0.7mm
        let name = chip_capacitor_name(2.0, 1.2, 0.7, DensityLevel::Nominal);
        assert_eq!(name, "CAPC2012X70N");
    }

    #[test]
    fn soic_name_8pin() {
        let name = soic_name("SOIC", 1.27, 4.9, 3.9, 1.75, 8, DensityLevel::Nominal);
        assert_eq!(name, "SOIC127P490X390X175-8N");
    }

    #[test]
    fn qfn_name_with_thermal() {
        let name = qfn_name("QFN", 0.5, 3.0, 3.0, 0.8, 16, true, DensityLevel::Nominal);
        assert_eq!(name, "QFN50P300X300X80-16TN");
    }

    #[test]
    fn mm_to_hundredths_conversion() {
        assert_eq!(mm_to_hundredths(1.27), 127);
        assert_eq!(mm_to_hundredths(0.5), 50);
        assert_eq!(mm_to_hundredths(3.0), 300);
        assert_eq!(mm_to_hundredths(0.45), 45);
    }

    #[test]
    fn mm_to_tenths_conversion() {
        assert_eq!(mm_to_tenths(1.6), 16);
        assert_eq!(mm_to_tenths(0.8), 8);
        assert_eq!(mm_to_tenths(2.0), 20);
        assert_eq!(mm_to_tenths(1.25), 13); // Rounds to nearest
    }
}
