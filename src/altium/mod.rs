//! Altium Designer file format handling.
//!
//! This module provides read/write capabilities for Altium Designer library files:
//!
//! - `.PcbLib` — PCB footprint libraries
//! - `.SchLib` — Schematic symbol libraries (TODO)
//!
//! # File Format
//!
//! Altium library files are OLE Compound Documents (CFB format) containing:
//!
//! - A `FileHeader` stream with library metadata
//! - One storage per component, each containing:
//!   - `Data` stream with primitives (pads, tracks, arcs, etc.)
//!   - `Parameters` stream with component properties
//!
//! # Architecture
//!
//! This module provides low-level file I/O. The AI handles:
//! - IPC-7351B calculations
//! - Package layout decisions
//! - Style choices

pub mod error;
pub mod pcblib;

pub use error::{AltiumError, AltiumResult};
pub use pcblib::{Footprint, PcbLib};
