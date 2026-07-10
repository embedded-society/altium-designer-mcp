//! Schematic symbol primitive types for `SchLib` files.
//!
//! These types represent the graphic elements that make up a schematic symbol:
//! pins, rectangles, lines, arcs, labels, and parameters.

use serde::{Deserialize, Serialize};

mod models;
pub use models::FootprintModel;
mod pins;
pub use pins::{Pin, PinElectricalType, PinFrac, PinOrientation, PinSymbol};
mod shapes;
pub use shapes::{
    Arc, Bezier, Ellipse, EllipticalArc, Image, Line, Pie, Polygon, Polyline, Rectangle, RoundRect,
};
mod text;
pub use text::{Label, Parameter, Text, TextFrame, TextJustification};

// Float rounding on serialization is shared (crate::altium::serde_round).

/// Universal display/lock flags shared by every `SchLib` graphic shape.
///
/// Altium's `SchGraphicalObject` base carries these on every primitive; they map
/// to the `GRAPHICALLYLOCKED` / `DISABLED` / `DIMMED` / `OWNERPARTDISPLAYMODE`
/// record keys. All four are **omit-when-default**: the reader defaults an absent
/// key to `false`/`0` and the writer emits a key only when it is non-default, so
/// a shape carrying only defaults stays byte-identical to Altium's output.
///
/// Embedded into each shape struct with `#[serde(flatten)]`, so the four fields
/// appear inline in the read/write JSON (no nested object).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // three independent Altium display flags
pub struct ShapeDisplayFlags {
    /// Whether the shape is graphically locked (`GRAPHICALLYLOCKED=T`). Default `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub graphically_locked: bool,
    /// Whether the shape is disabled (`DISABLED=T`). Default `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    /// Whether the shape is dimmed in display (`DIMMED=T`). Default `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dimmed: bool,
    /// Display mode this shape belongs to (`OWNERPARTDISPLAYMODE`).
    /// `0` = Normal, `1` = the first alternate (de-Morgan) mode, etc. Default `0`.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub owner_part_display_mode: i32,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde skip_serializing_if requires &T
const fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

const fn default_true() -> bool {
    true
}

const fn default_owner_part() -> i32 {
    1
}
