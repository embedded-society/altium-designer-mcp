//! IPC-7351B density levels and solder fillet goals (J-values).
//!
//! The IPC-7351B standard defines three density levels for land patterns:
//!
//! - **Most (M)**: Maximum solder fillet, largest pads for best reliability
//! - **Nominal (N)**: Standard density, recommended for most applications
//! - **Least (L)**: Minimum solder fillet, smallest pads for high-density boards

use std::fmt;

/// Density level per IPC-7351B.
///
/// Controls the solder fillet goals (J-values) which determine pad dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DensityLevel {
    /// Most (M) - Maximum land protrusion for best solder fillet.
    /// Use for high-reliability applications.
    Most,

    /// Nominal (N) - Standard density, recommended for most applications.
    #[default]
    Nominal,

    /// Least (L) - Minimum land protrusion for high-density boards.
    /// Use when space is critical.
    Least,
}

impl DensityLevel {
    /// Parses a density level from a string.
    ///
    /// Accepts: "M", "Most", "N", "Nominal", "L", "Least" (case-insensitive).
    #[must_use]
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "M" | "MOST" => Some(Self::Most),
            "N" | "NOMINAL" => Some(Self::Nominal),
            "L" | "LEAST" => Some(Self::Least),
            _ => None,
        }
    }

    /// Returns the suffix character for IPC names.
    #[must_use]
    pub const fn suffix(&self) -> char {
        match self {
            Self::Most => 'M',
            Self::Nominal => 'N',
            Self::Least => 'L',
        }
    }
}

impl fmt::Display for DensityLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Most => write!(f, "M"),
            Self::Nominal => write!(f, "N"),
            Self::Least => write!(f, "L"),
        }
    }
}

/// Solder fillet goals (J-values) per IPC-7351B Table 3-2.
///
/// These values determine how much the pad extends beyond the component
/// lead to achieve proper solder fillet formation.
///
/// All values in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolderFilletGoals {
    /// Toe fillet goal (Jt) - extension beyond the outer edge of the lead.
    pub toe: f64,

    /// Heel fillet goal (Jh) - extension beyond the inner edge of the lead.
    pub heel: f64,

    /// Side fillet goal (Js) - extension beyond the side of the lead.
    pub side: f64,
}

impl SolderFilletGoals {
    /// Creates new solder fillet goals.
    #[must_use]
    pub const fn new(toe: f64, heel: f64, side: f64) -> Self {
        Self { toe, heel, side }
    }

    /// Returns the solder fillet goals for chip components (resistors, capacitors).
    ///
    /// Per IPC-7351B Table 3-2 for rectangular or square-end components.
    #[must_use]
    pub const fn for_chip(density: DensityLevel) -> Self {
        match density {
            DensityLevel::Most => Self::new(0.55, 0.00, 0.05),
            DensityLevel::Nominal => Self::new(0.35, 0.00, 0.00),
            DensityLevel::Least => Self::new(0.15, 0.00, -0.05),
        }
    }

    /// Returns the solder fillet goals for MELF (cylindrical) components.
    ///
    /// Per IPC-7351B Table 3-3.
    #[must_use]
    pub const fn for_melf(density: DensityLevel) -> Self {
        match density {
            DensityLevel::Most => Self::new(0.60, 0.20, 0.10),
            DensityLevel::Nominal => Self::new(0.40, 0.10, 0.05),
            DensityLevel::Least => Self::new(0.20, 0.02, 0.01),
        }
    }

    /// Returns the solder fillet goals for gull-wing leads (SOIC, QFP, etc.).
    ///
    /// Per IPC-7351B Table 3-4.
    #[must_use]
    pub const fn for_gull_wing(density: DensityLevel) -> Self {
        match density {
            DensityLevel::Most => Self::new(0.55, 0.45, 0.05),
            DensityLevel::Nominal => Self::new(0.35, 0.35, 0.03),
            DensityLevel::Least => Self::new(0.15, 0.25, 0.01),
        }
    }

    /// Returns the solder fillet goals for J-lead components (PLCC, SOJ).
    ///
    /// Per IPC-7351B Table 3-5.
    #[must_use]
    pub const fn for_j_lead(density: DensityLevel) -> Self {
        match density {
            DensityLevel::Most => Self::new(0.55, 0.10, 0.05),
            DensityLevel::Nominal => Self::new(0.35, 0.00, 0.03),
            DensityLevel::Least => Self::new(0.15, -0.10, 0.01),
        }
    }

    /// Returns the solder fillet goals for no-lead components (QFN, DFN, SON).
    ///
    /// Per IPC-7351B Table 3-7.
    #[must_use]
    pub const fn for_no_lead(density: DensityLevel) -> Self {
        match density {
            DensityLevel::Most => Self::new(0.40, -0.04, 0.05),
            DensityLevel::Nominal => Self::new(0.30, -0.04, 0.00),
            DensityLevel::Least => Self::new(0.20, -0.04, -0.05),
        }
    }

    /// Returns the solder fillet goals for BGA/CSP components.
    ///
    /// Per IPC-7351B Table 3-8.
    /// BGA uses different calculation - pad is typically ball diameter × factor.
    /// These J-values are not directly used; instead we use diameter ratios.
    #[must_use]
    pub const fn for_bga(_density: DensityLevel) -> Self {
        // All density levels use the same calculation for BGA
        Self::new(0.0, 0.0, 0.0)
    }
}

/// Courtyard excess per IPC-7351B.
///
/// The courtyard defines the minimum area required around a component
/// for pick-and-place equipment clearance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CourtyardExcess {
    /// Courtyard excess in millimetres.
    pub excess: f64,
}

impl CourtyardExcess {
    /// Returns the courtyard excess for the given density level.
    ///
    /// Per IPC-7351B Section 3.4.
    #[must_use]
    pub const fn for_density(density: DensityLevel) -> Self {
        let excess = match density {
            DensityLevel::Most => 0.50,
            DensityLevel::Nominal => 0.25,
            DensityLevel::Least => 0.10,
        };
        Self { excess }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_from_string() {
        assert_eq!(DensityLevel::from_str_loose("M"), Some(DensityLevel::Most));
        assert_eq!(DensityLevel::from_str_loose("most"), Some(DensityLevel::Most));
        assert_eq!(DensityLevel::from_str_loose("N"), Some(DensityLevel::Nominal));
        assert_eq!(DensityLevel::from_str_loose("nominal"), Some(DensityLevel::Nominal));
        assert_eq!(DensityLevel::from_str_loose("L"), Some(DensityLevel::Least));
        assert_eq!(DensityLevel::from_str_loose("least"), Some(DensityLevel::Least));
        assert_eq!(DensityLevel::from_str_loose("X"), None);
    }

    #[test]
    fn density_suffix() {
        assert_eq!(DensityLevel::Most.suffix(), 'M');
        assert_eq!(DensityLevel::Nominal.suffix(), 'N');
        assert_eq!(DensityLevel::Least.suffix(), 'L');
    }

    #[test]
    fn chip_j_values() {
        let goals = SolderFilletGoals::for_chip(DensityLevel::Nominal);
        assert!((goals.toe - 0.35).abs() < f64::EPSILON);
        assert!((goals.heel - 0.00).abs() < f64::EPSILON);
        assert!((goals.side - 0.00).abs() < f64::EPSILON);
    }

    #[test]
    fn courtyard_excess() {
        assert!((CourtyardExcess::for_density(DensityLevel::Most).excess - 0.50).abs() < f64::EPSILON);
        assert!((CourtyardExcess::for_density(DensityLevel::Nominal).excess - 0.25).abs() < f64::EPSILON);
        assert!((CourtyardExcess::for_density(DensityLevel::Least).excess - 0.10).abs() < f64::EPSILON);
    }
}
