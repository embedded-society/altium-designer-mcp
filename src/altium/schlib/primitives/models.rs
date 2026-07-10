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
}

impl FootprintModel {
    /// Creates a new footprint model reference.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            library_path: None,
            is_current: false,
        }
    }
}
