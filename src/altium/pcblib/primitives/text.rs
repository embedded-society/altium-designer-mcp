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
/// definition is [`crate::altium::TextJustification`]. A `PcbLib` text's
/// text-box anchor defaults to `BottomLeft`, which encodes to the geometry
/// template's justification byte (`0x03` = Altium `LeftBottom`) at offset 132 —
/// so a from-scratch text stays byte-identical. This matches `AltiumSharp`'s
/// `PcbText.Justification` default.
pub use crate::altium::TextJustification;

/// Default text-box justification for a from-scratch `PcbLib` text: `BottomLeft`,
/// which the writer encodes to the template's `0x03` byte at geometry offset 132.
const fn default_justification() -> TextJustification {
    TextJustification::BottomLeft
}

/// Default font name for a from-scratch text: `"Arial"`, matching the geometry
/// template's UTF-16 font-name field (offsets 46-109).
fn default_font_name() -> String {
    "Arial".to_string()
}

/// Default net index for a from-scratch text/fill (`0xFFFF` = no net). The
/// common-header connectivity indices default to "none" so a free library
/// primitive writes the same `0xFF` header bytes as before (byte-identity).
const fn default_net_index() -> u16 {
    0xFFFF
}

/// Default polygon index for a from-scratch text/fill (`0xFFFF` = none).
const fn default_polygon_index() -> u16 {
    0xFFFF
}

/// Default component index for a from-scratch text/fill (`-1` = free primitive,
/// stored as the `0xFFFF` common-header sentinel).
const fn default_component_index() -> i32 {
    -1
}

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
    /// Font bold style (Altium `FontBold`, geometry offset 44 — the twin of
    /// [`Self::italic`]@45). Only meaningful when `kind` is `TrueType`. `false`
    /// (the from-scratch default) reproduces the template's `0x00` byte exactly.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    /// Whether the text is mirrored (Altium `IsMirrored`, geometry offset 35;
    /// bottom-side silkscreen). `false` (the from-scratch default) reproduces the
    /// template's `0x00` byte exactly.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror: bool,
    /// TrueType font name (Altium `FontName`, geometry offset 46, UTF-16, 64-byte
    /// field). Only meaningful when `kind` is `TrueType`. Defaults to `"Arial"`,
    /// matching the template; a from-scratch default text reproduces the template's
    /// exact 64-byte UTF-16 encoding.
    #[serde(default = "default_font_name")]
    pub font_name: String,
    /// Stroke line width in mm (Altium `StrokeWidth`, geometry offset 36). `None`
    /// uses Altium's template default (4 mil); a read value round-trips exactly.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::altium::serde_round::option::serialize"
    )]
    pub stroke_width: Option<f64>,
    /// Text-box justification / anchor (Altium `InvertedRectJustification`,
    /// geometry offset 132). Defaults to `BottomLeft`, which reproduces the
    /// template byte `0x03`.
    #[serde(default = "default_justification")]
    pub justification: TextJustification,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
    /// Net index into the board's net list — common-header u16 @3. `0xFFFF`
    /// (65535) means "no net", the from-scratch default (round-trip fidelity).
    #[serde(default = "default_net_index")]
    pub net_index: u16,
    /// Polygon index this text belongs to — common-header u16 @5. `0xFFFF`
    /// (none) from scratch, matching the historical writer output.
    #[serde(default = "default_polygon_index")]
    pub polygon_index: u16,
    /// Component index into the board's component list — common-header u16 @7
    /// (`0xFFFF` stored, exposed as `-1`). `-1` (free primitive) from scratch.
    #[serde(default = "default_component_index")]
    pub component_index: i32,
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
    /// Net index into the board's net list — common-header u16 @3. `0xFFFF`
    /// (65535) means "no net", the from-scratch default (round-trip fidelity).
    #[serde(default = "default_net_index")]
    pub net_index: u16,
    /// Polygon index this fill belongs to — common-header u16 @5. `0xFFFF`
    /// (none) from scratch, matching the historical writer output.
    #[serde(default = "default_polygon_index")]
    pub polygon_index: u16,
    /// Component index into the board's component list — common-header u16 @7
    /// (`0xFFFF` stored, exposed as `-1`). `-1` (free primitive) from scratch.
    #[serde(default = "default_component_index")]
    pub component_index: i32,
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
            net_index: default_net_index(),
            polygon_index: default_polygon_index(),
            component_index: default_component_index(),
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
            net_index: default_net_index(),
            polygon_index: default_polygon_index(),
            component_index: default_component_index(),
            solder_mask_expansion: None,
            keepout_restrictions: None,
            unique_id: None,
        }
    }
}
