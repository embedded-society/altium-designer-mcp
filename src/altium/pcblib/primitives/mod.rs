//! Footprint primitive types for `PcbLib` files.
//!
//! This module provides types representing the geometric elements that make up a footprint:
//! pads, tracks, arcs, regions, fills, text, and 3D model references.
//!
//! # Coordinate System
//!
//! All coordinates use millimeters (mm) as the unit of measurement. The coordinate system is:
//!
//! - **Origin**: Footprint centre (0, 0)
//! - **X-axis**: Positive to the right
//! - **Y-axis**: Positive upward
//! - **Rotation**: Counter-clockwise, 0° = right (positive X direction)
//!
//! ```text
//!           +Y
//!            │
//!            │
//!   ─────────┼─────────> +X
//!            │
//!            │
//! ```
//!
//! # Layers
//!
//! Primitives are placed on specific layers. The most common layers for footprints are:
//!
//! - [`Layer::TopLayer`] / [`Layer::BottomLayer`]: Copper layers for SMD pads
//! - [`Layer::MultiLayer`]: All copper layers (for through-hole pads)
//! - [`Layer::TopOverlay`] / [`Layer::BottomOverlay`]: Silkscreen
//! - [`Layer::TopPaste`] / [`Layer::BottomPaste`]: Solder paste stencil
//! - [`Layer::TopSolder`] / [`Layer::BottomSolder`]: Solder mask openings
//!
//! # Internal Units
//!
//! Altium's binary format uses internal units (1/10000 of a mil = 2.54 nm).
//! This module handles all conversions automatically - you always work in millimeters.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

mod layer;
pub use layer::Layer;
mod models3d;
pub use models3d::{ComponentBody, EmbeddedModel, Model3D};
mod pads;
pub use pads::{
    HoleShape, MaskExpansionMode, Pad, PadShape, PadStackMode, PowerPlaneConnectStyle, Via,
    ViaStackMode,
};
mod shapes;
pub use shapes::{Arc, Region, Track, Vertex};
mod text;
pub use text::{Fill, StrokeFont, Text, TextJustification, TextKind};

// Coordinate rounding on serialization is shared (crate::altium::serde_round).

bitflags! {
    /// Flags for PCB primitives stored in the common header (bytes 1-2).
    ///
    /// These flags control various properties of primitives such as locking,
    /// keep-out zones, and solder mask tenting.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct PcbFlags: u16 {
        /// Primitive is locked and cannot be moved/edited.
        const LOCKED = 0x0001;
        /// Primitive is part of a polygon pour.
        const POLYGON = 0x0002;
        /// Primitive defines a keep-out region.
        const KEEPOUT = 0x0004;
        /// Top solder mask tenting enabled (covers the pad/via).
        const TENTING_TOP = 0x0008;
        /// Bottom solder mask tenting enabled (covers the pad/via).
        const TENTING_BOTTOM = 0x0010;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_smd_creation() {
        let pad = Pad::smd("1", 0.5, 0.0, 0.9, 0.8);
        assert_eq!(pad.designator, "1");
        assert!((pad.x - 0.5).abs() < f64::EPSILON);
        assert!(pad.hole_size.is_none());
    }

    #[test]
    fn pad_through_hole_creation() {
        let pad = Pad::through_hole("1", 0.0, 0.0, 1.5, 1.5, 0.8);
        assert_eq!(pad.hole_size, Some(0.8));
    }

    #[test]
    fn layer_roundtrip() {
        let layer = Layer::TopOverlay;
        assert_eq!(Layer::parse(layer.as_str()), Some(layer));
    }
}
