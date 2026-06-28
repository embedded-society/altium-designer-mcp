//! Schematic symbol primitive types for `SchLib` files.
//!
//! These types represent the graphic elements that make up a schematic symbol:
//! pins, rectangles, lines, arcs, labels, and parameters.

use serde::{Deserialize, Serialize};

// Float rounding on serialization is shared (crate::altium::serde_round).

/// A schematic symbol pin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Pin flags match Altium binary format
pub struct Pin {
    /// Pin name (e.g., "VCC", "GND", "IN").
    pub name: String,

    /// Pin designator (e.g., "1", "2", "A1").
    pub designator: String,

    /// X position in schematic units (10 units = 1 grid).
    pub x: i32,

    /// Y position in schematic units.
    pub y: i32,

    /// Pin length in schematic units.
    pub length: i32,

    /// Pin orientation.
    #[serde(default)]
    pub orientation: PinOrientation,

    /// Electrical type.
    #[serde(default)]
    pub electrical_type: PinElectricalType,

    /// Whether the pin is hidden.
    #[serde(default)]
    pub hidden: bool,

    /// Whether to show the pin name.
    #[serde(default = "default_true")]
    pub show_name: bool,

    /// Whether to show the pin designator.
    #[serde(default = "default_true")]
    pub show_designator: bool,

    /// Pin description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,

    /// Owner part ID (for multi-part symbols).
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,

    /// Pin colour (BGR format).
    #[serde(default)]
    pub colour: u32,

    /// Whether the pin is graphically locked.
    #[serde(default)]
    pub graphically_locked: bool,

    /// Whether the pin is not accessible for selection in Altium's editor
    /// (conglomerate bit `0x20`); preserved on round-trip (#113).
    #[serde(default)]
    pub is_not_accessible: bool,

    /// Symbol decoration on the inner edge (closest to component body).
    #[serde(default)]
    pub symbol_inner_edge: PinSymbol,

    /// Symbol decoration on the outer edge (furthest from component body).
    #[serde(default)]
    pub symbol_outer_edge: PinSymbol,

    /// Symbol decoration inside the pin line.
    #[serde(default)]
    pub symbol_inside: PinSymbol,

    /// Symbol decoration outside the pin line.
    #[serde(default)]
    pub symbol_outside: PinSymbol,

    /// Pin formal type byte. Altium emits `1` for a normal pin; preserved on
    /// round-trip. Non-default values come from Altium-authored files.
    #[serde(default = "default_formal_type")]
    pub formal_type: u8,

    /// Pin swap-id group (empty for a from-scratch pin).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub swap_id_group: String,

    /// Pin part-and-sequence swap id. Altium's default for a fresh pin is `|&|`.
    #[serde(default = "default_part_and_sequence")]
    pub part_and_sequence: String,

    /// Pin default value (empty for a from-scratch pin).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_value: String,
}

const fn default_true() -> bool {
    true
}

const fn default_owner_part() -> i32 {
    1
}

const fn default_formal_type() -> u8 {
    1
}

fn default_part_and_sequence() -> String {
    "|&|".to_string()
}

impl Pin {
    /// Creates a new pin with the given name and designator.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        designator: impl Into<String>,
        x: i32,
        y: i32,
        length: i32,
        orientation: PinOrientation,
    ) -> Self {
        Self {
            name: name.into(),
            designator: designator.into(),
            x,
            y,
            length,
            orientation,
            electrical_type: PinElectricalType::Passive,
            hidden: false,
            show_name: true,
            show_designator: true,
            description: String::new(),
            owner_part_id: 1,
            colour: 0,
            graphically_locked: false,
            is_not_accessible: false,
            symbol_inner_edge: PinSymbol::None,
            symbol_outer_edge: PinSymbol::None,
            symbol_inside: PinSymbol::None,
            symbol_outside: PinSymbol::None,
            formal_type: 1,
            swap_id_group: String::new(),
            part_and_sequence: "|&|".to_string(),
            default_value: String::new(),
        }
    }
}

/// Pin symbol decoration (visual indicators on pin graphics).
///
/// These decorations appear at different positions on the pin to indicate
/// electrical characteristics or signal flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinSymbol {
    /// No decoration.
    #[default]
    None,
    /// Inversion dot (bubble).
    Dot,
    /// Right-to-left signal flow arrow.
    RightLeftSignalFlow,
    /// Clock input indicator.
    Clock,
    /// Active low input bar.
    ActiveLowInput,
    /// Analog signal input.
    AnalogSignalIn,
    /// Not a logic connection.
    NotLogicConnection,
    /// Postponed output.
    PostponedOutput,
    /// Open collector output.
    OpenCollector,
    /// High impedance.
    HiZ,
    /// High current.
    HighCurrent,
    /// Pulse.
    Pulse,
    /// Schmitt trigger input.
    Schmitt,
    /// Active low output bar.
    ActiveLowOutput,
    /// Open collector with pull-up.
    OpenCollectorPullUp,
    /// Open emitter output.
    OpenEmitter,
    /// Open emitter with pull-up.
    OpenEmitterPullUp,
    /// Digital signal input.
    DigitalSignalIn,
    /// Shift left.
    ShiftLeft,
    /// Open output.
    OpenOutput,
    /// Left-to-right signal flow arrow.
    LeftRightSignalFlow,
    /// Bidirectional signal flow.
    BidirectionalSignalFlow,
}

impl PinSymbol {
    /// Creates from Altium symbol ID.
    #[must_use]
    pub const fn from_id(id: u8) -> Self {
        match id {
            1 => Self::Dot,
            2 => Self::RightLeftSignalFlow,
            3 => Self::Clock,
            4 => Self::ActiveLowInput,
            5 => Self::AnalogSignalIn,
            6 => Self::NotLogicConnection,
            7 => Self::PostponedOutput,
            8 => Self::OpenCollector,
            9 => Self::HiZ,
            10 => Self::HighCurrent,
            11 => Self::Pulse,
            12 => Self::Schmitt,
            13 => Self::ActiveLowOutput,
            14 => Self::OpenCollectorPullUp,
            15 => Self::OpenEmitter,
            16 => Self::OpenEmitterPullUp,
            17 => Self::DigitalSignalIn,
            18 => Self::ShiftLeft,
            19 => Self::OpenOutput,
            20 => Self::LeftRightSignalFlow,
            21 => Self::BidirectionalSignalFlow,
            _ => Self::None,
        }
    }

    /// Returns the Altium symbol ID.
    #[must_use]
    pub const fn to_id(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Dot => 1,
            Self::RightLeftSignalFlow => 2,
            Self::Clock => 3,
            Self::ActiveLowInput => 4,
            Self::AnalogSignalIn => 5,
            Self::NotLogicConnection => 6,
            Self::PostponedOutput => 7,
            Self::OpenCollector => 8,
            Self::HiZ => 9,
            Self::HighCurrent => 10,
            Self::Pulse => 11,
            Self::Schmitt => 12,
            Self::ActiveLowOutput => 13,
            Self::OpenCollectorPullUp => 14,
            Self::OpenEmitter => 15,
            Self::OpenEmitterPullUp => 16,
            Self::DigitalSignalIn => 17,
            Self::ShiftLeft => 18,
            Self::OpenOutput => 19,
            Self::LeftRightSignalFlow => 20,
            Self::BidirectionalSignalFlow => 21,
        }
    }
}

/// Pin orientation (direction the pin points).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinOrientation {
    /// Pin points right (connection on left side).
    #[default]
    Right,
    /// Pin points left (connection on right side).
    Left,
    /// Pin points up (connection on bottom).
    Up,
    /// Pin points down (connection on top).
    Down,
}

impl PinOrientation {
    /// Creates orientation from rotation and flip flags.
    #[must_use]
    pub const fn from_flags(rotated: bool, flipped: bool) -> Self {
        match (rotated, flipped) {
            (false, false) => Self::Right,
            (false, true) => Self::Left,
            (true, false) => Self::Up,
            (true, true) => Self::Down,
        }
    }

    /// Returns the rotation and flip flags for this orientation.
    #[must_use]
    pub const fn to_flags(self) -> (bool, bool) {
        match self {
            Self::Right => (false, false),
            Self::Left => (false, true),
            Self::Up => (true, false),
            Self::Down => (true, true),
        }
    }
}

/// Pin electrical type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinElectricalType {
    /// Input pin.
    Input,
    /// Bidirectional pin (input/output).
    #[serde(alias = "input_output")]
    Bidirectional,
    /// Output pin.
    Output,
    /// Open collector output.
    OpenCollector,
    /// Passive component (resistor, capacitor).
    #[default]
    Passive,
    /// High impedance / tri-state.
    HiZ,
    /// Open emitter output.
    OpenEmitter,
    /// Power pin (VCC, GND).
    Power,
}

impl PinElectricalType {
    /// Creates from Altium electrical type ID.
    #[must_use]
    pub const fn from_id(id: u8) -> Self {
        match id {
            0 => Self::Input,
            1 => Self::Bidirectional,
            2 => Self::Output,
            3 => Self::OpenCollector,
            5 => Self::HiZ,
            6 => Self::OpenEmitter,
            7 => Self::Power,
            // 4 and unknown IDs default to Passive
            _ => Self::Passive,
        }
    }

    /// Returns the Altium electrical type ID.
    #[must_use]
    pub const fn to_id(self) -> u8 {
        match self {
            Self::Input => 0,
            Self::Bidirectional => 1,
            Self::Output => 2,
            Self::OpenCollector => 3,
            Self::Passive => 4,
            Self::HiZ => 5,
            Self::OpenEmitter => 6,
            Self::Power => 7,
        }
    }
}

/// A rectangle shape.
///
/// Coordinates are `f64` schematic units (integer part plus optional `…_Frac`,
/// see [`super::coord`]); `Eq` is not derived (floats are only `PartialEq`).
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
            unique_id: None,
        }
    }
}

/// A line segment.
///
/// Coordinates are `f64` schematic units: Altium stores the integer part plus an
/// optional `…_Frac` companion (see [`super::coord`]), so a line endpoint can sit
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
            unique_id: None,
        }
    }
}

/// A polyline (multiple connected line segments).
///
/// Point coordinates are `f64` schematic units (see [`super::coord`]); `Eq` is
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
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

/// A filled polygon.
///
/// Vertex coordinates are `f64` schematic units (see [`super::coord`]); `Eq` is
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
    /// Whether the polygon is filled.
    #[serde(default = "default_true")]
    pub filled: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
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
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

const fn default_end_angle() -> f64 {
    360.0
}

/// A cubic Bezier curve.
///
/// Defined by four control points:
/// - Start point (x1, y1)
/// - Control point 1 (x2, y2)
/// - Control point 2 (x3, y3)
/// - End point (x4, y4)
///
/// Coordinates are `f64` schematic units (see [`super::coord`]); `Eq` is not
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
/// Coordinates are `f64` schematic units (see [`super::coord`]); `Eq` is not
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
/// Coordinates are `f64` schematic units (see [`super::coord`]); `Eq` is not
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
        radius: f64,
        secondary_radius: f64,
        start_angle: f64,
        end_angle: f64,
    ) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            radius,
            secondary_radius,
            start_angle,
            end_angle,
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
        radius: f64,
        secondary_radius: f64,
    ) -> Self {
        Self::new(x, y, radius, secondary_radius, 0.0, 360.0)
    }
}

/// A text label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    /// X position.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,
    /// Y position.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,
    /// Text content.
    pub text: String,
    /// Font ID (1-based index into library fonts).
    #[serde(default = "default_font_id")]
    pub font_id: u8,
    /// Text colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Text justification.
    #[serde(default = "default_justification")]
    pub justification: TextJustification,
    /// Rotation in degrees.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub rotation: f64,
    /// Whether the label is mirrored horizontally.
    #[serde(default)]
    pub is_mirrored: bool,
    /// Whether the label is hidden.
    #[serde(default)]
    pub is_hidden: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

/// A text annotation (RECORD=3).
///
/// Similar to Label but uses different record format and positioning.
/// Used for general text annotations on schematic symbols.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    /// X position.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,
    /// Y position.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,
    /// Text content.
    pub text: String,
    /// Font ID (1-based index into library fonts).
    #[serde(default = "default_font_id")]
    pub font_id: u8,
    /// Text colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Text justification.
    #[serde(default = "default_justification")]
    pub justification: TextJustification,
    /// Rotation in degrees.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub rotation: f64,
    /// Whether the text is mirrored horizontally.
    #[serde(default)]
    pub is_mirrored: bool,
    /// Whether the text is hidden.
    #[serde(default)]
    pub is_hidden: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// shape identity; a from-scratch shape generates a fresh one on write (#113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

const fn default_font_id() -> u8 {
    1
}

/// Text justification. Shared with `PcbLib`; the canonical definition is
/// [`crate::altium::TextJustification`].
pub use crate::altium::TextJustification;

/// `SchLib`'s per-field default justification. Unlike `PcbLib` (which defaults
/// to `MiddleCenter`, the shared enum's `Default`), `SchLib` text/labels default
/// to `BottomLeft`, so the `justification` fields set this explicitly.
const fn default_justification() -> TextJustification {
    TextJustification::BottomLeft
}

/// A component parameter (e.g., Value, Part Number).
///
/// Coordinates are `f64` schematic units (see [`super::coord`]); `Eq` is not
/// derived (floats are only `PartialEq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    /// Parameter name (e.g., "Value", "Part Number").
    pub name: String,
    /// Parameter value (e.g., "10k", "*").
    #[serde(default)]
    pub value: String,
    /// X position.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,
    /// Y position.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,
    /// Font ID.
    #[serde(default = "default_font_id")]
    pub font_id: u8,
    /// Text colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Whether the parameter is hidden.
    #[serde(default)]
    pub hidden: bool,
    /// Read-only state (0 = editable, 1 = read-only).
    #[serde(default)]
    pub read_only_state: u8,
    /// Parameter type (0 = String, 1 = Boolean, 2 = Integer, 3 = Float).
    #[serde(default)]
    pub param_type: u8,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Altium unique ID (8-char). Preserved on read so a round-trip keeps the
    /// parameter identity; a from-scratch parameter generates a fresh one on write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

impl Parameter {
    /// Creates a new parameter.
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            x: 0.0,
            y: 0.0,
            font_id: 1,
            color: 0x80_00_00, // Dark blue (BGR)
            hidden: false,
            read_only_state: 0,
            param_type: 0,
            owner_part_id: 1,
            unique_id: None,
        }
    }
}

/// A footprint model reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootprintModel {
    /// Model name (footprint name in `PcbLib`).
    pub name: String,
    /// Description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Path to the `PcbLib` that contains this footprint, written as
    /// `ModelDatafile0`. When set, Altium resolves the footprint directly from
    /// that file (rendering the preview); when absent it falls back to searching
    /// available libraries by name, which reports "footprint not found" if the
    /// library isn't installed/in the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_path: Option<String>,
    /// Whether this is the current/default footprint model (`IsCurrent=T`).
    /// Preserved on read; on write the first model is still emitted as current
    /// (positional), so this is read-preserved only until multi-model authoring lands.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_current: bool,
}

impl FootprintModel {
    /// Creates a new footprint model reference.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            library_path: None,
            is_current: false,
        }
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

    #[test]
    fn pin_orientation_flags() {
        assert_eq!(
            PinOrientation::from_flags(false, false),
            PinOrientation::Right
        );
        assert_eq!(
            PinOrientation::from_flags(false, true),
            PinOrientation::Left
        );
        assert_eq!(PinOrientation::from_flags(true, false), PinOrientation::Up);
        assert_eq!(PinOrientation::from_flags(true, true), PinOrientation::Down);

        assert_eq!(PinOrientation::Right.to_flags(), (false, false));
        assert_eq!(PinOrientation::Left.to_flags(), (false, true));
    }

    #[test]
    fn pin_electrical_type_roundtrip() {
        for id in 0..8 {
            let etype = PinElectricalType::from_id(id);
            assert_eq!(etype.to_id(), id);
        }
    }
}
