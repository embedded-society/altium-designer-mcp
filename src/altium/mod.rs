//! Altium Designer file format handling.
//!
//! This module provides read/write capabilities for Altium Designer library files:
//!
//! - `.PcbLib` — PCB footprint libraries
//! - `.SchLib` — Schematic symbol libraries
//!
//! # File Format
//!
//! Altium library files are OLE Compound Documents (CFB format) containing:
//!
//! - A `FileHeader` stream with library metadata
//! - One storage per component, each containing:
//!   - `Data` stream with primitives (pads, tracks, arcs, etc.)
//!   - `Parameters` stream with component properties
//!
//! See `docs/PCBLIB_FORMAT.md` and `docs/SCHLIB_FORMAT.md` for detailed format documentation.
//!
//! # Architecture
//!
//! This module provides low-level file I/O. The AI handles:
//! - IPC-7351B calculations
//! - Package layout decisions
//! - Style choices

pub mod error;
pub mod pcblib;
pub mod schlib;

pub use error::{AltiumError, AltiumResult};
pub use pcblib::{Footprint, PcbLib};
pub use schlib::{SchLib, Symbol};

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{BuildHasher, Hash, Hasher};

/// Maximum length for OLE Compound File storage/stream names.
///
/// OLE Compound Document format limits entry names to 31 UTF-16 code units.
/// We enforce 31 bytes for ASCII compatibility.
pub const MAX_OLE_NAME_LEN: usize = 31;

/// Reserve 4 chars for "~XXX" suffix (allows 999 collisions).
const SUFFIX_LEN: usize = 4;

/// Generates a safe OLE storage name for a component.
///
/// OLE Compound File names are limited to 31 characters. This function:
/// - Returns the name as-is if it fits within the limit
/// - Truncates longer names and adds a unique suffix to avoid collisions
///
/// # Arguments
///
/// * `name` - The full component name
/// * `used_names` - Set of OLE names already in use (to avoid collisions)
///
/// # Returns
///
/// A safe OLE name (≤31 chars) that doesn't collide with existing names.
#[must_use]
pub fn generate_ole_name<S: BuildHasher>(name: &str, used_names: &HashSet<String, S>) -> String {
    if name.len() <= MAX_OLE_NAME_LEN && !used_names.contains(name) {
        return name.to_string();
    }

    // Need to truncate - use format: "{prefix}~{suffix}"
    let max_prefix_len = MAX_OLE_NAME_LEN - SUFFIX_LEN;

    // Truncate to max prefix length, respecting char boundaries
    let prefix: String = name.chars().take(max_prefix_len).collect();
    let prefix = if prefix.len() > max_prefix_len {
        // If multi-byte chars, truncate further
        prefix.chars().take(max_prefix_len - 1).collect()
    } else {
        prefix
    };

    // Find a unique suffix
    for i in 1..1000 {
        let candidate = format!("{prefix}~{i:03}");
        if !used_names.contains(&candidate) {
            return candidate;
        }
    }

    // Fallback: use hash-based suffix (extremely unlikely to reach here)
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();
    format!(
        "{}~{:03X}",
        &prefix[..prefix.len().saturating_sub(1)],
        hash & 0xFFF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_name_unchanged() {
        let used = HashSet::new();
        assert_eq!(generate_ole_name("RESISTOR", &used), "RESISTOR");
    }

    #[test]
    fn exactly_31_chars_unchanged() {
        let used = HashSet::new();
        let name = "A".repeat(31);
        assert_eq!(generate_ole_name(&name, &used), name);
    }

    #[test]
    fn long_name_truncated() {
        let used = HashSet::new();
        let name = "VERY_LONG_COMPONENT_NAME_THAT_EXCEEDS_LIMIT";
        let result = generate_ole_name(name, &used);
        assert!(result.len() <= MAX_OLE_NAME_LEN);
        assert!(result.starts_with("VERY_LONG_COMPONENT_NAME_TH"));
        assert!(result.contains('~'));
    }

    #[test]
    fn collision_avoided() {
        let mut used = HashSet::new();
        let name = "VERY_LONG_COMPONENT_NAME_THAT_EXCEEDS_LIMIT";

        let first = generate_ole_name(name, &used);
        used.insert(first.clone());

        let second = generate_ole_name(name, &used);
        assert_ne!(first, second);
        assert!(second.len() <= MAX_OLE_NAME_LEN);
    }

    #[test]
    fn short_name_collision_handled() {
        let mut used = HashSet::new();
        used.insert("RESISTOR".to_string());

        let result = generate_ole_name("RESISTOR", &used);
        assert_ne!(result, "RESISTOR");
        assert!(result.len() <= MAX_OLE_NAME_LEN);
    }
}
