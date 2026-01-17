//! IPC-7351B land pattern calculations for chip components.
//!
//! Chip components include:
//! - Resistors (0201, 0402, 0603, 0805, 1206, 1210, 2010, 2512, etc.)
//! - Capacitors (same sizes as resistors)
//! - Inductors (chip inductors)
//!
//! These are 2-terminal rectangular or square-end components per IPC-7351B Section 8.

use crate::ipc7351::density::{CourtyardExcess, DensityLevel, SolderFilletGoals};
use crate::ipc7351::naming;
use crate::ipc7351::packages::{
    Assembly, Courtyard, LandPattern, Line, Pad, PackageCalculator, PackageDimensions, Point,
    Silkscreen,
};

/// Calculator for chip components (resistors, capacitors, inductors).
#[derive(Debug, Default)]
pub struct ChipCalculator;

impl ChipCalculator {
    /// Creates a new chip calculator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Calculates land pattern for a chip component.
    ///
    /// Per IPC-7351B Section 8.1 - Rectangular or Square-End Components.
    ///
    /// # Arguments
    ///
    /// * `dims` - Package dimensions
    /// * `density` - Density level (M/N/L)
    ///
    /// # Land Pattern Calculation
    ///
    /// For chip components:
    /// - Pad width (X) = Terminal length + Toe fillet (Jt) + Heel fillet (Jh)
    /// - Pad height (Y) = Terminal width + 2 × Side fillet (Js)
    /// - Pad spacing = Lead span - Terminal length - Heel fillet (Jh)
    #[must_use]
    pub fn calculate(&self, dims: &PackageDimensions, density: DensityLevel) -> LandPattern {
        let goals = SolderFilletGoals::for_chip(density);
        let courtyard_excess = CourtyardExcess::for_density(density);

        // Calculate pad dimensions per IPC-7351B formulas
        // Z = Lmin + 2*Jt (overall pad span)
        // G = Smax - 2*Jh (gap between pads)
        // X = (Z - G) / 2 (pad width)
        // Y = Wmin + 2*Js (pad height)

        // For chip components, terminal_length is the land (termination) length
        // Lead span is body_length (toe-to-toe for chip)
        let lead_span = dims.lead_span();
        let terminal_length = dims.terminal_length;
        let terminal_width = dims.terminal_width;

        // IPC-7351B tolerances (typical manufacturing tolerances)
        // Using nominal values as the dimensions provided should be nominal
        let tolerance_span = 0.10; // Span tolerance
        let tolerance_term = 0.05; // Terminal length tolerance
        let tolerance_width = 0.05; // Width tolerance

        // Calculate minimum/maximum values
        let l_min = lead_span - tolerance_span; // Minimum overall length
        let s_max = 2.0f64.mul_add(-terminal_length, lead_span) + tolerance_term; // Maximum span (gap between terminals)
        let w_min = terminal_width - tolerance_width; // Minimum width

        // IPC-7351B formulas
        let z_max = 2.0f64.mul_add(goals.toe, l_min); // Maximum land pattern span
        let g_min = 2.0f64.mul_add(-goals.heel, s_max); // Minimum gap
        let pad_width = (z_max - g_min) / 2.0;
        let pad_height = 2.0f64.mul_add(goals.side, w_min);

        // Pad centre positions (symmetric about origin)
        let pad_centre_x = (z_max - pad_width) / 2.0;

        // Round to reasonable precision (0.01mm)
        let pad_width = round_to(pad_width, 0.01);
        let pad_height = round_to(pad_height, 0.01);
        let pad_centre_x = round_to(pad_centre_x, 0.01);

        // Create pads
        let pads = vec![
            Pad::rectangular(1, -pad_centre_x, 0.0, pad_width, pad_height),
            Pad::rectangular(2, pad_centre_x, 0.0, pad_width, pad_height),
        ];

        // Calculate courtyard
        let courtyard_x = pad_centre_x + (pad_width / 2.0) + courtyard_excess.excess;
        let courtyard_y = (pad_height / 2.0).max(dims.body_width / 2.0) + courtyard_excess.excess;

        // Round courtyard to grid (0.01mm for L, 0.05mm for N, 0.1mm for M)
        let grid = match density {
            DensityLevel::Most => 0.10,
            DensityLevel::Nominal => 0.05,
            DensityLevel::Least => 0.01,
        };
        let courtyard_x = round_up_to(courtyard_x, grid);
        let courtyard_y = round_up_to(courtyard_y, grid);

        let courtyard = Courtyard::from_bounds(-courtyard_x, -courtyard_y, courtyard_x, courtyard_y);

        // Create silkscreen (lines at top and bottom, avoiding pads)
        let silkscreen = Self::create_silkscreen(dims, pad_centre_x, pad_width);

        // Create assembly outline
        let assembly = Assembly::from_body(dims.body_length, dims.body_width);

        // Generate IPC name
        let ipc_name = naming::chip_name(dims.body_length, dims.body_width, dims.height, density);

        LandPattern {
            ipc_name,
            pads,
            courtyard,
            silkscreen,
            assembly,
            origin: Point::default(),
        }
    }

    /// Creates silkscreen lines for chip component.
    fn create_silkscreen(
        dims: &PackageDimensions,
        pad_centre_x: f64,
        pad_width: f64,
    ) -> Silkscreen {
        let line_width = 0.15; // Standard silkscreen line width
        let clearance = 0.15; // Clearance from pads

        // Silkscreen is typically at the body edge
        let body_half_y = dims.body_width / 2.0;
        let pad_edge_x = pad_centre_x - (pad_width / 2.0) - clearance;

        // Only draw silkscreen if there's enough space between pads
        if pad_edge_x > 0.1 {
            let lines = vec![
                // Top line
                Line::new(-pad_edge_x, body_half_y, pad_edge_x, body_half_y),
                // Bottom line
                Line::new(-pad_edge_x, -body_half_y, pad_edge_x, -body_half_y),
            ];
            Silkscreen::from_lines(lines, line_width)
        } else {
            // Not enough space for silkscreen
            Silkscreen::empty()
        }
    }
}

impl PackageCalculator for ChipCalculator {
    fn calculate(&self, dims: &PackageDimensions, density: DensityLevel) -> LandPattern {
        Self::calculate(self, dims, density)
    }
}

/// Standard chip component sizes per EIA standards.
///
/// Returns `(body_length_mm, body_width_mm, terminal_length_mm, height_mm)`.
#[must_use]
pub fn standard_chip_size(code: &str) -> Option<(f64, f64, f64, f64)> {
    // EIA metric codes (RESC/CAPC sizes)
    match code.to_uppercase().as_str() {
        // Metric codes (actual dimensions)
        "0201" | "0603M" => Some((0.60, 0.30, 0.15, 0.30)),
        "0402" | "1005M" => Some((1.00, 0.50, 0.25, 0.35)),
        "0603" | "1608M" => Some((1.60, 0.80, 0.35, 0.45)),
        "0805" | "2012M" => Some((2.00, 1.25, 0.50, 0.55)),
        "1206" | "3216M" => Some((3.20, 1.60, 0.50, 0.55)),
        "1210" | "3225M" => Some((3.20, 2.50, 0.50, 0.55)),
        "1812" | "4532M" => Some((4.50, 3.20, 0.60, 0.55)),
        "2010" | "5025M" => Some((5.00, 2.50, 0.60, 0.55)),
        "2512" | "6332M" => Some((6.30, 3.20, 0.60, 0.55)),
        _ => None,
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
    fn standard_sizes_exist() {
        assert!(standard_chip_size("0201").is_some());
        assert!(standard_chip_size("0402").is_some());
        assert!(standard_chip_size("0603").is_some());
        assert!(standard_chip_size("0805").is_some());
        assert!(standard_chip_size("1206").is_some());
        assert!(standard_chip_size("2512").is_some());
        assert!(standard_chip_size("INVALID").is_none());
    }

    #[test]
    fn calculate_0603_nominal() {
        let (body_l, body_w, term_l, height) = standard_chip_size("0603").unwrap();
        let dims = PackageDimensions::chip(body_l, body_w, term_l, height);
        let calc = ChipCalculator::new();

        let pattern = calc.calculate(&dims, DensityLevel::Nominal);

        // Should have 2 pads
        assert_eq!(pattern.pads.len(), 2);

        // Pads should be symmetric about origin
        let pad1 = &pattern.pads[0];
        let pad2 = &pattern.pads[1];
        assert!((pad1.x + pad2.x).abs() < 0.001);

        // IPC name should be correct format
        assert!(pattern.ipc_name.starts_with("RESC"));
        assert!(pattern.ipc_name.ends_with('N'));
    }

    #[test]
    fn calculate_0805_all_densities() {
        let (body_l, body_w, term_l, height) = standard_chip_size("0805").unwrap();
        let dims = PackageDimensions::chip(body_l, body_w, term_l, height);
        let calc = ChipCalculator::new();

        let most = calc.calculate(&dims, DensityLevel::Most);
        let nominal = calc.calculate(&dims, DensityLevel::Nominal);
        let least = calc.calculate(&dims, DensityLevel::Least);

        // Most density should have largest pads
        assert!(most.pads[0].width > nominal.pads[0].width);
        assert!(nominal.pads[0].width > least.pads[0].width);

        // Courtyard should follow same pattern
        assert!(most.courtyard.width() > nominal.courtyard.width());
        assert!(nominal.courtyard.width() > least.courtyard.width());
    }

    #[test]
    fn round_to_works() {
        assert!((round_to(1.234, 0.01) - 1.23).abs() < 0.001);
        assert!((round_to(1.235, 0.01) - 1.24).abs() < 0.001);
        assert!((round_to(1.27, 0.05) - 1.25).abs() < 0.001);
    }

    #[test]
    fn round_up_to_works() {
        assert!((round_up_to(1.01, 0.05) - 1.05).abs() < 0.001);
        assert!((round_up_to(1.00, 0.05) - 1.00).abs() < 0.001);
        assert!((round_up_to(1.06, 0.10) - 1.10).abs() < 0.001);
    }
}
