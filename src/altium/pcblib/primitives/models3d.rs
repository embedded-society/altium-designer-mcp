//! `PcbLib` embedded 3D-model types: `Model3D` reference, `EmbeddedModel`, and `ComponentBody`.

#[allow(clippy::wildcard_imports)] // sibling primitive types
use super::*;

/// A 3D model reference (simple version for programmatic creation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model3D {
    /// Path to the STEP file.
    pub filepath: String,
    /// X offset from footprint origin in mm.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub x_offset: f64,
    /// Y offset from footprint origin in mm.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub y_offset: f64,
    /// Z offset from board surface in mm.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub z_offset: f64,
    /// Rotation around Z axis in degrees.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
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
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub rotation_x: f64,

    /// Rotation around Y axis in degrees.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub rotation_y: f64,

    /// Rotation around Z axis in degrees.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub rotation_z: f64,

    /// Z offset (standoff from board) in mm.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub z_offset: f64,

    /// Overall height in mm.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub overall_height: f64,

    /// Standoff height in mm.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub standoff_height: f64,

    /// Layer the body outline is on.
    #[serde(default)]
    pub layer: Layer,

    /// 2D outline of the body in the footprint plane, as `(x, y)` vertices in mm.
    ///
    /// Altium stores a closed polygon giving the body's 2D extent. When this is
    /// empty the writer synthesises a bounding box from the footprint, since
    /// Altium needs a non-degenerate outline to place and render the body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outline: Vec<(f64, f64)>,

    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,

    /// Model integrity checksum (Altium `MODEL.CHECKSUM`). Round-tripped verbatim —
    /// never recomputed, because the checksum is over the raw (uncompressed) model
    /// bytes, which the parameter writer does not have. `0` is valid and the default
    /// for a fresh body, so default output stays byte-identical.
    #[serde(default)]
    pub model_checksum: i64,

    /// Body name (Altium `NAME`). Default is a single space `" "`, reproducing the
    /// `NAME= ` literal that template-default bodies emit (byte-identity).
    #[serde(default = "default_body_name")]
    pub name: String,

    /// Body kind (Altium `KIND`). Default `0`.
    #[serde(default)]
    pub kind: u8,

    /// Sub-polygon index (Altium `SUBPOLYINDEX`). Default `-1`.
    #[serde(default = "default_sub_poly_index")]
    pub sub_poly_index: i32,

    /// Union index (Altium `UNIONINDEX`). Default `0`.
    #[serde(default)]
    pub union_index: u32,

    /// Whether the body is shape-based (Altium `ISSHAPEBASED`). Default `false`.
    #[serde(default)]
    pub is_shape_based: bool,

    /// Body projection mode (Altium `BODYPROJECTION`). Default `0`.
    #[serde(default)]
    pub body_projection: u8,

    /// 3D body colour, decimal RGB (Altium `BODYCOLOR3D`). Default `8421504`
    /// (`0xE0E0E0`, `AltiumSharp`'s default grey).
    #[serde(default = "default_body_color")]
    pub body_color_3d: u32,

    /// 3D body opacity, `0.0`–`1.0` (Altium `BODYOPACITY3D`). Default `1.0`,
    /// written with `{:.3}` so the default renders as `1.000` (byte-identity).
    #[serde(default = "default_opacity")]
    pub body_opacity_3d: f64,

    /// 2D rotation in degrees (Altium `MODEL.2D.ROTATION`). Default `0.0`,
    /// written with `{:.3}` so the default renders as `0.000` (byte-identity).
    #[serde(default)]
    pub model_2d_rotation: f64,

    /// Net index into the board's net list — common-header u16 @3. `0xFFFF`
    /// (65535) means "no net", the from-scratch default (round-trip fidelity).
    #[serde(default = "default_net_index")]
    pub net_index: u16,

    /// Polygon index this body belongs to — common-header u16 @5. `0xFFFF`
    /// (none) from scratch, matching the historical writer output.
    #[serde(default = "default_polygon_index")]
    pub polygon_index: u16,

    /// Component index into the board's component list — common-header u16 @7
    /// (`0xFFFF` stored, exposed as `-1`). `-1` (free primitive) from scratch.
    #[serde(default = "default_component_index")]
    pub component_index: i32,

    /// Unmodelled parameter keys read verbatim from the body's `KEY=VALUE|...`
    /// block, in read order. An Altium body carries keys the typed model does not
    /// recognise (e.g. `TEXTURE`, `TEXTURECENTERX`, `MODEL.2D.X`, `MODEL.2D.Y`,
    /// `IDENTIFIER`, `MODEL.MODELTYPE`, `MODEL.MODELSOURCE`, `MODEL.EXTRUDED.MINZ`,
    /// `CAVITYHEIGHT`, and the repeated `ARCRESOLUTION`). Capturing them here and
    /// re-emitting them on write keeps a read-modify-write from silently dropping
    /// keys we don't model. Empty from scratch, so the writer appends nothing and
    /// the output stays byte-identical to the canonical form.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_parameters: Vec<(String, String)>,
}

const fn default_body_color() -> u32 {
    8_421_504
}

/// Default net index for a from-scratch body (`0xFFFF` = no net). The
/// common-header connectivity indices default to "none" so a free library
/// body writes the same `0xFF` header bytes as before (byte-identity).
const fn default_net_index() -> u16 {
    0xFFFF
}

/// Default polygon index for a from-scratch body (`0xFFFF` = none).
const fn default_polygon_index() -> u16 {
    0xFFFF
}

/// Default component index for a from-scratch body (`-1` = free primitive,
/// stored as the `0xFFFF` common-header sentinel).
const fn default_component_index() -> i32 {
    -1
}

const fn default_opacity() -> f64 {
    1.0
}

const fn default_sub_poly_index() -> i32 {
    -1
}

fn default_body_name() -> String {
    " ".to_string()
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
            outline: Vec::new(),
            unique_id: None,
            model_checksum: 0,
            name: default_body_name(),
            kind: 0,
            sub_poly_index: default_sub_poly_index(),
            union_index: 0,
            is_shape_based: false,
            body_projection: 0,
            body_color_3d: default_body_color(),
            body_opacity_3d: default_opacity(),
            model_2d_rotation: 0.0,
            net_index: default_net_index(),
            polygon_index: default_polygon_index(),
            component_index: default_component_index(),
            additional_parameters: Vec::new(),
        }
    }
}
