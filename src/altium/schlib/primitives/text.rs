//! `SchLib` text primitives (labels, text annotations, parameters).

#[allow(clippy::wildcard_imports)] // sibling primitive types
use super::*;

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
    /// Universal display/lock flags; omitted from JSON when all default.
    #[serde(default, flatten)]
    pub display_flags: ShapeDisplayFlags,
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
/// Coordinates are `f64` schematic units (see `super::coord`); `Eq` is not
/// derived (floats are only `PartialEq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // independent Altium display/visibility toggles
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
    /// Text orientation (`ORIENTATION`): `0`/`1`/`2`/`3` = 0°/90°/180°/270°.
    /// Omit-when-default: the reader defaults an absent key to `0` and the writer
    /// emits `Orientation` only when non-zero. Default `0`.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub orientation: i32,
    /// Whether to show the parameter name alongside its value (`SHOWNAME`).
    /// Omit-when-default: Altium omits the key for a from-scratch parameter (the
    /// golden's visible + hidden parameters carry neither `SHOWNAME` nor
    /// `HIDENAME`), so the reader defaults an absent key to `false` and the writer
    /// emits `ShowName=T` only when set. Default `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub show_name: bool,
    /// Whether to hide the parameter name, showing only the value (`HIDENAME`).
    /// Omit-when-default (see `show_name`). Default `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_name: bool,
    /// Parameter description text (`DESCRIPTION`). Omit-when-default: emitted only
    /// when non-empty. Default empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Whether the parameter is variant-configurable (`ISCONFIGURABLE`).
    /// Omit-when-default: emitted only when `true`. Default `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_configurable: bool,
    /// Owner part ID.
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,
    /// Universal display/lock flags; omitted from JSON when all default.
    #[serde(default, flatten)]
    pub display_flags: ShapeDisplayFlags,
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
            orientation: 0,
            show_name: false,
            hide_name: false,
            description: String::new(),
            is_configurable: false,
            owner_part_id: 1,
            display_flags: ShapeDisplayFlags::default(),
            unique_id: None,
        }
    }
}
