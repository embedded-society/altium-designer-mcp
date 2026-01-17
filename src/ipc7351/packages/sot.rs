//! IPC-7351B land pattern calculations for SOT (Small Outline Transistor) packages.
//!
//! SOT packages include:
//! - SOT23: 3, 5, or 6-lead variants
//! - SOT89: 3-lead power package
//! - SOT143: 4-lead with asymmetric leads
//! - SOT223: 4-lead power package
//! - SOTFL: Flat lead variants
//!
//! These are gull-wing lead packages per IPC-7351B Section 9.

use crate::ipc7351::density::{CourtyardExcess, DensityLevel, SolderFilletGoals};
use crate::ipc7351::naming;
use crate::ipc7351::packages::{
    Assembly, Courtyard, LandPattern, Line, Pad, PackageCalculator, Point, Silkscreen,
};

/// SOT package variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SotVariant {
    /// SOT23 - 3, 5, or 6 leads.
    Sot23,
    /// SOT89 - 3-lead power package.
    Sot89,
    /// SOT143 - 4-lead with asymmetric leads.
    Sot143,
    /// SOT223 - 4-lead power package with large collector tab.
    Sot223,
    /// SOT323 (SC-70) - smaller than SOT23.
    Sot323,
    /// SOT363 (SC-88) - 6-lead, smaller pitch.
    Sot363,
}

impl SotVariant {
    /// Returns the IPC naming prefix for this variant.
    #[must_use]
    pub const fn prefix(&self) -> &'static str {
        // All SOT variants use the same prefix
        "SOT"
    }
}

/// Dimensions for a SOT package.
#[derive(Debug, Clone)]
pub struct SotDimensions {
    /// Body length (D dimension, mm) - direction of leads.
    pub body_length: f64,

    /// Body width (E1 dimension, mm) - perpendicular to leads.
    pub body_width: f64,

    /// Component height (A dimension, mm).
    pub height: f64,

    /// Lead width (b dimension, mm).
    pub lead_width: f64,

    /// Lead length (L dimension, mm).
    pub lead_length: f64,

    /// Lead pitch (e dimension, mm).
    pub pitch: f64,

    /// Lead span (E dimension, mm) - outer edge to outer edge.
    pub lead_span: f64,

    /// Number of leads.
    pub lead_count: u32,

    /// Package variant.
    pub variant: SotVariant,
}

impl SotDimensions {
    /// Creates SOT23 dimensions for 3/5/6-lead variants.
    #[must_use]
    pub const fn sot23(lead_count: u32) -> Self {
        // Standard SOT23 dimensions per JEDEC TO-236
        Self {
            body_length: 2.90,  // D: 2.80-3.05mm nominal
            body_width: 1.30,   // E1: 1.20-1.40mm nominal
            height: 1.10,       // A: 0.90-1.20mm max
            lead_width: 0.40,   // b: 0.30-0.50mm
            lead_length: 0.45,  // L: 0.30-0.60mm
            pitch: 0.95,        // e: 0.95mm
            lead_span: 2.40,    // E: 2.20-2.60mm
            lead_count,
            variant: SotVariant::Sot23,
        }
    }

    /// Creates SOT89 dimensions (3-lead power package).
    #[must_use]
    pub const fn sot89() -> Self {
        // Standard SOT89 dimensions per JEDEC TO-243
        Self {
            body_length: 4.50,  // D: 4.40-4.60mm
            body_width: 2.50,   // E1: 2.40-2.60mm
            height: 1.50,       // A: 1.45-1.60mm
            lead_width: 0.50,   // b: 0.40-0.60mm (outer leads)
            lead_length: 0.90,  // L: 0.80-1.00mm
            pitch: 1.50,        // e: 1.50mm
            lead_span: 4.00,    // E: 3.80-4.20mm
            lead_count: 3,
            variant: SotVariant::Sot89,
        }
    }

    /// Creates SOT223 dimensions (4-lead power package).
    #[must_use]
    pub const fn sot223() -> Self {
        // Standard SOT223 dimensions per JEDEC TO-261
        Self {
            body_length: 6.50,  // D: 6.30-6.70mm
            body_width: 3.50,   // E1: 3.30-3.70mm
            height: 1.75,       // A: 1.60-1.80mm
            lead_width: 0.70,   // b: 0.60-0.80mm (outer leads)
            lead_length: 0.70,  // L: 0.50-0.90mm
            pitch: 2.30,        // e: 2.30mm
            lead_span: 6.70,    // E: 6.50-6.90mm
            lead_count: 4,
            variant: SotVariant::Sot223,
        }
    }

    /// Creates SOT323 (SC-70) dimensions.
    #[must_use]
    pub const fn sot323(lead_count: u32) -> Self {
        // Standard SOT323/SC-70 dimensions
        Self {
            body_length: 2.00,  // D: 1.90-2.10mm
            body_width: 1.25,   // E1: 1.15-1.35mm
            height: 0.95,       // A: 0.80-1.10mm
            lead_width: 0.26,   // b: 0.18-0.34mm
            lead_length: 0.35,  // L: 0.26-0.46mm
            pitch: 0.65,        // e: 0.65mm
            lead_span: 2.00,    // E: 1.80-2.20mm
            lead_count,
            variant: SotVariant::Sot323,
        }
    }

    /// Creates SOT363 (SC-88) dimensions (6-lead).
    #[must_use]
    pub const fn sot363() -> Self {
        // Standard SOT363/SC-88 dimensions
        Self {
            body_length: 2.00,  // D: 1.90-2.10mm
            body_width: 1.25,   // E1: 1.15-1.35mm
            height: 0.95,       // A: 0.80-1.10mm
            lead_width: 0.22,   // b: 0.15-0.30mm
            lead_length: 0.35,  // L: 0.26-0.46mm
            pitch: 0.65,        // e: 0.65mm
            lead_span: 2.00,    // E: 1.80-2.20mm
            lead_count: 6,
            variant: SotVariant::Sot363,
        }
    }

    /// Creates custom SOT dimensions.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn custom(
        body_length: f64,
        body_width: f64,
        height: f64,
        lead_width: f64,
        lead_length: f64,
        pitch: f64,
        lead_span: f64,
        lead_count: u32,
        variant: SotVariant,
    ) -> Self {
        Self {
            body_length,
            body_width,
            height,
            lead_width,
            lead_length,
            pitch,
            lead_span,
            lead_count,
            variant,
        }
    }

    /// Calculates the heel spacing (gap between pads).
    ///
    /// `S_min` = `Body_Width` - 2 × `Lead_Length_max`
    #[must_use]
    pub fn heel_spacing(&self) -> f64 {
        2.0f64.mul_add(-self.lead_length, self.body_width)
    }
}

/// Calculator for SOT packages.
#[derive(Debug, Default)]
pub struct SotCalculator;

impl SotCalculator {
    /// Creates a new SOT calculator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Calculates land pattern for a SOT component.
    ///
    /// Per IPC-7351B Section 9 - Gull-Wing Lead Components.
    ///
    /// # Arguments
    ///
    /// * `dims` - SOT package dimensions
    /// * `density` - Density level (M/N/L)
    ///
    /// # Land Pattern Calculation
    ///
    /// For gull-wing leads:
    /// - `Z_max` = `L_min` + 2×Jt (overall land pattern span)
    /// - `G_min` = `S_max` - 2×Jh (gap between pads)
    /// - X = (`Z_max` - `G_min`) / 2 (pad width)
    /// - Y = `W_min` + 2×Js (pad height)
    #[must_use]
    pub fn calculate(&self, dims: &SotDimensions, density: DensityLevel) -> LandPattern {
        let goals = SolderFilletGoals::for_gull_wing(density);
        let courtyard_excess = CourtyardExcess::for_density(density);

        // IPC-7351B tolerances (typical manufacturing tolerances)
        let tolerance_span = 0.15;   // Lead span tolerance
        let tolerance_lead = 0.10;   // Lead length tolerance
        let tolerance_width = 0.10;  // Lead width tolerance

        // Calculate minimum/maximum values
        let l_min = dims.lead_span - tolerance_span;        // Minimum lead span
        let s_max = dims.heel_spacing() + tolerance_lead;   // Maximum heel spacing
        let w_min = dims.lead_width - tolerance_width;      // Minimum lead width

        // IPC-7351B formulas for gull-wing leads
        let z_max = 2.0f64.mul_add(goals.toe, l_min);       // Maximum land pattern span
        let g_min = 2.0f64.mul_add(-goals.heel, s_max);     // Minimum gap
        let pad_width = (z_max - g_min) / 2.0;
        let pad_height = 2.0f64.mul_add(goals.side, w_min);

        // Pad centre positions (symmetric about Y axis)
        let pad_centre_x = (z_max + g_min) / 4.0;

        // Round to reasonable precision (0.01mm, with 0.05mm pitch rounding option)
        let round_off = 0.05; // Per Altium wizard default
        let pad_width = round_to(pad_width, round_off);
        let pad_height = round_to(pad_height, round_off);
        let pad_centre_x = round_to(pad_centre_x, round_off);

        // Create pads based on lead configuration
        let pads = Self::create_pads(dims, pad_width, pad_height, pad_centre_x);

        // Calculate courtyard
        let max_x = pad_centre_x + (pad_width / 2.0) + courtyard_excess.excess;
        let max_y = Self::max_y_extent(dims, pad_height) + courtyard_excess.excess;

        // Round courtyard to grid
        let grid = match density {
            DensityLevel::Most => 0.10,
            DensityLevel::Nominal => 0.05,
            DensityLevel::Least => 0.01,
        };
        let courtyard_x = round_up_to(max_x, grid);
        let courtyard_y = round_up_to(max_y, grid);

        let courtyard = Courtyard::from_bounds(-courtyard_x, -courtyard_y, courtyard_x, courtyard_y);

        // Create silkscreen
        let silkscreen = Self::create_silkscreen(dims, pad_centre_x, pad_width, pad_height);

        // Create assembly outline
        let assembly = Assembly::from_body(dims.body_width, dims.body_length);

        // Generate IPC name
        let ipc_name = naming::sot_name(
            "",  // No variant prefix for standard SOT naming
            dims.pitch,
            dims.body_width,
            dims.body_length,
            dims.height,
            dims.lead_count,
            density,
        );

        LandPattern {
            ipc_name,
            pads,
            courtyard,
            silkscreen,
            assembly,
            origin: Point::default(),
        }
    }

    /// Creates pads for the SOT package.
    fn create_pads(
        dims: &SotDimensions,
        pad_width: f64,
        pad_height: f64,
        pad_centre_x: f64,
    ) -> Vec<Pad> {
        let mut pads = Vec::with_capacity(dims.lead_count as usize);

        match dims.lead_count {
            3 => {
                // SOT23-3: 2 leads on one side, 1 on the other
                // Pin 1 and 2 on left (negative X), Pin 3 on right (positive X)
                let y_offset = dims.pitch / 2.0;

                pads.push(Pad::rectangular(1, -pad_centre_x, y_offset, pad_width, pad_height));
                pads.push(Pad::rectangular(2, -pad_centre_x, -y_offset, pad_width, pad_height));
                pads.push(Pad::rectangular(3, pad_centre_x, 0.0, pad_width, pad_height));
            }
            4 => {
                // SOT223/SOT143: 3 leads on one side, 1 tab on other
                // This varies by package - using SOT223 convention
                let y_offset = dims.pitch;

                // Left side: pins 1, 2, 3
                pads.push(Pad::rectangular(1, -pad_centre_x, y_offset, pad_width, pad_height));
                pads.push(Pad::rectangular(2, -pad_centre_x, 0.0, pad_width, pad_height));
                pads.push(Pad::rectangular(3, -pad_centre_x, -y_offset, pad_width, pad_height));

                // Right side: pin 4 (large tab for SOT223)
                if dims.variant == SotVariant::Sot223 {
                    // Large collector tab
                    let tab_width = pad_width;
                    let tab_height = dims.pitch * 1.5; // Wider tab
                    pads.push(Pad::rectangular(4, pad_centre_x, 0.0, tab_width, tab_height));
                } else {
                    pads.push(Pad::rectangular(4, pad_centre_x, 0.0, pad_width, pad_height));
                }
            }
            5 => {
                // SOT23-5: 3 leads on one side, 2 on the other
                let y_offset = dims.pitch;

                // Left side: pins 1, 2, 3
                pads.push(Pad::rectangular(1, -pad_centre_x, y_offset, pad_width, pad_height));
                pads.push(Pad::rectangular(2, -pad_centre_x, 0.0, pad_width, pad_height));
                pads.push(Pad::rectangular(3, -pad_centre_x, -y_offset, pad_width, pad_height));

                // Right side: pins 4, 5 (note: no centre pin)
                pads.push(Pad::rectangular(4, pad_centre_x, -y_offset, pad_width, pad_height));
                pads.push(Pad::rectangular(5, pad_centre_x, y_offset, pad_width, pad_height));
            }
            6 => {
                // SOT23-6 / SOT363: 3 leads on each side
                let y_offset = dims.pitch;

                // Left side: pins 1, 2, 3
                pads.push(Pad::rectangular(1, -pad_centre_x, y_offset, pad_width, pad_height));
                pads.push(Pad::rectangular(2, -pad_centre_x, 0.0, pad_width, pad_height));
                pads.push(Pad::rectangular(3, -pad_centre_x, -y_offset, pad_width, pad_height));

                // Right side: pins 4, 5, 6 (counter-clockwise)
                pads.push(Pad::rectangular(4, pad_centre_x, -y_offset, pad_width, pad_height));
                pads.push(Pad::rectangular(5, pad_centre_x, 0.0, pad_width, pad_height));
                pads.push(Pad::rectangular(6, pad_centre_x, y_offset, pad_width, pad_height));
            }
            _ => {
                // Fallback for unusual lead counts - linear arrangement
                let total_height = f64::from(dims.lead_count - 1) * dims.pitch;
                let start_y = total_height / 2.0;

                for i in 0..dims.lead_count {
                    let y = f64::from(i).mul_add(-dims.pitch, start_y);
                    let x = if i % 2 == 0 { -pad_centre_x } else { pad_centre_x };
                    pads.push(Pad::rectangular(i + 1, x, y, pad_width, pad_height));
                }
            }
        }

        pads
    }

    /// Calculates maximum Y extent for courtyard.
    fn max_y_extent(dims: &SotDimensions, pad_height: f64) -> f64 {
        let body_half = dims.body_length / 2.0;
        let leads_half = match dims.lead_count {
            3 => dims.pitch / 2.0 + pad_height / 2.0,
            4..=6 => dims.pitch + pad_height / 2.0,
            _ => (f64::from(dims.lead_count - 1) * dims.pitch / 2.0) + pad_height / 2.0,
        };

        body_half.max(leads_half)
    }

    /// Creates silkscreen lines for SOT component.
    fn create_silkscreen(
        dims: &SotDimensions,
        pad_centre_x: f64,
        pad_width: f64,
        pad_height: f64,
    ) -> Silkscreen {
        let line_width = 0.15;   // Standard silkscreen line width
        let clearance = 0.15;    // Clearance from pads

        let body_half_x = dims.body_width / 2.0;
        let body_half_y = dims.body_length / 2.0;
        let pad_inner_edge = pad_centre_x - (pad_width / 2.0) - clearance;

        // Calculate Y positions where pads are located
        let max_pad_y = match dims.lead_count {
            3 => dims.pitch / 2.0 + pad_height / 2.0 + clearance,
            4..=6 => dims.pitch + pad_height / 2.0 + clearance,
            _ => (f64::from(dims.lead_count - 1) * dims.pitch / 2.0) + pad_height / 2.0 + clearance,
        };

        let mut lines = Vec::new();

        // Top and bottom lines (if body extends beyond pads)
        if body_half_y > max_pad_y {
            // Top line
            lines.push(Line::new(-body_half_x, body_half_y, body_half_x, body_half_y));
            // Bottom line
            lines.push(Line::new(-body_half_x, -body_half_y, body_half_x, -body_half_y));
        }

        // Left and right lines between pads (if space permits)
        if pad_inner_edge > body_half_x {
            // Vertical lines at body edge
            if body_half_y > max_pad_y {
                lines.push(Line::new(-body_half_x, -body_half_y, -body_half_x, body_half_y));
                lines.push(Line::new(body_half_x, -body_half_y, body_half_x, body_half_y));
            }
        }

        // Pin 1 indicator (small line or notch at pin 1 corner)
        let indicator_size = 0.30;
        let indicator_x = -body_half_x - indicator_size;
        let indicator_y = match dims.lead_count {
            3 => dims.pitch / 2.0,
            _ => dims.pitch,
        };

        // Pin 1 indicator line (diagonal mark near pin 1)
        if indicator_x.abs() < pad_inner_edge {
            lines.push(Line::new(
                -body_half_x,
                indicator_y + indicator_size,
                -body_half_x - indicator_size,
                indicator_y,
            ));
        }

        if lines.is_empty() {
            Silkscreen::empty()
        } else {
            Silkscreen::from_lines(lines, line_width)
        }
    }
}

impl PackageCalculator for SotCalculator {
    fn calculate(
        &self,
        dims: &crate::ipc7351::packages::PackageDimensions,
        density: DensityLevel,
    ) -> LandPattern {
        // Convert generic dimensions to SOT dimensions
        let sot_dims = SotDimensions {
            body_length: dims.body_length,
            body_width: dims.body_width,
            height: dims.height,
            lead_width: dims.terminal_width,
            lead_length: dims.terminal_length,
            pitch: dims.pitch.unwrap_or(0.95), // Default SOT23 pitch
            lead_span: 2.0f64.mul_add(dims.terminal_length, dims.body_width),
            lead_count: dims.pin_count,
            variant: SotVariant::Sot23,
        };

        Self::calculate(self, &sot_dims, density)
    }
}

/// Rounds a value to the nearest multiple of step.
fn round_to(value: f64, step: f64) -> f64 {
    (value / step).round() * step
}

/// Rounds a value up to the nearest multiple of step.
fn round_up_to(value: f64, step: f64) -> f64 {
    (value / step).ceil() * step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sot23_3_dimensions() {
        let dims = SotDimensions::sot23(3);
        assert_eq!(dims.lead_count, 3);
        assert!((dims.pitch - 0.95).abs() < 0.001);
        assert!((dims.body_length - 2.90).abs() < 0.01);
    }

    #[test]
    fn sot23_6_dimensions() {
        let dims = SotDimensions::sot23(6);
        assert_eq!(dims.lead_count, 6);
        assert!((dims.pitch - 0.95).abs() < 0.001);
    }

    #[test]
    fn sot223_dimensions() {
        let dims = SotDimensions::sot223();
        assert_eq!(dims.lead_count, 4);
        assert!((dims.pitch - 2.30).abs() < 0.001);
        assert!(dims.variant == SotVariant::Sot223);
    }

    #[test]
    fn calculate_sot23_3_nominal() {
        let dims = SotDimensions::sot23(3);
        let calc = SotCalculator::new();

        let pattern = calc.calculate(&dims, DensityLevel::Nominal);

        // Should have 3 pads
        assert_eq!(pattern.pads.len(), 3);

        // Pads should be positioned correctly
        let pad1 = &pattern.pads[0];
        let pad2 = &pattern.pads[1];
        let pad3 = &pattern.pads[2];

        // Pads 1 and 2 on left side (negative X)
        assert!(pad1.x < 0.0);
        assert!(pad2.x < 0.0);
        // Pad 3 on right side (positive X)
        assert!(pad3.x > 0.0);

        // Pads 1 and 2 should be symmetric about X axis
        assert!((pad1.y + pad2.y).abs() < 0.001);

        // IPC name should be correct format
        assert!(pattern.ipc_name.starts_with("SOT"));
        assert!(pattern.ipc_name.ends_with('N'));
    }

    #[test]
    fn calculate_sot23_6_nominal() {
        let dims = SotDimensions::sot23(6);
        let calc = SotCalculator::new();

        let pattern = calc.calculate(&dims, DensityLevel::Nominal);

        // Should have 6 pads
        assert_eq!(pattern.pads.len(), 6);

        // 3 pads on left, 3 on right
        let left_count = pattern.pads.iter().filter(|p| p.x < 0.0).count();
        let right_count = pattern.pads.iter().filter(|p| p.x > 0.0).count();

        assert_eq!(left_count, 3);
        assert_eq!(right_count, 3);
    }

    #[test]
    fn calculate_all_densities() {
        let dims = SotDimensions::sot23(3);
        let calc = SotCalculator::new();

        let most = calc.calculate(&dims, DensityLevel::Most);
        let nominal = calc.calculate(&dims, DensityLevel::Nominal);
        let least = calc.calculate(&dims, DensityLevel::Least);

        // Most density should have largest pads
        assert!(most.pads[0].width >= nominal.pads[0].width);
        assert!(nominal.pads[0].width >= least.pads[0].width);

        // Courtyard should follow same pattern
        assert!(most.courtyard.width() >= nominal.courtyard.width());
        assert!(nominal.courtyard.width() >= least.courtyard.width());
    }

    #[test]
    fn heel_spacing_calculation() {
        let dims = SotDimensions::sot23(3);
        let expected = 2.0f64.mul_add(-dims.lead_length, dims.body_width);
        assert!((dims.heel_spacing() - expected).abs() < 0.001);
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn sot323_smaller_than_sot23() {
        let sot23 = SotDimensions::sot23(3);
        let sot323 = SotDimensions::sot323(3);

        assert!(sot323.body_length < sot23.body_length);
        assert!(sot323.pitch < sot23.pitch);
    }
}
