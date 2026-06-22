//! Shared text/alignment types used by both `PcbLib` and `SchLib`.

use serde::{Deserialize, Serialize};

/// Text justification within a 3x3 anchor grid, combining vertical
/// (Bottom/Middle/Top) and horizontal (Left/Centre/Right) alignment.
///
/// Shared by `SchLib` labels/text and `PcbLib` layer text — the serde
/// representation (`snake_case` strings) is identical across both formats. The
/// *default* differs per format (`SchLib` → `BottomLeft`, `PcbLib` →
/// `MiddleCenter`), so each `justification` field sets its own
/// `#[serde(default = ...)]` rather than relying on this enum's `Default`; the
/// enum-level default below matches `PcbLib` and is what `default()` returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextJustification {
    /// Bottom-left aligned.
    BottomLeft,
    /// Bottom-centre aligned.
    BottomCenter,
    /// Bottom-right aligned.
    BottomRight,
    /// Middle-left aligned.
    MiddleLeft,
    /// Middle-centre aligned.
    #[default]
    MiddleCenter,
    /// Middle-right aligned.
    MiddleRight,
    /// Top-left aligned.
    TopLeft,
    /// Top-centre aligned.
    TopCenter,
    /// Top-right aligned.
    TopRight,
}
