//! `PcbLib` pad and via primitives (shapes, stack modes).

#[allow(clippy::wildcard_imports)] // sibling primitive types
use super::*;

/// A PCB pad (SMD or through-hole).
///
/// Pads are the connection points on a footprint where component leads are soldered.
/// There are two main types:
///
/// - **SMD pads**: Surface-mount pads on a single layer (top or bottom)
/// - **Through-hole pads**: Pads with a drilled hole spanning multiple layers
///
/// # Examples
///
/// Create an SMD pad for a 0603 resistor:
///
/// ```
/// use altium_designer_mcp::altium::pcblib::primitives::Pad;
///
/// // 0.8mm × 0.9mm pad at position (-0.8, 0)
/// let pad = Pad::smd("1", -0.8, 0.0, 0.8, 0.9);
/// ```
///
/// Create a through-hole pad for a 2.54mm pin header:
///
/// ```
/// use altium_designer_mcp::altium::pcblib::primitives::Pad;
///
/// // 1.6mm diameter pad with 0.8mm hole
/// let pad = Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pad {
    /// Pad designator (e.g., "1", "2", "A1").
    pub designator: String,

    /// X position in mm (from footprint origin).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,

    /// Y position in mm (from footprint origin).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,

    /// Pad width in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub width: f64,

    /// Pad height in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub height: f64,

    /// Pad shape.
    #[serde(default)]
    pub shape: PadShape,

    /// Layer the pad is on.
    #[serde(default)]
    pub layer: Layer,

    /// Hole diameter for through-hole pads (mm). None for SMD pads.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub hole_size: Option<f64>,

    /// Hole shape for through-hole pads.
    #[serde(default, skip_serializing_if = "is_default_hole_shape")]
    pub hole_shape: HoleShape,

    /// Rotation angle in degrees.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub rotation: f64,

    /// Paste mask expansion in mm. None uses design rules.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub paste_mask_expansion: Option<f64>,

    /// Solder mask expansion in mm. None uses design rules.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub solder_mask_expansion: Option<f64>,

    /// Whether paste mask expansion is manually set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub paste_mask_expansion_manual: bool,

    /// Whether solder mask expansion is manually set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub solder_mask_expansion_manual: bool,

    /// Corner radius as percentage of smaller pad dimension (0-100).
    /// Only applies to `RoundedRectangle` shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius_percent: Option<u8>,

    /// Stack mode for per-layer pad geometry.
    #[serde(default)]
    pub stack_mode: PadStackMode,

    /// Per-layer pad sizes in mm (width, height) for 32 layers.
    /// Only used when `stack_mode` != `Simple`.
    /// Index 0 = Top Layer, Index 1 = Bottom Layer, Index 2-31 = Mid Layers.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::vec_tuple"
    )]
    pub per_layer_sizes: Option<Vec<(f64, f64)>>,

    /// Per-layer pad shapes for 32 layers.
    /// Only used when `stack_mode` != `Simple`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_layer_shapes: Option<Vec<PadShape>>,

    /// Per-layer corner radius percentages (0-100) for 32 layers.
    /// Only used when `stack_mode` != `Simple`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_layer_corner_radii: Option<Vec<u8>>,

    /// Per-layer offset from hole centre in mm (x, y) for 32 layers.
    /// Only used when `stack_mode` != `Simple`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::vec_tuple"
    )]
    pub per_layer_offsets: Option<Vec<(f64, f64)>>,

    /// Primitive flags (locked, keepout, tenting, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,

    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

/// Helper for serde to skip default hole shape in serialisation.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde requires reference
fn is_default_hole_shape(shape: &HoleShape) -> bool {
    *shape == HoleShape::default()
}

impl Pad {
    /// Creates a new SMD pad on the top layer.
    #[must_use]
    pub fn smd(designator: impl Into<String>, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            designator: designator.into(),
            x,
            y,
            width,
            height,
            shape: PadShape::RoundedRectangle,
            layer: Layer::TopLayer,
            hole_size: None,
            hole_shape: HoleShape::Round,
            rotation: 0.0,
            paste_mask_expansion: None,
            solder_mask_expansion: None,
            paste_mask_expansion_manual: false,
            solder_mask_expansion_manual: false,
            corner_radius_percent: None,
            stack_mode: PadStackMode::Simple,
            per_layer_sizes: None,
            per_layer_shapes: None,
            per_layer_corner_radii: None,
            per_layer_offsets: None,
            flags: PcbFlags::empty(),
            unique_id: None,
        }
    }

    /// Creates a new through-hole pad.
    #[must_use]
    pub fn through_hole(
        designator: impl Into<String>,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        hole_size: f64,
    ) -> Self {
        Self {
            designator: designator.into(),
            x,
            y,
            width,
            height,
            shape: PadShape::Round,
            layer: Layer::MultiLayer,
            hole_size: Some(hole_size),
            hole_shape: HoleShape::Round,
            rotation: 0.0,
            paste_mask_expansion: None,
            solder_mask_expansion: None,
            paste_mask_expansion_manual: false,
            solder_mask_expansion_manual: false,
            corner_radius_percent: None,
            stack_mode: PadStackMode::Simple,
            per_layer_sizes: None,
            per_layer_shapes: None,
            per_layer_corner_radii: None,
            per_layer_offsets: None,
            flags: PcbFlags::empty(),
            unique_id: None,
        }
    }
}

/// Pad shape types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadShape {
    /// Rectangular pad.
    Rectangle,
    /// Rectangular pad with rounded corners (most common for SMD).
    #[default]
    RoundedRectangle,
    /// Circular pad.
    Round,
    /// Oval/oblong pad.
    Oval,
}

/// Hole shape types for through-hole pads.
///
/// This is separate from `PadShape` as it describes the drill hole shape,
/// not the copper pad shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoleShape {
    /// Circular hole (most common).
    #[default]
    Round,
    /// Square hole.
    Square,
    /// Slot (oblong) hole.
    Slot,
}

/// Pad stack mode for per-layer pad geometry.
///
/// Controls whether pad size/shape varies per layer or uses uniform values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadStackMode {
    /// All layers use the same size and shape (most common).
    #[default]
    Simple,
    /// Top, middle, and bottom layers can have different sizes/shapes.
    TopMiddleBottom,
    /// Each of the 32 layers can have independent size/shape/corner radius.
    FullStack,
}

/// Via diameter stack mode.
///
/// Controls whether via diameters vary per layer or use a single uniform diameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViaStackMode {
    /// All layers use the same diameter (most common).
    #[default]
    Simple,
    /// Top, middle, and bottom layers can have different diameters.
    TopMiddleBottom,
    /// Each of the 32 layers can have an independent diameter.
    FullStack,
}

/// A PCB via (vertical interconnect access).
///
/// Vias connect traces between different copper layers. They have a drill hole
/// and copper annular rings on the connected layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Via {
    /// X position in mm (from footprint origin).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,

    /// Y position in mm (from footprint origin).
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,

    /// Via diameter (annular ring outer diameter) in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub diameter: f64,

    /// Hole diameter in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub hole_size: f64,

    /// Starting layer for the via.
    #[serde(default)]
    pub from_layer: Layer,

    /// Ending layer for the via.
    #[serde(default)]
    pub to_layer: Layer,

    /// Solder mask expansion in mm (negative = tented).
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub solder_mask_expansion: f64,

    /// Whether solder mask expansion is manually set.
    #[serde(default)]
    pub solder_mask_expansion_manual: bool,

    // Thermal relief settings (for polygon pours)
    /// Thermal relief air gap width in mm (default: 0.254mm = 10 mils).
    #[serde(
        default = "default_thermal_relief_gap",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub thermal_relief_gap: f64,

    /// Number of thermal relief conductors (default: 4).
    #[serde(default = "default_thermal_relief_conductors")]
    pub thermal_relief_conductors: u8,

    /// Thermal relief conductor width in mm (default: 0.254mm = 10 mils).
    #[serde(
        default = "default_thermal_relief_width",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub thermal_relief_width: f64,

    // Diameter stack mode
    /// Diameter stack mode (`Simple`, `TopMiddleBottom`, or `FullStack`).
    #[serde(default)]
    pub diameter_stack_mode: ViaStackMode,

    /// Per-layer diameters in mm (32 layers). Only used when `stack_mode` != `Simple`.
    /// Index 0 = Top Layer, Index 1 = Bottom Layer, Index 2-31 = Mid Layers.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::vec_f64"
    )]
    pub per_layer_diameters: Option<Vec<f64>>,

    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

/// Default thermal relief gap (10 mils = 0.254mm).
const fn default_thermal_relief_gap() -> f64 {
    0.254
}

/// Default thermal relief conductor count.
const fn default_thermal_relief_conductors() -> u8 {
    4
}

/// Default thermal relief conductor width (10 mils = 0.254mm).
const fn default_thermal_relief_width() -> f64 {
    0.254
}

impl Via {
    /// Creates a new via with default settings.
    ///
    /// By default, vias span from top to bottom layer with standard thermal relief
    /// (10 mil gap, 4 conductors, 10 mil width) and simple diameter stack mode.
    #[must_use]
    pub const fn new(x: f64, y: f64, diameter: f64, hole_size: f64) -> Self {
        Self {
            x,
            y,
            diameter,
            hole_size,
            from_layer: Layer::TopLayer,
            to_layer: Layer::BottomLayer,
            solder_mask_expansion: 0.0,
            solder_mask_expansion_manual: false,
            thermal_relief_gap: 0.254, // 10 mils
            thermal_relief_conductors: 4,
            thermal_relief_width: 0.254, // 10 mils
            diameter_stack_mode: ViaStackMode::Simple,
            per_layer_diameters: None,
            unique_id: None,
        }
    }

    /// Creates a blind via (connects outer layer to inner layer).
    #[must_use]
    pub const fn blind(
        x: f64,
        y: f64,
        diameter: f64,
        hole_size: f64,
        from: Layer,
        to: Layer,
    ) -> Self {
        Self {
            x,
            y,
            diameter,
            hole_size,
            from_layer: from,
            to_layer: to,
            solder_mask_expansion: 0.0,
            solder_mask_expansion_manual: false,
            thermal_relief_gap: 0.254, // 10 mils
            thermal_relief_conductors: 4,
            thermal_relief_width: 0.254, // 10 mils
            diameter_stack_mode: ViaStackMode::Simple,
            per_layer_diameters: None,
            unique_id: None,
        }
    }
}
