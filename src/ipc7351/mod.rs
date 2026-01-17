//! IPC-7351B land pattern standard implementation.
//!
//! This module provides IPC-7351B compliant land pattern calculations for
//! surface mount components. It implements the formulas and naming conventions
//! from the IPC-7351B specification.
//!
//! # Density Levels
//!
//! IPC-7351B defines three density levels:
//!
//! - **Most (M)**: Maximum land protrusion, best solder fillet, for high reliability
//! - **Nominal (N)**: Standard density, recommended for most applications
//! - **Least (L)**: Minimum land protrusion, for high-density boards
//!
//! # Supported Package Types
//!
//! Currently implemented:
//! - [`packages::chip`] - Chip resistors, capacitors, inductors (0201-2512)
//!
//! Planned:
//! - SOIC, SSOP, TSSOP, MSOP - Small outline packages
//! - QFP, LQFP, TQFP - Quad flat packages
//! - QFN, DFN, SON - Quad flat no-lead packages
//! - BGA, CSP - Ball grid arrays
//! - SOT family - Small outline transistors
//! - Discrete - MELF, SOD, SMA/SMB/SMC
//!
//! # Example
//!
//! ```
//! use altium_designer_mcp::ipc7351::{
//!     density::DensityLevel,
//!     packages::{PackageDimensions, chip::ChipCalculator},
//! };
//!
//! // Calculate footprint for 0603 resistor
//! let dims = PackageDimensions::chip(1.6, 0.8, 0.35, 0.45);
//! let calc = ChipCalculator::new();
//! let pattern = calc.calculate(&dims, DensityLevel::Nominal);
//!
//! println!("IPC Name: {}", pattern.ipc_name);
//! println!("Pad 1: {:?}", pattern.pads[0]);
//! ```

pub mod density;
pub mod naming;
pub mod packages;

pub use density::{CourtyardExcess, DensityLevel, SolderFilletGoals};
pub use packages::{LandPattern, PackageCalculator, PackageDimensions};
