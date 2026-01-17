//! IPC-7351B package type implementations.
//!
//! Each package family (CHIP, SOIC, QFP, etc.) has its own module
//! implementing the land pattern calculations per the IPC-7351B standard.

pub mod chip;
pub mod sot;

use serde::{Deserialize, Serialize};

use crate::ipc7351::density::DensityLevel;

/// Calculated land pattern result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandPattern {
    /// IPC-7351B compliant name for this footprint.
    pub ipc_name: String,

    /// Pad definitions.
    pub pads: Vec<Pad>,

    /// Courtyard boundary.
    pub courtyard: Courtyard,

    /// Silkscreen outline.
    pub silkscreen: Silkscreen,

    /// Assembly outline (component body).
    pub assembly: Assembly,

    /// Component origin (typically centre of component).
    pub origin: Point,
}

/// A pad in the land pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pad {
    /// Pad designator (1, 2, etc.).
    pub number: u32,

    /// Pad centre X coordinate (mm).
    pub x: f64,

    /// Pad centre Y coordinate (mm).
    pub y: f64,

    /// Pad width in X direction (mm).
    pub width: f64,

    /// Pad height in Y direction (mm).
    pub height: f64,

    /// Corner radius (0.0 for rectangular, >0 for rounded).
    pub corner_radius: f64,

    /// Pad shape.
    pub shape: PadShape,
}

impl Pad {
    /// Creates a new rectangular pad.
    #[must_use]
    pub const fn rectangular(number: u32, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            number,
            x,
            y,
            width,
            height,
            corner_radius: 0.0,
            shape: PadShape::Rectangle,
        }
    }

    /// Creates a new rounded rectangular pad.
    #[must_use]
    pub const fn rounded_rect(number: u32, x: f64, y: f64, width: f64, height: f64, radius: f64) -> Self {
        Self {
            number,
            x,
            y,
            width,
            height,
            corner_radius: radius,
            shape: PadShape::RoundedRectangle,
        }
    }
}

/// Pad shape type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadShape {
    /// Rectangular pad.
    Rectangle,
    /// Rounded rectangle pad.
    RoundedRectangle,
    /// Circular pad (for BGA).
    Circle,
    /// Oblong/oval pad.
    Oblong,
}

/// A 2D point.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Point {
    /// X coordinate (mm).
    pub x: f64,
    /// Y coordinate (mm).
    pub y: f64,
}

impl Point {
    /// Creates a new point.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Courtyard boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Courtyard {
    /// Minimum X coordinate (mm).
    pub min_x: f64,
    /// Minimum Y coordinate (mm).
    pub min_y: f64,
    /// Maximum X coordinate (mm).
    pub max_x: f64,
    /// Maximum Y coordinate (mm).
    pub max_y: f64,
    /// Line width for courtyard outline (mm).
    pub line_width: f64,
}

impl Courtyard {
    /// Creates a courtyard from bounds with default line width.
    #[must_use]
    pub const fn from_bounds(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
            line_width: 0.05, // IPC-7351B default
        }
    }

    /// Returns the courtyard width.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    /// Returns the courtyard height.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
}

/// Silkscreen outline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Silkscreen {
    /// Line segments forming the outline.
    pub lines: Vec<Line>,
    /// Line width (mm).
    pub line_width: f64,
}

impl Silkscreen {
    /// Creates a silkscreen with no lines.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            lines: Vec::new(),
            line_width: 0.15,
        }
    }

    /// Creates a silkscreen from line segments.
    #[must_use]
    pub const fn from_lines(lines: Vec<Line>, line_width: f64) -> Self {
        Self { lines, line_width }
    }
}

/// A line segment.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Line {
    /// Start X (mm).
    pub x1: f64,
    /// Start Y (mm).
    pub y1: f64,
    /// End X (mm).
    pub x2: f64,
    /// End Y (mm).
    pub y2: f64,
}

impl Line {
    /// Creates a new line segment.
    #[must_use]
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }
}

/// Assembly outline (component body).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assembly {
    /// Body outline rectangle.
    pub body: Rect,
    /// Line width (mm).
    pub line_width: f64,
}

impl Assembly {
    /// Creates an assembly outline from body dimensions.
    #[must_use]
    pub fn from_body(width: f64, height: f64) -> Self {
        Self {
            body: Rect::centred(width, height),
            line_width: 0.10,
        }
    }
}

/// A rectangle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    /// Minimum X (mm).
    pub min_x: f64,
    /// Minimum Y (mm).
    pub min_y: f64,
    /// Maximum X (mm).
    pub max_x: f64,
    /// Maximum Y (mm).
    pub max_y: f64,
}

impl Rect {
    /// Creates a rectangle centred at origin.
    #[must_use]
    pub fn centred(width: f64, height: f64) -> Self {
        let half_w = width / 2.0;
        let half_h = height / 2.0;
        Self {
            min_x: -half_w,
            min_y: -half_h,
            max_x: half_w,
            max_y: half_h,
        }
    }
}

/// Input dimensions for land pattern calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDimensions {
    /// Component body length (mm) - dimension in the direction of terminals.
    pub body_length: f64,

    /// Component body width (mm) - dimension perpendicular to terminals.
    pub body_width: f64,

    /// Terminal/lead length (mm) - how far the terminal extends from body.
    pub terminal_length: f64,

    /// Terminal/lead width (mm).
    pub terminal_width: f64,

    /// Component height (mm) - for IPC naming.
    pub height: f64,

    /// Lead pitch (mm) - for multi-row packages.
    pub pitch: Option<f64>,

    /// Number of pins/terminals.
    pub pin_count: u32,
}

impl PackageDimensions {
    /// Creates dimensions for a 2-terminal chip component.
    #[must_use]
    pub const fn chip(body_length: f64, body_width: f64, terminal_length: f64, height: f64) -> Self {
        Self {
            body_length,
            body_width,
            terminal_length,
            terminal_width: body_width, // Full width for chip components
            height,
            pitch: None,
            pin_count: 2,
        }
    }

    /// Calculates the lead span (toe-to-toe distance).
    #[must_use]
    pub const fn lead_span(&self) -> f64 {
        self.body_length
    }
}

/// Trait for package calculators.
pub trait PackageCalculator {
    /// Calculates the land pattern for the given dimensions and density.
    fn calculate(&self, dims: &PackageDimensions, density: DensityLevel) -> LandPattern;
}
