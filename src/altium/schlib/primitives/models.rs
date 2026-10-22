//! `SchLib` footprint model references.

#[allow(clippy::wildcard_imports)] // sibling primitive types
use super::*;

/// A footprint model reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootprintModel {
    /// Model name (footprint name in `PcbLib`).
    pub name: String,
    /// Description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Path to the `PcbLib` that contains this footprint, written as
    /// `ModelDatafile0`. When set, Altium resolves the footprint directly from
    /// that file (rendering the preview); when absent it falls back to searching
    /// available libraries by name, which reports "footprint not found" if the
    /// library isn't installed/in the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_path: Option<String>,
    /// Whether this is the current/default footprint model (`IsCurrent=T`).
    /// Preserved on read; on write the first model is still emitted as current
    /// (positional), so this is read-preserved only until multi-model authoring lands.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_current: bool,
    /// The record's `UniqueID`, preserved on read so a read-modify-write
    /// re-emits the same id; a from-scratch model generates a fresh one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
    /// The `RECORD=45` exactly as read (see `raw_params` on every graphic):
    /// a UI-authored link carries `IntegratedModel=T|DatabaseModel=T` and
    /// omits an empty `Description`, all of which come back as stored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_params: Vec<(String, String)>,
    /// Text records owned by this link's `RECORD=46` (its datafile link) —
    /// a `RECORD=47` pin-map definer, say — each as its `(key, value)`
    /// segments in stored order, written back after the 46 with `OwnerIndex`
    /// set to its new stream-index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datafile_link_records: Vec<Vec<(String, String)>>,
    /// Text records owned by this link's `RECORD=48` (its implementation) —
    /// an implementation parameter such as Altium 365's hidden
    /// `Available Preview Images`, say — each as its `(key, value)` segments
    /// in stored order, written back after the 48 with `OwnerIndex` set to
    /// its new stream-index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implementation_records: Vec<Vec<(String, String)>>,
}

impl FootprintModel {
    /// Creates a new footprint model reference.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            raw_params: Vec::new(),
            datafile_link_records: Vec::new(),
            implementation_records: Vec::new(),
            name: name.into(),
            description: String::new(),
            library_path: None,
            is_current: false,
            unique_id: None,
        }
    }
}
