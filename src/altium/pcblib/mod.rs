//! `PcbLib` file format handling.
//!
//! This module handles reading and writing Altium `.PcbLib` footprint library files.
//!
//! # File Structure
//!
//! A `.PcbLib` file is an OLE Compound Document containing:
//!
//! ```text
//! Root/
//! ├── FileHeader           # Library metadata (ASCII key=value pairs)
//! ├── ComponentName1/      # Storage for first footprint
//! │   ├── Data             # Primitives in binary format
//! │   └── Parameters       # Component parameters (ASCII)
//! ├── ComponentName2/      # Storage for second footprint
//! │   ├── Data
//! │   └── Parameters
//! └── ...
//! ```
//!
//! # Data Stream Binary Format
//!
//! The Data stream contains primitives in binary format:
//!
//! ```text
//! [name_block_len:4][str_len:1][name:str_len]  // Component name
//! [record_type:1][blocks...]                   // First primitive
//! [record_type:1][blocks...]                   // Second primitive
//! ...                                          // exactly the primitive count from the component header
//! ```
//!
//! There is NO trailing end marker: the writer must never emit a final `0x00`.
//! Altium reads exactly the primitive count from the component header, and a
//! stray `0x00` is mis-parsed as a zero-length record (see issue #68).
//!
//! Record types: Arc(1), Pad(2), Via(3), Track(4), Text(5), Fill(6), Region(11), ComponentBody(12)

mod flags;
pub mod primitives;
mod read_io;
mod reader;
mod units;
mod write_io;
mod writer;

use serde::{Deserialize, Serialize};

pub use primitives::{
    Arc, ComponentBody, DrillLayerPairType, EmbeddedModel, Fill, HoleShape, Layer,
    MaskExpansionMode, Model3D, Pad, PadPolygonConnect, PadShape, PadStackMode, PcbFlags,
    PowerPlaneConnectStyle, Region, RegionKind, StrokeFont, Text, TextJustification, TextKind,
    Track, Vertex, Via, ViaStackMode,
};

use crate::altium::error::{AltiumError, AltiumResult};

/// Internal OLE storage entries that should be filtered out when reading `PcbLib` files.
/// These are not actual footprints, but internal Altium data structures.
const INTERNAL_OLE_ENTRIES: &[&str] = &[
    "FileHeader",
    "Library",
    "Models",
    "Textures",
    "ModelsNoEmbed",
    "PadViaLibrary",
    "LayerKindMapping",
    "ComponentParamsTOC",
    "FileVersionInfo",
    "PrimitiveGuids",
    "UniqueIDPrimitiveInformation",
];

/// A complete PCB footprint.
///
/// # Example
///
/// ```
/// use altium_designer_mcp::altium::pcblib::{Footprint, Pad, Track, Layer};
///
/// let mut footprint = Footprint::new("RESC1608X55N");
/// footprint.description = "Chip Resistor 1608 (0603)".to_string();
///
/// // Add SMD pads
/// footprint.add_pad(Pad::smd("1", -0.75, 0.0, 0.85, 0.95));
/// footprint.add_pad(Pad::smd("2", 0.75, 0.0, 0.85, 0.95));
///
/// // Add silkscreen outline
/// footprint.add_track(Track::new(-0.35, 0.5, 0.35, 0.5, 0.15, Layer::TopOverlay));
/// footprint.add_track(Track::new(-0.35, -0.5, 0.35, -0.5, 0.15, Layer::TopOverlay));
///
/// assert_eq!(footprint.pads.len(), 2);
/// assert_eq!(footprint.tracks.len(), 2);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Footprint {
    /// Footprint name (e.g., "RESC1608X55N").
    pub name: String,

    /// Description of the footprint.
    #[serde(default)]
    pub description: String,

    /// Overall component height in mm — Altium's `HEIGHT` parameter, written
    /// as its mil string (`39.37mil` for 1 mm). `0.0` when unset, which is
    /// what a script-authored footprint carries.
    #[serde(default)]
    pub height: f64,

    /// Pads in the footprint.
    #[serde(default)]
    pub pads: Vec<Pad>,

    /// Vias in the footprint.
    #[serde(default)]
    pub vias: Vec<Via>,

    /// Tracks (lines) in the footprint.
    #[serde(default)]
    pub tracks: Vec<Track>,

    /// Arcs in the footprint.
    #[serde(default)]
    pub arcs: Vec<Arc>,

    /// Filled regions in the footprint.
    #[serde(default)]
    pub regions: Vec<Region>,

    /// Text items in the footprint.
    #[serde(default)]
    pub text: Vec<Text>,

    /// Filled rectangles in the footprint.
    #[serde(default)]
    pub fills: Vec<primitives::Fill>,

    /// 3D component bodies (embedded models).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub component_bodies: Vec<primitives::ComponentBody>,

    /// 3D model reference (legacy, use `component_bodies` for new code).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_3d: Option<Model3D>,

    /// Altium's stable identity for the footprint record itself — the
    /// `PrimitiveGuids` stream's kind-85 entry. Each primitive's own identity
    /// rides on the primitive (`guid` on all eight primitive structs), so a
    /// structural edit moves identities with their primitives; this field
    /// carries the one identity that names no primitive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,

    /// The CFB storage the footprint was read from, kept so a rewrite stores
    /// it under the same name. Altium resolves a footprint by re-deriving its
    /// storage name from the real name (`*` and `/` become `_`, 31-unit cut)
    /// or, for a long name, through `SectionKeys`; a storage that moved is a
    /// footprint Altium cannot load (#507). `None` for a footprint built from
    /// scratch, renamed or copied: its storage name is derived on save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_name: Option<String>,

    /// The `Parameters` keys read verbatim, in read order, other than `HEIGHT`:
    /// the `PATTERN` and `DESCRIPTION` bytes Altium wrote, the `UNICODE`
    /// twins that carry a name or description outside ASCII as UTF-16 code
    /// units, the item and revision GUIDs of a managed footprint, the `AREA` a
    /// UI-authored one carries, and whatever else an Altium version writes
    /// there. The writer re-emits them so a read-modify-write drops nothing,
    /// except that a `PATTERN`, `DESCRIPTION` or twin that no longer describes
    /// the footprint's name or description is rebuilt from the field. Empty
    /// when the block was exactly the five-key block the writer produces from
    /// scratch — which is what `read_pcblib` reports for a library this crate
    /// wrote — and empty from scratch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_parameters: Vec<(String, String)>,

    /// Every `Parameters` key in the exact order read, replayed on write so
    /// the block stays byte-identical: `HEIGHT` from its field, the rest from
    /// `additional_parameters`. Empty when `additional_parameters` is empty:
    /// canonical order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_key_order: Vec<String>,

    /// The order the primitives are stored in, one entry per primitive.
    ///
    /// Altium interleaves kinds in authoring order — `LOCKFLAGS_PCB` in the
    /// golden runs pad x6, track, pad x2, track, arc x2, fill x2 — and both
    /// identity streams key off a primitive's position in that sequence:
    /// `PrimitiveGuids` stores it as a record's second `u32`, and
    /// `UniqueIDPrimitiveInformation` as `PRIMITIVEINDEX`. Emitting the kinds
    /// in blocks would renumber every primitive and detach both.
    ///
    /// An entry names one of the lists above; its n-th occurrence refers to
    /// that list's n-th element, so the sequence alone reconstructs the
    /// interleaving. It is maintained by the `add_*` methods, which is how
    /// reading a footprint records the file's order. Empty when the primitive
    /// lists were populated directly, in which case the writer falls back to
    /// [`PrimitiveKind::WRITE_ORDER`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primitive_order: Vec<PrimitiveKind>,
}

/// How many primitives of each kind [`Footprint::move_layer`] moved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayerMove {
    /// Tracks moved.
    pub tracks: usize,
    /// Arcs moved.
    pub arcs: usize,
    /// Text moved.
    pub text: usize,
    /// Fills moved.
    pub fills: usize,
    /// Regions moved.
    pub regions: usize,
    /// Pads moved.
    pub pads: usize,
    /// Component bodies moved.
    pub component_bodies: usize,
}

impl LayerMove {
    /// Every primitive moved, all kinds together.
    #[must_use]
    pub const fn total(self) -> usize {
        self.tracks
            + self.arcs
            + self.text
            + self.fills
            + self.regions
            + self.pads
            + self.component_bodies
    }
}

primitive_kinds! {
    /// One of a [`Footprint`]'s primitive lists, as named by `primitive_order`.
    PrimitiveKind {
        /// [`Footprint::arcs`].
        Arc => "arc",
        /// [`Footprint::pads`].
        Pad => "pad",
        /// [`Footprint::vias`].
        Via => "via",
        /// [`Footprint::tracks`].
        Track => "track",
        /// [`Footprint::text`].
        Text => "text",
        /// [`Footprint::regions`].
        Region => "region",
        /// [`Footprint::fills`].
        Fill => "fill",
        /// [`Footprint::component_bodies`].
        ComponentBody => "component_body",
    }
}

impl PrimitiveKind {
    /// Altium's numeric object id for this kind, as a `PrimitiveGuids`
    /// record stores it.
    #[must_use]
    pub const fn altium_object_id(self) -> u32 {
        match self {
            Self::Arc => 1,
            Self::Pad => 2,
            Self::Via => 3,
            Self::Track => 4,
            Self::Text => 5,
            Self::Fill => 6,
            Self::Region => 89,
            Self::ComponentBody => 90,
        }
    }

    /// The kind for one of Altium's numeric object ids, or `None` for an id
    /// that names no library primitive (85 is the footprint record itself).
    #[must_use]
    pub const fn from_altium_object_id(id: u32) -> Option<Self> {
        match id {
            1 => Some(Self::Arc),
            2 => Some(Self::Pad),
            3 => Some(Self::Via),
            4 => Some(Self::Track),
            5 => Some(Self::Text),
            6 => Some(Self::Fill),
            89 => Some(Self::Region),
            90 => Some(Self::ComponentBody),
            _ => None,
        }
    }

    /// The `PRIMITIVEOBJECTID` token Altium writes for this kind in a
    /// `UniqueIDPrimitiveInformation` record.
    #[must_use]
    pub const fn object_id(self) -> &'static str {
        match self {
            Self::Arc => "Arc",
            Self::Pad => "Pad",
            Self::Via => "Via",
            Self::Track => "Track",
            Self::Text => "Text",
            Self::Region => "Region",
            Self::Fill => "Fill",
            Self::ComponentBody => "ComponentBody",
        }
    }
}

/// One `PrimitiveGuids` record as it sits in the stream.
///
/// Which primitive it names, and its GUID. Parse/emit carrier only — the
/// identities themselves live on the primitives (`guid`) and on
/// [`Footprint::guid`].
///
/// The stream is a `u32` count followed by that many 24-byte records of
/// `[object_kind: u32][ordinal: u32][guid: 16 bytes, little-endian]`.
/// `object_kind` is Altium's object id — 1 arc, 2 pad, 3 via, 4 track, 5 text,
/// 6 fill, 85 the footprint itself, 89 region, 90 component body.
///
/// `index` is the primitive's position among **all** the footprint's
/// primitives, not among its own kind: across the 22 golden footprints the
/// indices are a permutation of `0..n-1` once the footprint's own record (kind
/// 85, always index 0) is set aside, and `PRIMPROPS` puts its regions at 0, 4,
/// 5 and 6 with a pad at 1 and its texts at 2 and 3. It is the same ordinal
/// `UniqueIDPrimitiveInformation` calls `PRIMITIVEINDEX`, which is why
/// [`Footprint::primitive_order`] has to survive a write for either to mean
/// anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimitiveGuid {
    /// Altium object id of the primitive this GUID belongs to.
    pub object_kind: u32,
    /// Position of the primitive among all of the footprint's primitives.
    pub index: u32,
    /// The GUID, formatted `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
    pub guid: String,
}

/// Most overlapping pad pairs a tool reports before collapsing to a summary.
///
/// Overlaps are quadratic in pad count, so a systematic error on a large BGA
/// would otherwise bury the response in tens of thousands of entries. Shared by
/// `write_pcblib` and `validate_library` so both truncate alike.
pub const MAX_REPORTED_PAD_OVERLAPS: usize = 20;

impl Footprint {
    /// Finds pairs of pads whose copper overlaps on a shared layer.
    ///
    /// Overlapping copper merges into a single net, so a footprint can pass every
    /// integrity check while shorting pins together — the classic cause being an
    /// exposed thermal pad sized against a mis-read land dimension, which welds
    /// the EP to every perimeter pad.
    ///
    /// Returns `(index_a, index_b, overlap_x, overlap_y)` in mm with `a < b`, where
    /// the overlaps are the intersection rectangle's width and height.
    ///
    /// Deliberate constructions are excluded:
    /// - pads sharing a designator (same designator == same net in Altium, so
    ///   stacking them is a normal way to build a compound land),
    /// - pads with no layer in common (a Top-only and a Bottom-only pad may sit
    ///   at identical coordinates; `MultiLayer` shares both).
    ///
    /// Non-rectangular and rotated pads are compared by their axis-aligned
    /// bounding box, so the test errs toward reporting: a false positive costs a
    /// glance, a missed short costs a board. Pads that merely touch (zero gap)
    /// are reported, since Altium merges those too.
    #[must_use]
    pub fn overlapping_pad_pairs(&self) -> Vec<(usize, usize, f64, f64)> {
        /// Zero-gap tolerance. Altium stores coordinates in 2.54 nm units, so
        /// anything at or below this is contact rather than clearance.
        const TOUCH_TOL: f64 = 1e-6;

        fn spans(pad: &Pad) -> (f64, f64) {
            // Rotated pads use the AABB of the rotated rectangle.
            if pad.rotation.abs() < f64::EPSILON {
                return (pad.width, pad.height);
            }
            let radians = pad.rotation.to_radians();
            let (cos, sin) = (radians.cos().abs(), radians.sin().abs());
            (
                pad.width.mul_add(cos, pad.height * sin),
                pad.width.mul_add(sin, pad.height * cos),
            )
        }
        fn shares_layer(a: &Pad, b: &Pad) -> bool {
            let sides = |p: &Pad| match p.layer {
                Layer::TopLayer => (true, false),
                Layer::BottomLayer => (false, true),
                Layer::MultiLayer => (true, true),
                _ => (false, false), // non-copper pads cannot short
            };
            let (at, ab) = sides(a);
            let (bt, bb) = sides(b);
            (at && bt) || (ab && bb)
        }

        let mut hits = Vec::new();
        for (i, a) in self.pads.iter().enumerate() {
            for (j, b) in self.pads.iter().enumerate().skip(i + 1) {
                if a.designator == b.designator || !shares_layer(a, b) {
                    continue;
                }
                let (aw, ah) = spans(a);
                let (bw, bh) = spans(b);
                // Intersection extent, not penetration depth: when one pad sits
                // wholly inside the other's span on an axis, the overlap there is
                // the smaller pad's size, which is what a reader expects to see.
                let ox = (f64::midpoint(aw, bw) - (a.x - b.x).abs()).min(aw).min(bw);
                let oy = (f64::midpoint(ah, bh) - (a.y - b.y).abs()).min(ah).min(bh);
                if ox >= -TOUCH_TOL && oy >= -TOUCH_TOL {
                    hits.push((i, j, ox.max(0.0), oy.max(0.0)));
                }
            }
        }
        hits
    }

    /// Creates a new empty footprint with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            height: 0.0,
            pads: Vec::new(),
            vias: Vec::new(),
            tracks: Vec::new(),
            arcs: Vec::new(),
            regions: Vec::new(),
            text: Vec::new(),
            fills: Vec::new(),
            component_bodies: Vec::new(),
            model_3d: None,
            guid: None,
            storage_name: None,
            additional_parameters: Vec::new(),
            param_key_order: Vec::new(),
            primitive_order: Vec::new(),
        }
    }

    /// Adds a pad to the footprint.
    pub fn add_pad(&mut self, pad: Pad) {
        self.pads.push(pad);
        self.primitive_order.push(PrimitiveKind::Pad);
    }

    /// Adds a via to the footprint.
    pub fn add_via(&mut self, via: Via) {
        self.vias.push(via);
        self.primitive_order.push(PrimitiveKind::Via);
    }

    /// Adds a track to the footprint.
    pub fn add_track(&mut self, track: Track) {
        self.tracks.push(track);
        self.primitive_order.push(PrimitiveKind::Track);
    }

    /// Adds an arc to the footprint.
    pub fn add_arc(&mut self, arc: Arc) {
        self.arcs.push(arc);
        self.primitive_order.push(PrimitiveKind::Arc);
    }

    /// Adds a region to the footprint.
    pub fn add_region(&mut self, region: Region) {
        self.regions.push(region);
        self.primitive_order.push(PrimitiveKind::Region);
    }

    /// Adds text to the footprint.
    pub fn add_text(&mut self, text: Text) {
        self.text.push(text);
        self.primitive_order.push(PrimitiveKind::Text);
    }

    /// Adds a fill to the footprint.
    pub fn add_fill(&mut self, fill: primitives::Fill) {
        self.fills.push(fill);
        self.primitive_order.push(PrimitiveKind::Fill);
    }

    /// Adds a component body (3D model) to the footprint.
    pub fn add_component_body(&mut self, body: primitives::ComponentBody) {
        self.component_bodies.push(body);
        self.primitive_order.push(PrimitiveKind::ComponentBody);
    }

    /// Moves every primitive on `from` to `to` — tracks, arcs, text, fills,
    /// regions, pads and component bodies, every kind that sits on a layer —
    /// and returns how many of each moved. A carrier that named the old
    /// layer (a header byte kept as `raw_layer_id`, a region's or body's
    /// `V7_LAYER` token) is dropped so the writer derives it from the new
    /// layer instead of replaying a stale one. Vias span layers and are not
    /// on one; they are untouched.
    pub fn move_layer(&mut self, from: Layer, to: Layer) -> LayerMove {
        let mut moved = LayerMove::default();
        for track in &mut self.tracks {
            if track.layer == from {
                track.layer = to;
                track.raw_layer_id = None;
                moved.tracks += 1;
            }
        }
        for arc in &mut self.arcs {
            if arc.layer == from {
                arc.layer = to;
                arc.raw_layer_id = None;
                moved.arcs += 1;
            }
        }
        for text in &mut self.text {
            if text.layer == from {
                text.layer = to;
                text.raw_layer_id = None;
                moved.text += 1;
            }
        }
        for fill in &mut self.fills {
            if fill.layer == from {
                fill.layer = to;
                fill.raw_layer_id = None;
                moved.fills += 1;
            }
        }
        for region in &mut self.regions {
            if region.layer == from {
                region.layer = to;
                region.v7_layer = None;
                moved.regions += 1;
            }
        }
        for pad in &mut self.pads {
            if pad.layer == from {
                pad.layer = to;
                pad.raw_layer_id = None;
                moved.pads += 1;
            }
        }
        for body in &mut self.component_bodies {
            if body.layer == from {
                body.layer = to;
                body.raw_layer_id = None;
                body.v7_layer = None;
                moved.component_bodies += 1;
            }
        }
        moved
    }

    /// Keeps the component bodies `keep` accepts and drops the rest together
    /// with their slots in [`Self::primitive_order`], so every other primitive
    /// keeps its place in the file; a bare `retain` on the list leaves the
    /// order one slot long per body and the later bodies each move up one.
    /// Returns how many were dropped.
    pub fn retain_component_bodies(
        &mut self,
        mut keep: impl FnMut(&primitives::ComponentBody) -> bool,
    ) -> usize {
        let dropped: Vec<usize> = self
            .component_bodies
            .iter()
            .enumerate()
            .filter(|(_, body)| !keep(body))
            .map(|(index, _)| index)
            .collect();
        for &index in dropped.iter().rev() {
            self.component_bodies.remove(index);
            self.forget_order_slot(PrimitiveKind::ComponentBody, index);
        }
        dropped.len()
    }

    /// Removes the `index`-th slot of `kind` from [`Self::primitive_order`],
    /// if the order records one.
    fn forget_order_slot(&mut self, kind: PrimitiveKind, index: usize) {
        let slot = self
            .primitive_order
            .iter()
            .enumerate()
            .filter(|(_, k)| **k == kind)
            .nth(index)
            .map(|(position, _)| position);
        if let Some(position) = slot {
            self.primitive_order.remove(position);
        }
    }

    /// The strings the writer never places in a pipe-delimited record: a
    /// pad's designator and a text's string, font names and barcode font
    /// live in length-prefixed binary fields, a body's identifier is stored
    /// as code points, and the flag names carry a `|` of their own.
    pub const RECORD_TEXT_EXEMPT: &'static [&'static str] = &[
        "flags",
        "pads[].designator",
        "text[].text",
        "text[].font_name",
        "text[].barcode_font_name",
        "component_bodies[].identifier",
    ];

    /// Refuses text the record format cannot hold: a `|` in any string the
    /// writer would place between the separators of a pipe-delimited record
    /// (see [`Self::RECORD_TEXT_EXEMPT`] for the strings it never does).
    /// Altium's own PCB editor writes such a `|` raw and then reads the text
    /// back cut at it, so the text is refused rather than written to be lost.
    ///
    /// # Errors
    ///
    /// A message naming this footprint and the offending field's path.
    pub fn check_record_text(&self) -> Result<(), String> {
        crate::altium::record_separator_path(self, Self::RECORD_TEXT_EXEMPT).map_or(
            Ok(()),
            |path| {
                Err(format!(
                    "Footprint '{}' {path} contains '|', the separator of Altium's record format, \
                 which cannot hold it (Altium's own PCB editor writes it raw and then reads the \
                 text back cut at the '|')",
                    self.name
                ))
            },
        )
    }

    /// The footprint's primitives in the order they are written, as
    /// `(kind, index into that kind's list)` pairs.
    ///
    /// [`Self::primitive_order`] is advisory: the primitive lists are public,
    /// so a caller can push to or truncate one without it. Entries pointing
    /// past the end of their list are therefore dropped, and any primitive the
    /// sequence never reaches is appended in [`PrimitiveKind::WRITE_ORDER`] —
    /// so a footprint with no recorded order, or one edited behind its back,
    /// still writes every primitive exactly once.
    #[must_use]
    pub fn write_sequence(&self) -> Vec<(PrimitiveKind, usize)> {
        let mut taken: std::collections::HashMap<PrimitiveKind, usize> =
            std::collections::HashMap::new();
        let mut sequence = Vec::with_capacity(self.primitive_count());

        for &kind in &self.primitive_order {
            let next = taken.entry(kind).or_insert(0);
            if *next < self.count_of(kind) {
                sequence.push((kind, *next));
                *next += 1;
            }
        }
        for kind in PrimitiveKind::WRITE_ORDER {
            let next = taken.entry(kind).or_insert(0);
            while *next < self.count_of(kind) {
                sequence.push((kind, *next));
                *next += 1;
            }
        }
        sequence
    }

    /// Gives the footprint the identity of a brand-new component.
    ///
    /// A clone that will live beside its source must not share its
    /// identities: the kind-85 footprint GUID, every primitive's GUID and
    /// unique id, and the two per-pad identity GUIDs are cleared, so the
    /// writer omits the identity streams and mints fresh pad GUIDs exactly as
    /// for a footprint built from scratch; a replayed via block's identity
    /// slots are zeroed to the nil GUIDs Altium itself writes. Geometry and
    /// the replayed binary bases are untouched.
    pub fn reset_identities(&mut self) {
        self.guid = None;
        for pad in &mut self.pads {
            pad.guid = None;
            pad.unique_id = None;
            pad.identity_guid = None;
            pad.identity_guid_b = None;
        }
        for via in &mut self.vias {
            via.guid = None;
            via.unique_id = None;
            if let Some(block) = via.raw_block.as_mut() {
                if block.len() >= 291 {
                    block[259..291].fill(0);
                }
            }
        }
        for track in &mut self.tracks {
            track.guid = None;
            track.unique_id = None;
        }
        for arc in &mut self.arcs {
            arc.guid = None;
            arc.unique_id = None;
        }
        for region in &mut self.regions {
            region.guid = None;
            region.unique_id = None;
        }
        for text in &mut self.text {
            text.guid = None;
            text.unique_id = None;
        }
        for fill in &mut self.fills {
            fill.guid = None;
            fill.unique_id = None;
        }
        for body in &mut self.component_bodies {
            body.guid = None;
            body.unique_id = None;
        }
    }

    /// How many primitives of one kind the footprint holds.
    #[must_use]
    pub const fn count_of(&self, kind: PrimitiveKind) -> usize {
        match kind {
            PrimitiveKind::Arc => self.arcs.len(),
            PrimitiveKind::Pad => self.pads.len(),
            PrimitiveKind::Via => self.vias.len(),
            PrimitiveKind::Track => self.tracks.len(),
            PrimitiveKind::Text => self.text.len(),
            PrimitiveKind::Region => self.regions.len(),
            PrimitiveKind::Fill => self.fills.len(),
            PrimitiveKind::ComponentBody => self.component_bodies.len(),
        }
    }

    /// The layer each primitive of `kind` sits on, one entry per primitive.
    /// A via spans the stack rather than sitting on one layer and counts as
    /// `MultiLayer`, as Altium lists it.
    #[must_use]
    pub fn layers_of(&self, kind: PrimitiveKind) -> Vec<Layer> {
        match kind {
            PrimitiveKind::Arc => self.arcs.iter().map(|arc| arc.layer).collect(),
            PrimitiveKind::Pad => self.pads.iter().map(|pad| pad.layer).collect(),
            PrimitiveKind::Via => vec![Layer::MultiLayer; self.vias.len()],
            PrimitiveKind::Track => self.tracks.iter().map(|track| track.layer).collect(),
            PrimitiveKind::Text => self.text.iter().map(|text| text.layer).collect(),
            PrimitiveKind::Region => self.regions.iter().map(|region| region.layer).collect(),
            PrimitiveKind::Fill => self.fills.iter().map(|fill| fill.layer).collect(),
            PrimitiveKind::ComponentBody => self
                .component_bodies
                .iter()
                .map(|body| body.layer)
                .collect(),
        }
    }

    /// How many primitives the footprint holds in total.
    #[must_use]
    pub const fn primitive_count(&self) -> usize {
        self.pads.len()
            + self.vias.len()
            + self.tracks.len()
            + self.arcs.len()
            + self.regions.len()
            + self.text.len()
            + self.fills.len()
            + self.component_bodies.len()
    }
}

/// Library metadata parsed from the `FileHeader` stream.
///
/// The `FileHeader` contains metadata about the library as a whole,
/// including component names and descriptions indexed by position.
#[derive(Debug, Clone, Default)]
pub struct LibraryMetadata {
    /// File type identifier (e.g., "Protel for Windows - PCB Library").
    pub header: String,

    /// Component count from `CompCount` field.
    pub component_count: usize,

    /// Component names by index from `LibRef{N}` fields.
    ///
    /// Note: These may not match the footprint names stored in each
    /// component's Parameters stream (PATTERN field), which can be longer
    /// than the 31-character OLE storage name limit.
    pub component_names: Vec<String>,

    /// Component descriptions by index from `CompDescr{N}` fields.
    pub component_descriptions: Vec<String>,

    /// The library's own 8-character `UniqueId` from the `FileHeader`, kept
    /// for the library's lifetime as Altium keeps it; a library built from
    /// scratch is given one on its first save.
    pub unique_id: Option<String>,

    /// The `PADVIALIBRARY.LIBRARYID` of `/Library/PadViaLibrary`, likewise
    /// kept across saves rather than minted afresh each time.
    pub pad_via_library_id: Option<String>,

    /// The `/Library/Data` parameter block exactly as it was read, without its
    /// length prefix or trailing null.
    ///
    /// This block is the library's own board configuration: the V9 layer stack,
    /// every mechanical layer's name, kind and enabled flag, the layer sets and
    /// the view state. Almost none of it is modelled here, and a library
    /// routinely carries names a designer chose (`Mechanical 15 = Assembly
    /// Top`), so it is carried through byte-for-byte on a read-modify-write
    /// rather than replaced by the template stack a from-scratch library gets.
    ///
    /// `None` for a library built in memory, which has no stack to preserve.
    pub library_params: Option<Vec<u8>>,

    /// The Windows ANSI code page the library was authored in, detected on
    /// read from a footprint whose real name stands beside its `PATTERN` bytes
    /// (`crate::altium::detect_ansi_code_page`). The library's ANSI-only text
    /// is read through it and a rewrite writes through it again, so a GBK or
    /// Windows-1250 library reads as its real text and stays byte-identical.
    /// `None` without evidence, and for a library built in memory: the
    /// server's code page then applies.
    pub ansi_code_page: Option<u32>,
}

/// A `PcbLib` footprint library.
///
/// # Example
///
/// ```no_run
/// use altium_designer_mcp::altium::pcblib::{PcbLib, Footprint, Pad};
///
/// // Create a new library and add footprints
/// let mut lib = PcbLib::new();
///
/// let mut footprint = Footprint::new("RESC1608X55N");
/// footprint.add_pad(Pad::smd("1", -0.75, 0.0, 0.85, 0.95));
/// footprint.add_pad(Pad::smd("2", 0.75, 0.0, 0.85, 0.95));
/// lib.add(footprint);
///
/// // Save to file
/// lib.save("MyLibrary.PcbLib").unwrap();
///
/// // Open an existing library
/// let lib = PcbLib::open("MyLibrary.PcbLib").unwrap();
/// for name in lib.names() {
///     println!("Footprint: {name}");
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct PcbLib {
    /// Library file path (if loaded from file).
    filepath: Option<String>,

    /// Footprints in the library.
    footprints: Vec<Footprint>,

    /// Embedded 3D models from `/Library/Models/` storage.
    ///
    /// These are zlib-compressed STEP files that are referenced by
    /// `ComponentBody` records via their GUID.
    models: Vec<EmbeddedModel>,

    /// Library metadata from the `FileHeader` stream.
    metadata: LibraryMetadata,
}

impl PcbLib {
    /// Creates a new empty library.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a `PcbLib` from a file path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or is not a valid `PcbLib`.
    pub fn open(path: impl AsRef<std::path::Path>) -> AltiumResult<Self> {
        let path = path.as_ref();
        // The whole file is read into memory first: a compound-file reader
        // seeks through its sector chains constantly, and each seek against
        // an unbuffered file is a system call — several times the cost of
        // parsing the same bytes from memory.
        let bytes = std::fs::read(path).map_err(|e| AltiumError::file_read(path, e))?;

        let mut lib = Self::read(std::io::Cursor::new(bytes))?;
        lib.filepath = Some(path.display().to_string());
        Ok(lib)
    }

    /// Saves the library to a file.
    ///
    /// Uses atomic write: writes to a temporary file first, then renames on success.
    /// This prevents data loss if the write fails partway through. The library
    /// then belongs to `path`: the `FILENAME` Altium stores in `/Library/Data`
    /// is the file being written, not the one it was read from.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&mut self, path: impl AsRef<std::path::Path>) -> AltiumResult<()> {
        let path = path.as_ref();
        self.filepath = Some(path.display().to_string());
        crate::altium::save_atomic(path, "pcblib.tmp", |image| self.write(image))
    }

    /// Returns the number of footprints in the library.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.footprints.len()
    }

    /// Returns true if the library is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.footprints.is_empty()
    }

    /// Returns an iterator over the footprints.
    pub fn iter(&self) -> impl Iterator<Item = &Footprint> {
        self.footprints.iter()
    }

    /// Returns a mutable iterator over the footprints.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Footprint> {
        self.footprints.iter_mut()
    }

    /// Returns a list of footprint names.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.footprints.iter().map(|f| f.name.clone()).collect()
    }

    /// Returns the file path this library was loaded from, if any.
    #[must_use]
    pub fn filepath(&self) -> Option<&str> {
        self.filepath.as_deref()
    }

    /// The index of the footprint `name` resolves to: the exact name, else
    /// the footprint whose name is the same regardless of case — the way the
    /// file's own directory resolves it (see [`crate::altium::same_name`]).
    fn position(&self, name: &str) -> Option<usize> {
        self.footprints
            .iter()
            .position(|f| f.name == name)
            .or_else(|| {
                self.footprints
                    .iter()
                    .position(|f| crate::altium::same_name(&f.name, name))
            })
    }

    /// Gets a footprint by name — the exact name, else the one the name
    /// resolves to regardless of case.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Footprint> {
        self.position(name).map(|i| &self.footprints[i])
    }

    /// Gets a mutable reference to a footprint by name (resolved as
    /// [`Self::get`] resolves it).
    #[must_use]
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Footprint> {
        self.position(name).map(move |i| &mut self.footprints[i])
    }

    /// Adds a footprint to the library.
    pub fn add(&mut self, footprint: Footprint) {
        self.footprints.push(footprint);
    }

    /// Removes a footprint from the library by name.
    ///
    /// Returns the removed footprint if found, or `None` if no footprint with that name exists.
    pub fn remove(&mut self, name: &str) -> Option<Footprint> {
        self.position(name).map(|idx| self.footprints.remove(idx))
    }

    /// Updates a footprint in-place, preserving its position in the library.
    ///
    /// The footprint is matched by the `name` parameter. The replacement footprint's
    /// name does not need to match (allowing renames).
    ///
    /// Returns the old footprint if found, or `None` if no footprint with that name exists.
    pub fn update(&mut self, name: &str, replacement: Footprint) -> Option<Footprint> {
        self.position(name)
            .map(|i| std::mem::replace(&mut self.footprints[i], replacement))
    }

    /// Renames a footprint in place, so it keeps its position in the
    /// library and in the file. Returns whether `old_name` resolved to one.
    pub fn rename(&mut self, old_name: &str, new_name: &str) -> bool {
        self.rename_all(&[(old_name.to_string(), new_name.to_string())])
            .is_empty()
    }

    /// Renames several footprints at once, each in place. Every `(old, new)`
    /// pair resolves against the names as they were before the call, so a
    /// chain such as `A -> B, B -> C` renames both rather than renaming the
    /// new `B` twice. Returns the old names that resolved to nothing.
    pub fn rename_all(&mut self, renames: &[(String, String)]) -> Vec<String> {
        let mut missing = Vec::new();
        let mut resolved: Vec<(usize, &str)> = Vec::with_capacity(renames.len());
        for (old, new) in renames {
            match self.position(old) {
                Some(i) => resolved.push((i, new.as_str())),
                None => missing.push(old.clone()),
            }
        }
        for (i, new) in resolved {
            self.footprints[i].name = new.to_string();
            // A renamed footprint gets the storage its new name derives.
            self.footprints[i].storage_name = None;
        }
        missing
    }

    /// Reorders footprints according to the given name order.
    ///
    /// Footprints are reordered to match the order of names in `new_order`.
    /// Names not present in the library are ignored. Footprints not mentioned
    /// in `new_order` are placed at the end in their original relative order.
    ///
    /// Returns the new order of footprint names.
    pub fn reorder(&mut self, new_order: &[&str]) -> Vec<String> {
        // Stable-sort footprints into the desired order; footprints not listed
        // in `new_order` keep their relative order at the end.
        let rank = crate::altium::order_ranker(new_order);
        self.footprints.sort_by_key(|a| rank(a.name.as_str()));

        self.names()
    }

    /// Returns the number of embedded 3D models in the library.
    #[must_use]
    pub const fn model_count(&self) -> usize {
        self.models.len()
    }

    /// Returns an iterator over the embedded 3D models.
    pub fn models(&self) -> impl Iterator<Item = &EmbeddedModel> {
        self.models.iter()
    }

    /// Gets an embedded model by GUID.
    ///
    /// GUID matching is case-insensitive since Altium files may store GUIDs
    /// with inconsistent casing between component body references and the model index.
    #[must_use]
    pub fn get_model(&self, id: &str) -> Option<&EmbeddedModel> {
        self.models.iter().find(|m| m.id.eq_ignore_ascii_case(id))
    }

    /// Adds an embedded 3D model to the library.
    pub fn add_model(&mut self, model: EmbeddedModel) {
        self.models.push(model);
    }

    /// Returns all model GUIDs referenced by footprints in this library.
    ///
    /// GUIDs are normalised to lowercase for consistent matching.
    #[must_use]
    pub fn referenced_model_ids(&self) -> std::collections::HashSet<String> {
        let mut ids = std::collections::HashSet::new();
        for fp in &self.footprints {
            for cb in &fp.component_bodies {
                if cb.embedded {
                    ids.insert(cb.model_id.to_lowercase());
                }
            }
        }
        ids
    }

    /// Removes models that are not referenced by any footprint.
    ///
    /// This should be called after deleting footprints to prevent library bloat
    /// from orphaned embedded models.
    ///
    /// Returns the number of models removed.
    pub fn remove_orphaned_models(&mut self) -> usize {
        let referenced = self.referenced_model_ids();
        let original_count = self.models.len();
        self.models
            .retain(|m| referenced.contains(&m.id.to_lowercase()));
        let removed = original_count - self.models.len();
        if removed > 0 {
            tracing::debug!(removed, "Removed orphaned embedded models");
        }
        removed
    }

    /// Returns all model GUIDs that exist in the library's model collection.
    ///
    /// GUIDs are normalised to lowercase for consistent matching.
    #[must_use]
    pub fn available_model_ids(&self) -> std::collections::HashSet<String> {
        self.models.iter().map(|m| m.id.to_lowercase()).collect()
    }

    /// Removes component body references that point to non-existent models.
    ///
    /// This repairs libraries where `component_bodies` have `embedded: true`
    /// but the actual model data is missing from `/Library/Models/`.
    ///
    /// Returns a vector of (`footprint_name`, `removed_count`) for each affected footprint.
    pub fn remove_orphaned_component_bodies(&mut self) -> Vec<(String, usize)> {
        let available = self.available_model_ids();
        let mut results = Vec::new();

        for footprint in &mut self.footprints {
            let removed = footprint.retain_component_bodies(|cb| {
                // Keep external references (embedded: false) - they don't need model data
                if !cb.embedded {
                    return true;
                }
                // Keep if model_id is empty (shouldn't happen but be safe)
                if cb.model_id.is_empty() {
                    return true;
                }
                // Keep only if the model exists in the library
                available.contains(&cb.model_id.to_lowercase())
            });
            if removed > 0 {
                tracing::debug!(
                    footprint = %footprint.name,
                    removed,
                    "Removed orphaned component body references"
                );
                results.push((footprint.name.clone(), removed));
            }
        }

        results
    }

    /// The ANSI encoding the library's text is read and written in: the code
    /// page detected when it was read; for a library built in memory, the one
    /// its footprints' carried `PATTERN` bytes were written in, so footprints
    /// replayed into a new library keep their bytes; else the one in force
    /// (the server's).
    #[must_use]
    pub fn ansi_encoding(&self) -> &'static encoding_rs::Encoding {
        self.metadata
            .ansi_code_page
            .or_else(|| self.carried_code_page())
            .and_then(crate::altium::ansi_encoding_for)
            .unwrap_or_else(crate::altium::current_ansi_encoding)
    }

    /// The code page the footprints' carried `PATTERN` bytes and
    /// `UNICODE__PATTERN` twins agree on, if any.
    fn carried_code_page(&self) -> Option<u32> {
        let carried = |fp: &Footprint, key: &str| {
            fp.additional_parameters
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v.clone())
        };
        let pairs: Vec<(String, String)> = self
            .footprints
            .iter()
            .filter_map(|fp| {
                let twin = carried(fp, "UNICODE__PATTERN")?;
                let held = crate::altium::text_from_utf16_units(&twin)?;
                Some((held, carried(fp, "PATTERN")?))
            })
            .collect();
        crate::altium::detect_ansi_code_page(&pairs)
    }

    /// Returns a reference to the library metadata.
    ///
    /// The metadata contains information parsed from the `FileHeader` stream,
    /// including component names and descriptions.
    #[must_use]
    pub const fn metadata(&self) -> &LibraryMetadata {
        &self.metadata
    }
}

#[cfg(test)]
mod tests {
    /// A renamed footprint no longer belongs to its old storage: the
    /// carrier is dropped so the next save derives the new name's storage.
    #[test]
    fn renaming_a_footprint_drops_its_carried_storage_name() {
        let mut lib = super::PcbLib::new();
        let mut fp = super::Footprint::new("OLD");
        fp.storage_name = Some("OLD".to_string());
        lib.add(fp);
        let missing = lib.rename_all(&[("OLD".to_string(), "NEW".to_string())]);
        assert!(missing.is_empty());
        let renamed = lib.get("NEW").expect("renamed");
        assert_eq!(renamed.storage_name, None);
    }

    use super::*;

    /// Every kind reports one layer per primitive — an arm that returned
    /// nothing for a kind would hide that kind's layers.
    #[test]
    fn layers_of_reports_one_layer_per_primitive_of_every_kind() {
        let mut fp = Footprint::new("KINDS");
        let layer = Layer::Mechanical13;
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        fp.add_via(Via::new(0.0, 0.0, 0.6, 0.3));
        fp.add_track(Track::new(0.0, 0.0, 1.0, 0.0, 0.1, layer));
        fp.add_arc(Arc::circle(0.0, 0.0, 1.0, 0.1, layer));
        fp.add_text(Text::new(0.0, 0.0, "T", 1.0, layer));
        fp.add_region(Region::rectangle(0.0, 0.0, 1.0, 1.0, layer));
        fp.add_fill(Fill::new(0.0, 0.0, 1.0, 1.0, layer));
        let mut body = ComponentBody::new("", "b.step");
        body.layer = layer;
        fp.add_component_body(body);
        for kind in PrimitiveKind::WRITE_ORDER {
            let layers = fp.layers_of(kind);
            assert_eq!(layers.len(), fp.count_of(kind), "{kind:?}");
            let expected = match kind {
                PrimitiveKind::Via => Layer::MultiLayer,
                PrimitiveKind::Pad => Layer::TopLayer,
                _ => layer,
            };
            assert_eq!(layers, vec![expected], "{kind:?}");
        }
    }

    /// A footprint carrying an identity on every primitive kind, plus a via
    /// whose replayed block has identity bytes and one whose block is too
    /// short to hold any.
    fn footprint_with_identities_everywhere() -> Footprint {
        let guid = Some("{11111111-2222-3333-4444-555555555555}".to_string());
        let uid = Some("ABCDEFGH".to_string());
        let mut fp = Footprint::new("FP");
        fp.guid = guid.clone();

        let mut pad = Pad::smd("1", -0.5, 0.0, 0.6, 0.5);
        pad.guid = guid.clone();
        pad.unique_id = uid.clone();
        pad.identity_guid = guid.clone();
        pad.identity_guid_b = guid.clone();
        fp.add_pad(pad);

        let mut via = Via::new(1.5, 0.0, 0.6, 0.3);
        via.guid = guid.clone();
        via.unique_id = uid.clone();
        let mut block = vec![0u8; 300];
        block[259..291].fill(0xAB);
        block[0] = 0x4A; // non-identity bytes must survive
        via.raw_block = Some(block);
        fp.add_via(via);

        let mut short_via = Via::new(2.5, 0.0, 0.6, 0.3);
        short_via.raw_block = Some(vec![0xAB; 10]); // too short to hold identities
        fp.add_via(short_via);

        let mut track = Track::new(0.0, 0.0, 1.0, 0.0, 0.2, Layer::TopOverlay);
        track.guid = guid.clone();
        track.unique_id = uid.clone();
        fp.add_track(track);
        let mut arc = Arc::circle(0.0, 2.0, 0.5, 0.1, Layer::TopOverlay);
        arc.guid = guid.clone();
        arc.unique_id = uid.clone();
        fp.add_arc(arc);
        let mut region = Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopCourtyard);
        region.guid = guid.clone();
        region.unique_id = uid.clone();
        fp.add_region(region);
        let mut fill = Fill::new(-0.5, 0.8, 0.5, 1.2, Layer::TopLayer);
        fill.guid = guid.clone();
        fill.unique_id = uid.clone();
        fp.add_fill(fill);
        let mut body = ComponentBody::new("{G-1}", "m.step");
        body.guid = guid.clone();
        body.unique_id = uid.clone();
        fp.add_component_body(body);
        fp.add_text(Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 0.0,
            y: -2.0,
            text: ".Designator".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            kind: TextKind::Stroke,
            rotation: 0.0,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::default(),
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::default(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: uid,
            guid,
            raw_geometry: None,
        });
        fp
    }

    /// `reset_identities` clears every identity on every primitive kind and
    /// zeroes a replayed via block's identity slots, leaving geometry alone.
    #[test]
    fn reset_identities_clears_every_identity_and_keeps_geometry() {
        let mut fp = footprint_with_identities_everywhere();
        fp.reset_identities();

        assert!(fp.guid.is_none());
        let pad = &fp.pads[0];
        assert!(pad.guid.is_none() && pad.unique_id.is_none());
        assert!(pad.identity_guid.is_none() && pad.identity_guid_b.is_none());
        assert!((pad.width - 0.6).abs() < 1e-9, "geometry untouched");
        let via = &fp.vias[0];
        assert!(via.guid.is_none() && via.unique_id.is_none());
        let block = via.raw_block.as_ref().unwrap();
        assert!(
            block[259..291].iter().all(|&b| b == 0),
            "identity slots zeroed"
        );
        assert_eq!(block[0], 0x4A, "other replayed bytes untouched");
        assert_eq!(
            fp.vias[1].raw_block.as_deref(),
            Some(&[0xABu8; 10][..]),
            "short block left alone"
        );
        assert!(fp.tracks[0].guid.is_none() && fp.tracks[0].unique_id.is_none());
        assert!(fp.arcs[0].guid.is_none() && fp.arcs[0].unique_id.is_none());
        assert!(fp.regions[0].guid.is_none() && fp.regions[0].unique_id.is_none());
        assert!(fp.fills[0].guid.is_none() && fp.fills[0].unique_id.is_none());
        assert!(fp.text[0].guid.is_none() && fp.text[0].unique_id.is_none());
        assert!(
            fp.component_bodies[0].guid.is_none() && fp.component_bodies[0].unique_id.is_none()
        );
        assert_eq!(
            fp.component_bodies[0].model_id, "{G-1}",
            "model reference kept"
        );
    }

    /// Helper to compare floats with tolerance.
    fn approx_eq(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() < tolerance
    }

    #[test]
    fn every_primitive_kind_round_trips_through_its_altium_ids() {
        // The numeric id and the PRIMITIVEOBJECTID token are what a
        // UniqueIDPrimitiveInformation record is keyed by. A kind whose token
        // or id is wrong re-attaches a stable identity to the wrong primitive
        // on the next read, so each arm is pinned to its own value.
        for (kind, id, token) in [
            (PrimitiveKind::Arc, 1, "Arc"),
            (PrimitiveKind::Pad, 2, "Pad"),
            (PrimitiveKind::Via, 3, "Via"),
            (PrimitiveKind::Track, 4, "Track"),
            (PrimitiveKind::Text, 5, "Text"),
            (PrimitiveKind::Fill, 6, "Fill"),
            (PrimitiveKind::Region, 89, "Region"),
            (PrimitiveKind::ComponentBody, 90, "ComponentBody"),
        ] {
            assert_eq!(kind.altium_object_id(), id, "{token} id");
            assert_eq!(kind.object_id(), token, "{token} token");
            assert_eq!(
                PrimitiveKind::from_altium_object_id(id),
                Some(kind),
                "{token} reverse"
            );
        }

        // 85 is the footprint record itself, not a primitive; it and any other
        // unmapped id must not resolve to a kind.
        for id in [0, 7, 85, 91, u32::MAX] {
            assert_eq!(PrimitiveKind::from_altium_object_id(id), None, "id {id}");
        }
    }

    #[test]
    fn orphan_removal_keeps_bodies_it_cannot_judge() {
        // Only an embedded body naming a model the library does not carry is
        // an orphan. An external reference, or one with no model id to check
        // against, must survive — deleting those would drop live geometry.
        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("MIXED");

        let mut external = ComponentBody::new("{ABSENT}", "ext.step");
        external.embedded = false;
        fp.add_component_body(external);

        let mut anonymous = ComponentBody::new("", "anon.step");
        anonymous.embedded = true;
        fp.add_component_body(anonymous);

        let mut orphan = ComponentBody::new("{GONE}", "gone.step");
        orphan.embedded = true;
        fp.add_component_body(orphan);

        lib.add(fp);

        let results = lib.remove_orphaned_component_bodies();
        assert_eq!(results, vec![("MIXED".to_string(), 1)]);
        let kept = &lib.get("MIXED").expect("the footprint").component_bodies;
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().any(|cb| !cb.embedded));
        assert!(kept.iter().any(|cb| cb.model_id.is_empty()));
    }

    #[test]
    fn moving_a_layer_takes_every_kind_and_drops_the_stale_carriers() {
        // One of each layered kind on Mechanical 13, one track elsewhere, and
        // the carriers a read primitive may hold: an unmapped header byte and
        // a region's / body's V7_LAYER token, which named the old layer.
        let mut fp = Footprint::new("MOVE");
        let (from, to) = (Layer::Mechanical13, Layer::Mechanical20);
        fp.add_track(Track::new(0.0, 0.0, 1.0, 0.0, 0.1, from));
        fp.add_track(Track::new(0.0, 1.0, 1.0, 1.0, 0.1, Layer::TopOverlay));
        fp.add_arc(Arc::circle(0.0, 0.0, 1.0, 0.1, from));
        let mut text = Text::new(0.0, 0.0, "T", 1.0, from);
        text.raw_layer_id = Some(100);
        fp.add_text(text);
        fp.add_fill(Fill::new(0.0, 0.0, 1.0, 1.0, from));
        let mut region = Region::rectangle(0.0, 0.0, 1.0, 1.0, from);
        region.v7_layer = Some("MECHANICAL13".to_string());
        fp.add_region(region);
        let mut pad = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        pad.layer = from;
        fp.add_pad(pad);
        let mut body = ComponentBody::new("", "body.step");
        body.layer = from;
        body.v7_layer = Some("MECHANICAL13".to_string());
        fp.add_component_body(body);

        let moved = fp.move_layer(from, to);
        assert_eq!(
            moved,
            LayerMove {
                tracks: 1,
                arcs: 1,
                text: 1,
                fills: 1,
                regions: 1,
                pads: 1,
                component_bodies: 1,
            }
        );
        assert_eq!(moved.total(), 7);
        assert_eq!(fp.tracks[0].layer, to);
        assert_eq!(
            fp.tracks[1].layer,
            Layer::TopOverlay,
            "another layer is untouched"
        );
        assert_eq!(fp.arcs[0].layer, to);
        assert_eq!(fp.text[0].layer, to);
        assert_eq!(
            fp.text[0].raw_layer_id, None,
            "the carried byte went with the move"
        );
        assert_eq!(fp.fills[0].layer, to);
        assert_eq!(fp.regions[0].layer, to);
        assert_eq!(fp.regions[0].v7_layer, None, "the stale token went too");
        assert_eq!(fp.pads[0].layer, to);
        assert_eq!(fp.component_bodies[0].layer, to);
        assert_eq!(fp.component_bodies[0].v7_layer, None);
        assert_eq!(
            fp.move_layer(from, to),
            LayerMove::default(),
            "nothing left on it"
        );
    }

    #[test]
    fn dropping_a_body_leaves_the_other_primitives_where_they_were() {
        // Interleaved as a file might store it: body A, pad 1, body B, pad 2.
        // Removing A must not move B in front of pad 1, which is what a bare
        // `retain` on the list does once the order has one slot too many.
        let mut fp = Footprint::new("ORDER");
        let mut a = ComponentBody::new("{A}", "a.step");
        a.embedded = true;
        fp.add_component_body(a);
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        let mut b = ComponentBody::new("{B}", "b.step");
        b.embedded = true;
        fp.add_component_body(b);
        fp.add_pad(Pad::smd("2", 1.0, 0.0, 1.0, 1.0));

        assert_eq!(fp.retain_component_bodies(|cb| cb.model_id != "{A}"), 1);
        assert_eq!(fp.component_bodies.len(), 1);
        assert_eq!(fp.component_bodies[0].model_id, "{B}");
        assert_eq!(
            fp.write_sequence(),
            vec![
                (PrimitiveKind::Pad, 0),
                (PrimitiveKind::ComponentBody, 0),
                (PrimitiveKind::Pad, 1),
            ]
        );

        // The library-level repair goes through the same path.
        let mut lib = PcbLib::new();
        let mut fp = Footprint::new("REPAIR");
        let mut orphan = ComponentBody::new("{GONE}", "gone.step");
        orphan.embedded = true;
        fp.add_component_body(orphan);
        fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        let mut anonymous = ComponentBody::new("", "anon.step");
        anonymous.embedded = true;
        fp.add_component_body(anonymous);
        lib.add(fp);
        assert_eq!(
            lib.remove_orphaned_component_bodies(),
            vec![("REPAIR".to_string(), 1)]
        );
        assert_eq!(
            lib.get("REPAIR").expect("footprint").write_sequence(),
            vec![(PrimitiveKind::Pad, 0), (PrimitiveKind::ComponentBody, 0)]
        );
    }

    #[test]
    fn a_library_reports_its_source_path_only_once_it_has_one() {
        let mut lib = PcbLib::new();
        assert_eq!(lib.filepath(), None);

        lib.filepath = Some("Parts.PcbLib".to_string());
        assert_eq!(lib.filepath(), Some("Parts.PcbLib"));
    }

    #[test]
    fn updating_an_absent_footprint_reports_no_previous_value() {
        // The return value is the caller's signal that it replaced something;
        // a miss must not silently insert.
        let mut lib = PcbLib::new();
        lib.add(Footprint::new("PRESENT"));

        assert!(lib.update("ABSENT", Footprint::new("ABSENT")).is_none());
        assert_eq!(lib.len(), 1);

        let old = lib.update("PRESENT", Footprint::new("REPLACED"));
        assert_eq!(old.expect("the replaced footprint").name, "PRESENT");
    }

    #[test]
    fn footprint_creation() {
        let mut fp = Footprint::new("TEST");
        fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.8, 0.9));
        fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.8, 0.9));

        assert_eq!(fp.name, "TEST");
        assert_eq!(fp.pads.len(), 2);
    }

    #[test]
    fn file_version_info_frames_to_canonical_bytes() {
        // The embedded FileVersionInfo text must frame to exactly the bytes
        // Altium expects. Guards the asset against accidental mangling (a stray
        // newline, an editor re-encode, git EOL normalisation) that would
        // silently change the emitted /FileVersionInfo stream.
        let mut data = Vec::new();
        crate::altium::framing::write_cstring_param_block(&mut data, PcbLib::FVI_TEXT.as_bytes());
        assert_eq!(data.len(), 2573, "FileVersionInfo stream size changed");
        assert_eq!(&data[..4], &[0x09, 0x0a, 0x00, 0x00]); // LE length prefix = 2569
        assert_eq!(*data.last().unwrap(), 0x00);
        assert!(PcbLib::FVI_TEXT.starts_with("|COUNT="));
    }

    #[test]
    fn library_operations() {
        let mut lib = PcbLib::new();
        assert!(lib.is_empty());

        lib.add(Footprint::new("FP1"));
        lib.add(Footprint::new("FP2"));

        assert_eq!(lib.len(), 2);
        assert_eq!(lib.names(), vec!["FP1", "FP2"]);
        assert!(lib.get("FP1").is_some());
        assert!(lib.get("FP3").is_none());
    }

    #[test]
    fn library_reorder() {
        let mut lib = PcbLib::new();

        lib.add(Footprint::new("A"));
        lib.add(Footprint::new("B"));
        lib.add(Footprint::new("C"));
        lib.add(Footprint::new("D"));

        assert_eq!(lib.names(), vec!["A", "B", "C", "D"]);

        // Reorder: C, A first; B, D should follow in original relative order
        let new_order = lib.reorder(&["C", "A"]);
        assert_eq!(new_order, vec!["C", "A", "B", "D"]);

        // Reorder with non-existent names (should be ignored)
        let new_order = lib.reorder(&["D", "X", "B"]);
        assert_eq!(new_order, vec!["D", "B", "C", "A"]);

        // Reorder to completely reverse
        let new_order = lib.reorder(&["A", "C", "B", "D"]);
        assert_eq!(new_order, vec!["A", "C", "B", "D"]);
    }

    #[test]
    fn binary_roundtrip_pads() {
        // Create a footprint with pads
        let mut original = Footprint::new("ROUNDTRIP_PAD");
        original.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        original.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));

        // Encode to binary
        let data = writer::encode_data_stream(&original).expect("encoding should succeed");

        // Decode from binary
        let mut decoded = Footprint::new("ROUNDTRIP_PAD");
        reader::parse_data_stream(&mut decoded, &data, None);

        // Verify
        assert_eq!(decoded.pads.len(), 2);
        assert_eq!(decoded.pads[0].designator, "1");
        assert_eq!(decoded.pads[1].designator, "2");
        assert!(approx_eq(decoded.pads[0].x, -0.5, 0.001));
        assert!(approx_eq(decoded.pads[1].x, 0.5, 0.001));
        assert!(approx_eq(decoded.pads[0].width, 0.6, 0.001));
        assert!(approx_eq(decoded.pads[0].height, 0.5, 0.001));
    }

    #[test]
    fn binary_roundtrip_pad_thermal_relief() {
        // A pad with NON-default thermal-relief / power-plane settings must survive
        // encode -> decode with all six fields intact.
        let mut original = Footprint::new("ROUNDTRIP_PAD_RELIEF");
        let mut pad = Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8);
        pad.power_plane_connect_style = PowerPlaneConnectStyle::Direct;
        pad.relief_conductor_width = 0.3; // != 0.254 default
        pad.relief_entries = 2; // != 4 default
        pad.relief_air_gap = 0.2; // != 0.254 default
        pad.power_plane_relief_expansion = 0.6; // != 0.508 default
        pad.power_plane_clearance = 0.7; // != 0.508 default
        original.add_pad(pad);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_PAD_RELIEF");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 1);
        let p = &decoded.pads[0];
        assert_eq!(p.power_plane_connect_style, PowerPlaneConnectStyle::Direct);
        assert!(approx_eq(p.relief_conductor_width, 0.3, 0.0001));
        assert_eq!(p.relief_entries, 2);
        assert!(approx_eq(p.relief_air_gap, 0.2, 0.0001));
        assert!(approx_eq(p.power_plane_relief_expansion, 0.6, 0.0001));
        assert!(approx_eq(p.power_plane_clearance, 0.7, 0.0001));
    }

    #[test]
    fn binary_roundtrip_pad_polygon_connect() {
        // A from-scratch pad with an override is written in AD24's layout: the
        // 194-byte block and the 34-byte override after it.
        use super::PadPolygonConnect;

        let connect = PadPolygonConnect {
            style: PowerPlaneConnectStyle::Relief,
            air_gap: 0.1524,
            conductor_width: 0.3556,
            conductors: 2,
            auto_conductors: true,
            rotation: 45,
            min_distance: 0.508,
            min_distance_enabled: true,
        };
        let mut original = Footprint::new("ROUNDTRIP_PAD_POLYGON_CONNECT");
        let mut pad = Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8);
        pad.polygon_connect = Some(connect);
        original.add_pad(pad);
        original.add_pad(Pad::through_hole("2", 2.54, 0.0, 1.6, 1.6, 0.8));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_PAD_POLYGON_CONNECT");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 2);
        let with = &decoded.pads[0];
        let got = with.polygon_connect.expect("override read back");
        assert_eq!(got.style, connect.style);
        assert!(approx_eq(got.air_gap, connect.air_gap, 0.0001));
        assert!(approx_eq(
            got.conductor_width,
            connect.conductor_width,
            0.0001
        ));
        assert_eq!(got.conductors, 2);
        assert!(got.auto_conductors);
        assert_eq!(got.rotation, 45);
        assert!(approx_eq(got.min_distance, connect.min_distance, 0.0001));
        assert!(got.min_distance_enabled);
        assert_eq!(
            with.raw_tail.as_ref().map(Vec::len),
            Some(228 - 61),
            "194-byte block plus the 34-byte override"
        );
        let without = &decoded.pads[1];
        assert_eq!(without.polygon_connect, None);
        assert_eq!(
            without.raw_tail.as_ref().map(Vec::len),
            Some(202 - 61),
            "a pad without an override keeps the 202-byte template"
        );
    }

    #[test]
    fn pad_default_thermal_relief_byte_identical() {
        // A pad created with default thermal-relief must produce byte-for-byte
        // identical output regardless of whether the writer emits the struct
        // fields or the fixed template constants. We prove this by checking
        // that the default field values map back to the canonical template raw
        // values (style 0; conductor width / air gap 100000; entries 4; relief
        // expansion / clearance 200000), so the oracle stays at 0 regressions.
        let pad = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        assert_eq!(
            pad.power_plane_connect_style,
            PowerPlaneConnectStyle::Relief
        );
        assert_eq!(pad.power_plane_connect_style.to_id(), 0);
        assert_eq!(units::from_mm(pad.relief_conductor_width), 100_000);
        assert_eq!(pad.relief_entries, 4);
        assert_eq!(units::from_mm(pad.relief_air_gap), 100_000);
        assert_eq!(units::from_mm(pad.power_plane_relief_expansion), 200_000);
        assert_eq!(units::from_mm(pad.power_plane_clearance), 200_000);
    }

    #[test]
    fn binary_roundtrip_pad_slot_hole_and_tolerances() {
        // PR-8: a pad with a slot hole (non-zero slot length + rotation) and
        // non-default drill tolerances must survive encode -> decode.
        let mut original = Footprint::new("ROUNDTRIP_PAD_SLOT");
        let mut pad = Pad::through_hole("1", 0.0, 0.0, 2.0, 1.2, 0.8);
        pad.hole_shape = HoleShape::Slot;
        pad.hole_slot_length = 1.5; // != 0 default
        pad.hole_rotation = 45.0; // != 0 default
        pad.hole_positive_tolerance = Some(0.05);
        pad.hole_negative_tolerance = Some(0.02);
        original.add_pad(pad);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_PAD_SLOT");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 1);
        let p = &decoded.pads[0];
        assert_eq!(p.hole_shape, HoleShape::Slot);
        assert!(approx_eq(p.hole_slot_length, 1.5, 0.0001));
        assert!(approx_eq(p.hole_rotation, 45.0, 0.0001));
        assert!(approx_eq(p.hole_positive_tolerance.unwrap(), 0.05, 0.0001));
        assert!(approx_eq(p.hole_negative_tolerance.unwrap(), 0.02, 0.0001));
    }

    #[test]
    fn pad_default_slot_hole_fields_byte_identical() {
        // A default (round-hole, unset-tolerance) pad must map its new fields back to
        // exactly the writer's previous hard-coded values so the oracle stays at 0
        // regressions: slot length 0, rotation 0, tolerances -> 0x7FFFFFFF sentinel.
        let pad = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        assert_eq!(units::from_mm(pad.hole_slot_length), 0); // writer hard-coded 0
        assert!(approx_eq(pad.hole_rotation, 0.0, 1e-9)); // writer hard-coded 0.0
        assert_eq!(pad.hole_positive_tolerance, None); // None -> sentinel
        assert_eq!(pad.hole_negative_tolerance, None);
    }

    #[test]
    fn binary_roundtrip_tracks() {
        let mut original = Footprint::new("ROUNDTRIP_TRACK");
        original.add_track(Track::new(-1.0, -0.5, 1.0, -0.5, 0.15, Layer::TopOverlay));
        original.add_track(Track::new(1.0, -0.5, 1.0, 0.5, 0.15, Layer::TopOverlay));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_TRACK");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.tracks.len(), 2);
        assert!(approx_eq(decoded.tracks[0].x1, -1.0, 0.001));
        assert!(approx_eq(decoded.tracks[0].x2, 1.0, 0.001));
        assert!(approx_eq(decoded.tracks[0].width, 0.15, 0.001));
        assert_eq!(decoded.tracks[0].layer, Layer::TopOverlay);
    }

    #[test]
    fn binary_roundtrip_arcs() {
        let mut original = Footprint::new("ROUNDTRIP_ARC");
        original.add_arc(Arc::circle(0.0, 0.0, 1.0, 0.15, Layer::TopOverlay));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_ARC");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.arcs.len(), 1);
        assert!(approx_eq(decoded.arcs[0].x, 0.0, 0.001));
        assert!(approx_eq(decoded.arcs[0].y, 0.0, 0.001));
        assert!(approx_eq(decoded.arcs[0].radius, 1.0, 0.001));
        assert!(approx_eq(decoded.arcs[0].start_angle, 0.0, 0.001));
        assert!(approx_eq(decoded.arcs[0].end_angle, 360.0, 0.001));
    }

    #[test]
    fn binary_roundtrip_mixed_primitives() {
        let mut original = Footprint::new("ROUNDTRIP_MIXED");

        // Add arcs first (record type 0x01)
        original.add_arc(Arc::circle(0.0, 0.0, 0.5, 0.1, Layer::TopOverlay));

        // Add pads (record type 0x02)
        original.add_pad(Pad::smd("1", -1.0, 0.0, 0.6, 0.5));
        original.add_pad(Pad::smd("2", 1.0, 0.0, 0.6, 0.5));

        // Add tracks (record type 0x04)
        original.add_track(Track::new(-1.5, -0.3, 1.5, -0.3, 0.12, Layer::TopOverlay));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_MIXED");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.arcs.len(), 1);
        assert_eq!(decoded.pads.len(), 2);
        assert_eq!(decoded.tracks.len(), 1);
    }

    #[test]
    fn binary_roundtrip_coordinate_precision() {
        let mut original = Footprint::new("ROUNDTRIP_PRECISION");

        // Test various coordinate values
        original.add_pad(Pad::smd("1", 0.125, 0.0, 0.3, 0.4));
        original.add_pad(Pad::smd("2", 1.27, 0.0, 0.5, 0.5));
        original.add_pad(Pad::smd("3", 2.54, 0.0, 1.0, 1.0));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_PRECISION");
        reader::parse_data_stream(&mut decoded, &data, None);

        // Altium internal units give ~2.54nm resolution
        assert!(approx_eq(decoded.pads[0].x, 0.125, 0.0001));
        assert!(approx_eq(decoded.pads[1].x, 1.27, 0.0001));
        assert!(approx_eq(decoded.pads[2].x, 2.54, 0.0001));
    }

    #[test]
    fn binary_roundtrip_through_hole_pad() {
        let mut original = Footprint::new("ROUNDTRIP_TH");
        original.add_pad(Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_TH");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 1);
        assert!(decoded.pads[0].hole_size.is_some());
        assert!(approx_eq(decoded.pads[0].hole_size.unwrap(), 0.8, 0.001));
    }

    #[test]
    fn binary_roundtrip_component_layers() {
        // Test that component layer pairs roundtrip correctly
        let mut original = Footprint::new("ROUNDTRIP_LAYERS");

        // Add tracks on each component layer pair
        original.add_track(Track::new(-1.0, 0.0, 1.0, 0.0, 0.1, Layer::TopAssembly));
        original.add_track(Track::new(-1.0, 0.1, 1.0, 0.1, 0.1, Layer::TopCourtyard));
        original.add_track(Track::new(-1.0, 0.2, 1.0, 0.2, 0.1, Layer::Top3DBody));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_LAYERS");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.tracks.len(), 3);
        assert_eq!(decoded.tracks[0].layer, Layer::TopAssembly);
        assert_eq!(decoded.tracks[1].layer, Layer::TopCourtyard);
        assert_eq!(decoded.tracks[2].layer, Layer::Top3DBody);
    }

    #[test]
    fn binary_roundtrip_text() {
        let mut original = Footprint::new("ROUNDTRIP_TEXT");

        // Add text with different rotations
        original.add_text(Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 0.0,
            y: 1.0,
            text: ".Designator".to_string(),
            height: 0.8,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::MiddleCenter,
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        });
        original.add_text(Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 1.5,
            y: 0.5,
            text: "TEST".to_string(),
            height: 0.5,
            layer: Layer::TopOverlay,
            rotation: 90.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::TopLeft,
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        });

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_TEXT");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.text.len(), 2);

        // First text
        assert_eq!(decoded.text[0].text, ".Designator");
        assert!(approx_eq(decoded.text[0].x, 0.0, 0.001));
        assert!(approx_eq(decoded.text[0].y, 1.0, 0.001));
        assert!(approx_eq(decoded.text[0].height, 0.8, 0.001));
        assert!(approx_eq(decoded.text[0].rotation, 0.0, 0.001));
        assert_eq!(decoded.text[0].layer, Layer::TopOverlay);

        // Second text (rotated)
        assert_eq!(decoded.text[1].text, "TEST");
        assert!(approx_eq(decoded.text[1].x, 1.5, 0.001));
        assert!(approx_eq(decoded.text[1].y, 0.5, 0.001));
        assert!(approx_eq(decoded.text[1].height, 0.5, 0.001));
        assert!(approx_eq(decoded.text[1].rotation, 90.0, 0.001));
    }

    #[test]
    fn text_stroke_width_round_trips() {
        // An explicit StrokeWidth (geometry offset 36) must survive the
        // round-trip rather than inheriting the template's 4-mil stroke.
        let mut original = Footprint::new("TEXT_STROKE");
        original.add_text(Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 0.0,
            y: 0.0,
            text: "W".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: Some(0.2),
            italic: false,
            bold: false,
            mirror: false,
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::MiddleCenter,
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        });

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("TEXT_STROKE");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.text.len(), 1);
        let w = decoded.text[0]
            .stroke_width
            .expect("explicit stroke width should round-trip");
        assert!(approx_eq(w, 0.2, 0.001), "expected 0.2 mm, got {w}");
    }

    #[test]
    fn text_truetype_italic_round_trips() {
        // A TrueType italic text must round-trip italic@45 and derive baseFontType@43=1.
        let mut original = Footprint::new("TT_ITALIC");
        original.add_text(Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 0.0,
            y: 0.0,
            text: "String".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::TrueType,
            stroke_font: None,
            stroke_width: None,
            italic: true,
            bold: false,
            mirror: false,
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::MiddleCenter,
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        });

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("TT_ITALIC");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.text.len(), 1);
        assert_eq!(decoded.text[0].kind, TextKind::TrueType);
        assert!(decoded.text[0].italic, "italic must survive the round-trip");
    }

    #[test]
    fn text_default_stroke_geometry_byte_identical() {
        // Guards the oracle: a from-scratch stroke text with default styling must emit
        // the unmodified template at the styling offsets (43, 45 == 0) and at the
        // PR-10 offsets (mirror@35, bold@44, font-name@46-109, justification@132).
        let geom = writer::encode_text_geometry(
            &Text {
                raw_layer_id: None,
                barcode_full_width: None,
                barcode_full_height: None,
                barcode_x_margin: None,
                barcode_y_margin: None,
                barcode_kind: 0,
                barcode_font_name: String::new(),
                barcode_inverted: false,
                barcode_show_text: false,
                x: 0.0,
                y: 0.0,
                text: "X".into(),
                height: 1.0,
                layer: Layer::TopOverlay,
                rotation: 0.0,
                kind: TextKind::Stroke,
                stroke_font: None,
                stroke_width: None,
                italic: false,
                bold: false,
                mirror: false,
                is_comment: false,
                is_designator: false,
                font_name: "Arial".to_string(),
                // BottomLeft is the from-scratch default; it encodes to the template's 0x03.
                justification: TextJustification::BottomLeft,
                is_inverted: false,
                inverted_border: None,
                use_inverted_rectangle: false,
                inverted_rect_width: None,
                inverted_rect_height: None,
                inverted_rect_text_offset: None,
                flags: PcbFlags::empty(),
                net_index: 0xFFFF,
                polygon_index: 0xFFFF,
                component_index: -1,
                unique_id: None,
                guid: None,
                raw_geometry: None,
            },
            None,
        );
        assert_eq!(
            geom[43], 0x00,
            "stroke baseFontType must stay template default"
        );
        assert_eq!(geom[45], 0x00, "non-italic must stay template default");
        // PR-10 byte-identity: every new/wired field's default equals the template byte.
        assert_eq!(geom[35], 0x00, "default mirror must stay template 0x00");
        assert_eq!(geom[40], 0x00, "default is_comment must stay template 0x00");
        assert_eq!(
            geom[41], 0x00,
            "default is_designator must stay template 0x00"
        );
        assert_eq!(geom[44], 0x00, "default bold must stay template 0x00");
        assert_eq!(
            geom[132], 0x03,
            "default justification must stay template 0x03"
        );
        // Font-name field (46-109): "Arial" UTF-16 LE + zero fill, exactly the template.
        let mut expected_font = [0u8; 64];
        expected_font[..10]
            .copy_from_slice(&[0x41, 0x00, 0x72, 0x00, 0x69, 0x00, 0x61, 0x00, 0x6C, 0x00]);
        assert_eq!(
            &geom[46..110],
            &expected_font,
            "default font name must reproduce the template's Arial UTF-16 field"
        );
    }

    #[test]
    fn text_pr10_fields_round_trip() {
        // A non-default text must survive encode -> decode for every PR-10 field:
        // mirror, bold, font_name, a non-default justification, kind, italic.
        let mut original = Footprint::new("TEXT_PR10");
        original.add_text(Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 0.0,
            y: 0.0,
            text: "Fancy".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::TrueType,
            stroke_font: None,
            stroke_width: None,
            italic: true,
            bold: true,
            mirror: true,
            is_comment: false,
            is_designator: false,
            font_name: "Times New Roman".to_string(),
            justification: TextJustification::TopRight,
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        });

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("TEXT_PR10");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.text.len(), 1);
        let t = &decoded.text[0];
        assert!(t.mirror, "mirror must round-trip");
        assert!(t.bold, "bold must round-trip");
        assert!(t.italic, "italic must round-trip");
        assert_eq!(t.kind, TextKind::TrueType, "kind must round-trip");
        assert_eq!(t.font_name, "Times New Roman", "font_name must round-trip");
        assert_eq!(
            t.justification,
            TextJustification::TopRight,
            "justification must round-trip"
        );
    }

    #[test]
    fn text_comment_designator_flags_round_trip() {
        // IsComment@40 / IsDesignator@41 must
        // survive encode -> decode; a plain text keeps both false.
        let mut original = Footprint::new("TEXT_FLAGS");
        let mut text = Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 0.0,
            y: 0.0,
            text: ".Designator".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            is_comment: false,
            is_designator: true,
            font_name: "Arial".to_string(),
            justification: TextJustification::BottomLeft,
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        };
        original.add_text(text.clone());
        text.text = ".Comment".to_string();
        text.is_comment = true;
        text.is_designator = false;
        original.add_text(text.clone());
        text.text = "plain".to_string();
        text.is_comment = false;
        original.add_text(text);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("TEXT_FLAGS");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.text.len(), 3);
        let by_content = |c: &str| {
            decoded
                .text
                .iter()
                .find(|t| t.text == c)
                .unwrap_or_else(|| panic!("text {c:?} not found"))
        };
        let designator = by_content(".Designator");
        assert!(designator.is_designator, "is_designator must round-trip");
        assert!(!designator.is_comment);
        let comment = by_content(".Comment");
        assert!(comment.is_comment, "is_comment must round-trip");
        assert!(!comment.is_designator);
        let plain = by_content("plain");
        assert!(!plain.is_comment, "plain text keeps is_comment false");
        assert!(!plain.is_designator, "plain text keeps is_designator false");
    }

    #[test]
    fn pad_is_plated_round_trips() {
        // is_plated @60 is an independent field, not derived from hole_size: it
        // must survive encode -> decode, both at the default (true —
        // Altium's for every pad, SMD included) and explicitly unplated.
        let mut original = Footprint::new("PAD_PLATED");
        original.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 0.6));
        let mut unplated = Pad::through_hole("2", 2.0, 0.0, 1.6, 1.6, 0.8);
        unplated.is_plated = false;
        original.add_pad(unplated);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("PAD_PLATED");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 2);
        let smd = decoded.pads.iter().find(|p| p.designator == "1").unwrap();
        assert!(smd.is_plated, "SMD pad reads back Altium's plated default");
        let th = decoded.pads.iter().find(|p| p.designator == "2").unwrap();
        assert!(!th.is_plated, "explicitly unplated hole must round-trip");
    }

    #[test]
    fn pad_identity_guids_round_trip() {
        // The two per-pad identity GUIDs @126/@142 must read back verbatim, so
        // that encode -> decode -> encode reproduces the same GUID bytes
        // instead of regenerating them.
        let mut original = Footprint::new("PAD_GUIDS");
        original.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 0.6));

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("PAD_GUIDS");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 1);
        let pad = &decoded.pads[0];
        let nil = "{00000000-0000-0000-0000-000000000000}";
        let guid_a = pad.identity_guid.as_deref().expect("GUID-A read back");
        let guid_b = pad.identity_guid_b.as_deref().expect("GUID-B read back");
        assert_ne!(guid_a, nil, "fresh GUID-A is non-nil");
        assert_ne!(guid_a, guid_b, "GUID-A and GUID-B are independent");
        // Braced uppercase GUID shape.
        assert!(
            guid_a.starts_with('{') && guid_a.ends_with('}') && guid_a.len() == 38,
            "GUID string shape: {guid_a}"
        );

        // Re-encoding the decoded footprint preserves the exact GUID bytes: the
        // second stream carries them verbatim rather than fresh randoms.
        let data2 = writer::encode_data_stream(&decoded).expect("re-encode");
        let mut decoded2 = Footprint::new("PAD_GUIDS");
        reader::parse_data_stream(&mut decoded2, &data2, None);
        assert_eq!(
            decoded2.pads[0].identity_guid.as_deref(),
            Some(guid_a),
            "GUID-A must be replayed verbatim on re-encode"
        );
        assert_eq!(
            decoded2.pads[0].identity_guid_b.as_deref(),
            Some(guid_b),
            "GUID-B must be replayed verbatim on re-encode"
        );
    }

    #[test]
    fn text_inverted_rect_round_trip() {
        // A framed inverted (knockout) text must survive encode -> decode for the
        // whole inverted-rect descriptor: IsInverted@110, InvertedBorder@111,
        // UseInvertedRectangle@123, InvertedRectWidth@124, InvertedRectHeight@128,
        // InvertedRectTextOffset@133 (offsets verified against AltiumSharp ReadText).
        let mut original = Footprint::new("TEXT_INVRECT");
        original.add_text(Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 0.0,
            y: 0.0,
            text: "KO".to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::MiddleCenter,
            is_inverted: true,
            inverted_border: Some(0.0254),
            use_inverted_rectangle: true,
            inverted_rect_width: Some(0.254),
            inverted_rect_height: Some(0.127),
            inverted_rect_text_offset: Some(0.0508),
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        });

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("TEXT_INVRECT");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.text.len(), 1);
        let t = &decoded.text[0];
        assert!(t.is_inverted, "is_inverted must round-trip");
        assert!(
            t.use_inverted_rectangle,
            "use_inverted_rectangle must round-trip"
        );
        assert_eq!(t.inverted_border, Some(0.0254), "border must round-trip");
        assert_eq!(t.inverted_rect_width, Some(0.254), "width must round-trip");
        assert_eq!(
            t.inverted_rect_height,
            Some(0.127),
            "height must round-trip"
        );
        assert_eq!(
            t.inverted_rect_text_offset,
            Some(0.0508),
            "text offset must round-trip"
        );
    }

    #[test]
    fn binary_roundtrip_text_flags() {
        // parse_text must carry the flag word through:
        // a locked / tented text round-trips its flags.
        let mut original = Footprint::new("TEXT_FLAGS");
        original.add_text(Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 0.0,
            y: 0.0,
            text: "LOCKED".to_string(),
            height: 0.5,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::MiddleCenter,
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::LOCKED | PcbFlags::TENTING_TOP,
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        });

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("TEXT_FLAGS");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.text.len(), 1);
        assert!(decoded.text[0].flags.contains(PcbFlags::LOCKED));
        assert!(decoded.text[0].flags.contains(PcbFlags::TENTING_TOP));
    }

    #[test]
    fn library_roundtrip_text_longer_than_the_block_1_limit() {
        // Block 1 of a Text record is a Pascal SHORT string, so a text over 255
        // bytes cannot be stored inline: Altium truncates block 1 and carries
        // the full value in /WideStrings, addressed by the index in the
        // geometry block. A library Altium can author must therefore survive a
        // write -> read cycle here, rather than the save being refused.
        //
        // Two texts, so a mis-addressed index resolves to the wrong entry
        // instead of coincidentally matching.
        use std::io::Cursor;

        let long = "A".repeat(260) + "_END";
        let text_at = |content: &str, y: f64| Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 0.0,
            y,
            text: content.to_string(),
            height: 1.0,
            layer: Layer::TopOverlay,
            kind: TextKind::Stroke,
            rotation: 0.0,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::default(),
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::default(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        };

        let mut fp = Footprint::new("LONG_TEXT");
        fp.add_text(text_at(&long, 0.0));
        fp.add_text(text_at("SHORT", -2.0));
        let mut lib = PcbLib::new();
        lib.add(fp);

        let mut buffer = Cursor::new(Vec::new());
        lib.write(&mut buffer)
            .expect("a 264-character text must be writable");
        buffer.set_position(0);
        let read_back = PcbLib::read(&mut buffer).expect("read back");

        let fp = read_back.get("LONG_TEXT").expect("footprint");
        let contents: Vec<&str> = fp.text.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            contents,
            vec![long.as_str(), "SHORT"],
            "both texts must survive, the long one in full"
        );
    }

    #[test]
    fn solder_mask_from_hole_edge_round_trips() {
        // Main-block bool @125: measure mask expansion from the hole edge instead of
        // the pad edge. A default pad leaves it clear, so the byte stays identical to
        // Altium's template unless it is asked for.
        let mut original = Footprint::new("HOLE_EDGE");
        let mut from_hole = Pad::through_hole("1", 0.0, 0.0, 1.8, 1.8, 1.0);
        from_hole.solder_mask_expansion_from_hole_edge = true;
        original.add_pad(from_hole);
        original.add_pad(Pad::through_hole("2", 3.0, 0.0, 1.8, 1.8, 1.0));

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("HOLE_EDGE");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 2);
        assert!(decoded.pads[0].solder_mask_expansion_from_hole_edge);
        assert!(
            !decoded.pads[1].solder_mask_expansion_from_hole_edge,
            "a from-scratch pad measures from the pad edge"
        );
    }

    #[test]
    fn component_body_model_2d_offset_round_trips() {
        // MODEL.2D.X/Y sit in BODY_MODELLED_PARAM_KEYS, so the additional_parameters
        // passthrough skips them: before they were parsed, a non-zero offset was
        // dropped on read and the writer always emitted 0mil.
        let mut original = Footprint::new("MODEL2D");
        let mut body = ComponentBody::new("{GUID}", "MODEL.STEP");
        body.model_2d_x = 1.27;
        body.model_2d_y = -0.635;
        original.add_component_body(body);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("MODEL2D");
        reader::parse_data_stream(&mut decoded, &data, None);

        let read = &decoded.component_bodies[0];
        assert!(
            (read.model_2d_x - 1.27).abs() < 1e-4,
            "x: {}",
            read.model_2d_x
        );
        assert!(
            (read.model_2d_y + 0.635).abs() < 1e-4,
            "y: {}",
            read.model_2d_y
        );
        assert!(
            !read
                .additional_parameters
                .iter()
                .any(|(k, _)| k.starts_with("MODEL.2D.")),
            "the offset is a typed field, not a passthrough entry"
        );
    }

    #[test]
    fn jumper_id_round_trips() {
        // Main-block i16 @110-111. Zero is "no jumper" and must leave the template
        // bytes alone, so the default pad is the control.
        let mut original = Footprint::new("JUMPER");
        let mut a = Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8);
        a.jumper_id = 7;
        original.add_pad(a);
        original.add_pad(Pad::through_hole("2", 3.0, 0.0, 1.6, 1.6, 0.8));

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("JUMPER");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads[0].jumper_id, 7);
        assert_eq!(decoded.pads[1].jumper_id, 0);
    }

    #[test]
    fn testpoint_flags_round_trip_and_imply_locked() {
        // The fabrication test-point bits (0x0080 / 0x0100 in Altium's flag word)
        // must survive encode -> decode. Altium clears the unlocked bit on a pad it
        // marks as a test point, so the writer does the same and the pad decodes as
        // LOCKED as well — matching what the golden's authored pads carry.
        let mut original = Footprint::new("TP_FLAGS");
        let mut top = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        top.flags = PcbFlags::TESTPOINT_TOP;
        original.add_pad(top);
        let mut bottom = Pad::smd("2", 2.0, 0.0, 1.0, 1.0);
        bottom.flags = PcbFlags::TESTPOINT_BOTTOM;
        original.add_pad(bottom);
        original.add_pad(Pad::smd("3", 4.0, 0.0, 1.0, 1.0));

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("TP_FLAGS");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 3);
        assert!(decoded.pads[0].flags.contains(PcbFlags::TESTPOINT_TOP));
        assert!(decoded.pads[0].flags.contains(PcbFlags::LOCKED));
        assert!(!decoded.pads[0].flags.contains(PcbFlags::TESTPOINT_BOTTOM));
        assert!(decoded.pads[1].flags.contains(PcbFlags::TESTPOINT_BOTTOM));
        assert!(decoded.pads[1].flags.contains(PcbFlags::LOCKED));
        assert!(!decoded.pads[1].flags.contains(PcbFlags::TESTPOINT_TOP));
        assert!(
            decoded.pads[2].flags.is_empty(),
            "the untouched pad stays unflagged"
        );
    }

    #[test]
    fn binary_roundtrip_common_indices() {
        // A board-context Track and Text carrying a net/component association must
        // survive encode -> decode via the common-header indices (@3 net, @5
        // polygon, @7 component). Dropping them on read and hard-coding 0xFF
        // on write loses the association.
        let mut original = Footprint::new("ROUNDTRIP_INDICES");

        let mut track = Track::new(-1.0, 0.0, 1.0, 0.0, 0.25, Layer::TopLayer);
        track.net_index = 5;
        track.polygon_index = 2;
        track.component_index = 3;
        original.add_track(track);

        original.add_text(Text {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x: 0.0,
            y: 1.0,
            text: "NET".to_string(),
            height: 0.8,
            layer: Layer::TopOverlay,
            rotation: 0.0,
            kind: TextKind::Stroke,
            stroke_font: None,
            stroke_width: None,
            italic: false,
            bold: false,
            mirror: false,
            is_comment: false,
            is_designator: false,
            font_name: "Arial".to_string(),
            justification: TextJustification::MiddleCenter,
            is_inverted: false,
            inverted_border: None,
            use_inverted_rectangle: false,
            inverted_rect_width: None,
            inverted_rect_height: None,
            inverted_rect_text_offset: None,
            flags: PcbFlags::empty(),
            net_index: 9,
            polygon_index: 0xFFFF,
            component_index: 4,
            unique_id: None,
            guid: None,
            raw_geometry: None,
        });

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_INDICES");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.tracks.len(), 1);
        assert_eq!(decoded.tracks[0].net_index, 5, "track net index");
        assert_eq!(decoded.tracks[0].polygon_index, 2, "track polygon index");
        assert_eq!(
            decoded.tracks[0].component_index, 3,
            "track component index"
        );

        assert_eq!(decoded.text.len(), 1);
        assert_eq!(decoded.text[0].net_index, 9, "text net index");
        assert_eq!(
            decoded.text[0].polygon_index, 0xFFFF,
            "text polygon stays none"
        );
        assert_eq!(decoded.text[0].component_index, 4, "text component index");
    }

    #[test]
    fn binary_roundtrip_default_indices_are_none() {
        // A from-scratch primitive reads back the "none" defaults (net/polygon
        // 0xFFFF, component -1), confirming the 0xFF header bytes decode correctly.
        let mut original = Footprint::new("DEFAULT_INDICES");
        original.add_track(Track::new(0.0, 0.0, 1.0, 0.0, 0.2, Layer::TopOverlay));

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("DEFAULT_INDICES");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.tracks.len(), 1);
        assert_eq!(decoded.tracks[0].net_index, 0xFFFF);
        assert_eq!(decoded.tracks[0].polygon_index, 0xFFFF);
        assert_eq!(decoded.tracks[0].component_index, -1);
    }

    #[test]
    fn binary_roundtrip_region() {
        let mut original = Footprint::new("ROUNDTRIP_REGION");

        // Add a triangular region (similar to user's sample)
        original.add_region(Region {
            vertices: vec![
                Vertex {
                    x: -2.286,
                    y: 1.778,
                },
                Vertex {
                    x: -0.762,
                    y: 1.778,
                },
                Vertex {
                    x: -1.524,
                    y: 1.016,
                },
            ],
            layer: Layer::TopAssembly,
            ..Region::default()
        });

        // Add a rectangular region
        original.add_region(Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopCourtyard));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_REGION");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.regions.len(), 2);

        // Triangle
        assert_eq!(decoded.regions[0].vertices.len(), 3);
        assert!(approx_eq(decoded.regions[0].vertices[0].x, -2.286, 0.001));
        assert!(approx_eq(decoded.regions[0].vertices[0].y, 1.778, 0.001));
        assert!(approx_eq(decoded.regions[0].vertices[1].x, -0.762, 0.001));
        assert!(approx_eq(decoded.regions[0].vertices[2].x, -1.524, 0.001));
        assert_eq!(decoded.regions[0].layer, Layer::TopAssembly);

        // Rectangle
        assert_eq!(decoded.regions[1].vertices.len(), 4);
        assert_eq!(decoded.regions[1].layer, Layer::TopCourtyard);
    }

    #[test]
    fn binary_roundtrip_fill() {
        use super::primitives::Fill;

        let mut original = Footprint::new("ROUNDTRIP_FILL");

        // Add a simple fill rectangle
        original.add_fill(Fill::new(-2.0, -1.0, 2.0, 1.0, Layer::TopPaste));

        // Add a rotated fill
        original.add_fill(Fill {
            raw_layer_id: None,
            x1: -1.5,
            y1: -0.5,
            x2: 1.5,
            y2: 0.5,
            layer: Layer::BottomPaste,
            rotation: 45.0,
            flags: PcbFlags::empty(),
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            solder_mask_expansion: None,
            keepout_restrictions: None,
            unique_id: None,
            guid: None,
        });

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_FILL");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.fills.len(), 2);

        // First fill
        assert!(approx_eq(decoded.fills[0].x1, -2.0, 0.001));
        assert!(approx_eq(decoded.fills[0].y1, -1.0, 0.001));
        assert!(approx_eq(decoded.fills[0].x2, 2.0, 0.001));
        assert!(approx_eq(decoded.fills[0].y2, 1.0, 0.001));
        assert_eq!(decoded.fills[0].layer, Layer::TopPaste);
        assert!(approx_eq(decoded.fills[0].rotation, 0.0, 0.001));

        // Second fill (rotated)
        assert!(approx_eq(decoded.fills[1].x1, -1.5, 0.001));
        assert!(approx_eq(decoded.fills[1].y1, -0.5, 0.001));
        assert!(approx_eq(decoded.fills[1].x2, 1.5, 0.001));
        assert!(approx_eq(decoded.fills[1].y2, 0.5, 0.001));
        assert_eq!(decoded.fills[1].layer, Layer::BottomPaste);
        assert!(approx_eq(decoded.fills[1].rotation, 45.0, 0.001));
    }

    #[test]
    fn fill_extended_tail_round_trips() {
        use super::primitives::Fill;

        // Solder-mask expansion @37-40 and keepout @46 round-trip; a default fill
        // stays None (additive — byte-identical to a zero tail).
        let mut fp = Footprint::new("FILL_TAIL");
        let mut fill = Fill::new(0.0, 0.0, 2.0, 1.0, Layer::TopLayer);
        fill.solder_mask_expansion = Some(0.1);
        fill.keepout_restrictions = Some(0x05);
        fp.add_fill(fill);
        fp.add_fill(Fill::new(5.0, 0.0, 6.0, 1.0, Layer::TopOverlay));

        let data = writer::encode_data_stream(&fp).expect("encode");
        let mut decoded = Footprint::new("FILL_TAIL");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.fills.len(), 2);
        assert!(approx_eq(
            decoded.fills[0].solder_mask_expansion.unwrap(),
            0.1,
            0.001
        ));
        assert_eq!(decoded.fills[0].keepout_restrictions, Some(0x05));
        // Additive: the default fill did not gain these fields.
        assert_eq!(decoded.fills[1].solder_mask_expansion, None);
        assert_eq!(decoded.fills[1].keepout_restrictions, None);
    }

    #[test]
    fn binary_roundtrip_component_body() {
        use super::primitives::ComponentBody;

        let mut original = Footprint::new("ROUNDTRIP_COMPONENT_BODY");

        // Add a ComponentBody with typical values and an explicit outline.
        let body = ComponentBody {
            raw_layer_id: None,
            v7_layer: None,
            model_id: "{TEST-GUID-1234-5678-ABCDEFGH}".to_string(),
            identifier: String::new(),
            texture_center_x: None,
            texture_center_y: None,
            texture_size_x: None,
            texture_size_y: None,
            texture_rotation: None,
            model_name: "TEST_MODEL.step".to_string(),
            embedded: true,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 45.0,
            z_offset: 0.5,        // mm
            overall_height: 1.0,  // mm
            standoff_height: 0.1, // mm
            cavity_height: 0.3,   // mm
            layer: Layer::Top3DBody,
            outline: vec![(-2.0, 1.0), (-2.0, -1.0), (2.0, -1.0), (2.0, 1.0)],
            unique_id: None,
            guid: None,
            model_checksum: 7_654_321,
            name: " ".to_string(),
            kind: 0,
            sub_poly_index: -1,
            union_index: 0,
            is_shape_based: false,
            body_projection: 0,
            body_color_3d: 8_421_504,
            body_opacity_3d: 1.0,
            model_2d_rotation: 0.0,
            model_2d_x: 0.0,
            model_2d_y: 0.0,
            net_index: 0xFFFF,
            polygon_index: 0xFFFF,
            component_index: -1,
            additional_parameters: Vec::new(),
            param_key_order: Vec::new(),
        };
        original.add_component_body(body);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_COMPONENT_BODY");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.component_bodies.len(), 1);

        let body = &decoded.component_bodies[0];
        assert_eq!(body.model_id, "{TEST-GUID-1234-5678-ABCDEFGH}");
        assert_eq!(body.model_name, "TEST_MODEL.step");
        assert!(body.embedded);
        assert!(approx_eq(body.rotation_x, 0.0, 0.001));
        assert!(approx_eq(body.rotation_y, 0.0, 0.001));
        assert!(approx_eq(body.rotation_z, 45.0, 0.001));
        // Heights are converted to/from mils with some precision loss
        assert!(approx_eq(body.z_offset, 0.5, 0.01));
        assert!(approx_eq(body.overall_height, 1.0, 0.01));
        assert!(approx_eq(body.standoff_height, 0.1, 0.01));
        assert!(approx_eq(body.cavity_height, 0.3, 0.01));
        assert_eq!(body.layer, Layer::Top3DBody);
        // MODEL.CHECKSUM round-trips verbatim rather than being hard-coded to 0.
        assert_eq!(body.model_checksum, 7_654_321);

        // The explicit outline round-trips (4 vertices, in mm).
        assert_eq!(body.outline.len(), 4);
        assert!(approx_eq(body.outline[0].0, -2.0, 0.001));
        assert!(approx_eq(body.outline[0].1, 1.0, 0.001));
        assert!(approx_eq(body.outline[2].0, 2.0, 0.001));
        assert!(approx_eq(body.outline[2].1, -1.0, 0.001));
    }

    #[test]
    fn component_body_emits_single_block_with_outline() {
        // A footprint with two bodies must not emit stray empty blocks between
        // them (Altium reads exactly one block per body; trailing zero bytes are
        // mis-read as a bogus object-id-0 primitive — the #68 class of bug).
        let mut fp = Footprint::new("TWO_BODIES");
        fp.add_pad(Pad::smd("1", -1.0, 0.0, 0.6, 0.5));
        fp.add_pad(Pad::smd("2", 1.0, 0.0, 0.6, 0.5));
        for i in 0..2 {
            fp.add_component_body(ComponentBody {
                raw_layer_id: None,
                v7_layer: None,
                model_id: format!("{{GUID-{i}}}"),
                identifier: String::new(),
                texture_center_x: None,
                texture_center_y: None,
                texture_size_x: None,
                texture_size_y: None,
                texture_rotation: None,
                model_name: format!("M{i}.step"),
                embedded: true,
                rotation_x: 0.0,
                rotation_y: 0.0,
                rotation_z: 0.0,
                z_offset: 0.0,
                overall_height: 1.0,
                standoff_height: 0.0,
                cavity_height: 0.0,
                layer: Layer::Top3DBody,
                outline: Vec::new(), // exercise the synthesised-bbox fallback
                unique_id: None,
                guid: None,
                model_checksum: 0,
                name: " ".to_string(),
                kind: 0,
                sub_poly_index: -1,
                union_index: 0,
                is_shape_based: false,
                body_projection: 0,
                body_color_3d: 8_421_504,
                body_opacity_3d: 1.0,
                model_2d_rotation: 0.0,
                model_2d_x: 0.0,
                model_2d_y: 0.0,
                net_index: 0xFFFF,
                polygon_index: 0xFFFF,
                component_index: -1,
                additional_parameters: Vec::new(),
                param_key_order: Vec::new(),
            });
        }

        let data = writer::encode_data_stream(&fp).expect("encoding should succeed");
        let mut decoded = Footprint::new("TWO_BODIES");
        reader::parse_data_stream(&mut decoded, &data, None);

        // Both bodies survive (no desync from stray blocks), and each gets a
        // non-degenerate synthesised outline (the pad bounding box).
        assert_eq!(decoded.component_bodies.len(), 2);
        for body in &decoded.component_bodies {
            assert_eq!(body.outline.len(), 4, "body must have a non-empty outline");
        }

        // Byte-level: a body must be EXACTLY one size-prefixed block (as Altium
        // writes). This catches the stray empty blocks regardless of whether our
        // own reader tolerates them. Build a one-body footprint and walk it.
        let mut single = Footprint::new("ONE_BODY");
        single.add_component_body(ComponentBody::new("{G}", "M.step"));
        let d = writer::encode_data_stream(&single).expect("encoding should succeed");
        let name_len = u32::from_le_bytes(d[0..4].try_into().unwrap()) as usize;
        let mut off = 4 + name_len;
        assert_eq!(d[off], 0x0C, "expected ComponentBody object id");
        off += 1;
        let block_len = u32::from_le_bytes(d[off..off + 4].try_into().unwrap()) as usize;
        off += 4 + block_len;
        assert_eq!(
            off,
            d.len(),
            "ComponentBody must be a single block with no trailing empty blocks"
        );

        // The body param block carries the full key set Altium emits.
        let s = String::from_utf8_lossy(&d);
        assert!(s.contains("IDENTIFIER="), "missing IDENTIFIER key");
        assert!(s.contains("TEXTURE="), "missing TEXTURE key");
        assert_eq!(
            s.matches("ARCRESOLUTION=").count(),
            2,
            "Altium emits ARCRESOLUTION twice"
        );
    }

    #[test]
    fn models_data_record_has_no_leading_pipe() {
        // AltiumSharp and every BODY_3D golden start the record at EMBED= with no
        // leading pipe; the u32 length prefix is followed directly by 'E'.
        let models = vec![EmbeddedModel::new("{GUID}", "part.step", Vec::new())];
        let stream = writer::encode_model_data_stream(&models);
        // [u32 len][record + NUL]; first record byte (offset 4) must be 'E', not '|'.
        assert_eq!(stream[4], b'E', "Models/Data record must start at EMBED=");
        assert_ne!(
            stream[4], b'|',
            "Models/Data record must not have a leading pipe"
        );
    }

    #[test]
    fn via_is_single_321_byte_block() {
        // #113: Altium writes a via as ONE block — the 13-byte common header plus
        // the 321-byte via SubRecord-1 — matching `PcbLibWriter.WriteVia`. We used
        // to emit six pad-style blocks, which Altium misreads. A self-consistent
        // round-trip can't catch that, so assert the on-disk block structure.
        let mut fp = Footprint::new("VIA_ONLY");
        fp.add_via(Via::new(1.0, 2.0, 0.6, 0.3));
        let data = writer::encode_data_stream(&fp).expect("encode");

        // The via record is `[0x03][block_len: u32 LE][block]`; 321 == 0x0000_0141.
        let sig = [0x03u8, 0x41, 0x01, 0x00, 0x00];
        let pos = data
            .windows(sig.len())
            .position(|w| w == sig)
            .expect("via must be a single 321-byte block");
        let block = &data[pos + 5..pos + 5 + 321];
        // Common-header layer byte is MultiLayer (74) for a via.
        assert_eq!(block[0], 74, "via common header should be on MultiLayer");
        // Exactly one block — no second via sub-block follows.
        assert!(
            !data[pos + 5 + 321..].windows(sig.len()).any(|w| w == sig),
            "via should emit exactly one block, not several"
        );
    }

    #[test]
    fn track_arc_extended_tail_round_trips() {
        // #113: a track/arc's solder-mask expansion and keepout restrictions are
        // preserved on read->write rather than dropped. Additive: a
        // default primitive (None) must round-trip back to None.
        let mut fp = Footprint::new("FIDELITY");
        let mut track = Track::new(0.0, 0.0, 1.0, 0.0, 0.2, Layer::TopLayer);
        track.solder_mask_expansion = Some(0.1);
        track.keepout_restrictions = Some(0x05);
        fp.add_track(track);
        let mut arc = Arc::circle(2.0, 0.0, 0.5, 0.15, Layer::TopLayer);
        arc.solder_mask_expansion = Some(0.08);
        arc.keepout_restrictions = Some(0x03);
        fp.add_arc(arc);
        // A default track to prove additivity (None stays None).
        fp.add_track(Track::new(5.0, 0.0, 6.0, 0.0, 0.2, Layer::TopOverlay));

        let data = writer::encode_data_stream(&fp).expect("encode");
        let mut decoded = Footprint::new("FIDELITY");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.tracks.len(), 2);
        assert!(approx_eq(
            decoded.tracks[0].solder_mask_expansion.unwrap(),
            0.1,
            0.001
        ));
        assert_eq!(decoded.tracks[0].keepout_restrictions, Some(0x05));
        assert!(approx_eq(
            decoded.arcs[0].solder_mask_expansion.unwrap(),
            0.08,
            0.001
        ));
        assert_eq!(decoded.arcs[0].keepout_restrictions, Some(0x03));
        // Additive: the default track did not gain these fields.
        assert_eq!(decoded.tracks[1].solder_mask_expansion, None);
        assert_eq!(decoded.tracks[1].keepout_restrictions, None);
    }

    #[test]
    fn binary_roundtrip_via() {
        let mut original = Footprint::new("ROUNDTRIP_VIA");

        // Add a simple through via (top to bottom)
        original.add_via(Via::new(0.0, 0.0, 0.6, 0.3));

        // Add a via at different position
        original.add_via(Via::new(2.54, 1.27, 0.8, 0.4));

        // Add a blind via (top to mid layer) - though layers may map differently
        original.add_via(Via::blind(
            -1.0,
            -1.0,
            0.5,
            0.25,
            Layer::TopLayer,
            Layer::BottomLayer,
        ));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_VIA");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.vias.len(), 3);

        // First via
        assert!(approx_eq(decoded.vias[0].x, 0.0, 0.001));
        assert!(approx_eq(decoded.vias[0].y, 0.0, 0.001));
        assert!(approx_eq(decoded.vias[0].diameter, 0.6, 0.001));
        assert!(approx_eq(decoded.vias[0].hole_size, 0.3, 0.001));

        // Second via
        assert!(approx_eq(decoded.vias[1].x, 2.54, 0.001));
        assert!(approx_eq(decoded.vias[1].y, 1.27, 0.001));
        assert!(approx_eq(decoded.vias[1].diameter, 0.8, 0.001));
        assert!(approx_eq(decoded.vias[1].hole_size, 0.4, 0.001));

        // Third via (blind via)
        assert!(approx_eq(decoded.vias[2].x, -1.0, 0.001));
        assert!(approx_eq(decoded.vias[2].y, -1.0, 0.001));
        assert!(approx_eq(decoded.vias[2].diameter, 0.5, 0.001));
        assert!(approx_eq(decoded.vias[2].hole_size, 0.25, 0.001));
        assert_eq!(decoded.vias[2].from_layer, Layer::TopLayer);
        assert_eq!(decoded.vias[2].to_layer, Layer::BottomLayer);
    }

    #[test]
    fn via_drill_pair_and_hole_edge_round_trip() {
        use super::primitives::DrillLayerPairType;

        // SubRecord-1 @258 (mask measured from the hole edge) and @312 (drill-pair
        // classification). Both are 0 in Altium's template, so a default via must stay
        // byte-identical while a configured one survives the round trip.
        let mut original = Footprint::new("VIA_DRILL");
        let mut configured = Via::new(0.0, 0.0, 0.6, 0.3);
        configured.solder_mask_expansion_from_hole_edge = true;
        configured.drill_layer_pair_type = DrillLayerPairType::Mid;
        original.add_via(configured);
        original.add_via(Via::new(2.0, 0.0, 0.6, 0.3));

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("VIA_DRILL");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.vias.len(), 2);
        assert!(decoded.vias[0].solder_mask_expansion_from_hole_edge);
        assert_eq!(
            decoded.vias[0].drill_layer_pair_type,
            DrillLayerPairType::Mid
        );
        assert!(!decoded.vias[1].solder_mask_expansion_from_hole_edge);
        assert_eq!(
            decoded.vias[1].drill_layer_pair_type,
            DrillLayerPairType::Through,
            "a from-scratch via is a through via"
        );

        // Every byte value maps back to itself, including the unknown-value fallback.
        for (id, kind) in [
            (0, DrillLayerPairType::Through),
            (1, DrillLayerPairType::BlindBuriedStart),
            (2, DrillLayerPairType::Mid),
            (3, DrillLayerPairType::End),
        ] {
            assert_eq!(DrillLayerPairType::from_id(id), kind);
            assert_eq!(kind.to_id(), id);
        }
        assert_eq!(
            DrillLayerPairType::from_id(99),
            DrillLayerPairType::Through,
            "an unknown classification reads as a through via"
        );
    }

    #[test]
    fn via_solder_mask_mode_round_trips() {
        use super::primitives::MaskExpansionMode;

        // A fresh via carries None (byte 66 = 0, `eCacheInvalid`), matching what Altium
        // writes for a factory via: the cached expansion is stale, so Altium takes the
        // value from the design rule. A Manual via must round-trip.
        let mut original = Footprint::new("VIA_MASK_MODE");
        assert_eq!(
            Via::new(0.0, 0.0, 0.6, 0.3).solder_mask_expansion_mode,
            MaskExpansionMode::None
        );
        original.add_via(Via::new(0.0, 0.0, 0.6, 0.3));
        let mut manual = Via::new(1.0, 1.0, 0.6, 0.3);
        manual.solder_mask_expansion_mode = MaskExpansionMode::Manual;
        original.add_via(manual);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("VIA_MASK_MODE");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.vias.len(), 2);
        assert_eq!(
            decoded.vias[0].solder_mask_expansion_mode,
            MaskExpansionMode::None
        );
        assert_eq!(
            decoded.vias[1].solder_mask_expansion_mode,
            MaskExpansionMode::Manual
        );
    }

    #[test]
    fn via_thermal_power_plane_fields_round_trip() {
        use super::primitives::PowerPlaneConnectStyle;

        // PR-7: the via flag word (tenting/keepout/locked), power-plane connection,
        // paste-mask expansion and net index all survive encode -> decode.
        let mut original = Footprint::new("VIA_PP");
        let mut via = Via::new(1.0, 2.0, 0.8, 0.4);
        via.flags =
            PcbFlags::TENTING_TOP | PcbFlags::TENTING_BOTTOM | PcbFlags::KEEPOUT | PcbFlags::LOCKED;
        via.power_plane_connect_style = PowerPlaneConnectStyle::Direct;
        via.power_plane_relief_expansion = 0.6;
        via.power_plane_clearance = 0.7;
        via.paste_mask_expansion = 0.05;
        via.net_index = 42;
        original.add_via(via);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("VIA_PP");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.vias.len(), 1);
        let d = &decoded.vias[0];
        assert!(d.flags.contains(PcbFlags::TENTING_TOP));
        assert!(d.flags.contains(PcbFlags::TENTING_BOTTOM));
        assert!(d.flags.contains(PcbFlags::KEEPOUT));
        assert!(d.flags.contains(PcbFlags::LOCKED));
        assert_eq!(d.power_plane_connect_style, PowerPlaneConnectStyle::Direct);
        assert!(approx_eq(d.power_plane_relief_expansion, 0.6, 0.001));
        assert!(approx_eq(d.power_plane_clearance, 0.7, 0.001));
        assert!(approx_eq(d.paste_mask_expansion, 0.05, 0.001));
        assert_eq!(d.net_index, 42);
    }

    #[test]
    fn default_via_defaults_match_template() {
        use super::primitives::PowerPlaneConnectStyle;

        // A from-scratch via must default to exactly the VIA_SR1_TEMPLATE constants
        // so it serialises byte-identically (the readability oracle exercises vias).
        let via = Via::new(0.0, 0.0, 0.6, 0.3);
        assert_eq!(via.flags, PcbFlags::empty()); // flag word 0x000C (saved|unlocked)
        assert_eq!(via.net_index, 0xFFFF); // @3-4 = 0xFFFF (no net)
        assert_eq!(
            via.power_plane_connect_style,
            PowerPlaneConnectStyle::Relief // @31 = 0
        );
        assert!(approx_eq(via.power_plane_relief_expansion, 0.508, 1e-9)); // @42 = 200000
        assert!(approx_eq(via.power_plane_clearance, 0.508, 1e-9)); // @46 = 200000
        assert!(approx_eq(via.paste_mask_expansion, 0.0, 1e-9)); // @50 = 0

        // Encode and confirm the SubRecord-1 bytes equal the template at each offset.
        let mut fp = Footprint::new("VIA_TEMPLATE");
        fp.add_via(via);
        let data = writer::encode_data_stream(&fp).expect("encode");
        let sig = [0x03u8, 0x41, 0x01, 0x00, 0x00];
        let pos = data
            .windows(sig.len())
            .position(|w| w == sig)
            .expect("via block");
        let block = &data[pos + 5..pos + 5 + 321];
        assert_eq!(&block[1..3], &[0x0C, 0x00]); // flags word
        assert_eq!(&block[3..5], &[0xFF, 0xFF]); // net index
        assert_eq!(block[31], 0x00); // power-plane connect style
        assert_eq!(&block[42..46], &200_000i32.to_le_bytes()); // relief expansion
        assert_eq!(&block[46..50], &200_000i32.to_le_bytes()); // plane clearance
        assert_eq!(&block[50..54], &0i32.to_le_bytes()); // paste-mask expansion
                                                         // PR-8: default drill tolerances stay the 0x7FFFFFFF "unset" sentinel @291/@295.
        assert_eq!(&block[291..295], &i32::MAX.to_le_bytes());
        assert_eq!(&block[295..299], &i32::MAX.to_le_bytes());
    }

    #[test]
    fn from_scratch_vias_carry_nil_identity_guids() {
        // The in-record identity GUID slots @259-290 are ZEROS in every
        // AD-authored library via (the golden), and a via read from a file
        // replays its whole block verbatim. We used to invent two fresh GUIDs
        // per save, which made the record change on every write; nil is what
        // Altium itself writes.
        let mut fp = Footprint::new("VIA_GUIDS");
        fp.add_via(Via::new(0.0, 0.0, 0.6, 0.3));
        let data = writer::encode_data_stream(&fp).expect("encode");

        let sig = [0x03u8, 0x41, 0x01, 0x00, 0x00];
        let first = data
            .windows(sig.len())
            .position(|w| w == sig)
            .expect("via block");
        let via = &data[first + 5..first + 5 + 321];
        assert_eq!(
            &via[259..291],
            &[0u8; 32],
            "from-scratch identity GUID slots are nil, as AD writes them"
        );
    }

    #[test]
    fn binary_roundtrip_via_tolerances() {
        // PR-8: a via with non-default drill tolerances must survive encode -> decode.
        // Vias carry no slot geometry.
        let mut original = Footprint::new("VIA_TOL");
        let mut via = Via::new(1.0, 2.0, 0.8, 0.4);
        via.hole_positive_tolerance = Some(0.05);
        via.hole_negative_tolerance = Some(0.02);
        original.add_via(via);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("VIA_TOL");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.vias.len(), 1);
        let d = &decoded.vias[0];
        assert!(approx_eq(d.hole_positive_tolerance.unwrap(), 0.05, 0.001));
        assert!(approx_eq(d.hole_negative_tolerance.unwrap(), 0.02, 0.001));
    }

    #[test]
    fn via_default_tolerances_unset() {
        // A from-scratch via leaves both drill tolerances unset (None -> sentinel), so
        // it serialises byte-identically to the template.
        let via = Via::new(0.0, 0.0, 0.6, 0.3);
        assert_eq!(via.hole_positive_tolerance, None);
        assert_eq!(via.hole_negative_tolerance, None);
    }

    #[test]
    fn pad_mask_expansion_mode_round_trips() {
        use super::primitives::MaskExpansionMode;

        // A fresh pad carries None (bytes 101/102 = 0, `eCacheInvalid`), matching what
        // Altium writes for a factory pad: the cached expansion is stale, so Altium
        // takes the value from the design rule. A Manual pad must round-trip.
        let fresh = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        assert_eq!(fresh.paste_mask_expansion_mode, MaskExpansionMode::None);
        assert_eq!(fresh.solder_mask_expansion_mode, MaskExpansionMode::None);

        let mut original = Footprint::new("PAD_MASK_MODE");
        original.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        let mut manual = Pad::smd("2", 1.0, 1.0, 1.0, 1.0);
        manual.paste_mask_expansion_mode = MaskExpansionMode::Manual;
        manual.solder_mask_expansion_mode = MaskExpansionMode::Manual;
        original.add_pad(manual);

        let data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("PAD_MASK_MODE");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 2);
        assert_eq!(
            decoded.pads[0].paste_mask_expansion_mode,
            MaskExpansionMode::None
        );
        assert_eq!(
            decoded.pads[0].solder_mask_expansion_mode,
            MaskExpansionMode::None
        );
        assert_eq!(
            decoded.pads[1].paste_mask_expansion_mode,
            MaskExpansionMode::Manual
        );
        assert_eq!(
            decoded.pads[1].solder_mask_expansion_mode,
            MaskExpansionMode::Manual
        );
    }

    #[test]
    fn via_solder_mask_back_round_trips() {
        // A default via leaves the back mask `None` (back@242 == front@54), and an
        // asymmetric via must round-trip a distinct back-face expansion. Tests the
        // deterministic encode_via -> parse_via path (a full library write embeds
        // fresh UUIDs/timestamps, so it is not byte-deterministic).
        let mut original = Footprint::new("VIA_SMASK");

        // Default via: back is None and must survive the round-trip as None.
        original.add_via(Via::new(0.0, 0.0, 0.6, 0.3));

        // Asymmetric via: distinct front/back mask expansion.
        let mut asym = Via::new(2.54, 0.0, 0.6, 0.3);
        asym.solder_mask_expansion = 0.1; // front 0.1 mm
        asym.solder_mask_expansion_back = Some(0.2); // back 0.2 mm
        original.add_via(asym);

        let mut data = writer::encode_data_stream(&original).expect("encode");
        let mut decoded = Footprint::new("VIA_SMASK");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.vias.len(), 2);
        assert_eq!(decoded.vias[0].solder_mask_expansion_back, None);
        assert_eq!(decoded.vias[1].solder_mask_expansion_back, Some(0.2));
        // Front face unaffected.
        assert!((decoded.vias[1].solder_mask_expansion - 0.1).abs() < 1e-6);

        // Idempotent re-encode proves a byte-stable round-trip for vias — apart
        // from the two per-via identity GUIDs @259/@275, which are freshly random
        // on every encode (PR-R6, matching the pad). Zero those ranges in each via
        // block so the comparison covers every deterministic byte.
        let mut data2 = writer::encode_data_stream(&decoded).expect("re-encode");
        let mask_via_guids = |bytes: &mut [u8]| {
            let sig = [0x03u8, 0x41, 0x01, 0x00, 0x00]; // via record: [0x03][len=321]
            let mut search = 0;
            while let Some(rel) = bytes[search..].windows(sig.len()).position(|w| w == sig) {
                let block = search + rel + 5;
                bytes[block + 259..block + 291].fill(0);
                search = block + 321;
            }
        };
        mask_via_guids(&mut data);
        mask_via_guids(&mut data2);
        assert_eq!(
            data, data2,
            "via encode is not byte-stable outside the identity GUIDs"
        );
    }

    #[test]
    fn binary_roundtrip_mixed_with_vias() {
        let mut original = Footprint::new("ROUNDTRIP_MIXED_VIA");

        // Add various primitives including vias
        original.add_pad(Pad::smd("1", -1.0, 0.0, 0.6, 0.5));
        original.add_pad(Pad::smd("2", 1.0, 0.0, 0.6, 0.5));
        original.add_via(Via::new(0.0, 0.0, 0.5, 0.25));
        original.add_track(Track::new(-1.5, -0.3, 1.5, -0.3, 0.12, Layer::TopOverlay));

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_MIXED_VIA");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 2);
        assert_eq!(decoded.vias.len(), 1);
        assert_eq!(decoded.tracks.len(), 1);

        // Verify via data
        assert!(approx_eq(decoded.vias[0].x, 0.0, 0.001));
        assert!(approx_eq(decoded.vias[0].diameter, 0.5, 0.001));
        assert!(approx_eq(decoded.vias[0].hole_size, 0.25, 0.001));
    }

    #[test]
    fn binary_roundtrip_pad_advanced_features() {
        use super::primitives::MaskExpansionMode;

        let mut original = Footprint::new("ROUNDTRIP_PAD_ADVANCED");

        // Create a pad with hole shape and mask expansion
        let mut pad_with_square_hole = Pad::through_hole("1", -2.54, 0.0, 1.8, 1.8, 1.0);
        pad_with_square_hole.hole_shape = HoleShape::Square;
        pad_with_square_hole.solder_mask_expansion = Some(0.1);
        pad_with_square_hole.solder_mask_expansion_mode = MaskExpansionMode::Manual;
        original.add_pad(pad_with_square_hole);

        // Create a pad with slot hole
        let mut pad_with_slot = Pad::through_hole("2", 0.0, 0.0, 2.0, 1.5, 0.8);
        pad_with_slot.hole_shape = HoleShape::Slot;
        pad_with_slot.paste_mask_expansion = Some(-0.05);
        pad_with_slot.paste_mask_expansion_mode = MaskExpansionMode::Manual;
        original.add_pad(pad_with_slot);

        // Create a simple pad with round hole (default)
        let pad_with_round_hole = Pad::through_hole("3", 2.54, 0.0, 1.5, 1.5, 0.8);
        original.add_pad(pad_with_round_hole);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_PAD_ADVANCED");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 3);

        // Non-round hole shapes now round-trip via the 596-byte size/shape block
        // (hole type at offset 262), alongside the main-block mask-expansion.

        // Pad 1: Square hole + solder mask expansion
        assert_eq!(decoded.pads[0].designator, "1");
        assert_eq!(decoded.pads[0].hole_shape, HoleShape::Square);
        assert!(decoded.pads[0].solder_mask_expansion.is_some());
        assert!(approx_eq(
            decoded.pads[0].solder_mask_expansion.unwrap(),
            0.1,
            0.001
        ));
        assert_eq!(
            decoded.pads[0].solder_mask_expansion_mode,
            MaskExpansionMode::Manual
        );

        // Pad 2: Slot hole + paste mask expansion
        assert_eq!(decoded.pads[1].designator, "2");
        assert_eq!(decoded.pads[1].hole_shape, HoleShape::Slot);
        assert!(decoded.pads[1].paste_mask_expansion.is_some());
        assert!(approx_eq(
            decoded.pads[1].paste_mask_expansion.unwrap(),
            -0.05,
            0.001
        ));
        assert_eq!(
            decoded.pads[1].paste_mask_expansion_mode,
            MaskExpansionMode::Manual
        );

        // Pad 3: Default round hole (empty Block 5)
        assert_eq!(decoded.pads[2].designator, "3");
        assert_eq!(decoded.pads[2].hole_shape, HoleShape::Round);
    }

    #[test]
    fn binary_roundtrip_pad_stack_modes() {
        let mut original = Footprint::new("ROUNDTRIP_STACK_MODES");

        // Pad with Simple stack mode (using Rectangle shape to avoid FullStack upgrade)
        // Note: RoundedRectangle pads automatically get FullStack to preserve corner radius
        let mut pad_simple = Pad::smd("1", -2.54, 0.0, 1.0, 0.5);
        pad_simple.shape = PadShape::Rectangle;
        assert_eq!(pad_simple.stack_mode, PadStackMode::Simple);
        original.add_pad(pad_simple);

        // Pad with TopMiddleBottom stack mode
        let mut pad_tmb = Pad::through_hole("2", 0.0, 0.0, 1.5, 1.5, 0.8);
        pad_tmb.stack_mode = PadStackMode::TopMiddleBottom;
        original.add_pad(pad_tmb);

        // Pad with FullStack stack mode
        let mut pad_full = Pad::through_hole("3", 2.54, 0.0, 1.8, 1.8, 1.0);
        pad_full.stack_mode = PadStackMode::FullStack;
        original.add_pad(pad_full);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_STACK_MODES");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 3);

        // Verify stack modes preserved
        assert_eq!(decoded.pads[0].stack_mode, PadStackMode::Simple);
        assert_eq!(decoded.pads[1].stack_mode, PadStackMode::TopMiddleBottom);
        assert_eq!(decoded.pads[2].stack_mode, PadStackMode::FullStack);
    }

    #[test]
    fn binary_roundtrip_pad_corner_radius() {
        let mut original = Footprint::new("ROUNDTRIP_CORNER_RADIUS");

        // SMD pad with explicit corner radius
        let mut pad_with_radius = Pad::smd("1", 0.0, 0.0, 2.0, 1.0);
        pad_with_radius.shape = PadShape::RoundedRectangle;
        pad_with_radius.corner_radius_percent = Some(25);
        // Setting corner radius requires FullStack mode
        pad_with_radius.stack_mode = PadStackMode::FullStack;
        original.add_pad(pad_with_radius);

        // Simple SMD pad with an EXPLICIT corner radius: now round-trips via the
        // 596-byte size/shape block (no FullStack needed).
        let mut pad_simple_radius = Pad::smd("2", 2.54, 0.0, 1.5, 0.8);
        pad_simple_radius.corner_radius_percent = Some(30);
        original.add_pad(pad_simple_radius);

        // Rectangle pad (no corner radius needed)
        let mut pad_no_radius = Pad::smd("3", 5.08, 0.0, 1.5, 0.8);
        pad_no_radius.shape = PadShape::Rectangle;
        original.add_pad(pad_no_radius);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_CORNER_RADIUS");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 3);

        // Verify explicit corner radius preserved
        assert_eq!(decoded.pads[0].corner_radius_percent, Some(25));
        assert_eq!(decoded.pads[0].stack_mode, PadStackMode::FullStack);

        // Simple pad's explicit corner radius round-trips without FullStack.
        assert_eq!(decoded.pads[1].corner_radius_percent, Some(30));
        assert_eq!(decoded.pads[1].stack_mode, PadStackMode::Simple);
        assert_eq!(decoded.pads[1].shape, PadShape::RoundedRectangle);

        // Rectangle pad has no corner radius
        assert_eq!(decoded.pads[2].corner_radius_percent, None);
        assert_eq!(decoded.pads[2].stack_mode, PadStackMode::Simple);
    }

    #[test]
    fn binary_roundtrip_per_layer_pad_data() {
        let mut original = Footprint::new("ROUNDTRIP_PER_LAYER");

        // Create a pad with per-layer data
        let mut pad = Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8);
        pad.stack_mode = PadStackMode::FullStack;

        // Set up per-layer sizes (32 layers)
        let mut sizes = vec![(1.6, 1.6); 32];
        sizes[0] = (1.8, 1.8); // Top layer larger
        sizes[1] = (1.4, 1.4); // Bottom layer smaller
        pad.per_layer_sizes = Some(sizes);

        // Set up per-layer shapes
        let mut shapes = vec![PadShape::Round; 32];
        shapes[0] = PadShape::RoundedRectangle; // Top layer rounded rect
        pad.per_layer_shapes = Some(shapes);

        // Set up per-layer corner radii
        let mut radii = vec![0_u8; 32];
        radii[0] = 50; // Top layer 50% corner radius
        pad.per_layer_corner_radii = Some(radii);

        // Set up per-layer offsets
        let mut offsets = vec![(0.0, 0.0); 32];
        offsets[0] = (0.1, 0.05); // Top layer offset from hole centre
        pad.per_layer_offsets = Some(offsets);

        original.add_pad(pad);

        let data = writer::encode_data_stream(&original).expect("encoding should succeed");
        let mut decoded = Footprint::new("ROUNDTRIP_PER_LAYER");
        reader::parse_data_stream(&mut decoded, &data, None);

        assert_eq!(decoded.pads.len(), 1);
        let decoded_pad = &decoded.pads[0];

        // Verify stack mode
        assert_eq!(decoded_pad.stack_mode, PadStackMode::FullStack);

        // Verify per-layer sizes
        assert!(decoded_pad.per_layer_sizes.is_some());
        let decoded_sizes = decoded_pad.per_layer_sizes.as_ref().unwrap();
        assert_eq!(decoded_sizes.len(), 32);
        assert!(approx_eq(decoded_sizes[0].0, 1.8, 0.001)); // Top X
        assert!(approx_eq(decoded_sizes[0].1, 1.8, 0.001)); // Top Y
        assert!(approx_eq(decoded_sizes[1].0, 1.4, 0.001)); // Bottom X
        assert!(approx_eq(decoded_sizes[1].1, 1.4, 0.001)); // Bottom Y

        // Verify per-layer shapes
        assert!(decoded_pad.per_layer_shapes.is_some());
        let decoded_shapes = decoded_pad.per_layer_shapes.as_ref().unwrap();
        assert_eq!(decoded_shapes.len(), 32);
        assert_eq!(decoded_shapes[0], PadShape::RoundedRectangle);
        assert_eq!(decoded_shapes[1], PadShape::Round);

        // Verify per-layer corner radii
        assert!(decoded_pad.per_layer_corner_radii.is_some());
        let decoded_radii = decoded_pad.per_layer_corner_radii.as_ref().unwrap();
        assert_eq!(decoded_radii.len(), 32);
        assert_eq!(decoded_radii[0], 50);
        assert_eq!(decoded_radii[1], 0);

        // Verify per-layer offsets
        assert!(decoded_pad.per_layer_offsets.is_some());
        let decoded_offsets = decoded_pad.per_layer_offsets.as_ref().unwrap();
        assert_eq!(decoded_offsets.len(), 32);
        assert!(approx_eq(decoded_offsets[0].0, 0.1, 0.001));
        assert!(approx_eq(decoded_offsets[0].1, 0.05, 0.001));
        assert!(approx_eq(decoded_offsets[1].0, 0.0, 0.001));
        assert!(approx_eq(decoded_offsets[1].1, 0.0, 0.001));
    }

    // =========================================================================
    // UniqueID Roundtrip Tests
    // =========================================================================

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn unique_id_parse_stream() {
        // Test parsing the UniqueIDPrimitiveInformation stream format
        let mut test_data = Vec::new();

        // Record 1: |PRIMITIVEINDEX=1|PRIMITIVEOBJECTID=Pad|UNIQUEID=QHHMRSCB
        let record1 = b"|PRIMITIVEINDEX=1|PRIMITIVEOBJECTID=Pad|UNIQUEID=QHHMRSCB";
        test_data.extend_from_slice(&(record1.len() as u32).to_le_bytes());
        test_data.extend_from_slice(record1);

        // Record 2: |PRIMITIVEINDEX=2|PRIMITIVEOBJECTID=Pad|UNIQUEID=ABCD1234
        let record2 = b"|PRIMITIVEINDEX=2|PRIMITIVEOBJECTID=Pad|UNIQUEID=ABCD1234";
        test_data.extend_from_slice(&(record2.len() as u32).to_le_bytes());
        test_data.extend_from_slice(record2);

        // Record 3: |PRIMITIVEINDEX=1|PRIMITIVEOBJECTID=Track|UNIQUEID=WXYZ9876
        let record3 = b"|PRIMITIVEINDEX=1|PRIMITIVEOBJECTID=Track|UNIQUEID=WXYZ9876";
        test_data.extend_from_slice(&(record3.len() as u32).to_le_bytes());
        test_data.extend_from_slice(record3);

        let entries = reader::parse_unique_id_stream(&test_data);

        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].primitive_index, 1);
        assert_eq!(entries[0].primitive_type, "Pad");
        assert_eq!(entries[0].unique_id, "QHHMRSCB");

        assert_eq!(entries[1].primitive_index, 2);
        assert_eq!(entries[1].primitive_type, "Pad");
        assert_eq!(entries[1].unique_id, "ABCD1234");

        assert_eq!(entries[2].primitive_index, 1);
        assert_eq!(entries[2].primitive_type, "Track");
        assert_eq!(entries[2].unique_id, "WXYZ9876");
    }

    #[test]
    fn unique_id_encode_stream() {
        // Create a footprint with unique IDs
        let mut footprint = Footprint::new("TEST_UNIQUE_ID");

        let mut pad1 = Pad::smd("1", -0.5, 0.0, 0.6, 0.5);
        pad1.unique_id = Some("UID00001".to_string());
        footprint.add_pad(pad1);

        let mut pad2 = Pad::smd("2", 0.5, 0.0, 0.6, 0.5);
        pad2.unique_id = Some("UID00002".to_string());
        footprint.add_pad(pad2);

        let mut track = Track::new(-1.0, 0.0, 1.0, 0.0, 0.15, Layer::TopOverlay);
        track.unique_id = Some("TRACK001".to_string());
        footprint.add_track(track);

        // Encode the unique ID stream
        let uid_data = writer::encode_unique_id_stream(&footprint);
        assert!(uid_data.is_some());

        // Parse it back
        let entries = reader::parse_unique_id_stream(&uid_data.unwrap());

        assert_eq!(entries.len(), 3);

        // Find Pad entries
        let pad_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.primitive_type == "Pad")
            .collect();
        assert_eq!(pad_entries.len(), 2);
        assert_eq!(pad_entries[0].unique_id, "UID00001");
        assert_eq!(pad_entries[1].unique_id, "UID00002");

        // Find Track entry
        let track_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.primitive_type == "Track")
            .collect();
        assert_eq!(track_entries.len(), 1);
        assert_eq!(track_entries[0].unique_id, "TRACK001");
    }

    #[test]
    fn unique_id_apply_to_footprint() {
        // Create a footprint without unique IDs
        let mut footprint = Footprint::new("TEST_APPLY");
        footprint.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        footprint.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
        footprint.add_track(Track::new(-1.0, 0.0, 1.0, 0.0, 0.15, Layer::TopOverlay));

        // Global Data-stream ordinals: no arcs, so the two pads are 0 and 1, and the
        // track (after the pads/vias slot) is 2.
        let entries = vec![
            reader::UniqueIdEntry {
                primitive_index: 0,
                primitive_type: "Pad".to_string(),
                unique_id: "PADUID01".to_string(),
            },
            reader::UniqueIdEntry {
                primitive_index: 1,
                primitive_type: "Pad".to_string(),
                unique_id: "PADUID02".to_string(),
            },
            reader::UniqueIdEntry {
                primitive_index: 2,
                primitive_type: "Track".to_string(),
                unique_id: "TRKUID01".to_string(),
            },
        ];

        // Apply unique IDs
        reader::apply_unique_ids(&mut footprint, &entries);

        // Verify
        assert_eq!(footprint.pads[0].unique_id, Some("PADUID01".to_string()));
        assert_eq!(footprint.pads[1].unique_id, Some("PADUID02".to_string()));
        assert_eq!(footprint.tracks[0].unique_id, Some("TRKUID01".to_string()));
    }

    #[test]
    fn unique_id_global_ordinal_round_trip() {
        // A real footprint often has a silkscreen arc before the pads, so the first
        // pad is PRIMITIVEINDEX=1 (a single global, 0-based, Data-stream ordinal),
        // never 0 — the arc occupies ordinal 0. This locks the writer/reader contract.
        let mut fp = Footprint::new("RT_UID");
        let mut arc = Arc::circle(0.0, 0.0, 0.5, 0.1, Layer::TopOverlay);
        arc.unique_id = Some("ARCUID01".to_string());
        fp.add_arc(arc);
        let mut p1 = Pad::smd("1", -0.5, 0.0, 0.6, 0.5);
        p1.unique_id = Some("PADUID01".to_string());
        fp.add_pad(p1);
        let mut p2 = Pad::smd("2", 0.5, 0.0, 0.6, 0.5);
        p2.unique_id = Some("PADUID02".to_string());
        fp.add_pad(p2);

        let entries =
            reader::parse_unique_id_stream(&writer::encode_unique_id_stream(&fp).unwrap());

        // Arc=0, then the two pads at 1 and 2 (never 0).
        let arc_e = entries.iter().find(|e| e.primitive_type == "Arc").unwrap();
        assert_eq!(arc_e.primitive_index, 0);
        let mut pad_idx: Vec<usize> = entries
            .iter()
            .filter(|e| e.primitive_type == "Pad")
            .map(|e| e.primitive_index)
            .collect();
        pad_idx.sort_unstable();
        assert_eq!(pad_idx, vec![1, 2]);

        // Round-trip onto a fresh, id-less footprint of identical shape.
        let mut fresh = Footprint::new("RT_UID");
        fresh.add_arc(Arc::circle(0.0, 0.0, 0.5, 0.1, Layer::TopOverlay));
        fresh.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        fresh.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
        reader::apply_unique_ids(&mut fresh, &entries);
        assert_eq!(fresh.arcs[0].unique_id.as_deref(), Some("ARCUID01"));
        assert_eq!(fresh.pads[0].unique_id.as_deref(), Some("PADUID01"));
        assert_eq!(fresh.pads[1].unique_id.as_deref(), Some("PADUID02"));
    }

    #[test]
    fn unique_id_via_survives_stream_round_trip() {
        // PR-R1: a Via's identity GUID survives the UniqueIDs stream encode ->
        // decode -> apply round-trip (the write-tool path now passes it through).
        use crate::altium::pcblib::Via;
        let mut fp = Footprint::new("RT_VIA_UID");
        let mut via = Via::new(0.0, 0.0, 0.6, 0.3);
        via.unique_id = Some("VIAUID42".to_string());
        fp.add_via(via);

        let entries =
            reader::parse_unique_id_stream(&writer::encode_unique_id_stream(&fp).unwrap());
        // Apply onto a fresh, id-less footprint of identical shape.
        let mut fresh = Footprint::new("RT_VIA_UID");
        fresh.add_via(Via::new(0.0, 0.0, 0.6, 0.3));
        reader::apply_unique_ids(&mut fresh, &entries);
        assert_eq!(fresh.vias[0].unique_id.as_deref(), Some("VIAUID42"));
    }

    #[test]
    fn unique_id_no_primitives_with_ids() {
        // Create a footprint without unique IDs
        let mut footprint = Footprint::new("TEST_NO_IDS");
        footprint.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        footprint.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));

        // Encode should return None since no primitives have unique IDs
        let uid_data = writer::encode_unique_id_stream(&footprint);
        assert!(uid_data.is_none());
    }

    #[test]
    fn unique_id_partial_primitives() {
        // Create a footprint where only some primitives have unique IDs
        let mut footprint = Footprint::new("TEST_PARTIAL");

        let mut pad1 = Pad::smd("1", -0.5, 0.0, 0.6, 0.5);
        pad1.unique_id = Some("ONLYTHIS".to_string());
        footprint.add_pad(pad1);

        // Pad 2 has no unique ID
        footprint.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));

        // Encode
        let uid_data = writer::encode_unique_id_stream(&footprint);
        assert!(uid_data.is_some());

        // Parse back
        let entries = reader::parse_unique_id_stream(&uid_data.unwrap());

        // Should only have 1 entry (the pad with the unique ID)
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].primitive_index, 0); // 0-based index
        assert_eq!(entries[0].unique_id, "ONLYTHIS");
    }

    #[test]
    fn wrong_file_type_schlib_as_pcblib() {
        use std::io::Cursor;

        // Create a SchLib file in memory
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut cfb = cfb::CompoundFile::create(&mut buffer).expect("create cfb");

            // Write a SchLib FileHeader (ASCII, just pipe-delimited - PcbLib expects ASCII format)
            let header = "|HEADER=Protel for Windows - Schematic Library Editor Binary File Version 5.0|COMPCOUNT=0|";
            let mut stream = cfb.create_stream("/FileHeader").expect("create stream");
            std::io::Write::write_all(&mut stream, header.as_bytes()).expect("write header");
        }

        // Try to read it as PcbLib - should fail with WrongFileType
        buffer.set_position(0);
        let result = PcbLib::read(buffer);

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("Wrong file type"),
            "Expected 'Wrong file type' error, got: {err_str}"
        );
        assert!(
            err_str.contains("expected PcbLib"),
            "Expected 'expected PcbLib' in error, got: {err_str}"
        );
    }
    /// The TPA6130A2 field failure, encoded. TI's RTJ0020D land drawing marks
    /// (3.8) as pad row CENTRE-to-CENTRE; reading it as outer-edge-to-outer-edge
    /// puts the 20 perimeter pads at 1.6 mm instead of 1.9 mm, whose inner edges
    /// (1.30) then collide with the correctly-sized 2.7 mm exposed pad (1.35).
    /// The library passed every integrity check while shorting all 21 pads.
    #[test]
    fn overlapping_pad_pairs_catches_undersized_qfn_land() {
        fn wqfn20(centre: f64) -> Footprint {
            let mut fp = Footprint::new("QFN50P400X400X80-21N");
            let mut push = |des: &str, x: f64, y: f64, w: f64, h: f64| {
                fp.pads.push(Pad::smd(des, x, y, w, h));
            };
            for k in 0..5 {
                let o = f64::from(k).mul_add(-0.5, 1.0);
                push(&format!("{}", k + 1), -centre, o, 0.6, 0.24);
                push(&format!("{}", k + 6), -o, -centre, 0.24, 0.6);
                push(&format!("{}", k + 11), centre, -o, 0.6, 0.24);
                push(&format!("{}", k + 16), o, centre, 0.24, 0.6);
            }
            push("21", 0.0, 0.0, 2.7, 2.7); // exposed pad
            fp
        }

        // As shipped: every perimeter pad welded to the EP by 0.05 mm.
        let bad = wqfn20(1.6).overlapping_pad_pairs();
        assert_eq!(bad.len(), 20, "all 20 pads must be reported against the EP");
        for &(_, j, ox, oy) in &bad {
            assert_eq!(j, 20, "the EP (last pad) is the other half of every pair");
            // 0.05 mm along the pad's long axis, the pad's own 0.24 mm across it;
            // which axis is which flips between the side rows and the top/bottom
            // rows, so compare the unordered pair.
            let (lo, hi) = if ox < oy { (ox, oy) } else { (oy, ox) };
            assert!(
                (lo - 0.05).abs() < 1e-9,
                "short overlap {lo} (from {ox}x{oy})"
            );
            assert!(
                (hi - 0.24).abs() < 1e-9,
                "long overlap {hi} (from {ox}x{oy})"
            );
        }

        // Corrected centres: 0.25 mm clearance, nothing reported.
        assert!(
            wqfn20(1.9).overlapping_pad_pairs().is_empty(),
            "correct land pattern must be clean"
        );
    }

    #[test]
    fn overlapping_pad_pairs_excludes_legitimate_constructions() {
        let base = |des: &str, x: f64, layer: Layer| {
            let mut p = Pad::smd(des, x, 0.0, 1.0, 1.0);
            p.layer = layer;
            p
        };

        // Same designator == same net: stacking is a normal compound land.
        let mut fp = Footprint::new("STACKED");
        fp.pads.push(base("1", 0.0, Layer::TopLayer));
        fp.pads.push(base("1", 0.4, Layer::TopLayer));
        assert!(fp.overlapping_pad_pairs().is_empty(), "same designator");

        // Opposite sides never short, even at identical coordinates.
        let mut fp = Footprint::new("OPPOSITE");
        fp.pads.push(base("1", 0.0, Layer::TopLayer));
        fp.pads.push(base("2", 0.0, Layer::BottomLayer));
        assert!(fp.overlapping_pad_pairs().is_empty(), "top vs bottom");

        // A through-hole (MultiLayer) pad does short against a top-layer pad.
        let mut fp = Footprint::new("THT");
        fp.pads.push(base("1", 0.0, Layer::TopLayer));
        fp.pads.push(base("2", 0.5, Layer::MultiLayer));
        assert_eq!(fp.overlapping_pad_pairs().len(), 1, "multi-layer overlap");

        // Merely touching still merges in Altium, so it is reported.
        let mut fp = Footprint::new("TOUCHING");
        fp.pads.push(base("1", 0.0, Layer::TopLayer));
        fp.pads.push(base("2", 1.0, Layer::TopLayer));
        assert_eq!(fp.overlapping_pad_pairs().len(), 1, "zero-gap contact");

        // A clear gap is not reported.
        let mut fp = Footprint::new("CLEAR");
        fp.pads.push(base("1", 0.0, Layer::TopLayer));
        fp.pads.push(base("2", 1.2, Layer::TopLayer));
        assert!(fp.overlapping_pad_pairs().is_empty(), "0.2mm gap");
    }

    #[test]
    fn overlapping_pad_pairs_uses_rotated_bounding_box() {
        // A 90-degree rotated pad swaps its effective span, so a pair that clears
        // unrotated can collide once rotated. The AABB comparison must see that.
        let mk = |des: &str, y: f64, rot: f64| {
            let mut p = Pad::smd(des, 0.0, y, 2.0, 0.2);
            p.rotation = rot;
            p
        };
        let mut fp = Footprint::new("FLAT");
        fp.pads.push(mk("1", 0.0, 0.0));
        fp.pads.push(mk("2", 0.5, 0.0));
        assert!(
            fp.overlapping_pad_pairs().is_empty(),
            "flat pads clear in y"
        );

        let mut fp = Footprint::new("ROTATED");
        fp.pads.push(mk("1", 0.0, 90.0));
        fp.pads.push(mk("2", 0.5, 90.0));
        assert_eq!(
            fp.overlapping_pad_pairs().len(),
            1,
            "rotated pads span 2.0mm in y and must collide"
        );
    }
    /// A full-stack per-layer rounded rectangle survives the roundtrip: the
    /// block stores shape id 1 with the layer's corner-radius byte carrying
    /// the rounding, and the reader reconstructs the shape from the pair.
    #[test]
    fn per_layer_rounded_rectangle_shape_survives_a_full_stack_roundtrip() {
        let mut lib = PcbLib::new();
        let mut footprint = Footprint::new("RRECT_STACK");
        let mut pad = Pad::through_hole("1", 0.0, 0.0, 1.6, 1.6, 0.8);
        pad.stack_mode = PadStackMode::FullStack;
        pad.per_layer_sizes = Some(vec![(1.6, 1.6); 32]);
        let mut shapes = vec![PadShape::Round; 32];
        shapes[5] = PadShape::RoundedRectangle;
        pad.per_layer_shapes = Some(shapes);
        let mut radii = vec![0u8; 32];
        radii[5] = 40;
        pad.per_layer_corner_radii = Some(radii);
        footprint.add_pad(pad);
        lib.add(footprint);

        let mut buffer = std::io::Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("write");
        buffer.set_position(0);
        let read_lib = PcbLib::read(buffer).expect("read");
        let pad = &read_lib.get("RRECT_STACK").expect("footprint").pads[0];
        let shapes = pad.per_layer_shapes.as_deref().expect("shapes survive");
        assert_eq!(shapes[5], PadShape::RoundedRectangle);
        assert_eq!(shapes[4], PadShape::Round);
        let radii = pad
            .per_layer_corner_radii
            .as_deref()
            .expect("radii survive");
        assert_eq!(radii[5], 40);
    }

    /// A unique id on a region, fill or component body travels through the
    /// `UniqueIdPrimitiveInformation` streams and is re-attached on read.
    #[test]
    fn unique_ids_on_region_fill_and_body_survive_the_roundtrip() {
        let mut lib = PcbLib::new();
        let mut footprint = Footprint::new("UIDS");
        let mut region = Region::rectangle(0.0, 0.0, 2.0, 1.0, Layer::TopLayer);
        region.unique_id = Some("REGIONID".to_string());
        footprint.add_region(region);
        let mut fill = Fill::new(0.0, 0.0, 1.0, 1.0, Layer::TopLayer);
        fill.unique_id = Some("FILLIDAA".to_string());
        footprint.add_fill(fill);
        let mut body = ComponentBody::new("", "");
        body.unique_id = Some("BODYIDAA".to_string());
        footprint.add_component_body(body);
        lib.add(footprint);

        let mut buffer = std::io::Cursor::new(Vec::new());
        lib.write(&mut buffer).expect("write");
        buffer.set_position(0);
        let read_lib = PcbLib::read(buffer).expect("read");
        let footprint = read_lib.get("UIDS").expect("footprint");
        assert_eq!(footprint.regions[0].unique_id.as_deref(), Some("REGIONID"));
        assert_eq!(footprint.fills[0].unique_id.as_deref(), Some("FILLIDAA"));
        assert_eq!(
            footprint.component_bodies[0].unique_id.as_deref(),
            Some("BODYIDAA")
        );
    }

    /// Coincident pads on a non-copper layer never short: copper merging is a
    /// copper-layer phenomenon, so mechanical-layer overlap is not reported.
    #[test]
    fn overlapping_pads_on_a_non_copper_layer_do_not_short() {
        let mut footprint = Footprint::new("MECH_OVERLAP");
        for designator in ["1", "2"] {
            let mut pad = Pad::smd(designator, 0.0, 0.0, 2.0, 2.0);
            pad.layer = Layer::Mechanical13;
            footprint.add_pad(pad);
        }
        assert!(
            footprint.overlapping_pad_pairs().is_empty(),
            "non-copper pads cannot short"
        );
    }

    /// `reset_identities` on a via whose raw block is shorter than the
    /// identity region leaves the block alone instead of slicing past it.
    #[test]
    fn reset_identities_tolerates_a_via_raw_block_shorter_than_291_bytes() {
        let mut footprint = Footprint::new("SHORT_VIA");
        let mut via = Via::new(0.0, 0.0, 0.6, 0.3);
        via.raw_block = Some(vec![0xAA; 100]);
        footprint.add_via(via);
        footprint.reset_identities();
        assert_eq!(
            footprint.vias[0].raw_block.as_deref(),
            Some(vec![0xAA; 100].as_slice()),
            "a short raw block stays untouched"
        );
    }
}
