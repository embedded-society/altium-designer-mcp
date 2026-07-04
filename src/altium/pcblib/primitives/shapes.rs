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
    /// Solder mask expansion in mm. `None` (the default) writes 0; preserved
    /// when reading an Altium-authored track (round-trip fidelity, #113).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub solder_mask_expansion: Option<f64>,
    /// Keepout restrictions bitmask. `None` (the default) writes 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keepout_restrictions: Option<u8>,
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
            solder_mask_expansion: None,
            keepout_restrictions: None,
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
///     solder_mask_expansion: None,
///     keepout_restrictions: None,
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
    /// Solder mask expansion in mm. `None` (the default) writes 0; preserved
    /// when reading an Altium-authored arc (round-trip fidelity, #113).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub solder_mask_expansion: Option<f64>,
    /// Keepout restrictions bitmask. `None` (the default) writes 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keepout_restrictions: Option<u8>,
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
            solder_mask_expansion: None,
            keepout_restrictions: None,
        }
    }
}

/// The kind of a region, from the `KIND` key of its nested parameter block.
///
/// Altium serialises this as an integer (`KIND=0` for copper). We model the two
/// documented values plus an `Other(i32)` catch-all so an unrecognised kind
/// round-trips verbatim. `Copper` (=`KIND=0`) is the from-scratch default and the
/// value the writer historically hard-coded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    /// Copper pour / polygon fill (`KIND=0`). Altium's default.
    #[default]
    Copper,
    /// Board / polygon cutout (`KIND=1`).
    Cutout,
    /// Any other `KIND` integer Altium may write; preserved for round-trip.
    Other(i32),
}

impl RegionKind {
    /// Creates from the Altium `KIND` integer (`0` = `Copper`, `1` = `Cutout`).
    #[must_use]
    pub const fn from_id(id: i32) -> Self {
        match id {
            0 => Self::Copper,
            1 => Self::Cutout,
            other => Self::Other(other),
        }
    }

    /// Returns the Altium `KIND` integer.
    #[must_use]
    pub const fn to_id(self) -> i32 {
        match self {
            Self::Copper => 0,
            Self::Cutout => 1,
            Self::Other(other) => other,
        }
    }
}

/// A filled region (polygon).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    /// Vertices of the polygon.
    pub vertices: Vec<Vertex>,
    /// Interior hole contours (multi-contour region). Empty for a simple polygon.
    /// Each contour is appended after the outline as `[u32 count][count x 16-byte
    /// (x, y) doubles]`, mirroring Altium's `WriteRegion`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Vec<Vertex>>,
    /// Layer the region is on.
    pub layer: Layer,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
    /// Region kind — the `KIND` param key. `Copper` (=`KIND=0`) is Altium's default
    /// and the headline distinction between a copper pour and a cutout.
    #[serde(default)]
    pub kind: RegionKind,
    /// Region name — the `NAME` param key. Empty (`NAME=`) from scratch.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Net index into the board's net list — common-header u16 @3 (and the optional
    /// `NET` param). `0xFFFF` (65535) means "no net", the from-scratch default.
    #[serde(default = "default_region_net_index")]
    pub net_index: u16,
    /// Polygon index this region belongs to — common-header u16 @5. `0xFFFF`
    /// (none) from scratch, matching the historical writer output.
    #[serde(default = "default_region_polygon_index")]
    pub polygon_index: u16,
    /// Component index into the board's component list — common-header u16 @7
    /// (`0xFFFF` stored, exposed as `-1`). `-1` (free primitive) from scratch.
    #[serde(default = "default_region_component_index")]
    pub component_index: i32,
    /// Arc-approximation resolution in mm — the `ARCRESOLUTION` param (Altium formats
    /// it mil-suffixed, e.g. `0.5mil`). `0.0` (`0mil`) from scratch.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub arc_resolution: f64,
    /// Cavity height in mm for embedded components — the `CAVITYHEIGHT` param
    /// (mil-suffixed). `0.0` (`0mil`) from scratch.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub cavity_height: f64,
    /// Sub-polygon index — the `SUBPOLYINDEX` param. `-1` from scratch.
    #[serde(default = "default_region_sub_poly_index")]
    pub sub_poly_index: i32,
    /// Union index for grouped primitives — the `UNIONINDEX` param. `0` from scratch.
    #[serde(default)]
    pub union_index: i32,
    /// Whether the region is shape-based — the `ISSHAPEBASED` param. `false` from scratch.
    #[serde(default)]
    pub is_shape_based: bool,
    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

/// Default net index for a from-scratch region (`0xFFFF` = no net).
const fn default_region_net_index() -> u16 {
    0xFFFF
}

/// Default polygon index for a from-scratch region (`0xFFFF` = none).
const fn default_region_polygon_index() -> u16 {
    0xFFFF
}

/// Default component index for a from-scratch region (`-1` = free primitive).
const fn default_region_component_index() -> i32 {
    -1
}

/// Default sub-polygon index for a from-scratch region (`-1`).
const fn default_region_sub_poly_index() -> i32 {
    -1
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
            ..Self::default()
        }
    }
}

impl Default for Region {
    fn default() -> Self {
        Self {
            vertices: Vec::new(),
            holes: Vec::new(),
            layer: Layer::default(),
            flags: PcbFlags::empty(),
            kind: RegionKind::Copper,
            name: String::new(),
            net_index: default_region_net_index(),
            polygon_index: default_region_polygon_index(),
            component_index: default_region_component_index(),
            arc_resolution: 0.0,
            cavity_height: 0.0,
            sub_poly_index: default_region_sub_poly_index(),
            union_index: 0,
            is_shape_based: false,
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
