//! `SchLib` graphic shape primitives (rectangles, lines, arcs, ellipses, curves, images).

#[allow(clippy::wildcard_imports)] // sibling primitive types
use super::*;

/// A rectangle shape.
///
/// Coordinates are `f64` schematic units (integer part plus optional `…_Frac`,
/// see `super::coord`); `Eq` is not derived (floats are only `PartialEq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rectangle {
    /// Left X coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x1: f64,
    /// Bottom Y coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y1: f64,
    /// Right X coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x2: f64,
    /// Top Y coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y2: f64,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub line_color: u32,
    /// Fill colour (BGR format).
    #[serde(default)]
    pub fill_color: u32,
    /// Line style (0 = Solid, 1 = Dashed, 2 = Dotted). Maps to the
    /// `LINESTYLEEXT` parameter for rectangles (Altium omits `LINESTYLE`).
    #[serde(default)]
    pub line_style: u8,
    /// Whether the rectangle is filled.
    #[serde(default = "default_true")]
    pub filled: bool,
    /// Whether the rectangle is transparent.
    #[serde(default)]
    pub transparent: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Universal display/lock flags (graphically-locked, disabled, dimmed,
    /// owner-part display mode); omitted from JSON when all default.
    #[serde(default, flatten)]
    pub display_flags: ShapeDisplayFlags,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

const fn default_line_width() -> u8 {
    1
}

impl Rectangle {
    /// Creates a new rectangle. Accepts integer or float coordinates.
    #[must_use]
    pub fn new(
        x1: impl Into<f64>,
        y1: impl Into<f64>,
        x2: impl Into<f64>,
        y2: impl Into<f64>,
    ) -> Self {
        Self {
            x1: x1.into(),
            y1: y1.into(),
            x2: x2.into(),
            y2: y2.into(),
            line_width: 1,
            line_color: 0x00_00_80, // Dark red (BGR)
            fill_color: 0xB0_FF_FF, // Light yellow (BGR), matches Altium's default AreaColor (11599871)
            line_style: 0,
            filled: true,
            transparent: false,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        }
    }
}

/// A line segment.
///
/// Coordinates are `f64` schematic units: Altium stores the integer part plus an
/// optional `…_Frac` companion (see `super::coord`), so a line endpoint can sit
/// off the integer grid. `Eq` is therefore not derived (floats are only `PartialEq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    /// Start X coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x1: f64,
    /// Start Y coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y1: f64,
    /// End X coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x2: f64,
    /// End Y coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y2: f64,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Line style (0 = Solid, 1 = Dashed, 2 = Dotted).
    #[serde(default)]
    pub line_style: u8,
    /// Whether the line is marked not-accessible. Altium tags every line
    /// `IsNotAccesible` (its own single-'s' spelling), so this defaults to true.
    #[serde(default = "default_true")]
    pub is_not_accessible: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Universal display/lock flags; omitted from JSON when all default.
    #[serde(default, flatten)]
    pub display_flags: ShapeDisplayFlags,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

impl Line {
    /// Creates a new line. Accepts integer or float coordinates (`impl Into<f64>`),
    /// so existing integer-literal call sites keep working.
    #[must_use]
    pub fn new(
        x1: impl Into<f64>,
        y1: impl Into<f64>,
        x2: impl Into<f64>,
        y2: impl Into<f64>,
    ) -> Self {
        Self {
            x1: x1.into(),
            y1: y1.into(),
            x2: x2.into(),
            y2: y2.into(),
            line_width: 1,
            color: 0x00_00_80, // Dark red (BGR)
            line_style: 0,
            is_not_accessible: true,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        }
    }
}

/// A polyline (multiple connected line segments).
///
/// Point coordinates are `f64` schematic units (see `super::coord`); `Eq` is
/// not derived (floats are only `PartialEq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polyline {
    /// Points as (x, y) pairs.
    pub points: Vec<(f64, f64)>,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Line style (0 = Solid, 1 = Dashed, 2 = Dotted).
    #[serde(default)]
    pub line_style: u8,
    /// Start endpoint shape.
    #[serde(default)]
    pub start_line_shape: u8,
    /// End endpoint shape.
    #[serde(default)]
    pub end_line_shape: u8,
    /// Size of endpoint shapes.
    #[serde(default)]
    pub line_shape_size: u8,
    /// Whether the polyline is transparent.
    #[serde(default)]
    pub transparent: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Universal display/lock flags; omitted from JSON when all default.
    #[serde(default, flatten)]
    pub display_flags: ShapeDisplayFlags,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

/// A filled polygon.
///
/// Vertex coordinates are `f64` schematic units (see `super::coord`); `Eq` is
/// not derived (floats are only `PartialEq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polygon {
    /// Vertices as (x, y) pairs.
    pub points: Vec<(f64, f64)>,
    /// Border line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Border colour (BGR format).
    #[serde(default)]
    pub line_color: u32,
    /// Fill colour (BGR format).
    #[serde(default)]
    pub fill_color: u32,
    /// Border line style (0 = Solid, 1 = Dashed, 2 = Dotted). Altium omits the
    /// `LineStyle` key when zero, so a solid polygon stays byte-identical.
    #[serde(default)]
    pub line_style: u8,
    /// Whether the polygon is filled.
    #[serde(default = "default_true")]
    pub filled: bool,
    /// Whether the polygon fill is transparent (vs opaque). Altium emits
    /// `Transparent=T` only when true, so an opaque polygon stays byte-identical.
    #[serde(default)]
    pub transparent: bool,
    /// Whether the polygon is marked not-accessible. Altium tags every polygon
    /// `IsNotAccesible` (its own single-'s' spelling), so this defaults to true;
    /// a `false` value round-trips (Altium omits the key when false).
    #[serde(default = "default_true")]
    pub is_not_accessible: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Universal display/lock flags; omitted from JSON when all default.
    #[serde(default, flatten)]
    pub display_flags: ShapeDisplayFlags,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

/// An arc or circle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    /// Centre X coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,
    /// Centre Y coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,
    /// Radius.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub radius: f64,
    /// Whether the arc is marked not-accessible. Altium tags every arc
    /// `IsNotAccesible` (its own single-'s' spelling), so this defaults to true.
    #[serde(default = "default_true")]
    pub is_not_accessible: bool,
    /// Start angle in degrees (0 = right, counter-clockwise).
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub start_angle: f64,
    /// End angle in degrees.
    #[serde(
        default = "default_end_angle",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub end_angle: f64,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Fill colour (BGR format). Maps to the `AreaColor` parameter; omitted
    /// when zero.
    #[serde(default)]
    pub fill_color: u32,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Universal display/lock flags; omitted from JSON when all default.
    #[serde(default, flatten)]
    pub display_flags: ShapeDisplayFlags,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

const fn default_end_angle() -> f64 {
    360.0
}

/// A pie (filled circular sector / wedge) — `SchLib` `RECORD=9`.
///
/// Geometrically an [`Arc`] (centre + radius + start/end angle) closed to its
/// centre and fillable, so it also carries `IsSolid` / `Transparent` like an
/// [`Ellipse`]. Coordinates are `f64` schematic units (see `super::coord`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pie {
    /// Centre X coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,
    /// Centre Y coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,
    /// Radius.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub radius: f64,
    /// Whether the pie is marked not-accessible. Altium tags every shape
    /// `IsNotAccesible` (its own single-'s' spelling), so this defaults to true.
    #[serde(default = "default_true")]
    pub is_not_accessible: bool,
    /// Start angle in degrees (0 = right, counter-clockwise).
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub start_angle: f64,
    /// End angle in degrees.
    #[serde(
        default = "default_end_angle",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub end_angle: f64,
    /// Border line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Border colour (BGR format).
    #[serde(default)]
    pub line_color: u32,
    /// Fill colour (BGR format). Maps to the `AreaColor` parameter; omitted when zero.
    #[serde(default)]
    pub fill_color: u32,
    /// Whether the pie is filled (`IsSolid`).
    #[serde(default)]
    pub filled: bool,
    /// Whether the fill is transparent (vs opaque). Emitted only when true.
    #[serde(default)]
    pub transparent: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Universal display/lock flags; omitted from JSON when all default.
    #[serde(default, flatten)]
    pub display_flags: ShapeDisplayFlags,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

impl Pie {
    /// Creates a new pie (filled sector) with the given centre, radius and angles.
    #[must_use]
    pub fn new(
        x: impl Into<f64>,
        y: impl Into<f64>,
        radius: impl Into<f64>,
        start_angle: f64,
        end_angle: f64,
    ) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            radius: radius.into(),
            is_not_accessible: true,
            start_angle,
            end_angle,
            line_width: 1,
            line_color: 0,
            fill_color: 0,
            filled: true,
            transparent: false,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        }
    }
}

/// An embedded or linked raster image — `SchLib` `RECORD=30`.
///
/// A bounding-box graphic (two corners) with an optional border and fill plus a
/// referenced/embedded picture. This models the record metadata; the raw image
/// bytes of an embedded image live in the library's `/Storage` stream and are a
/// separate fidelity concern (like the pin auxiliary streams). Coordinates are
/// `f64` schematic units (see `super::coord`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // independent Altium record flags
pub struct Image {
    /// First corner X (`Location.X`).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x1: f64,
    /// First corner Y (`Location.Y`).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y1: f64,
    /// Second corner X (`Corner.X`).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x2: f64,
    /// Second corner Y (`Corner.Y`).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y2: f64,
    /// Whether the image is marked not-accessible. Altium tags every shape
    /// `IsNotAccesible` (its own single-'s' spelling), so this defaults to true.
    #[serde(default = "default_true")]
    pub is_not_accessible: bool,
    /// Border line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Border colour (BGR format).
    #[serde(default)]
    pub line_color: u32,
    /// Border line style (0 = Solid). Omitted when zero.
    #[serde(default)]
    pub line_style: u8,
    /// Fill colour (BGR format, `AreaColor`). Omitted when zero.
    #[serde(default)]
    pub fill_color: u32,
    /// Whether the box is filled (`IsSolid`).
    #[serde(default)]
    pub filled: bool,
    /// Whether the fill is transparent. Emitted only when true.
    #[serde(default)]
    pub transparent: bool,
    /// Whether the border is shown (`ShowBorder`).
    #[serde(default)]
    pub show_border: bool,
    /// Whether the image keeps its aspect ratio (`KeepAspect`).
    #[serde(default)]
    pub keep_aspect: bool,
    /// Whether the image bytes are embedded in the library (`EmbedImage`), vs a
    /// link to an external `file_name`.
    #[serde(default)]
    pub embed_image: bool,
    /// The image file name (`FileName`) — the reference name or embedded key.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file_name: String,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Universal display/lock flags; omitted from JSON when all default.
    #[serde(default, flatten)]
    pub display_flags: ShapeDisplayFlags,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

impl Image {
    /// Creates a new image spanning the box `(x1,y1)`-`(x2,y2)` referencing
    /// `file_name`.
    #[must_use]
    pub fn new(
        x1: impl Into<f64>,
        y1: impl Into<f64>,
        x2: impl Into<f64>,
        y2: impl Into<f64>,
        file_name: impl Into<String>,
    ) -> Self {
        Self {
            x1: x1.into(),
            y1: y1.into(),
            x2: x2.into(),
            y2: y2.into(),
            is_not_accessible: true,
            line_width: 1,
            line_color: 0,
            line_style: 0,
            fill_color: 0,
            filled: false,
            transparent: false,
            show_border: false,
            keep_aspect: false,
            embed_image: false,
            file_name: file_name.into(),
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        }
    }
}

/// A cubic Bezier curve.
///
/// Defined by four control points:
/// - Start point (x1, y1)
/// - Control point 1 (x2, y2)
/// - Control point 2 (x3, y3)
/// - End point (x4, y4)
///
/// Coordinates are `f64` schematic units (see `super::coord`); `Eq` is not
/// derived (floats are only `PartialEq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bezier {
    /// Start point X.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x1: f64,
    /// Start point Y.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y1: f64,
    /// Control point 1 X.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x2: f64,
    /// Control point 1 Y.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y2: f64,
    /// Control point 2 X.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x3: f64,
    /// Control point 2 Y.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y3: f64,
    /// End point X.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x4: f64,
    /// End point Y.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y4: f64,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Whether the curve is marked not-accessible. Altium tags every Bezier
    /// `IsNotAccesible` (its own single-'s' spelling), so this defaults to true.
    #[serde(default = "default_true")]
    pub is_not_accessible: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

impl Bezier {
    /// Creates a new Bezier curve.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        x1: impl Into<f64>,
        y1: impl Into<f64>,
        x2: impl Into<f64>,
        y2: impl Into<f64>,
        x3: impl Into<f64>,
        y3: impl Into<f64>,
        x4: impl Into<f64>,
        y4: impl Into<f64>,
    ) -> Self {
        Self {
            x1: x1.into(),
            y1: y1.into(),
            x2: x2.into(),
            y2: y2.into(),
            x3: x3.into(),
            y3: y3.into(),
            x4: x4.into(),
            y4: y4.into(),
            line_width: 1,
            color: 0x00_00_80, // Dark red (BGR)
            is_not_accessible: true,
            owner_part_id: 1,
            unique_id: None,
        }
    }
}

/// An ellipse.
///
/// Coordinates are `f64` schematic units (see `super::coord`); `Eq` is not
/// derived (floats are only `PartialEq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ellipse {
    /// Centre X coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,
    /// Centre Y coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,
    /// X radius (horizontal).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub radius_x: f64,
    /// Y radius (vertical).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub radius_y: f64,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub line_color: u32,
    /// Fill colour (BGR format).
    #[serde(default)]
    pub fill_color: u32,
    /// Whether the ellipse is filled.
    #[serde(default = "default_true")]
    pub filled: bool,
    /// Whether the ellipse is transparent.
    #[serde(default)]
    pub transparent: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Universal display/lock flags; omitted from JSON when all default.
    #[serde(default, flatten)]
    pub display_flags: ShapeDisplayFlags,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

impl Ellipse {
    /// Creates a new ellipse. Accepts integer or float coordinates.
    #[must_use]
    pub fn new(
        x: impl Into<f64>,
        y: impl Into<f64>,
        radius_x: impl Into<f64>,
        radius_y: impl Into<f64>,
    ) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            radius_x: radius_x.into(),
            radius_y: radius_y.into(),
            line_width: 1,
            line_color: 0x00_00_80, // Dark red (BGR)
            fill_color: 0xB0_FF_FF, // Light yellow (BGR), matches Altium's default AreaColor (11599871)
            filled: true,
            transparent: false,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        }
    }

    /// Creates a new circle (equal radii).
    #[must_use]
    pub fn circle(x: impl Into<f64>, y: impl Into<f64>, radius: impl Into<f64>) -> Self {
        let (x, y, radius) = (x.into(), y.into(), radius.into());
        Self::new(x, y, radius, radius)
    }
}

/// A rounded rectangle.
///
/// Defined by two corner points and corner radii for rounding.
/// Coordinates are `f64` schematic units (see `super::coord`); `Eq` is not
/// derived (floats are only `PartialEq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundRect {
    /// Left/bottom X coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x1: f64,
    /// Left/bottom Y coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y1: f64,
    /// Right/top X coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x2: f64,
    /// Right/top Y coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y2: f64,
    /// Corner X radius (horizontal rounding).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub corner_x_radius: f64,
    /// Corner Y radius (vertical rounding).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub corner_y_radius: f64,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub line_color: u32,
    /// Fill colour (BGR format).
    #[serde(default)]
    pub fill_color: u32,
    /// Line style (0 = Solid, 1 = Dashed, 2 = Dotted).
    #[serde(default)]
    pub line_style: u8,
    /// Whether the rectangle is filled.
    #[serde(default = "default_true")]
    pub filled: bool,
    /// Whether the rectangle is transparent.
    #[serde(default)]
    pub transparent: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Universal display/lock flags; omitted from JSON when all default.
    #[serde(default, flatten)]
    pub display_flags: ShapeDisplayFlags,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

impl RoundRect {
    /// Creates a new rounded rectangle.
    #[must_use]
    #[allow(clippy::similar_names)]
    pub fn new(
        x1: impl Into<f64>,
        y1: impl Into<f64>,
        x2: impl Into<f64>,
        y2: impl Into<f64>,
        corner_x_radius: impl Into<f64>,
        corner_y_radius: impl Into<f64>,
    ) -> Self {
        Self {
            x1: x1.into(),
            y1: y1.into(),
            x2: x2.into(),
            y2: y2.into(),
            corner_x_radius: corner_x_radius.into(),
            corner_y_radius: corner_y_radius.into(),
            line_width: 1,
            line_color: 0x00_00_80, // Dark red (BGR)
            fill_color: 0xB0_FF_FF, // Light yellow (BGR), matches Altium's default AreaColor (11599871)
            line_style: 0,
            filled: true,
            transparent: false,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        }
    }
}

/// An elliptical arc.
///
/// An arc segment of an ellipse, defined by centre, radii, and angle range.
/// Radii support fractional parts for precise positioning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EllipticalArc {
    /// Centre X coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,
    /// Centre Y coordinate.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,
    /// Primary radius (horizontal).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub radius: f64,
    /// Secondary radius (vertical).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub secondary_radius: f64,
    /// Start angle in degrees (0 = right, counter-clockwise).
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub start_angle: f64,
    /// End angle in degrees.
    #[serde(
        default = "default_end_angle",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub end_angle: f64,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Fill colour (BGR format). Maps to the `AreaColor` parameter; omitted
    /// when zero.
    #[serde(default)]
    pub fill_color: u32,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

impl EllipticalArc {
    /// Creates a new elliptical arc.
    #[must_use]
    pub fn new(
        x: impl Into<f64>,
        y: impl Into<f64>,
        radius: impl Into<f64>,
        secondary_radius: impl Into<f64>,
        start_angle: impl Into<f64>,
        end_angle: impl Into<f64>,
    ) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            radius: radius.into(),
            secondary_radius: secondary_radius.into(),
            start_angle: start_angle.into(),
            end_angle: end_angle.into(),
            line_width: 1,
            color: 0x00_00_80, // Dark red (BGR)
            fill_color: 0,
            owner_part_id: 1,
            unique_id: None,
        }
    }

    /// Creates a full ellipse (0 to 360 degrees).
    #[must_use]
    pub fn full_ellipse(
        x: impl Into<f64>,
        y: impl Into<f64>,
        radius: impl Into<f64>,
        secondary_radius: impl Into<f64>,
    ) -> Self {
        Self::new(x, y, radius, secondary_radius, 0.0, 360.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filled_shapes_default_to_altium_light_yellow() {
        // Altium's default schematic fill is light yellow = 0xB0FFFF (BGR) = 11599871,
        // the same value emitted as the component header's AreaColor. The earlier
        // default 0xFFFFB0 was that value byte-swapped and rendered as cyan in Altium.
        const ALTIUM_LIGHT_YELLOW: u32 = 0xB0_FF_FF;
        assert_eq!(ALTIUM_LIGHT_YELLOW, 11_599_871);
        assert_eq!(Rectangle::new(0, 0, 10, 10).fill_color, ALTIUM_LIGHT_YELLOW);
        assert_eq!(Ellipse::new(0, 0, 5, 5).fill_color, ALTIUM_LIGHT_YELLOW);
        assert_eq!(
            RoundRect::new(0, 0, 10, 10, 2, 2).fill_color,
            ALTIUM_LIGHT_YELLOW
        );
    }
}
