//! Footprint primitive types for `PcbLib` files.
//!
//! These types represent the geometric elements that make up a footprint:
//! pads, tracks, arcs, regions, and text.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    /// Flags for PCB primitives stored in the common header (bytes 1-2).
    ///
    /// These flags control various properties of primitives such as locking,
    /// keep-out zones, and solder mask tenting.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct PcbFlags: u16 {
        /// Primitive is locked and cannot be moved/edited.
        const LOCKED = 0x0001;
        /// Primitive is part of a polygon pour.
        const POLYGON = 0x0002;
        /// Primitive defines a keep-out region.
        const KEEPOUT = 0x0004;
        /// Top solder mask tenting enabled (covers the pad/via).
        const TENTING_TOP = 0x0008;
        /// Bottom solder mask tenting enabled (covers the pad/via).
        const TENTING_BOTTOM = 0x0010;
    }
}

/// A PCB pad (SMD or through-hole).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pad {
    /// Pad designator (e.g., "1", "2", "A1").
    pub designator: String,

    /// X position in mm (from footprint origin).
    pub x: f64,

    /// Y position in mm (from footprint origin).
    pub y: f64,

    /// Pad width in mm.
    pub width: f64,

    /// Pad height in mm.
    pub height: f64,

    /// Pad shape.
    #[serde(default)]
    pub shape: PadShape,

    /// Layer the pad is on.
    #[serde(default)]
    pub layer: Layer,

    /// Hole diameter for through-hole pads (mm). None for SMD pads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hole_size: Option<f64>,

    /// Hole shape for through-hole pads.
    #[serde(default, skip_serializing_if = "is_default_hole_shape")]
    pub hole_shape: HoleShape,

    /// Rotation angle in degrees.
    #[serde(default)]
    pub rotation: f64,

    /// Paste mask expansion in mm. None uses design rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paste_mask_expansion: Option<f64>,

    /// Solder mask expansion in mm. None uses design rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_layer_sizes: Option<Vec<(f64, f64)>>,

    /// Per-layer pad shapes for 32 layers.
    /// Only used when `stack_mode` != `Simple`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_layer_shapes: Option<Vec<PadShape>>,

    /// Per-layer corner radius percentages (0-100) for 32 layers.
    /// Only used when `stack_mode` != `Simple`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_layer_corner_radii: Option<Vec<u8>>,

    /// Per-layer offset from hole center in mm (x, y) for 32 layers.
    /// Only used when `stack_mode` != `Simple`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_layer_offsets: Option<Vec<(f64, f64)>>,

    /// Primitive flags (locked, keepout, tenting, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
}

/// Helper for serde to skip default hole shape in serialization.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde requires reference
fn is_default_hole_shape(shape: &HoleShape) -> bool {
    *shape == HoleShape::default()
}

impl Pad {
    /// Creates a new SMD pad.
    #[must_use]
    pub fn smd(designator: impl Into<String>, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            designator: designator.into(),
            x,
            y,
            width,
            height,
            shape: PadShape::RoundedRectangle,
            layer: Layer::MultiLayer,
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
    pub x: f64,

    /// Y position in mm (from footprint origin).
    pub y: f64,

    /// Via diameter (annular ring outer diameter) in mm.
    pub diameter: f64,

    /// Hole diameter in mm.
    pub hole_size: f64,

    /// Starting layer for the via.
    #[serde(default)]
    pub from_layer: Layer,

    /// Ending layer for the via.
    #[serde(default)]
    pub to_layer: Layer,

    /// Solder mask expansion in mm (negative = tented).
    #[serde(default)]
    pub solder_mask_expansion: f64,

    /// Whether solder mask expansion is manually set.
    #[serde(default)]
    pub solder_mask_expansion_manual: bool,

    // Thermal relief settings (for polygon pours)
    /// Thermal relief air gap width in mm (default: 0.254mm = 10 mils).
    #[serde(default = "default_thermal_relief_gap")]
    pub thermal_relief_gap: f64,

    /// Number of thermal relief conductors (default: 4).
    #[serde(default = "default_thermal_relief_conductors")]
    pub thermal_relief_conductors: u8,

    /// Thermal relief conductor width in mm (default: 0.254mm = 10 mils).
    #[serde(default = "default_thermal_relief_width")]
    pub thermal_relief_width: f64,

    // Diameter stack mode
    /// Diameter stack mode (`Simple`, `TopMiddleBottom`, or `FullStack`).
    #[serde(default)]
    pub diameter_stack_mode: ViaStackMode,

    /// Per-layer diameters in mm (32 layers). Only used when `stack_mode` != `Simple`.
    /// Index 0 = Top Layer, Index 1 = Bottom Layer, Index 2-31 = Mid Layers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_layer_diameters: Option<Vec<f64>>,
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
        }
    }
}

/// A track (line segment) on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Start X position in mm.
    pub x1: f64,
    /// Start Y position in mm.
    pub y1: f64,
    /// End X position in mm.
    pub x2: f64,
    /// End Y position in mm.
    pub y2: f64,
    /// Line width in mm.
    pub width: f64,
    /// Layer the track is on.
    pub layer: Layer,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
}

impl Track {
    /// Creates a new track.
    #[must_use]
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64, width: f64, layer: Layer) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            width,
            layer,
            flags: PcbFlags::empty(),
        }
    }
}

/// An arc or circle on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    /// Centre X position in mm.
    pub x: f64,
    /// Centre Y position in mm.
    pub y: f64,
    /// Radius in mm.
    pub radius: f64,
    /// Start angle in degrees (0 = right, counter-clockwise).
    pub start_angle: f64,
    /// End angle in degrees.
    pub end_angle: f64,
    /// Line width in mm.
    pub width: f64,
    /// Layer the arc is on.
    pub layer: Layer,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
}

impl Arc {
    /// Creates a full circle.
    #[must_use]
    pub const fn circle(x: f64, y: f64, radius: f64, width: f64, layer: Layer) -> Self {
        Self {
            x,
            y,
            radius,
            start_angle: 0.0,
            end_angle: 360.0,
            width,
            layer,
            flags: PcbFlags::empty(),
        }
    }
}

/// A filled region (polygon).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    /// Vertices of the polygon.
    pub vertices: Vec<Vertex>,
    /// Layer the region is on.
    pub layer: Layer,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
}

impl Region {
    /// Creates a rectangular region.
    #[must_use]
    pub fn rectangle(min_x: f64, min_y: f64, max_x: f64, max_y: f64, layer: Layer) -> Self {
        Self {
            vertices: vec![
                Vertex { x: min_x, y: min_y },
                Vertex { x: max_x, y: min_y },
                Vertex { x: max_x, y: max_y },
                Vertex { x: min_x, y: max_y },
            ],
            layer,
            flags: PcbFlags::empty(),
        }
    }
}

/// A vertex in a region polygon.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    /// X position in mm.
    pub x: f64,
    /// Y position in mm.
    pub y: f64,
}

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

/// Text justification (alignment).
///
/// Specifies how text is aligned relative to its anchor point.
/// The 9 positions form a 3x3 grid combining vertical (Bottom/Middle/Top)
/// and horizontal (Left/Center/Right) alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextJustification {
    /// Bottom-left aligned.
    BottomLeft,
    /// Bottom-center aligned.
    BottomCenter,
    /// Bottom-right aligned.
    BottomRight,
    /// Middle-left aligned.
    MiddleLeft,
    /// Middle-center aligned.
    #[default]
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

/// A text string on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    /// X position in mm.
    pub x: f64,
    /// Y position in mm.
    pub y: f64,
    /// Text content.
    pub text: String,
    /// Text height in mm.
    pub height: f64,
    /// Layer the text is on.
    pub layer: Layer,
    /// Rotation angle in degrees.
    #[serde(default)]
    pub rotation: f64,
    /// Text rendering kind (Stroke, TrueType, or `BarCode`).
    #[serde(default)]
    pub kind: TextKind,
    /// Stroke font type (only applies when `kind` is `Stroke`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_font: Option<StrokeFont>,
    /// Text justification (alignment).
    #[serde(default)]
    pub justification: TextJustification,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
}

/// A filled rectangle on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    /// First corner X position in mm.
    pub x1: f64,
    /// First corner Y position in mm.
    pub y1: f64,
    /// Second corner X position in mm.
    pub x2: f64,
    /// Second corner Y position in mm.
    pub y2: f64,
    /// Layer the fill is on.
    pub layer: Layer,
    /// Rotation angle in degrees.
    #[serde(default)]
    pub rotation: f64,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
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
        }
    }
}

/// A 3D model reference (simple version for programmatic creation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model3D {
    /// Path to the STEP file.
    pub filepath: String,
    /// X offset from footprint origin in mm.
    #[serde(default)]
    pub x_offset: f64,
    /// Y offset from footprint origin in mm.
    #[serde(default)]
    pub y_offset: f64,
    /// Z offset from board surface in mm.
    #[serde(default)]
    pub z_offset: f64,
    /// Rotation around Z axis in degrees.
    #[serde(default)]
    pub rotation: f64,
}

/// An embedded 3D model stored in the `/Library/Models/` storage.
///
/// 3D models in Altium `PcbLib` files are stored as zlib-compressed STEP data.
/// Each model has a GUID identifier that is referenced by `ComponentBody` records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddedModel {
    /// Model GUID (e.g., "{ABCD1234-5678-90EF-GHIJ-KLMNOPQRSTUV}").
    pub id: String,

    /// Model filename (e.g., "RESC1005X04L.step").
    #[serde(default)]
    pub name: String,

    /// Decompressed STEP file data.
    ///
    /// This is the raw STEP/IGES model data after zlib decompression.
    #[serde(skip)]
    pub data: Vec<u8>,

    /// Compressed size in bytes (for reference).
    #[serde(skip)]
    pub compressed_size: usize,
}

impl EmbeddedModel {
    /// Creates a new embedded model with the given ID and data.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            compressed_size: 0,
            data,
        }
    }

    /// Returns the decompressed data as a string (if valid UTF-8).
    ///
    /// STEP files are ASCII text, so this should work for valid models.
    #[must_use]
    pub fn as_string(&self) -> Option<String> {
        String::from_utf8(self.data.clone()).ok()
    }

    /// Returns the size of the decompressed data in bytes.
    #[must_use]
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// A 3D component body primitive (record type 0x0C).
///
/// This represents an embedded 3D model in the footprint. The model data
/// is stored in `/Library/Models/N` streams and referenced by GUID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentBody {
    /// Model identifier (GUID) referencing `/Library/Models/Data`.
    pub model_id: String,

    /// Model filename (e.g., "RESC1005X04L.step").
    #[serde(default)]
    pub model_name: String,

    /// Whether the model is embedded in the library.
    #[serde(default)]
    pub embedded: bool,

    /// Rotation around X axis in degrees.
    #[serde(default)]
    pub rotation_x: f64,

    /// Rotation around Y axis in degrees.
    #[serde(default)]
    pub rotation_y: f64,

    /// Rotation around Z axis in degrees.
    #[serde(default)]
    pub rotation_z: f64,

    /// Z offset (standoff from board) in mm.
    #[serde(default)]
    pub z_offset: f64,

    /// Overall height in mm.
    #[serde(default)]
    pub overall_height: f64,

    /// Standoff height in mm.
    #[serde(default)]
    pub standoff_height: f64,

    /// Layer the body outline is on.
    #[serde(default)]
    pub layer: Layer,
}

impl ComponentBody {
    /// Creates a new `ComponentBody` with the given model ID.
    #[must_use]
    pub fn new(model_id: impl Into<String>, model_name: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            model_name: model_name.into(),
            embedded: true,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            z_offset: 0.0,
            overall_height: 0.0,
            standoff_height: 0.0,
            layer: Layer::Top3DBody,
        }
    }
}

/// Altium layer identifiers.
///
/// # Recommended Layers for Footprints
///
/// AI assistants should prefer these dedicated layers over generic mechanical layers:
///
/// | Purpose | Recommended Layer |
/// |---------|-------------------|
/// | Pads (SMD) | `TopLayer` or `BottomLayer` |
/// | Pads (through-hole) | `MultiLayer` |
/// | Silkscreen | `TopOverlay` / `BottomOverlay` |
/// | Assembly outline | `TopAssembly` / `BottomAssembly` |
/// | Courtyard | `TopCourtyard` / `BottomCourtyard` |
/// | 3D body outline | `Top3DBody` / `Bottom3DBody` |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Layer {
    // Copper layers
    /// Top copper layer (ID 1).
    #[serde(rename = "Top Layer", alias = "TopLayer")]
    TopLayer,
    /// Mid layer 1 (ID 2).
    #[serde(rename = "Mid-Layer 1", alias = "MidLayer1")]
    MidLayer1,
    /// Mid layer 2 (ID 3).
    #[serde(rename = "Mid-Layer 2", alias = "MidLayer2")]
    MidLayer2,
    /// Mid layer 3 (ID 4).
    #[serde(rename = "Mid-Layer 3", alias = "MidLayer3")]
    MidLayer3,
    /// Mid layer 4 (ID 5).
    #[serde(rename = "Mid-Layer 4", alias = "MidLayer4")]
    MidLayer4,
    /// Mid layer 5 (ID 6).
    #[serde(rename = "Mid-Layer 5", alias = "MidLayer5")]
    MidLayer5,
    /// Mid layer 6 (ID 7).
    #[serde(rename = "Mid-Layer 6", alias = "MidLayer6")]
    MidLayer6,
    /// Mid layer 7 (ID 8).
    #[serde(rename = "Mid-Layer 7", alias = "MidLayer7")]
    MidLayer7,
    /// Mid layer 8 (ID 9).
    #[serde(rename = "Mid-Layer 8", alias = "MidLayer8")]
    MidLayer8,
    /// Mid layer 9 (ID 10).
    #[serde(rename = "Mid-Layer 9", alias = "MidLayer9")]
    MidLayer9,
    /// Mid layer 10 (ID 11).
    #[serde(rename = "Mid-Layer 10", alias = "MidLayer10")]
    MidLayer10,
    /// Mid layer 11 (ID 12).
    #[serde(rename = "Mid-Layer 11", alias = "MidLayer11")]
    MidLayer11,
    /// Mid layer 12 (ID 13).
    #[serde(rename = "Mid-Layer 12", alias = "MidLayer12")]
    MidLayer12,
    /// Mid layer 13 (ID 14).
    #[serde(rename = "Mid-Layer 13", alias = "MidLayer13")]
    MidLayer13,
    /// Mid layer 14 (ID 15).
    #[serde(rename = "Mid-Layer 14", alias = "MidLayer14")]
    MidLayer14,
    /// Mid layer 15 (ID 16).
    #[serde(rename = "Mid-Layer 15", alias = "MidLayer15")]
    MidLayer15,
    /// Mid layer 16 (ID 17).
    #[serde(rename = "Mid-Layer 16", alias = "MidLayer16")]
    MidLayer16,
    /// Mid layer 17 (ID 18).
    #[serde(rename = "Mid-Layer 17", alias = "MidLayer17")]
    MidLayer17,
    /// Mid layer 18 (ID 19).
    #[serde(rename = "Mid-Layer 18", alias = "MidLayer18")]
    MidLayer18,
    /// Mid layer 19 (ID 20).
    #[serde(rename = "Mid-Layer 19", alias = "MidLayer19")]
    MidLayer19,
    /// Mid layer 20 (ID 21).
    #[serde(rename = "Mid-Layer 20", alias = "MidLayer20")]
    MidLayer20,
    /// Mid layer 21 (ID 22).
    #[serde(rename = "Mid-Layer 21", alias = "MidLayer21")]
    MidLayer21,
    /// Mid layer 22 (ID 23).
    #[serde(rename = "Mid-Layer 22", alias = "MidLayer22")]
    MidLayer22,
    /// Mid layer 23 (ID 24).
    #[serde(rename = "Mid-Layer 23", alias = "MidLayer23")]
    MidLayer23,
    /// Mid layer 24 (ID 25).
    #[serde(rename = "Mid-Layer 24", alias = "MidLayer24")]
    MidLayer24,
    /// Mid layer 25 (ID 26).
    #[serde(rename = "Mid-Layer 25", alias = "MidLayer25")]
    MidLayer25,
    /// Mid layer 26 (ID 27).
    #[serde(rename = "Mid-Layer 26", alias = "MidLayer26")]
    MidLayer26,
    /// Mid layer 27 (ID 28).
    #[serde(rename = "Mid-Layer 27", alias = "MidLayer27")]
    MidLayer27,
    /// Mid layer 28 (ID 29).
    #[serde(rename = "Mid-Layer 28", alias = "MidLayer28")]
    MidLayer28,
    /// Mid layer 29 (ID 30).
    #[serde(rename = "Mid-Layer 29", alias = "MidLayer29")]
    MidLayer29,
    /// Mid layer 30 (ID 31).
    #[serde(rename = "Mid-Layer 30", alias = "MidLayer30")]
    MidLayer30,
    /// Bottom copper layer (ID 32).
    #[serde(rename = "Bottom Layer", alias = "BottomLayer")]
    BottomLayer,
    /// Multi-layer (all copper layers, for through-hole pads).
    #[default]
    #[serde(rename = "Multi-Layer", alias = "MultiLayer")]
    MultiLayer,

    // Silkscreen
    /// Top silkscreen (overlay).
    #[serde(rename = "Top Overlay", alias = "TopOverlay")]
    TopOverlay,
    /// Bottom silkscreen.
    #[serde(rename = "Bottom Overlay", alias = "BottomOverlay")]
    BottomOverlay,

    // Solder mask
    /// Top solder mask.
    #[serde(rename = "Top Solder", alias = "TopSolder")]
    TopSolder,
    /// Bottom solder mask.
    #[serde(rename = "Bottom Solder", alias = "BottomSolder")]
    BottomSolder,

    // Internal planes (IDs 39-54)
    /// Internal plane 1 (ID 39).
    #[serde(rename = "Internal Plane 1", alias = "InternalPlane1")]
    InternalPlane1,
    /// Internal plane 2 (ID 40).
    #[serde(rename = "Internal Plane 2", alias = "InternalPlane2")]
    InternalPlane2,
    /// Internal plane 3 (ID 41).
    #[serde(rename = "Internal Plane 3", alias = "InternalPlane3")]
    InternalPlane3,
    /// Internal plane 4 (ID 42).
    #[serde(rename = "Internal Plane 4", alias = "InternalPlane4")]
    InternalPlane4,
    /// Internal plane 5 (ID 43).
    #[serde(rename = "Internal Plane 5", alias = "InternalPlane5")]
    InternalPlane5,
    /// Internal plane 6 (ID 44).
    #[serde(rename = "Internal Plane 6", alias = "InternalPlane6")]
    InternalPlane6,
    /// Internal plane 7 (ID 45).
    #[serde(rename = "Internal Plane 7", alias = "InternalPlane7")]
    InternalPlane7,
    /// Internal plane 8 (ID 46).
    #[serde(rename = "Internal Plane 8", alias = "InternalPlane8")]
    InternalPlane8,
    /// Internal plane 9 (ID 47).
    #[serde(rename = "Internal Plane 9", alias = "InternalPlane9")]
    InternalPlane9,
    /// Internal plane 10 (ID 48).
    #[serde(rename = "Internal Plane 10", alias = "InternalPlane10")]
    InternalPlane10,
    /// Internal plane 11 (ID 49).
    #[serde(rename = "Internal Plane 11", alias = "InternalPlane11")]
    InternalPlane11,
    /// Internal plane 12 (ID 50).
    #[serde(rename = "Internal Plane 12", alias = "InternalPlane12")]
    InternalPlane12,
    /// Internal plane 13 (ID 51).
    #[serde(rename = "Internal Plane 13", alias = "InternalPlane13")]
    InternalPlane13,
    /// Internal plane 14 (ID 52).
    #[serde(rename = "Internal Plane 14", alias = "InternalPlane14")]
    InternalPlane14,
    /// Internal plane 15 (ID 53).
    #[serde(rename = "Internal Plane 15", alias = "InternalPlane15")]
    InternalPlane15,
    /// Internal plane 16 (ID 54).
    #[serde(rename = "Internal Plane 16", alias = "InternalPlane16")]
    InternalPlane16,

    // Drill layers
    /// Drill guide layer (ID 55).
    #[serde(rename = "Drill Guide", alias = "DrillGuide")]
    DrillGuide,
    /// Drill drawing layer (ID 73).
    #[serde(rename = "Drill Drawing", alias = "DrillDrawing")]
    DrillDrawing,

    // Paste
    /// Top solder paste.
    #[serde(rename = "Top Paste", alias = "TopPaste")]
    TopPaste,
    /// Bottom solder paste.
    #[serde(rename = "Bottom Paste", alias = "BottomPaste")]
    BottomPaste,

    // Component layer pairs (preferred over generic mechanical layers)
    /// Top assembly outline (component body outline for documentation).
    #[serde(rename = "Top Assembly", alias = "TopAssembly")]
    TopAssembly,
    /// Bottom assembly outline.
    #[serde(rename = "Bottom Assembly", alias = "BottomAssembly")]
    BottomAssembly,
    /// Top courtyard (component keepout area per IPC-7351).
    #[serde(rename = "Top Courtyard", alias = "TopCourtyard")]
    TopCourtyard,
    /// Bottom courtyard.
    #[serde(rename = "Bottom Courtyard", alias = "BottomCourtyard")]
    BottomCourtyard,
    /// Top 3D body outline (for 3D model placement).
    #[serde(rename = "Top 3D Body", alias = "Top3DBody")]
    Top3DBody,
    /// Bottom 3D body outline.
    #[serde(rename = "Bottom 3D Body", alias = "Bottom3DBody")]
    Bottom3DBody,

    // Generic mechanical layers (use component layer pairs when possible)
    /// Mechanical layer 1 (ID 57).
    #[serde(rename = "Mechanical 1", alias = "Mechanical1")]
    Mechanical1,
    /// Mechanical layer 2 (ID 58 - aliased to `TopAssembly`).
    #[serde(rename = "Mechanical 2", alias = "Mechanical2")]
    Mechanical2,
    /// Mechanical layer 3 (ID 59 - aliased to `BottomAssembly`).
    #[serde(rename = "Mechanical 3", alias = "Mechanical3")]
    Mechanical3,
    /// Mechanical layer 4 (ID 60 - aliased to `TopCourtyard`).
    #[serde(rename = "Mechanical 4", alias = "Mechanical4")]
    Mechanical4,
    /// Mechanical layer 5 (ID 61 - aliased to `BottomCourtyard`).
    #[serde(rename = "Mechanical 5", alias = "Mechanical5")]
    Mechanical5,
    /// Mechanical layer 6 (ID 62 - aliased to `Top3DBody`).
    #[serde(rename = "Mechanical 6", alias = "Mechanical6")]
    Mechanical6,
    /// Mechanical layer 7 (ID 63 - aliased to `Bottom3DBody`).
    #[serde(rename = "Mechanical 7", alias = "Mechanical7")]
    Mechanical7,
    /// Mechanical layer 8 (ID 64).
    #[serde(rename = "Mechanical 8", alias = "Mechanical8")]
    Mechanical8,
    /// Mechanical layer 9 (ID 65).
    #[serde(rename = "Mechanical 9", alias = "Mechanical9")]
    Mechanical9,
    /// Mechanical layer 10 (ID 66).
    #[serde(rename = "Mechanical 10", alias = "Mechanical10")]
    Mechanical10,
    /// Mechanical layer 11 (ID 67).
    #[serde(rename = "Mechanical 11", alias = "Mechanical11")]
    Mechanical11,
    /// Mechanical layer 12 (ID 68).
    #[serde(rename = "Mechanical 12", alias = "Mechanical12")]
    Mechanical12,
    /// Mechanical layer 13 (ID 69).
    #[serde(rename = "Mechanical 13", alias = "Mechanical13")]
    Mechanical13,
    /// Mechanical layer 14 (ID 70).
    #[serde(rename = "Mechanical 14", alias = "Mechanical14")]
    Mechanical14,
    /// Mechanical layer 15 (ID 71).
    #[serde(rename = "Mechanical 15", alias = "Mechanical15")]
    Mechanical15,
    /// Mechanical layer 16 (ID 72).
    #[serde(rename = "Mechanical 16", alias = "Mechanical16")]
    Mechanical16,

    // Special layers (IDs 75-85)
    /// Connect layer (ID 75).
    #[serde(rename = "Connect Layer", alias = "ConnectLayer")]
    ConnectLayer,
    /// Background layer (ID 76).
    #[serde(rename = "Background Layer", alias = "BackgroundLayer")]
    BackgroundLayer,
    /// DRC error layer (ID 77).
    #[serde(rename = "DRC Error Layer", alias = "DRCErrorLayer")]
    DRCErrorLayer,
    /// Highlight layer (ID 78).
    #[serde(rename = "Highlight Layer", alias = "HighlightLayer")]
    HighlightLayer,
    /// Grid color 1 layer (ID 79).
    #[serde(rename = "Grid Color 1", alias = "GridColor1")]
    GridColor1,
    /// Grid color 10 layer (ID 80).
    #[serde(rename = "Grid Color 10", alias = "GridColor10")]
    GridColor10,
    /// Pad hole layer (ID 81).
    #[serde(rename = "Pad Hole Layer", alias = "PadHoleLayer")]
    PadHoleLayer,
    /// Via hole layer (ID 82).
    #[serde(rename = "Via Hole Layer", alias = "ViaHoleLayer")]
    ViaHoleLayer,
    /// Top pad master layer (ID 83).
    #[serde(rename = "Top Pad Master", alias = "TopPadMaster")]
    TopPadMaster,
    /// Bottom pad master layer (ID 84).
    #[serde(rename = "Bottom Pad Master", alias = "BottomPadMaster")]
    BottomPadMaster,
    /// DRC detail layer (ID 85).
    #[serde(rename = "DRC Detail Layer", alias = "DRCDetailLayer")]
    DRCDetailLayer,

    // Keep-out
    /// Keep-out layer (ID 56).
    #[serde(rename = "Keep-Out Layer", alias = "KeepOut")]
    KeepOut,
}

impl Layer {
    /// Returns the Altium layer name string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::TopLayer => "Top Layer",
            Self::MidLayer1 => "Mid-Layer 1",
            Self::MidLayer2 => "Mid-Layer 2",
            Self::MidLayer3 => "Mid-Layer 3",
            Self::MidLayer4 => "Mid-Layer 4",
            Self::MidLayer5 => "Mid-Layer 5",
            Self::MidLayer6 => "Mid-Layer 6",
            Self::MidLayer7 => "Mid-Layer 7",
            Self::MidLayer8 => "Mid-Layer 8",
            Self::MidLayer9 => "Mid-Layer 9",
            Self::MidLayer10 => "Mid-Layer 10",
            Self::MidLayer11 => "Mid-Layer 11",
            Self::MidLayer12 => "Mid-Layer 12",
            Self::MidLayer13 => "Mid-Layer 13",
            Self::MidLayer14 => "Mid-Layer 14",
            Self::MidLayer15 => "Mid-Layer 15",
            Self::MidLayer16 => "Mid-Layer 16",
            Self::MidLayer17 => "Mid-Layer 17",
            Self::MidLayer18 => "Mid-Layer 18",
            Self::MidLayer19 => "Mid-Layer 19",
            Self::MidLayer20 => "Mid-Layer 20",
            Self::MidLayer21 => "Mid-Layer 21",
            Self::MidLayer22 => "Mid-Layer 22",
            Self::MidLayer23 => "Mid-Layer 23",
            Self::MidLayer24 => "Mid-Layer 24",
            Self::MidLayer25 => "Mid-Layer 25",
            Self::MidLayer26 => "Mid-Layer 26",
            Self::MidLayer27 => "Mid-Layer 27",
            Self::MidLayer28 => "Mid-Layer 28",
            Self::MidLayer29 => "Mid-Layer 29",
            Self::MidLayer30 => "Mid-Layer 30",
            Self::BottomLayer => "Bottom Layer",
            Self::MultiLayer => "Multi-Layer",
            Self::TopOverlay => "Top Overlay",
            Self::BottomOverlay => "Bottom Overlay",
            Self::TopSolder => "Top Solder",
            Self::BottomSolder => "Bottom Solder",
            Self::InternalPlane1 => "Internal Plane 1",
            Self::InternalPlane2 => "Internal Plane 2",
            Self::InternalPlane3 => "Internal Plane 3",
            Self::InternalPlane4 => "Internal Plane 4",
            Self::InternalPlane5 => "Internal Plane 5",
            Self::InternalPlane6 => "Internal Plane 6",
            Self::InternalPlane7 => "Internal Plane 7",
            Self::InternalPlane8 => "Internal Plane 8",
            Self::InternalPlane9 => "Internal Plane 9",
            Self::InternalPlane10 => "Internal Plane 10",
            Self::InternalPlane11 => "Internal Plane 11",
            Self::InternalPlane12 => "Internal Plane 12",
            Self::InternalPlane13 => "Internal Plane 13",
            Self::InternalPlane14 => "Internal Plane 14",
            Self::InternalPlane15 => "Internal Plane 15",
            Self::InternalPlane16 => "Internal Plane 16",
            Self::DrillGuide => "Drill Guide",
            Self::DrillDrawing => "Drill Drawing",
            Self::TopPaste => "Top Paste",
            Self::BottomPaste => "Bottom Paste",
            Self::TopAssembly => "Top Assembly",
            Self::BottomAssembly => "Bottom Assembly",
            Self::TopCourtyard => "Top Courtyard",
            Self::BottomCourtyard => "Bottom Courtyard",
            Self::Top3DBody => "Top 3D Body",
            Self::Bottom3DBody => "Bottom 3D Body",
            Self::Mechanical1 => "Mechanical 1",
            Self::Mechanical2 => "Mechanical 2",
            Self::Mechanical3 => "Mechanical 3",
            Self::Mechanical4 => "Mechanical 4",
            Self::Mechanical5 => "Mechanical 5",
            Self::Mechanical6 => "Mechanical 6",
            Self::Mechanical7 => "Mechanical 7",
            Self::Mechanical8 => "Mechanical 8",
            Self::Mechanical9 => "Mechanical 9",
            Self::Mechanical10 => "Mechanical 10",
            Self::Mechanical11 => "Mechanical 11",
            Self::Mechanical12 => "Mechanical 12",
            Self::Mechanical13 => "Mechanical 13",
            Self::Mechanical14 => "Mechanical 14",
            Self::Mechanical15 => "Mechanical 15",
            Self::Mechanical16 => "Mechanical 16",
            Self::ConnectLayer => "Connect Layer",
            Self::BackgroundLayer => "Background Layer",
            Self::DRCErrorLayer => "DRC Error Layer",
            Self::HighlightLayer => "Highlight Layer",
            Self::GridColor1 => "Grid Color 1",
            Self::GridColor10 => "Grid Color 10",
            Self::PadHoleLayer => "Pad Hole Layer",
            Self::ViaHoleLayer => "Via Hole Layer",
            Self::TopPadMaster => "Top Pad Master",
            Self::BottomPadMaster => "Bottom Pad Master",
            Self::DRCDetailLayer => "DRC Detail Layer",
            Self::KeepOut => "Keep-Out Layer",
        }
    }

    /// Parses a layer from its Altium name string.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Top Layer" => Some(Self::TopLayer),
            "Mid-Layer 1" => Some(Self::MidLayer1),
            "Mid-Layer 2" => Some(Self::MidLayer2),
            "Mid-Layer 3" => Some(Self::MidLayer3),
            "Mid-Layer 4" => Some(Self::MidLayer4),
            "Mid-Layer 5" => Some(Self::MidLayer5),
            "Mid-Layer 6" => Some(Self::MidLayer6),
            "Mid-Layer 7" => Some(Self::MidLayer7),
            "Mid-Layer 8" => Some(Self::MidLayer8),
            "Mid-Layer 9" => Some(Self::MidLayer9),
            "Mid-Layer 10" => Some(Self::MidLayer10),
            "Mid-Layer 11" => Some(Self::MidLayer11),
            "Mid-Layer 12" => Some(Self::MidLayer12),
            "Mid-Layer 13" => Some(Self::MidLayer13),
            "Mid-Layer 14" => Some(Self::MidLayer14),
            "Mid-Layer 15" => Some(Self::MidLayer15),
            "Mid-Layer 16" => Some(Self::MidLayer16),
            "Mid-Layer 17" => Some(Self::MidLayer17),
            "Mid-Layer 18" => Some(Self::MidLayer18),
            "Mid-Layer 19" => Some(Self::MidLayer19),
            "Mid-Layer 20" => Some(Self::MidLayer20),
            "Mid-Layer 21" => Some(Self::MidLayer21),
            "Mid-Layer 22" => Some(Self::MidLayer22),
            "Mid-Layer 23" => Some(Self::MidLayer23),
            "Mid-Layer 24" => Some(Self::MidLayer24),
            "Mid-Layer 25" => Some(Self::MidLayer25),
            "Mid-Layer 26" => Some(Self::MidLayer26),
            "Mid-Layer 27" => Some(Self::MidLayer27),
            "Mid-Layer 28" => Some(Self::MidLayer28),
            "Mid-Layer 29" => Some(Self::MidLayer29),
            "Mid-Layer 30" => Some(Self::MidLayer30),
            "Bottom Layer" => Some(Self::BottomLayer),
            "Multi-Layer" => Some(Self::MultiLayer),
            "Top Overlay" => Some(Self::TopOverlay),
            "Bottom Overlay" => Some(Self::BottomOverlay),
            "Top Solder" => Some(Self::TopSolder),
            "Bottom Solder" => Some(Self::BottomSolder),
            "Internal Plane 1" => Some(Self::InternalPlane1),
            "Internal Plane 2" => Some(Self::InternalPlane2),
            "Internal Plane 3" => Some(Self::InternalPlane3),
            "Internal Plane 4" => Some(Self::InternalPlane4),
            "Internal Plane 5" => Some(Self::InternalPlane5),
            "Internal Plane 6" => Some(Self::InternalPlane6),
            "Internal Plane 7" => Some(Self::InternalPlane7),
            "Internal Plane 8" => Some(Self::InternalPlane8),
            "Internal Plane 9" => Some(Self::InternalPlane9),
            "Internal Plane 10" => Some(Self::InternalPlane10),
            "Internal Plane 11" => Some(Self::InternalPlane11),
            "Internal Plane 12" => Some(Self::InternalPlane12),
            "Internal Plane 13" => Some(Self::InternalPlane13),
            "Internal Plane 14" => Some(Self::InternalPlane14),
            "Internal Plane 15" => Some(Self::InternalPlane15),
            "Internal Plane 16" => Some(Self::InternalPlane16),
            "Drill Guide" => Some(Self::DrillGuide),
            "Drill Drawing" => Some(Self::DrillDrawing),
            "Top Paste" => Some(Self::TopPaste),
            "Bottom Paste" => Some(Self::BottomPaste),
            "Top Assembly" => Some(Self::TopAssembly),
            "Bottom Assembly" => Some(Self::BottomAssembly),
            "Top Courtyard" => Some(Self::TopCourtyard),
            "Bottom Courtyard" => Some(Self::BottomCourtyard),
            "Top 3D Body" => Some(Self::Top3DBody),
            "Bottom 3D Body" => Some(Self::Bottom3DBody),
            "Mechanical 1" => Some(Self::Mechanical1),
            "Mechanical 2" => Some(Self::Mechanical2),
            "Mechanical 3" => Some(Self::Mechanical3),
            "Mechanical 4" => Some(Self::Mechanical4),
            "Mechanical 5" => Some(Self::Mechanical5),
            "Mechanical 6" => Some(Self::Mechanical6),
            "Mechanical 7" => Some(Self::Mechanical7),
            "Mechanical 8" => Some(Self::Mechanical8),
            "Mechanical 9" => Some(Self::Mechanical9),
            "Mechanical 10" => Some(Self::Mechanical10),
            "Mechanical 11" => Some(Self::Mechanical11),
            "Mechanical 12" => Some(Self::Mechanical12),
            "Mechanical 13" => Some(Self::Mechanical13),
            "Mechanical 14" => Some(Self::Mechanical14),
            "Mechanical 15" => Some(Self::Mechanical15),
            "Mechanical 16" => Some(Self::Mechanical16),
            "Connect Layer" => Some(Self::ConnectLayer),
            "Background Layer" => Some(Self::BackgroundLayer),
            "DRC Error Layer" => Some(Self::DRCErrorLayer),
            "Highlight Layer" => Some(Self::HighlightLayer),
            "Grid Color 1" => Some(Self::GridColor1),
            "Grid Color 10" => Some(Self::GridColor10),
            "Pad Hole Layer" => Some(Self::PadHoleLayer),
            "Via Hole Layer" => Some(Self::ViaHoleLayer),
            "Top Pad Master" => Some(Self::TopPadMaster),
            "Bottom Pad Master" => Some(Self::BottomPadMaster),
            "DRC Detail Layer" => Some(Self::DRCDetailLayer),
            "Keep-Out Layer" => Some(Self::KeepOut),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_smd_creation() {
        let pad = Pad::smd("1", 0.5, 0.0, 0.9, 0.8);
        assert_eq!(pad.designator, "1");
        assert!((pad.x - 0.5).abs() < f64::EPSILON);
        assert!(pad.hole_size.is_none());
    }

    #[test]
    fn pad_through_hole_creation() {
        let pad = Pad::through_hole("1", 0.0, 0.0, 1.5, 1.5, 0.8);
        assert_eq!(pad.hole_size, Some(0.8));
    }

    #[test]
    fn layer_roundtrip() {
        let layer = Layer::TopOverlay;
        assert_eq!(Layer::parse(layer.as_str()), Some(layer));
    }
}
