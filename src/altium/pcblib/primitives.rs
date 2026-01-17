//! Footprint primitive types for `PcbLib` files.
//!
//! These types represent the geometric elements that make up a footprint:
//! pads, tracks, arcs, regions, and text.

use serde::{Deserialize, Serialize};

/// A PCB pad (SMD or through-hole).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pad {
    /// Pad designator (e.g., "1", "2", "A1").
    pub designator: String,

    /// X position in mm (from footprint origin).
    pub x: f64,

    /// Y position in mm (from footprint origin).
    pub y: f64,

    /// Pad width in mm.
    pub width: f64,

    /// Pad height in mm.
    pub height: f64,

    /// Pad shape.
    #[serde(default)]
    pub shape: PadShape,

    /// Layer the pad is on.
    #[serde(default)]
    pub layer: Layer,

    /// Hole diameter for through-hole pads (mm). None for SMD pads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hole_size: Option<f64>,

    /// Rotation angle in degrees.
    #[serde(default)]
    pub rotation: f64,
}

impl Pad {
    /// Creates a new SMD pad.
    #[must_use]
    pub fn smd(designator: impl Into<String>, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            designator: designator.into(),
            x,
            y,
            width,
            height,
            shape: PadShape::RoundedRectangle,
            layer: Layer::MultiLayer,
            hole_size: None,
            rotation: 0.0,
        }
    }

    /// Creates a new through-hole pad.
    #[must_use]
    pub fn through_hole(
        designator: impl Into<String>,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        hole_size: f64,
    ) -> Self {
        Self {
            designator: designator.into(),
            x,
            y,
            width,
            height,
            shape: PadShape::Round,
            layer: Layer::MultiLayer,
            hole_size: Some(hole_size),
            rotation: 0.0,
        }
    }
}

/// Pad shape types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadShape {
    /// Rectangular pad.
    Rectangle,
    /// Rectangular pad with rounded corners (most common for SMD).
    #[default]
    RoundedRectangle,
    /// Circular pad.
    Round,
    /// Oval/oblong pad.
    Oval,
}

/// A track (line segment) on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Start X position in mm.
    pub x1: f64,
    /// Start Y position in mm.
    pub y1: f64,
    /// End X position in mm.
    pub x2: f64,
    /// End Y position in mm.
    pub y2: f64,
    /// Line width in mm.
    pub width: f64,
    /// Layer the track is on.
    pub layer: Layer,
}

impl Track {
    /// Creates a new track.
    #[must_use]
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64, width: f64, layer: Layer) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            width,
            layer,
        }
    }
}

/// An arc or circle on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    /// Center X position in mm.
    pub x: f64,
    /// Center Y position in mm.
    pub y: f64,
    /// Radius in mm.
    pub radius: f64,
    /// Start angle in degrees (0 = right, counter-clockwise).
    pub start_angle: f64,
    /// End angle in degrees.
    pub end_angle: f64,
    /// Line width in mm.
    pub width: f64,
    /// Layer the arc is on.
    pub layer: Layer,
}

impl Arc {
    /// Creates a full circle.
    #[must_use]
    pub const fn circle(x: f64, y: f64, radius: f64, width: f64, layer: Layer) -> Self {
        Self {
            x,
            y,
            radius,
            start_angle: 0.0,
            end_angle: 360.0,
            width,
            layer,
        }
    }
}

/// A filled region (polygon).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    /// Vertices of the polygon.
    pub vertices: Vec<Vertex>,
    /// Layer the region is on.
    pub layer: Layer,
}

impl Region {
    /// Creates a rectangular region.
    #[must_use]
    pub fn rectangle(min_x: f64, min_y: f64, max_x: f64, max_y: f64, layer: Layer) -> Self {
        Self {
            vertices: vec![
                Vertex { x: min_x, y: min_y },
                Vertex { x: max_x, y: min_y },
                Vertex { x: max_x, y: max_y },
                Vertex { x: min_x, y: max_y },
            ],
            layer,
        }
    }
}

/// A vertex in a region polygon.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    /// X position in mm.
    pub x: f64,
    /// Y position in mm.
    pub y: f64,
}

/// A text string on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    /// X position in mm.
    pub x: f64,
    /// Y position in mm.
    pub y: f64,
    /// Text content.
    pub text: String,
    /// Text height in mm.
    pub height: f64,
    /// Layer the text is on.
    pub layer: Layer,
    /// Rotation angle in degrees.
    #[serde(default)]
    pub rotation: f64,
}

/// A 3D model reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model3D {
    /// Path to the STEP file.
    pub filepath: String,
    /// X offset from footprint origin in mm.
    #[serde(default)]
    pub x_offset: f64,
    /// Y offset from footprint origin in mm.
    #[serde(default)]
    pub y_offset: f64,
    /// Z offset from board surface in mm.
    #[serde(default)]
    pub z_offset: f64,
    /// Rotation around Z axis in degrees.
    #[serde(default)]
    pub rotation: f64,
}

/// Altium layer identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Layer {
    // Copper layers
    /// Top copper layer.
    TopLayer,
    /// Bottom copper layer.
    BottomLayer,
    /// Multi-layer (all copper layers, for through-hole pads).
    #[default]
    MultiLayer,

    // Silkscreen
    /// Top silkscreen (overlay).
    TopOverlay,
    /// Bottom silkscreen.
    BottomOverlay,

    // Solder mask
    /// Top solder mask.
    TopSolder,
    /// Bottom solder mask.
    BottomSolder,

    // Paste
    /// Top solder paste.
    TopPaste,
    /// Bottom solder paste.
    BottomPaste,

    // Mechanical layers
    /// Mechanical layer 1 (typically assembly outline).
    Mechanical1,
    /// Mechanical layer 2.
    Mechanical2,
    /// Mechanical layer 13 (typically 3D body outline).
    Mechanical13,
    /// Mechanical layer 15 (typically courtyard).
    Mechanical15,

    // Keep-out
    /// Keep-out layer.
    KeepOut,
}

impl Layer {
    /// Returns the Altium layer name string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::TopLayer => "Top Layer",
            Self::BottomLayer => "Bottom Layer",
            Self::MultiLayer => "Multi-Layer",
            Self::TopOverlay => "Top Overlay",
            Self::BottomOverlay => "Bottom Overlay",
            Self::TopSolder => "Top Solder",
            Self::BottomSolder => "Bottom Solder",
            Self::TopPaste => "Top Paste",
            Self::BottomPaste => "Bottom Paste",
            Self::Mechanical1 => "Mechanical 1",
            Self::Mechanical2 => "Mechanical 2",
            Self::Mechanical13 => "Mechanical 13",
            Self::Mechanical15 => "Mechanical 15",
            Self::KeepOut => "Keep-Out Layer",
        }
    }

    /// Parses a layer from its Altium name string.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Top Layer" => Some(Self::TopLayer),
            "Bottom Layer" => Some(Self::BottomLayer),
            "Multi-Layer" => Some(Self::MultiLayer),
            "Top Overlay" => Some(Self::TopOverlay),
            "Bottom Overlay" => Some(Self::BottomOverlay),
            "Top Solder" => Some(Self::TopSolder),
            "Bottom Solder" => Some(Self::BottomSolder),
            "Top Paste" => Some(Self::TopPaste),
            "Bottom Paste" => Some(Self::BottomPaste),
            "Mechanical 1" => Some(Self::Mechanical1),
            "Mechanical 2" => Some(Self::Mechanical2),
            "Mechanical 13" => Some(Self::Mechanical13),
            "Mechanical 15" => Some(Self::Mechanical15),
            "Keep-Out Layer" => Some(Self::KeepOut),
            _ => None,
        }
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
