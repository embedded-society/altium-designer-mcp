//! `PcbLib` text and fill primitives.

#[allow(clippy::wildcard_imports)] // sibling primitive types
use super::*;

/// Text rendering kind.
///
/// Altium supports three types of text rendering:
/// - Stroke: Vector-based text using stroke fonts (most common in PCB footprints)
/// - TrueType: Text rendered using TrueType fonts
/// - `BarCode`: Barcode text (1D or 2D codes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextKind {
    /// Stroke (vector) font text - most common for PCB footprints.
    #[default]
    Stroke,
    /// TrueType font text.
    TrueType,
    /// Barcode text (1D or 2D).
    BarCode,
}

/// Stroke font type for vector text.
///
/// When `TextKind` is `Stroke`, this specifies which stroke font to use.
/// Stroke fonts are simple vector fonts built into Altium.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeFont {
    /// Default stroke font.
    #[default]
    Default,
    /// Sans-serif stroke font.
    SansSerif,
    /// Serif stroke font.
    Serif,
}

/// Text justification (alignment). Shared with `SchLib`; the canonical
/// definition is [`crate::altium::TextJustification`]. `PcbLib` text defaults to
/// `MiddleCenter` (this enum's `Default`).
pub use crate::altium::TextJustification;

/// A text string on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    /// X position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,
    /// Y position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,
    /// Text content.
    pub text: String,
    /// Text height in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub height: f64,
    /// Layer the text is on.
    pub layer: Layer,
    /// Rotation angle in degrees.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub rotation: f64,
    /// Text rendering kind (Stroke, TrueType, or `BarCode`).
    #[serde(default)]
    pub kind: TextKind,
    /// Stroke font type (only applies when `kind` is `Stroke`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_font: Option<StrokeFont>,
    /// TrueType italic style (Altium `FontItalic`, geometry offset 45). Only
    /// meaningful when `kind` is `TrueType`. `false` (the from-scratch default)
    /// reproduces the template byte exactly.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    /// Stroke line width in mm (Altium `StrokeWidth`, geometry offset 36). `None`
    /// uses Altium's template default (4 mil); a read value round-trips exactly.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::altium::serde_round::option::serialize"
    )]
    pub stroke_width: Option<f64>,
    /// Text justification (alignment).
    #[serde(default)]
    pub justification: TextJustification,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

/// A filled rectangle on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    /// First corner X position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x1: f64,
    /// First corner Y position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y1: f64,
    /// Second corner X position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x2: f64,
    /// Second corner Y position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y2: f64,
    /// Layer the fill is on.
    pub layer: Layer,
    /// Rotation angle in degrees.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub rotation: f64,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
    /// Solder-mask expansion override in mm (geometry offset 37). `None` uses the
    /// rule default; round-trips like the Track/Arc extended tail.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub solder_mask_expansion: Option<f64>,
    /// Keepout restriction bitmask (geometry offset 46). `None` = zero on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keepout_restrictions: Option<u8>,
    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

impl Fill {
    /// Creates a new Fill from corner coordinates.
    #[must_use]
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64, layer: Layer) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            layer,
            rotation: 0.0,
            flags: PcbFlags::empty(),
            solder_mask_expansion: None,
            keepout_restrictions: None,
            unique_id: None,
        }
    }

    /// Creates a Fill from centre position and dimensions.
    #[must_use]
    pub fn from_center(x: f64, y: f64, width: f64, height: f64, layer: Layer) -> Self {
        let half_w = width / 2.0;
        let half_h = height / 2.0;
        Self {
            x1: x - half_w,
            y1: y - half_h,
            x2: x + half_w,
            y2: y + half_h,
            layer,
            rotation: 0.0,
            flags: PcbFlags::empty(),
            solder_mask_expansion: None,
            keepout_restrictions: None,
            unique_id: None,
        }
    }
}
