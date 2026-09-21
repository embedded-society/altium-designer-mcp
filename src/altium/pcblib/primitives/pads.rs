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

    /// The header layer byte exactly as read, kept only when the layer table
    /// does not map it — the primitive then sits on the `MultiLayer`
    /// catch-all, and without the byte a rewrite would store `74` in its
    /// place. Replayed while the primitive still sits there; moving it to a
    /// layer the model can name discards the byte for that layer's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_layer_id: Option<u8>,

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

    /// Whether the hole is plated — main-block byte @60. Altium stores this as
    /// an independent bool defaulting to `1` for every pad, SMD included (the
    /// golden fixture's SMD pads all carry `1` here, matching `AltiumSharp`'s
    /// `PcbPad.IsPlated = true` default), so the from-scratch default is `true`.
    #[serde(default = "default_true")]
    pub is_plated: bool,

    /// Whether solder-mask expansion is measured from the HOLE edge rather than
    /// the pad edge — main-block bool @125. Only meaningful on a pad with a hole.
    /// Default `false`, matching a factory Altium pad.
    #[serde(default)]
    pub solder_mask_expansion_from_hole_edge: bool,

    /// Hole shape for through-hole pads.
    #[serde(default, skip_serializing_if = "is_default_hole_shape")]
    pub hole_shape: HoleShape,

    /// Slot length in mm for a `Slot` hole — size/shape block i32 @263.
    /// Only meaningful when `hole_shape` is `Slot`. Default 0.0 (matches the
    /// value the writer emits by default).
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub hole_slot_length: f64,

    /// Hole rotation in degrees — size/shape block f64 @267. Rotates a slot hole.
    /// Default 0.0 (matches the value the writer emits by default).
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

    /// Jumper group id — main-block i16 @110-111. Pads sharing a non-zero id are
    /// linked as a jumper / 0-ohm net. `0` (no jumper) from scratch.
    #[serde(default, skip_serializing_if = "is_zero_i16")]
    pub jumper_id: i16,

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

    /// The pad's own polygon-connect style — Pad Stack → Thermal Relief in
    /// AD24 — stored after the main block's 194 bytes. `None` when the pad
    /// takes the style from the design rules, as a new pad does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polygon_connect: Option<PadPolygonConnect>,

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
        with = "crate::altium::serde_round::wh_pairs_opt"
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
        with = "crate::altium::serde_round::xy_points_opt"
    )]
    pub per_layer_offsets: Option<Vec<(f64, f64)>>,

    /// Primitive flags (locked, keepout, tenting, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,

    /// Net index into the board's net list — common-header u16 @3. `0xFFFF`
    /// (65535) means "no net", the from-scratch default (round-trip fidelity).
    #[serde(default = "default_net_index")]
    pub net_index: u16,

    /// Polygon index this pad belongs to — common-header u16 @5. `0xFFFF`
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
    /// Altium's stable identity for this primitive, from the footprint's
    /// `PrimitiveGuids` stream (`{XXXXXXXX-…}`), or `None` for a primitive
    /// with no recorded identity (anything built from scratch). Riding on the
    /// primitive itself, the identity follows it through structural edits —
    /// deleting a neighbour cannot re-point it, which the old footprint-level
    /// ordinal list could not guarantee.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    /// The pad's extended tail (main-block bytes 61..end) exactly as read
    /// (base64 in JSON), used as the write-side base with the typed tail
    /// fields overlaid. AD24 writes a 133-byte tail where the `AltiumSharp`
    /// template is 141, with two bytes differing — only replay bridges every
    /// AD version at once. `None` (from scratch) uses the template.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::base64_opt"
    )]
    pub raw_tail: Option<Vec<u8>>,

    /// Per-pad identity GUID — extended-tail bytes @126-141 ("GUID-A"), read
    /// back verbatim as a braced uppercase GUID string (Windows little-endian
    /// byte order, matching `AltiumSharp`'s `PcbPad.IdentityGuid`). A loaded
    /// pad round-trips its exact bytes (the scripting-authored golden carries
    /// the nil GUID `{00000000-…}` — preserved, not regenerated); `None` (the
    /// from-scratch default) makes the writer generate a fresh random GUID per
    /// pad, exactly the historical behaviour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_guid: Option<String>,

    /// Pad-stack / footprint-scoped identity GUID — extended-tail bytes
    /// @142-157 ("GUID-B", the twin of [`Self::identity_guid`]). Same
    /// round-trip-verbatim / fresh-when-`None` semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_guid_b: Option<String>,
}

/// Default `true` for serde (Altium's plated-hole default; see
/// [`Pad::is_plated`]).
const fn default_true() -> bool {
    true
}

/// Helper for serde to skip default hole shape in serialisation.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde requires reference
fn is_default_hole_shape(shape: &HoleShape) -> bool {
    *shape == HoleShape::default()
}

/// Default net index for a from-scratch pad/via (`0xFFFF` = no net). The
/// common-header connectivity indices default to "none" so a free library
/// primitive writes the same `0xFF` header bytes as before (byte-identity).
const fn default_net_index() -> u16 {
    0xFFFF
}

/// Default polygon index for a from-scratch pad/via (`0xFFFF` = none).
const fn default_polygon_index() -> u16 {
    0xFFFF
}

/// Default component index for a from-scratch pad/via (`-1` = free primitive,
/// stored as the `0xFFFF` common-header sentinel).
const fn default_component_index() -> i32 {
    -1
}

/// Default pad thermal-relief conductor width (10 mil = 0.254mm; raw 100000).
#[allow(clippy::trivially_copy_pass_by_ref)] // serde requires &T
const fn is_zero_i16(v: &i16) -> bool {
    *v == 0
}

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
            raw_layer_id: None,
            designator: designator.into(),
            x,
            y,
            width,
            height,
            shape: PadShape::RoundedRectangle,
            layer: Layer::TopLayer,
            hole_size: None,
            is_plated: true,
            jumper_id: 0,
            solder_mask_expansion_from_hole_edge: false,
            hole_shape: HoleShape::Round,
            hole_slot_length: 0.0,
            hole_rotation: 0.0,
            hole_positive_tolerance: None,
            hole_negative_tolerance: None,
            rotation: 0.0,
            paste_mask_expansion: None,
            solder_mask_expansion: None,
            paste_mask_expansion_mode: MaskExpansionMode::None,
            solder_mask_expansion_mode: MaskExpansionMode::None,
            power_plane_connect_style: PowerPlaneConnectStyle::Relief,
            relief_conductor_width: default_pad_relief_conductor_width(),
            relief_entries: default_pad_relief_entries(),
            relief_air_gap: default_pad_relief_air_gap(),
            power_plane_relief_expansion: default_pad_power_plane_relief_expansion(),
            power_plane_clearance: default_pad_power_plane_clearance(),
            polygon_connect: None,
            corner_radius_percent: None,
            stack_mode: PadStackMode::Simple,
            per_layer_sizes: None,
            per_layer_shapes: None,
            per_layer_corner_radii: None,
            per_layer_offsets: None,
            flags: PcbFlags::empty(),
            net_index: default_net_index(),
            polygon_index: default_polygon_index(),
            component_index: default_component_index(),
            unique_id: None,
            guid: None,
            raw_tail: None,
            identity_guid: None,
            identity_guid_b: None,
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
            raw_layer_id: None,
            designator: designator.into(),
            x,
            y,
            width,
            height,
            shape: PadShape::Round,
            layer: Layer::MultiLayer,
            hole_size: Some(hole_size),
            is_plated: true,
            jumper_id: 0,
            solder_mask_expansion_from_hole_edge: false,
            hole_shape: HoleShape::Round,
            hole_slot_length: 0.0,
            hole_rotation: 0.0,
            hole_positive_tolerance: None,
            hole_negative_tolerance: None,
            rotation: 0.0,
            paste_mask_expansion: None,
            solder_mask_expansion: None,
            paste_mask_expansion_mode: MaskExpansionMode::None,
            solder_mask_expansion_mode: MaskExpansionMode::None,
            power_plane_connect_style: PowerPlaneConnectStyle::Relief,
            relief_conductor_width: default_pad_relief_conductor_width(),
            relief_entries: default_pad_relief_entries(),
            relief_air_gap: default_pad_relief_air_gap(),
            power_plane_relief_expansion: default_pad_power_plane_relief_expansion(),
            power_plane_clearance: default_pad_power_plane_clearance(),
            polygon_connect: None,
            corner_radius_percent: None,
            stack_mode: PadStackMode::Simple,
            per_layer_sizes: None,
            per_layer_shapes: None,
            per_layer_corner_radii: None,
            per_layer_offsets: None,
            flags: PcbFlags::empty(),
            net_index: default_net_index(),
            polygon_index: default_polygon_index(),
            component_index: default_component_index(),
            unique_id: None,
            guid: None,
            raw_tail: None,
            identity_guid: None,
            identity_guid_b: None,
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

/// Solder / paste mask expansion mode. Shared by pads and vias.
///
/// This is Altium's `TCacheState = (eCacheInvalid, eCacheValid, eCacheManual)`
/// (ordinals 0/1/2, from the `Advpcb.dll` RTTI), so it describes the state of a
/// *cached* expansion rather than an on/off switch. The stored expansion value
/// is only authoritative when the state says so:
///
/// - `None` (0, `eCacheInvalid`) — the cached value is stale. This is what a
///   fresh Altium pad or via carries, so it is also our default.
/// - `FromRule` (1, `eCacheValid`) — the stored value is a rule result Altium
///   already computed. This is what Altium leaves behind once it has opened a
///   library and resolved the expansion itself.
/// - `Manual` (2, `eCacheManual`) — the stored value was specified by hand.
///
/// Only `Manual` survives a trip through Altium. `scripts/Verify-MaskCacheState.ps1`
/// hands Altium a library carrying all three states and shows it recomputing the
/// rule-driven ones on load: a pad written as `None`/0.0 and a pad written as
/// `FromRule`/0.0 both come back as `FromRule` with the rule's 4 mil, while the
/// `Manual` pad keeps its own number. So the state a rule-driven pad is written
/// with is a fidelity question, not a fabrication-outcome one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskExpansionMode {
    /// Cached expansion invalid — Altium recomputes it from the design rule.
    #[default]
    None,
    /// The stored expansion is a rule result Altium computed and will honour.
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

/// A pad's own polygon-connect style: how a polygon pour on the pad's net
/// joins it, set per pad in AD24 under Pad Stack → Thermal Relief (the
/// "Edit Polygon Connect Style" dialog).
///
/// Stored as a 34-byte block after the first 194 bytes of the pad's main
/// block: an `i32` length of 30, four zero bytes, `1` (override present),
/// the style byte, air gap and conductor width (`i32`), the rotation byte
/// (`1` = 90°, `0` = 45°), the conductor count, seven bytes this crate
/// does not model, the Auto-conductors byte, the minimum distance (`i32`),
/// the Min Distance checkbox and a zero byte (`manual/thermal_relief.PcbLib`). A pad without an
/// override has no such block.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PadPolygonConnect {
    /// How the pour joins the pad. Default: `Relief`.
    #[serde(default)]
    pub style: PowerPlaneConnectStyle,

    /// Gap between the pad and the pour in mm. Default: 0.254 (10 mil).
    #[serde(
        default = "ten_mil",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub air_gap: f64,

    /// Width of each relief conductor in mm. Default: 0.254 (10 mil).
    #[serde(
        default = "ten_mil",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub conductor_width: f64,

    /// Number of relief conductors, 2 or 4. Default: 4.
    #[serde(default = "four_conductors")]
    pub conductors: u8,

    /// Lets Altium choose the conductor count (the dialog's Auto).
    #[serde(default)]
    pub auto_conductors: bool,

    /// Conductor angle in degrees, 45 or 90. Default: 90.
    #[serde(default = "ninety_degrees")]
    pub rotation: u16,

    /// The dialog's Min Distance in mm (shown with Auto), used when
    /// [`Self::min_distance_enabled`] is set. Default: 0.381 (15 mil).
    #[serde(
        default = "fifteen_mil",
        serialize_with = "crate::altium::serde_round::serialize"
    )]
    pub min_distance: f64,

    /// The dialog's Min Distance checkbox. Default: unticked.
    #[serde(default)]
    pub min_distance_enabled: bool,
}

impl Default for PadPolygonConnect {
    /// The override AD24 writes when the Thermal Relief box is ticked and
    /// nothing else is changed.
    fn default() -> Self {
        Self {
            style: PowerPlaneConnectStyle::Relief,
            air_gap: ten_mil(),
            conductor_width: ten_mil(),
            conductors: four_conductors(),
            auto_conductors: false,
            rotation: ninety_degrees(),
            min_distance: fifteen_mil(),
            min_distance_enabled: false,
        }
    }
}

/// 10 mil in mm.
const fn ten_mil() -> f64 {
    0.254
}

/// 15 mil in mm.
const fn fifteen_mil() -> f64 {
    0.381
}

/// The default relief conductor count.
const fn four_conductors() -> u8 {
    4
}

/// The default relief conductor angle.
const fn ninety_degrees() -> u16 {
    90
}

/// How a via's drill span is classified — `SubRecord-1` byte @312.
///
/// A through via spans the whole board; the blind/buried kinds mark which end of a
/// drill-pair sequence the via belongs to. `from_layer` / `to_layer` describe the span
/// but not this classification, which Altium stores separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrillLayerPairType {
    /// Spans the full board (Altium's default).
    #[default]
    Through,
    /// First drill pair of a blind/buried sequence.
    BlindBuriedStart,
    /// An intermediate drill pair.
    Mid,
    /// Final drill pair of the sequence.
    End,
}

impl DrillLayerPairType {
    /// Creates from the Altium byte (`0` = `Through`, `1` = `BlindBuriedStart`,
    /// `2` = `Mid`, `3` = `End`). An unknown value reads as `Through`.
    #[must_use]
    pub const fn from_id(id: u8) -> Self {
        match id {
            1 => Self::BlindBuriedStart,
            2 => Self::Mid,
            3 => Self::End,
            _ => Self::Through,
        }
    }

    /// Returns the Altium byte.
    #[must_use]
    pub const fn to_id(self) -> u8 {
        match self {
            Self::Through => 0,
            Self::BlindBuriedStart => 1,
            Self::Mid => 2,
            Self::End => 3,
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

    /// Solder mask expansion mode (`None` / `FromRule` / `Manual`) — `SubRecord-1`
    /// byte @66. A fresh Altium via carries `None`, deferring to the design rule.
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

    /// Polygon index this via belongs to — common-header u16 @5. `0xFFFF`
    /// (none) from scratch, matching the historical writer output.
    #[serde(default = "default_polygon_index")]
    pub polygon_index: u16,

    /// Component index into the board's component list — common-header u16 @7
    /// (`0xFFFF` stored, exposed as `-1`). `-1` (free primitive) from scratch.
    #[serde(default = "default_component_index")]
    pub component_index: i32,

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

    /// Whether solder-mask expansion is measured from the HOLE edge rather than the
    /// pad edge — `SubRecord-1` bool @258. Default `false`, matching the template.
    #[serde(default)]
    pub solder_mask_expansion_from_hole_edge: bool,

    /// Drill-pair classification — `SubRecord-1` byte @312. Default `Through`.
    #[serde(default)]
    pub drill_layer_pair_type: DrillLayerPairType,

    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
    /// Altium's stable identity for this primitive, from the footprint's
    /// `PrimitiveGuids` stream (`{XXXXXXXX-…}`), or `None` for a primitive
    /// with no recorded identity (anything built from scratch). Riding on the
    /// primitive itself, the identity follows it through structural edits —
    /// deleting a neighbour cannot re-point it, which the old footprint-level
    /// ordinal list could not guarantee.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    /// The via's whole record block exactly as read (base64 in JSON), used as
    /// the write-side base with every typed field overlaid — so unmodelled
    /// bytes (the two in-record identity GUID slots, cache values, template
    /// drift between AD versions, the thirty bytes an older library's
    /// 351-byte vias carry past the 321-byte template) round-trip verbatim,
    /// length included. `None` (from scratch) uses the template with the GUID
    /// slots zeroed, which is what AD24 itself writes for library vias (the
    /// golden's are all zeros).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::base64_opt"
    )]
    pub raw_block: Option<Vec<u8>>,
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
            solder_mask_expansion_mode: MaskExpansionMode::None,
            solder_mask_expansion_back: None,
            hole_positive_tolerance: None,
            hole_negative_tolerance: None,
            paste_mask_expansion: 0.0,
            power_plane_connect_style: PowerPlaneConnectStyle::Relief,
            power_plane_relief_expansion: 0.508, // 20 mils
            power_plane_clearance: 0.508,        // 20 mils
            net_index: 0xFFFF,                   // no net
            polygon_index: 0xFFFF,               // none
            component_index: -1,                 // free primitive
            thermal_relief_gap: 0.254,           // 10 mils
            thermal_relief_conductors: 4,
            thermal_relief_width: 0.254, // 10 mils
            diameter_stack_mode: ViaStackMode::Simple,
            per_layer_diameters: None,
            flags: PcbFlags::empty(),
            solder_mask_expansion_from_hole_edge: false,
            drill_layer_pair_type: DrillLayerPairType::Through,
            unique_id: None,
            guid: None,
            raw_block: None,
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
            solder_mask_expansion_mode: MaskExpansionMode::None,
            solder_mask_expansion_back: None,
            hole_positive_tolerance: None,
            hole_negative_tolerance: None,
            paste_mask_expansion: 0.0,
            power_plane_connect_style: PowerPlaneConnectStyle::Relief,
            power_plane_relief_expansion: 0.508, // 20 mils
            power_plane_clearance: 0.508,        // 20 mils
            net_index: 0xFFFF,                   // no net
            polygon_index: 0xFFFF,               // none
            component_index: -1,                 // free primitive
            thermal_relief_gap: 0.254,           // 10 mils
            thermal_relief_conductors: 4,
            thermal_relief_width: 0.254, // 10 mils
            diameter_stack_mode: ViaStackMode::Simple,
            per_layer_diameters: None,
            flags: PcbFlags::empty(),
            solder_mask_expansion_from_hole_edge: false,
            drill_layer_pair_type: DrillLayerPairType::Through,
            unique_id: None,
            guid: None,
            raw_block: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HoleShape, MaskExpansionMode, Pad, PowerPlaneConnectStyle, Via};

    #[test]
    fn mask_expansion_mode_round_trips_and_defaults() {
        for (id, mode) in [
            (0, MaskExpansionMode::None),
            (1, MaskExpansionMode::FromRule),
            (2, MaskExpansionMode::Manual),
        ] {
            assert_eq!(MaskExpansionMode::from_id(id), mode);
            assert_eq!(mode.to_id(), id);
        }
        // Unknown bytes fall back to the design-rule default.
        assert_eq!(MaskExpansionMode::from_id(99), MaskExpansionMode::FromRule);
    }

    #[test]
    fn power_plane_connect_style_round_trips_and_defaults() {
        for (id, style) in [
            (0, PowerPlaneConnectStyle::Relief),
            (1, PowerPlaneConnectStyle::Direct),
            (2, PowerPlaneConnectStyle::NoConnect),
        ] {
            assert_eq!(PowerPlaneConnectStyle::from_id(id), style);
            assert_eq!(style.to_id(), id);
        }
        // Unknown bytes fall back to thermal relief (Altium's default).
        assert_eq!(
            PowerPlaneConnectStyle::from_id(99),
            PowerPlaneConnectStyle::Relief
        );
    }

    // ==================== serde defaults =====================================
    //
    // These fire when a caller's JSON omits the field — the read-modify-write
    // path, where `read_pcblib` skips anything at its default and the value has
    // to come back on the way in. A wrong default here is silent: the primitive
    // deserialises fine and writes different bytes than it was read with.

    #[test]
    fn an_omitted_pad_field_deserialises_to_altiums_own_default() {
        let pad: Pad =
            serde_json::from_str(r#"{"designator":"1","x":0.0,"y":0.0,"width":1.0,"height":1.0}"#)
                .expect("a minimal pad should deserialise");

        // Altium marks every pad plated, SMD included, so omitting the key must
        // not read as "unplated" — that would turn a plated hole into an NPTH.
        assert!(pad.is_plated);
        assert_eq!(pad.hole_shape, HoleShape::default());
    }

    #[test]
    fn an_omitted_via_field_deserialises_to_the_altium_template_value() {
        let via: Via = serde_json::from_str(r#"{"x":0.0,"y":0.0,"diameter":0.6,"hole_size":0.3}"#)
            .expect("a minimal via should deserialise");

        // Thermal relief: a zero gap or zero conductor width would flood-connect
        // the via straight into a plane, making the board an infinite heat sink
        // and effectively unsolderable.
        assert!((via.thermal_relief_gap - 0.254).abs() < 1e-9);
        assert_eq!(via.thermal_relief_conductors, 4);
        assert!((via.thermal_relief_width - 0.254).abs() < 1e-9);

        // Power-plane clearances: zero here shorts the via to every plane it
        // passes through.
        assert!((via.power_plane_relief_expansion - 0.508).abs() < 1e-9);
        assert!((via.power_plane_clearance - 0.508).abs() < 1e-9);

        // 0xFFFF is the "no net" sentinel; 0 would claim net index zero.
        assert_eq!(via.net_index, 0xFFFF);
    }
}
