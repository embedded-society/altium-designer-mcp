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
        }
    }
}
