//! `PcbLib` copper/mechanical layer enum and its Altium layer-ID mapping.

use serde::{Deserialize, Serialize};

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

    // Extended mechanical layers (IDs 186-201, Altium Designer 18+)
    /// Mechanical layer 17 (ID 186).
    #[serde(rename = "Mechanical 17", alias = "Mechanical17")]
    Mechanical17,
    /// Mechanical layer 18 (ID 187).
    #[serde(rename = "Mechanical 18", alias = "Mechanical18")]
    Mechanical18,
    /// Mechanical layer 19 (ID 188).
    #[serde(rename = "Mechanical 19", alias = "Mechanical19")]
    Mechanical19,
    /// Mechanical layer 20 (ID 189).
    #[serde(rename = "Mechanical 20", alias = "Mechanical20")]
    Mechanical20,
    /// Mechanical layer 21 (ID 190).
    #[serde(rename = "Mechanical 21", alias = "Mechanical21")]
    Mechanical21,
    /// Mechanical layer 22 (ID 191).
    #[serde(rename = "Mechanical 22", alias = "Mechanical22")]
    Mechanical22,
    /// Mechanical layer 23 (ID 192).
    #[serde(rename = "Mechanical 23", alias = "Mechanical23")]
    Mechanical23,
    /// Mechanical layer 24 (ID 193).
    #[serde(rename = "Mechanical 24", alias = "Mechanical24")]
    Mechanical24,
    /// Mechanical layer 25 (ID 194).
    #[serde(rename = "Mechanical 25", alias = "Mechanical25")]
    Mechanical25,
    /// Mechanical layer 26 (ID 195).
    #[serde(rename = "Mechanical 26", alias = "Mechanical26")]
    Mechanical26,
    /// Mechanical layer 27 (ID 196).
    #[serde(rename = "Mechanical 27", alias = "Mechanical27")]
    Mechanical27,
    /// Mechanical layer 28 (ID 197).
    #[serde(rename = "Mechanical 28", alias = "Mechanical28")]
    Mechanical28,
    /// Mechanical layer 29 (ID 198).
    #[serde(rename = "Mechanical 29", alias = "Mechanical29")]
    Mechanical29,
    /// Mechanical layer 30 (ID 199).
    #[serde(rename = "Mechanical 30", alias = "Mechanical30")]
    Mechanical30,
    /// Mechanical layer 31 (ID 200).
    #[serde(rename = "Mechanical 31", alias = "Mechanical31")]
    Mechanical31,
    /// Mechanical layer 32 (ID 201).
    #[serde(rename = "Mechanical 32", alias = "Mechanical32")]
    Mechanical32,

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
    #[allow(clippy::too_many_lines)] // Layer name lookup for all layers
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
            Self::Mechanical17 => "Mechanical 17",
            Self::Mechanical18 => "Mechanical 18",
            Self::Mechanical19 => "Mechanical 19",
            Self::Mechanical20 => "Mechanical 20",
            Self::Mechanical21 => "Mechanical 21",
            Self::Mechanical22 => "Mechanical 22",
            Self::Mechanical23 => "Mechanical 23",
            Self::Mechanical24 => "Mechanical 24",
            Self::Mechanical25 => "Mechanical 25",
            Self::Mechanical26 => "Mechanical 26",
            Self::Mechanical27 => "Mechanical 27",
            Self::Mechanical28 => "Mechanical 28",
            Self::Mechanical29 => "Mechanical 29",
            Self::Mechanical30 => "Mechanical 30",
            Self::Mechanical31 => "Mechanical 31",
            Self::Mechanical32 => "Mechanical 32",
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
    #[allow(clippy::too_many_lines)] // Layer name parsing for all layers
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
            "Mechanical 17" => Some(Self::Mechanical17),
            "Mechanical 18" => Some(Self::Mechanical18),
            "Mechanical 19" => Some(Self::Mechanical19),
            "Mechanical 20" => Some(Self::Mechanical20),
            "Mechanical 21" => Some(Self::Mechanical21),
            "Mechanical 22" => Some(Self::Mechanical22),
            "Mechanical 23" => Some(Self::Mechanical23),
            "Mechanical 24" => Some(Self::Mechanical24),
            "Mechanical 25" => Some(Self::Mechanical25),
            "Mechanical 26" => Some(Self::Mechanical26),
            "Mechanical 27" => Some(Self::Mechanical27),
            "Mechanical 28" => Some(Self::Mechanical28),
            "Mechanical 29" => Some(Self::Mechanical29),
            "Mechanical 30" => Some(Self::Mechanical30),
            "Mechanical 31" => Some(Self::Mechanical31),
            "Mechanical 32" => Some(Self::Mechanical32),
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
