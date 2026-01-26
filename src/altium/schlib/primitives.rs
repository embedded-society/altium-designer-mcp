//! Schematic symbol primitive types for `SchLib` files.
//!
//! These types represent the graphic elements that make up a schematic symbol:
//! pins, rectangles, lines, arcs, labels, and parameters.

use serde::{Deserialize, Serialize};

/// Custom serialisation for floating-point values to avoid precision artifacts.
///
/// Rounds values to 6 decimal places to prevent output like `359.999999` instead of `360.0`.
mod float_serde {
    use serde::Serializer;

    /// Rounds a floating-point value to 6 decimal places.
    #[inline]
    fn round_float(value: f64) -> f64 {
        (value * 1_000_000.0).round() / 1_000_000.0
    }

    /// Serialises an f64 with rounding.
    #[allow(clippy::trivially_copy_pass_by_ref)] // serde requires &T signature
    pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(round_float(*value))
    }
}

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
}

const fn default_true() -> bool {
    true
}

const fn default_owner_part() -> i32 {
    1
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
            symbol_inner_edge: PinSymbol::None,
            symbol_outer_edge: PinSymbol::None,
            symbol_inside: PinSymbol::None,
            symbol_outside: PinSymbol::None,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rectangle {
    /// Left X coordinate.
    pub x1: i32,
    /// Bottom Y coordinate.
    pub y1: i32,
    /// Right X coordinate.
    pub x2: i32,
    /// Top Y coordinate.
    pub y2: i32,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub line_color: u32,
    /// Fill colour (BGR format).
    #[serde(default)]
    pub fill_color: u32,
    /// Whether the rectangle is filled.
    #[serde(default = "default_true")]
    pub filled: bool,
    /// Whether the rectangle is transparent.
    #[serde(default)]
    pub transparent: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
}

const fn default_line_width() -> u8 {
    1
}

impl Rectangle {
    /// Creates a new rectangle.
    #[must_use]
    pub const fn new(x1: i32, y1: i32, x2: i32, y2: i32) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            line_width: 1,
            line_color: 0x00_00_80, // Dark red (BGR)
            fill_color: 0xFF_FF_B0, // Light yellow (BGR)
            filled: true,
            transparent: false,
            owner_part_id: 1,
        }
    }
}

/// A line segment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    /// Start X coordinate.
    pub x1: i32,
    /// Start Y coordinate.
    pub y1: i32,
    /// End X coordinate.
    pub x2: i32,
    /// End Y coordinate.
    pub y2: i32,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
}

impl Line {
    /// Creates a new line.
    #[must_use]
    pub const fn new(x1: i32, y1: i32, x2: i32, y2: i32) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            line_width: 1,
            color: 0x00_00_80, // Dark red (BGR)
            owner_part_id: 1,
        }
    }
}

/// A polyline (multiple connected line segments).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Polyline {
    /// Points as (x, y) pairs.
    pub points: Vec<(i32, i32)>,
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
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
}

/// A filled polygon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Polygon {
    /// Vertices as (x, y) pairs.
    pub points: Vec<(i32, i32)>,
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
}

/// An arc or circle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    /// Centre X coordinate.
    pub x: i32,
    /// Centre Y coordinate.
    pub y: i32,
    /// Radius.
    pub radius: i32,
    /// Start angle in degrees (0 = right, counter-clockwise).
    #[serde(default, serialize_with = "float_serde::serialize")]
    pub start_angle: f64,
    /// End angle in degrees.
    #[serde(
        default = "default_end_angle",
        serialize_with = "float_serde::serialize"
    )]
    pub end_angle: f64,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bezier {
    /// Start point X.
    pub x1: i32,
    /// Start point Y.
    pub y1: i32,
    /// Control point 1 X.
    pub x2: i32,
    /// Control point 1 Y.
    pub y2: i32,
    /// Control point 2 X.
    pub x3: i32,
    /// Control point 2 Y.
    pub y3: i32,
    /// End point X.
    pub x4: i32,
    /// End point Y.
    pub y4: i32,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
}

impl Bezier {
    /// Creates a new Bezier curve.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        x3: i32,
        y3: i32,
        x4: i32,
        y4: i32,
    ) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            x3,
            y3,
            x4,
            y4,
            line_width: 1,
            color: 0x00_00_80, // Dark red (BGR)
            owner_part_id: 1,
        }
    }
}

/// An ellipse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ellipse {
    /// Centre X coordinate.
    pub x: i32,
    /// Centre Y coordinate.
    pub y: i32,
    /// X radius (horizontal).
    pub radius_x: i32,
    /// Y radius (vertical).
    pub radius_y: i32,
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
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
}

impl Ellipse {
    /// Creates a new ellipse.
    #[must_use]
    pub const fn new(x: i32, y: i32, radius_x: i32, radius_y: i32) -> Self {
        Self {
            x,
            y,
            radius_x,
            radius_y,
            line_width: 1,
            line_color: 0x00_00_80, // Dark red (BGR)
            fill_color: 0xFF_FF_B0, // Light yellow (BGR)
            filled: true,
            owner_part_id: 1,
        }
    }

    /// Creates a new circle (equal radii).
    #[must_use]
    pub const fn circle(x: i32, y: i32, radius: i32) -> Self {
        Self::new(x, y, radius, radius)
    }
}

/// A rounded rectangle.
///
/// Defined by two corner points and corner radii for rounding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundRect {
    /// Left/bottom X coordinate.
    pub x1: i32,
    /// Left/bottom Y coordinate.
    pub y1: i32,
    /// Right/top X coordinate.
    pub x2: i32,
    /// Right/top Y coordinate.
    pub y2: i32,
    /// Corner X radius (horizontal rounding).
    pub corner_x_radius: i32,
    /// Corner Y radius (vertical rounding).
    pub corner_y_radius: i32,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub line_color: u32,
    /// Fill colour (BGR format).
    #[serde(default)]
    pub fill_color: u32,
    /// Whether the rectangle is filled.
    #[serde(default = "default_true")]
    pub filled: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
}

impl RoundRect {
    /// Creates a new rounded rectangle.
    #[must_use]
    #[allow(clippy::similar_names)]
    pub const fn new(
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        corner_x_radius: i32,
        corner_y_radius: i32,
    ) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            corner_x_radius,
            corner_y_radius,
            line_width: 1,
            line_color: 0x00_00_80, // Dark red (BGR)
            fill_color: 0xFF_FF_B0, // Light yellow (BGR)
            filled: true,
            owner_part_id: 1,
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
    pub x: i32,
    /// Centre Y coordinate.
    pub y: i32,
    /// Primary radius (horizontal).
    #[serde(serialize_with = "float_serde::serialize")]
    pub radius: f64,
    /// Secondary radius (vertical).
    #[serde(serialize_with = "float_serde::serialize")]
    pub secondary_radius: f64,
    /// Start angle in degrees (0 = right, counter-clockwise).
    #[serde(default, serialize_with = "float_serde::serialize")]
    pub start_angle: f64,
    /// End angle in degrees.
    #[serde(
        default = "default_end_angle",
        serialize_with = "float_serde::serialize"
    )]
    pub end_angle: f64,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
}

impl EllipticalArc {
    /// Creates a new elliptical arc.
    #[must_use]
    pub const fn new(
        x: i32,
        y: i32,
        radius: f64,
        secondary_radius: f64,
        start_angle: f64,
        end_angle: f64,
    ) -> Self {
        Self {
            x,
            y,
            radius,
            secondary_radius,
            start_angle,
            end_angle,
            line_width: 1,
            color: 0x00_00_80, // Dark red (BGR)
            owner_part_id: 1,
        }
    }

    /// Creates a full ellipse (0 to 360 degrees).
    #[must_use]
    pub const fn full_ellipse(x: i32, y: i32, radius: f64, secondary_radius: f64) -> Self {
        Self::new(x, y, radius, secondary_radius, 0.0, 360.0)
    }
}

/// A text label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    /// X position.
    pub x: i32,
    /// Y position.
    pub y: i32,
    /// Text content.
    pub text: String,
    /// Font ID (1-based index into library fonts).
    #[serde(default = "default_font_id")]
    pub font_id: u8,
    /// Text colour (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Text justification.
    #[serde(default)]
    pub justification: TextJustification,
    /// Rotation in degrees.
    #[serde(default, serialize_with = "float_serde::serialize")]
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
}

const fn default_font_id() -> u8 {
    1
}

/// Text justification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextJustification {
    /// Bottom-left aligned.
    #[default]
    BottomLeft,
    /// Bottom-centre aligned.
    BottomCenter,
    /// Bottom-right aligned.
    BottomRight,
    /// Middle-left aligned.
    MiddleLeft,
    /// Middle-centre aligned.
    MiddleCenter,
    /// Middle-right aligned.
    MiddleRight,
    /// Top-left aligned.
    TopLeft,
    /// Top-centre aligned.
    TopCenter,
    /// Top-right aligned.
    TopRight,
}

/// A component parameter (e.g., Value, Part Number).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    /// Parameter name (e.g., "Value", "Part Number").
    pub name: String,
    /// Parameter value (e.g., "10k", "*").
    #[serde(default)]
    pub value: String,
    /// X position.
    #[serde(default)]
    pub x: i32,
    /// Y position.
    #[serde(default)]
    pub y: i32,
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
}

impl Parameter {
    /// Creates a new parameter.
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            x: 0,
            y: 0,
            font_id: 1,
            color: 0x80_00_00, // Dark blue (BGR)
            hidden: false,
            read_only_state: 0,
            param_type: 0,
            owner_part_id: 1,
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
}

impl FootprintModel {
    /// Creates a new footprint model reference.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
