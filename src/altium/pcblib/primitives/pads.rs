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

    /// Slot length in mm for a `Slot` hole — size/shape block i32 @263.
    /// Only meaningful when `hole_shape` is `Slot`. Default 0.0 (matches the
    /// value the writer previously hard-coded).
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub hole_slot_length: f64,

    /// Hole rotation in degrees — size/shape block f64 @267. Rotates a slot hole.
    /// Default 0.0 (matches the value the writer previously hard-coded).
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub hole_rotation: f64,

    /// Positive drill tolerance in mm — extended-tail i32 @162. `None` writes the
    /// `0x7FFFFFFF` "unset" sentinel Altium uses (byte-identical to the template);
    /// `Some(mm)` writes the raw tolerance.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub hole_positive_tolerance: Option<f64>,

    /// Negative drill tolerance in mm — extended-tail i32 @166. `None` writes the
    /// `0x7FFFFFFF` "unset" sentinel (byte-identical to the template).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub hole_negative_tolerance: Option<f64>,

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

    /// Paste-mask expansion mode (None/FromRule/Manual) — main-block tri-state byte @101.
    #[serde(default)]
    pub paste_mask_expansion_mode: MaskExpansionMode,

    /// Solder-mask expansion mode (main-block tri-state byte @102).
    #[serde(default)]
    pub solder_mask_expansion_mode: MaskExpansionMode,

    /// Power-plane connection style — extended-tail byte @67
    /// (`Relief` / `Direct` / `NoConnect`). Altium's default is `Relief`.
    #[serde(default)]
    pub power_plane_connect_style: PowerPlaneConnectStyle,

    /// Thermal-relief spoke (conductor) width in mm — extended-tail i32 @68.
    /// Default: 0.254mm (10 mil), matching Altium's pad template.
    #[serde(
        default = "default_pad_relief_conductor_width",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub relief_conductor_width: f64,

    /// Number of thermal-relief spokes (entries) — extended-tail i16 @72.
    /// Default: 4.
    #[serde(default = "default_pad_relief_entries")]
    pub relief_entries: i16,

    /// Thermal-relief air-gap width in mm — extended-tail i32 @74.
    /// Default: 0.254mm (10 mil), matching Altium's pad template.
    #[serde(
        default = "default_pad_relief_air_gap",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub relief_air_gap: f64,

    /// Power-plane relief expansion in mm — extended-tail i32 @78.
    /// Default: 0.508mm (20 mil), matching Altium's pad template.
    #[serde(
        default = "default_pad_power_plane_relief_expansion",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub power_plane_relief_expansion: f64,

    /// Power-plane (anti-pad) clearance to the plane in mm — extended-tail i32 @82.
    /// Default: 0.508mm (20 mil), matching Altium's pad template.
    #[serde(
        default = "default_pad_power_plane_clearance",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub power_plane_clearance: f64,

    /// Corner radius as percentage of smaller pad dimension (0-100).
    /// Only applies to `RoundedRectangle` shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius_percent: Option<u8>,

    /// Stack mode for per-layer pad geometry.
    #[serde(default)]
    pub stack_mode: PadStackMode,

    /// Per-layer pad sizes in mm (width, height).
    /// Only used when `stack_mode` != `Simple`.
    ///
    /// - For `FullStack`: 32 entries, Index 0 = Top Layer, Index 1 = Bottom
    ///   Layer, Index 2-31 = Mid Layers.
    /// - For `TopMiddleBottom`: 3 entries, `[top, mid, bottom]`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::vec_tuple"
    )]
    pub per_layer_sizes: Option<Vec<(f64, f64)>>,

    /// Per-layer pad shapes.
    /// Only used when `stack_mode` != `Simple`.
    ///
    /// - For `FullStack`: 32 entries (one per layer).
    /// - For `TopMiddleBottom`: 3 entries, `[top, mid, bottom]`.
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

/// Default pad thermal-relief conductor width (10 mil = 0.254mm; raw 100000).
const fn default_pad_relief_conductor_width() -> f64 {
    0.254
}

/// Default pad thermal-relief spoke count (matches Altium's pad template).
const fn default_pad_relief_entries() -> i16 {
    4
}

/// Default pad thermal-relief air gap (10 mil = 0.254mm; raw 100000).
const fn default_pad_relief_air_gap() -> f64 {
    0.254
}

/// Default pad power-plane relief expansion (20 mil = 0.508mm; raw 200000).
const fn default_pad_power_plane_relief_expansion() -> f64 {
    0.508
}

/// Default pad power-plane (anti-pad) clearance (20 mil = 0.508mm; raw 200000).
const fn default_pad_power_plane_clearance() -> f64 {
    0.508
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
            hole_slot_length: 0.0,
            hole_rotation: 0.0,
            hole_positive_tolerance: None,
            hole_negative_tolerance: None,
            rotation: 0.0,
            paste_mask_expansion: None,
            solder_mask_expansion: None,
            paste_mask_expansion_mode: MaskExpansionMode::FromRule,
            solder_mask_expansion_mode: MaskExpansionMode::FromRule,
            power_plane_connect_style: PowerPlaneConnectStyle::Relief,
            relief_conductor_width: default_pad_relief_conductor_width(),
            relief_entries: default_pad_relief_entries(),
            relief_air_gap: default_pad_relief_air_gap(),
            power_plane_relief_expansion: default_pad_power_plane_relief_expansion(),
            power_plane_clearance: default_pad_power_plane_clearance(),
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
            hole_slot_length: 0.0,
            hole_rotation: 0.0,
            hole_positive_tolerance: None,
            hole_negative_tolerance: None,
            rotation: 0.0,
            paste_mask_expansion: None,
            solder_mask_expansion: None,
            paste_mask_expansion_mode: MaskExpansionMode::FromRule,
            solder_mask_expansion_mode: MaskExpansionMode::FromRule,
            power_plane_connect_style: PowerPlaneConnectStyle::Relief,
            relief_conductor_width: default_pad_relief_conductor_width(),
            relief_entries: default_pad_relief_entries(),
            relief_air_gap: default_pad_relief_air_gap(),
            power_plane_relief_expansion: default_pad_power_plane_relief_expansion(),
            power_plane_clearance: default_pad_power_plane_clearance(),
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
    /// Oval/oblong pad. Altium has no dedicated oval shape; it draws a Round pad
    /// with unequal X/Y sizes as an oblong, so this serialises as Round (id 1).
    Oval,
    /// Octagonal pad (Altium shape id 3).
    Octagonal,
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

/// Solder / paste mask expansion mode.
///
/// Altium stores this as a tri-state byte, not a boolean: the expansion can be
/// off, taken from the design rule, or a manually-specified value. (Shared by
/// vias and, later, pads.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskExpansionMode {
    /// No mask expansion.
    None,
    /// Expansion taken from the design rule (Altium's default).
    #[default]
    FromRule,
    /// A manually-specified expansion value is used.
    Manual,
}

impl MaskExpansionMode {
    /// Creates from the Altium tri-state byte (`0` = `None`, `1` = `FromRule`, `2` = `Manual`).
    #[must_use]
    pub const fn from_id(id: u8) -> Self {
        match id {
            0 => Self::None,
            2 => Self::Manual,
            _ => Self::FromRule,
        }
    }

    /// Returns the Altium tri-state byte.
    #[must_use]
    pub const fn to_id(self) -> u8 {
        match self {
            Self::None => 0,
            Self::FromRule => 1,
            Self::Manual => 2,
        }
    }
}

/// Power-plane connection style for a pad (Altium `TPlaneConnectStyle`).
///
/// Controls how a pad connects to an internal power plane: with a thermal-relief
/// spoke pattern, a solid (direct) copper connection, or no connection at all.
/// Stored as a single byte at extended-tail offset 67.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerPlaneConnectStyle {
    /// Thermal-relief connection (spokes). Altium's default.
    #[default]
    Relief,
    /// Solid/direct copper connection to the plane.
    Direct,
    /// No connection to the plane.
    NoConnect,
}

impl PowerPlaneConnectStyle {
    /// Creates from the Altium byte (`0` = `Relief`, `1` = `Direct`, `2` = `NoConnect`).
    #[must_use]
    pub const fn from_id(id: u8) -> Self {
        match id {
            1 => Self::Direct,
            2 => Self::NoConnect,
            _ => Self::Relief,
        }
    }

    /// Returns the Altium connection-style byte.
    #[must_use]
    pub const fn to_id(self) -> u8 {
        match self {
            Self::Relief => 0,
            Self::Direct => 1,
            Self::NoConnect => 2,
        }
    }
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

    /// Solder mask expansion mode (`None` / `FromRule` / `Manual`). Altium stores this
    /// as a tri-state byte; `FromRule` is the default for a fresh via.
    #[serde(default)]
    pub solder_mask_expansion_mode: MaskExpansionMode,

    /// Paste-mask expansion in mm — `SubRecord-1` i32 @50. Default: 0.0, matching
    /// Altium's via template (a via has no paste by default).
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub paste_mask_expansion: f64,

    /// Power-plane connection style — `SubRecord-1` byte @31
    /// (`Relief` / `Direct` / `NoConnect`). Altium's default is `Relief`.
    #[serde(default)]
    pub power_plane_connect_style: PowerPlaneConnectStyle,

    /// Power-plane relief expansion in mm — `SubRecord-1` i32 @42.
    /// Default: 0.508mm (20 mil), matching Altium's via template.
    #[serde(
        default = "default_via_power_plane_relief_expansion",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub power_plane_relief_expansion: f64,

    /// Power-plane (anti-pad) clearance to the plane in mm — `SubRecord-1` i32 @46.
    /// Default: 0.508mm (20 mil), matching Altium's via template.
    #[serde(
        default = "default_via_power_plane_clearance",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub power_plane_clearance: f64,

    /// Net index into the board's net list — `SubRecord-1` u16 @3.
    /// `0xFFFF` (65535) means "no net", the default for a footprint via.
    #[serde(default = "default_via_net_index")]
    pub net_index: u16,

    /// Bottom-face solder-mask expansion in mm (Altium geometry offset 242). `None`
    /// mirrors the front-face `solder_mask_expansion` (Altium's template encodes both
    /// faces equally), so a default via round-trips byte-identically.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub solder_mask_expansion_back: Option<f64>,

    /// Positive drill tolerance in mm — `SubRecord-1` i32 @291. `None` writes the
    /// `0x7FFFFFFF` "unset" sentinel Altium uses (byte-identical to the template);
    /// `Some(mm)` writes the raw tolerance.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub hole_positive_tolerance: Option<f64>,

    /// Negative drill tolerance in mm — `SubRecord-1` i32 @295. `None` writes the
    /// `0x7FFFFFFF` "unset" sentinel (byte-identical to the template).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub hole_negative_tolerance: Option<f64>,

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

    /// Primitive flags (locked, keepout, tenting top/bottom) — common-header word @1-2.
    /// Tenting a via covers its pad with solder mask on the given face.
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,

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

/// Default via power-plane relief expansion (20 mil = 0.508mm; raw 200000).
const fn default_via_power_plane_relief_expansion() -> f64 {
    0.508
}

/// Default via power-plane (anti-pad) clearance (20 mil = 0.508mm; raw 200000).
const fn default_via_power_plane_clearance() -> f64 {
    0.508
}

/// Default via net index (`0xFFFF` = no net).
const fn default_via_net_index() -> u16 {
    0xFFFF
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
            solder_mask_expansion_mode: MaskExpansionMode::FromRule,
            solder_mask_expansion_back: None,
            hole_positive_tolerance: None,
            hole_negative_tolerance: None,
            paste_mask_expansion: 0.0,
            power_plane_connect_style: PowerPlaneConnectStyle::Relief,
            power_plane_relief_expansion: 0.508, // 20 mils
            power_plane_clearance: 0.508,        // 20 mils
            net_index: 0xFFFF,                   // no net
            thermal_relief_gap: 0.254,           // 10 mils
            thermal_relief_conductors: 4,
            thermal_relief_width: 0.254, // 10 mils
            diameter_stack_mode: ViaStackMode::Simple,
            per_layer_diameters: None,
            flags: PcbFlags::empty(),
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
            solder_mask_expansion_mode: MaskExpansionMode::FromRule,
            solder_mask_expansion_back: None,
            hole_positive_tolerance: None,
            hole_negative_tolerance: None,
            paste_mask_expansion: 0.0,
            power_plane_connect_style: PowerPlaneConnectStyle::Relief,
            power_plane_relief_expansion: 0.508, // 20 mils
            power_plane_clearance: 0.508,        // 20 mils
            net_index: 0xFFFF,                   // no net
            thermal_relief_gap: 0.254,           // 10 mils
            thermal_relief_conductors: 4,
            thermal_relief_width: 0.254, // 10 mils
            diameter_stack_mode: ViaStackMode::Simple,
            per_layer_diameters: None,
            flags: PcbFlags::empty(),
            unique_id: None,
        }
    }
}
