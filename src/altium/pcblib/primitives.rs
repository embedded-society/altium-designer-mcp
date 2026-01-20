//! Footprint primitive types for `PcbLib` files.
//!
//! These types represent the geometric elements that make up a footprint:
//! pads, tracks, arcs, regions, and text.

use serde::{Deserialize, Serialize};

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
    /// Top copper layer.
    #[serde(rename = "Top Layer", alias = "TopLayer")]
    TopLayer,
    /// Bottom copper layer.
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
    /// Mechanical layer 1.
    #[serde(rename = "Mechanical 1", alias = "Mechanical1")]
    Mechanical1,
    /// Mechanical layer 2.
    #[serde(rename = "Mechanical 2", alias = "Mechanical2")]
    Mechanical2,
    /// Mechanical layer 13.
    #[serde(rename = "Mechanical 13", alias = "Mechanical13")]
    Mechanical13,
    /// Mechanical layer 15.
    #[serde(rename = "Mechanical 15", alias = "Mechanical15")]
    Mechanical15,

    // Keep-out
    /// Keep-out layer.
    #[serde(rename = "Keep-Out Layer", alias = "KeepOut")]
    KeepOut,
}

impl Layer {
    /// Returns the Altium layer name string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::TopLayer => "Top Layer",
            Self::BottomLayer => "Bottom Layer",
            Self::MultiLayer => "Multi-Layer",
            Self::TopOverlay => "Top Overlay",
            Self::BottomOverlay => "Bottom Overlay",
            Self::TopSolder => "Top Solder",
            Self::BottomSolder => "Bottom Solder",
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
            Self::Mechanical13 => "Mechanical 13",
            Self::Mechanical15 => "Mechanical 15",
            Self::KeepOut => "Keep-Out Layer",
        }
    }

    /// Parses a layer from its Altium name string.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Top Layer" => Some(Self::TopLayer),
            "Bottom Layer" => Some(Self::BottomLayer),
            "Multi-Layer" => Some(Self::MultiLayer),
            "Top Overlay" => Some(Self::TopOverlay),
            "Bottom Overlay" => Some(Self::BottomOverlay),
            "Top Solder" => Some(Self::TopSolder),
            "Bottom Solder" => Some(Self::BottomSolder),
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
            "Mechanical 13" => Some(Self::Mechanical13),
            "Mechanical 15" => Some(Self::Mechanical15),
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
