//! `PcbLib` track, arc, region, and vertex primitives.

#[allow(clippy::wildcard_imports)] // sibling primitive types
use super::*;

/// A track (line segment) on a layer.
///
/// Tracks are used to draw silkscreen outlines, keepout boundaries, and other
/// line-based graphics in a footprint.
///
/// # Example
///
/// Create silkscreen outline tracks for a component:
///
/// ```
/// use altium_designer_mcp::altium::pcblib::primitives::{Track, Layer};
///
/// // Draw a box outline with 0.15mm wide lines
/// let tracks = vec![
///     Track::new(-1.0, -0.5, 1.0, -0.5, 0.15, Layer::TopOverlay), // bottom
///     Track::new(1.0, -0.5, 1.0, 0.5, 0.15, Layer::TopOverlay),   // right
///     Track::new(1.0, 0.5, -1.0, 0.5, 0.15, Layer::TopOverlay),   // top
///     Track::new(-1.0, 0.5, -1.0, -0.5, 0.15, Layer::TopOverlay), // left
/// ];
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Start X position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x1: f64,
    /// Start Y position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y1: f64,
    /// End X position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x2: f64,
    /// End Y position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y2: f64,
    /// Line width in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub width: f64,
    /// Layer the track is on.
    pub layer: Layer,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
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
            flags: PcbFlags::empty(),
            unique_id: None,
        }
    }
}

/// An arc or circle on a layer.
///
/// Arcs are used to draw curved silkscreen outlines, pin 1 indicators,
/// and other curved graphics in a footprint.
///
/// # Examples
///
/// Create a circle for pin 1 indicator:
///
/// ```
/// use altium_designer_mcp::altium::pcblib::primitives::{Arc, Layer};
///
/// // Small circle on silkscreen to mark pin 1
/// let pin1_marker = Arc::circle(-1.2, 0.6, 0.2, 0.15, Layer::TopOverlay);
/// ```
///
/// Create a 90-degree arc:
///
/// ```
/// use altium_designer_mcp::altium::pcblib::primitives::{Arc, Layer};
///
/// // Quarter circle from 0° to 90°
/// let arc = Arc {
///     x: 0.0,
///     y: 0.0,
///     radius: 1.0,
///     start_angle: 0.0,
///     end_angle: 90.0,
///     width: 0.15,
///     layer: Layer::TopOverlay,
///     flags: Default::default(),
///     unique_id: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    /// Centre X position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,
    /// Centre Y position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,
    /// Radius in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub radius: f64,
    /// Start angle in degrees (0 = right, counter-clockwise).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub start_angle: f64,
    /// End angle in degrees.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub end_angle: f64,
    /// Line width in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub width: f64,
    /// Layer the arc is on.
    pub layer: Layer,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
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
            flags: PcbFlags::empty(),
            unique_id: None,
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
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
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
            flags: PcbFlags::empty(),
            unique_id: None,
        }
    }
}

/// A vertex in a region polygon.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    /// X position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,
    /// Y position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,
}
