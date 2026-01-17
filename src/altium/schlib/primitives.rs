//! Schematic symbol primitive types for `SchLib` files.
//!
//! These types represent the graphic elements that make up a schematic symbol:
//! pins, rectangles, lines, arcs, labels, and parameters.

use serde::{Deserialize, Serialize};

/// A schematic symbol pin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Bidirectional pin.
    InputOutput,
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
            1 => Self::InputOutput,
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
            Self::InputOutput => 1,
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
    /// Line color (BGR format).
    #[serde(default)]
    pub line_color: u32,
    /// Fill color (BGR format).
    #[serde(default)]
    pub fill_color: u32,
    /// Whether the rectangle is filled.
    #[serde(default = "default_true")]
    pub filled: bool,
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
    /// Line color (BGR format).
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
    /// Line color (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
}

/// An arc or circle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    /// Center X coordinate.
    pub x: i32,
    /// Center Y coordinate.
    pub y: i32,
    /// Radius.
    pub radius: i32,
    /// Start angle in degrees (0 = right, counter-clockwise).
    #[serde(default)]
    pub start_angle: f64,
    /// End angle in degrees.
    #[serde(default = "default_end_angle")]
    pub end_angle: f64,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line color (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
}

const fn default_end_angle() -> f64 {
    360.0
}

/// An ellipse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ellipse {
    /// Center X coordinate.
    pub x: i32,
    /// Center Y coordinate.
    pub y: i32,
    /// X radius (horizontal).
    pub radius_x: i32,
    /// Y radius (vertical).
    pub radius_y: i32,
    /// Line width.
    #[serde(default = "default_line_width")]
    pub line_width: u8,
    /// Line color (BGR format).
    #[serde(default)]
    pub line_color: u32,
    /// Fill color (BGR format).
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
    /// Text color (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Text justification.
    #[serde(default)]
    pub justification: TextJustification,
    /// Rotation in degrees.
    #[serde(default)]
    pub rotation: f64,
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
    /// Bottom-center aligned.
    BottomCenter,
    /// Bottom-right aligned.
    BottomRight,
    /// Middle-left aligned.
    MiddleLeft,
    /// Middle-center aligned.
    MiddleCenter,
    /// Middle-right aligned.
    MiddleRight,
    /// Top-left aligned.
    TopLeft,
    /// Top-center aligned.
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
    pub value: String,
    /// X position.
    pub x: i32,
    /// Y position.
    pub y: i32,
    /// Font ID.
    #[serde(default = "default_font_id")]
    pub font_id: u8,
    /// Text color (BGR format).
    #[serde(default)]
    pub color: u32,
    /// Whether the parameter is hidden.
    #[serde(default)]
    pub hidden: bool,
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
